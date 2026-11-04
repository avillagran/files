use super::*;
use crate::{
    directory_sync_journal::Object, directory_sync_manifest::NamespaceSnapshot,
    exdev_source::observed_identity,
};
use sha2::Digest;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreationEvidence {
    ExclusiveFile,
    LinkedHeldDescriptor,
    NamedPostimage,
}
/// Historical positive syscall/readback/ACK facts, not native approval or lasting freshness.
#[derive(Debug)]
pub struct CreatedStageNode {
    file: File,
    object: Object,
    proof: [u8; 212],
    journal: (u64, u64, u64),
    pid: libc::pid_t,
    node: u32,
    evidence: CreationEvidence,
    copied: u64,
    verified: u64,
}
impl CreatedStageNode {
    pub fn object(&self) -> Object {
        self.object
    }
    pub fn node(&self) -> u32 {
        self.node
    }
    pub fn evidence(&self) -> CreationEvidence {
        self.evidence
    }
    pub fn copied(&self) -> u64 {
        self.copied
    }
    pub fn verified(&self) -> u64 {
        self.verified
    }
}
#[derive(Debug)]
pub struct StageCreateFailure {
    pub cause: io::Error,
    pub intent_acknowledged: bool,
    pub syscall_attempted: bool,
    pub syscall_succeeded: bool,
    pub identity_recorded: bool,
}
impl std::fmt::Display for StageCreateFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.cause.fmt(f)
    }
}
impl std::error::Error for StageCreateFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.cause)
    }
}
#[derive(Clone, Copy)]
pub(in crate::directory_sync_store) enum Point {
    IntentWritten,
    IntentFileSynced,
    IntentAcknowledged,
    IntentDurable,
    BeforeCreate,
    Created,
    Recorded,
    Payload,
    DataSynced,
    Ready,
}
fn object(file: &File) -> io::Result<Object> {
    Ok(Object {
        identity: observed_identity(&file.metadata()?)?,
        mount: node(file)?.mount,
    })
}
fn open_at(parent: &File, name: &[u8], flags: i32) -> io::Result<File> {
    owned(unsafe {
        libc::openat(
            parent.as_raw_fd(),
            c(name)?.as_ptr(),
            flags | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
        )
    })
}
fn parent(workspace: &File, spec: &wire::create::StageSpec) -> io::Result<File> {
    private_directory(workspace)?;
    if object(workspace)? != spec.workspace {
        return Err(bad("Workspace staged sustituido"));
    }
    let path = c(format!("/proc/self/fd/{}", workspace.as_raw_fd()).as_bytes())?;
    let mut current = owned(unsafe {
        libc::open(
            path.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOATIME | libc::O_CLOEXEC,
        )
    })?;
    for (name, expected) in &spec.parents {
        current = open_at(
            &current,
            name,
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOATIME,
        )?;
        private_directory(&current)?;
        if object(&current)? != *expected {
            return Err(bad("Padre staged sustituido"));
        }
    }
    Ok(current)
}
impl Journal<'_> {
    /// Explicit trusted-coordinator creation; no native approval or automatic recovery.
    pub fn create_stage_node(
        &mut self,
        index: u32,
        source: &NamespaceSnapshot,
        workspace: &File,
        leader: Option<&CreatedStageNode>,
        read: u64,
        cancel: &AtomicBool,
    ) -> Result<CreatedStageNode, StageCreateFailure> {
        self.create_stage_inner(index, source, workspace, leader, read, cancel, None, |_| {
            Ok(())
        })
    }
    pub fn create_stage_node_controlled(
        &mut self,
        index: u32,
        source: &NamespaceSnapshot,
        workspace: &File,
        leader: Option<&CreatedStageNode>,
        read: u64,
        control: &crate::copy_engine::Control,
    ) -> Result<CreatedStageNode, StageCreateFailure> {
        self.create_stage_inner(
            index,
            source,
            workspace,
            leader,
            read,
            control.cancellation_flag(),
            Some(control),
            |_| Ok(()),
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub(in crate::directory_sync_store) fn create_stage_inner(
        &mut self,
        index: u32,
        source: &NamespaceSnapshot,
        workspace: &File,
        leader: Option<&CreatedStageNode>,
        read: u64,
        cancel: &AtomicBool,
        control: Option<&crate::copy_engine::Control>,
        mut hook: impl FnMut(Point) -> io::Result<()>,
    ) -> Result<CreatedStageNode, StageCreateFailure> {
        let mut acknowledged = false;
        let mut attempted = false;
        let mut succeeded = false;
        let mut recorded = false;
        let mut writing = false;
        let result = (|| {
            let checkpoint = || -> io::Result<()> {
                check_cancel(cancel)?;
                if let Some(c) = control {
                    c.checkpoint()?;
                }
                Ok(())
            };
            checkpoint()?;
            self.ensure_current()?;
            self.schema.staging_admission()?;
            if read > crate::directory_compare::MAX_READ {
                return Err(bad("Allowance creación inválido"));
            }
            let spec = self.replay.state.stage_spec(&self.schema, index)?;
            let mut input: crate::exdev_source::StageInput<'_> = source.inventory.stage_input(
                &source.parent,
                &source.name,
                spec.source_index,
                cancel,
            )?;
            let (root, observed, relative) = input.binding();
            if root != spec.source_root
                || observed != spec.source
                || relative != spec.source_relative
            {
                return Err(bad("Source no corresponde al nodo sellado"));
            }
            if observed.object.identity.kind == Kind::File && observed.revision.size > read {
                return Err(bad("Payload excede allowance creación"));
            }
            let p = parent(workspace, &spec)?;
            let parent_object = object(&p)?;
            let name = c(&spec.name)?;
            match open_at(&p, &spec.name, libc::O_PATH) {
                Err(e) if e.kind() == io::ErrorKind::NotFound => (),
                Err(e) => return Err(e),
                Ok(_) => {
                    return Err(io::Error::new(
                        io::ErrorKind::AlreadyExists,
                        "Nombre staged ocupado",
                    ));
                }
            }
            let expected_links = if let Some(expected) = spec.leader {
                let held = leader.ok_or_else(|| bad("Falta recibo live del líder"))?;
                if held.object != expected
                    || held.proof != self.store.proof.encode()
                    || held.journal != self.identity()
                    || held.pid != unsafe { libc::getpid() }
                    || held.node >= index
                    || object(&held.file)? != expected
                {
                    return Err(bad("Líder staged no corresponde"));
                }
                let links = self.replay.state.known_links(expected);
                if held.file.metadata()?.nlink() != links {
                    return Err(bad("Alias externo o líder cambiado"));
                }
                links + 1
            } else {
                if leader.is_some() {
                    return Err(bad("Líder inesperado"));
                }
                1
            };
            let literal = if observed.object.identity.kind == Kind::Symlink {
                Some(input.literal(cancel)?)
            } else {
                None
            };
            writing = true;
            self.append_with(
                Event::NodeCreateIntent {
                    node: index,
                    seal: spec.seal,
                },
                cancel,
                |at| match at {
                    Barrier::Write => hook(Point::IntentWritten),
                    Barrier::FileSynced => hook(Point::IntentFileSynced),
                    Barrier::Acknowledged => hook(Point::IntentAcknowledged),
                    _ => Ok(()),
                },
            )?;
            acknowledged = true;
            hook(Point::IntentDurable)?;
            checkpoint()?;
            input.verify(cancel)?;
            let p2 = parent(workspace, &spec)?;
            if object(&p2)? != parent_object {
                return Err(bad("Padre staged cambió"));
            }
            hook(Point::BeforeCreate)?;
            checkpoint()?;
            self.ensure_current()?;
            if self.replay.state.node_create_guard() != Some((index, spec.seal)) {
                return Err(bad("Intent creación cambió"));
            }
            let mut exclusive = None;
            let evidence = if let Some(held) = leader {
                if held.file.metadata()?.nlink() + 1 != expected_links {
                    return Err(bad("Líder cambió antes de link"));
                }
                let path = c(format!("/proc/self/fd/{}", held.file.as_raw_fd()).as_bytes())?;
                attempted = true;
                if unsafe {
                    libc::linkat(
                        libc::AT_FDCWD,
                        path.as_ptr(),
                        p.as_raw_fd(),
                        name.as_ptr(),
                        libc::AT_SYMLINK_FOLLOW,
                    )
                } != 0
                {
                    return Err(io::Error::last_os_error());
                }
                CreationEvidence::LinkedHeldDescriptor
            } else {
                match observed.object.identity.kind {
                    Kind::File => {
                        attempted = true;
                        exclusive = Some(owned(unsafe {
                            libc::openat(
                                p.as_raw_fd(),
                                name.as_ptr(),
                                libc::O_CREAT
                                    | libc::O_EXCL
                                    | libc::O_RDWR
                                    | libc::O_NOFOLLOW
                                    | libc::O_NONBLOCK
                                    | libc::O_NOATIME
                                    | libc::O_CLOEXEC,
                                0o600,
                            )
                        })?);
                        CreationEvidence::ExclusiveFile
                    }
                    Kind::Directory => {
                        attempted = true;
                        if unsafe { libc::mkdirat(p.as_raw_fd(), name.as_ptr(), 0o700) } != 0 {
                            return Err(io::Error::last_os_error());
                        }
                        CreationEvidence::NamedPostimage
                    }
                    Kind::Symlink => {
                        let target = c(literal.as_ref().expect("literal"))?;
                        attempted = true;
                        if unsafe { libc::symlinkat(target.as_ptr(), p.as_raw_fd(), name.as_ptr()) }
                            != 0
                        {
                            return Err(io::Error::last_os_error());
                        }
                        CreationEvidence::NamedPostimage
                    }
                }
            };
            succeeded = true;
            hook(Point::Created)?;
            checkpoint()?;
            self.ensure_current()?;
            let pin = if let Some(f) = exclusive {
                f
            } else {
                open_at(&p, &spec.name, libc::O_PATH)?
            };
            let o = object(&pin)?;
            if o.identity.kind != spec.destination.identity.kind
                || o.identity.device != spec.destination.identity.device
                || o.mount != spec.destination.mount
                || pin.metadata()?.uid() != unsafe { libc::geteuid() }
                || spec.leader.is_some_and(|e| e != o)
            {
                return Err(bad("Postimagen staged inesperada"));
            }
            let file = if o.identity.kind == Kind::Symlink
                || evidence == CreationEvidence::ExclusiveFile
            {
                pin
            } else {
                let path = c(format!("/proc/self/fd/{}", pin.as_raw_fd()).as_bytes())?;
                let fd = owned(unsafe {
                    libc::open(
                        path.as_ptr(),
                        libc::O_RDONLY | libc::O_NONBLOCK | libc::O_NOATIME | libc::O_CLOEXEC,
                    )
                })?;
                if object(&fd)? != o {
                    return Err(bad("Postimagen FD cambió"));
                }
                fd
            };
            if o.identity.kind == Kind::Directory {
                private_directory(&file)?;
                let path = format!("/proc/self/fd/{}", file.as_raw_fd());
                if fs::read_dir(path)?.next().is_some() {
                    return Err(bad("Directorio recién creado no está vacío"));
                }
            } else if file.metadata()?.nlink() != expected_links {
                return Err(bad("Links staged inesperados"));
            }
            let binding = || -> io::Result<()> {
                checkpoint()?;
                let current = parent(workspace, &spec)?;
                if object(&current)? != parent_object {
                    return Err(bad("Padre staged cambió"));
                }
                let named = open_at(&current, &spec.name, libc::O_PATH)?;
                if object(&named)? != o || object(&file)? != o {
                    return Err(bad("Nombre staged sustituido"));
                }
                Ok(())
            };
            binding()?;
            self.append(
                Event::NodeCreated {
                    node: index,
                    object: o,
                },
                cancel,
            )?;
            recorded = true;
            hook(Point::Recorded)?;
            if evidence == CreationEvidence::ExclusiveFile {
                binding()?;
                self.ensure_current()?;
                if unsafe { libc::fchmod(file.as_raw_fd(), 0o600) } != 0 {
                    return Err(io::Error::last_os_error());
                }
            }
            let mut copied = 0;
            let mut verified = 0;
            if o.identity.kind == Kind::File && spec.leader.is_none() {
                let mut writer = &file;
                copied = input.copy(&mut writer, || {
                    binding()?;
                    self.ensure_current()
                })?;
            }
            hook(Point::Payload)?;
            binding()?;
            input.verify(cancel)?;
            if o.identity.kind == Kind::File {
                if file.metadata()?.len() > read {
                    return Err(bad("Readback excede allowance"));
                }
                let mut reader = &file;
                reader.seek(SeekFrom::Start(0))?;
                let mut h = sha2::Sha256::new();
                let mut b = vec![0u8; 64 * 1024];
                while verified < observed.revision.size {
                    binding()?;
                    self.ensure_current()?;
                    let remaining = observed.revision.size - verified;
                    let length = remaining.min(b.len() as u64) as usize;
                    let n = reader.read(&mut b[..length])?;
                    if n == 0 {
                        return Err(bad("Readback truncado"));
                    }
                    verified += n as u64;
                    h.update(&b[..n]);
                }
                binding()?;
                self.ensure_current()?;
                if reader.read(&mut [0; 1])? != 0 {
                    return Err(bad("Readback payload creció"));
                }
                let hash: [u8; 32] = h.finalize().into();
                if verified != observed.revision.size || hash != observed.content {
                    return Err(bad("Readback staged difiere"));
                }
            } else if o.identity.kind == Kind::Symlink {
                let mut b = vec![0; 4097];
                let n = unsafe {
                    libc::readlinkat(
                        file.as_raw_fd(),
                        c"".as_ptr(),
                        b.as_mut_ptr().cast(),
                        b.len(),
                    )
                };
                if n < 0 {
                    return Err(io::Error::last_os_error());
                }
                b.truncate(n as usize);
                if b != literal.expect("literal") {
                    return Err(bad("Symlink staged difiere"));
                }
            }
            if o.identity.kind != Kind::Directory && file.metadata()?.nlink() != expected_links {
                return Err(bad("Links staged cambiaron"));
            }
            if o.identity.kind != Kind::Symlink {
                file.sync_all()?;
            }
            p.sync_all()?;
            hook(Point::DataSynced)?;
            binding()?;
            input.verify(cancel)?;
            self.ensure_current()?;
            self.append(Event::NodeMaterialized(index), cancel)?;
            hook(Point::Ready)?;
            binding()?;
            self.ensure_current()?;
            Ok(CreatedStageNode {
                file,
                object: o,
                proof: self.store.proof.encode(),
                journal: self.identity(),
                pid: unsafe { libc::getpid() },
                node: index,
                evidence,
                copied,
                verified,
            })
        })();
        if result.is_err() && (writing || attempted) {
            self.state = State::Uncertain;
        }
        result.map_err(|cause| StageCreateFailure {
            cause,
            intent_acknowledged: acknowledged,
            syscall_attempted: attempted,
            syscall_succeeded: succeeded,
            identity_recorded: recorded,
        })
    }
}
