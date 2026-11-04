//! ZIP32 member validation/streaming only. May write unverified bytes to caller's
//! sink before final CRC: caller must retain private staging and publish separately.
use crate::archive_list::{Entry, Listing};
use flate2::{Decompress, FlushDecompress, Status};
use std::{
    io::{self, Read, Seek, SeekFrom, Write},
    sync::atomic::{AtomicBool, Ordering},
};
pub const INPUT_CHUNK: usize = 8192;
pub const OUTPUT_CHUNK: usize = 32768;
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub compressed: u64,
    pub output: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub local_start: u64,
    pub payload_start: u64,
    pub payload_end: u64,
    pub record_end: u64,
}
fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes(b[at..at + 2].try_into().unwrap())
}
fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}
fn check(cancel: &AtomicBool) -> io::Result<()> {
    if cancel.load(Ordering::Acquire) {
        Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "Lectura ZIP cancelada",
        ))
    } else {
        Ok(())
    }
}
fn exact(input: &mut impl Read, bytes: &mut [u8], cancel: &AtomicBool) -> io::Result<()> {
    let mut at = 0;
    while at < bytes.len() {
        check(cancel)?;
        match input.read(&mut bytes[at..]) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "ZIP incompleto",
                ));
            }
            Ok(n) => at += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    check(cancel)
}
pub fn validate(entry: &Entry, limits: Limits) -> io::Result<()> {
    let kind = entry.unix_mode.map_or(0, |m| m & 0o170000);
    if entry.name.is_empty()
        || entry.name.len() > crate::archive_list::MAX_NAME_BYTES
        || crate::archive_list::unsafe_name(&entry.name)
        || entry.duplicate
        || entry.encrypted
        || entry.symlink
        || entry.unsafe_lexical
        || !matches!(entry.method, 0 | 8)
        || entry.flags & !0x80e != 0
        || (entry.method == 0 && entry.flags & 6 != 0)
        || !matches!(entry.needed_version, 10 | 20)
        || (entry.method == 8 && entry.needed_version < 20)
        || (entry.directory && !matches!(kind, 0 | 0o040000))
        || (!entry.directory && !matches!(kind, 0 | 0o100000))
    {
        return Err(invalid(
            "Miembro ZIP cifrado, duplicado, enlace, tipo, versión o método/flags no admitidos",
        ));
    }
    if u64::from(entry.compressed_bytes) > limits.compressed
        || u64::from(entry.uncompressed_bytes) > limits.output
        || (entry.method == 0 && entry.compressed_bytes != entry.uncompressed_bytes)
        || (entry.directory && (entry.uncompressed_bytes != 0 || entry.crc32 != 0))
    {
        return Err(invalid(
            "Tamaños ZIP contradictorios o fuera del presupuesto",
        ));
    }
    if entry.flags & 0x800 != 0 && std::str::from_utf8(&entry.name).is_err() {
        return Err(invalid(
            "Nombre ZIP declara UTF8 pero sus bytes no son válidos",
        ));
    }
    Ok(())
}
/// Local/central metadata agreement, bounded extras and signed/unsigned descriptor.
/// Does not read/decode/validate payload content; returns observational ranges only.
pub fn inspect(
    input: &mut (impl Read + Seek),
    listing: &Listing,
    index: usize,
    limits: Limits,
) -> io::Result<Span> {
    inspect_cancellable(input, listing, index, limits, &AtomicBool::new(false))
}
pub fn inspect_cancellable(
    input: &mut (impl Read + Seek),
    listing: &Listing,
    index: usize,
    limits: Limits,
    cancel: &AtomicBool,
) -> io::Result<Span> {
    check(cancel)?;
    let e = listing
        .entries
        .get(index)
        .ok_or_else(|| invalid("Índice ZIP fuera de rango"))?;
    validate(e, limits)?;
    input.seek(SeekFrom::Start(u64::from(e.local_offset)))?;
    let mut h = [0; 30];
    exact(input, &mut h, cancel)?;
    if h[..4] != *b"PK\x03\x04"
        || u16_at(&h, 4) != e.needed_version
        || u16_at(&h, 6) != e.flags
        || u16_at(&h, 8) != e.method
    {
        return Err(invalid("Registro local ZIP no coincide con el directorio"));
    }
    let names = usize::from(u16_at(&h, 26));
    let extras = usize::from(u16_at(&h, 28));
    if names != e.name.len() {
        return Err(invalid("Nombre local ZIP no coincide"));
    }
    let payload_start = u64::from(e.local_offset) + 30 + names as u64 + extras as u64;
    let payload_end = payload_start
        .checked_add(u64::from(e.compressed_bytes))
        .filter(|&v| v <= u64::from(listing.directory_offset))
        .ok_or_else(|| invalid("Datos ZIP fuera del área de contenido"))?;
    let mut name = vec![0; names];
    exact(input, &mut name, cancel)?;
    if name != e.name {
        return Err(invalid("Nombre local ZIP contradictorio"));
    }
    drop(name);
    // One extra field at a time, not a Vec per 64KiB header.
    let mut remaining = extras;
    let mut scratch = [0; 1024];
    while remaining > 0 {
        if remaining < 4 {
            return Err(invalid("Extra local ZIP incompleto"));
        }
        let mut header = [0; 4];
        exact(input, &mut header, cancel)?;
        remaining -= 4;
        let tag = u16_at(&header, 0);
        let size = usize::from(u16_at(&header, 2));
        if tag == 1 || size > remaining {
            return Err(invalid("Extra local ZIP inválido o ZIP64 no admitido"));
        }
        let mut skip = size;
        while skip > 0 {
            let n = skip.min(scratch.len());
            exact(input, &mut scratch[..n], cancel)?;
            skip -= n;
        }
        remaining -= size;
    }
    let descriptor = e.flags & 8 != 0;
    for (at, expected) in [
        (14, e.crc32),
        (18, e.compressed_bytes),
        (22, e.uncompressed_bytes),
    ] {
        let actual = u32_at(&h, at);
        if actual != expected && !(descriptor && actual == 0) {
            return Err(invalid("CRC/tamaño local ZIP contradictorio"));
        }
    }
    let mut record_end = payload_end;
    if descriptor {
        let available = u64::from(listing.directory_offset) - payload_end;
        if available < 12 {
            return Err(invalid("Descriptor ZIP fuera de rango"));
        }
        input.seek(SeekFrom::Start(payload_end))?;
        let mut d = [0; 16];
        exact(input, &mut d[..12], cancel)?;
        let unsigned = u32_at(&d, 0) == e.crc32
            && u32_at(&d, 4) == e.compressed_bytes
            && u32_at(&d, 8) == e.uncompressed_bytes;
        let mut signed = false;
        if d[..4] == *b"PK\x07\x08" && available >= 16 {
            exact(input, &mut d[12..], cancel)?;
            signed = u32_at(&d, 4) == e.crc32
                && u32_at(&d, 8) == e.compressed_bytes
                && u32_at(&d, 12) == e.uncompressed_bytes;
        }
        record_end += match (unsigned, signed) {
            (true, false) => 12,
            (false, true) => 16,
            _ => return Err(invalid("Descriptor ZIP contradictorio o ambiguo")),
        };
    }
    Ok(Span {
        local_start: u64::from(e.local_offset),
        payload_start,
        payload_end,
        record_end,
    })
}
/// Output is provisional until this returns Ok and caller independently verifies
/// source revision, complete-archive plan, destination identity/sync/publication.
pub fn copy_to(
    input: &mut (impl Read + Seek),
    listing: &Listing,
    index: usize,
    limits: Limits,
    output: &mut impl Write,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64),
) -> io::Result<u64> {
    check(cancel)?;
    let span = inspect_cancellable(input, listing, index, limits, cancel)?;
    check(cancel)?;
    let e = &listing.entries[index];
    input.seek(SeekFrom::Start(span.payload_start))?;
    let mut checksum = crc32fast::Hasher::new();
    let mut emitted = 0u64;
    let mut emit = |bytes: &[u8]| -> io::Result<()> {
        check(cancel)?;
        let next = emitted
            .checked_add(bytes.len() as u64)
            .filter(|&n| n <= limits.output && n <= u64::from(e.uncompressed_bytes))
            .ok_or_else(|| invalid("Expansión ZIP supera tamaño o presupuesto"))?;
        output.write_all(bytes)?;
        check(cancel)?;
        checksum.update(bytes);
        emitted = next;
        progress(emitted);
        Ok(())
    };
    let mut encoded = [0; INPUT_CHUNK];
    let mut remaining = u64::from(e.compressed_bytes);
    if e.method == 0 {
        while remaining > 0 {
            check(cancel)?;
            let wanted = remaining.min(encoded.len() as u64) as usize;
            exact(input, &mut encoded[..wanted], cancel)?;
            check(cancel)?;
            emit(&encoded[..wanted])?;
            remaining -= wanted as u64;
        }
    } else {
        let mut decoder = Decompress::new(false);
        let mut decoded = [0; OUTPUT_CHUNK];
        let (mut at, mut length) = (0, 0);
        loop {
            check(cancel)?;
            if at == length && remaining > 0 {
                length = remaining.min(encoded.len() as u64) as usize;
                exact(input, &mut encoded[..length], cancel)?;
                remaining -= length as u64;
                at = 0;
                check(cancel)?;
            }
            let before_in = decoder.total_in();
            let before_out = decoder.total_out();
            // Never Finish: its API assumes enough output for the entire remainder.
            // Require actual StreamEnd even after all declared input is exhausted.
            let status = decoder
                .decompress(&encoded[at..length], &mut decoded, FlushDecompress::None)
                .map_err(|_| invalid("DEFLATE ZIP inválido"))?;
            let used = (decoder.total_in() - before_in) as usize;
            let produced = (decoder.total_out() - before_out) as usize;
            at += used;
            if produced > 0 {
                emit(&decoded[..produced])?;
            }
            if status == Status::StreamEnd {
                if remaining != 0
                    || at != length
                    || decoder.total_in() != u64::from(e.compressed_bytes)
                {
                    return Err(invalid("DEFLATE ZIP contiene bytes adicionales"));
                }
                break;
            }
            if used == 0 && produced == 0 {
                return Err(invalid("DEFLATE ZIP incompleto o sin progreso"));
            }
        }
    }
    check(cancel)?;
    if emitted != u64::from(e.uncompressed_bytes) || checksum.finalize() != e.crc32 {
        return Err(invalid("Contenido ZIP incompleto o CRC incorrecto"));
    }
    Ok(emitted)
}
#[cfg(test)]
pub(crate) mod fixtures {
    use std::io::Write;
    pub struct Spec<'a> {
        pub name: &'a [u8],
        pub data: &'a [u8],
        pub method: u16,
        pub descriptor: u8,
        pub mode: u16,
        pub flags: u16,
        pub packed: Option<&'a [u8]>,
        pub declared: Option<u32>,
        pub crc: Option<u32>,
    }
    impl<'a> Spec<'a> {
        pub fn file(name: &'a [u8], data: &'a [u8]) -> Self {
            Self {
                name,
                data,
                method: 8,
                descriptor: 0,
                mode: 0o100600,
                flags: 0,
                packed: None,
                declared: None,
                crc: None,
            }
        }
    }
    pub fn zip(members: &[Spec<'_>]) -> Vec<u8> {
        let mut bytes = Vec::new();
        let mut central = Vec::new();
        for e in members {
            let encoded = if let Some(p) = e.packed {
                p.to_vec()
            } else if e.method == 8 {
                let mut z =
                    flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::fast());
                z.write_all(e.data).unwrap();
                z.finish().unwrap()
            } else {
                e.data.to_vec()
            };
            let offset = bytes.len() as u32;
            let crc = e.crc.unwrap_or_else(|| crc32fast::hash(e.data));
            let size = e.declared.unwrap_or(e.data.len() as u32);
            let packed = encoded.len() as u32;
            let flags = e.flags | if e.descriptor > 0 { 8 } else { 0 };
            let mut h = [0; 30];
            h[..4].copy_from_slice(b"PK\x03\x04");
            h[4..6].copy_from_slice(&20u16.to_le_bytes());
            h[6..8].copy_from_slice(&flags.to_le_bytes());
            h[8..10].copy_from_slice(&e.method.to_le_bytes());
            h[26..28].copy_from_slice(&(e.name.len() as u16).to_le_bytes());
            if e.descriptor == 0 {
                h[14..18].copy_from_slice(&crc.to_le_bytes());
                h[18..22].copy_from_slice(&packed.to_le_bytes());
                h[22..26].copy_from_slice(&size.to_le_bytes());
            }
            bytes.extend_from_slice(&h);
            bytes.extend_from_slice(e.name);
            bytes.extend_from_slice(&encoded);
            if e.descriptor > 0 {
                if e.descriptor == 2 {
                    bytes.extend_from_slice(b"PK\x07\x08");
                }
                for n in [crc, packed, size] {
                    bytes.extend_from_slice(&n.to_le_bytes());
                }
            }
            let mut c = [0; 46];
            c[..4].copy_from_slice(b"PK\x01\x02");
            c[4..6].copy_from_slice(&0x314u16.to_le_bytes());
            c[6..8].copy_from_slice(&20u16.to_le_bytes());
            c[8..10].copy_from_slice(&flags.to_le_bytes());
            c[10..12].copy_from_slice(&e.method.to_le_bytes());
            c[16..20].copy_from_slice(&crc.to_le_bytes());
            c[20..24].copy_from_slice(&packed.to_le_bytes());
            c[24..28].copy_from_slice(&size.to_le_bytes());
            c[28..30].copy_from_slice(&(e.name.len() as u16).to_le_bytes());
            c[38..42].copy_from_slice(&(u32::from(e.mode) << 16).to_le_bytes());
            c[42..46].copy_from_slice(&offset.to_le_bytes());
            central.extend_from_slice(&c);
            central.extend_from_slice(e.name);
        }
        let offset = bytes.len() as u32;
        let length = central.len() as u32;
        bytes.extend_from_slice(&central);
        let mut e = [0; 22];
        e[..4].copy_from_slice(b"PK\x05\x06");
        e[8..10].copy_from_slice(&(members.len() as u16).to_le_bytes());
        e[10..12].copy_from_slice(&(members.len() as u16).to_le_bytes());
        e[12..16].copy_from_slice(&length.to_le_bytes());
        e[16..20].copy_from_slice(&offset.to_le_bytes());
        bytes.extend_from_slice(&e);
        bytes
    }
}
#[cfg(test)]
mod tests {
    use super::fixtures::{Spec, zip};
    use super::*;
    use std::io::Cursor;
    fn limits() -> Limits {
        Limits {
            compressed: 8 * 1024 * 1024,
            output: 1024 * 1024,
        }
    }
    fn run(bytes: Vec<u8>, output: &mut impl Write) -> io::Result<u64> {
        let mut r = Cursor::new(bytes);
        let listing = crate::archive_list::read(&mut r)?;
        copy_to(
            &mut r,
            &listing,
            0,
            limits(),
            output,
            &AtomicBool::new(false),
            |_| {},
        )
    }
    #[test]
    fn streams_stored_deflate_signed_unsigned_and_empty_members() {
        let data = "Datos ñ🙂\n".repeat(5000);
        for method in [0, 8] {
            for descriptor in [0, 1, 2] {
                let mut e = Spec::file(b"raw-\xff", data.as_bytes());
                e.method = method;
                e.descriptor = descriptor;
                let mut out = Vec::new();
                assert_eq!(run(zip(&[e]), &mut out).unwrap(), data.len() as u64);
                assert_eq!(out, data.as_bytes());
            }
        }
        assert_eq!(
            run(zip(&[Spec::file(b"empty", b"")]), &mut Vec::new()).unwrap(),
            0
        );
    }
    #[test]
    fn empty_truncated_deflate_remains_rejected_by_shared_reader() {
        let mut old = flate2::read::DeflateDecoder::new(&b""[..]);
        let mut v = Vec::new();
        let legacy = old.read_to_end(&mut v);
        assert!(legacy.is_err());
        assert_eq!(old.total_in(), 0);
        let mut e = Spec::file(b"empty", b"");
        e.packed = Some(b"");
        assert!(run(zip(&[e]), &mut Vec::new()).is_err());
    }
    #[test]
    fn crc_trailing_and_truncated_streams_never_return_success() {
        let mut e = Spec::file(b"badcrc", b"data");
        e.crc = Some(1);
        assert!(run(zip(&[e]), &mut Vec::new()).is_err());
        let mut z = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::fast());
        z.write_all(b"data").unwrap();
        let valid = z.finish().unwrap();
        let mut packed = valid.clone();
        packed.extend_from_slice(b"trailing");
        let mut e = Spec::file(b"trailing", b"data");
        e.packed = Some(&packed);
        assert!(run(zip(&[e]), &mut Vec::new()).is_err());
        let mut e = Spec::file(b"truncated", b"data");
        e.packed = Some(&valid[..valid.len() - 1]);
        assert!(run(zip(&[e]), &mut Vec::new()).is_err());
    }
    #[test]
    fn expansion_is_rejected_before_sink_accepts_bytes_beyond_declared_quota() {
        let mut e = Spec::file(b"bomb", b"must not be written");
        e.declared = Some(0);
        let mut out = Vec::new();
        assert!(run(zip(&[e]), &mut out).is_err());
        assert!(out.is_empty());
        let bytes = zip(&[Spec::file(b"limit", b"12345")]);
        let mut r = Cursor::new(bytes);
        let list = crate::archive_list::read(&mut r).unwrap();
        assert!(
            copy_to(
                &mut r,
                &list,
                0,
                Limits {
                    compressed: 99,
                    output: 4
                },
                &mut out,
                &AtomicBool::new(false),
                |_| {}
            )
            .is_err()
        );
        assert!(out.is_empty());
    }
    #[test]
    fn cancel_before_and_after_progress_never_reports_completed_output() {
        let data = vec![0; 200000];
        let mut r = Cursor::new(zip(&[Spec::file(b"large", &data)]));
        let list = crate::archive_list::read(&mut r).unwrap();
        let cancel = AtomicBool::new(true);
        let mut out = Vec::new();
        assert_eq!(
            copy_to(&mut r, &list, 0, limits(), &mut out, &cancel, |_| {})
                .unwrap_err()
                .kind(),
            io::ErrorKind::Interrupted
        );
        assert!(out.is_empty());
        cancel.store(false, Ordering::Release);
        assert_eq!(
            copy_to(&mut r, &list, 0, limits(), &mut out, &cancel, |_| cancel
                .store(true, Ordering::Release))
            .unwrap_err()
            .kind(),
            io::ErrorKind::Interrupted
        );
        assert!(!out.is_empty() && out.len() < data.len());
    }
    #[test]
    fn unsigned_crc_equal_to_descriptor_signature_is_not_mistaken_for_signed() {
        for descriptor in [1, 2] {
            let mut e = Spec::file(b"signature-crc", b"data");
            e.descriptor = descriptor;
            e.crc = Some(0x08074b50);
            let mut r = Cursor::new(zip(&[e]));
            let list = crate::archive_list::read(&mut r).unwrap();
            let span = inspect(&mut r, &list, 0, limits()).unwrap();
            assert_eq!(
                span.record_end - span.payload_end,
                if descriptor == 1 { 12 } else { 16 }
            );
            assert!(
                copy_to(
                    &mut r,
                    &list,
                    0,
                    limits(),
                    &mut Vec::new(),
                    &AtomicBool::new(false),
                    |_| {}
                )
                .is_err()
            ); // deliberately false payload CRC remains an execution failure
        }
    }
    #[test]
    fn cancellation_after_interrupted_read_is_not_swallowed_by_read_exact() {
        let bytes = zip(&[Spec::file(b"cancel", b"data")]);
        let list = crate::archive_list::read(&mut Cursor::new(&bytes)).unwrap();
        let cancel = AtomicBool::new(false);
        struct Interrupted<'a> {
            input: Cursor<Vec<u8>>,
            cancel: &'a AtomicBool,
            calls: usize,
        }
        impl Read for Interrupted<'_> {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                self.calls += 1;
                assert_eq!(self.calls, 1, "must not retry after cancellation");
                self.cancel.store(true, Ordering::Release);
                Err(io::Error::from(io::ErrorKind::Interrupted))
            }
        }
        impl Seek for Interrupted<'_> {
            fn seek(&mut self, p: SeekFrom) -> io::Result<u64> {
                self.input.seek(p)
            }
        }
        let mut input = Interrupted {
            input: Cursor::new(bytes),
            cancel: &cancel,
            calls: 0,
        };
        let mut output = Vec::new();
        assert_eq!(
            copy_to(&mut input, &list, 0, limits(), &mut output, &cancel, |_| {})
                .unwrap_err()
                .kind(),
            io::ErrorKind::Interrupted
        );
        assert!(output.is_empty());
    }
    #[test]
    fn short_interrupted_reads_and_failing_sink_are_handled() {
        struct Short(Cursor<Vec<u8>>, bool);
        impl Read for Short {
            fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
                if !self.1 {
                    self.1 = true;
                    return Err(io::Error::from(io::ErrorKind::Interrupted));
                }
                let n = b.len().min(3);
                self.0.read(&mut b[..n])
            }
        }
        impl Seek for Short {
            fn seek(&mut self, p: SeekFrom) -> io::Result<u64> {
                self.0.seek(p)
            }
        }
        let mut r = Short(
            Cursor::new(zip(&[Spec::file(b"short", b"bytes unchanged")])),
            false,
        );
        let list = crate::archive_list::read(&mut r).unwrap();
        let mut out = Vec::new();
        copy_to(
            &mut r,
            &list,
            0,
            limits(),
            &mut out,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
        assert_eq!(out, b"bytes unchanged");
        struct Fail;
        impl Write for Fail {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::Error::from(io::ErrorKind::StorageFull))
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        assert_eq!(
            run(zip(&[Spec::file(b"failure", b"bytes")]), &mut Fail)
                .unwrap_err()
                .kind(),
            io::ErrorKind::StorageFull
        );
    }
}
