//! Explicit control of a durably registered mount from a new owner process.
//! Never signals/waits the original SSHFS (not our Child), adopts a connection,
//! performs automatic remote reads, force/lazy unmounts, deletes or kills on Drop.
#![cfg(target_os = "linux")]
use crate::{
    sftp_connection::{self, Diagnostic},
    sftp_registry::{
        Phase as Saved, Process, ProcessStatus, Publication, Record, Recovery, Store,
        process_status,
    },
};
use std::{
    io,
    path::{Path, PathBuf},
    process::{Child, Command},
};
#[derive(Debug, PartialEq, Eq)]
pub enum Phase {
    Registered,
    Disconnecting,
    Retained,
    NoMount,
    Absent,
    Uncertain,
}
#[derive(Debug)]
pub enum Admission {
    Started,
    StartedUncertain(String),
    NotStarted(String),
}
#[derive(Debug)]
pub struct Snapshot {
    pub phase: Phase,
    pub point: PathBuf,
    pub writable: bool,
    pub pending_pid: Option<u32>,
    pub generation: u64,
    pub saved_phase: Saved,
    pub diagnostic: String,
}
pub struct Controller {
    store: Store,
    record: Record,
    program: PathBuf,
    pending: Option<(Child, Diagnostic)>,
    requested: bool,
    local_intent: bool,
    error: String,
}
fn gone(process: Process) -> io::Result<bool> {
    Ok(matches!(
        process_status(process)?,
        ProcessStatus::Missing | ProcessStatus::Reused
    ))
}
impl Controller {
    pub fn open(root: &Path, program: &Path) -> io::Result<Self> {
        sftp_connection::program(program)?;
        let (store, record) = Store::open(root)?;
        let record = record
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "No hay conexión registrada"))?;
        Ok(Self {
            store,
            record,
            program: program.into(),
            pending: None,
            requested: false,
            local_intent: false,
            error: String::new(),
        })
    }
    fn note(&mut self, value: impl std::fmt::Display) {
        self.error = value.to_string();
        if self.error.len() > 4096 {
            let mut n = 4096 - 24;
            while !self.error.is_char_boundary(n) {
                n -= 1;
            }
            self.error.truncate(n);
            self.error.push_str("\n[Informe truncado]");
        }
    }
    fn persist(&mut self, mut next: Record) -> io::Result<bool> {
        let publication = self.store.publish(&mut next)?;
        self.record = next;
        match publication {
            Publication::Durable => Ok(true),
            other => {
                self.note(format!("Registro pendiente: {other:?}"));
                Ok(false)
            }
        }
    }
    fn available(&self) -> io::Result<()> {
        if !self.store.writable() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Otra ventana controla el registro, o hay sync/identidad pendiente",
            ));
        }
        self.store.current(&self.record)?;
        if self.pending.is_some() {
            return Err(io::Error::other(
                "Desmontaje propio todavía pendiente de ACK",
            ));
        }
        if let Some(p) = self.record.unmount
            && !gone(p)?
        {
            return Err(io::Error::other(
                "Desmontaje anterior vivo o zombie; no duplicar",
            ));
        }
        if self.record.phase == Saved::Disconnecting
            && self.record.unmount.is_none()
            && !self.requested
            && !self.local_intent
        {
            return Err(io::Error::other(
                "Intento anterior sin PID/ACK; reconciliación manual necesaria",
            ));
        }
        Ok(())
    }
    pub fn disconnect(&mut self) -> io::Result<Admission> {
        self.available()?;
        self.store
            .action_mount(&self.record, &sftp_connection::table()?)?;
        let diagnostic = Diagnostic::new()?;
        let mut intent = self.record.clone();
        intent.phase = Saved::Disconnecting;
        intent.unmount = None;
        let durable = self.persist(intent)?;
        self.local_intent = true;
        if !durable {
            return Ok(Admission::NotStarted(self.error.clone()));
        }
        // Sync may have taken time: fresh record, parent and exact mount again.
        if let Err(error) = self
            .store
            .action_mount(&self.record, &sftp_connection::table()?)
        {
            let mut retained = self.record.clone();
            retained.phase = Saved::Uncertain;
            if let Err(e) = self.persist(retained) {
                self.note(e);
            }
            return Err(error);
        }
        match sftp_connection::spawn(
            Command::new(&self.program)
                .arg("-u")
                .arg(&self.record.point),
            &diagnostic,
        ) {
            Err(error) => {
                let mut retained = self.record.clone();
                retained.phase = Saved::Uncertain;
                if let Err(e) = self.persist(retained) {
                    self.note(e);
                }
                Err(error)
            }
            Ok(child) => {
                let pid = child.id();
                self.pending = Some((child, diagnostic));
                self.requested = true;
                self.error.clear();
                let result = (|| {
                    let identity = Process::capture(pid)?;
                    let mut next = self.record.clone();
                    next.unmount = Some(identity);
                    self.persist(next)
                })();
                match result {
                    Ok(true) => Ok(Admission::Started),
                    Ok(false) => Ok(Admission::StartedUncertain(self.error.clone())),
                    Err(e) => {
                        self.note(e);
                        Ok(Admission::StartedUncertain(self.error.clone()))
                    }
                }
            }
        }
    }
    /// Explicitly account for a previous recorded operation; never repeats it.
    /// An unregistered PID publication gap cannot be silently declared finished.
    /// Explicit read-only open request. May block on remote I/O off the UI.
    /// Does not adopt the original server or change the durable transaction.
    pub fn verified_path(&self) -> io::Result<PathBuf> {
        if self.pending.is_some() || self.record.phase == Saved::Disconnecting {
            return Err(io::Error::other("Desmontaje pendiente: no abrir"));
        }
        let rows = sftp_connection::table()?;
        let mount = self.store.action_mount(&self.record, &rows)?;
        if !matches!(
            self.store.observe(&self.record, &rows)?,
            Recovery::MatchingMountAndProcess
        ) {
            return Err(io::Error::other("Proceso/montaje no coincide: no abrir"));
        }
        sftp_connection::serving_root(&self.record.point, &mount)?;
        let rows = sftp_connection::table()?;
        if self.store.action_mount(&self.record, &rows)? != mount
            || !matches!(
                self.store.observe(&self.record, &rows)?,
                Recovery::MatchingMountAndProcess
            )
        {
            return Err(io::Error::other(
                "Registro/proceso/montaje cambió durante lectura",
            ));
        }
        Ok(self.record.point.clone())
    }
    pub fn reconcile(&mut self) -> io::Result<bool> {
        self.available()?;
        let rows = sftp_connection::table()?;
        let observation = self.store.observe(&self.record, &rows)?;
        match observation {
            Recovery::NoMount {
                server: None | Some(ProcessStatus::Missing | ProcessStatus::Reused),
            } => {
                if self.record.phase == Saved::Absent {
                    return Ok(true);
                }
                let mut next = self.record.clone();
                next.phase = Saved::Absent;
                next.mount = None;
                next.server = None;
                next.unmount = None;
                self.persist(next)
            }
            _ => {
                let mut next = self.record.clone();
                next.phase = Saved::Uncertain;
                next.unmount = None;
                self.persist(next)
            }
        }
    }
    pub fn retry_sync(&mut self) -> io::Result<()> {
        self.store.retry_sync()
    }
    /// May perform local metadata I/O off-UI; no blocking wait/join for a child.
    pub fn poll(&mut self) -> Snapshot {
        match self
            .pending
            .as_mut()
            .map(|(child, _)| child.try_wait())
            .transpose()
        {
            Ok(Some(Some(status))) => {
                let (_, diagnostic) = self.pending.take().expect("Owned child held");
                let mut next = self.record.clone();
                next.unmount = None;
                next.phase = Saved::Uncertain;
                if !status.success() {
                    self.note(format!(
                        "Desmontaje retenido ({status}): {}",
                        diagnostic.text()
                    ));
                }
                if let Err(e) = self.persist(next) {
                    self.note(e);
                }
            }
            Err(e) => self.note(e),
            _ => {}
        }
        if self.requested
            && self.pending.is_none()
            && self.store.writable()
            && self.record.phase == Saved::Absent
        {
            self.requested = false;
        }
        if self.requested && self.pending.is_none() && self.store.writable() {
            match sftp_connection::table().and_then(|rows| self.store.observe(&self.record, &rows))
            {
                Ok(Recovery::NoMount {
                    server: None | Some(ProcessStatus::Missing | ProcessStatus::Reused),
                }) => {
                    let mut next = self.record.clone();
                    next.phase = Saved::Absent;
                    next.server = None;
                    next.unmount = None;
                    next.mount = None;
                    match self.persist(next) {
                        Ok(true) => self.requested = false,
                        Ok(false) => {}
                        Err(e) => self.note(e),
                    }
                }
                Err(e) => self.note(e),
                _ => {}
            }
        }
        self.snapshot()
    }
    pub fn snapshot(&self) -> Snapshot {
        let observed = sftp_connection::table().and_then(|rows| {
            self.store.current(&self.record)?;
            self.store.observe(&self.record, &rows)
        });
        let mut diagnostic = self.error.clone();
        let phase = if self.pending.is_some() {
            Phase::Disconnecting
        } else if self.record.phase == Saved::Disconnecting
            && self.record.unmount.is_none()
            && !self.local_intent
            && !self.requested
        {
            diagnostic="Intento de desmontaje previo sin PID/ACK; no repetir ni declarar terminado por inferencia".into();
            Phase::Uncertain
        } else {
            match observed {
                Ok(Recovery::MatchingMountAndProcess) => {
                    if self.record.phase == Saved::Verified && self.error.is_empty() {
                        Phase::Registered
                    } else {
                        Phase::Retained
                    }
                }
                Ok(Recovery::NoMount {
                    server: None | Some(ProcessStatus::Missing | ProcessStatus::Reused),
                }) => {
                    if self.record.phase == Saved::Absent {
                        Phase::Absent
                    } else {
                        Phase::NoMount
                    }
                }
                Ok(Recovery::Uncertain(reason)) => {
                    diagnostic = format!("No se pudo confirmar: {reason}\n{}", self.error);
                    Phase::Uncertain
                }
                Err(e) => {
                    diagnostic = format!(
                        "No se pudo verificar el registro/montaje: {e}\n{}",
                        self.error
                    );
                    Phase::Uncertain
                }
                _ => {
                    diagnostic = format!(
                        "Montaje ausente pero proceso vivo o sin reap\n{}",
                        self.error
                    );
                    Phase::Uncertain
                }
            }
        };
        if diagnostic.len() > 4096 {
            let mut n = 4096 - 24;
            while !diagnostic.is_char_boundary(n) {
                n -= 1;
            }
            diagnostic.truncate(n);
            diagnostic.push_str("\n[Informe truncado]");
        }
        Snapshot {
            phase,
            point: self.record.point.clone(),
            writable: self.store.writable(),
            pending_pid: self.pending.as_ref().map(|(p, _)| p.id()),
            generation: self.record.generation(),
            saved_phase: self.record.phase,
            diagnostic,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{mount_table::Mount, sftp_mount_point::Parent, sftp_target::Target};
    use std::{
        fs,
        os::unix::fs::DirBuilderExt,
        process::Stdio,
        sync::atomic::{AtomicU64, Ordering},
    };
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let root = PathBuf::from(std::env::var_os("HOME").unwrap()).join(".cache/files-test");
            fs::create_dir_all(&root).unwrap();
            loop {
                let path = root.join(format!(
                    "sftp-recovered-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                match fs::DirBuilder::new().mode(0o700).create(&path) {
                    Ok(()) => return Self(path),
                    Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
                    Err(e) => panic!("{e}"),
                }
            }
        }
        fn record(&self) -> Record {
            Record::prepared(
                Target::parse("host", "", "2222", "/").unwrap(),
                &Parent::open(&self.0).unwrap().create().unwrap(),
            )
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    fn mounted(record: &mut Record) {
        record.mount = Some(Mount {
            id: 999,
            parent: 1,
            device: (0, 2),
            root: "/".into(),
            point: record.point.clone(),
            filesystem: b"fuse.sshfs".to_vec(),
            source: record.point.file_name().unwrap().into(),
        });
    }
    #[test]
    fn explicit_open_rejects_missing_mount_without_mutating_registration() {
        let f = Fixture::new();
        let (mut store, _) = Store::open(&f.0).unwrap();
        let mut record = f.record();
        store.publish(&mut record).unwrap();
        drop(store);
        let before = fs::read(f.0.join("connection.bin")).unwrap();
        let c = Controller::open(&f.0, Path::new("/nonexistent-fusermount-test")).unwrap();
        assert!(c.verified_path().is_err());
        assert_eq!(fs::read(f.0.join("connection.bin")).unwrap(), before);
        assert!(c.pending.is_none());
    }
    #[test]
    fn readonly_reader_never_dispatches_and_never_upgrades_after_owner_drop() {
        let f = Fixture::new();
        let (mut owner, _) = Store::open(&f.0).unwrap();
        let mut record = f.record();
        owner.publish(&mut record).unwrap();
        let bytes = fs::read(f.0.join("connection.bin")).unwrap();
        let mut c = Controller::open(&f.0, Path::new("/nonexistent-fusermount-test")).unwrap();
        assert!(!c.snapshot().writable);
        assert!(c.disconnect().is_err());
        drop(owner);
        assert!(c.disconnect().is_err());
        assert!(c.pending.is_none());
        assert_eq!(fs::read(f.0.join("connection.bin")).unwrap(), bytes);
    }
    #[test]
    fn stale_or_foreign_namespace_blocks_before_spawning() {
        let f = Fixture::new();
        let (mut store, _) = Store::open(&f.0).unwrap();
        let mut record = f.record();
        store.publish(&mut record).unwrap();
        record.phase = Saved::Uncertain;
        mounted(&mut record);
        store.publish(&mut record).unwrap();
        drop(store);
        let mut c = Controller::open(&f.0, Path::new("/nonexistent-fusermount-test")).unwrap();
        let bytes = fs::read(f.0.join("connection.bin")).unwrap();
        assert!(c.disconnect().is_err());
        assert!(c.pending.is_none());
        assert_eq!(fs::read(f.0.join("connection.bin")).unwrap(), bytes);
        fs::write(f.0.join("connection.bin"), b"external future data").unwrap();
        assert!(c.disconnect().is_err());
        assert!(c.pending.is_none());
        assert_eq!(
            fs::read(f.0.join("connection.bin")).unwrap(),
            b"external future data"
        );
    }
    #[test]
    fn publication_gap_without_registered_pid_cannot_repeat_previous_unmount() {
        let f = Fixture::new();
        let (mut store, _) = Store::open(&f.0).unwrap();
        let mut record = f.record();
        store.publish(&mut record).unwrap();
        record.phase = Saved::Disconnecting;
        mounted(&mut record);
        store.publish(&mut record).unwrap();
        drop(store);
        let mut c = Controller::open(&f.0, Path::new("/nonexistent-fusermount-test")).unwrap();
        assert!(
            c.disconnect()
                .unwrap_err()
                .to_string()
                .contains("sin PID/ACK")
        );
        assert!(c.reconcile().is_err());
        assert_eq!(c.snapshot().phase, Phase::Uncertain);
        assert!(c.pending.is_none());
    }
    #[test]
    fn observation_does_not_clear_but_explicit_reconcile_closes_after_reap() {
        let f = Fixture::new();
        let (mut store, _) = Store::open(&f.0).unwrap();
        let mut record = f.record();
        store.publish(&mut record).unwrap();
        let mut child = Command::new("/usr/bin/cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        record.server = Some(Process::capture(child.id()).unwrap());
        record.phase = Saved::Verified;
        mounted(&mut record);
        store.publish(&mut record).unwrap();
        drop(store);
        drop(child.stdin.take());
        child.wait().unwrap();
        let mut c = Controller::open(&f.0, Path::new("/nonexistent-fusermount-test")).unwrap();
        let before = fs::read(f.0.join("connection.bin")).unwrap();
        assert_eq!(c.poll().phase, Phase::NoMount);
        assert_eq!(fs::read(f.0.join("connection.bin")).unwrap(), before);
        assert!(c.reconcile().unwrap());
        assert_eq!(c.snapshot().phase, Phase::Absent);
        let generation = c.snapshot().generation;
        assert!(c.reconcile().unwrap());
        assert_eq!(c.snapshot().generation, generation);
        assert!(c.pending.is_none());
    }
    #[test]
    fn live_previous_unmount_is_not_repeated() {
        let f = Fixture::new();
        let (mut store, _) = Store::open(&f.0).unwrap();
        let mut record = f.record();
        store.publish(&mut record).unwrap();
        record.phase = Saved::Disconnecting;
        mounted(&mut record);
        record.unmount = Some(Process::capture(std::process::id()).unwrap());
        store.publish(&mut record).unwrap();
        drop(store);
        let mut c = Controller::open(&f.0, Path::new("/nonexistent-fusermount-test")).unwrap();
        assert!(
            c.disconnect()
                .unwrap_err()
                .to_string()
                .contains("no duplicar")
        );
        assert!(c.reconcile().is_err());
        assert!(c.pending.is_none());
    }
}
