//! Strict bounded Linux mountinfo observations; no filesystem or mount operations.
#![cfg(target_os = "linux")]
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};
const MAX_BYTES: usize = 1024 * 1024;
const MAX_RECORDS: usize = 4096;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mount {
    pub id: u64,
    pub parent: u64,
    pub device: (u32, u32),
    pub root: PathBuf,
    pub point: PathBuf,
    pub filesystem: Vec<u8>,
    pub source: OsString,
}
fn number<T: std::str::FromStr>(b: &[u8]) -> Result<T, &'static str> {
    if b.is_empty() || !b.iter().all(u8::is_ascii_digit) {
        return Err("Número de montaje inválido");
    }
    std::str::from_utf8(b)
        .map_err(|_| "Número inválido")?
        .parse()
        .map_err(|_| "Número de montaje fuera del límite")
}
#[cfg(unix)]
fn field(b: &[u8], limit: usize) -> Result<OsString, &'static str> {
    use std::os::unix::ffi::OsStringExt;
    fn visit(b: &[u8], mut emit: impl FnMut(u8)) -> Result<usize, &'static str> {
        let (mut i, mut n) = (0, 0);
        while i < b.len() {
            let v = if b[i] == b'\\' {
                let code = b.get(i + 1..i + 4).ok_or("Escape de montaje truncado")?;
                i += 4;
                match code {
                    b"040" => b' ',
                    b"011" => b'\t',
                    b"012" => b'\n',
                    b"134" => b'\\',
                    _ => return Err("Escape de montaje inválido"),
                }
            } else {
                let v = b[i];
                i += 1;
                if v == 0 || v.is_ascii_whitespace() {
                    return Err("Campo de montaje inválido");
                }
                v
            };
            emit(v);
            n += 1;
        }
        Ok(n)
    }
    let n = visit(b, |_| {})?;
    if n == 0 || n > limit {
        return Err("Campo de montaje fuera del límite");
    }
    let mut out = Vec::with_capacity(n);
    visit(b, |c| out.push(c))?;
    Ok(OsString::from_vec(out))
}
#[cfg(unix)]
pub fn parse(bytes: &[u8]) -> Result<Vec<Mount>, &'static str> {
    if bytes.is_empty() || bytes.len() > MAX_BYTES || !bytes.ends_with(b"\n") {
        return Err("Tabla de montajes truncada o superior a1MiB");
    }
    let mut records = Vec::new();
    for line in bytes[..bytes.len() - 1].split(|&b| b == b'\n') {
        if line.iter().any(|&b| b.is_ascii_control()) {
            return Err("Control sin escapar en tabla de montajes");
        }
        if records.len() == MAX_RECORDS || line.len() > 16 * 1024 {
            return Err("Tabla de montajes fuera del presupuesto");
        }
        let mut parts = [b"".as_slice(); 64];
        let mut count = 0;
        for part in line.split(|&b| b == b' ') {
            if count == parts.len() || part.is_empty() {
                return Err("Registro de montaje inválido");
            }
            parts[count] = part;
            count += 1;
        }
        let p = &parts[..count];
        let dash = p
            .iter()
            .position(|&s| s == b"-")
            .ok_or("Separador de montaje ausente")?;
        if dash < 6 || count != dash + 4 {
            return Err("Campos de montaje incompletos");
        }
        let id = number(p[0])?;
        if id == 0 {
            return Err("ID de montaje inválido");
        }
        let parent = number(p[1])?;
        let colon = p[2]
            .iter()
            .position(|&c| c == b':')
            .ok_or("Dispositivo de montaje inválido")?;
        let device = (number(&p[2][..colon])?, number(&p[2][colon + 1..])?);
        let root = PathBuf::from(field(p[3], 4096)?);
        let point = PathBuf::from(field(p[4], 4096)?);
        if !root.is_absolute() || !point.is_absolute() {
            return Err("Ruta de montaje no absoluta");
        }
        if p[dash + 1].len() > 64 || p[dash + 1].contains(&b'\\') {
            return Err("Tipo de montaje inválido");
        }
        let filesystem = p[dash + 1].to_vec();
        let source = field(p[dash + 2], 4096)?;
        records.push(Mount {
            id,
            parent,
            device,
            root,
            point,
            filesystem,
            source,
        });
    }
    // Duplicated IDs make any later identity lookup ambiguous. No silent partial table.
    let mut ids: Vec<_> = records.iter().map(|r| r.id).collect();
    ids.sort_unstable();
    if ids.windows(2).any(|w| w[0] == w[1]) {
        return Err("IDs de montaje duplicados");
    }
    Ok(records)
}
/// Observation only: a unique path, exact caller-supplied source marker and SSHFS type.
/// This is NOT ownership authority or an atomic permission to unmount a pathname later.
pub fn unique_sshfs<'a>(
    mounts: &'a [Mount],
    point: &Path,
    source: &std::ffi::OsStr,
) -> Result<&'a Mount, &'static str> {
    let mut matching = mounts.iter().filter(|m| m.point == point);
    let record = matching.next().ok_or("El montaje ya no está presente")?;
    if matching.next().is_some() {
        return Err("Hay montajes superpuestos; no desmontar por inferencia");
    }
    if record.filesystem != b"fuse.sshfs" || record.source != source {
        return Err("El montaje no coincide con el esperado");
    }
    Ok(record)
}
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    #[test]
    fn raw_paths_and_kernel_escapes_are_preserved() {
        use std::os::unix::ffi::OsStrExt;
        let v = parse(b"42 1 0:8 / /owned/a\\040b-\xff rw shared:2 - fuse.sshfs files-sftp-1 rw\n")
            .unwrap();
        assert_eq!(v[0].point.as_os_str().as_bytes(), b"/owned/a b-\xff");
        assert_eq!(v[0].device, (0, 8));
        assert_eq!(v[0].id, 42);
        assert_eq!(
            unique_sshfs(&v, &v[0].point, std::ffi::OsStr::new("files-sftp-1"))
                .unwrap()
                .id,
            42
        );
    }
    #[test]
    fn truncation_bad_fields_escapes_and_duplicate_ids_reject_the_entire_table() {
        for b in [
            b"".as_slice(),
            b"42 1 0:8 / /x rw - fuse.sshfs source rw".as_slice(),
            b"42 1 0:8 / /x\\999 rw - fuse.sshfs source rw\n",
            b"42 1 0:8 / /x rw - fuse.sshfs\n",
            b"0 1 0:8 / /x rw - fuse.sshfs source rw\n",
            b"42 1 0:8 / /x rw - fuse.sshfs source rw\n42 1 0:8 / /y rw - fuse.sshfs source rw\n",
        ] {
            assert!(parse(b).is_err());
        }
        assert!(parse(&vec![b'x'; MAX_BYTES + 1]).is_err());
    }
    #[test]
    fn stacked_or_substituted_mounts_never_select_an_arbitrary_record() {
        let v = parse(
            b"42 1 0:8 / /x rw - fuse.sshfs files-sftp-1 rw\n43 1 0:9 / /x rw - tmpfs actor rw\n",
        )
        .unwrap();
        assert!(unique_sshfs(&v, Path::new("/x"), std::ffi::OsStr::new("files-sftp-1")).is_err());
        assert!(
            unique_sshfs(
                &v[1..],
                Path::new("/x"),
                std::ffi::OsStr::new("files-sftp-1")
            )
            .is_err()
        );
        assert!(unique_sshfs(&[], Path::new("/x"), std::ffi::OsStr::new("files-sftp-1")).is_err());
    }
}
