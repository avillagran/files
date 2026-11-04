//! Off-UI Linux SSHFS process engine. No thread, daemon, automatic unmount,
//! recursive cleanup or killing on Drop. Product caller must durably register
//! the mount/process before admitting this engine and reconcile on restart.
#![cfg(target_os = "linux")]
use crate::{
    mount_table::{self, Mount},
    sftp_lifecycle::{Controller, Intent, Phase, Ticket},
    sftp_mount_point::Point,
    sftp_target::Target,
};
use std::{
    ffi::{OsStr, OsString},
    fs::{File, OpenOptions},
    io::{self, Read},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{
            fs::{FileExt, OpenOptionsExt},
            process::CommandExt,
        },
    },
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
const ERROR_CAP: usize = 8192;
pub(crate) struct Diagnostic(File);
impl Diagnostic {
    pub(crate) fn new() -> io::Result<Self> {
        let fd = unsafe {
            libc::memfd_create(
                c"files-sftp-stderr".as_ptr(),
                libc::MFD_CLOEXEC | libc::MFD_ALLOW_SEALING,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let file = unsafe { File::from_raw_fd(fd) };
        if unsafe { libc::ftruncate(file.as_raw_fd(), ERROR_CAP as libc::off_t) } != 0
            || unsafe {
                libc::fcntl(
                    fd,
                    libc::F_ADD_SEALS,
                    libc::F_SEAL_GROW | libc::F_SEAL_SHRINK | libc::F_SEAL_SEAL,
                )
            } < 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(file))
    }
    fn stdio(&self) -> io::Result<Stdio> {
        Ok(Stdio::from(self.0.try_clone()?))
    }
    pub(crate) fn text(&self) -> String {
        let cursor = unsafe { libc::lseek(self.0.as_raw_fd(), 0, libc::SEEK_CUR) };
        if cursor < 0 {
            return "No se pudo leer la posición de stderr acotado".into();
        }
        let end = (cursor as usize).min(ERROR_CAP);
        let mut truncated = cursor as usize >= ERROR_CAP;
        const MARK: &str = "\n[Diagnóstico truncado: límite de captura/visualización]";
        let mut bytes = [0u8; ERROR_CAP];
        let mut n = 0;
        while n < end {
            match self.0.read_at(&mut bytes[n..end], n as u64) {
                Ok(0) => break,
                Ok(k) => n += k,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => return "No se pudo leer stderr acotado".into(),
            }
        }
        let decoded = String::from_utf8_lossy(&bytes[..n]);
        let mut out = String::new();
        for c in decoded
            .chars()
            .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
        {
            if out.len() + c.len_utf8() > 4096 - MARK.len() {
                truncated = true;
                break;
            }
            out.push(c);
        }
        if truncated {
            out.push_str(MARK);
        }
        out
    }
}
pub(crate) fn program(path: &Path) -> io::Result<()> {
    if !path.is_absolute()
        || path.as_os_str().len() > 4096
        || path.as_os_str().as_encoded_bytes().contains(&0)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Herramienta absoluta inválida",
        ));
    }
    Ok(())
}
pub(crate) fn table() -> io::Result<Vec<Mount>> {
    let mut bytes = Vec::new();
    File::open("/proc/self/mountinfo")?
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    mount_table::parse(&bytes).map_err(io::Error::other)
}
fn exact(table: Vec<Mount>, point: &Path, marker: &OsStr) -> io::Result<Option<Mount>> {
    if !table.iter().any(|m| m.point == point) {
        return Ok(None);
    }
    mount_table::unique_sshfs(&table, point, marker).map_err(io::Error::other)?;
    Ok(table.into_iter().find(|m| m.point == point))
}
pub(crate) fn serving_root(point: &Path, mount: &Mount) -> io::Result<()> {
    let directory = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(point)?;
    let mut proof = std::mem::MaybeUninit::<libc::statx>::zeroed();
    if unsafe {
        libc::statx(
            directory.as_raw_fd(),
            c"".as_ptr(),
            libc::AT_EMPTY_PATH | libc::AT_SYMLINK_NOFOLLOW,
            libc::STATX_BASIC_STATS | libc::STATX_MNT_ID,
            proof.as_mut_ptr(),
        )
    } != 0
    {
        return Err(io::Error::last_os_error());
    }
    let proof = unsafe { proof.assume_init() };
    if proof.stx_mask & libc::STATX_MNT_ID == 0
        || proof.stx_mnt_id != mount.id
        || (proof.stx_dev_major, proof.stx_dev_minor) != mount.device
    {
        return Err(io::Error::other(
            "Descriptor remoto no corresponde al montaje esperado",
        ));
    }
    let mut buffer = [0u8; 4096];
    // Fresh mount, first bounded READDIR. No filenames retained or exposed.
    // This may block off-UI during filesystem/network I/O; not a deadline proof.
    let n = unsafe {
        libc::syscall(
            libc::SYS_getdents64,
            directory.as_raw_fd(),
            buffer.as_mut_ptr(),
            buffer.len(),
        )
    };
    if n < 0 {
        return Err(io::Error::last_os_error());
    }
    if n as usize > buffer.len() {
        return Err(io::Error::other("Respuesta de directorio inválida"));
    }
    let after = exact(table()?, point, &mount.source)?;
    if after.as_ref() != Some(mount) {
        return Err(io::Error::other(
            "Montaje cambió durante la comprobación remota",
        ));
    }
    Ok(())
}
pub(crate) fn spawn(command: &mut Command, diagnostic: &Diagnostic) -> io::Result<Child> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(diagnostic.stdio()?);
    // Persistent OS filesystem, deliberately unlike transient preview helpers:
    // no PDEATHSIG and no controlling-terminal lifetime inherited from Files.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    command.spawn()
}
#[derive(Debug)]
pub struct Snapshot {
    pub phase: Phase,
    pub point: PathBuf,
    pub diagnostic: String,
    pub server_pid: Option<u32>,
    pub unmount_pid: Option<u32>,
}
pub struct Connection {
    control: Controller,
    ticket: Ticket,
    point: Point,
    marker: OsString,
    server: Option<Child>,
    stderr: Diagnostic,
    unmount: Option<(Child, Diagnostic)>,
    unmount_program: PathBuf,
    started: Instant,
    cancelled: bool,
    verified: bool,
    root_probe_failed: bool,
    last_mount_present: Option<bool>,
    error: String,
}
impl Connection {
    pub fn start(target: Target, point: Point, sshfs: &Path, unmount: &Path) -> io::Result<Self> {
        Self::start_inner(target, point, sshfs, unmount, None)
    }
    /// Debug laboratory only; never compiled into the release connection API.
    #[cfg(debug_assertions)]
    pub fn start_lab(
        target: Target,
        point: Point,
        sshfs: &Path,
        unmount: &Path,
        config: &Path,
    ) -> io::Result<Self> {
        program(config)?;
        Self::start_inner(target, point, sshfs, unmount, Some(config))
    }
    fn start_inner(
        target: Target,
        mut point: Point,
        sshfs: &Path,
        unmount: &Path,
        config: Option<&Path>,
    ) -> io::Result<Self> {
        program(sshfs)?;
        program(unmount)?;
        point.verify_unmounted_empty(&table()?)?;
        let marker = point
            .path()
            .file_name()
            .ok_or_else(|| io::Error::other("Nombre de montaje ausente"))?
            .to_os_string();
        let raw = marker.as_encoded_bytes();
        if !raw.starts_with(b"files-sftp-")
            || raw.len() != 43
            || !raw[11..].iter().all(u8::is_ascii_hexdigit)
        {
            return Err(io::Error::other("Marcador no generado por Point"));
        }
        let mut args = target
            .foreground_args(point.path())
            .map_err(io::Error::other)?;
        args.push("-o".into());
        args.push(format!("fsname={}", marker.to_string_lossy()).into());
        let mut control = Controller::default();
        let ticket = control
            .begin(Intent {
                target,
                point: point.path().to_owned(),
                source_marker: marker.clone(),
            })
            .map_err(io::Error::other)?;
        let stderr = Diagnostic::new()?;
        let mut command = Command::new(sshfs);
        if let Some(config) = config {
            command.arg("-F").arg(config);
        }
        command.args(args);
        control.admit_spawn(ticket).map_err(io::Error::other)?;
        // Failure leaves only the already-created empty Point; its Drop never deletes.
        let server = spawn(&mut command, &stderr)?;
        Ok(Self {
            control,
            ticket,
            point,
            marker,
            server: Some(server),
            stderr,
            unmount: None,
            unmount_program: unmount.into(),
            started: Instant::now(),
            cancelled: false,
            verified: false,
            root_probe_failed: false,
            last_mount_present: Some(false),
            error: String::new(),
        })
    }
    fn remember_error(&mut self, error: impl std::fmt::Display) {
        self.error = error.to_string();
        if self.error.len() > 4096 {
            const MARK: &str = "\n[Informe truncado]";
            let mut limit = 4096 - MARK.len();
            while !self.error.is_char_boundary(limit) {
                limit -= 1;
            }
            self.error.truncate(limit);
            self.error.push_str(MARK);
        }
    }
    pub fn poll(&mut self) -> Snapshot {
        let server_exit = self.server.as_mut().map(Child::try_wait).transpose();
        match server_exit {
            Ok(Some(Some(status))) => {
                self.server = None;
                let _ = self.control.child_finished(self.ticket);
                if !status.success() {
                    self.remember_error(format!("SSHFS terminó {status}: {}", self.stderr.text()));
                }
            }
            Err(error) => {
                let _ = self.control.observation_failed(self.ticket);
                self.remember_error(error);
                self.last_mount_present = None;
                return self.snapshot();
            }
            _ => {}
        }
        let unmount_exit = self.unmount.as_mut().map(|(c, _)| c.try_wait()).transpose();
        match unmount_exit {
            Ok(Some(Some(status))) => {
                let (_, diagnostic) = self.unmount.take().expect("Unmount active");
                let _ = self.control.unmount_finished(self.ticket, status.success());
                if !status.success() {
                    self.remember_error(format!(
                        "No se desmontó ({status}): {}",
                        diagnostic.text()
                    ));
                }
            }
            Err(error) => {
                let _ = self.control.observation_failed(self.ticket);
                self.remember_error(error);
                self.last_mount_present = None;
                return self.snapshot();
            }
            _ => {}
        }
        let observed = table().and_then(|v| exact(v, self.point.path(), &self.marker));
        match observed {
            Ok(mount) => {
                let present = mount.is_some();
                self.last_mount_present = Some(present);
                let accepted = match self.control.observe(self.ticket, mount) {
                    Ok(()) => true,
                    Err(error) => {
                        self.last_mount_present = None;
                        self.remember_error(error);
                        false
                    }
                };
                if accepted && present {
                    let proof = if !self.verified && !self.root_probe_failed {
                        self.point.verify_parent().and_then(|()| {
                            serving_root(
                                self.point.path(),
                                self.control
                                    .current()
                                    .and_then(|t| t.mounted())
                                    .expect("Accepted mount"),
                            )
                        })
                    } else {
                        Ok(())
                    };
                    if let Err(error) = proof {
                        self.root_probe_failed = true;
                        self.remember_error(format!(
                            "Montaje sin respuesta remota verificada: {error}; {}",
                            self.stderr.text()
                        ));
                    }
                    if self.root_probe_failed {
                        let _ = self.control.observation_failed(self.ticket);
                    } else {
                        self.verified = true;
                    }
                }
                if self.phase() == Phase::Absent {
                    match table().and_then(|v| self.point.verify_unmounted_empty(&v)) {
                        Ok(()) => {}
                        Err(e) => {
                            let _ = self.control.observation_failed(self.ticket);
                            self.remember_error(e);
                        }
                    }
                }
            }
            Err(error) => {
                self.last_mount_present = None;
                let _ = self.control.observation_failed(self.ticket);
                self.remember_error(error);
            }
        }
        if !self.cancelled
            && !self.verified
            && self.server.is_some()
            && self.last_mount_present == Some(false)
            && self.started.elapsed() >= Duration::from_secs(15)
        {
            self.cancel();
            self.remember_error("Conexión sin montaje verificado tras 15s; cancelación solicitada, estado pendiente de reconciliar");
        }
        // The first remote proof can outlive the foreground process. Reap again
        // before publishing Ready; a known-ended server is retained, not serving.
        if self.verified {
            match self.server.as_mut().map(Child::try_wait).transpose() {
                Ok(Some(Some(status))) => {
                    self.server = None;
                    let _ = self.control.child_finished(self.ticket);
                    let _ = self.control.cancel(self.ticket);
                    if !status.success() {
                        self.remember_error(format!(
                            "SSHFS terminó {status}: {}",
                            self.stderr.text()
                        ));
                    }
                }
                Err(e) => {
                    let _ = self.control.observation_failed(self.ticket);
                    self.last_mount_present = None;
                    self.remember_error(e);
                }
                _ => {}
            }
        }
        self.snapshot()
    }
    pub fn observed_mount(&self) -> Option<&Mount> {
        self.control.current().and_then(|t| t.mounted())
    }
    pub fn phase(&self) -> Phase {
        self.control.current().expect("Transaction held").phase()
    }
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            phase: self.phase(),
            point: self.point.path().to_owned(),
            diagnostic: self.error.clone(),
            server_pid: self.server.as_ref().map(Child::id),
            unmount_pid: self.unmount.as_ref().map(|(c, _)| c.id()),
        }
    }
    /// Does not silently unmount a verified filesystem. Late mount is retained.
    pub fn cancel(&mut self) {
        self.cancelled = true;
        let _ = self.control.cancel(self.ticket);
        if self.last_mount_present == Some(false)
            && let Some(server) = &mut self.server
        {
            // Child is unreaped, so its PID cannot be reused. Do not signal
            // descendants or any process discovered only by a name.
            if unsafe { libc::kill(server.id() as libc::pid_t, libc::SIGTERM) } != 0 {
                self.remember_error(io::Error::last_os_error());
            }
        }
    }
    pub fn disconnect(&mut self) -> io::Result<()> {
        if self.unmount.is_some() {
            return Err(io::Error::other("Desmontaje ya activo"));
        }
        self.point.verify_parent()?;
        let fresh = exact(table()?, self.point.path(), &self.marker)?
            .ok_or_else(|| io::Error::other("Montaje ausente; reconciliar antes de desconectar"))?;
        let diagnostic = Diagnostic::new()?;
        // An exact fresh namespace observation can authorize explicit ordinary
        // disconnect of a retained, non-serving FUSE mount. No remote read needed.
        self.control.cancel(self.ticket).map_err(io::Error::other)?;
        self.control
            .observe(self.ticket, Some(fresh.clone()))
            .map_err(io::Error::other)?;
        self.control
            .admit_disconnect(self.ticket, &fresh)
            .map_err(io::Error::other)?;
        let result = spawn(
            Command::new(&self.unmount_program)
                .arg("-u")
                .arg(self.point.path()),
            &diagnostic,
        );
        match result {
            Ok(child) => {
                self.error.clear();
                self.unmount = Some((child, diagnostic));
                Ok(())
            }
            Err(e) => {
                let _ = self.control.unmount_finished(self.ticket, false);
                Err(e)
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    #[test]
    fn diagnostic_is_sealed_bounded_and_writer_survives_reader_drop() {
        let diagnostic = Diagnostic::new().unwrap();
        let mut writer = diagnostic.0.try_clone().unwrap();
        writer.write_all(b"bounded error\n").unwrap();
        assert_eq!(diagnostic.text(), "bounded error\n");
        let seals = unsafe { libc::fcntl(writer.as_raw_fd(), libc::F_GET_SEALS) };
        assert_eq!(
            seals & (libc::F_SEAL_GROW | libc::F_SEAL_SHRINK),
            libc::F_SEAL_GROW | libc::F_SEAL_SHRINK
        );
        let observer = Diagnostic(writer.try_clone().unwrap());
        drop(diagnostic);
        writer.write_all(b"still writable").unwrap();
        let used = unsafe { libc::lseek(writer.as_raw_fd(), 0, libc::SEEK_CUR) };
        assert!((0..ERROR_CAP as i64).contains(&used));
        writer
            .write_all(&[b'x'; ERROR_CAP][..ERROR_CAP - used as usize])
            .unwrap();
        assert!(writer.write_all(b"overflow").is_err());
        assert_eq!(writer.metadata().unwrap().len(), ERROR_CAP as u64);
        assert!(observer.text().contains("truncado"));
        assert!(observer.text().len() <= 4096);
    }
    #[test]
    fn mount_observation_rejects_foreign_marker_instead_of_reporting_absence() {
        let point = Path::new("/owned/mount");
        let rows =
            mount_table::parse(b"5 1 0:2 / /owned/mount rw - fuse.sshfs foreign rw\n").unwrap();
        assert!(exact(rows, point, OsStr::new("expected")).is_err());
    }
    #[test]
    fn failed_exec_retains_the_created_point_without_network_or_cleanup() {
        use std::os::unix::fs::DirBuilderExt;
        let root =
            std::path::PathBuf::from(std::env::var_os("HOME").unwrap()).join(".cache/files-test");
        std::fs::create_dir_all(&root).unwrap();
        let base = root.join(format!("sftp-failed-exec-{}", std::process::id()));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&base)
            .unwrap();
        let point = crate::sftp_mount_point::Parent::open(&base)
            .unwrap()
            .create()
            .unwrap();
        let path = point.path().to_owned();
        let result = Connection::start(
            Target::parse("unused-host", "", "", "").unwrap(),
            point,
            &base.join("absent-sshfs"),
            Path::new("/usr/bin/fusermount3"),
        );
        assert!(result.is_err());
        assert!(path.is_dir());
        assert!(std::fs::read_dir(&path).unwrap().next().is_none());
        std::fs::remove_dir_all(base).unwrap();
    }
    #[test]
    fn remote_root_proof_rejects_an_fd_from_a_different_mount_before_readdir() {
        let expected = Mount {
            id: 0,
            parent: 1,
            device: (0, 0),
            root: "/".into(),
            point: "/".into(),
            filesystem: b"fuse.sshfs".to_vec(),
            source: "fixture".into(),
        };
        assert!(
            serving_root(Path::new("/"), &expected)
                .unwrap_err()
                .to_string()
                .contains("Descriptor remoto")
        );
    }
}
