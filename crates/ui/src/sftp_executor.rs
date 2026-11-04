//! One lazy transport worker. It never blocks preferences or the graphical thread.
//! No Drop kill: a registered OS mount intentionally survives the Files window.
use crate::sftp_state::Capability;
use files_core::sftp_target::Target;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    time::{Duration, Instant},
};
#[derive(Debug)]
pub enum Command {
    Inspect(Capability),
    Connect(Target, Capability),
    Poll,
    Open,
    Disconnect,
    Reconcile,
    Sync,
}
impl Command {
    fn mutating(&self) -> bool {
        !matches!(self, Self::Inspect(_) | Self::Poll | Self::Open)
    }
}
#[derive(Clone, Debug, Default)]
pub struct Status {
    pub phase: String,
    pub point: Option<PathBuf>,
    pub diagnostic: String,
    pub open: bool,
    pub writable: bool,
}
impl Status {
    pub fn label(&self) -> &str {
        match self.phase.as_str() {
            "Ready" => "Conectado · servicio verificado",
            "Absent" | "Recuperada · Absent" => "Desconectado · registro actualizado",
            "Mounting" | "Verifying" => "Conectando · comprobando servicio remoto",
            "Disconnecting" | "Recuperada · Disconnecting" => {
                "Desconectando · esperando resultado"
            }
            "Retained" | "Recuperada · Retained" => "Montaje conservado · revisar informe",
            "Uncertain" | "Recuperada · Uncertain" => "Estado incierto · registro conservado",
            "Recuperada · Registered" | "Registrado · Verified" => {
                "Conexión guardada · abrir requiere comprobación"
            }
            "Registrado · Absent" => "Conexión anterior desconectada",
            "Recuperada · NoMount" => "Montaje ausente · reconciliar registro",
            other => other,
        }
    }
}
fn bounded_error(error: &str) -> String {
    const CAP: usize = 4096;
    const MARK: &str = " [Informe truncado]";
    if error.len() <= CAP {
        return error.into();
    }
    let mut n = CAP - MARK.len();
    while !error.is_char_boundary(n) {
        n -= 1;
    }
    let mut value = error[..n].to_string();
    value.push_str(MARK);
    value
}
#[derive(Default)]
struct Gate {
    closing: AtomicBool,
    epoch: AtomicU64,
}
impl Gate {
    fn accepts(&self, epoch: u64) -> bool {
        !self.closing.load(Ordering::SeqCst) && self.epoch.load(Ordering::SeqCst) == epoch
    }
    fn stop(&self) {
        if !self.closing.swap(true, Ordering::SeqCst) {
            let mut old = self.epoch.load(Ordering::SeqCst);
            loop {
                let next = old.checked_add(1).expect("SFTP admission epochs exhausted");
                match self.epoch.compare_exchange_weak(
                    old,
                    next,
                    Ordering::SeqCst,
                    Ordering::SeqCst,
                ) {
                    Ok(_) => break,
                    Err(value) => old = value,
                }
            }
        }
    }
    fn resume(&self) {
        self.closing.store(false, Ordering::SeqCst);
    }
}
struct Worker {
    tx: SyncSender<(u64, Command)>,
    rx: Receiver<Status>,
}
#[derive(Default)]
pub struct Client {
    worker: Option<Worker>,
    gate: Arc<Gate>,
    pending: Option<bool>,
    pub status: Status,
    last: Option<Instant>,
}
impl Client {
    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }
    pub fn send(&mut self, command: Command) -> Result<(), String> {
        if self.gate.closing.load(Ordering::SeqCst) || self.busy() {
            return Err("Acción SFTP en curso o cierre pendiente".into());
        }
        if self.worker.is_none() {
            let (tx, input) = mpsc::sync_channel(1);
            let (output, rx) = mpsc::sync_channel(1);
            let gate = self.gate.clone();
            std::thread::Builder::new()
                .name("files-sftp".into())
                .spawn(move || {
                    let mut engine = Engine::default();
                    while let Ok((epoch, command)) = input.recv() {
                        let result = if !gate.accepts(epoch) {
                            engine.status("Cierre: no se admiten nuevas acciones")
                        } else {
                            engine.run(command)
                        };
                        if output.send(result).is_err() {
                            break;
                        }
                    }
                })
                .map_err(|e| e.to_string())?;
            self.worker = Some(Worker { tx, rx });
        }
        let mutating = command.mutating();
        self.worker
            .as_ref()
            .unwrap()
            .tx
            .try_send((self.gate.epoch.load(Ordering::SeqCst), command))
            .map_err(|e| e.to_string())?;
        self.pending = Some(mutating);
        Ok(())
    }
    pub fn pump(&mut self) -> bool {
        let Some(w) = &self.worker else {
            return false;
        };
        match w.rx.try_recv() {
            Ok(value) => {
                self.status = value;
                self.pending = None;
                self.last = Some(Instant::now());
                true
            }
            Err(mpsc::TryRecvError::Disconnected) if self.pending.is_some() => {
                self.pending = None;
                self.status.open = false;
                self.status.diagnostic = "Trabajador SFTP desconectado; registro conservado".into();
                true
            }
            _ => false,
        }
    }
    pub fn tick(&mut self) {
        if !self.busy()
            && self.status.point.is_some()
            && self
                .last
                .is_some_and(|t| t.elapsed() >= Duration::from_millis(500))
        {
            let _ = self.send(Command::Poll);
        }
    }
    pub fn resume(&mut self) {
        self.gate.resume();
    }
    pub fn close_ready(&mut self) -> bool {
        self.gate.stop();
        self.pending != Some(true)
    }
}
#[cfg(target_os = "linux")]
#[derive(Default)]
struct Engine {
    fresh: Option<Fresh>,
    recovered: Option<files_core::sftp_recovered::Controller>,
    status: Status,
}
#[cfg(target_os = "linux")]
struct Fresh {
    connection: files_core::sftp_connection::Connection,
    store: files_core::sftp_registry::Store,
    record: files_core::sftp_registry::Record,
    blocked: bool,
}
#[cfg(target_os = "linux")]
impl Fresh {
    fn publish(&mut self) -> Result<(), String> {
        use files_core::sftp_registry::Publication;
        let result = self
            .store
            .publish(&mut self.record)
            .map_err(|e| e.to_string())
            .and_then(|p| match p {
                Publication::Durable => Ok(()),
                other => Err(format!("Registro no durable: {other:?}")),
            });
        if result.is_err() {
            self.blocked = true;
        }
        result
    }
    fn persist(&mut self) -> Result<Status, String> {
        use files_core::{
            sftp_lifecycle::Phase as Live,
            sftp_registry::{Phase, Process},
        };
        let snap = self.connection.snapshot();
        let process = |old: Option<Process>, pid| -> Result<Option<Process>, String> {
            match pid {
                Some(pid) if old.is_some_and(|v| v.pid == pid) => Ok(old),
                Some(pid) => Process::capture(pid).map(Some).map_err(|e| e.to_string()),
                None => Ok(None),
            }
        };
        let server = if snap.phase != Live::Absent && snap.server_pid.is_none() {
            self.record.server
        } else {
            process(self.record.server, snap.server_pid)?
        };
        let unmount = process(self.record.unmount, snap.unmount_pid)?;
        let mount = if snap.phase == Live::Absent {
            None
        } else {
            self.connection.observed_mount().cloned()
        };
        let phase = match snap.phase {
            Live::Ready => Phase::Verified,
            Live::Absent => Phase::Absent,
            Live::Disconnecting if mount.is_some() => Phase::Disconnecting,
            Live::Verifying if server.is_some() => Phase::Spawned,
            _ => Phase::Uncertain,
        };
        if self.record.phase != phase
            || self.record.server != server
            || self.record.unmount != unmount
            || self.record.mount != mount
        {
            self.record.phase = phase;
            self.record.server = server;
            self.record.unmount = unmount;
            self.record.mount = mount;
            self.publish()?;
        }
        Ok(Status {
            phase: format!("{:?}", snap.phase),
            point: Some(snap.point),
            diagnostic: snap.diagnostic,
            open: snap.phase == Live::Ready && !self.blocked,
            writable: self.store.writable() && !self.blocked,
        })
    }
}
#[cfg(target_os = "linux")]
impl Engine {
    fn status(&self, error: &str) -> Status {
        let mut s = self.status.clone();
        if !error.is_empty() {
            s.diagnostic = bounded_error(error);
            s.open = false;
        }
        s
    }
    fn run(&mut self, command: Command) -> Status {
        if matches!(command, Command::Poll | Command::Open)
            && let Some(f) = &mut self.fresh
            && f.blocked
        {
            // Reap owned children, but never silently retry a failed publication.
            f.connection.poll();
            return self.status.clone();
        }
        match self.execute(command) {
            Ok(status) => {
                self.status = status.clone();
                status
            }
            Err(e) => {
                self.status = self.status(&e);
                self.status.clone()
            }
        }
    }
    fn execute(&mut self, command: Command) -> Result<Status, String> {
        self.execute_io(command).map_err(|e| e.to_string())
    }
    fn execute_io(&mut self, command: Command) -> Result<Status, std::io::Error> {
        use files_core::{
            sftp_connection::Connection,
            sftp_mount_point::Parent,
            sftp_recovered::{Admission, Controller},
            sftp_registry::{Phase, Publication, Record, Store},
        };
        use std::{fs::DirBuilder, io, os::unix::fs::DirBuilderExt};
        let root = files_core::theme::config_dir().join("sftp");
        match command {
            Command::Inspect(cap) if self.fresh.is_none() && self.recovered.is_none() => {
                // Inspection alone does not create a directory or hold a writer lease.
                if !root.try_exists()? {
                    return Ok(Status {
                        phase: "Sin conexión registrada".into(),
                        ..Status::default()
                    });
                }
                let (store, record) = Store::open(&root)?;
                let Some(record) = record else {
                    return Ok(Status::default());
                };
                let writable = store.writable();
                drop(store);
                self.status = Status {
                    phase: format!("Registrado · {:?}", record.phase),
                    point: Some(record.point),
                    diagnostic:
                        "Inspección: usar Reconciliar para controlar; no acredita servicio remoto"
                            .into(),
                    open: false,
                    writable,
                };
                // No controller/lease until an explicit action.
                let _ = cap;
            }
            Command::Connect(target, cap) => {
                if self
                    .fresh
                    .as_ref()
                    .is_some_and(|f| !f.blocked && f.record.phase == Phase::Absent)
                {
                    self.fresh = None;
                }
                if self.recovered.as_ref().is_some_and(|c| {
                    c.snapshot().phase == files_core::sftp_recovered::Phase::Absent
                }) {
                    self.recovered = None;
                }
                if self.fresh.is_some() || self.recovered.is_some() {
                    return Err(io::Error::other(
                        "Ya hay un controlador; conservar registro",
                    ));
                }
                if !cap.tools_ready() {
                    return Err(io::Error::other(cap.summary()));
                }
                match DirBuilder::new().mode(0o700).create(&root) {
                    Ok(()) => {}
                    Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
                    Err(e) => return Err(e),
                }
                let (mut store, old) = Store::open(&root)?;
                if !store.writable() || old.is_some_and(|r| r.phase != Phase::Absent) {
                    return Err(io::Error::other(
                        "Registro existente: reconciliar antes de conectar",
                    ));
                }
                let point = Parent::open(&root)?.create()?;
                let mut record = Record::prepared(target.clone(), &point);
                if !matches!(store.publish(&mut record)?, Publication::Durable) {
                    return Err(io::Error::other(
                        "Preparación no durable: no se inicia SSHFS",
                    ));
                }
                let connection = Connection::start(
                    target,
                    point,
                    cap.sshfs.as_ref().unwrap(),
                    cap.unmount.as_ref().unwrap(),
                )?;
                // Retain the child even if PID capture/publication fails after spawn.
                self.fresh = Some(Fresh {
                    connection,
                    store,
                    record,
                    blocked: false,
                });
                let f = self.fresh.as_mut().unwrap();
                self.status.point = Some(f.connection.snapshot().point);
                match f.persist() {
                    Ok(s) => return Ok(s),
                    Err(e) => {
                        f.blocked = true;
                        return Err(io::Error::other(e));
                    }
                }
            }
            Command::Open => {
                if let Some(f) = &mut self.fresh {
                    f.connection.poll();
                    let result = f.persist().map_err(io::Error::other);
                    if result.is_err() {
                        f.blocked = true;
                    }
                    return result;
                }
                if self.recovered.is_none() {
                    let cap = crate::sftp_state::probe().map_err(io::Error::other)?;
                    let program = cap
                        .unmount
                        .ok_or_else(|| io::Error::other("Falta fusermount3"))?;
                    self.recovered = Some(Controller::open(&root, &program)?);
                }
                let c = self.recovered.as_ref().unwrap();
                let point = c.verified_path()?;
                let snapshot = c.snapshot();
                return Ok(Status {
                    phase: "Recuperada · Lectura verificada".into(),
                    point: Some(point),
                    diagnostic: snapshot.diagnostic,
                    open: true,
                    writable: snapshot.writable,
                });
            }
            Command::Inspect(_) | Command::Poll => {}
            action => {
                if let Some(f) = &mut self.fresh {
                    if f.blocked && !matches!(action, Command::Sync) {
                        return Err(io::Error::other(
                            "Registro incierto: cierre y recuperación explícita; no repetir acciones",
                        ));
                    }
                    match action {
                        Command::Disconnect => {
                            f.record.phase = Phase::Disconnecting;
                            f.publish().map_err(io::Error::other)?;
                            f.connection.disconnect()?;
                        }
                        Command::Reconcile => {}
                        Command::Sync => {
                            f.store.retry_sync()?;
                            f.blocked = false;
                        }
                        _ => unreachable!(),
                    }
                    return f.persist().map_err(io::Error::other);
                } else {
                    if self.recovered.is_none() {
                        let cap = crate::sftp_state::probe().map_err(io::Error::other)?;
                        let program = cap
                            .unmount
                            .ok_or_else(|| io::Error::other("Falta fusermount3"))?;
                        self.recovered = Some(Controller::open(&root, &program)?);
                    }
                    let c = self.recovered.as_mut().unwrap();
                    match action {
                        Command::Disconnect => match c.disconnect()? {
                            Admission::Started => {}
                            Admission::StartedUncertain(e) | Admission::NotStarted(e) => {
                                return Err(io::Error::other(e));
                            }
                        },
                        Command::Reconcile => {
                            c.reconcile()?;
                        }
                        Command::Sync => c.retry_sync()?,
                        _ => unreachable!(),
                    }
                }
            }
        }
        if let Some(f) = &mut self.fresh {
            f.connection.poll();
            let result = f.persist().map_err(io::Error::other);
            if result.is_err() {
                f.blocked = true;
            }
            return result;
        }
        if let Some(c) = &mut self.recovered {
            let s = c.poll();
            return Ok(Status {
                phase: format!("Recuperada · {:?}", s.phase),
                point: Some(s.point),
                diagnostic: s.diagnostic,
                open: false,
                writable: s.writable,
            });
        }
        Ok(self.status.clone())
    }
}
#[cfg(not(target_os = "linux"))]
#[derive(Default)]
struct Engine;
#[cfg(not(target_os = "linux"))]
impl Engine {
    fn status(&self, error: &str) -> Status {
        Status {
            diagnostic: bounded_error(error),
            phase: "Sólo Linux en esta entrega".into(),
            ..Status::default()
        }
    }
    fn run(&mut self, _: Command) -> Status {
        self.status("")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn held(mutating: bool) -> (Client, SyncSender<Status>, Receiver<(u64, Command)>) {
        let (tx, input) = mpsc::sync_channel(1);
        let (output, rx) = mpsc::sync_channel(1);
        (
            Client {
                worker: Some(Worker { tx, rx }),
                pending: Some(mutating),
                ..Client::default()
            },
            output,
            input,
        )
    }
    #[test]
    fn cancelling_close_never_revalidates_pre_close_queued_permission() {
        let gate = Gate::default();
        let old = gate.epoch.load(Ordering::SeqCst);
        assert!(gate.accepts(old));
        gate.stop();
        assert!(!gate.accepts(old));
        gate.resume();
        assert!(!gate.accepts(old));
        let new = gate.epoch.load(Ordering::SeqCst);
        assert!(gate.accepts(new));
        gate.stop();
        gate.stop();
        gate.resume();
        assert!(!gate.accepts(old) && !gate.accepts(new));
        assert!(gate.accepts(gate.epoch.load(Ordering::SeqCst)));
    }
    #[test]
    fn errors_bound_utf8_bytes_and_make_truncation_explicit() {
        let error = "🦀".repeat(4096);
        let text = bounded_error(&error);
        assert!(text.len() <= 4096 && text.ends_with(" [Informe truncado]"));
        assert!(error.starts_with(text.strip_suffix(" [Informe truncado]").unwrap()));
        assert_eq!(bounded_error("error corto"), "error corto");
    }
    #[test]
    fn close_requires_mutation_ack_and_never_admits_late_actions() {
        let (mut c, output, _input) = held(true);
        assert!(!c.close_ready());
        assert!(c.send(Command::Disconnect).is_err());
        output
            .send(Status {
                phase: "Registrado".into(),
                ..Status::default()
            })
            .unwrap();
        assert!(c.pump());
        assert!(c.close_ready());
        assert!(c.send(Command::Reconcile).is_err());
    }
    #[test]
    fn readonly_pending_allows_close_without_fabricating_ack() {
        let (mut c, _output, _input) = held(false);
        assert!(c.close_ready());
        assert!(c.busy());
        assert_eq!(c.pending, Some(false));
    }
    #[test]
    fn single_flight_and_reply_survive_modal_independence() {
        let (mut c, output, _input) = held(true);
        assert!(c.send(Command::Disconnect).is_err());
        let path = PathBuf::from("/owned/mount");
        output
            .send(Status {
                phase: "Ready".into(),
                point: Some(path.clone()),
                open: true,
                ..Status::default()
            })
            .unwrap();
        assert!(c.pump());
        assert_eq!(c.status.point, Some(path));
        assert!(c.status.open && !c.busy());
    }
    #[test]
    fn lost_worker_does_not_leave_open_permission_or_fake_success() {
        let (mut c, output, _input) = held(true);
        c.status.open = true;
        drop(output);
        assert!(c.pump());
        assert!(!c.status.open);
        assert!(c.status.diagnostic.contains("registro conservado"));
        assert!(c.close_ready());
    }
}
