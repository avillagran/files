//! ZIP central-directory text only. No extraction, decompression or execution.
//! Listing metadata does not prove payload integrity or that extraction is safe.
use files_core::archive_list::{self, Entry, Listing};
use std::{io, path::Path};
const MAX_FILE: u64 = 8 * 1024 * 1024 * 1024;
const MAX_TEXT: usize = 128 * 1024;
const MAX_LINES: usize = 2000;
const FOOTER_RESERVE: usize = 256;

pub fn load(path: &Path) -> io::Result<crate::text_preview::Preview> {
    if !path.is_absolute()
        || path.as_os_str().as_encoded_bytes().len() > 4096
        || path.as_os_str().as_encoded_bytes().contains(&0)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Ruta ZIP no válida",
        ));
    }
    #[cfg(not(target_os = "linux"))]
    {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Vista ZIP segura pendiente en esta plataforma",
        ));
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
            .open(path)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() || metadata.len() > MAX_FILE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "ZIP debe ser archivo regular de hasta 8GiB",
            ));
        }
        // The same opened FD is used throughout. Parent symlinks, mounts and
        // concurrent in-place writes remain outside a snapshot guarantee.
        describe(&archive_list::read(&mut file)?)
    }
}
fn row(entry: &Entry) -> String {
    let kind = if entry.symlink {
        "enlace"
    } else if entry.directory {
        "carpeta"
    } else {
        "archivo"
    };
    let mut labels = String::new();
    for (flag, label) in [
        (entry.unsafe_lexical, " · RUTA INSEGURA"),
        (entry.duplicate, " · NOMBRE DUPLICADO"),
        (entry.encrypted, " · cifrado"),
    ] {
        if flag {
            labels.push_str(label);
        }
    }
    let mode = entry
        .unix_mode
        .map(|value| format!(" · modo {value:06o}"))
        .unwrap_or_default();
    format!(
        "{} [{kind}{labels}] · {} B / {} B comprimidos · método {}{mode}\n",
        archive_list::display_name(&entry.name),
        entry.uncompressed_bytes,
        entry.compressed_bytes,
        entry.method
    )
}
fn describe(list: &Listing) -> io::Result<crate::text_preview::Preview> {
    use std::fmt::Write;
    let mut text = String::with_capacity(4096);
    text.push_str("ZIP · listado del directorio central\nSin extracción; datos, registros locales y CRC NO comprobados.\nNo se ejecutan ni se abren los nombres enumerados.\n");
    let _ = writeln!(
        text,
        "{} entradas · {} B declarados sin comprimir · {} B comprimidos\n",
        list.entries.len(),
        list.uncompressed_bytes,
        list.compressed_bytes
    );
    let lines = text.lines().count();
    let mut shown = 0usize;
    for (index, entry) in list.entries.iter().enumerate() {
        let row = row(entry);
        if lines + index + 1 > MAX_LINES - 3
            || text
                .len()
                .checked_add(row.len())
                .is_none_or(|n| n > MAX_TEXT - FOOTER_RESERVE)
        {
            break;
        }
        text.push_str(&row);
        shown = index + 1;
    }
    let truncated = shown != list.entries.len();
    let _ = writeln!(
        text,
        "\nEnumeradas {shown} de {} entradas{}.",
        list.entries.len(),
        if truncated {
            " · LISTADO TRUNCADO"
        } else {
            ""
        }
    );
    if text.len() > MAX_TEXT || text.lines().count() > MAX_LINES {
        return Err(io::Error::other(
            "Listado ZIP fuera del límite de presentación",
        ));
    }
    Ok(crate::text_preview::Preview {
        relative_source: None,
        text,
        encoding: "ZIP · directorio central",
        truncated,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn entry(name: Vec<u8>) -> Entry {
        Entry {
            name,
            compressed_bytes: 1,
            uncompressed_bytes: 20,
            method: 8,
            needed_version: 20,
            flags: 0,
            crc32: 0,
            local_offset: 0,
            encrypted: true,
            utf8_declared: false,
            unix_mode: Some(0o120777),
            directory: false,
            symlink: true,
            unsafe_lexical: true,
            duplicate: true,
        }
    }
    #[test]
    fn safety_labels_raw_names_and_disclaimer_are_visible() {
        let result = describe(&Listing {
            directory_offset: 0,
            entries: vec![entry(b"../raw\xff\n".to_vec())],
            compressed_bytes: 1,
            uncompressed_bytes: 20,
        })
        .unwrap();
        assert!(!result.truncated);
        for expected in [
            "CRC NO comprobados",
            "RUTA INSEGURA",
            "NOMBRE DUPLICADO",
            "cifrado",
            "enlace",
            "\\xFF\\x0A",
            "Enumeradas 1 de 1",
        ] {
            assert!(result.text.contains(expected));
        }
    }
    #[test]
    fn text_and_line_budgets_are_explicit_not_silent() {
        for (count, width) in [(10000, 1), (40, 4096)] {
            let entries = (0..count).map(|_| entry(vec![b'x'; width])).collect();
            let result = describe(&Listing {
                directory_offset: 0,
                entries,
                compressed_bytes: 0,
                uncompressed_bytes: 0,
            })
            .unwrap();
            assert!(result.truncated);
            assert!(result.text.contains("LISTADO TRUNCADO"));
            assert!(result.text.len() <= MAX_TEXT && result.text.lines().count() <= MAX_LINES);
        }
    }
}
