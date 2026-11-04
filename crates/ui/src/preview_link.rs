//! Política léxica para destinos de previews renderizados, sin I/O ni red.
//!
//! El parser Markdown/HTML debe entregar atributos ya desescapados (entidades
//! HTML incluidas). Una ruta aceptada NO es autorización para abrirla: el helper
//! debe resolverla dentro del directorio permitido sin seguir enlaces que salgan
//! de él, y comprobar identidad al abrir. Llamar resolve_relative con el documento
//! actual antes del helper: Target puede contener padres pendientes. Nunca volver
//! a decodificar el resultado.
use std::path::{Component, Path, PathBuf};

pub const MAX_TARGET_BYTES: usize = 4096;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    Fragment(String),
    Relative {
        path: PathBuf,
        fragment: Option<String>,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub enum Rejected {
    TooLong,
    Empty,
    InvalidEncoding,
    Control,
    Unsupported,
    OutsideRoot,
    InvalidCurrent,
}

/// Se admiten fragmentos y rutas relativas con padres pendientes. Schemes, autoridades,
/// consultas y rutas absolutas requieren otro flujo explícito de navegación.
pub fn parse(value: &str) -> Result<Target, Rejected> {
    if value.len() > MAX_TARGET_BYTES {
        return Err(Rejected::TooLong);
    }
    if value.is_empty() {
        return Err(Rejected::Empty);
    }
    let (path, fragment) = match value.split_once('#') {
        Some((path, fragment)) => (path, Some(decode(fragment)?)),
        None => (value, None),
    };
    if path.is_empty() {
        return fragment.map(Target::Fragment).ok_or(Rejected::Empty);
    }
    // '?' literal es consulta URL; %3F tampoco se admite como nombre portable.
    let path = decode(path)?;
    if path.starts_with('/') || path.contains(['\\', ':', '?']) {
        return Err(Rejected::Unsupported);
    }
    let mut normalized = PathBuf::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if matches!(
                    normalized.components().next_back(),
                    Some(Component::Normal(_))
                ) {
                    normalized.pop();
                } else {
                    normalized.push("..");
                }
            }
            name => normalized.push(name),
        }
    }
    if normalized.as_os_str().is_empty() {
        return Err(Rejected::Empty);
    }
    Ok(Target::Relative {
        path: normalized,
        fragment,
    })
}

/// `current` is the document's path relative to its authorized root, not an
/// absolute path. `target` was URL-decoded exactly once by the link parser.
/// Produces a normalized path for the descriptor-based helper; does no I/O and
/// does not establish a filesystem sandbox or follow any symlinks.
pub fn resolve_relative(current: &Path, target: &Path) -> Result<PathBuf, Rejected> {
    if current.as_os_str().as_encoded_bytes().len() > MAX_TARGET_BYTES
        || target.as_os_str().as_encoded_bytes().len() > MAX_TARGET_BYTES
    {
        return Err(Rejected::TooLong);
    }
    if current.as_os_str().is_empty()
        || current
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(Rejected::InvalidCurrent);
    }
    // Targets are portable UTF8 URL paths, but existing directory names may
    // contain non-UTF8 Unix bytes. Preserve those bytes, never URL-decode them.
    let target_text = target.to_str().ok_or(Rejected::Unsupported)?;
    if target_text.is_empty() {
        return Err(Rejected::Empty);
    }
    if target_text.contains(['\\', ':', '?']) || target_text.chars().any(char::is_control) {
        return Err(Rejected::Unsupported);
    }
    let mut resolved = current
        .parent()
        .ok_or(Rejected::InvalidCurrent)?
        .to_path_buf();
    for part in target.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                if !resolved.pop() {
                    return Err(Rejected::OutsideRoot);
                }
            }
            Component::Normal(name) => {
                let separator = usize::from(!resolved.as_os_str().is_empty());
                if resolved
                    .as_os_str()
                    .as_encoded_bytes()
                    .len()
                    .checked_add(separator)
                    .and_then(|n| n.checked_add(name.as_encoded_bytes().len()))
                    .is_none_or(|n| n > MAX_TARGET_BYTES)
                {
                    return Err(Rejected::TooLong);
                }
                resolved.push(name);
            }
            _ => return Err(Rejected::Unsupported),
        }
    }
    if resolved.as_os_str().is_empty() {
        return Err(Rejected::Empty);
    }
    Ok(resolved)
}

