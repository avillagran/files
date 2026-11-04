//! Office adapter over the shared bounded ZIP streaming reader. No filesystem output.
use files_core::{
    archive_list::Listing,
    archive_member::{self, Limits},
};
use std::{
    io::{self, Read, Seek},
    sync::atomic::AtomicBool,
};
const MAX_MEMBER: usize = 1024 * 1024;
pub fn read(
    input: &mut (impl Read + Seek),
    listing: &Listing,
    name: &[u8],
    limit: usize,
) -> io::Result<Vec<u8>> {
    if limit == 0 || limit > MAX_MEMBER {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Límite de contenido ZIP inválido",
        ));
    }
    let index = listing
        .entries
        .iter()
        .position(|e| e.name == name)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "El DOCX no contiene word/document.xml",
            )
        })?;
    let e = &listing.entries[index];
    if e.directory {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "El miembro DOCX no es un archivo",
        ));
    }
    let limits = Limits {
        compressed: 8 * 1024 * 1024,
        output: limit as u64,
    };
    // Validate metadata before reserving even a bounded output Vec.
    archive_member::inspect(input, listing, index, limits)?;
    let mut output = Vec::with_capacity(e.uncompressed_bytes as usize);
    archive_member::copy_to(
        input,
        listing,
        index,
        limits,
        &mut output,
        &AtomicBool::new(false),
        |_| {},
    )?;
    Ok(output)
}
#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::{Cursor, Write};
    fn u32_at(bytes: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
    }
    pub fn fixture(data: &[u8], method: u16, descriptor: bool) -> Vec<u8> {
        let name = b"word/document.xml";
        let packed = if method == 8 {
            let mut e = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::fast());
            e.write_all(data).unwrap();
            e.finish().unwrap()
        } else {
            data.to_vec()
        };
        let crc = crc32fast::hash(data);
        let flags: u16 = if descriptor { 8 } else { 0 };
        let mut local = vec![0u8; 30];
        local[..4].copy_from_slice(b"PK\x03\x04");
        local[4..6].copy_from_slice(&20u16.to_le_bytes());
        local[6..8].copy_from_slice(&flags.to_le_bytes());
        local[8..10].copy_from_slice(&method.to_le_bytes());
        local[26..28].copy_from_slice(&(name.len() as u16).to_le_bytes());
        if !descriptor {
            local[14..18].copy_from_slice(&crc.to_le_bytes());
            local[18..22].copy_from_slice(&(packed.len() as u32).to_le_bytes());
            local[22..26].copy_from_slice(&(data.len() as u32).to_le_bytes());
        }
        local.extend_from_slice(name);
        local.extend_from_slice(&packed);
        if descriptor {
            local.extend_from_slice(b"PK\x07\x08");
            local.extend_from_slice(&crc.to_le_bytes());
            local.extend_from_slice(&(packed.len() as u32).to_le_bytes());
            local.extend_from_slice(&(data.len() as u32).to_le_bytes());
        }
        let offset = local.len() as u32;
        let mut c = vec![0u8; 46];
        c[..4].copy_from_slice(b"PK\x01\x02");
        c[6..8].copy_from_slice(&20u16.to_le_bytes());
        c[8..10].copy_from_slice(&flags.to_le_bytes());
        c[10..12].copy_from_slice(&method.to_le_bytes());
        c[16..20].copy_from_slice(&crc.to_le_bytes());
        c[20..24].copy_from_slice(&(packed.len() as u32).to_le_bytes());
        c[24..28].copy_from_slice(&(data.len() as u32).to_le_bytes());
        c[28..30].copy_from_slice(&(name.len() as u16).to_le_bytes());
        c.extend_from_slice(name);
        let size = c.len() as u32;
        local.extend_from_slice(&c);
        let mut end = vec![0u8; 22];
        end[..4].copy_from_slice(b"PK\x05\x06");
        end[8..10].copy_from_slice(&1u16.to_le_bytes());
        end[10..12].copy_from_slice(&1u16.to_le_bytes());
        end[12..16].copy_from_slice(&size.to_le_bytes());
        end[16..20].copy_from_slice(&offset.to_le_bytes());
        local.extend_from_slice(&end);
        local
    }
    fn load(bytes: Vec<u8>, limit: usize) -> io::Result<Vec<u8>> {
        let mut c = Cursor::new(bytes);
        let listing = files_core::archive_list::read(&mut c)?;
        read(&mut c, &listing, b"word/document.xml", limit)
    }
    #[test]
    fn stored_deflated_and_descriptor_members_have_exact_bytes_and_crc() {
        for method in [0, 8] {
            for descriptor in [false, true] {
                let data = "niño\t文档\n".repeat(400);
                assert_eq!(
                    load(fixture(data.as_bytes(), method, descriptor), MAX_MEMBER).unwrap(),
                    data.as_bytes()
                );
            }
        }
    }
    #[test]
    fn corrupted_local_name_crc_descriptor_and_truncation_fail_closed() {
        let original = fixture(b"hello", 0, false);
        for index in [0, 6, 8, 14, 18, 22, 26, 30, 47] {
            let mut b = original.clone();
            b[index] ^= 1;
            assert!(load(b, MAX_MEMBER).is_err(), "index {index}");
        }
        let mut b = fixture(b"hello", 8, true);
        let end = u32_at(&b, b.len() - 6) as usize;
        b[end - 1] ^= 1;
        assert!(load(b, MAX_MEMBER).is_err());
        let mut b = original;
        b.truncate(b.len() - 3);
        assert!(load(b, MAX_MEMBER).is_err());
    }
    #[test]
    fn declared_and_actual_expansion_are_bounded_before_append() {
        assert!(load(fixture(&vec![b'x'; MAX_MEMBER + 1], 8, false), MAX_MEMBER).is_err());
        let mut b = fixture(&vec![b'x'; 4096], 8, false);
        let end = u32_at(&b, b.len() - 6) as usize;
        b[22..26].copy_from_slice(&1u32.to_le_bytes());
        b[end + 24..end + 28].copy_from_slice(&1u32.to_le_bytes());
        assert!(load(b, MAX_MEMBER).is_err());
        assert!(load(fixture(b"x", 0, false), 0).is_err());
        assert!(load(fixture(b"x", 0, false), MAX_MEMBER + 1).is_err());
        assert_eq!(load(fixture(b"", 8, false), 1).unwrap(), b"");
    }
}
