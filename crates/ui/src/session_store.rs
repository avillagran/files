//! Session writer ownership lives on the existing AuxIo thread. Advisory lock is
//! held across validation and publication; no claim against non-cooperating writers.
use crate::session_codec::{self, Session};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
pub struct Opened {
    pub session: Option<Session>,
    pub writable: bool,
    pub notice: Option<String>,
}
pub enum Published {
    Durable,
    Unsynced(String),
}
struct SessionLock {
    file: File,
    #[cfg(unix)]
    owner_pid: libc::pid_t,
}
#[cfg(unix)]
impl Drop for SessionLock {
    fn drop(&mut self) {
        use std::os::fd::AsRawFd;
        // A sibling thread's fork may retain the open description past close.
        // Only its creator releases advisory ownership; a child copy may not.
        if self.owner_pid == unsafe { libc::getpid() } {
            loop {
                if unsafe { libc::flock(self.file.as_raw_fd(), libc::LOCK_UN) } == 0 {
                    break;
                }
                if std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted {
                    break;
                }
            }
        }
    }
}
pub struct Store {
    path: PathBuf,
    lock_path: PathBuf,
    lock: Option<SessionLock>,
    blocked: bool,
}
impl Store {
    pub fn open(directory: &Path, restore: bool) -> (Self, Opened) {
        let path = directory.join("session.bin");
        let lock_path = directory.join("session.lock");
        let mut s = Self {
            path,
            lock_path,
            lock: None,
            blocked: false,
        };
        let lock = s.acquire();
        let mut notice = lock.err();
        let mut session = None;
        match read_session(&s.path) {
            Ok(Some(data)) => match session_codec::validate(&data) {
                Ok(()) => {
                    if restore {
                        match session_codec::decode(&data) {
                            Ok(doc) => session = Some(doc),
                            Err(e) => {
                                s.blocked = true;
                                notice = Some(e.into());
                            }
                        }
                    }
                }
                Err(e) => {
                    s.blocked = true;
                    notice = Some(format!("Sesión preservada sin modificar: {e}"));
                }
            },
            Ok(None) => {}
            Err(e) => {
                s.blocked = true;
                notice = Some(format!("No se pudo leer la sesión: {e}"));
            }
        }
        let writable = s.lock.is_some() && !s.blocked;
        (
            s,
            Opened {
                session,
                writable,
                notice,
            },
        )
    }
    #[cfg(unix)]
    fn acquire(&mut self) -> Result<(), String> {
        use std::os::{fd::AsRawFd, unix::fs::OpenOptionsExt};
        fs::create_dir_all(self.path.parent().unwrap()).map_err(|e| e.to_string())?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&self.lock_path)
            .map_err(|e| e.to_string())?;
        if !lock.metadata().map_err(|e| e.to_string())?.is_file() {
            return Err("Lock de sesión no es archivo regular".into());
        }
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err("Sesión sólo lectura: otra ventana tiene el guardado; no se tomará control automáticamente".into());
        }
        self.lock = Some(SessionLock {
            file: lock,
            owner_pid: unsafe { libc::getpid() },
        });
        Ok(())
    }
    #[cfg(not(unix))]
    fn acquire(&mut self) -> Result<(), String> {
        Err("Ownership de sesión pendiente en esta plataforma".into())
    }
    fn check_lock(&self) -> Result<(), String> {
        if self.blocked {
            return Err("Sesión incompatible/inválida preservada; guardado deshabilitado".into());
        }
        let lock = self
            .lock
            .as_ref()
            .ok_or("Sesión sólo lectura: sin ownership")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let a = lock.file.metadata().map_err(|e| e.to_string())?;
            let b = fs::symlink_metadata(&self.lock_path).map_err(|e| e.to_string())?;
            if !b.is_file() || a.dev() != b.dev() || a.ino() != b.ino() {
                return Err("Lock de sesión reemplazado; guardado bloqueado".into());
            }
        }
        Ok(())
    }
    pub fn save(&mut self, bytes: &[u8]) -> Result<Published, String> {
        self.save_with_sync(bytes, |parent| {
            #[cfg(unix)]
            {
                File::open(parent)?.sync_all()
            }
            #[cfg(not(unix))]
            {
                let _ = parent;
                Ok(())
            }
        })
    }
    fn save_with_sync(
        &mut self,
        bytes: &[u8],
        sync: impl FnOnce(&Path) -> std::io::Result<()>,
    ) -> Result<Published, String> {
        self.check_lock()?;
        session_codec::validate(bytes).map_err(str::to_owned)?;
        // Revalidate the effective file on EVERY attempt, including after rename
        // succeeded but directory sync failed. Never overwrite future/corrupt data.
        if let Some(old) = read_session(&self.path)?
            && let Err(e) = session_codec::validate(&old)
        {
            self.blocked = true;
            return Err(format!("Sesión externa incompatible preservada: {e}"));
        }
        let parent = self.path.parent().unwrap();
        let temp = parent.join(format!(
            "session.bin.tmp-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let result = (|| {
            let mut opts = OpenOptions::new();
            opts.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                opts.mode(0o600);
            }
            let mut file = opts.open(&temp).map_err(|e| e.to_string())?;
            file.write_all(bytes).map_err(|e| e.to_string())?;
            file.sync_all().map_err(|e| e.to_string())?;
            drop(file);
            self.check_lock()?;
            fs::rename(&temp, &self.path).map_err(|e| e.to_string())?;
            if let Err(e) = sync(parent) {
                return Ok(Published::Unsynced(format!(
                    "Sesión publicada; durabilidad de directorio no confirmada: {e}"
                )));
            }
            Ok(Published::Durable)
        })();
        if result.is_err() {
            let _ = fs::remove_file(temp);
        }
        result
    }
}
fn read_session(path: &Path) -> Result<Option<Vec<u8>>, String> {
    let mut opts = OpenOptions::new();
    opts.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC);
    }
    let mut file = match opts.open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    let meta = file.metadata().map_err(|e| e.to_string())?;
    if !meta.is_file() {
        return Err("Sesión no es archivo regular".into());
    }
    if meta.len() > session_codec::MAX_BYTES as u64 {
        return Err("Sesión supera4MiB".into());
    }
    let mut data = Vec::with_capacity(meta.len() as usize);
    let mut chunk = [0u8; 8192];
    loop {
        let n = file.read(&mut chunk).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        let needed = data
            .len()
            .checked_add(n)
            .filter(|n| *n <= session_codec::MAX_BYTES)
            .ok_or("Sesión supera4MiB")?;
        if needed > data.capacity() {
            data.try_reserve_exact(session_codec::MAX_BYTES - data.len())
                .map_err(|_| "No hay memoria para leer sesión")?;
        }
        data.extend_from_slice(&chunk[..n]);
    }
    Ok(Some(data))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn dir() -> PathBuf {
        let p = files_core::theme::home().join(format!(
            ".cache/files-test/session-store-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&p).unwrap();
        p
    }
    #[test]
    fn lock_is_lifetime_and_invalid_file_is_preserved() {
        let p = dir();
        let (first, a) = Store::open(&p, false);
        assert!(a.writable);
        let (second, b) = Store::open(&p, false);
        assert!(!b.writable && b.notice.is_some());
        drop(first);
        assert!(second.check_lock().is_err());
        drop(second);
        fs::write(p.join("session.bin"), b"future-invalid").unwrap();
        let (mut s, o) = Store::open(&p, true);
        assert!(!o.writable && o.notice.is_some());
        assert!(s.save(b"x").is_err());
        assert_eq!(fs::read(p.join("session.bin")).unwrap(), b"future-invalid");
        drop(s);
        fs::remove_dir_all(p).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn replaced_lock_is_never_trusted() {
        let p = dir();
        let (s, o) = Store::open(&p, false);
        assert!(o.writable);
        fs::rename(p.join("session.lock"), p.join("old.lock")).unwrap();
        fs::write(p.join("session.lock"), b"").unwrap();
        assert!(s.check_lock().is_err());
        drop(s);
        fs::remove_dir_all(p).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn symlink_and_fifo_are_rejected_without_reading() {
        use std::{
            ffi::CString,
            os::unix::{ffi::OsStrExt, fs::symlink},
        };
        let p = dir();
        let path = p.join("session.bin");
        let c = CString::new(path.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
        let (_s, o) = Store::open(&p, true);
        assert!(!o.writable && o.notice.is_some());
        drop(_s);
        fs::remove_file(&path).unwrap();
        fs::write(p.join("other"), b"keep").unwrap();
        symlink(p.join("other"), &path).unwrap();
        let (_s, o) = Store::open(&p, true);
        assert!(!o.writable && o.notice.is_some());
        assert_eq!(fs::read(p.join("other")).unwrap(), b"keep");
        drop(_s);
        fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn directory_sync_failure_reports_publication_and_retry_uses_effective_file() {
        use crate::{model::View, refresh_selection::Snapshot, session_codec::Record};
        use files_core::{listing::Sort, navigation_history::History};
        let p = dir();
        let (mut store, o) = Store::open(&p, false);
        assert!(o.writable);
        let history = History::default();
        let selected = Snapshot::capture(std::iter::empty(), None).unwrap();
        let record = Record {
            path: Path::new("/private-session-fixture"),
            history: &history,
            selection: &selected,
            view: View::List,
            sort: Sort::Name,
            descending: false,
            query: "",
            scroll: 0,
            grid_zoom: 1.,
            pan_x: 0.,
        };
        let bytes = session_codec::encode(&[record], &[], 0).unwrap();
        let outcome = store
            .save_with_sync(&bytes, |_| {
                Err(std::io::Error::other("injected directory sync"))
            })
            .unwrap();
        assert!(matches!(outcome, Published::Unsynced(_)));
        assert_eq!(fs::read(p.join("session.bin")).unwrap(), bytes);
        assert!(matches!(store.save(&bytes).unwrap(), Published::Durable));
        assert_eq!(fs::read(p.join("session.bin")).unwrap(), bytes);
        drop(store);
        fs::remove_dir_all(p).unwrap();
    }
}

#[cfg(all(test, target_os = "linux"))]
mod fork_tests {
    use super::*;
    fn lock(root: &Path) -> Result<SessionLock, String> {
        let (mut store, opened) = Store::open(root, false);
        store
            .lock
            .take()
            .ok_or_else(|| opened.notice.unwrap_or_else(|| "No session owner".into()))
    }
    // The child executes only inherited File drop and libc read/write/_exit; no
    // allocator, logging, test assertions or other post-fork runtime locks.
    fn fork_case(child_drops: bool) {
        let root = files_core::theme::home()
            .join(".cache/files-test")
            .join(format!(
                "session-owner-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        let guard = lock(&root).unwrap();
        let mut ready = [0; 2];
        let mut go = [0; 2];
        assert_eq!(
            unsafe { libc::pipe2(ready.as_mut_ptr(), libc::O_CLOEXEC) },
            0
        );
        assert_eq!(unsafe { libc::pipe2(go.as_mut_ptr(), libc::O_CLOEXEC) }, 0);
        let pid = unsafe { libc::fork() };
        assert!(pid >= 0);
        if pid == 0 {
            unsafe {
                libc::close(ready[0]);
                libc::close(go[1]);
            }
            if child_drops {
                drop(guard)
            }
            let byte = 1u8;
            unsafe {
                libc::write(ready[1], (&byte as *const u8).cast(), 1);
                let mut ack = 0u8;
                libc::read(go[0], (&mut ack as *mut u8).cast(), 1);
                libc::_exit(0)
            }
        }
        unsafe {
            libc::close(ready[1]);
            libc::close(go[0]);
        }
        let mut byte = 0u8;
        let ready_n = unsafe { libc::read(ready[0], (&mut byte as *mut u8).cast(), 1) };
        let attempt = if child_drops {
            let r = lock(&root);
            drop(guard);
            r
        } else {
            drop(guard);
            lock(&root)
        };
        let correct = if child_drops {
            matches!(&attempt,Err(e)if e.contains("otra ventana"))
        } else {
            attempt.is_ok()
        };
        let child_still_live = unsafe { libc::kill(pid, 0) } == 0;
        unsafe {
            libc::write(go[1], (&byte as *const u8).cast(), 1);
            libc::close(go[1]);
            libc::close(ready[0]);
        }
        let mut status = 0;
        let reaped = unsafe { libc::waitpid(pid, &mut status, 0) };
        drop(attempt);
        fs::remove_dir_all(&root).unwrap();
        assert_eq!(ready_n, 1);
        assert!(child_still_live);
        assert_eq!(reaped, pid);
        assert_eq!(status, 0);
        assert!(correct);
    }
    #[test]
    fn creator_unlocks_even_while_forked_child_retains_descriptor() {
        fork_case(false)
    }
    #[test]
    fn child_drop_never_unlocks_creators_live_owner() {
        fork_case(true)
    }
}
