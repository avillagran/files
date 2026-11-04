//! "Create link" (Nautilus Shift+Ctrl+M): a symbolic link to each target,
//! created in a folder under a free name ("Enlace a X", "Enlace a X (2)", …).
//! The link stores the target's absolute path; nothing is ever replaced.
use std::{
    ffi::OsString,
    io,
    path::{Path, PathBuf},
};

const MAX_SUFFIX: u32 = 999;

fn candidate(base: &OsString, n: u32) -> OsString {
    if n == 1 {
        return base.clone();
    }
    let mut name = base.clone();
    name.push(format!(" ({n})"));
    name
}

/// Creates one link per target inside `dir`; each result is the new link
/// path or why it failed.
pub fn create(dir: &Path, targets: &[PathBuf]) -> Vec<Result<PathBuf, String>> {
    targets
        .iter()
        .map(|t| create_one(dir, t).map_err(|e| e.to_string()))
        .collect()
}

fn create_one(dir: &Path, target: &Path) -> io::Result<PathBuf> {
    if !target.is_absolute() || !dir.is_absolute() {
        return Err(io::Error::other("ruta no absoluta"));
    }
    let name = target
        .file_name()
        .ok_or_else(|| io::Error::other("el destino no tiene nombre"))?;
    let mut base = OsString::from("Enlace a ");
    base.push(name);
    if base.len() > 240 {
        return Err(io::Error::other("nombre demasiado largo para el enlace"));
    }
    for n in 1..=MAX_SUFFIX {
        let link = dir.join(candidate(&base, n));
        match platform::symlink(target, &link) {
            Ok(()) => return Ok(link),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::other("no hay un nombre libre para el enlace"))
}

#[cfg(unix)]
mod platform {
    pub fn symlink(target: &std::path::Path, link: &std::path::Path) -> std::io::Result<()> {
        // symlink(2) never replaces an existing entry (EEXIST).
        std::os::unix::fs::symlink(target, link)
    }
}
#[cfg(not(unix))]
mod platform {
    pub fn symlink(_: &std::path::Path, _: &std::path::Path) -> std::io::Result<()> {
        Err(std::io::Error::other(
            "Enlaces pendientes en esta plataforma",
        ))
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn links_get_free_names_and_point_to_the_target() {
        let d = crate::persistence::test_dir("links-create");
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("a.txt"), b"x").unwrap();
        fs::write(d.join("Enlace a a.txt"), b"occupied").unwrap();
        let out = create(
            &d,
            &[d.join("a.txt"), d.join("a.txt"), PathBuf::from("rel")],
        );
        assert_eq!(out[0].as_ref().unwrap(), &d.join("Enlace a a.txt (2)"));
        assert_eq!(out[1].as_ref().unwrap(), &d.join("Enlace a a.txt (3)"));
        assert!(out[2].is_err());
        assert_eq!(
            fs::read_link(d.join("Enlace a a.txt (2)")).unwrap(),
            d.join("a.txt")
        );
        assert_eq!(fs::read(d.join("Enlace a a.txt")).unwrap(), b"occupied");
        let _ = fs::remove_dir_all(&d);
    }
}
