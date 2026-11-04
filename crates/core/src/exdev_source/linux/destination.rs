//! Owned temporary destination only. Caller supplies a private empty directory;
//! source remains in place. No publication, source removal or durable recovery.
use super::*;
pub const MAX_PINNED_NODES: usize = 128;
struct Created {
    pin: File,
    identity: Identity,
    pending_parent: Option<File>,
}
pub struct OwnedCopy {
    inventory: Inventory,
    stage: File,
    created: Vec<Created>,
    verified: bool,
    publication_parent: Option<File>,
}
impl std::fmt::Debug for OwnedCopy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OwnedCopy")
            .field("created", &self.created.len())
            .field("verified", &self.verified)
            .finish_non_exhaustive()
    }
}
pub struct FailedCopy {
    pub cause: io::Error,
    pub partial: Box<OwnedCopy>,
}
impl std::fmt::Debug for FailedCopy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FailedCopy")
            .field("cause", &self.cause)
            .field("created", &self.partial.created.len())
            .finish()
    }
}
impl OwnedCopy {
    pub fn inventory(&self) -> &Inventory {
        &self.inventory
    }
    pub fn stage(&self) -> &File {
        &self.stage
    }
    pub fn created_count(&self) -> usize {
        self.created.len()
    }
    pub fn pinned_fds(&self) -> usize {
        self.created.len()
            + usize::from(self.publication_parent.is_some())
            + 2
            + self
                .created
                .iter()
                .filter(|c| c.pending_parent.is_some())
                .count()
    } // source root, stage, created pins; executing temps separate
    pub fn verified(&self) -> bool {
        self.verified
    }
    pub fn destination_identities(&self) -> impl Iterator<Item = Identity> + '_ {
        self.created.iter().map(|c| c.identity)
    }
    fn destination_parent(&self, relative: &[u8]) -> io::Result<(File, std::ffi::OsString)> {
        if relative.is_empty() || relative.len() > MAX_PATH {
            return Err(invalid("Ruta destino inválida"));
        }
        let mut parent = self
            .publication_parent
            .as_ref()
            .unwrap_or(&self.stage)
            .try_clone()?;
        let mut offset = 0usize;
        let mut parts = relative.split(|b| *b == b'/').peekable();
        while let Some(part) = parts.next() {
            component(part)?;
            if parts.peek().is_none() {
                return Ok((parent, std::ffi::OsString::from_vec(part.to_vec())));
            }
            offset += part.len();
            let prefix = &relative[..offset];
            let index = self
                .inventory
                .entries
                .binary_search_by(|e| e.relative.as_slice().cmp(prefix))
                .map_err(|_| invalid("Padre destino fuera del manifiesto"))?;
            let created = self
                .created
                .get(index)
                .filter(|c| c.identity.kind == Kind::Directory)
                .ok_or(invalid("Padre destino no creado/propio"))?;
            matches_pin(&parent, OsStr::from_bytes(part), created)?;
            // Use the PIN, not an open of a possibly replaced pathname after
            // validation. Relocation may move our directory, never redirects IO.
            parent = created.pin.try_clone()?;
            offset += 1;
        }
        Err(invalid("Ruta destino vacía"))
    }
    pub(crate) fn root_pin(&self) -> io::Result<&File> {
        self.created
            .first()
            .map(|c| &c.pin)
            .ok_or(invalid("Destino raíz no creado"))
    }
    pub(crate) fn note_publication(&mut self, parent: File) -> io::Result<()> {
        self.verified = false;
        self.publication_parent = Some(parent);
        let root = self
            .inventory
            .entries
            .first()
            .ok_or(invalid("Inventario vacío"))?;
        let (parent, name) = self.destination_parent(&root.relative)?;
        matches_pin(
            &parent,
            &name,
            self.created
                .first()
                .ok_or(invalid("Raíz destino ausente"))?,
        )
    }
    pub(crate) fn rebase_source_after_own_rename(
        &mut self,
        parent: &File,
        name: &OsStr,
        cancel: &AtomicBool,
    ) -> io::Result<()> {
        self.inventory
            .rebase_after_own_root_rename(parent, name, cancel)
    }
    /// Remove only live pinned objects, reverse order, without following parents.
    /// Replacement or nonempty directory stops cleanup and retains ownership FDs.
    /// This is runtime cleanup, NOT recovery from a serialized identity alone.
    pub fn cleanup(self) -> Result<Inventory, FailedCopy> {
        self.cleanup_with(|| Ok(()))
    }
    fn cleanup_with(
        mut self,
        mut before_sync: impl FnMut() -> io::Result<()>,
    ) -> Result<Inventory, FailedCopy> {
        self.verified = false;
        if self.publication_parent.is_some() {
            return Err(FailedCopy {
                cause: invalid("Destino publicado: cleanup automático prohibido"),
                partial: Box::new(self),
            });
        }
        while !self.created.is_empty() {
            let index = self.created.len() - 1;
            let result = (|| {
                if self.created[index].pending_parent.is_none() {
                    let entry = &self.inventory.entries[index];
                    let (parent, name) = self.destination_parent(&entry.relative)?;
                    matches_pin(&parent, &name, &self.created[index])?;
                    let flags = if self.created[index].identity.kind == Kind::Directory {
                        libc::AT_REMOVEDIR
                    } else {
                        0
                    };
                    if unsafe {
                        libc::unlinkat(parent.as_raw_fd(), c(name.as_bytes())?.as_ptr(), flags)
                    } != 0
                    {
                        return Err(io::Error::last_os_error());
                    }
                    // Syscall success is retained even if fsync/ACK subsequently
                    // fails. Retry syncs THIS parent, never touches that name.
                    self.created[index].pending_parent = Some(parent);
                }
                before_sync()?;
                self.created[index]
                    .pending_parent
                    .as_ref()
                    .unwrap()
                    .sync_all()?;
                Ok(())
            })();
            if let Err(cause) = result {
                return Err(FailedCopy {
                    cause,
                    partial: Box::new(self),
                });
            }
            self.created.pop();
        }
        let final_check = (|| {
            if DirectoryStream::new(&self.stage)?.next()?.is_some() {
                return Err(invalid("Staging contiene objetos ajenos; conservados"));
            }
            self.stage.sync_all()
        })();
        if let Err(cause) = final_check {
            return Err(FailedCopy {
                cause,
                partial: Box::new(self),
            });
        }
        Ok(self.inventory)
    }
    /// Verify content, metadata and topology against source, then sync every
    /// created file/directory and private stage. No success on any sync error.
    pub fn verify_and_sync(&mut self, cancel: &AtomicBool) -> io::Result<()> {
        self.verify_and_sync_inner(cancel, None)
    }
    pub(crate) fn verify_and_sync_controlled(&mut self, control: &Control) -> io::Result<()> {
        self.verify_and_sync_inner(control.cancellation_flag(), Some(control))
    }
    fn verify_and_sync_inner(
        &mut self,
        cancel: &AtomicBool,
        control: Option<&Control>,
    ) -> io::Result<()> {
        self.verified = false;
        if self.created.len() != self.inventory.entries.len() {
            return Err(invalid("Copia incompleta"));
        }
        let expected: HashMap<&[u8], &Entry> = self
            .inventory
            .entries
            .iter()
            .map(|e| (e.relative.as_slice(), e))
            .collect();
        // Before any readlink changes access time, check the intended destination
        // access time against the original source observation, including symlinks.
        for (entry, created) in self.inventory.entries.iter().zip(&self.created) {
            check_control(cancel, control)?;
            let (parent, name) = self.destination_parent(&entry.relative)?;
            matches_pin(&parent, &name, created)?;
            let m = created.pin.metadata()?;
            if (m.atime(), m.atime_nsec()) != (entry.access.seconds, entry.access.nanos as i64) {
                return Err(invalid("Atime destino no preservado"));
            }
        }
        let mut seen = 0usize;
        let mut source_groups = HashMap::new();
        let mut destination_groups = HashMap::new();
        let mut walker = Walker::with_control(cancel, Some(&expected), control, |observed| {
            let index = self
                .inventory
                .entries
                .binary_search_by(|e| e.relative.cmp(&observed.relative))
                .map_err(|_| invalid("Entrada extra en destino"))?;
            let expected = &self.inventory.entries[index];
            let actual = &self.created[index];
            if observed.identity != actual.identity
                || observed.identity.kind != expected.identity.kind
                || observed.content_digest != expected.content_digest
                || observed.metadata_digest != expected.metadata_digest
                || (observed.identity.kind == Kind::File
                    && observed.revision.size != expected.revision.size)
                || (observed.identity.kind != Kind::Directory
                    && observed.revision.links != expected.revision.links)
            {
                return Err(invalid(
                    "Destino no coincide con contenido/metadatos/manifiesto",
                ));
            }
            let source = (expected.identity.device, expected.identity.inode);
            let destination = (observed.identity.device, observed.identity.inode);
            if source_groups
                .get(&source)
                .is_some_and(|old| *old != destination)
                || destination_groups
                    .get(&destination)
                    .is_some_and(|old| *old != source)
            {
                return Err(invalid("Topología hardlink destino distinta"));
            }
            source_groups.insert(source, destination);
            destination_groups.insert(destination, source);
            seen += 1;
            Ok(())
        });
        let name = std::ffi::OsString::from_vec(self.inventory.entries[0].relative.clone());
        let effective_parent = self.publication_parent.as_ref().unwrap_or(&self.stage);
        matches_pin(effective_parent, &name, &self.created[0])?;
        let root = open(effective_parent, &name)?;
        walker.walk(effective_parent, &name, root, name.as_bytes(), 0)?;
        matches_pin(effective_parent, &name, &self.created[0])?;
        if seen != self.created.len() {
            return Err(invalid("Faltan entradas destino"));
        }
        // Verification readlink may update symlink atime. Restore only our pinned
        // destination links, not source, then sync their parents.
        for index in (0..self.created.len()).rev() {
            check_control(cancel, control)?;
            let entry = &self.inventory.entries[index];
            let created = &self.created[index];
            let (parent, name) = self.destination_parent(&entry.relative)?;
            matches_pin(&parent, &name, created)?;
            if created.identity.kind == Kind::Symlink {
                set_times(&parent, &name, &created.pin, entry, true)?;
                parent.sync_all()?;
            } else {
                created.pin.sync_all()?;
            }
        }
        self.stage.sync_all()?;
        if let Some(parent) = &self.publication_parent {
            parent.sync_all()?;
        }
        self.verified = true;
        Ok(())
    }
}
/// Effective copy of a bounded tree, raw paths, hardlinks and exact metadata.
/// Admission across review/queue/ACK is the caller's additional responsibility.
/// Returned ownership (including on error) must not be duplicated per UI result.
pub fn copy_into(
    inventory: Inventory,
    source_parent: &File,
    stage: File,
    cancel: &AtomicBool,
    progress: impl FnMut(u64),
) -> Result<OwnedCopy, FailedCopy> {
    copy_into_inner(inventory, source_parent, stage, cancel, None, progress)
}
pub(crate) fn copy_into_controlled(
    inventory: Inventory,
    source_parent: &File,
    stage: File,
    control: &Control,
    progress: impl FnMut(u64),
) -> Result<OwnedCopy, FailedCopy> {
    copy_into_inner(
        inventory,
        source_parent,
        stage,
        control.cancellation_flag(),
        Some(control),
        progress,
    )
}
fn copy_into_inner(
    inventory: Inventory,
    source_parent: &File,
    stage: File,
    cancel: &AtomicBool,
    control: Option<&Control>,
    mut progress: impl FnMut(u64),
) -> Result<OwnedCopy, FailedCopy> {
    let mut owned = OwnedCopy {
        inventory,
        stage,
        created: Vec::new(),
        verified: false,
        publication_parent: None,
    };
    let result = (|| {
        check_control(cancel, control)?;
        let metadata = owned.stage.metadata()?;
        if !metadata.is_dir()
            || metadata.uid() != unsafe { libc::geteuid() }
            || metadata.mode() & 0o077 != 0
        {
            return Err(invalid("Staging destino requiere directorio privado"));
        }
        if owned.inventory.entries.len() > MAX_PINNED_NODES {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Copia EXDEV temporal limitada a128nodos por raíz; origen conservado",
            ));
        }
        let mut links: HashMap<(u64, u64), u64> = HashMap::new();
        for e in &owned.inventory.entries {
            if e.identity.kind != Kind::Directory {
                *links
                    .entry((e.identity.device, e.identity.inode))
                    .or_default() += 1;
            }
        }
        for e in &owned.inventory.entries {
            if e.identity.kind != Kind::Directory
                && links[&(e.identity.device, e.identity.inode)] != e.revision.links
            {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "Hardlinks fuera de esta raíz requieren ledger de lote; origen conservado",
                ));
            }
        }
        if DirectoryStream::new(&owned.stage)?.next()?.is_some() {
            return Err(invalid("Staging destino no está vacío"));
        }
        owned
            .created
            .try_reserve_exact(owned.inventory.entries.len())
            .map_err(|_| invalid("Sin memoria para propiedad temporal"))?;
        let root = std::ffi::OsString::from_vec(owned.inventory.entries[0].relative.clone());
        owned
            .inventory
            .verify_inner(source_parent, &root, cancel, control)?;
        let mut groups: HashMap<(u64, u64), usize> = HashMap::new();
        let mut scratch = vec![0u8; BUFFER];
        let mut total = 0u64;
        for (index, entry) in owned.inventory.entries.iter().enumerate() {
            check_control(cancel, control)?;
            let (sp, sn) = parent_of(source_parent, &entry.relative)?;
            let source = open(&sp, &sn)?;
            stable(&sp, &sn, &source, entry.identity, entry.revision)?;
            let (dp, dn) = owned.destination_parent(&entry.relative)?;
            let key = (entry.identity.device, entry.identity.inode);
            if let Some(first) = groups.get(&key).copied() {
                let prior: &Entry = &owned.inventory.entries[first];
                let (fp, fnm) = owned.destination_parent(&prior.relative)?;
                matches_pin(&fp, &fnm, &owned.created[first])?;
                if unsafe {
                    libc::linkat(
                        fp.as_raw_fd(),
                        c(fnm.as_bytes())?.as_ptr(),
                        dp.as_raw_fd(),
                        c(dn.as_bytes())?.as_ptr(),
                        0,
                    )
                } != 0
                {
                    return Err(io::Error::last_os_error());
                }
                let pin = if entry.identity.kind == Kind::File {
                    data(&dp, &dn, owned.created[first].identity, false)?
                } else {
                    open(&dp, &dn)?
                };
                let identity = id(&pin.metadata()?)?;
                if identity != owned.created[first].identity {
                    return Err(invalid("Hardlink creado no confirmado"));
                }
                owned.created.push(Created {
                    pin,
                    identity,
                    pending_parent: None,
                });
            } else {
                let pin = match entry.identity.kind {
                    Kind::Directory => {
                        if unsafe {
                            libc::mkdirat(dp.as_raw_fd(), c(dn.as_bytes())?.as_ptr(), 0o700)
                        } != 0
                        {
                            return Err(io::Error::last_os_error());
                        }
                        data_open_directory(&dp, &dn)?
                    }
                    Kind::Symlink => {
                        let mut target = [0u8; MAX_PATH + 1];
                        let n = unsafe {
                            libc::readlinkat(
                                source.as_raw_fd(),
                                c(b"")?.as_ptr(),
                                target.as_mut_ptr().cast(),
                                target.len(),
                            )
                        };
                        if n < 0 {
                            return Err(io::Error::last_os_error());
                        }
                        if n as usize > MAX_PATH {
                            return Err(invalid("Symlink excede límite"));
                        }
                        let target = c(&target[..n as usize])?;
                        if unsafe {
                            libc::symlinkat(
                                target.as_ptr(),
                                dp.as_raw_fd(),
                                c(dn.as_bytes())?.as_ptr(),
                            )
                        } != 0
                        {
                            return Err(io::Error::last_os_error());
                        }
                        open(&dp, &dn)?
                    }
                    Kind::File => super::owned(unsafe {
                        libc::openat(
                            dp.as_raw_fd(),
                            c(dn.as_bytes())?.as_ptr(),
                            libc::O_RDWR
                                | libc::O_CREAT
                                | libc::O_EXCL
                                | libc::O_NOFOLLOW
                                | libc::O_NONBLOCK
                                | libc::O_NOATIME
                                | libc::O_CLOEXEC,
                            0o600,
                        )
                    })?,
                };
                let identity = id(&pin.metadata()?)?;
                if identity.kind != entry.identity.kind {
                    return Err(invalid("Tipo destino creado no confirmado"));
                }
                owned.created.push(Created {
                    pin,
                    identity,
                    pending_parent: None,
                });
                groups.insert(key, index);
                if entry.identity.kind == Kind::File {
                    let mut input = data(&sp, &sn, entry.identity, false)?;
                    let mut output = owned.created[index].pin.try_clone()?;
                    let mut remaining = entry.revision.size;
                    let mut hash = Sha256::new();
                    while remaining > 0 {
                        check_control(cancel, control)?;
                        let n =
                            input.read(&mut scratch[..remaining.min(BUFFER as u64) as usize])?;
                        if n == 0 {
                            return Err(invalid("Fuente truncada durante copia"));
                        }
                        use std::io::Write;
                        output.write_all(&scratch[..n])?;
                        hash.update(&scratch[..n]);
                        remaining -= n as u64;
                        total = total
                            .checked_add(n as u64)
                            .ok_or(invalid("Overflow bytes copiados"))?;
                        progress(total);
                    }
                    check_control(cancel, control)?;
                    if input.read(&mut [0u8; 1])? != 0
                        || <[u8; 32]>::from(hash.finalize()) != entry.content_digest
                    {
                        return Err(invalid("Fuente cambió durante copia"));
                    }
                    stable(&sp, &sn, &input, entry.identity, entry.revision)?;
                }
            }
        }
        // Children first: directory timestamps are applied after all creations.
        for (entry, created) in owned.inventory.entries.iter().zip(&owned.created).rev() {
            check_control(cancel, control)?;
            let (sp, sn) = parent_of(source_parent, &entry.relative)?;
            let source = open(&sp, &sn)?;
            stable(&sp, &sn, &source, entry.identity, entry.revision)?;
            let (dp, dn) = owned.destination_parent(&entry.relative)?;
            matches_pin(&dp, &dn, created)?;
            copy_metadata(&sp, &sn, entry, &dp, &dn, created, cancel)?;
            stable(&sp, &sn, &source, entry.identity, entry.revision)?;
        }
        owned
            .inventory
            .verify_inner(source_parent, &root, cancel, control)?;
        owned.verify_and_sync_inner(cancel, control)?;
        Ok(())
    })();
    match result {
        Ok(()) => Ok(owned),
        Err(cause) => Err(FailedCopy {
            cause,
            partial: Box::new(owned),
        }),
    }
}
fn data_open_directory(parent: &File, name: &OsStr) -> io::Result<File> {
    owned(unsafe {
        libc::openat(
            parent.as_raw_fd(),
            c(name.as_bytes())?.as_ptr(),
            libc::O_RDONLY
                | libc::O_DIRECTORY
                | libc::O_NOFOLLOW
                | libc::O_NOATIME
                | libc::O_CLOEXEC,
        )
    })
}
fn parent_of(base: &File, relative: &[u8]) -> io::Result<(File, std::ffi::OsString)> {
    if relative.is_empty() || relative.len() > MAX_PATH {
        return Err(invalid("Ruta relativa inválida"));
    }
    let mut parts = relative.split(|b| *b == b'/').peekable();
    let mut parent = base.try_clone()?;
    let mut depth = 0;
    while let Some(part) = parts.next() {
        component(part)?;
        depth += 1;
        if depth > 64 {
            return Err(invalid("Profundidad excedida"));
        }
        if parts.peek().is_none() {
            return Ok((parent, std::ffi::OsString::from_vec(part.to_vec())));
        }
        parent = data_open_directory(&parent, OsStr::from_bytes(part))?;
    }
    Err(invalid("Ruta relativa vacía"))
}
fn matches_pin(parent: &File, name: &OsStr, created: &Created) -> io::Result<()> {
    if id(&created.pin.metadata()?)? != created.identity
        || id(&entry_at(parent, name)?)? != created.identity
    {
        return Err(invalid("Destino propio sustituido: conservar reemplazo"));
    }
    Ok(())
}
fn named_path(parent: &File, name: &OsStr) -> io::Result<CString> {
    let mut path = format!("/proc/self/fd/{}/", parent.as_raw_fd()).into_bytes();
    path.extend_from_slice(name.as_bytes());
    c(&path)
}
fn set_times(
    _parent: &File,
    _name: &OsStr,
    pin: &File,
    e: &Entry,
    symlink: bool,
) -> io::Result<()> {
    let times = [
        libc::timespec {
            tv_sec: e.access.seconds,
            tv_nsec: e.access.nanos as _,
        },
        libc::timespec {
            tv_sec: e.revision.mtime_seconds,
            tv_nsec: e.revision.mtime_nanos as _,
        },
    ];
    let result = if symlink {
        unsafe {
            libc::utimensat(
                pin.as_raw_fd(),
                c(b"")?.as_ptr(),
                times.as_ptr(),
                libc::AT_EMPTY_PATH | libc::AT_SYMLINK_NOFOLLOW,
            )
        }
    } else {
        unsafe { libc::futimens(pin.as_raw_fd(), times.as_ptr()) }
    };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}
