//! Bounded text source preview; not a Markdown or HTML renderer.
use std::io::{self, Read};
use std::path::Path;

pub const MAX_BYTES: usize = 256 * 1024;
pub const MAX_LINES: usize = 2000;

pub struct Preview {
    pub text: String,
    pub encoding: &'static str,
    pub truncated: bool,
    pub relative_source: Option<files_core::relative_source::Proof>,
}

pub fn load(path: &Path) -> io::Result<Preview> {
    use files_core::{
        media_source,
        relative_source::{DirectoryIdentity, Proof},
    };
    let (mut file, revision) = media_source::open(path)?;
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("Documento sin raíz"))?;
    // Direct text retains its historical parent-symlink semantics. Relative
    // resources separately require a NOFOLLOW root and all normal components.
    let root = std::fs::File::open(parent)?;
    let identity = DirectoryIdentity::from_file(&root)?;
    let mut preview = read(&mut file)?;
    revision.revalidate(&file, path)?;
    if DirectoryIdentity::from_metadata(&std::fs::metadata(parent)?)? != identity {
        return Err(io::Error::other("La raíz del documento cambió"));
    }
    preview.relative_source = Some(Proof {
        root: identity,
        file: revision,
    });
    Ok(preview)
}
fn read(input: impl Read) -> io::Result<Preview> {
    let mut data = Vec::new();
    input.take((MAX_BYTES + 1) as u64).read_to_end(&mut data)?;
    decode(data)
}
pub fn load_relative(root: &Path, relative: &Path) -> io::Result<Preview> {
    let mut opened = files_core::relative_source::Opened::open(root, relative)?;
    let mut preview = decode(files_core::relative_source::read_prefix(
        &mut opened.file,
        MAX_BYTES + 1,
    )?)?;
    opened.revalidate()?;
    preview.relative_source = Some(opened.proof);
    Ok(preview)
}

