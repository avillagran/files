//! Bounded ASCII transport of one directory through UTF-8-only user-app brokers.
//! No percent decoding, normalization, shell quoting or filesystem operations.
use std::path::{Path, PathBuf};
pub const MAX_BYTES: usize = 4096;
fn valid(path: &Path) -> Result<(), &'static str> {
    let bytes = path.as_os_str().as_encoded_bytes();
    if !path.is_absolute() || bytes.len() > MAX_BYTES || bytes.contains(&0) {
        Err("Terminal: carpeta absoluta de hasta4096bytes, sin NUL")
    } else {
        Ok(())
    }
}
pub fn encode(path: &Path) -> Result<String, &'static str> {
    valid(path)?;
    let bytes = path.as_os_str().as_encoded_bytes();
    let mut result = String::with_capacity(bytes.len() * 2);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for &b in bytes {
        result.push(HEX[(b >> 4) as usize] as char);
        result.push(HEX[(b & 15) as usize] as char);
    }
    Ok(result)
}
fn digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}
pub fn decode(text: &str) -> Result<PathBuf, &'static str> {
    let bytes = text.as_bytes();
    if bytes.is_empty() || bytes.len() > MAX_BYTES * 2 || !bytes.len().is_multiple_of(2) {
        return Err("Terminal: ruta hexadecimal fuera del límite");
    }
    // Validate the entire borrowed payload before materializing its path.
    for pair in bytes.as_chunks::<2>().0 {
        let a = digit(pair[0]).ok_or("Terminal: hexadecimal inválido")?;
        let b = digit(pair[1]).ok_or("Terminal: hexadecimal inválido")?;
        if (a << 4 | b) == 0 {
            return Err("Terminal: NUL rechazado");
        }
    }
    let decoded = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|p| digit(p[0]).unwrap() << 4 | digit(p[1]).unwrap())
        .collect::<Vec<_>>();
    #[cfg(unix)]
    let path = {
        use std::os::unix::ffi::OsStringExt;
        PathBuf::from(std::ffi::OsString::from_vec(decoded))
    };
    #[cfg(not(unix))]
    let path =
        PathBuf::from(String::from_utf8(decoded).map_err(|_| "Terminal: ruta no soportada")?);
    valid(&path)?;
    Ok(path)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    #[test]
    fn raw_roundtrip_and_shell_symbols_are_ascii_transport_only() {
        use std::os::unix::ffi::OsStringExt;
        for raw in [
            b"/raw \xff '% $() -- \n".as_slice(),
            "/Unicode/á ñ".as_bytes(),
        ] {
            let p = PathBuf::from(std::ffi::OsString::from_vec(raw.to_vec()));
            let hex = encode(&p).unwrap();
            assert!(hex.bytes().all(|b| b.is_ascii_hexdigit()));
            assert_eq!(decode(&hex).unwrap(), p);
            assert_eq!(decode(&hex.to_uppercase()).unwrap(), p);
        }
    }
    #[test]
    fn hostile_payloads_reject_before_path_admission() {
        for value in ["", "2", "GG", "2f00", "72656c6174697665"] {
            assert!(decode(value).is_err(), "{value}");
        }
        assert!(decode(&"aa".repeat(4097)).is_err());
        assert!(encode(Path::new("relative")).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn exact_byte_budget_is_symmetric() {
        let path = PathBuf::from(format!("/{}", "a".repeat(4095)));
        assert_eq!(encode(&path).unwrap().len(), 8192);
        assert_eq!(decode(&encode(&path).unwrap()).unwrap(), path);
        assert!(encode(&PathBuf::from(format!("/{}", "a".repeat(4096)))).is_err());
    }
}
