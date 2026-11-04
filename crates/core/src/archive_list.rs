//! Original, bounded ZIP32 central-directory listing. No decompression, local
//! record matching, CRC/data validation, extraction, filesystem access or execution.
//! Names retain their exact bytes; display escaping is presentation, never a path
//! to open. ZIP64, split disks and central-directory trailers are unsupported.
use std::{
    collections::HashMap,
    io::{self, Read, Seek, SeekFrom},
};
pub const MAX_ENTRIES: usize = 10_000;
pub const MAX_DIRECTORY_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_NAME_BYTES: usize = 4096;
pub const MAX_NAMES_BYTES: usize = 256 * 1024;
const EOCD: [u8; 4] = *b"PK\x05\x06";
const CENTRAL: [u8; 4] = *b"PK\x01\x02";
const ZIP64_LOCATOR: [u8; 4] = *b"PK\x06\x07";
#[derive(Debug)]
pub struct Entry {
    pub name: Vec<u8>,
    pub compressed_bytes: u32,
    pub uncompressed_bytes: u32,
    pub method: u16,
    pub needed_version: u16,
    pub flags: u16,
    pub crc32: u32,
    pub local_offset: u32,
    pub encrypted: bool,
    pub utf8_declared: bool,
    pub unix_mode: Option<u16>,
    pub directory: bool,
    pub symlink: bool,
    pub unsafe_lexical: bool,
    /// Exact raw-name collision, including the first occurrence. This does not
    /// claim casefold/Unicode/path-alias equivalence on any destination FS.
    pub duplicate: bool,
}
#[derive(Debug)]
pub struct Listing {
    pub entries: Vec<Entry>,
    pub directory_offset: u32,
    pub compressed_bytes: u64,
    pub uncompressed_bytes: u64,
}
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn u16_at(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}
fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}
pub fn read(input: &mut (impl Read + Seek)) -> io::Result<Listing> {
    let length = input.seek(SeekFrom::End(0))?;
    if length < 22 {
        return Err(invalid("ZIP incompleto: falta el directorio central"));
    }
    let tail_len = length.min(65_557) as usize;
    input.seek(SeekFrom::Start(length - tail_len as u64))?;
    let mut tail = vec![0u8; tail_len];
    input.read_exact(&mut tail)?;
    let eocd = (0..=tail_len - 22)
        .rev()
        .find(|at| {
            tail[*at..*at + 4] == EOCD && *at + 22 + u16_at(&tail, *at + 20) as usize == tail_len
        })
        .ok_or_else(|| invalid("ZIP incompleto o final/comentario no válido"))?;
    let end_position = length - tail_len as u64 + eocd as u64;
    if end_position >= 20 {
        input.seek(SeekFrom::Start(end_position - 20))?;
        let mut signature = [0; 4];
        input.read_exact(&mut signature)?;
        if signature == ZIP64_LOCATOR {
            return Err(invalid("ZIP64 no admitido en esta vista previa"));
        }
    }
    let end = &tail[eocd..eocd + 22];
    let count = u16_at(end, 10);
    let size = u32_at(end, 12);
    let offset = u32_at(end, 16);
    if count == u16::MAX || size == u32::MAX || offset == u32::MAX {
        return Err(invalid("ZIP64 no admitido en esta vista previa"));
    }
    if u16_at(end, 4) != 0 || u16_at(end, 6) != 0 || u16_at(end, 8) != count {
        return Err(invalid("ZIP multidisco no admitido en esta vista previa"));
    }
    if count as usize > MAX_ENTRIES || size as usize > MAX_DIRECTORY_BYTES {
        return Err(invalid(
            "ZIP excede 10000 entradas o 4MiB de directorio central",
        ));
    }
    let directory_end = u64::from(offset)
        .checked_add(u64::from(size))
        .filter(|value| *value <= length && *value == end_position)
        .ok_or_else(|| {
            invalid("Posición/tamaño del directorio ZIP no válido o tráiler no admitido")
        })?;
    if u64::from(count) * 46 > u64::from(size) || (count == 0 && size != 0) {
        return Err(invalid("Contador/tamaño del directorio ZIP no coincide"));
    }
    // All directory memory is bounded BEFORE allocation; filenames are budgeted
    // before their per-entry copy and exact-duplicate lookup copy.
    input.seek(SeekFrom::Start(u64::from(offset)))?;
    let mut central = vec![0; size as usize];
    input.read_exact(&mut central)?;
    drop(tail);
    let mut entries: Vec<Entry> = Vec::with_capacity(count as usize);
    let mut seen: HashMap<Vec<u8>, usize> = HashMap::with_capacity(count as usize);
    let mut at = 0usize;
    let mut names_bytes = 0usize;
    let mut compressed_bytes = 0u64;
    let mut uncompressed_bytes = 0u64;
    for _ in 0..count {
        let fixed_end = at
            .checked_add(46)
            .filter(|end| *end <= central.len())
            .ok_or_else(|| invalid("Registro central ZIP incompleto"))?;
        let h = &central[at..fixed_end];
        if h[..4] != CENTRAL {
            return Err(invalid("Firma del registro central ZIP no válida"));
        }
        let flags = u16_at(h, 8);
        let compressed = u32_at(h, 20);
        let uncompressed = u32_at(h, 24);
        let local_offset = u32_at(h, 42);
        let disk = u16_at(h, 34);
        if compressed == u32::MAX
            || uncompressed == u32::MAX
            || local_offset == u32::MAX
            || disk == u16::MAX
        {
            return Err(invalid("Entrada ZIP64 no admitida en esta vista previa"));
        }
        if disk != 0 {
            return Err(invalid("Entrada ZIP multidisco no admitida"));
        }
        // This checks range only. Local header signatures, name agreement,
        // compressed payload extents and CRCs are intentionally NOT verified.
        if u64::from(local_offset)
            .checked_add(30)
            .is_none_or(|end| end > u64::from(offset))
        {
            return Err(invalid("Posición del registro local ZIP fuera de rango"));
        }
        let name_len = u16_at(h, 28) as usize;
        let extra_len = u16_at(h, 30) as usize;
        let comment_len = u16_at(h, 32) as usize;
        if name_len == 0 || name_len > MAX_NAME_BYTES {
            return Err(invalid("Nombre ZIP vacío o superior a 4096 bytes"));
        }
        names_bytes = names_bytes
            .checked_add(name_len)
            .filter(|n| *n <= MAX_NAMES_BYTES)
            .ok_or_else(|| invalid("Nombres ZIP exceden el presupuesto agregado de 256KiB"))?;
        let record_end = fixed_end
            .checked_add(name_len)
            .and_then(|n| n.checked_add(extra_len))
            .and_then(|n| n.checked_add(comment_len))
            .filter(|n| *n <= central.len())
            .ok_or_else(|| invalid("Longitud del registro central ZIP fuera de rango"))?;
        let name = &central[fixed_end..fixed_end + name_len];
        let extra_end = fixed_end + name_len + extra_len;
        let mut extra_at = fixed_end + name_len;
        while extra_at < extra_end {
            if extra_end - extra_at < 4 {
                return Err(invalid("Campo adicional ZIP incompleto"));
            }
            let tag = u16_at(&central, extra_at);
            let len = u16_at(&central, extra_at + 2) as usize;
            extra_at = extra_at
                .checked_add(4)
                .and_then(|n| n.checked_add(len))
                .filter(|n| *n <= extra_end)
                .ok_or_else(|| invalid("Longitud adicional ZIP no válida"))?;
            if tag == 1 {
                return Err(invalid("Campo ZIP64 no admitido en esta vista previa"));
            }
        }
        let mut duplicate = false;
        if let Some(index) = seen.get(name) {
            entries[*index].duplicate = true;
            duplicate = true;
        } else {
            seen.insert(name.to_vec(), entries.len());
        }
        let attributes = u32_at(h, 38);
        let host = h[5];
        let unix_mode = matches!(host, 3 | 19).then_some((attributes >> 16) as u16);
        let symlink = unix_mode.is_some_and(|mode| mode & 0o170000 == 0o120000);
        let directory = name.ends_with(b"/")
            || attributes & 0x10 != 0
            || unix_mode.is_some_and(|mode| mode & 0o170000 == 0o040000);
        entries.push(Entry {
            name: name.to_vec(),
            compressed_bytes: compressed,
            uncompressed_bytes: uncompressed,
            method: u16_at(h, 10),
            needed_version: u16_at(h, 6),
            flags,
            crc32: u32_at(h, 16),
            local_offset,
            encrypted: flags & 0x41 != 0,
            utf8_declared: flags & 0x800 != 0,
            unix_mode,
            directory,
            symlink,
            unsafe_lexical: unsafe_name(name),
            duplicate,
        });
        compressed_bytes = compressed_bytes
            .checked_add(u64::from(compressed))
            .ok_or_else(|| invalid("Suma comprimida ZIP fuera de rango"))?;
        uncompressed_bytes = uncompressed_bytes
            .checked_add(u64::from(uncompressed))
            .ok_or_else(|| invalid("Suma sin comprimir ZIP fuera de rango"))?;
        at = record_end;
    }
    if at != central.len() || u64::from(offset) + at as u64 != directory_end {
        return Err(invalid(
            "Contador central ZIP no coincide o hay registros adicionales",
        ));
    }
    Ok(Listing {
        entries,
        directory_offset: offset,
        compressed_bytes,
        uncompressed_bytes,
    })
}
pub fn unsafe_name(name: &[u8]) -> bool {
    name.is_empty()
        || name.starts_with(b"/")
        || name.contains(&b'\\')
        || (name.len() >= 2 && name[0].is_ascii_alphabetic() && name[1] == b':')
        || name.split(|b| *b == b'/').any(|part| part == b"..")
        || name.iter().any(|b| *b < 32 || *b == 127)
        || std::str::from_utf8(name).is_ok_and(|name| {
            name.chars().any(|c| {
                c.is_control() || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
            })
        })
}
/// Backslashes are escaped too, so literal "\\xFF" cannot masquerade as an
/// invalid raw byte. Valid Unicode is preserved; controls and invalid bytes are
/// escaped explicitly. This representation is never a filesystem pathname.
pub fn display_name(mut name: &[u8]) -> String {
    use std::fmt::Write;
    let mut output = String::new();
    while !name.is_empty() {
        let (valid, invalid) = match std::str::from_utf8(name) {
            Ok(text) => (text, 0),
            Err(error) => (
                std::str::from_utf8(&name[..error.valid_up_to()]).expect("validated prefix"),
                error
                    .error_len()
                    .unwrap_or(name.len() - error.valid_up_to()),
            ),
        };
        for character in valid.chars() {
            match character {
                '\\' => output.push_str("\\\\"),
                c if c.is_ascii_control() => {
                    let _ = write!(output, "\\x{:02X}", c as u32);
                }
                c if c.is_control()
                    || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}') =>
                {
                    let _ = write!(output, "\\u{{{:X}}}", c as u32);
                }
                c => output.push(c),
            }
        }
        let valid_len = valid.len();
        for byte in &name[valid_len..valid_len + invalid] {
            let _ = write!(output, "\\x{byte:02X}");
        }
        name = &name[valid_len + invalid..];
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    fn zip(names: &[&[u8]]) -> Vec<u8> {
        let mut data = Vec::new();
        let mut local_offsets = Vec::new();
        for name in names {
            local_offsets.push(data.len() as u32);
            let mut local = [0u8; 30];
            local[..4].copy_from_slice(b"PK\x03\x04");
            local[4..6].copy_from_slice(&20u16.to_le_bytes());
            local[26..28].copy_from_slice(&(name.len() as u16).to_le_bytes());
            data.extend_from_slice(&local);
            data.extend_from_slice(name);
        }
        let offset = data.len() as u32;
        for (i, name) in names.iter().enumerate() {
            let mut h = [0u8; 46];
            h[..4].copy_from_slice(&CENTRAL);
            h[4..6].copy_from_slice(&0x0314u16.to_le_bytes());
            h[6..8].copy_from_slice(&20u16.to_le_bytes());
            h[28..30].copy_from_slice(&(name.len() as u16).to_le_bytes());
            h[42..46].copy_from_slice(&local_offsets[i].to_le_bytes());
            data.extend_from_slice(&h);
            data.extend_from_slice(name);
        }
        let size = data.len() as u32 - offset;
        let mut end = [0; 22];
        end[..4].copy_from_slice(&EOCD);
        end[8..10].copy_from_slice(&(names.len() as u16).to_le_bytes());
        end[10..12].copy_from_slice(&(names.len() as u16).to_le_bytes());
        end[12..16].copy_from_slice(&size.to_le_bytes());
        end[16..20].copy_from_slice(&offset.to_le_bytes());
        data.extend_from_slice(&end);
        data
    }
    fn parse(bytes: Vec<u8>) -> io::Result<Listing> {
        read(&mut Cursor::new(bytes))
    }
    #[test]
    fn empty_and_raw_names_duplicates_symlink_and_directory() {
        assert!(parse(zip(&[])).unwrap().entries.is_empty());
        let names: &[&[u8]] = &[
            b"normal",
            b"normal",
            b"../escape",
            b"dir/",
            b"link",
            b"raw\xff",
        ];
        let mut data = zip(names);
        let link_offset = names.iter().map(|name| 30 + name.len()).sum::<usize>()
            + names[..4].iter().map(|name| 46 + name.len()).sum::<usize>();
        data[link_offset + 38..link_offset + 42]
            .copy_from_slice(&(0o120777u32 << 16).to_le_bytes());
        let list = parse(data).unwrap();
        assert!(list.entries[0].duplicate && list.entries[1].duplicate);
        assert!(list.entries[2].unsafe_lexical && list.entries[3].directory);
        assert!(list.entries[4].symlink);
        assert_eq!(list.entries[5].name, b"raw\xff");
    }
    #[test]
    fn stored_metadata_and_zip64_extra_are_explicit() {
        let mut data = zip(&[b"a"]);
        data[31 + 8..31 + 10].copy_from_slice(&0x801u16.to_le_bytes());
        data[31 + 10..31 + 12].copy_from_slice(&8u16.to_le_bytes());
        data[31 + 20..31 + 24].copy_from_slice(&12u32.to_le_bytes());
        data[31 + 24..31 + 28].copy_from_slice(&45u32.to_le_bytes());
        let list = parse(data).unwrap();
        let entry = &list.entries[0];
        assert!(entry.encrypted && entry.utf8_declared);
        assert_eq!(
            (
                entry.method,
                entry.compressed_bytes,
                entry.uncompressed_bytes
            ),
            (8, 12, 45)
        );
        assert_eq!((list.compressed_bytes, list.uncompressed_bytes), (12, 45));
        // Add an empty ZIP64 extra, even without sentinel dimensions.
        let mut data = zip(&[b"a"]);
        let old_end = data.len() - 22;
        data.splice(old_end..old_end, [1, 0, 0, 0]);
        data[31 + 30..31 + 32].copy_from_slice(&4u16.to_le_bytes());
        let end = data.len() - 22;
        let size = u32_at(&data, end + 12) + 4;
        data[end + 12..end + 16].copy_from_slice(&size.to_le_bytes());
        assert!(parse(data).is_err());
    }
    #[test]
    fn display_is_distinct_for_non_utf8_literal_escape_controls_and_unicode() {
        assert_eq!(display_name(b"raw\xff\n"), "raw\\xFF\\x0A");
        assert_eq!(display_name(b"raw\\xFF"), "raw\\\\xFF");
        assert_eq!(display_name("日本語".as_bytes()), "日本語");
        assert_eq!(display_name(b"\xf0\x9f"), "\\xF0\\x9F");
        for name in [
            b"/a".as_slice(),
            b"C:a",
            b"a\\b",
            b"a/../b",
            b"a\0b",
            "a\u{202e}b".as_bytes(),
        ] {
            assert!(unsafe_name(name));
        }
        assert!(!unsafe_name(b"a/.../b"));
    }
    #[test]
    fn rejects_limits_before_per_name_allocation() {
        let mut count = zip(&[]);
        count[8..10].copy_from_slice(&10001u16.to_le_bytes());
        count[10..12].copy_from_slice(&10001u16.to_le_bytes());
        assert!(parse(count).is_err());
        let mut size = zip(&[]);
        size[12..16].copy_from_slice(&((MAX_DIRECTORY_BYTES + 1) as u32).to_le_bytes());
        assert!(parse(size).is_err());
        let long = vec![b'x'; MAX_NAME_BYTES + 1];
        assert!(parse(zip(&[&long])).is_err());
        let name = vec![b'x'; MAX_NAME_BYTES];
        let names = vec![name.as_slice(); MAX_NAMES_BYTES / MAX_NAME_BYTES + 1];
        assert!(parse(zip(&names)).is_err());
    }
    #[test]
    fn rejects_zip64_multidisk_truncation_and_offset_overflow() {
        for (at, bytes) in [(12, u32::MAX.to_le_bytes()), (16, u32::MAX.to_le_bytes())] {
            let mut data = zip(&[]);
            data[at..at + 4].copy_from_slice(&bytes);
            assert!(parse(data).is_err());
        }
        let mut data = zip(&[]);
        data[4..6].copy_from_slice(&1u16.to_le_bytes());
        assert!(parse(data).is_err());
        let mut data = zip(&[b"a"]);
        let at = data.len() - 22;
        data[at + 16..at + 20].copy_from_slice(&(u32::MAX - 1).to_le_bytes());
        assert!(parse(data).is_err());
        let mut data = zip(&[b"a"]);
        data.pop();
        assert!(parse(data).is_err());
        let mut data = zip(&[b"a"]);
        data[31] = 0;
        assert!(parse(data).is_err());
        let mut data = zip(&[b"a"]);
        data[31 + 30..31 + 32].copy_from_slice(&100u16.to_le_bytes());
        assert!(parse(data).is_err());
    }
    #[test]
    fn eocd_comments_require_exact_end_and_counts() {
        let mut data = zip(&[]);
        data[20..22].copy_from_slice(&3u16.to_le_bytes());
        data.extend_from_slice(b"abc");
        assert!(parse(data.clone()).is_ok());
        data.push(0);
        assert!(parse(data).is_err());
        let mut data = zip(&[b"a"]);
        let end = data.len() - 22;
        data[end + 8..end + 10].copy_from_slice(&0u16.to_le_bytes());
        assert!(parse(data).is_err());
    }
}
