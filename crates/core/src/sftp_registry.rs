//! Single active SFTP transaction journal, anchored to its private directory.
//! Advisory lifetime lock; no takeover, automatic adoption/unmount/delete,
//! hostile-writer isolation or power-loss acceptance. All APIs run off-UI.
#![cfg(target_os = "linux")]
use crate::{
    mount_table::Mount,
    sftp_mount_point::{Parent, Point, Provenance},
    sftp_target::Target,
};
use std::{
    ffi::{CStr, CString, OsStr},
    fs::File,
    io::{self, Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::ffi::OsStrExt,
    },
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
const LIMIT: usize = 16 * 1024;
const MAGIC: &[u8] = b"FILESSFTP\x01";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Prepared,
    Spawned,
    Verified,
    Disconnecting,
    Uncertain,
    Absent,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Process {
    pub pid: u32,
    pub start_ticks: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub target: Target,
    pub point: PathBuf,
    pub provenance: Provenance,
    pub phase: Phase,
    pub server: Option<Process>,
    pub unmount: Option<Process>,
    pub mount: Option<Mount>,
    generation: u64,
}
impl Record {
    pub fn prepared(target: Target, point: &Point) -> Self {
        Self {
            target,
            point: point.path().to_owned(),
            provenance: point.provenance(),
            phase: Phase::Prepared,
            server: None,
            unmount: None,
            mount: None,
            generation: 0,
        }
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    fn validate(&self, root: &Path) -> io::Result<()> {
        let bad = || invalid("Registro SFTP inválido o fuera del límite");
        let raw = self.point.as_os_str().as_bytes();
        let name = self.point.file_name().ok_or_else(bad)?.as_bytes();
        if raw.len() > 4096
            || raw.contains(&0)
            || self.point.parent() != Some(root)
            || name.len() != 43
            || !name.starts_with(b"files-sftp-")
            || !name[11..].iter().all(u8::is_ascii_hexdigit)
            || self
                .point
                .components()
                .any(|p| !matches!(p, Component::RootDir | Component::Normal(_)))
            || self.provenance.parent_inode == 0
            || self.provenance.point_inode == 0
        {
            return Err(bad());
        }
        for p in [self.server, self.unmount].into_iter().flatten() {
            if p.pid == 0 || p.pid > i32::MAX as u32 || p.start_ticks == 0 {
                return Err(bad());
            }
        }
        if let Some(m) = &self.mount
            && (m.id == 0
                || m.point != self.point
                || m.source != self.point.file_name().unwrap()
                || m.filesystem != b"fuse.sshfs"
                || m.root.as_os_str().len() > 4096
                || !m.root.is_absolute()
                || m.root.as_os_str().as_bytes().contains(&0))
        {
            return Err(bad());
        }
        match self.phase {
            Phase::Prepared
                if self.server.is_some() || self.unmount.is_some() || self.mount.is_some() =>
            {
                Err(bad())
            }
            Phase::Spawned if self.server.is_none() || self.unmount.is_some() => Err(bad()),
            Phase::Verified
                if self.server.is_none() || self.mount.is_none() || self.unmount.is_some() =>
            {
                Err(bad())
            }
            Phase::Disconnecting if self.mount.is_none() => Err(bad()),
            Phase::Absent
                if self.server.is_some() || self.unmount.is_some() || self.mount.is_some() =>
            {
                Err(bad())
            }
            _ => Ok(()),
        }
    }
    fn same_intent(&self, other: &Self) -> bool {
        self.target == other.target
            && self.point == other.point
            && self.provenance == other.provenance
    }
}
fn next_temp() -> Option<u64> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let mut old = NEXT.load(Ordering::Relaxed);
    loop {
        let next = old.checked_add(1)?;
        match NEXT.compare_exchange_weak(old, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return Some(old),
            Err(actual) => old = actual,
        }
    }
}
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn blob(out: &mut Vec<u8>, bytes: &[u8]) -> io::Result<()> {
    let n = u16::try_from(bytes.len()).map_err(|_| invalid("Campo demasiado grande"))?;
    out.extend(n.to_le_bytes());
    out.extend(bytes);
    Ok(())
}
fn number(out: &mut Vec<u8>, value: u64) {
    out.extend(value.to_le_bytes());
}
fn encode(record: &Record, root: &Path, generation: u64) -> io::Result<Vec<u8>> {
    record.validate(root)?;
    let (host, user, port, dir) = record.target.parts();
    // Preflight before duplicating field bytes; all variable fields are bounded.
    let total = MAGIC.len()
        + 128
        + host.len()
        + user.len()
        + dir.len()
        + record.point.as_os_str().len()
        + record
            .mount
            .as_ref()
            .map_or(0, |m| m.root.as_os_str().len());
    if total > LIMIT {
        return Err(invalid("Registro supera16KiB"));
    }
    let mut out = Vec::with_capacity(total);
    out.extend(MAGIC);
    number(&mut out, generation);
    out.push(record.phase as u8);
    for v in [
        record.provenance.parent_device,
        record.provenance.parent_inode,
        record.provenance.point_device,
        record.provenance.point_inode,
    ] {
        number(&mut out, v);
    }
    blob(&mut out, host.as_bytes())?;
    blob(&mut out, user.as_bytes())?;
    out.extend(port.unwrap_or(0).to_le_bytes());
    blob(&mut out, dir.as_bytes())?;
    blob(&mut out, record.point.as_os_str().as_bytes())?;
    for p in [record.server, record.unmount] {
        out.push(u8::from(p.is_some()));
        if let Some(p) = p {
            out.extend(p.pid.to_le_bytes());
            number(&mut out, p.start_ticks);
        }
    }
    out.push(u8::from(record.mount.is_some()));
    if let Some(m) = &record.mount {
        number(&mut out, m.id);
        number(&mut out, m.parent);
        out.extend(m.device.0.to_le_bytes());
        out.extend(m.device.1.to_le_bytes());
        blob(&mut out, m.root.as_os_str().as_bytes())?;
    }
    Ok(out)
}
struct Cursor<'a> {
    bytes: &'a [u8],
    position: usize,
}
impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> io::Result<&'a [u8]> {
        let end = self
            .position
            .checked_add(n)
            .ok_or_else(|| invalid("Longitud inválida"))?;
        let v = self
            .bytes
            .get(self.position..end)
            .ok_or_else(|| invalid("Registro truncado"))?;
        self.position = end;
        Ok(v)
    }
    fn byte(&mut self) -> io::Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn flag(&mut self) -> io::Result<bool> {
        match self.byte()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(invalid("Flag inválido")),
        }
    }
    fn u64(&mut self) -> io::Result<u64> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> io::Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn bytes(&mut self, limit: usize) -> io::Result<&'a [u8]> {
        let n = u16::from_le_bytes(self.take(2)?.try_into().unwrap()) as usize;
        if n > limit {
            return Err(invalid("Campo supera límite"));
        }
        self.take(n)
    }
    fn text(&mut self, limit: usize) -> io::Result<&'a str> {
        std::str::from_utf8(self.bytes(limit)?).map_err(|_| invalid("Texto no UTF8"))
    }
    fn process(&mut self) -> io::Result<Option<Process>> {
        Ok(if self.flag()? {
            Some(Process {
                pid: self.u32()?,
                start_ticks: self.u64()?,
            })
        } else {
            None
        })
    }
}
fn decode(bytes: &[u8], root: &Path) -> io::Result<Record> {
    if bytes.len() > LIMIT || !bytes.starts_with(MAGIC) {
        return Err(invalid(
            "Versión futura/desconocida o registro SFTP inválido; conservado",
        ));
    }
    let mut c = Cursor {
        bytes,
        position: MAGIC.len(),
    };
    let generation = c.u64()?;
    if generation == 0 {
        return Err(invalid("Generación inválida"));
    }
    let phase = match c.byte()? {
        0 => Phase::Prepared,
        1 => Phase::Spawned,
        2 => Phase::Verified,
        3 => Phase::Disconnecting,
        4 => Phase::Uncertain,
        5 => Phase::Absent,
        _ => return Err(invalid("Fase futura/desconocida")),
    };
    let provenance = Provenance {
        parent_device: c.u64()?,
        parent_inode: c.u64()?,
        point_device: c.u64()?,
        point_inode: c.u64()?,
    };
    let host = c.text(253)?;
    let user = c.text(64)?;
    let port = u16::from_le_bytes(c.take(2)?.try_into().unwrap());
    let dir = c.text(4096)?;
    let point = c.bytes(4096)?;
    let server = c.process()?;
    let unmount = c.process()?;
    let mount = if c.flag()? {
        Some((c.u64()?, c.u64()?, c.u32()?, c.u32()?, c.bytes(4096)?))
    } else {
        None
    };
    if c.position != bytes.len() {
        return Err(invalid("Datos adicionales no reconocidos"));
    }
    // Only materialize payload after complete length/UTF8/framing validation.
    let port = if port == 0 {
        String::new()
    } else {
        port.to_string()
    };
    let target = Target::parse(host, user, &port, dir).map_err(invalid)?;
    let point = PathBuf::from(OsStr::from_bytes(point));
    let mount = mount.map(|(id, parent, major, minor, raw)| Mount {
        id,
        parent,
        device: (major, minor),
        root: PathBuf::from(OsStr::from_bytes(raw)),
        source: point.file_name().unwrap_or_default().to_os_string(),
        point: point.clone(),
        filesystem: b"fuse.sshfs".to_vec(),
    });
    let record = Record {
        target,
        point,
        provenance,
        phase,
        server,
        unmount,
        mount,
        generation,
    };
    record.validate(root)?;
    Ok(record)
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Revision {
    device: u64,
    inode: u64,
    size: i64,
    mtime: (i64, i64),
    ctime: (i64, i64),
}
fn regular(file: &File) -> io::Result<Revision> {
    let mut s = std::mem::MaybeUninit::<libc::stat>::uninit();
    if unsafe { libc::fstat(file.as_raw_fd(), s.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    let s = unsafe { s.assume_init() };
    if s.st_mode & libc::S_IFMT != libc::S_IFREG
        || s.st_uid != unsafe { libc::geteuid() }
        || s.st_nlink != 1
        || s.st_mode & 0o7777 != 0o600
    {
        return Err(invalid(
            "Registro/lock no es regular propio0600 con un enlace",
        ));
    }
    Ok(Revision {
        device: s.st_dev,
        inode: s.st_ino,
        size: s.st_size,
        mtime: (s.st_mtime, s.st_mtime_nsec),
        ctime: (s.st_ctime, s.st_ctime_nsec),
    })
}
fn open_file(parent: &File, name: &CStr, flags: i32) -> io::Result<File> {
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            flags | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
            0o600,
        )
    };
    if fd < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(unsafe { File::from_raw_fd(fd) })
    }
}
#[derive(PartialEq, Eq)]
struct Current {
    revision: Revision,
    bytes: Vec<u8>,
}
fn read_current(parent: &File) -> io::Result<Option<Current>> {
    let mut file = match open_file(parent, c"connection.bin", libc::O_RDONLY) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let before = regular(&file)?;
    if before.size < 0 || before.size as u64 > LIMIT as u64 {
        return Err(invalid("Registro supera16KiB"));
    }
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(LIMIT as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > LIMIT
        || regular(&file)? != before
        || regular(&open_file(parent, c"connection.bin", libc::O_RDONLY)?)? != before
    {
        return Err(invalid("Registro cambió durante lectura"));
    }
    Ok(Some(Current {
        revision: before,
        bytes,
    }))
}
#[derive(Debug)]
pub enum Publication {
    Durable,
    Unsynced(String),
    Uncertain(String),
}
// CLOEXEC does not release a flock inherited by another thread's pre-exec child.
// Explicit owner-only unlock avoids retaining ownership after the owner drops.
struct Lease {
    file: File,
    owner_pid: Option<libc::pid_t>,
}
impl std::ops::Deref for Lease {
    type Target = File;
    fn deref(&self) -> &File {
        &self.file
    }
}
impl Lease {
    fn release(&self) {
        if self.owner_pid != Some(unsafe { libc::getpid() }) {
            return;
        }
        loop {
            if unsafe { libc::flock(self.file.as_raw_fd(), libc::LOCK_UN) } == 0 {
                break;
            }
            if io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
                break;
            }
        }
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        self.release();
    }
}
pub struct Store {
    parent: Parent,
    root: PathBuf,
    lock: Lease,
    lock_identity: (u64, u64),
    writable: bool,
    blocked: bool,
    unsynced: bool,
    expected: Option<Current>,
    generation: u64,
}
impl Store {
    pub fn open(root: &Path) -> io::Result<(Self, Option<Record>)> {
        let parent = Parent::open(root)?;
        let lock = open_file(
            parent.directory(),
            c"connection.lock",
            libc::O_RDWR | libc::O_CREAT,
        )?;
        let rev = regular(&lock)?;
        let locked = unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        let writable = if locked == 0 {
            true
        } else {
            let e = io::Error::last_os_error();
            if !matches!(e.raw_os_error(), Some(libc::EWOULDBLOCK)) {
                return Err(e);
            }
            false
        };
        let lock = Lease {
            file: lock,
            owner_pid: writable.then(|| unsafe { libc::getpid() }),
        };
        parent.verify()?;
        let expected = read_current(parent.directory())?;
        let record = expected
            .as_ref()
            .map(|r| decode(&r.bytes, root))
            .transpose()?;
        if record.as_ref().is_some_and(|r| {
            parent.identity_pair() != (r.provenance.parent_device, r.provenance.parent_inode)
        }) {
            return Err(invalid(
                "Proveniencia del padre guardado no coincide; registro conservado",
            ));
        }
        let generation = record.as_ref().map_or(0, |r| r.generation);
        let s = Self {
            parent,
            root: root.into(),
            lock,
            lock_identity: (rev.device, rev.inode),
            writable,
            blocked: false,
            unsynced: false,
            expected,
            generation,
        };
        s.check()?;
        Ok((s, record))
    }
    pub(crate) fn current(&self, record: &Record) -> io::Result<()> {
        self.check()?;
        let loaded = self
            .expected
            .as_ref()
            .map(|c| decode(&c.bytes, &self.root))
            .transpose()?;
        if loaded.as_ref() != Some(record) {
            return Err(invalid("Registro de acción obsoleto"));
        }
        Ok(())
    }
    pub(crate) fn action_mount(&self, record: &Record, rows: &[Mount]) -> io::Result<Mount> {
        self.current(record)?;
        record.validate(&self.root)?;
        if self.parent.identity_pair()
            != (
                record.provenance.parent_device,
                record.provenance.parent_inode,
            )
        {
            return Err(invalid("Padre distinto; no actuar"));
        }
        let fresh = crate::mount_table::unique_sshfs(
            rows,
            &record.point,
            record.point.file_name().expect("Validated point"),
        )
        .map_err(invalid)?;
        if record.mount.as_ref() != Some(fresh) {
            return Err(invalid("Firma distinta o no registrada; no desmontar"));
        }
        Ok(fresh.clone())
    }
    pub fn writable(&self) -> bool {
        self.writable && !self.blocked && !self.unsynced
    }
    fn check(&self) -> io::Result<()> {
        self.parent.verify()?;
        let held = regular(&self.lock)?;
        let named = regular(&open_file(
            self.parent.directory(),
            c"connection.lock",
            libc::O_RDONLY,
        )?)?;
        if (held.device, held.inode) != self.lock_identity
            || (named.device, named.inode) != self.lock_identity
        {
            return Err(invalid("Lock reemplazado; no tomar control"));
        }
        if read_current(self.parent.directory())? != self.expected {
            return Err(invalid("Registro externo modificado; no sobrescribir"));
        }
        Ok(())
    }
    pub fn publish(&mut self, record: &mut Record) -> io::Result<Publication> {
        self.publish_with_sync(record, File::sync_all)
    }
    fn publish_with_sync(
        &mut self,
        record: &mut Record,
        sync: impl FnOnce(&File) -> io::Result<()>,
    ) -> io::Result<Publication> {
        if !self.writable() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Registro sólo lectura, pendiente de sync o bloqueado",
            ));
        }
        self.check()?;
        record.validate(&self.root)?;
        let root_proof = self.parent.identity_pair();
        if root_proof
            != (
                record.provenance.parent_device,
                record.provenance.parent_inode,
            )
        {
            return Err(invalid("Proveniencia del padre no coincide"));
        }
        if let Some(old) = self
            .expected
            .as_ref()
            .map(|r| decode(&r.bytes, &self.root))
            .transpose()?
        {
            let new_intent = record.generation == 0;
            if new_intent {
                if old.phase != Phase::Absent || record.phase != Phase::Prepared {
                    return Err(invalid("Conexión anterior requiere reconciliación"));
                }
            } else if record.generation != self.generation || !old.same_intent(record) {
                return Err(invalid("Revisión o intención obsoleta"));
            } else if record.phase != Phase::Absent
                && (old.phase == Phase::Absent
                    || record.phase == Phase::Prepared
                    || (old.server.is_some() && record.server != old.server)
                    || (old.mount.is_some() && record.mount != old.mount))
            {
                return Err(invalid(
                    "No borrar/reemplazar identidades activas sin cierre reconciliado",
                ));
            }
            if record.phase == Phase::Absent {
                let mut bytes = Vec::new();
                File::open("/proc/self/mountinfo")?
                    .take(1024 * 1024 + 1)
                    .read_to_end(&mut bytes)?;
                let rows = crate::mount_table::parse(&bytes).map_err(invalid)?;
                match self.observe(&old, &rows)? {
                    Recovery::NoMount {
                        server: None | Some(ProcessStatus::Missing | ProcessStatus::Reused),
                    } => {}
                    _ => {
                        return Err(invalid(
                            "No cerrar registro: montaje/proceso presente, zombie o identidad incierta",
                        ));
                    }
                }
                if let Some(p) = old.unmount
                    && !matches!(
                        process_status(p)?,
                        ProcessStatus::Missing | ProcessStatus::Reused
                    )
                {
                    return Err(invalid("Desmontaje todavía no reaped; conservar registro"));
                }
            }
        } else if record.generation != 0 || record.phase != Phase::Prepared {
            return Err(invalid("Falta intención preparada inicial"));
        }
        let next = self
            .generation
            .checked_add(1)
            .ok_or_else(|| invalid("Generaciones agotadas"))?;
        let bytes = encode(record, &self.root, next)?;
        let (name, mut file) = (0..64)
            .find_map(|_| {
                let id = next_temp()?;
                let name =
                    CString::new(format!("connection.tmp-{}-{id}", std::process::id())).unwrap();
                match open_file(
                    self.parent.directory(),
                    &name,
                    libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
                ) {
                    Ok(f) => Some(Ok((name, f))),
                    Err(e) if e.kind() == io::ErrorKind::AlreadyExists => None,
                    Err(e) => Some(Err(e)),
                }
            })
            .ok_or_else(|| invalid("Temporales exclusivos agotados"))??;
        let before_publish = (|| {
            regular(&file)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            self.check()?;
            let staged = regular(&file)?;
            if regular(&open_file(self.parent.directory(), &name, libc::O_RDONLY)?)? != staged {
                return Err(invalid("Temporal sustituido"));
            }
            let flags = if self.expected.is_none() {
                libc::RENAME_NOREPLACE
            } else {
                0
            };
            if unsafe {
                libc::renameat2(
                    self.parent.directory().as_raw_fd(),
                    name.as_ptr(),
                    self.parent.directory().as_raw_fd(),
                    c"connection.bin".as_ptr(),
                    flags,
                )
            } != 0
            {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        })();
        if let Err(e) = before_publish {
            return Err(io::Error::new(
                e.kind(),
                format!(
                    "Sin ACK de publicación; no se borró por inferencia; nombre temporal solicitado {name:?}: {e}"
                ),
            ));
        }
        record.generation = next;
        self.generation = next;
        match read_current(self.parent.directory()) {
            Ok(Some(current)) if current.bytes == bytes => self.expected = Some(current),
            _ => {
                self.blocked = true;
                return Ok(Publication::Uncertain(
                    "Publicación ACK pero lectura/identidad posterior no confirmada".into(),
                ));
            }
        }
        if let Err(e) = self.check() {
            self.blocked = true;
            return Ok(Publication::Uncertain(e.to_string()));
        }
        match sync(self.parent.directory()) {
            Ok(()) => Ok(Publication::Durable),
            Err(e) => {
                self.unsynced = true;
                Ok(Publication::Unsynced(e.to_string()))
            }
        }
    }
    pub fn retry_sync(&mut self) -> io::Result<()> {
        if !self.writable || self.blocked || !self.unsynced {
            return Err(invalid("No hay sync pendiente admisible"));
        }
        self.check()?;
        self.parent.directory().sync_all()?;
        self.unsynced = false;
        Ok(())
    }
    pub fn observe(&self, record: &Record, table: &[Mount]) -> io::Result<Recovery> {
        self.parent.verify()?;
        record.validate(&self.root)?;
        if self.parent.identity_pair()
            != (
                record.provenance.parent_device,
                record.provenance.parent_inode,
            )
        {
            return Ok(Recovery::Uncertain("Padre reemplazado"));
        }
        let mut candidates = table.iter().filter(|m| m.point == record.point);
        let Some(current) = candidates.next() else {
            if table.iter().any(|m| m.point.starts_with(&record.point)) {
                return Ok(Recovery::Uncertain(
                    "Montaje descendiente; no inspeccionar ni borrar",
                ));
            }
            match self
                .parent
                .private_child_identity(record.point.file_name().expect("Validated point"))
            {
                Ok(pair)
                    if pair
                        == (
                            record.provenance.point_device,
                            record.provenance.point_inode,
                        ) => {}
                _ => {
                    return Ok(Recovery::Uncertain(
                        "Punto ausente, sustituido o no privado; conservar registro",
                    ));
                }
            }
            return Ok(Recovery::NoMount {
                server: record.server.map(process_status).transpose()?,
            });
        };
        if candidates.next().is_some() || record.mount.as_ref() != Some(current) {
            return Ok(Recovery::Uncertain(
                "Montaje sin firma registrada o distinto; no adoptar",
            ));
        }
        match record.server.map(process_status).transpose()? {
            Some(ProcessStatus::MatchingLive) => Ok(Recovery::MatchingMountAndProcess),
            _ => Ok(Recovery::Uncertain(
                "Proceso ausente, reutilizado o terminado; no adoptar",
            )),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessStatus {
    MatchingLive,
    Missing,
    Reused,
    Ended,
}
#[derive(Debug, PartialEq, Eq)]
pub enum Recovery {
    MatchingMountAndProcess,
    NoMount { server: Option<ProcessStatus> },
    Uncertain(&'static str),
}
fn read_process(pid: u32) -> io::Result<Option<(Process, bool)>> {
    let directory = PathBuf::from(format!("/proc/{pid}"));
    let file = match File::open(directory.join("stat")) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            match std::fs::symlink_metadata(&directory) {
                Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
                _ => return Err(invalid("stat ausente con identidad no confirmada")),
            }
        }
        Err(e) => return Err(e),
    };
    let mut bytes = Vec::new();
    file.take(4097).read_to_end(&mut bytes)?;
    if bytes.len() > 4096 {
        return Err(invalid("stat supera límite"));
    }
    let closing = bytes
        .iter()
        .rposition(|&b| b == b')')
        .ok_or_else(|| invalid("stat truncado"))?;
    let fields = bytes
        .get(closing + 2..)
        .ok_or_else(|| invalid("stat truncado"))?;
    let mut parts = fields
        .split(u8::is_ascii_whitespace)
        .filter(|p| !p.is_empty());
    let state = parts.next().ok_or_else(|| invalid("Estado ausente"))?;
    let raw = parts.nth(18).ok_or_else(|| invalid("starttime ausente"))?;
    let start_ticks = std::str::from_utf8(raw)
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|&v| v != 0)
        .ok_or_else(|| invalid("starttime inválido"))?;
    Ok(Some((
        Process { pid, start_ticks },
        !matches!(state, b"Z" | b"X" | b"x"),
    )))
}
impl Process {
    pub fn capture(pid: u32) -> io::Result<Self> {
        match read_process(pid)? {
            Some((p, true)) => Ok(p),
            _ => Err(invalid("Proceso no vivo para registrar")),
        }
    }
}
pub(crate) fn process_status(expected: Process) -> io::Result<ProcessStatus> {
    Ok(match read_process(expected.pid)? {
        None => ProcessStatus::Missing,
        Some((p, _)) if p != expected => ProcessStatus::Reused,
        Some((_, true)) => ProcessStatus::MatchingLive,
        Some((_, false)) => ProcessStatus::Ended,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
        process::{Command, Stdio},
    };
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let base = PathBuf::from(std::env::var_os("HOME").unwrap()).join(".cache/files-test");
            fs::create_dir_all(&base).unwrap();
            loop {
                let path = base.join(format!(
                    "sftp-registry-{}-{}",
                    std::process::id(),
                    next_temp().unwrap()
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
                Target::parse("host", "user", "2222", "/Fotos ñ/%20/$()").unwrap(),
                &Parent::open(&self.0).unwrap().create().unwrap(),
            )
        }
        fn put(&self, name: &str, bytes: &[u8]) {
            let mut f = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(self.0.join(name))
                .unwrap();
            f.write_all(bytes).unwrap();
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    fn verified(mut r: Record) -> Record {
        r.phase = Phase::Verified;
        r.server = Some(Process::capture(std::process::id()).unwrap());
        r.mount = Some(Mount {
            id: 22,
            parent: 1,
            device: (0, 9),
            root: "/remote".into(),
            point: r.point.clone(),
            filesystem: b"fuse.sshfs".to_vec(),
            source: r.point.file_name().unwrap().into(),
        });
        r
    }
    #[test]
    fn codec_roundtrip_raw_paths_and_every_truncation_rejected() {
        use std::os::unix::ffi::OsStringExt;
        let f = Fixture::new();
        let raw = f.0.join(std::ffi::OsString::from_vec(b"raw-\xff".to_vec()));
        fs::DirBuilder::new().mode(0o700).create(&raw).unwrap();
        let point = Parent::open(&raw).unwrap().create().unwrap();
        let mut r = verified(Record::prepared(
            Target::parse("host", "", "65535", "/ñ/%20").unwrap(),
            &point,
        ));
        r.generation = 19;
        let bytes = encode(&r, &raw, 19).unwrap();
        assert_eq!(decode(&bytes, &raw).unwrap(), r);
        for end in 0..bytes.len() {
            assert!(decode(&bytes[..end], &raw).is_err(), "prefix {end}");
        }
        let mut extra = bytes.clone();
        extra.push(0);
        assert!(decode(&extra, &raw).is_err());
        let mut future = bytes;
        future[MAGIC.len() - 1] = 2;
        assert!(decode(&future, &raw).is_err());
    }
    #[test]
    fn owner_drop_unlocks_despite_preexec_fd_and_child_cannot_unlock_parent() {
        let f = Fixture::new();
        let (owner, _) = Store::open(&f.0).unwrap();
        let mut ready = [-1; 2];
        let mut finish = [-1; 2];
        assert_eq!(
            unsafe { libc::pipe2(ready.as_mut_ptr(), libc::O_CLOEXEC) },
            0
        );
        assert_eq!(
            unsafe { libc::pipe2(finish.as_mut_ptr(), libc::O_CLOEXEC) },
            0
        );
        let child = unsafe { libc::fork() };
        assert!(child >= 0);
        if child == 0 {
            // Only stack and async-signal-safe syscalls after multithreaded fork.
            unsafe {
                libc::close(ready[0]);
                libc::close(finish[1]);
            }
            owner.lock.release(); // PID guard must not unlock the parent.
            let mut byte = 1u8;
            let sent = unsafe { libc::write(ready[1], (&byte as *const u8).cast(), 1) };
            let read = unsafe { libc::read(finish[0], (&mut byte as *mut u8).cast(), 1) };
            unsafe {
                libc::_exit(if sent == 1 && read == 1 { 0 } else { 1 });
            }
        }
        unsafe {
            libc::close(ready[1]);
            libc::close(finish[0]);
        }
        let mut byte = 0u8;
        assert_eq!(
            unsafe { libc::read(ready[0], (&mut byte as *mut u8).cast(), 1) },
            1
        );
        let reader = Store::open(&f.0).unwrap().0;
        let parent_protected = !reader.writable();
        drop(owner);
        let new_owner = Store::open(&f.0).unwrap().0;
        let released_with_child_holding_fd = new_owner.writable();
        assert_eq!(
            unsafe { libc::write(finish[1], (&byte as *const u8).cast(), 1) },
            1
        );
        let mut status = 0;
        assert_eq!(unsafe { libc::waitpid(child, &mut status, 0) }, child);
        unsafe {
            libc::close(ready[0]);
            libc::close(finish[1]);
        }
        assert!(libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0);
        assert!(parent_protected, "forked child released live parent lease");
        assert!(
            released_with_child_holding_fd,
            "owner drop retained inherited flock"
        );
        assert!(!reader.writable(), "reader auto-upgraded");
    }
    #[test]
    fn lifetime_lock_is_readonly_without_automatic_upgrade_and_reopens_after_drop() {
        let f = Fixture::new();
        let (mut owner, _) = Store::open(&f.0).unwrap();
        let mut record = f.record();
        assert!(matches!(
            owner.publish(&mut record).unwrap(),
            Publication::Durable
        ));
        let before = fs::read(f.0.join("connection.bin")).unwrap();
        assert_eq!(record.generation(), 1);
        let (mut reader, restored) = Store::open(&f.0).unwrap();
        assert!(!reader.writable());
        assert_eq!(restored.as_ref(), Some(&record));
        let mut update = record.clone();
        update.phase = Phase::Uncertain;
        assert!(reader.publish(&mut update).is_err());
        drop(owner);
        assert!(!reader.writable());
        assert!(reader.publish(&mut update).is_err());
        let (new_owner, restored) = Store::open(&f.0).unwrap();
        assert!(new_owner.writable());
        assert_eq!(restored.unwrap(), record);
        assert_eq!(fs::read(f.0.join("connection.bin")).unwrap(), before);
        assert_eq!(
            fs::metadata(f.0.join("connection.bin"))
                .unwrap()
                .permissions()
                .mode()
                & 0o7777,
            0o600
        );
    }
    #[test]
    fn invalid_future_oversize_and_symlink_records_are_preserved() {
        for bytes in [
            b"future".to_vec(),
            [MAGIC[..MAGIC.len() - 1].to_vec(), vec![2]].concat(),
            vec![0; LIMIT + 1],
        ] {
            let f = Fixture::new();
            f.put("connection.bin", &bytes);
            assert!(Store::open(&f.0).is_err());
            assert_eq!(fs::read(f.0.join("connection.bin")).unwrap(), bytes);
        }
        let f = Fixture::new();
        f.put("untouched", b"keep");
        std::os::unix::fs::symlink("untouched", f.0.join("connection.bin")).unwrap();
        assert!(Store::open(&f.0).is_err());
        assert_eq!(fs::read(f.0.join("untouched")).unwrap(), b"keep");
    }
    #[test]
    fn fifo_and_hardlinked_locks_rejected_without_block_or_rewrite() {
        for name in ["connection.lock", "connection.bin"] {
            let f = Fixture::new();
            let path = CString::new(f.0.join(name).as_os_str().as_bytes()).unwrap();
            assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
            assert!(Store::open(&f.0).is_err());
            assert!(
                fs::symlink_metadata(f.0.join(name)).unwrap().file_type()
                    != fs::metadata(&f.0).unwrap().file_type()
            );
        }
        let f = Fixture::new();
        f.put("connection.lock", b"");
        fs::hard_link(f.0.join("connection.lock"), f.0.join("alias")).unwrap();
        assert!(Store::open(&f.0).is_err());
        assert!(f.0.join("alias").exists());
    }
    #[test]
    fn external_record_change_and_replaced_lock_block_publication() {
        let f = Fixture::new();
        let (mut store, _) = Store::open(&f.0).unwrap();
        let mut record = f.record();
        store.publish(&mut record).unwrap();
        fs::write(f.0.join("connection.bin"), b"actor future data").unwrap();
        record.phase = Phase::Uncertain;
        assert!(store.publish(&mut record).is_err());
        assert_eq!(
            fs::read(f.0.join("connection.bin")).unwrap(),
            b"actor future data"
        );
        let f = Fixture::new();
        let (mut store, _) = Store::open(&f.0).unwrap();
        let mut record = f.record();
        store.publish(&mut record).unwrap();
        let before = fs::read(f.0.join("connection.bin")).unwrap();
        fs::rename(f.0.join("connection.lock"), f.0.join("retained-lock")).unwrap();
        f.put("connection.lock", b"");
        record.phase = Phase::Uncertain;
        assert!(store.publish(&mut record).is_err());
        assert_eq!(fs::read(f.0.join("connection.bin")).unwrap(), before);
    }
    #[test]
    fn post_rename_sync_failure_retains_visible_revision_and_retry_only_syncs() {
        let f = Fixture::new();
        let (mut store, _) = Store::open(&f.0).unwrap();
        let mut record = f.record();
        let result = store
            .publish_with_sync(&mut record, |_| {
                Err(io::Error::from_raw_os_error(libc::EIO))
            })
            .unwrap();
        assert!(matches!(result, Publication::Unsynced(_)));
        assert_eq!(record.generation(), 1);
        assert!(!store.writable());
        let before = fs::read(f.0.join("connection.bin")).unwrap();
        assert_eq!(decode(&before, &f.0).unwrap(), record);
        record.phase = Phase::Uncertain;
        assert!(store.publish(&mut record).is_err());
        assert_eq!(fs::read(f.0.join("connection.bin")).unwrap(), before);
        store.retry_sync().unwrap();
        assert!(store.writable());
        assert_eq!(fs::read(f.0.join("connection.bin")).unwrap(), before);
        store.publish(&mut record).unwrap();
        assert_eq!(record.generation(), 2);
    }
    #[test]
    fn stale_revision_and_new_intent_cannot_overwrite_unresolved_connection() {
        let f = Fixture::new();
        let (mut store, _) = Store::open(&f.0).unwrap();
        let mut record = f.record();
        store.publish(&mut record).unwrap();
        let mut stale = record.clone();
        record.phase = Phase::Uncertain;
        store.publish(&mut record).unwrap();
        stale.phase = Phase::Absent;
        assert!(store.publish(&mut stale).is_err());
        let mut new = f.record();
        assert!(store.publish(&mut new).is_err());
        record.phase = Phase::Absent;
        store.publish(&mut record).unwrap();
        store.publish(&mut new).unwrap();
        assert_eq!(new.generation(), 4);
    }
    #[test]
    fn synthetic_recovery_requires_full_mount_record_and_matching_live_process() {
        let f = Fixture::new();
        let (store, _) = Store::open(&f.0).unwrap();
        let mut record = verified(f.record());
        let mount = record.mount.clone().unwrap();
        assert_eq!(
            store
                .observe(&record, std::slice::from_ref(&mount))
                .unwrap(),
            Recovery::MatchingMountAndProcess
        );
        let mut changed = mount.clone();
        changed.id += 1;
        assert!(matches!(
            store.observe(&record, &[changed]).unwrap(),
            Recovery::Uncertain(_)
        ));
        record.mount = None;
        record.phase = Phase::Uncertain;
        assert!(matches!(
            store
                .observe(&record, std::slice::from_ref(&mount))
                .unwrap(),
            Recovery::Uncertain(_)
        ));
        record.mount = Some(mount.clone());
        record.server.as_mut().unwrap().start_ticks += 1;
        assert!(matches!(
            store.observe(&record, &[mount]).unwrap(),
            Recovery::Uncertain(_)
        ));
        assert_eq!(
            store.observe(&record, &[]).unwrap(),
            Recovery::NoMount {
                server: Some(ProcessStatus::Reused)
            }
        );
    }
    #[test]
    fn zombie_is_ended_not_live_then_reaped() {
        let mut child = Command::new("/usr/bin/cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let identity = Process::capture(child.id()).unwrap();
        drop(child.stdin.take());
        let mut info = std::mem::MaybeUninit::<libc::siginfo_t>::zeroed();
        let waited = unsafe {
            libc::waitid(
                libc::P_PID,
                child.id(),
                info.as_mut_ptr(),
                libc::WEXITED | libc::WNOWAIT,
            )
        };
        let status = process_status(identity);
        let reaped = child.wait().unwrap();
        assert_eq!(waited, 0);
        assert_eq!(status.unwrap(), ProcessStatus::Ended);
        assert!(reaped.success());
        assert!(!matches!(
            process_status(identity).unwrap(),
            ProcessStatus::MatchingLive
        ));
    }
    #[test]
    fn invalid_modes_and_parent_substitution_never_change_actor_data() {
        let f = Fixture::new();
        f.put("connection.lock", b"");
        fs::set_permissions(
            f.0.join("connection.lock"),
            fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        assert!(Store::open(&f.0).is_err());
        assert_eq!(
            fs::metadata(f.0.join("connection.lock"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o644
        );
        let f = Fixture::new();
        let root = f.0.join("root");
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        let (mut store, _) = Store::open(&root).unwrap();
        let point = Parent::open(&root).unwrap().create().unwrap();
        let mut record = Record::prepared(Target::parse("host", "", "", "").unwrap(), &point);
        fs::rename(&root, f.0.join("old-root")).unwrap();
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        fs::write(root.join("actor"), b"keep").unwrap();
        assert!(store.publish(&mut record).is_err());
        assert_eq!(fs::read(root.join("actor")).unwrap(), b"keep");
        assert!(!root.join("connection.bin").exists());
    }
    #[test]
    fn no_mount_is_not_accepted_as_original_point_after_replacement_or_nested_mount() {
        let f = Fixture::new();
        let (store, _) = Store::open(&f.0).unwrap();
        let record = f.record();
        let nested = Mount {
            id: 33,
            parent: 1,
            device: (0, 1),
            root: "/".into(),
            point: record.point.join("nested"),
            filesystem: b"fuse.sshfs".to_vec(),
            source: "actor".into(),
        };
        assert!(matches!(
            store.observe(&record, &[nested]).unwrap(),
            Recovery::Uncertain(_)
        ));
        fs::rename(&record.point, f.0.join("retained")).unwrap();
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&record.point)
            .unwrap();
        fs::write(record.point.join("actor"), b"keep").unwrap();
        assert!(matches!(
            store.observe(&record, &[]).unwrap(),
            Recovery::Uncertain(_)
        ));
        assert_eq!(fs::read(record.point.join("actor")).unwrap(), b"keep");
    }
    #[test]
    fn active_server_and_mount_signatures_are_retained_until_explicit_absent() {
        let f = Fixture::new();
        let (mut store, _) = Store::open(&f.0).unwrap();
        let mut record = f.record();
        store.publish(&mut record).unwrap();
        let mut active = verified(record.clone());
        store.publish(&mut active).unwrap();
        let before = fs::read(f.0.join("connection.bin")).unwrap();
        let mut lost = active.clone();
        lost.phase = Phase::Uncertain;
        lost.server = None;
        assert!(store.publish(&mut lost).is_err());
        lost = active.clone();
        lost.phase = Phase::Uncertain;
        lost.mount = None;
        assert!(store.publish(&mut lost).is_err());
        lost = active.clone();
        lost.mount.as_mut().unwrap().id += 1;
        assert!(store.publish(&mut lost).is_err());
        assert_eq!(fs::read(f.0.join("connection.bin")).unwrap(), before);
        active.phase = Phase::Uncertain;
        store.publish(&mut active).unwrap();
        active.phase = Phase::Absent;
        active.server = None;
        active.mount = None;
        assert!(store.publish(&mut active).is_err());
    }
    #[test]
    fn absent_publication_requires_real_namespace_absence_and_process_reap() {
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
        let mut active = verified(record);
        active.server = Some(Process::capture(child.id()).unwrap());
        store.publish(&mut active).unwrap();
        let mut absent = active.clone();
        absent.phase = Phase::Absent;
        absent.server = None;
        absent.mount = None;
        let live_rejected = store.publish(&mut absent).is_err();
        drop(child.stdin.take());
        let mut info = std::mem::MaybeUninit::<libc::siginfo_t>::zeroed();
        let waited = unsafe {
            libc::waitid(
                libc::P_PID,
                child.id(),
                info.as_mut_ptr(),
                libc::WEXITED | libc::WNOWAIT,
            )
        };
        let zombie_rejected = store.publish(&mut absent).is_err();
        let reaped = child.wait().unwrap();
        assert!(live_rejected && zombie_rejected);
        assert_eq!(waited, 0);
        assert!(reaped.success());
        assert!(matches!(
            store.publish(&mut absent).unwrap(),
            Publication::Durable
        ));
    }
}
