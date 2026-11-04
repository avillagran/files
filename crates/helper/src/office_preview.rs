//! DOCX main-body text only. No resource resolution, network, macros or rendering.
use files_core::{
    archive_list,
    media_source::{self, Revision},
};
use quick_xml::{events::Event, name::ResolveResult, reader::NsReader};
use std::{io, path::Path};
pub const MAX_TEXT: usize = 256 * 1024;
const MAX_XML: usize = 1024 * 1024;
const MAX_EVENTS: usize = 65_536;
const WORD: &[u8] = b"http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const STRICT_WORD: &[u8] = b"http://purl.oclc.org/ooxml/wordprocessingml/main";
pub struct Preview {
    pub text: String,
    pub truncated: bool,
    pub source: Revision,
}
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn xml_error(error: impl std::fmt::Display) -> io::Error {
    invalid(&format!("XML DOCX no válido: {error}"))
}
pub fn load(path: &Path, expected: Revision) -> io::Result<Preview> {
    if !path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("docx"))
    {
        return Err(invalid("Sólo texto DOCX está disponible en Office"));
    }
    let (mut file, source) = media_source::open(path)?;
    if source != expected {
        return Err(invalid("El original DOCX cambió antes de abrirlo"));
    }
    if source.bytes == 0 || source.bytes > 8 * 1024 * 1024 {
        return Err(invalid("DOCX debe ser archivo regular de hasta8MiB"));
    }
    let listing = archive_list::read(&mut file)?;
    let xml = crate::zip_member::read(&mut file, &listing, b"word/document.xml", MAX_XML)?;
    drop(listing);
    let (text, truncated) = extract(&xml)?;
    source.revalidate(&file, path)?;
    Ok(Preview {
        text,
        truncated,
        source,
    })
}
fn valid_xml(c: char) -> bool {
    matches!(c,'\t'|'\n'|'\r'|'\u{20}'..='\u{d7ff}'|'\u{e000}'..='\u{fffd}'|'\u{10000}'..='\u{10ffff}')
}
#[derive(Default)]
struct Output {
    text: String,
    lines: usize,
    truncated: bool,
}
impl Output {
    fn push(&mut self, value: &str) -> io::Result<()> {
        // Continue validating the whole XML even after presentation truncation.
        for c in value.chars() {
            if !valid_xml(c) {
                return Err(invalid("Carácter XML DOCX no permitido"));
            }
            if self.truncated {
                continue;
            }
            if self
                .text
                .len()
                .checked_add(c.len_utf8())
                .is_none_or(|n| n > MAX_TEXT)
                || (c == '\n' && self.lines >= 2000)
            {
                self.truncated = true;
                continue;
            }
            self.text.push(c);
            self.lines += usize::from(c == '\n');
        }
        Ok(())
    }
}
fn extract(bytes: &[u8]) -> io::Result<(String, bool)> {
    if bytes.len() > MAX_XML {
        return Err(invalid("XML DOCX supera1MiB"));
    }
    let xml = std::str::from_utf8(bytes)
        .map_err(|_| invalid("XML DOCX requiere UTF-8; UTF-16 todavía pendiente"))?;
    if xml.chars().any(|c| !valid_xml(c)) {
        return Err(invalid("Carácter XML DOCX no permitido"));
    }
    let mut reader = NsReader::from_str(xml);
    reader.config_mut().check_comments = true;
    let mut out = Output::default();
    let (mut depth, mut events) = (0usize, 0usize);
    let (mut root, mut body, mut body_seen) = (false, None, false);
    let (mut text_depth, mut deleted) = (None, None);
    let mut declaration_seen = false;
    loop {
        events += 1;
        if events > MAX_EVENTS {
            return Err(invalid("XML DOCX supera65536eventos"));
        }
        let (namespace, event) = reader.read_resolved_event().map_err(xml_error)?;
        let word = match namespace {
            ResolveResult::Bound(ns) => matches!(ns.as_ref(), WORD | STRICT_WORD),
            ResolveResult::Unknown(_) => return Err(invalid("Prefijo XML DOCX no declarado")),
            ResolveResult::Unbound => false,
        };
        let empty = matches!(&event, Event::Empty(_));
        match event {
            Event::Start(start) | Event::Empty(start) => {
                if depth >= 64 || start.name().as_ref().len() > 256 {
                    return Err(invalid("XML DOCX excede profundidad/nombre admitidos"));
                }
                let mut attrs = 0;
                for attr in start.attributes() {
                    let attr = attr.map_err(xml_error)?;
                    attrs += 1;
                    if attrs > 64 || attr.key.as_ref().len() > 256 || attr.value.len() > 4096 {
                        return Err(invalid("Atributos XML DOCX fuera de límite"));
                    }
                    quick_xml::escape::unescape(
                        std::str::from_utf8(&attr.value).map_err(xml_error)?,
                    )
                    .map_err(xml_error)?;
                }
                let local = start.local_name();
                let name = local.as_ref();
                if depth == 0 {
                    if root || !word || name != b"document" {
                        return Err(invalid("Raíz XML no es un documento Word compatible"));
                    }
                    root = true;
                }
                if text_depth.is_some() {
                    return Err(invalid("Elemento anidado dentro del texto DOCX"));
                }
                let level = depth + 1;
                if word && name == b"body" {
                    if depth != 1 || body_seen {
                        return Err(invalid("Body DOCX duplicado/fuera de raíz"));
                    }
                    body_seen = true;
                    if !empty {
                        body = Some(level);
                    }
                }
                if word && body.is_some() && deleted.is_none() {
                    if name == b"del" && !empty {
                        deleted = Some(level);
                    } else if name == b"t" && !empty {
                        text_depth = Some(level);
                    } else if matches!(name, b"tab" | b"ptab") {
                        out.push("\t")?;
                    } else if matches!(name, b"br" | b"cr") {
                        out.push("\n")?;
                    }
                }
                if !empty {
                    depth = level;
                }
            }
            Event::End(end) => {
                if depth == 0 {
                    return Err(invalid("Cierre XML DOCX inesperado"));
                }
                if text_depth == Some(depth) {
                    text_depth = None;
                }
                if deleted == Some(depth) {
                    deleted = None;
                }
                if word && body.is_some() && deleted.is_none() {
                    match end.local_name().as_ref() {
                        b"p" | b"tr" => out.push("\n")?,
                        b"tc" => out.push("\t")?,
                        _ => {}
                    }
                }
                if body == Some(depth) {
                    body = None;
                }
                depth -= 1;
            }
            Event::Text(t) => {
                let value = t.xml10_content().map_err(xml_error)?;
                if text_depth.is_some() && deleted.is_none() {
                    out.push(&value)?;
                } else if depth == 0 && !value.trim().is_empty() {
                    return Err(invalid("Contenido fuera de raíz DOCX"));
                }
            }
            Event::CData(t) => {
                if depth == 0 {
                    return Err(invalid("CDATA fuera de raíz DOCX"));
                }
                if text_depth.is_some() && deleted.is_none() {
                    out.push(&t.xml10_content().map_err(xml_error)?)?;
                }
            }
            Event::GeneralRef(r) => {
                if depth == 0 {
                    return Err(invalid("Contenido fuera de raíz DOCX"));
                }
                let decoded = r.decode().map_err(xml_error)?;
                let c = match decoded.as_ref() {
                    "amp" => '&',
                    "lt" => '<',
                    "gt" => '>',
                    "quot" => '"',
                    "apos" => '\'',
                    _ => r
                        .resolve_char_ref()
                        .map_err(xml_error)?
                        .ok_or_else(|| invalid("Entidad DOCX desconocida"))?,
                };
                if !valid_xml(c) {
                    return Err(invalid("Referencia XML DOCX no permitida"));
                }
                if text_depth.is_some() && deleted.is_none() {
                    out.push(c.encode_utf8(&mut [0; 4]))?;
                }
            }
            Event::DocType(_) | Event::PI(_) => {
                return Err(invalid("DTD/instrucciones XML DOCX no admitidos"));
            }
            Event::Decl(d) => {
                if declaration_seen
                    || events != 1
                    || root
                    || d.version().map_err(xml_error)?.as_ref() != b"1.0"
                    || d.encoding()
                        .transpose()
                        .map_err(xml_error)?
                        .is_some_and(|e| !e.eq_ignore_ascii_case(b"utf-8"))
                {
                    return Err(invalid("Declaración XML DOCX no admitida"));
                }
                declaration_seen = true;
            }
            Event::Comment(_) => {}
            Event::Eof => {
                if !root || !body_seen || depth != 0 {
                    return Err(invalid("XML DOCX incompleto/sin cuerpo"));
                }
                break;
            }
        }
    }
    Ok((out.text, out.truncated))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn doc(body: &str) -> Vec<u8> {
        format!(
            "<w:document xmlns:w=\"{}\"><w:body>{body}</w:body></w:document>",
            std::str::from_utf8(WORD).unwrap()
        )
        .into_bytes()
    }
    #[test]
    fn unicode_entities_runs_tabs_breaks_tables_and_deleted_text() {
        let xml = doc(
            "<w:p><w:r><w:t>niño &amp; 文档 &#x1f642;</w:t><w:tab/><w:t xml:space=\"preserve\"> fin </w:t><w:br/><w:t><![CDATA[<literal>]]></w:t></w:r><w:del><w:r><w:delText>secreto borrado</w:delText></w:r></w:del><w:r><w:instrText>ignore</w:instrText></w:r></w:p>",
        );
        let (text, truncated) = extract(&xml).unwrap();
        assert!(!truncated);
        assert_eq!(text, "niño & 文档 🙂\t fin \n<literal>\n");
    }
    #[test]
    fn prefix_alias_strict_namespace_and_empty_document_work() {
        let xml = String::from_utf8(doc("<w:p/>"))
            .unwrap()
            .replace("w:", "word:")
            .replace("xmlns:w=", "xmlns:word=")
            .replace(
                std::str::from_utf8(WORD).unwrap(),
                std::str::from_utf8(STRICT_WORD).unwrap(),
            );
        assert_eq!(extract(xml.as_bytes()).unwrap(), ("".into(), false));
        assert!(extract(&doc("<w:p><w:r><w:t/></w:r></w:p>")).is_ok());
    }
    #[test]
    fn dtd_network_entities_unknown_prefix_malformed_utf8_and_depth_fail() {
        for value in [
            b"<!DOCTYPE x SYSTEM 'https://invalid.test/x'>".as_slice(),
            b"<?external data?>",
            b"<w:document xmlns:w='bad'><w:body/></w:document>",
            b"<x:document/>",
            b"\xff",
            b"<w:document/>",
        ] {
            assert!(extract(value).is_err());
        }
        assert!(extract(&doc("<w:p><w:t>&unknown;</w:t></w:p>")).is_err());
        assert!(extract(&doc("<w:p><w:t>&#0;</w:t></w:p>")).is_err());
        assert!(extract(&doc("<w:p></w:bad>")).is_err());
        assert!(
            extract(&doc(&format!(
                "{}{}",
                "<w:p>".repeat(65),
                "</w:p>".repeat(65)
            )))
            .is_err()
        );
        assert!(extract(&doc("<w:body/>")).is_err());
    }
    #[test]
    fn cdata_outside_root_and_misplaced_declarations_fail() {
        let root = String::from_utf8(doc("")).unwrap();
        for xml in [
            format!("<![CDATA[basura]]>{root}"),
            format!("{root}<![CDATA[basura]]>"),
            format!("<![CDATA[ ]]>{root}"),
            format!("<?xml version=\"1.0\"?><?xml version=\"1.0\"?>{root}"),
            format!("<!-- comment --><?xml version=\"1.0\"?>{root}"),
            format!(" <?xml version=\"1.0\"?>{root}"),
            format!("{root}<?xml version=\"1.0\"?>"),
        ] {
            assert!(extract(xml.as_bytes()).is_err(), "accepted: {xml}");
        }
        assert!(extract(format!("<?xml version=\"1.0\"?>{root}").as_bytes()).is_ok());
        assert!(extract(format!("<!-- comment -->{root}<!-- end -->").as_bytes()).is_ok());
    }
    #[test]
    fn text_is_truncated_at_utf8_boundary_but_malformed_tail_still_rejected() {
        let (text, truncated) = extract(&doc(&format!(
            "<w:p><w:t>{}</w:t></w:p>",
            "é".repeat(MAX_TEXT / 2 + 1)
        )))
        .unwrap();
        assert_eq!(text.len(), MAX_TEXT);
        assert!(truncated);
        let (text, truncated) = extract(&doc(&"<w:p><w:t>x</w:t></w:p>".repeat(2001))).unwrap();
        assert_eq!(text.bytes().filter(|b| *b == b'\n').count(), 2000);
        assert!(truncated);
        assert!(
            extract(&doc(&format!(
                "<w:p><w:t>{}&bad;</w:t></w:p>",
                "x".repeat(MAX_TEXT + 1)
            )))
            .is_err()
        );
        assert!(extract(&vec![b'x'; MAX_XML + 1]).is_err());
    }
    #[test]
    fn checked_source_rejects_verified_a_replaced_before_open() {
        let root = std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .unwrap()
            .join(".cache/files-test");
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join(format!(
            "docx-source-{}-{}.docx",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let bytes =
            crate::zip_member::tests::fixture(&doc("<w:p><w:r><w:t>A</w:t></w:r></w:p>"), 8, false);
        std::fs::write(&path, &bytes).unwrap();
        let (_, a) = media_source::open(&path).unwrap();
        let ready = load(&path, a).unwrap();
        assert_eq!(ready.text, "A\n");
        assert_eq!(ready.source, a);
        let replacement = path.with_extension("next");
        std::fs::write(&replacement, &bytes).unwrap();
        std::fs::rename(&replacement, &path).unwrap();
        assert!(load(&path, a).is_err());
        std::fs::remove_file(path).unwrap();
    }
}
