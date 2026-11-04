//! Bounded native file MIME codecs. No I/O, clipboard ownership or deletion.
//! Parse complete MIME bytes before decoding each URI exactly once. Unix raw
//! paths stay raw. These codecs do not turn plain text into a file permission.
use std::path::{Path, PathBuf};
pub const MAX_WIRE: usize = 1024 * 1024;
pub const MAX_PATH: usize = 4096;
pub const MAX_ROOTS: usize = 128;
const MAX_PATH_BYTES: usize = MAX_ROOTS * MAX_PATH;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    UriList,
    GnomeCopiedFiles,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Intent {
    Copy,
    Cut,
}
#[derive(Debug, PartialEq, Eq)]
pub struct Files {
    pub intent: Intent,
    pub paths: Vec<PathBuf>,
}
fn invalid() -> &'static str {
    "Lista MIME de archivos inválida o fuera de límites"
}
fn lines(data: &[u8]) -> impl Iterator<Item = &[u8]> {
    data.split(|b| *b == b'\n')
        .map(|l| l.strip_suffix(b"\r").unwrap_or(l))
}
fn body(format: Format, wire: &[u8]) -> Result<(Intent, &[u8]), &'static str> {
    if wire.len() > MAX_WIRE {
        return Err(invalid());
    }
    match format {
        Format::UriList => Ok((Intent::Copy, wire)),
        Format::GnomeCopiedFiles => {
            let end = wire.iter().position(|b| *b == b'\n').ok_or_else(invalid)?;
            let header = wire[..end].strip_suffix(b"\r").unwrap_or(&wire[..end]);
            let intent = match header {
                b"copy" => Intent::Copy,
                b"cut" => Intent::Cut,
                _ => return Err(invalid()),
            };
            Ok((intent, &wire[end + 1..]))
        }
    }
}
fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}
/// Visit without assigning even one decoded path; also used by the second pass.
fn uri(line: &[u8], mut emit: impl FnMut(u8)) -> Result<usize, &'static str> {
    if line.len() < 8 || !line[..7].eq_ignore_ascii_case(b"file://") {
        return Err(invalid());
    }
    let rest = &line[7..];
    let slash = rest.iter().position(|b| *b == b'/').ok_or_else(invalid)?;
    let authority = &rest[..slash];
    if !authority.is_empty() && !authority.eq_ignore_ascii_case(b"localhost") {
        return Err("Sólo se admiten URI file locales");
    }
    let path = &rest[slash..];
    let mut i = 0;
    let mut count = 0;
    let mut segment = 0;
    let mut dots = true;
    while i < path.len() {
        let byte = if path[i] == b'%' {
            let a = *path.get(i + 1).ok_or_else(invalid)?;
            let b = *path.get(i + 2).ok_or_else(invalid)?;
            i += 3;
            (hex(a).ok_or_else(invalid)? << 4) | hex(b).ok_or_else(invalid)?
        } else {
            let b = path[i];
            i += 1;
            if b <= b' ' || b >= 127 || matches!(b, b'?' | b'#' | b'\\') {
                return Err(invalid());
            }
            b
        };
        if byte == 0 {
            return Err(invalid());
        }
        if byte == b'/' {
            if count > 0 && (segment == 0 || dots && segment <= 2) {
                return Err(invalid());
            }
            segment = 0;
            dots = true;
        } else {
            segment += 1;
            dots &= byte == b'.';
        }
        count += 1;
        if count > MAX_PATH {
            return Err(invalid());
        }
        emit(byte);
    }
    if count <= 1 || segment == 0 || dots && segment <= 2 {
        return Err(invalid());
    }
    Ok(count)
}
#[cfg(unix)]
pub fn decode(format: Format, wire: &[u8]) -> Result<Files, &'static str> {
    use std::os::unix::ffi::OsStringExt;
    let (intent, wire) = body(format, wire)?;
    let included = |l: &&[u8]| !l.is_empty() && !(format == Format::UriList && l.starts_with(b"#"));
    let mut count = 0usize;
    let mut bytes = 0usize;
    for line in lines(wire).filter(included) {
        count = count
            .checked_add(1)
            .filter(|n| *n <= MAX_ROOTS)
            .ok_or_else(invalid)?;
        bytes = bytes
            .checked_add(uri(line, |_| {})?)
            .filter(|n| *n <= MAX_PATH_BYTES)
            .ok_or_else(invalid)?;
    }
    if count == 0 {
        return Err(invalid());
    }
    // Every count/path/aggregate has been checked before any owned paths exist.
    let mut paths = Vec::with_capacity(count);
    for line in lines(wire).filter(included) {
        let len = uri(line, |_| {})?;
        let mut decoded = Vec::with_capacity(len);
        uri(line, |b| decoded.push(b))?;
        let path = PathBuf::from(std::ffi::OsString::from_vec(decoded));
        if paths.contains(&path) {
            return Err("URI de archivo duplicada");
        }
        paths.push(path);
    }
    Ok(Files { intent, paths })
}
#[cfg(not(unix))]
pub fn decode(_format: Format, _wire: &[u8]) -> Result<Files, &'static str> {
    Err("Adaptador URI de esta plataforma pendiente")
}
fn safe(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'/' | b'-' | b'_' | b'.' | b'~')
}
#[cfg(unix)]
fn path_bytes(path: &Path) -> Result<&[u8], &'static str> {
    use std::os::unix::ffi::OsStrExt;
    let b = path.as_os_str().as_bytes();
    if b.len() > MAX_PATH
        || b.len() < 2
        || b[0] != b'/'
        || b.contains(&0)
        || b[1..]
            .split(|b| *b == b'/')
            .any(|s| s.is_empty() || s == b"." || s == b"..")
    {
        return Err(invalid());
    }
    Ok(b)
}
#[cfg(unix)]
pub fn encode(format: Format, intent: Intent, paths: &[PathBuf]) -> Result<Vec<u8>, &'static str> {
    encode_bounded(format, intent, paths, MAX_WIRE)
}
#[cfg(unix)]
pub fn encode_bounded(
    format: Format,
    intent: Intent,
    paths: &[PathBuf],
    limit: usize,
) -> Result<Vec<u8>, &'static str> {
    let limit = limit.min(MAX_WIRE);
    if paths.is_empty() || paths.len() > MAX_ROOTS {
        return Err(invalid());
    }
    if format == Format::UriList && intent == Intent::Cut {
        return Err("uri-list no expresa Cut; requiere MIME explícito");
    }
    let header: &[u8] = match (format, intent) {
        (Format::UriList, _) => b"",
        (_, Intent::Copy) => b"copy\n",
        (_, Intent::Cut) => b"cut\n",
    };
    let mut size = header.len();
    let mut total = 0usize;
    for (i, p) in paths.iter().enumerate() {
        let b = path_bytes(p)?;
        if paths[..i].contains(p) {
            return Err("Ruta duplicada");
        }
        total = total
            .checked_add(b.len())
            .filter(|n| *n <= MAX_PATH_BYTES)
            .ok_or_else(invalid)?;
        size = size.checked_add(9).ok_or_else(invalid)?; // file:// + CRLF
        for &b in b {
            size = size
                .checked_add(if safe(b) { 1 } else { 3 })
                .filter(|n| *n <= limit)
                .ok_or_else(invalid)?;
        }
    }
    let mut result = Vec::with_capacity(size);
    result.extend_from_slice(header);
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for p in paths {
        result.extend_from_slice(b"file://");
        for &b in path_bytes(p)? {
            if safe(b) {
                result.push(b);
            } else {
                result.extend_from_slice(&[b'%', HEX[(b >> 4) as usize], HEX[(b & 15) as usize]]);
            }
        }
        result.extend_from_slice(b"\r\n");
    }
    debug_assert_eq!(result.len(), size);
    Ok(result)
}
#[cfg(not(unix))]
pub fn encode(
    _format: Format,
    _intent: Intent,
    _paths: &[PathBuf],
) -> Result<Vec<u8>, &'static str> {
    Err("Adaptador URI de esta plataforma pendiente")
}
#[cfg(not(unix))]
pub fn encode_bounded(
    _format: Format,
    _intent: Intent,
    _paths: &[PathBuf],
    _limit: usize,
) -> Result<Vec<u8>, &'static str> {
    Err("Adaptador URI de esta plataforma pendiente")
}
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    #[test]
    fn bounded_uri_preflights_encoded_expansion_before_allocation() {
        let p = PathBuf::from("/a é%");
        let paths = [p];
        let wire = encode(Format::UriList, Intent::Copy, &paths).unwrap();
        assert!(encode_bounded(Format::UriList, Intent::Copy, &paths, wire.len() - 1).is_err());
        assert_eq!(
            encode_bounded(Format::UriList, Intent::Copy, &paths, wire.len()).unwrap(),
            wire
        );
    }
    #[test]
    fn raw_percent_newline_crlf_and_localhost_decode_exactly_once() {
        let data =
            b"# comment\r\nfile:///a/%FF%0Ab%25%30%41\r\nFILE://LOCALHOST/a/unicode-%C3%A1\n";
        let f = decode(Format::UriList, data).unwrap();
        assert_eq!(f.intent, Intent::Copy);
        assert_eq!(
            f.paths[0],
            PathBuf::from(OsString::from_vec(b"/a/\xff\nb%0A".to_vec()))
        );
        assert_eq!(f.paths[1], PathBuf::from("/a/unicode-á"));
        assert_eq!(
            decode(
                Format::UriList,
                &encode(Format::UriList, Intent::Copy, &f.paths).unwrap()
            )
            .unwrap(),
            f
        );
    }
    #[test]
    fn cut_requires_explicit_format_never_plain_uri_or_other_header() {
        let p = vec![PathBuf::from("/a/file")];
        let w = encode(Format::GnomeCopiedFiles, Intent::Cut, &p).unwrap();
        assert_eq!(
            decode(Format::GnomeCopiedFiles, &w).unwrap(),
            Files {
                intent: Intent::Cut,
                paths: p.clone()
            }
        );
        assert!(encode(Format::UriList, Intent::Cut, &p).is_err());
        assert_eq!(
            decode(Format::UriList, b"file:///a/file").unwrap().intent,
            Intent::Copy
        );
        for d in [
            b"CUT\nfile:///a/file".as_slice(),
            b"move\nfile:///a/file",
            b"cut",
            b"cut\n#comment\nfile:///a/file",
        ] {
            assert!(decode(Format::GnomeCopiedFiles, d).is_err());
        }
    }
    #[test]
    fn rejects_remote_nul_traversal_malformed_authority_query_and_ambiguous_lines() {
        for uri in [
            "file://server/a",
            "sftp://host/a",
            "file://user@localhost/a",
            "file://localhost:1/a",
            "file:///a/%00",
            "file:///a/%",
            "file:///a/%G0",
            "file:///a/../b",
            "file:///a/%2e%2e/b",
            "file:///a/./b",
            "file:////a",
            "file:///a/",
            "file:///",
            "file:///a?b",
            "file:///a#b",
            "file:///a b",
            "file:///a\\b",
            "/a",
            "file:///a\rb",
        ] {
            assert!(decode(Format::UriList, uri.as_bytes()).is_err(), "{uri}");
        }
        assert!(decode(Format::UriList, b"file:///a\nfile://localhost/a").is_err());
    }
    #[test]
    fn literal_encoded_controls_and_percent_are_not_extra_roots() {
        let p = vec![PathBuf::from(OsString::from_vec(
            b"/a/new\nline\r%00?#\\\xff".to_vec(),
        ))];
        let w = encode(Format::UriList, Intent::Copy, &p).unwrap();
        assert!(w.windows(3).any(|w| w == b"%0A"));
        assert_eq!(w.iter().filter(|b| **b == b'\n').count(), 1);
        assert_eq!(decode(Format::UriList, &w).unwrap().paths, p);
    }
    #[test]
    fn cap_count_path_wire_and_output_preflight() {
        let paths = (0..MAX_ROOTS)
            .map(|i| PathBuf::from(format!("/a/{i}")))
            .collect::<Vec<_>>();
        let w = encode(Format::UriList, Intent::Copy, &paths).unwrap();
        assert_eq!(decode(Format::UriList, &w).unwrap().paths.len(), MAX_ROOTS);
        let mut many = paths;
        many.push(PathBuf::from("/extra"));
        assert!(encode(Format::UriList, Intent::Copy, &many).is_err());
        assert!(decode(Format::UriList, &[w, b"file:///extra\n".to_vec()].concat()).is_err());
        let exact = PathBuf::from(format!("/{}", "a".repeat(MAX_PATH - 1)));
        assert!(encode(Format::UriList, Intent::Copy, &[exact]).is_ok());
        assert!(
            encode(
                Format::UriList,
                Intent::Copy,
                &[PathBuf::from(format!("/{}", "a".repeat(MAX_PATH)))]
            )
            .is_err()
        );
        assert!(decode(Format::UriList, &vec![b'#'; MAX_WIRE + 1]).is_err());
        let expansion = (0..MAX_ROOTS)
            .map(|i| PathBuf::from(format!("/{}-{i}", " ".repeat(MAX_PATH - 8))))
            .collect::<Vec<_>>();
        assert!(encode(Format::UriList, Intent::Copy, &expansion).is_err());
    }
}
