//! Small Nautilus actions: "New document" from `~/Templates` and "New
//! folder with selection". Never replace anything: free names are chosen
//! and moves use renameat2(RENAME_NOREPLACE) inside the same folder.
use std::{
    ffi::{OsStr, OsString},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

/// Largest template copied (templates are small starter documents).
pub const MAX_TEMPLATE: u64 = 16 * 1024 * 1024;
const MAX_TEMPLATES: usize = 32;

/// The user's templates folder (`XDG_TEMPLATES_DIR`, else `~/Templates`).
pub fn templates_dir() -> PathBuf {
    let home = crate::theme::home();
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".config"));
    if let Ok(text) = std::fs::read_to_string(config.join("user-dirs.dirs")) {
        for line in text.lines() {
            if let Some(v) = line.trim().strip_prefix("XDG_TEMPLATES_DIR=") {
                let v = v.trim().trim_matches('"');
                let path = match v.strip_prefix("$HOME") {
                    Some(rest) => home.join(rest.trim_start_matches('/')),
                    None => PathBuf::from(v),
                };
                if path.is_absolute() && path != home {
                    return path;
                }
            }
        }
    }
    home.join("Templates")
}

/// Regular files in the templates folder (not hidden), sorted by name.
pub fn templates(dir: &Path) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = rd
        .flatten()
        .filter(|e| !e.file_name().as_encoded_bytes().starts_with(b"."))
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.path())
        .collect();
    out.sort();
    out.truncate(MAX_TEMPLATES);
    out
}

/// "name", "name (2)", … keeping the extension: "doc.txt" → "doc (2).txt".
pub fn numbered(name: &OsStr, n: u32) -> OsString {
    if n <= 1 {
        return name.to_os_string();
    }
    let s = name.to_string_lossy();
    match s.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => OsString::from(format!("{stem} ({n}).{ext}")),
        _ => OsString::from(format!("{s} ({n})")),
    }
}

/// Copies `template` into `dir` under a free name; returns the new path.
pub fn from_template(template: &Path, dir: &Path) -> io::Result<PathBuf> {
    let name = template
        .file_name()
        .ok_or_else(|| io::Error::other("plantilla sin nombre"))?;
    let mut src = std::fs::File::open(template)?;
    if src.metadata()?.len() > MAX_TEMPLATE {
        return Err(io::Error::other("plantilla demasiado grande (16 MiB)"));
    }
    for n in 1..=999 {
        let path = dir.join(numbered(name, n));
        let mut dst = match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(f) => f,
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        };
        let mut buf = Vec::new();
        Read::take(&mut src, MAX_TEMPLATE).read_to_end(&mut buf)?;
        if let Err(e) = dst.write_all(&buf).and_then(|_| dst.sync_all()) {
            let _ = std::fs::remove_file(&path);
            return Err(e);
        }
        return Ok(path);
    }
    Err(io::Error::other("no hay un nombre libre"))
}

#[derive(Debug, Default)]
pub struct FolderWith {
    pub folder: PathBuf,
    pub moved: usize,
    pub failures: Vec<(OsString, String)>,
}

/// Creates a free "Carpeta nueva" in `dir` and moves `names` into it.
pub fn folder_with_selection(dir: &Path, names: &[OsString]) -> io::Result<FolderWith> {
    let base = OsStr::new("Carpeta nueva");
    let mut folder = None;
    for n in 1..=999 {
        let p = dir.join(numbered(base, n));
        match std::fs::create_dir(&p) {
            Ok(()) => {
                folder = Some(p);
                break;
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    let folder = folder.ok_or_else(|| io::Error::other("no hay un nombre libre"))?;
    let mut out = FolderWith {
        folder: folder.clone(),
        ..Default::default()
    };
    for name in names {
        if name.is_empty()
            || name.as_encoded_bytes().contains(&b'/')
            || folder.file_name() == Some(name)
        {
            out.failures.push((name.clone(), "nombre no válido".into()));
            continue;
        }
        match platform::move_noreplace(&dir.join(name), &folder.join(name)) {
            Ok(()) => out.moved += 1,
            Err(e) => out.failures.push((name.clone(), e.to_string())),
        }
    }
    Ok(out)
}

#[cfg(target_os = "linux")]
mod platform {
    use std::{ffi::CString, io, os::unix::ffi::OsStrExt, path::Path};
    pub fn move_noreplace(from: &Path, to: &Path) -> io::Result<()> {
        let a = CString::new(from.as_os_str().as_bytes()).map_err(io::Error::other)?;
        let b = CString::new(to.as_os_str().as_bytes()).map_err(io::Error::other)?;
        let r = unsafe {
            libc::syscall(
                libc::SYS_renameat2,
                libc::AT_FDCWD,
                a.as_ptr(),
                libc::AT_FDCWD,
                b.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        };
        if r != 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}
#[cfg(not(target_os = "linux"))]
mod platform {
    pub fn move_noreplace(_: &std::path::Path, _: &std::path::Path) -> std::io::Result<()> {
        Err(std::io::Error::other("pendiente en esta plataforma"))
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn templates_and_folder_with_selection() {
        let d = crate::persistence::test_dir("quick-ops");
        let _ = fs::remove_dir_all(&d);
        let (t, w) = (d.join("Templates"), d.join("work"));
        fs::create_dir_all(&t).unwrap();
        fs::create_dir_all(&w).unwrap();
        fs::write(t.join("Documento.txt"), b"hola").unwrap();
        fs::write(t.join(".hidden"), b"x").unwrap();
        fs::create_dir(t.join("sub")).unwrap();
        assert_eq!(templates(&t), vec![t.join("Documento.txt")]);
        let a = from_template(&t.join("Documento.txt"), &w).unwrap();
        let b = from_template(&t.join("Documento.txt"), &w).unwrap();
        assert_eq!(
            (a.file_name().unwrap(), b.file_name().unwrap()),
            (OsStr::new("Documento.txt"), OsStr::new("Documento (2).txt"))
        );
        assert_eq!(fs::read(&b).unwrap(), b"hola");
        fs::create_dir(w.join("Carpeta nueva")).unwrap();
        let r = folder_with_selection(&w, &["Documento.txt".into(), "missing".into()]).unwrap();
        assert_eq!(r.folder, w.join("Carpeta nueva (2)"));
        assert_eq!((r.moved, r.failures.len()), (1, 1));
        assert!(
            w.join("Carpeta nueva (2)/Documento.txt").exists() && !w.join("Documento.txt").exists()
        );
        assert_eq!(
            numbered(OsStr::new(".bashrc"), 2),
            OsString::from(".bashrc (2)")
        );
        let _ = fs::remove_dir_all(&d);
    }
}