fn decode(value: &str) -> Result<String, Rejected> {
    let mut bytes = Vec::with_capacity(value.len());
    let mut input = value.bytes();
    while let Some(byte) = input.next() {
        if byte == b'%' {
            let high = input
                .next()
                .and_then(hex)
                .ok_or(Rejected::InvalidEncoding)?;
            let low = input
                .next()
                .and_then(hex)
                .ok_or(Rejected::InvalidEncoding)?;
            bytes.push(high * 16 + low);
        } else {
            bytes.push(byte);
        }
    }
    let result = String::from_utf8(bytes).map_err(|_| Rejected::InvalidEncoding)?;
    if result.chars().any(char::is_control) {
        return Err(Rejected::Control);
    }
    Ok(result)
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unicode_spaces_and_fragment_are_decoded_once() {
        assert_eq!(
            parse("./gu%C3%ADas/a%20b.md#secci%C3%B3n"),
            Ok(Target::Relative {
                path: PathBuf::from("guías/a b.md"),
                fragment: Some("sección".into()),
            })
        );
        assert_eq!(parse("#"), Ok(Target::Fragment(String::new())));
        assert_eq!(parse("#a+b"), Ok(Target::Fragment("a+b".into())));
        assert_eq!(
            parse("%252e%252e.txt"),
            Ok(Target::Relative {
                path: PathBuf::from("%2e%2e.txt"),
                fragment: None,
            })
        );
    }

    #[test]
    fn encoded_separators_require_context_and_cannot_escape_root() {
        for value in ["../a", "%2e%2e/a", "a/%2e%2e/%2e%2e/a", "a%2f..%2f../b"] {
            let Target::Relative { path, .. } = parse(value).unwrap() else {
                panic!("relative target expected")
            };
            assert_eq!(
                resolve_relative(Path::new("overview.md"), &path),
                Err(Rejected::OutsideRoot),
                "{value}"
            );
            assert_eq!(
                resolve_relative(Path::new("docs/guide.md"), &path),
                Ok(PathBuf::from(if value.ends_with('b') { "b" } else { "a" })),
                "{value}"
            );
        }
        assert_eq!(
            parse("a/../b"),
            Ok(Target::Relative {
                path: "b".into(),
                fragment: None
            })
        );
    }

    #[test]
    fn external_and_platform_absolute_paths_are_rejected() {
        for value in [
            "https://a/b",
            "javascript:alert(1)",
            "data:text/html,a",
            "file:///a",
            "//server/a",
            "/a",
            "%2fa",
            "C:/a",
            "C%3a/a",
            "a\\b",
            "a%5cb",
            "a?x=1",
            "a%3fx=1",
        ] {
            assert_eq!(parse(value), Err(Rejected::Unsupported), "{value}");
        }
    }

    #[test]
    fn controls_and_invalid_utf8_or_escapes_are_rejected() {
        for value in ["a%00b", "a\nb", "#%7f", "#%C2%85"] {
            assert_eq!(parse(value), Err(Rejected::Control));
        }
        for value in ["%", "%a", "%zz", "%ff", "#%c3"] {
            assert_eq!(parse(value), Err(Rejected::InvalidEncoding));
        }
    }

    #[test]
    fn encoded_hash_is_part_of_filename_and_input_is_bounded() {
        assert_eq!(
            parse("a%23b.md#part"),
            Ok(Target::Relative {
                path: "a#b.md".into(),
                fragment: Some("part".into()),
            })
        );
        assert_eq!(
            parse(&"a".repeat(MAX_TARGET_BYTES + 1)),
            Err(Rejected::TooLong)
        );
        for value in ["", ".", "a/.."] {
            assert_eq!(parse(value), Err(Rejected::Empty));
        }
    }
}

#[cfg(test)]
mod contextual_tests {
    use super::*;
    #[test]
    fn sibling_parent_and_nested_links_remain_in_root() {
        for (current, target, want) in [
            ("overview.md", "guide.md", "guide.md"),
            ("docs/guide.md", "../overview.md", "overview.md"),
            ("docs/nested/guide.md", "../../overview.md", "overview.md"),
            (
                "docs/guide.md",
                "../assets/./palette.png",
                "assets/palette.png",
            ),
            ("docs/guide.md", "extra/../next.md", "docs/next.md"),
        ] {
            assert_eq!(
                resolve_relative(Path::new(current), Path::new(target)),
                Ok(PathBuf::from(want))
            );
        }
    }
    #[test]
    fn parent_cannot_cross_authorized_root() {
        for (current, target) in [
            ("overview.md", "../secret"),
            ("docs/a.md", "../../secret"),
            ("docs/a.md", "x/../../../secret"),
        ] {
            assert_eq!(
                resolve_relative(Path::new(current), Path::new(target)),
                Err(Rejected::OutsideRoot)
            );
        }
    }
    #[test]
    fn target_escapes_are_never_decoded_again() {
        assert_eq!(
            resolve_relative(Path::new("docs/a.md"), Path::new("%2e%2e/guías a#b.png")),
            Ok(PathBuf::from("docs/%2e%2e/guías a#b.png"))
        );
    }
    #[test]
    fn caller_current_must_already_be_root_relative() {
        for current in ["", "/a", "../a", "docs/../a", "./a"] {
            assert_eq!(
                resolve_relative(Path::new(current), Path::new("b")),
                Err(Rejected::InvalidCurrent)
            );
        }
    }
    #[test]
    fn portable_target_validation_and_length_bound() {
        for target in ["/a", "//server/a", "C:/a", "a\\b", "a?b", "a\0b"] {
            assert_eq!(
                resolve_relative(Path::new("docs/a"), Path::new(target)),
                Err(Rejected::Unsupported)
            );
        }
        let target = "a".repeat(MAX_TARGET_BYTES);
        assert_eq!(
            resolve_relative(Path::new("a"), Path::new(&target)),
            Ok(PathBuf::from(&target))
        );
        assert_eq!(
            resolve_relative(Path::new("docs/a"), Path::new(&target)),
            Err(Rejected::TooLong)
        );
        assert_eq!(
            resolve_relative(Path::new("a"), Path::new(&format!("{target}b"))),
            Err(Rejected::TooLong)
        );
    }
    #[cfg(unix)]
    #[test]
    fn existing_non_utf8_directory_bytes_are_preserved() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;
        let current = Path::new(OsStr::from_bytes(b"docs\xff/a.md"));
        let result = resolve_relative(current, Path::new("b.png")).unwrap();
        assert_eq!(result.as_os_str().as_bytes(), b"docs\xff/b.png");
    }
}