fn decode(mut data: Vec<u8>) -> io::Result<Preview> {
    let truncated = data.len() > MAX_BYTES;
    data.truncate(MAX_BYTES);
    let (mut text, encoding) = if data.starts_with(&[0xff, 0xfe]) || data.starts_with(&[0xfe, 0xff])
    {
        let little = data.starts_with(&[0xff, 0xfe]);
        let payload = &data[2..];
        if !payload.len().is_multiple_of(2) && !truncated {
            return Err(invalid_encoding());
        }
        let mut units: Vec<u16> = payload
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| {
                if little {
                    u16::from_le_bytes([pair[0], pair[1]])
                } else {
                    u16::from_be_bytes([pair[0], pair[1]])
                }
            })
            .collect();
        if truncated && units.last().is_some_and(|u| (0xd800..=0xdbff).contains(u)) {
            units.pop();
        }
        let text = String::from_utf16(&units).map_err(|_| invalid_encoding())?;
        (text, if little { "UTF-16LE" } else { "UTF-16BE" })
    } else {
        let bytes = data.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&data);
        let bytes = match std::str::from_utf8(bytes) {
            Ok(_) => bytes,
            Err(error) if truncated && error.error_len().is_none() => &bytes[..error.valid_up_to()],
            Err(_) => return Err(invalid_encoding()),
        };
        (
            std::str::from_utf8(bytes)
                .map_err(|_| invalid_encoding())?
                .to_owned(),
            "UTF-8",
        )
    };
    if text
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Archivo binario o con controles no representables",
        ));
    }
    let mut lines = 0usize;
    let mut cut = None;
    for (index, byte) in text.bytes().enumerate() {
        if byte == b'\n' {
            lines += 1;
            if lines == MAX_LINES {
                if index + 1 < text.len() {
                    cut = Some(index + 1);
                }
                break;
            }
        }
    }
    if let Some(end) = cut {
        text.truncate(end);
    }
    Ok(Preview {
        text,
        encoding,
        truncated: truncated || cut.is_some(),
        relative_source: None,
    })
}
fn invalid_encoding() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "Codificación no válida: se admite UTF-8 o UTF-16 con BOM",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "linux")]
    #[test]
    fn relative_text_uses_same_decoder_and_truncation() {
        let fixture = RelativeFixture::new();
        let path = fixture.0.join("text");
        for input in [
            b"Hola\r\n".to_vec(),
            vec![0xff, 0xfe, 0xe1, 0, 10, 0],
            vec![b'x'; MAX_BYTES + 20],
            "x\n".repeat(MAX_LINES + 1).into_bytes(),
        ] {
            std::fs::write(&path, input).unwrap();
            let normal = load(&path).unwrap();
            let relative = load_relative(&fixture.0, Path::new("text")).unwrap();
            assert_eq!(
                (relative.text, relative.encoding, relative.truncated),
                (normal.text, normal.encoding, normal.truncated)
            );
        }
        std::fs::write(&path, b"binary\0").unwrap();
        assert!(load_relative(&fixture.0, Path::new("text")).is_err());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn relative_text_rejects_escape_links_and_invalid_roots() {
        use std::os::unix::fs::symlink;
        let fixture = RelativeFixture::new();
        std::fs::create_dir(fixture.0.join("base")).unwrap();
        std::fs::write(fixture.0.join("outside"), b"outside").unwrap();
        let root = fixture.0.join("base");
        symlink("../outside", root.join("link")).unwrap();
        symlink("..", root.join("dir")).unwrap();
        for path in ["../outside", "link", "dir/outside", "/etc/passwd"] {
            assert!(load_relative(&root, Path::new(path)).is_err(), "{path}");
        }
        symlink("base", fixture.0.join("root-link")).unwrap();
        for root in [
            fixture.0.join("outside"),
            fixture.0.join("root-link"),
            fixture.0.join("missing"),
        ] {
            assert!(load_relative(&root, Path::new("text")).is_err());
        }
        // Los bytes %2e no se vuelven a interpretar como '..'.
        std::fs::write(root.join("%2e%2e"), b"literal").unwrap();
        assert_eq!(
            load_relative(&root, Path::new("%2e%2e")).unwrap().text,
            "literal"
        );
    }

    #[cfg(target_os = "linux")]
    struct RelativeFixture(std::path::PathBuf);
    #[cfg(target_os = "linux")]
    impl RelativeFixture {
        fn new() -> Self {
            use std::sync::atomic::{AtomicU64, Ordering};
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "files-relative-text-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    #[cfg(target_os = "linux")]
    impl Drop for RelativeFixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn empty_unicode_bom_and_exact_limit() {
        assert_eq!(read(&b""[..]).unwrap().text, "");
        assert_eq!(
            read("\u{feff}Hola 🐜\r\n".as_bytes()).unwrap().text,
            "Hola 🐜\r\n"
        );
        let p = read(&vec![b'a'; MAX_BYTES][..]).unwrap();
        assert!(!p.truncated);
        assert_eq!(p.text.len(), MAX_BYTES);
    }
    #[test]
    fn bounded_read_never_consumes_more_than_limit_plus_one() {
        struct Endless(usize);
        impl Read for Endless {
            fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
                self.0 += buffer.len();
                assert!(self.0 <= MAX_BYTES + 1);
                buffer.fill(b'a');
                Ok(buffer.len())
            }
        }
        let p = read(Endless(0)).unwrap();
        assert!(p.truncated);
        assert_eq!(p.text.len(), MAX_BYTES);
    }
    #[test]
    fn utf8_boundary_is_trimmed_only_when_truncated() {
        let mut input = vec![b'a'; MAX_BYTES - 1];
        input.extend_from_slice("🐜".as_bytes());
        let p = read(&input[..]).unwrap();
        assert!(p.truncated);
        assert_eq!(p.text.len(), MAX_BYTES - 1);
        assert!(read(&b"a\xf0\x9f"[..]).is_err());
        assert!(read(&b"abc\0def"[..]).is_err());
    }
    #[test]
    fn utf16_both_endiannesses_and_invalid_units() {
        for little in [true, false] {
            let mut bytes = if little {
                vec![0xff, 0xfe]
            } else {
                vec![0xfe, 0xff]
            };
            for u in "Hola 🐜".encode_utf16() {
                bytes.extend_from_slice(&if little {
                    u.to_le_bytes()
                } else {
                    u.to_be_bytes()
                });
            }
            assert_eq!(read(&bytes[..]).unwrap().text, "Hola 🐜");
        }
        assert!(read(&[0xff, 0xfe, 0x00, 0xd8][..]).is_err());
        assert!(read(&[0xff, 0xfe, 0x41][..]).is_err());
    }
    #[test]
    fn line_limit_preserves_complete_lines_and_flags_remaining_content() {
        let exact = "x\n".repeat(MAX_LINES);
        assert!(!read(exact.as_bytes()).unwrap().truncated);
        let p = read(format!("{exact}last").as_bytes()).unwrap();
        assert!(p.truncated);
        assert_eq!(p.text, exact);
    }
}
