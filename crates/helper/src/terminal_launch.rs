//! User-requested external terminal, not a media decoder or an owned helper task.
use std::io;
#[cfg(target_os = "linux")]
use std::path::Path;
#[cfg(target_os = "linux")]
fn command(path: &Path, executable: &Path) -> std::process::Command {
    let mut arg = std::ffi::OsString::from("--dir=");
    arg.push(path);
    let mut command = std::process::Command::new(executable);
    command.arg(arg).arg("--").current_dir(path);
    command
}
#[cfg(target_os = "linux")]
fn launcher(paths: &std::ffi::OsStr) -> io::Result<std::path::PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    if paths.len() > 64 * 1024 || std::env::split_paths(paths).take(257).count() > 256 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Terminal: PATH fuera del límite",
        ));
    }
    // Resolve before chdir and ignore relative/empty entries: directory contents
    // must not become the launcher when opening a terminal in an untrusted folder.
    for base in std::env::split_paths(paths).filter(|p| p.is_absolute()) {
        let candidate = base.join("xdg-terminal-exec");
        if std::fs::metadata(&candidate)
            .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        {
            return Ok(candidate);
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "Terminal: xdg-terminal-exec no disponible en PATH absoluto",
    ))
}
pub fn run(encoded: &str) -> io::Result<()> {
    let path = files_core::terminal_directory::decode(encoded)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::process::CommandExt;
        let executable = launcher(&std::env::var_os("PATH").unwrap_or_default())?;
        Err(command(&path, &executable).exec())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = path;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Terminal: integración pendiente en este sistema",
        ))
    }
}
#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStringExt;
    #[test]
    fn decoded_directory_is_one_raw_option_and_cwd_without_shell() {
        let path =
            std::path::PathBuf::from(std::ffi::OsString::from_vec(b"/raw \xff '$()' --".to_vec()));
        let cmd = command(&path, Path::new("/usr/bin/xdg-terminal-exec"));
        let mut option = std::ffi::OsString::from("--dir=");
        option.push(&path);
        assert_eq!(
            cmd.get_args().map(|a| a.to_owned()).collect::<Vec<_>>(),
            [option, "--".into()]
        );
        assert_eq!(cmd.get_current_dir(), Some(path.as_path()));
        assert_eq!(cmd.get_program(), "/usr/bin/xdg-terminal-exec");
    }
    #[test]
    fn malformed_directory_never_executes() {
        for input in ["", "2f00", "72656c6174697665", "no hex"] {
            assert_eq!(run(input).unwrap_err().kind(), io::ErrorKind::InvalidInput);
        }
    }
    #[test]
    fn launcher_resolution_ignores_relative_entries_and_non_executable_files() {
        use std::{
            fs,
            os::unix::fs::PermissionsExt,
            sync::atomic::{AtomicU64, Ordering},
        };
        static NEXT: AtomicU64 = AtomicU64::new(0);
        struct Owned(std::path::PathBuf);
        impl Drop for Owned {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let root = Owned(
            files_core::theme::home()
                .join(".cache/files-test")
                .join(format!(
                    "terminal-lookup-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                )),
        );
        fs::create_dir_all(&root.0).unwrap();
        let executable = root.0.join("xdg-terminal-exec");
        fs::write(&executable, b"fixture").unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(launcher(std::ffi::OsStr::new(".:relative:")).is_err());
        let paths = std::env::join_paths([Path::new("."), root.0.as_path()]).unwrap();
        assert_eq!(launcher(&paths).unwrap(), executable);
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(launcher(&paths).is_err());
        assert_eq!(
            launcher(std::ffi::OsStr::new(&"x".repeat(65537)))
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
    }
}
