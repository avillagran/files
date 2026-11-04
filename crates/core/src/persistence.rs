//! Small, bounded configuration writes. No file in a browsed folder is modified.
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

pub const MAX_CONFIG_BYTES: usize = 4 * 1024 * 1024;
static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);
static CONFIG_UPDATE: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Read at most `limit` bytes, including when the file grows during the read.
pub fn read_bounded(path: &Path, limit: usize) -> io::Result<Vec<u8>> {
    let file = File::open(path)?;
    let mut data = Vec::new();
    file.take(limit.saturating_add(1) as u64)
        .read_to_end(&mut data)?;
    if data.len() > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "file exceeds size limit",
        ));
    }
    Ok(data)
}

/// Write a same-directory exclusive temporary file, sync, then atomically replace.
/// Failures before rename preserve the previous destination. A directory-sync
/// failure after rename is reported, but the new file may already be visible.
pub fn atomic_write(path: &Path, data: &[u8]) -> io::Result<()> {
    if data.len() > MAX_CONFIG_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "file exceeds size limit",
        ));
    }
    atomic_write_inner(path, |file| file.write_all(data))
}

#[derive(Debug)]
pub enum Publication {
    Durable,
    Unsynced(io::Error),
}
/// A failed directory sync is distinguished from failure before publication.
/// The final check runs after temp sync and immediately before rename.
pub fn atomic_write_checked(
    path: &Path,
    data: &[u8],
    check: impl FnOnce() -> io::Result<()>,
) -> io::Result<Publication> {
    if data.len() > MAX_CONFIG_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "file exceeds size limit",
        ));
    }
    atomic_write_checked_inner(
        path,
        |file| file.write_all(data),
        check,
        |parent| {
            #[cfg(unix)]
            {
                File::open(parent)?.sync_all()
            }
            #[cfg(not(unix))]
            {
                let _ = parent;
                Ok(())
            }
        },
    )
}
fn atomic_write_inner(
    path: &Path,
    write: impl FnOnce(&mut File) -> io::Result<()>,
) -> io::Result<()> {
    match atomic_write_checked_inner(
        path,
        write,
        || Ok(()),
        |parent| {
            #[cfg(unix)]
            {
                File::open(parent)?.sync_all()
            }
            #[cfg(not(unix))]
            {
                let _ = parent;
                Ok(())
            }
        },
    )? {
        Publication::Durable => Ok(()),
        Publication::Unsynced(e) => Err(e),
    }
}
fn atomic_write_checked_inner(
    path: &Path,
    write: impl FnOnce(&mut File) -> io::Result<()>,
    check: impl FnOnce() -> io::Result<()>,
    sync: impl FnOnce(&Path) -> io::Result<()>,
) -> io::Result<Publication> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing file name"))?;
    let (temp, mut file) = (0..64)
        .find_map(|_| {
            let mut temp_name = name.to_os_string();
            temp_name.push(format!(
                ".tmp-{}-{}",
                std::process::id(),
                NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
            ));
            let temp = parent.join(temp_name);
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            match options.open(&temp) {
                Ok(file) => Some(Ok((temp, file))),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => None,
                Err(e) => Some(Err(e)),
            }
        })
        .unwrap_or_else(|| {
            Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "temporary name attempts exhausted",
            ))
        })?;
    let result = (|| {
        write(&mut file)?;
        file.sync_all()?;
        drop(file);
        check()?;
        fs::rename(&temp, path)?;
        Ok(match sync(parent) {
            Ok(()) => Publication::Durable,
            Err(error) => Publication::Unsynced(error),
        })
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

/// Update one literal top-level key without discarding unknown fields/tables.
/// Malformed or oversized existing files are left intact. Updates in this
/// process are serialized; external writers must coordinate separately.
pub fn update_config(path: &Path, key: &str, value: toml::Value) -> Result<(), String> {
    update_config_values(path, &[(key, value)])
}

/// Commit a coherent preference snapshot in a single atomic rename.
pub fn update_config_values(path: &Path, values: &[(&str, toml::Value)]) -> Result<(), String> {
    if values.len() > 32
        || values
            .iter()
            .any(|(key, _)| key.is_empty() || key.len() > 256)
    {
        return Err("invalid configuration key".into());
    }
    let _guard = CONFIG_UPDATE
        .lock()
        .map_err(|_| "configuration lock poisoned")?;
    let mut table = match read_bounded(path, MAX_CONFIG_BYTES) {
        Ok(bytes) => {
            let text = std::str::from_utf8(&bytes).map_err(|e| e.to_string())?;
            toml::from_str::<toml::Table>(text).map_err(|e| e.to_string())?
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => toml::Table::new(),
        Err(e) => return Err(e.to_string()),
    };
    for (key, value) in values {
        table.insert((*key).to_owned(), value.clone());
    }
    let text = toml::to_string_pretty(&table).map_err(|e| e.to_string())?;
    atomic_write(path, text.as_bytes()).map_err(|e| e.to_string())
}

#[cfg(test)]
pub(crate) fn test_dir(label: &str) -> std::path::PathBuf {
    let root = crate::theme::home().join(".cache/files-test");
    let path = root.join(format!(
        "{label}-{}-{}",
        std::process::id(),
        NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_write_distinguishes_prepublication_failure_from_unsynced_publication() {
        let root = test_dir("publication-phase");
        let path = root.join("state");
        fs::write(&path, b"old").unwrap();
        let result = atomic_write_checked_inner(
            &path,
            |f| f.write_all(b"new"),
            || Err(io::Error::other("changed")),
            |_| panic!("no sync before publish"),
        );
        assert!(result.is_err());
        assert_eq!(fs::read(&path).unwrap(), b"old");
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        let result = atomic_write_checked_inner(
            &path,
            |f| f.write_all(b"new"),
            || Ok(()),
            |_| Err(io::Error::other("synthetic sync")),
        )
        .unwrap();
        assert!(matches!(result, Publication::Unsynced(_)));
        assert_eq!(fs::read(&path).unwrap(), b"new");
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn config_preserves_unknown_fields_and_rejects_corruption() {
        let dir = test_dir("config");
        let path = dir.join("config.toml");
        fs::write(
            &path,
            "corners='square'\neffects=false\n[future]\nvalue=42\n",
        )
        .unwrap();
        update_config(&path, "corners", toml::Value::String("rounded".into())).unwrap();
        let table: toml::Table = toml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(table["corners"].as_str(), Some("rounded"));
        assert_eq!(table["effects"].as_bool(), Some(false));
        assert_eq!(table["future"]["value"].as_integer(), Some(42));
        fs::write(&path, "[broken").unwrap();
        assert!(update_config(&path, "corners", "square".into()).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "[broken");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn failed_write_preserves_destination_and_removes_temp() {
        let dir = test_dir("atomic");
        let path = dir.join("state.toml");
        fs::write(&path, "original").unwrap();
        let result = atomic_write_inner(&path, |file| {
            file.write_all(b"partial")?;
            Err(io::Error::other("injected write failure"))
        });
        assert!(result.is_err());
        assert_eq!(fs::read(&path).unwrap(), b"original");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        assert!(atomic_write(&path, &vec![0; MAX_CONFIG_BYTES + 1]).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"original");
        atomic_write(&path, b"complete").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"complete");
        fs::remove_dir_all(dir).unwrap();
    }
}