fn copy_metadata(
    sp: &File,
    sn: &OsStr,
    e: &Entry,
    dp: &File,
    dn: &OsStr,
    dest: &Created,
    cancel: &AtomicBool,
) -> io::Result<()> {
    let symlink = e.identity.kind == Kind::Symlink;
    let source = if symlink {
        open(sp, sn)?
    } else {
        data(sp, sn, e.identity, e.identity.kind == Kind::Directory)?
    };
    let source_path = if symlink {
        Some(named_path(sp, sn)?)
    } else {
        None
    };
    let destination_path = if symlink {
        Some(named_path(dp, dn)?)
    } else {
        None
    };
    let mut attrs = Vec::new();
    let digest = attributes_with(&source, source_path.as_ref(), cancel, |name, value| {
        attrs.push((name, value));
        Ok(())
    })?;
    if metadata_digest(&source.metadata()?, e.access, digest) != e.metadata_digest {
        return Err(invalid("Metadata origen cambió antes de aplicar"));
    }
    let chown = if symlink {
        unsafe {
            libc::fchownat(
                dest.pin.as_raw_fd(),
                c(b"")?.as_ptr(),
                e.revision.uid,
                e.revision.gid,
                libc::AT_EMPTY_PATH | libc::AT_SYMLINK_NOFOLLOW,
            )
        }
    } else {
        unsafe { libc::fchown(dest.pin.as_raw_fd(), e.revision.uid, e.revision.gid) }
    };
    if chown != 0 {
        return Err(io::Error::last_os_error());
    }
    if !symlink && unsafe { libc::fchmod(dest.pin.as_raw_fd(), e.revision.mode) } != 0 {
        return Err(io::Error::last_os_error());
    }
    let mut inherited = Vec::new();
    attributes_with(&dest.pin, destination_path.as_ref(), cancel, |name, _| {
        inherited.push(name);
        Ok(())
    })?;
    if symlink && (!attrs.is_empty() || !inherited.is_empty()) {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "xattrs de symlink requieren backend FD; origen conservado",
        ));
    }
    for name in inherited {
        if !attrs.iter().any(|(source, _)| *source == name) {
            let r = unsafe {
                match &destination_path {
                    Some(path) => libc::lremovexattr(path.as_ptr(), name.as_ptr()),
                    None => libc::fremovexattr(dest.pin.as_raw_fd(), name.as_ptr()),
                }
            };
            if r != 0 {
                return Err(io::Error::last_os_error());
            }
        }
    }
    for (name, value) in attrs {
        check(cancel)?;
        let r = unsafe {
            match &destination_path {
                Some(path) => libc::lsetxattr(
                    path.as_ptr(),
                    name.as_ptr(),
                    value.as_ptr().cast(),
                    value.len(),
                    0,
                ),
                None => libc::fsetxattr(
                    dest.pin.as_raw_fd(),
                    name.as_ptr(),
                    value.as_ptr().cast(),
                    value.len(),
                    0,
                ),
            }
        };
        if r != 0 {
            return Err(io::Error::last_os_error());
        }
    }
    set_times(dp, dn, &dest.pin, e, symlink)?;
    matches_pin(dp, dn, dest)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        os::unix::fs::{PermissionsExt, symlink},
        sync::atomic::Ordering,
    };
    fn fixture(label: &str) -> (std::path::PathBuf, File, File) {
        let d = crate::persistence::test_dir(label);
        fs::create_dir(d.join("source")).unwrap();
        fs::create_dir(d.join("stage")).unwrap();
        fs::set_permissions(d.join("stage"), fs::Permissions::from_mode(0o700)).unwrap();
        let source = File::open(d.join("source")).unwrap();
        let stage = File::open(d.join("stage")).unwrap();
        (d, source, stage)
    }
    #[test]
    fn tree_copy_exact_metadata_raw_hardlinks_symlink_verify_sync_and_cleanup() {
        let (d, source, stage) = fixture("exdev-copy-tree");
        fs::create_dir_all(d.join("source/tree/a")).unwrap();
        fs::write(d.join("source/tree/a/z"), b"abc").unwrap();
        fs::write(d.join("source/tree/a-b"), b"dash").unwrap();
        fs::hard_link(d.join("source/tree/a/z"), d.join("source/tree/hardlink")).unwrap();
        symlink("a/z", d.join("source/tree/link")).unwrap();
        let raw = std::ffi::OsString::from_vec(b"raw-\xff".to_vec());
        fs::write(d.join("source/tree").join(&raw), b"raw").unwrap();
        let path = c(d.join("source/tree/a/z").as_os_str().as_bytes()).unwrap();
        let attr = c(b"user.files-exdev").unwrap();
        assert_eq!(
            unsafe { libc::setxattr(path.as_ptr(), attr.as_ptr(), b"test".as_ptr().cast(), 4, 0) },
            0
        );
        fs::set_permissions(d.join("source/tree/a/z"), fs::Permissions::from_mode(0o751)).unwrap();
        let flag = AtomicBool::new(false);
        let inventory = capture(&source, OsStr::new("tree"), &flag).unwrap();
        let entries = inventory.entries().len();
        let mut copy = copy_into(inventory, &source, stage, &flag, |_| {}).unwrap();
        assert_eq!(copy.created_count(), entries);
        assert!(copy.verified());
        copy.verify_and_sync(&flag).unwrap();
        let first = fs::symlink_metadata(d.join("stage/tree/a/z")).unwrap();
        let second = fs::symlink_metadata(d.join("stage/tree/hardlink")).unwrap();
        assert_eq!(first.ino(), second.ino());
        assert_eq!(first.nlink(), 2);
        assert_eq!(first.mode() & 0o7777, 0o751);
        assert_eq!(fs::read(d.join("stage/tree/a/z")).unwrap(), b"abc");
        assert_eq!(fs::read(d.join("stage/tree").join(&raw)).unwrap(), b"raw");
        assert_eq!(
            fs::read_link(d.join("stage/tree/link")).unwrap(),
            std::path::Path::new("a/z")
        );
        let inventory = copy.cleanup().map_err(|e| e.cause).unwrap();
        inventory
            .verify(&source, OsStr::new("tree"), &flag)
            .unwrap();
        assert!(!d.join("stage/tree").exists());
        assert_eq!(fs::read(d.join("source/tree/a/z")).unwrap(), b"abc");
        fs::remove_dir_all(d).unwrap();
    }
    #[test]
    fn cancel_after_transferred_chunk_preserves_source_and_cleans_only_own_nodes() {
        let (d, source, stage) = fixture("exdev-copy-cancel");
        let bytes = vec![9u8; 3 * BUFFER];
        fs::write(d.join("source/file"), &bytes).unwrap();
        let flag = AtomicBool::new(false);
        let inventory = capture(&source, OsStr::new("file"), &flag).unwrap();
        let failed = copy_into(inventory, &source, stage, &flag, |_| {
            flag.store(true, Ordering::Release)
        })
        .unwrap_err();
        assert_eq!(failed.cause.kind(), io::ErrorKind::Interrupted);
        assert_eq!(failed.partial.created_count(), 1);
        assert_eq!(
            fs::metadata(d.join("stage/file")).unwrap().len(),
            BUFFER as u64
        );
        assert_eq!(fs::read(d.join("source/file")).unwrap(), bytes);
        failed.partial.cleanup().map_err(|e| e.cause).unwrap();
        assert!(!d.join("stage/file").exists());
        fs::remove_dir_all(d).unwrap();
    }
    #[test]
    fn parent_replacement_with_hardlink_to_owned_inode_never_redirects_cleanup_or_metadata() {
        let (d, source, stage) = fixture("exdev-copy-parent-replaced");
        fs::create_dir(d.join("source/tree")).unwrap();
        fs::write(d.join("source/tree/file"), b"own").unwrap();
        let flag = AtomicBool::new(false);
        let inventory = capture(&source, OsStr::new("tree"), &flag).unwrap();
        let copy = copy_into(inventory, &source, stage, &flag, |_| {}).unwrap();
        fs::rename(d.join("stage/tree"), d.join("stage/rescue")).unwrap();
        fs::create_dir(d.join("stage/tree")).unwrap();
        fs::hard_link(d.join("stage/rescue/file"), d.join("stage/tree/file")).unwrap();
        let before = fs::symlink_metadata(d.join("stage/tree/file")).unwrap();
        assert!(copy.destination_parent(b"tree/file").is_err());
        let failed = copy.cleanup().unwrap_err();
        assert!(failed.partial.created_count() > 0);
        let after = fs::symlink_metadata(d.join("stage/tree/file")).unwrap();
        assert_eq!(
            (before.dev(), before.ino(), before.nlink(), before.mode()),
            (after.dev(), after.ino(), after.nlink(), after.mode())
        );
        assert_eq!(fs::read(d.join("stage/tree/file")).unwrap(), b"own");
        assert_eq!(fs::read(d.join("stage/rescue/file")).unwrap(), b"own");
        drop(failed);
        fs::remove_dir_all(d).unwrap();
    }
    #[test]
    fn unlink_success_sync_failure_retries_only_sync_and_preserves_new_name() {
        let (d, source, stage) = fixture("exdev-copy-cleanup-sync");
        fs::write(d.join("source/file"), b"source").unwrap();
        let flag = AtomicBool::new(false);
        let inventory = capture(&source, OsStr::new("file"), &flag).unwrap();
        let copy = copy_into(inventory, &source, stage, &flag, |_| {}).unwrap();
        let failed = copy
            .cleanup_with(|| Err(io::Error::other("sync failed after unlink")))
            .unwrap_err();
        assert!(!d.join("stage/file").exists());
        assert_eq!(failed.partial.created_count(), 1);
        assert!(failed.partial.created[0].pending_parent.is_some());
        fs::write(d.join("stage/file"), b"foreign").unwrap();
        let failed = failed.partial.cleanup().unwrap_err();
        assert_eq!(failed.partial.created_count(), 0);
        assert_eq!(fs::read(d.join("stage/file")).unwrap(), b"foreign");
        assert_eq!(fs::read(d.join("source/file")).unwrap(), b"source");
        fs::remove_file(d.join("stage/file")).unwrap();
        failed.partial.cleanup().map_err(|e| e.cause).unwrap();
        fs::remove_dir_all(d).unwrap();
    }
    #[test]
    fn preflight_node_cap_external_hardlinks_nonempty_stage_and_corruption_keep_source() {
        let (d, source, stage) = fixture("exdev-copy-preflight");
        fs::create_dir(d.join("source/tree")).unwrap();
        for n in 0..MAX_PINNED_NODES {
            fs::write(d.join(format!("source/tree/f{n}")), b"x").unwrap();
        }
        let flag = AtomicBool::new(false);
        let inventory = capture(&source, OsStr::new("tree"), &flag).unwrap();
        let failed = copy_into(inventory, &source, stage, &flag, |_| {}).unwrap_err();
        assert_eq!(failed.partial.created_count(), 0);
        assert_eq!(fs::read_dir(d.join("stage")).unwrap().count(), 0);
        drop(failed);
        fs::write(d.join("source/file"), b"source").unwrap();
        fs::hard_link(d.join("source/file"), d.join("alias")).unwrap();
        let inventory = capture(&source, OsStr::new("file"), &flag).unwrap();
        let failed = copy_into(
            inventory,
            &source,
            File::open(d.join("stage")).unwrap(),
            &flag,
            |_| {},
        )
        .unwrap_err();
        assert_eq!(failed.partial.created_count(), 0);
        drop(failed);
        fs::remove_file(d.join("alias")).unwrap();
        fs::write(d.join("stage/foreign"), b"preserve").unwrap();
        let inventory = capture(&source, OsStr::new("file"), &flag).unwrap();
        let failed = copy_into(
            inventory,
            &source,
            File::open(d.join("stage")).unwrap(),
            &flag,
            |_| {},
        )
        .unwrap_err();
        assert_eq!(failed.partial.created_count(), 0);
        assert_eq!(fs::read(d.join("stage/foreign")).unwrap(), b"preserve");
        drop(failed);
        fs::remove_file(d.join("stage/foreign")).unwrap();
        let inventory = capture(&source, OsStr::new("file"), &flag).unwrap();
        let mut copy = copy_into(
            inventory,
            &source,
            File::open(d.join("stage")).unwrap(),
            &flag,
            |_| {},
        )
        .unwrap();
        fs::write(d.join("stage/file"), b"corrupt").unwrap();
        assert!(copy.verify_and_sync(&flag).is_err());
        assert!(!copy.verified());
        assert_eq!(fs::read(d.join("source/file")).unwrap(), b"source");
        drop(copy);
        fs::remove_dir_all(d).unwrap();
    }
    #[test]
    fn publication_uses_final_parent_for_verification_and_blocks_cleanup() {
        let (d, source, stage) = fixture("exdev-copy-published");
        fs::create_dir(d.join("published")).unwrap();
        fs::write(d.join("source/file"), b"payload").unwrap();
        let flag = AtomicBool::new(false);
        let inventory = capture(&source, OsStr::new("file"), &flag).unwrap();
        let mut copy = copy_into(inventory, &source, stage, &flag, |_| {}).unwrap();
        let previous = copy.pinned_fds();
        fs::rename(d.join("stage/file"), d.join("published/file")).unwrap();
        copy.note_publication(File::open(d.join("published")).unwrap())
            .unwrap();
        assert_eq!(copy.pinned_fds(), previous + 1);
        copy.verify_and_sync(&flag).unwrap();
        let failed = copy.cleanup().unwrap_err();
        assert_eq!(fs::read(d.join("published/file")).unwrap(), b"payload");
        assert_eq!(fs::read(d.join("source/file")).unwrap(), b"payload");
        drop(failed);
        fs::remove_dir_all(d).unwrap();
    }
}
