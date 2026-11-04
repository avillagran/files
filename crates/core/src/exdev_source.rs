//! Effective EXDEV inventory and opt-in Linux sync namespace primitives.
//! FD-anchored no-follow traversal with bounded names/metadata and cooperative
//! checkpoints. Sync staging barriers and journal-bound renames are explicit;
//! snapshots alone never authorize publication, deletion or rollback.
#[cfg(target_os = "linux")]
mod linux {
    use crate::copy_engine::Control;
    use crate::move_manifest::{
        AccessTime, Entry, Identity, Kind, MAX_ENTRIES, MAX_PATH, MAX_WIRE, Revision,
    };
    use sha2::{Digest, Sha256};
    use std::{
        collections::HashMap,
        ffi::{CString, OsStr},
        fs::{self, File},
        io::{self, Read, Seek, SeekFrom},
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::{
                ffi::{OsStrExt, OsStringExt},
                fs::MetadataExt,
            },
        },
        sync::atomic::{AtomicBool, Ordering},
    };
    const MAX_ATTRIBUTE_BYTES: usize = 128 * 1024;
    const MAX_ATTRIBUTES: usize = 64;
    const MAX_NAME_WORK: usize = 1024 * 1024;
    const BUFFER: usize = 64 * 1024;
    /// A final union of two bounded inputs, excluding neither root's children.
    pub const MAX_NAMESPACE_ENTRIES: usize = crate::directory_sync_admission::MAX_SCOPE_ENTRIES;
    pub const MAX_NAMESPACE_NAMES: usize = crate::directory_sync_admission::MAX_SCOPE_NAMES;
    #[derive(Clone, Copy)]
    struct SyncBounds {
        read: u64,
        prefix: usize,
        namespace: bool,
    }
    impl SyncBounds {
        fn count(self) -> usize {
            if self.namespace {
                MAX_NAMESPACE_ENTRIES
            } else {
                MAX_ENTRIES + 1
            }
        }
        fn names(self) -> usize {
            if self.namespace {
                MAX_NAMESPACE_NAMES
            } else {
                crate::directory_compare::MAX_NAMES
            }
        }
        fn factor(self) -> usize {
            if self.namespace { 4 } else { 2 }
        }
    }
    pub struct Inventory {
        root: File,
        entries: Vec<Entry>,
        sync: Option<SyncBounds>,
        mounts: Option<Vec<u64>>,
        payload_read: u64,
    }
    impl std::fmt::Debug for Inventory {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("Inventory")
                .field("entries", &self.entries.len())
                .finish_non_exhaustive()
        }
    }
    impl Inventory {
        pub fn entries(&self) -> &[Entry] {
            &self.entries
        }
        pub(crate) fn mounts(&self) -> Option<&[u64]> {
            self.mounts.as_deref()
        }
        pub(crate) fn payload_read(&self) -> u64 {
            self.payload_read
        }
        pub(crate) fn fork_metadata(
            &self,
            parent: &File,
            name: &OsStr,
            read: u64,
            cancel: &AtomicBool,
            control: Option<&Control>,
        ) -> io::Result<(Self, u64)> {
            let spent = self.verify_staging(parent, name, read, cancel, control)?;
            let root = self.root.try_clone()?;
            let entries = self
                .entries
                .iter()
                .map(|e| Entry {
                    relative: e.relative.clone(),
                    identity: e.identity,
                    revision: e.revision,
                    access: e.access,
                    content_digest: e.content_digest,
                    metadata_digest: e.metadata_digest,
                })
                .collect();
            check(cancel)?;
            Ok((
                Self {
                    root,
                    entries,
                    sync: self.sync,
                    mounts: self.mounts.clone(),
                    payload_read: self.payload_read,
                },
                spent,
            ))
        }
        pub fn root(&self) -> &File {
            &self.root
        }
        pub(crate) fn rebase_after_own_root_rename(
            &mut self,
            parent: &File,
            name: &OsStr,
            cancel: &AtomicBool,
        ) -> io::Result<()> {
            self.rebase_inner(parent, name, cancel, None)
        }
        pub(crate) fn rebase_controlled(
            &mut self,
            parent: &File,
            name: &OsStr,
            control: &Control,
        ) -> io::Result<()> {
            self.rebase_inner(parent, name, control.cancellation_flag(), Some(control))
        }
        fn rebase_inner(
            &mut self,
            parent: &File,
            name: &OsStr,
            cancel: &AtomicBool,
            control: Option<&Control>,
        ) -> io::Result<()> {
            if let Some(bounds) = self.sync {
                return if control.is_none() {
                    self.rebase_for_sync(parent, name, bounds.read, cancel)
                } else {
                    self.rebase_sync_inner(parent, name, bounds.read, cancel, control)
                        .map(|change| change.payload_read())
                }
                .map(|_| ());
            }
            if self.entries.len() != 1
                || self.entries[0].identity.kind != Kind::File
                || self.entries[0].revision.links != 1
            {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "Rebase inicial sólo archivo sin hardlinks",
                ));
            }
            let mut observed = capture_inner(parent, name, cancel, control)?;
            if id(&observed.root.metadata()?)? != id(&self.root.metadata()?)? {
                return Err(invalid("Raíz de staging sustituida"));
            }
            let revision = observed.entries[0].revision;
            if !revision.matches_after_rename(self.entries[0].revision) {
                return Err(invalid(
                    "Revisión de staging no corresponde a rename propio",
                ));
            }
            observed.entries[0].revision = self.entries[0].revision;
            if observed.entries != self.entries {
                return Err(invalid("Contenido/metadatos cambiaron durante staging"));
            }
            self.entries[0].revision = revision;
            Ok(())
        }
        /// Read-only reconciliation AFTER a positively identified rename by the
        /// caller. This does not prove who renamed it or grant effect authority.
        /// Only the root ctime may differ; every descendant and mount is exact.
        /// Failure preserves the original inventory; do not retry as if fresh.
        pub(crate) fn rebase_for_sync(
            &mut self,
            parent: &File,
            name: &OsStr,
            read: u64,
            cancel: &AtomicBool,
        ) -> io::Result<u64> {
            self.reconcile_root_rename(parent, name, read, cancel)
                .map(|change| change.payload_read())
        }
        /// Read-only exact transition evidence after caller-identified own rename.
        /// It is not provenance, durability, human approval or effect authority.
        pub fn reconcile_root_rename(
            &mut self,
            parent: &File,
            name: &OsStr,
            read: u64,
            cancel: &AtomicBool,
        ) -> io::Result<RenameChange> {
            self.rebase_sync_inner(parent, name, read, cancel, None)
        }
        fn rebase_sync_inner(
            &mut self,
            parent: &File,
            name: &OsStr,
            read: u64,
            cancel: &AtomicBool,
            control: Option<&Control>,
        ) -> io::Result<RenameChange> {
            let bounds = self
                .sync
                .ok_or_else(|| invalid("Rebase requiere inventario sync"))?;
            if read > crate::directory_compare::MAX_READ {
                return Err(invalid("Presupuesto de rebase inválido"));
            }
            component(name.as_bytes())?;
            check_control(cancel, control)?;
            let mounts = self
                .mounts
                .as_ref()
                .ok_or_else(|| invalid("Rebase sin montajes"))?;
            if mounts.len() != self.entries.len() || self.entries.is_empty() {
                return Err(invalid("Inventario incompleto"));
            }
            let root_entry = &self.entries[0];
            let root = open(parent, name)?;
            if id(&root.metadata()?)? != root_entry.identity
                || id(&self.root.metadata()?)? != root_entry.identity
            {
                return Err(invalid("Raíz renombrada sustituida"));
            }
            // Allocate every replacement path before changing the owner. The
            // suffixes, not display names or decoded strings, define identity.
            let mut paths = Vec::with_capacity(self.entries.len());
            for e in &self.entries {
                let suffix = e
                    .relative
                    .get(bounds.prefix..)
                    .ok_or_else(|| invalid("Prefijo inválido"))?;
                if !suffix.is_empty() && suffix[0] != b'/' {
                    return Err(invalid("Prefijo de hijo inválido"));
                }
                let mut path = name.as_bytes().to_vec();
                path.extend_from_slice(suffix);
                paths.push(path);
            }
            let expected: HashMap<&[u8], &Entry> = paths
                .iter()
                .zip(&self.entries)
                .map(|(p, e)| (p.as_slice(), e))
                .collect();
            let mut count = 0;
            let mut root_revision = None;
            let mut walker = Walker::with_control(cancel, Some(&expected), control, |observed| {
                let old = expected
                    .get(observed.relative.as_slice())
                    .ok_or_else(|| invalid("Nombre externo durante rename"))?;
                let is_root = observed.relative == name.as_bytes();
                if observed.identity != old.identity
                    || observed.access != old.access
                    || observed.content_digest != old.content_digest
                    || observed.metadata_digest != old.metadata_digest
                    || if is_root {
                        !observed.revision.matches_after_rename(old.revision)
                    } else {
                        observed.revision != old.revision
                    }
                {
                    return Err(invalid("Objeto cambió además del rename propio"));
                }
                if is_root {
                    root_revision = Some(observed.revision);
                }
                count += 1;
                Ok(())
            });
            walker.set_sync(Some(SyncBounds {
                read,
                prefix: name.as_bytes().len(),
                ..bounds
            }));
            walker.walk(parent, name, root, name.as_bytes(), 0)?;
            let payload = walker.payload_read;
            let topology = walker
                .topology
                .take()
                .ok_or_else(|| invalid("Rebase sin topología"))?;
            drop(walker);
            if count != self.entries.len()
                || paths
                    .iter()
                    .zip(mounts)
                    .any(|(p, m)| topology.get(p.as_slice()) != Some(m))
            {
                return Err(invalid("Namespace/montaje cambió durante rename"));
            }
            let revision = root_revision.ok_or_else(|| invalid("Raíz no observada"))?;
            stable(parent, name, &self.root, root_entry.identity, revision)?;
            if mount_id(&self.root, None)? != mounts[0]
                || mount_id(parent, Some(name))? != mounts[0]
            {
                return Err(invalid("Montaje raíz cambió"));
            }
            check_control(cancel, control)?;
            let change = RenameChange {
                identity: root_entry.identity,
                before: root_entry.revision,
                after: revision,
                access: root_entry.access,
                content: root_entry.content_digest,
                metadata: root_entry.metadata_digest,
                mount: mounts[0],
                payload,
            };
            // No fallible operation remains. Original root FD/mount evidence,
            // child revisions and original read accounting stay owned unchanged.
            for (e, path) in self.entries.iter_mut().zip(paths) {
                e.relative = path;
            }
            self.entries[0].revision = revision;
            self.sync = Some(SyncBounds {
                prefix: name.as_bytes().len(),
                ..bounds
            });
            Ok(change)
        }
        pub fn into_parts(self) -> (File, Vec<Entry>) {
            (self.root, self.entries)
        }
        /// Rewalk and compare against captured observations without retaining a
        /// second owned inventory. Call on worker, never draw/UI. No allowance
        /// for our future mutations here: ledger updates are separate work.
        pub fn verify(&self, parent: &File, name: &OsStr, cancel: &AtomicBool) -> io::Result<()> {
            self.verify_inner(parent, name, cancel, None)
        }
        pub(crate) fn verify_controlled(
            &self,
            parent: &File,
            name: &OsStr,
            control: &Control,
        ) -> io::Result<()> {
            self.verify_inner(parent, name, control.cancellation_flag(), Some(control))
        }
        fn verify_inner(
            &self,
            parent: &File,
            name: &OsStr,
            cancel: &AtomicBool,
            control: Option<&Control>,
        ) -> io::Result<()> {
            self.verify_observed(parent, name, cancel, control, None)
                .map(|_| ())
        }
        pub(crate) fn verify_for_sync(
            &self,
            parent: &File,
            name: &OsStr,
            read: u64,
            cancel: &AtomicBool,
        ) -> io::Result<u64> {
            if self.sync.is_none() || read > crate::directory_compare::MAX_READ {
                return Err(invalid("Verificación de sincronización inválida"));
            }
            self.verify_observed(parent, name, cancel, None, Some(read))
        }
        fn verify_observed(
            &self,
            parent: &File,
            name: &OsStr,
            cancel: &AtomicBool,
            control: Option<&Control>,
            read: Option<u64>,
        ) -> io::Result<u64> {
            component(name.as_bytes())?;
            check_control(cancel, control)?;
            let expected: HashMap<&[u8], &Entry> = self
                .entries
                .iter()
                .map(|e| (e.relative.as_slice(), e))
                .collect();
            let mut at = 0usize;
            let mut walker = Walker::with_control(cancel, Some(&expected), control, |entry| {
                if expected.get(entry.relative.as_slice()).copied() != Some(&entry) {
                    return Err(invalid("Origen cambió respecto del inventario"));
                }
                at += 1;
                Ok(())
            });
            walker.set_sync(self.sync.map(|mut b| {
                if let Some(read) = read {
                    b.read = read;
                }
                b
            }));
            let root = open(parent, name)?;
            if id(&root.metadata()?)? != id(&self.root.metadata()?)? {
                return Err(invalid("Raíz sustituida"));
            }
            walker.walk(parent, name, root, name.as_bytes(), 0)?;
            if let Some(mounts) = &self.mounts {
                let observed = walker
                    .topology
                    .as_ref()
                    .ok_or(invalid("Topología ausente"))?;
                for (entry, &mount) in self.entries.iter().zip(mounts) {
                    if observed.get(&entry.relative) != Some(&mount) {
                        return Err(invalid("Montaje cambió respecto del inventario"));
                    }
                }
            }
            let payload_read = walker.payload_read;
            drop(walker);
            if at != self.entries.len() {
                return Err(invalid("Faltan entradas del inventario"));
            }
            Ok(payload_read)
        }
    }
    pub fn capture(parent: &File, name: &OsStr, cancel: &AtomicBool) -> io::Result<Inventory> {
        capture_inner(parent, name, cancel, None)
    }
    pub(crate) fn capture_for_sync(
        parent: &File,
        name: &OsStr,
        read: u64,
        cancel: &AtomicBool,
    ) -> io::Result<Inventory> {
        if read > crate::directory_compare::MAX_READ {
            return Err(invalid("Presupuesto de captura inválido"));
        }
        capture_options(
            parent,
            name,
            cancel,
            None,
            Some(SyncBounds {
                read,
                prefix: name.as_bytes().len(),
                namespace: false,
            }),
        )
    }
    pub(crate) fn capture_controlled(
        parent: &File,
        name: &OsStr,
        control: &Control,
    ) -> io::Result<Inventory> {
        capture_inner(parent, name, control.cancellation_flag(), Some(control))
    }
    fn capture_inner(
        parent: &File,
        name: &OsStr,
        cancel: &AtomicBool,
        control: Option<&Control>,
    ) -> io::Result<Inventory> {
        capture_options(parent, name, cancel, control, None)
    }
    fn capture_options(
        parent: &File,
        name: &OsStr,
        cancel: &AtomicBool,
        control: Option<&Control>,
        sync: Option<SyncBounds>,
    ) -> io::Result<Inventory> {
        component(name.as_bytes())?;
        check(cancel)?;
        let root = open(parent, name)?;
        if sync.is_some_and(|b| b.namespace) && id(&root.metadata()?)?.kind != Kind::Directory {
            return Err(invalid("Namespace requiere raíz directorio"));
        }
        let pin = root.try_clone()?;
        let mut entries = Vec::new();
        let mut walker = Walker::with_control(cancel, None, control, |entry| {
            if entries.len() == entries.capacity() {
                let extra = (entries.capacity().max(32))
                    .min(sync.map_or(MAX_ENTRIES, SyncBounds::count) - entries.len());
                entries
                    .try_reserve_exact(extra)
                    .map_err(|_| invalid("Sin memoria para inventario"))?;
            }
            entries.push(entry);
            Ok(())
        });
        walker.set_sync(sync);
        walker.walk(parent, name, root, name.as_bytes(), 0)?;
        let payload_read = walker.payload_read;
        let mut topology = walker.topology.take();
        drop(walker);
        entries.sort_unstable_by(|a, b| a.relative.cmp(&b.relative));
        let mounts = topology
            .as_mut()
            .map(|map| {
                entries
                    .iter()
                    .map(|e| {
                        map.remove(&e.relative)
                            .ok_or_else(|| invalid("Topología incompleta o nombre duplicado"))
                    })
                    .collect::<io::Result<Vec<_>>>()
            })
            .transpose()?;
        Ok(Inventory {
            root: pin,
            entries,
            sync,
            mounts,
            payload_read,
        })
    }
    fn mount_id(file: &File, name: Option<&OsStr>) -> io::Result<u64> {
        let name = match name {
            Some(n) => c(n.as_bytes())?,
            None => c"".to_owned(),
        };
        let mut value = std::mem::MaybeUninit::<libc::statx>::zeroed();
        let flags = libc::AT_SYMLINK_NOFOLLOW
            | if name.as_bytes().is_empty() {
                libc::AT_EMPTY_PATH
            } else {
                0
            };
        let rc = unsafe {
            libc::statx(
                file.as_raw_fd(),
                name.as_ptr(),
                flags,
                libc::STATX_MNT_ID,
                value.as_mut_ptr(),
            )
        };
        if rc != 0 {
            return Err(io::Error::last_os_error());
        }
        let value = unsafe { value.assume_init() };
        if value.stx_mask & libc::STATX_MNT_ID == 0 || value.stx_mnt_id == 0 {
            return Err(invalid("Identidad de montaje no disponible"));
        }
        Ok(value.stx_mnt_id)
    }
    fn invalid(s: &'static str) -> io::Error {
        io::Error::new(io::ErrorKind::InvalidData, s)
    }
    fn check(c: &AtomicBool) -> io::Result<()> {
        if c.load(Ordering::Acquire) {
            Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "Inventario cancelado",
            ))
        } else {
            Ok(())
        }
    }
    fn check_control(cancel: &AtomicBool, control: Option<&Control>) -> io::Result<()> {
        check(cancel)?;
        if let Some(control) = control {
            control.checkpoint()?;
        }
        Ok(())
    }
    fn component(p: &[u8]) -> io::Result<()> {
        if p.is_empty()
            || p.len() > 255
            || p == b"."
            || p == b".."
            || p.contains(&0)
            || p.contains(&b'/')
        {
            Err(invalid("Componente inválido"))
        } else {
            Ok(())
        }
    }
    fn c(p: &[u8]) -> io::Result<CString> {
        CString::new(p).map_err(|_| invalid("NUL en ruta"))
    }
    fn owned(fd: i32) -> io::Result<File> {
        if fd < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(unsafe { File::from_raw_fd(fd) })
        }
    }
    fn open(parent: &File, name: &OsStr) -> io::Result<File> {
        component(name.as_bytes())?;
        // O_PATH pins any kind first; never opens a FIFO/device for data.
        owned(unsafe {
            libc::openat(
                parent.as_raw_fd(),
                c(name.as_bytes())?.as_ptr(),
                libc::O_PATH | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        })
    }
    fn data(parent: &File, name: &OsStr, expected: Identity, directory: bool) -> io::Result<File> {
        let flags = libc::O_RDONLY
            | libc::O_NOFOLLOW
            | libc::O_NONBLOCK
            | libc::O_CLOEXEC
            | libc::O_NOATIME
            | if directory { libc::O_DIRECTORY } else { 0 };
        let file = owned(unsafe {
            libc::openat(parent.as_raw_fd(), c(name.as_bytes())?.as_ptr(), flags)
        })?;
        if id(&file.metadata()?)? != expected {
            return Err(invalid("Entrada sustituida al abrir datos"));
        }
        Ok(file)
    }
    pub(crate) fn id(m: &fs::Metadata) -> io::Result<Identity> {
        let kind = if m.is_file() {
            Kind::File
        } else if m.is_dir() {
            Kind::Directory
        } else if m.file_type().is_symlink() {
            Kind::Symlink
        } else {
            return Err(invalid("Tipo especial no admitido en EXDEV"));
        };
        Ok(Identity {
            device: m.dev(),
            inode: m.ino(),
            kind,
        })
    }
    pub(crate) fn revision(m: &fs::Metadata) -> io::Result<Revision> {
        let ns = |v: i64| {
            u32::try_from(v)
                .ok()
                .filter(|n| *n < 1_000_000_000)
                .ok_or(invalid("Timestamp inválido"))
        };
        Ok(Revision {
            size: m.size(),
            mtime_seconds: m.mtime(),
            mtime_nanos: ns(m.mtime_nsec())?,
            ctime_seconds: m.ctime(),
            ctime_nanos: ns(m.ctime_nsec())?,
            mode: m.mode() & 0o7777,
            uid: m.uid(),
            gid: m.gid(),
            links: m.nlink(),
        })
    }
    fn entry_at(parent: &File, name: &OsStr) -> io::Result<fs::Metadata> {
        fs::symlink_metadata(
            std::path::Path::new(&format!("/proc/self/fd/{}", parent.as_raw_fd())).join(name),
        )
    }
    fn stable(
        parent: &File,
        name: &OsStr,
        file: &File,
        identity: Identity,
        expected: Revision,
    ) -> io::Result<()> {
        let fd = file.metadata()?;
        let named = entry_at(parent, name)?;
        if id(&fd)? != identity
            || id(&named)? != identity
            || revision(&fd)? != expected
            || revision(&named)? != expected
        {
            return Err(invalid("Fuente cambió durante inventario"));
        }
        Ok(())
    }
    #[cfg(test)]
    fn hash_exact(
        reader: &mut impl Read,
        size: u64,
        scratch: &mut [u8],
        cancel: &AtomicBool,
    ) -> io::Result<[u8; 32]> {
        hash_exact_controlled(reader, size, scratch, cancel, None)
    }
    fn hash_exact_controlled(
        reader: &mut impl Read,
        mut size: u64,
        scratch: &mut [u8],
        cancel: &AtomicBool,
        control: Option<&Control>,
    ) -> io::Result<[u8; 32]> {
        let mut hash = Sha256::new();
        while size > 0 {
            check_control(cancel, control)?;
            let length = size.min(scratch.len() as u64) as usize;
            let n = reader.read(&mut scratch[..length])?;
            if n == 0 {
                return Err(invalid("Archivo truncado durante hash"));
            }
            hash.update(&scratch[..n]);
            size -= n as u64;
        }
        check_control(cancel, control)?;
        if reader.read(&mut [0u8; 1])? != 0 {
            return Err(invalid("Archivo creció durante hash"));
        }
        Ok(hash.finalize().into())
    }
    fn sized(h: &mut Sha256, p: &[u8]) {
        h.update((p.len() as u64).to_le_bytes());
        h.update(p);
    }
    // Source may contain arbitrary attribute values. Names/values are hashed in
    // canonical byte order; ACL/security/capability attrs are included, not ignored.
    fn attributes(
        file: &File,
        named: Option<&CString>,
        cancel: &AtomicBool,
    ) -> io::Result<[u8; 32]> {
        attributes_with(file, named, cancel, |_, _| Ok(()))
    }
    fn attributes_with(
        file: &File,
        named: Option<&CString>,
        cancel: &AtomicBool,
        mut consume: impl FnMut(CString, Vec<u8>) -> io::Result<()>,
    ) -> io::Result<[u8; 32]> {
        let list = |ptr: *mut libc::c_char, len: usize| unsafe {
            match named {
                Some(path) => libc::llistxattr(path.as_ptr(), ptr, len),
                None => libc::flistxattr(file.as_raw_fd(), ptr, len),
            }
        };
        check(cancel)?;
        let n = list(std::ptr::null_mut(), 0);
        if n < 0 {
            return Err(io::Error::last_os_error());
        }
        let n = n as usize;
        if n > MAX_ATTRIBUTE_BYTES {
            return Err(invalid("Nombres xattr exceden límite"));
        }
        let mut names = vec![0u8; n];
        let read = list(names.as_mut_ptr().cast(), n);
        if read < 0 {
            return Err(io::Error::last_os_error());
        }
        if read as usize != n {
            return Err(invalid("xattrs cambiaron durante lectura"));
        }
        let count = names
            .split(|b| *b == 0)
            .filter(|s| !s.is_empty())
            .take(MAX_ATTRIBUTES + 1)
            .count();
        if count > MAX_ATTRIBUTES || (!names.is_empty() && names.last() != Some(&0)) {
            return Err(invalid("Catálogo xattr inválido o excesivo"));
        }
        let mut fields: Vec<&[u8]> = names.split(|b| *b == 0).filter(|s| !s.is_empty()).collect();
        fields.sort_unstable();
        let mut total = n;
        let mut h = Sha256::new();
        let mut previous: Option<&[u8]> = None;
        for field in fields {
            check(cancel)?;
            if previous == Some(field) {
                return Err(invalid("xattr duplicado"));
            }
            previous = Some(field);
            let field_c = c(field)?;
            let get = |ptr: *mut libc::c_void, len: usize| unsafe {
                match named {
                    Some(path) => libc::lgetxattr(path.as_ptr(), field_c.as_ptr(), ptr, len),
                    None => libc::fgetxattr(file.as_raw_fd(), field_c.as_ptr(), ptr, len),
                }
            };
            let n = get(std::ptr::null_mut(), 0);
            if n < 0 {
                return Err(io::Error::last_os_error());
            }
            let n = n as usize;
            total = total
                .checked_add(n)
                .filter(|n| *n <= MAX_ATTRIBUTE_BYTES)
                .ok_or(invalid("Valores xattr exceden límite"))?;
            let mut value = vec![0u8; n];
            let read = get(value.as_mut_ptr().cast(), n);
            if read < 0 {
                return Err(io::Error::last_os_error());
            }
            if read as usize != n {
                return Err(invalid("xattr cambió durante lectura"));
            }
            sized(&mut h, field);
            sized(&mut h, &value);
            consume(field_c, value)?;
        }
        Ok(h.finalize().into())
    }
    fn metadata_digest(m: &fs::Metadata, access: AccessTime, attrs: [u8; 32]) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update((m.mode() & 0o7777).to_le_bytes());
        h.update(m.uid().to_le_bytes());
        h.update(m.gid().to_le_bytes());
        h.update(access.seconds.to_le_bytes());
        h.update(access.nanos.to_le_bytes());
        h.update(m.mtime().to_le_bytes());
        h.update(m.mtime_nsec().to_le_bytes());
        h.update(attrs);
        h.finalize().into()
    }
    struct DirectoryStream(*mut libc::DIR);
    impl DirectoryStream {
        fn new(file: &File) -> io::Result<Self> {
            let fd = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 3) };
            if fd < 0 {
                return Err(io::Error::last_os_error());
            }
            let stream = unsafe { libc::fdopendir(fd) };
            if stream.is_null() {
                let error = io::Error::last_os_error();
                unsafe { libc::close(fd) };
                return Err(error);
            }
            unsafe {
                libc::rewinddir(stream);
            }
            Ok(Self(stream))
        }
        fn next(&mut self) -> io::Result<Option<&[u8]>> {
            loop {
                unsafe { *libc::__errno_location() = 0 };
                let entry = unsafe { libc::readdir(self.0) };
                if entry.is_null() {
                    let error = io::Error::last_os_error();
                    return if error.raw_os_error() == Some(0) {
                        Ok(None)
                    } else {
                        Err(error)
                    };
                }
                let name = unsafe { std::ffi::CStr::from_ptr((*entry).d_name.as_ptr()) }.to_bytes();
                if name != b"." && name != b".." {
                    return Ok(Some(name));
                }
            }
        }
    }
    impl Drop for DirectoryStream {
        fn drop(&mut self) {
            unsafe { libc::closedir(self.0) };
        }
    }
    struct Group {
        kind: Kind,
        revision: Revision,
        access: AccessTime,
        content: [u8; 32],
        metadata: [u8; 32],
        seen: u64,
    }
    struct Walker<'a, 'b, F> {
        cancel: &'a AtomicBool,
        control: Option<&'a Control>,
        expected: Option<&'b HashMap<&'b [u8], &'b Entry>>,
        emit: F,
        count: usize,
        wire: usize,
        name_work: usize,
        pending_count: usize,
        groups: HashMap<(u64, u64), Group>,
        scratch: Vec<u8>,
        sync: Option<SyncBounds>,
        topology: Option<HashMap<Vec<u8>, u64>>,
        payload_read: u64,
        names_retained: usize,
    }
    impl<'a, 'b, F: FnMut(Entry) -> io::Result<()>> Walker<'a, 'b, F> {
        fn with_control(
            cancel: &'a AtomicBool,
            expected: Option<&'b HashMap<&'b [u8], &'b Entry>>,
            control: Option<&'a Control>,
            emit: F,
        ) -> Self {
            Self {
                cancel,
                control,
                expected,
                emit,
                count: 0,
                wire: 0,
                name_work: 0,
                pending_count: 0,
                groups: HashMap::new(),
                scratch: vec![0; BUFFER],
                sync: None,
                topology: None,
                payload_read: 0,
                names_retained: 0,
            }
        }
        fn set_sync(&mut self, bounds: Option<SyncBounds>) {
            self.sync = bounds;
            self.topology = bounds.map(|_| HashMap::new());
        }
        fn max_count(&self) -> usize {
            self.sync.map_or(MAX_ENTRIES, SyncBounds::count)
        }
        fn max_path(&self) -> usize {
            MAX_PATH + self.sync.map_or(0, |b| b.prefix + 1)
        }
        fn max_names(&self) -> usize {
            if self.sync.is_some_and(|b| b.namespace) {
                crate::directory_sync_admission::MAX_SCOPE_PENDING
            } else {
                MAX_NAME_WORK * self.sync.map_or(1, SyncBounds::factor)
            }
        }
        fn walk(
            &mut self,
            parent: &File,
            name: &OsStr,
            pin: File,
            relative: &[u8],
            depth: usize,
        ) -> io::Result<()> {
            check_control(self.cancel, self.control)?;
            if depth
                >= if self.sync.is_some() {
                    crate::directory_sync_admission::MAX_SCOPE_DEPTH + 1
                } else {
                    64
                }
                || relative.len() > self.max_path()
            {
                return Err(invalid("Profundidad/ruta excede límite EXDEV"));
            }
            self.count += 1;
            self.wire = self
                .wire
                .checked_add(147 + relative.len())
                .filter(|n| {
                    *n <= if self.sync.is_some_and(|b| b.namespace) {
                        crate::directory_sync_admission::MAX_SCOPE_WIRE
                    } else {
                        MAX_WIRE * self.sync.map_or(1, SyncBounds::factor) - 16384
                    }
                })
                .ok_or(invalid("Manifiesto excede presupuesto"))?;
            if let Some(bounds) = self.sync {
                self.names_retained = self
                    .names_retained
                    .checked_add(if depth == 0 {
                        0
                    } else {
                        relative.len() - bounds.prefix - 1
                    })
                    .filter(|n| *n <= bounds.names())
                    .ok_or(invalid("Nombres de sincronización exceden límite"))?;
            }
            if self.count > self.max_count() {
                return Err(invalid("Demasiadas entradas EXDEV"));
            }
            let mount = if self.sync.is_some() {
                let mount = mount_id(&pin, None)?;
                if mount_id(parent, Some(name))? != mount {
                    return Err(invalid("Montaje sustituido al capturar"));
                }
                Some(mount)
            } else {
                None
            };
            let before = pin.metadata()?;
            let identity = id(&before)?;
            let expected = revision(&before)?;
            // NOATIME requires owner/CAP_FOWNER. Don't silently alter timestamps
            // or weaken comparison for sources we cannot observe equivalently.
            if before.uid() != unsafe { libc::geteuid() } {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "Inventario EXDEV requiere fuente propia por ahora",
                ));
            }
            let mut access = AccessTime {
                seconds: before.atime(),
                nanos: u32::try_from(before.atime_nsec())
                    .ok()
                    .filter(|n| *n < 1_000_000_000)
                    .ok_or(invalid("Atime inválido"))?,
            };
            if identity.kind == Kind::Symlink {
                if let Some(entry) = self.expected.and_then(|map| map.get(relative)) {
                    access = entry.access;
                } else if let Some(group) = self.groups.get(&(identity.device, identity.inode)) {
                    access = group.access;
                }
            }
            let mut children = None;
            let (content, attrs) = match identity.kind {
                Kind::File => {
                    let mut file = data(parent, name, identity, false)?;
                    let attrs = attributes(&file, None, self.cancel)?;
                    let digest =
                        if let Some(group) = self.groups.get(&(identity.device, identity.inode)) {
                            if group.revision != expected
                                || group.access != access
                                || group.metadata != metadata_digest(&before, access, attrs)
                            {
                                return Err(invalid("Grupo hardlink cambió"));
                            }
                            group.content
                        } else {
                            let total = self
                                .payload_read
                                .checked_add(before.len())
                                .ok_or(invalid("Presupuesto de lectura excedido"))?;
                            if self.sync.is_some_and(|b| total > b.read) {
                                return Err(invalid(
                                    "Presupuesto de lectura de sincronización excedido",
                                ));
                            }
                            file.seek(SeekFrom::Start(0))?;
                            let digest = hash_exact_controlled(
                                &mut file,
                                before.len(),
                                &mut self.scratch,
                                self.cancel,
                                self.control,
                            )?;
                            if self.sync.is_some() {
                                self.payload_read = total;
                            }
                            digest
                        };
                    stable(parent, name, &file, identity, expected)?;
                    (digest, attrs)
                }
                Kind::Symlink => {
                    let mut target = [0u8; MAX_PATH + 1];
                    let empty = c(b"")?;
                    let n = unsafe {
                        libc::readlinkat(
                            pin.as_raw_fd(),
                            empty.as_ptr(),
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
                    let mut bytes = format!("/proc/self/fd/{}/", parent.as_raw_fd()).into_bytes();
                    bytes.extend_from_slice(name.as_bytes());
                    let named = c(&bytes)?;
                    let attrs = attributes(&pin, Some(&named), self.cancel)?;
                    let mut h = Sha256::new();
                    h.update(&target[..n as usize]);
                    (h.finalize().into(), attrs)
                }
                Kind::Directory => {
                    let file = data(parent, name, identity, true)?;
                    let attrs = attributes(&file, None, self.cancel)?;
                    let mut names = Vec::new();
                    let mut bytes = 0usize;
                    let mut stream = DirectoryStream::new(&file)?;
                    while let Some(raw) = stream.next()? {
                        check_control(self.cancel, self.control)?;
                        component(raw)?;
                        if self.pending_count + self.count >= self.max_count() {
                            return Err(invalid("Demasiados hijos EXDEV"));
                        }
                        if names.len() == names.capacity() {
                            let extra =
                                names.capacity().max(32).min(self.max_count() - names.len());
                            let charge = extra * std::mem::size_of::<std::ffi::OsString>();
                            if self.name_work + bytes + charge > self.max_names() {
                                return Err(invalid("Capacidad de nombres excede presupuesto"));
                            }
                            names
                                .try_reserve_exact(extra)
                                .map_err(|_| invalid("Sin memoria para nombres"))?;
                            bytes += charge;
                        }
                        if self.name_work + bytes + raw.len() > self.max_names() {
                            return Err(invalid("Nombres pendientes exceden presupuesto"));
                        }
                        bytes += raw.len();
                        self.pending_count += 1;
                        names.push(std::ffi::OsString::from_vec(raw.to_vec()));
                    }
                    names.sort_unstable_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
                    let mut h = Sha256::new();
                    for name in &names {
                        sized(&mut h, name.as_bytes());
                    }
                    self.name_work += bytes;
                    children = Some((file, names, bytes));
                    (h.finalize().into(), attrs)
                }
            };
            let meta = metadata_digest(&before, access, attrs);
            if let Some(group) = self.groups.get_mut(&(identity.device, identity.inode)) {
                if identity.kind == Kind::Directory
                    || group.kind != identity.kind
                    || group.revision != expected
                    || group.access != access
                    || group.content != content
                    || group.metadata != meta
                {
                    return Err(invalid("Alias/inventario de inode incoherente"));
                }
                group.seen += 1;
                if group.seen > expected.links {
                    return Err(invalid("Hardlinks internos exceden nlink"));
                }
            } else {
                self.groups.insert(
                    (identity.device, identity.inode),
                    Group {
                        kind: identity.kind,
                        revision: expected,
                        access,
                        content,
                        metadata: meta,
                        seen: 1,
                    },
                );
            }
            stable(parent, name, &pin, identity, expected)?;
            if let Some(mount) = mount {
                if mount_id(&pin, None)? != mount || mount_id(parent, Some(name))? != mount {
                    return Err(invalid("Montaje cambió durante inventario"));
                }
                if self
                    .topology
                    .as_mut()
                    .expect("sync topology")
                    .insert(relative.to_vec(), mount)
                    .is_some()
                {
                    return Err(invalid("Nombre duplicado durante inventario"));
                }
            }
            (self.emit)(Entry {
                relative: relative.to_vec(),
                identity,
                revision: expected,
                access,
                content_digest: content,
                metadata_digest: meta,
            })?;
            if let Some((file, names, bytes)) = children {
                for name in names {
                    self.pending_count -= 1;
                    check_control(self.cancel, self.control)?;
                    let length = relative.len() + 1 + name.as_bytes().len();
                    if length > self.max_path() {
                        return Err(invalid("Ruta descendiente excede límite"));
                    }
                    let mut path = Vec::with_capacity(length);
                    path.extend_from_slice(relative);
                    path.push(b'/');
                    path.extend_from_slice(name.as_bytes());
                    let child = open(&file, &name)?;
                    self.walk(&file, &name, child, &path, depth + 1)?;
                }
                self.name_work -= bytes;
                stable(parent, name, &file, identity, expected)?;
                if let Some(mount) = mount
                    && (mount_id(&pin, None)? != mount || mount_id(parent, Some(name))? != mount)
                {
                    return Err(invalid("Montaje cambió al recorrer directorio"));
                }
            }
            Ok(())
        }
    }
    mod destination;
    mod sync_alias;
    mod sync_namespace;
    pub use sync_namespace::{
        MAX_BATCH_NAMESPACE_ENTRIES, MAX_BATCH_NAMESPACE_NAMES, MAX_BATCH_NAMESPACE_SCOPES,
        MAX_BATCH_NAMESPACE_WIRE, NamespaceFailure, NamespaceMove, NamespacePath, NamespaceScope,
        capture_namespace, capture_namespace_controlled, plan_batch_namespace_move,
        plan_batch_namespace_move_controlled, plan_namespace_move, plan_namespace_move_controlled,
    };
    pub(crate) use sync_namespace::{NamespaceControl, NamespaceLog, OwnPoint};
    mod sync_apply;
    mod sync_create;
    mod sync_meta_batch;
    mod sync_metadata;
    pub(crate) use destination::copy_into_controlled;
    pub use destination::{FailedCopy, MAX_PINNED_NODES, OwnedCopy, copy_into};
    pub use sync_alias::{AliasReport, AliasScope, MAX_ALIAS_ENTRIES, RenameChange};
    pub(crate) use sync_apply::Point as StageMetadataPoint;
    pub use sync_apply::{AppliedStageMetadata, StageMetadataFailure};
    pub(crate) use sync_create::StageInput;
    pub(crate) use sync_meta_batch::MetadataBatch;
    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::move_manifest::Manifest;
        use std::{os::unix::fs::symlink, path::Path, sync::Arc};
        fn fixture(label: &str) -> std::path::PathBuf {
            crate::persistence::test_dir(label)
        }
        fn parent(path: &Path) -> File {
            File::open(path).unwrap()
        }
        #[test]
        fn sync_rebase_complete_tree_raw_prefix_hardlinks_and_symlink_roundtrip() {
            use std::os::unix::ffi::OsStringExt;
            let base = fixture("sync-rebase-tree");
            fs::create_dir(base.join("tree")).unwrap();
            fs::create_dir(base.join("destination")).unwrap();
            fs::create_dir(base.join("tree/empty")).unwrap();
            fs::write(base.join("tree/a"), b"data").unwrap();
            fs::hard_link(base.join("tree/a"), base.join("tree/b")).unwrap();
            symlink("a", base.join("tree/link")).unwrap();
            let old_parent = parent(&base);
            let destination = parent(&base.join("destination"));
            let flag = AtomicBool::new(false);
            let mut inv = capture_for_sync(&old_parent, OsStr::new("tree"), 4, &flag).unwrap();
            let old = inv.entries[0].revision;
            let child_revs: Vec<_> = inv.entries.iter().skip(1).map(|e| e.revision).collect();
            let new = std::ffi::OsString::from_vec(b"long-raw-\xff".to_vec());
            fs::rename(base.join("tree"), base.join("destination").join(&new)).unwrap();
            assert_eq!(
                inv.rebase_for_sync(&destination, &new, 4, &flag).unwrap(),
                4
            );
            assert!(inv.entries[0].revision.matches_after_rename(old));
            assert_eq!(
                inv.entries
                    .iter()
                    .skip(1)
                    .map(|e| e.revision)
                    .collect::<Vec<_>>(),
                child_revs
            );
            inv.verify_for_sync(&destination, &new, 4, &flag).unwrap();
            fs::rename(base.join("destination").join(&new), base.join("z")).unwrap();
            inv.rebase_after_own_root_rename(&old_parent, OsStr::new("z"), &flag)
                .unwrap();
            inv.verify(&old_parent, OsStr::new("z"), &flag).unwrap();
            assert_eq!(inv.payload_read(), 4);
            fs::remove_dir_all(base).unwrap();
        }
        #[test]
        fn sync_rebase_root_hardlink_and_symlink_preserve_full_evidence() {
            for link in [false, true] {
                let base = fixture("sync-rebase-leaf");
                fs::write(base.join("data"), b"data").unwrap();
                if link {
                    symlink("data", base.join("leaf")).unwrap();
                } else {
                    fs::hard_link(base.join("data"), base.join("leaf")).unwrap();
                }
                let parent = parent(&base);
                let flag = AtomicBool::new(false);
                let mut inv = capture_for_sync(&parent, OsStr::new("leaf"), 4, &flag).unwrap();
                let before = inv.entries[0].revision;
                let alias = capture_for_sync(&parent, OsStr::new("data"), 4, &flag).unwrap();
                fs::rename(base.join("leaf"), base.join("new-name")).unwrap();
                inv.rebase_for_sync(&parent, OsStr::new("new-name"), 4, &flag)
                    .unwrap();
                assert_eq!(inv.entries[0].revision.links, before.links);
                inv.verify(&parent, OsStr::new("new-name"), &flag).unwrap();
                if !link {
                    // Other snapshots of this inode do not acquire a blanket
                    // ctime exception. Their cross-scope ledger is separate.
                    assert!(alias.verify(&parent, OsStr::new("data"), &flag).is_err());
                }
                assert_eq!(fs::read(base.join("data")).unwrap(), b"data");
                fs::remove_dir_all(base).unwrap();
            }
        }
        #[test]
        fn sync_rebase_rejects_unrelated_changes_and_failure_never_updates_owner() {
            use std::os::unix::fs::PermissionsExt;
            for case in [
                "content",
                "mode",
                "root-mode",
                "child-ctime",
                "add",
                "remove",
                "nlink",
                "replace",
                "budget",
                "cancel",
                "mount",
            ] {
                let base = fixture("sync-rebase-reject");
                fs::create_dir(base.join("tree")).unwrap();
                fs::write(base.join("tree/a"), b"data").unwrap();
                fs::set_permissions(base.join("tree/a"), fs::Permissions::from_mode(0o644))
                    .unwrap();
                fs::set_permissions(base.join("tree"), fs::Permissions::from_mode(0o755)).unwrap();
                let parent = parent(&base);
                let flag = AtomicBool::new(false);
                let mut inv = capture_for_sync(&parent, OsStr::new("tree"), 4, &flag).unwrap();
                if case == "mount" {
                    inv.mounts.as_mut().unwrap()[1] ^= 1;
                }
                let before = format!("{:?}", inv.entries);
                let prefix = inv.sync.unwrap().prefix;
                fs::rename(base.join("tree"), base.join("renamed")).unwrap();
                match case {
                    "content" => fs::write(base.join("renamed/a"), b"evil").unwrap(),
                    "mode" => fs::set_permissions(
                        base.join("renamed/a"),
                        fs::Permissions::from_mode(0o600),
                    )
                    .unwrap(),
                    "root-mode" => {
                        fs::set_permissions(base.join("renamed"), fs::Permissions::from_mode(0o700))
                            .unwrap()
                    }
                    "child-ctime" => fs::set_permissions(
                        base.join("renamed/a"),
                        fs::Permissions::from_mode(0o644),
                    )
                    .unwrap(),
                    "add" => fs::write(base.join("renamed/new"), b"new").unwrap(),
                    "remove" => fs::remove_file(base.join("renamed/a")).unwrap(),
                    "nlink" => {
                        fs::hard_link(base.join("renamed/a"), base.join("external")).unwrap()
                    }
                    "replace" => {
                        fs::rename(base.join("renamed"), base.join("kept")).unwrap();
                        fs::create_dir(base.join("renamed")).unwrap();
                    }
                    "cancel" => flag.store(true, Ordering::Relaxed),
                    _ => (),
                }
                assert!(
                    inv.rebase_for_sync(
                        &parent,
                        OsStr::new("renamed"),
                        if case == "budget" { 0 } else { 4 },
                        &flag
                    )
                    .is_err(),
                    "{case}"
                );
                assert_eq!(format!("{:?}", inv.entries), before, "{case}");
                assert_eq!(inv.sync.unwrap().prefix, prefix);
                fs::remove_dir_all(base).unwrap();
            }
        }
        #[test]
        fn sync_rebase_full_inventory_keeps_every_child_under_new_prefix() {
            let base = fixture("sync-rebase-full");
            fs::create_dir(base.join("tree")).unwrap();
            for i in 0..crate::directory_compare::MAX_ITEMS {
                fs::write(base.join("tree").join(format!("n{i:05}")), b"").unwrap();
            }
            let parent = parent(&base);
            let flag = AtomicBool::new(false);
            let mut inv = capture_for_sync(&parent, OsStr::new("tree"), 0, &flag).unwrap();
            let count = crate::directory_compare::MAX_ITEMS + 1;
            assert_eq!(inv.entries.len(), count);
            fs::rename(base.join("tree"), base.join("different-prefix")).unwrap();
            assert_eq!(
                inv.rebase_for_sync(&parent, OsStr::new("different-prefix"), 0, &flag)
                    .unwrap(),
                0
            );
            assert_eq!(inv.entries.len(), count);
            assert!(
                inv.entries
                    .iter()
                    .all(|e| e.relative.starts_with(b"different-prefix"))
            );
            inv.verify_for_sync(&parent, OsStr::new("different-prefix"), 0, &flag)
                .unwrap();
            fs::remove_dir_all(base).unwrap();
        }
        #[test]
        fn sync_rebase_is_opt_in_and_cannot_weaken_legacy_tree_policy() {
            let base = fixture("sync-rebase-legacy");
            fs::create_dir(base.join("tree")).unwrap();
            fs::write(base.join("tree/a"), b"data").unwrap();
            let parent = parent(&base);
            let flag = AtomicBool::new(false);
            let mut inv = capture(&parent, OsStr::new("tree"), &flag).unwrap();
            fs::rename(base.join("tree"), base.join("new")).unwrap();
            assert!(
                inv.rebase_for_sync(&parent, OsStr::new("new"), 4, &flag)
                    .is_err()
            );
            assert!(
                inv.rebase_after_own_root_rename(&parent, OsStr::new("new"), &flag)
                    .is_err()
            );
            assert!(inv.mounts.is_none());
            fs::remove_dir_all(base).unwrap();
        }
        #[test]
        fn opt_in_mount_proof_is_checked_but_legacy_capture_has_no_mount_dependency() {
            let base = fixture("sync-mount-proof");
            fs::write(base.join("file"), b"data").unwrap();
            let parent = parent(&base);
            let flag = AtomicBool::new(false);
            let legacy = capture(&parent, OsStr::new("file"), &flag).unwrap();
            assert!(legacy.mounts.is_none());
            legacy.verify(&parent, OsStr::new("file"), &flag).unwrap();
            let mut pinned = capture_for_sync(&parent, OsStr::new("file"), 4, &flag).unwrap();
            assert!(pinned.mounts.as_ref().unwrap()[0] > 0);
            pinned.verify(&parent, OsStr::new("file"), &flag).unwrap();
            pinned.mounts.as_mut().unwrap()[0] ^= 1;
            assert!(pinned.verify(&parent, OsStr::new("file"), &flag).is_err());
            let _ = fs::remove_dir_all(base);
        }
        #[test]
        fn controlled_hash_pauses_between_chunks_and_cancel_wakes_waiter() {
            use std::sync::{Arc, atomic::AtomicUsize, mpsc};
            use std::time::Duration;
            struct Reader {
                control: Control,
                reads: Arc<AtomicUsize>,
                first: Option<mpsc::Sender<()>>,
            }
            impl Read for Reader {
                fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
                    if bytes.len() == 1 {
                        return Ok(0);
                    }
                    bytes.fill(1);
                    self.reads.fetch_add(1, Ordering::SeqCst);
                    if let Some(first) = self.first.take() {
                        self.control.pause();
                        first.send(()).unwrap();
                    }
                    Ok(bytes.len())
                }
            }
            for cancel in [false, true] {
                let control = Control::default();
                let reads = Arc::new(AtomicUsize::new(0));
                let (first_tx, first_rx) = mpsc::channel();
                let (done_tx, done_rx) = mpsc::channel();
                let worker_control = control.clone();
                let worker_reads = reads.clone();
                let worker = std::thread::spawn(move || {
                    let mut reader = Reader {
                        control: worker_control.clone(),
                        reads: worker_reads,
                        first: Some(first_tx),
                    };
                    let result = hash_exact_controlled(
                        &mut reader,
                        (2 * BUFFER) as u64,
                        &mut vec![0; BUFFER],
                        worker_control.cancellation_flag(),
                        Some(&worker_control),
                    );
                    done_tx.send(result).unwrap();
                });
                first_rx.recv_timeout(Duration::from_secs(2)).unwrap();
                assert!(done_rx.recv_timeout(Duration::from_millis(30)).is_err());
                assert_eq!(reads.load(Ordering::SeqCst), 1);
                if cancel {
                    control.cancel();
                } else {
                    control.resume();
                }
                let result = done_rx.recv_timeout(Duration::from_secs(2)).unwrap();
                if cancel {
                    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::Interrupted);
                } else {
                    result.unwrap();
                    assert_eq!(reads.load(Ordering::SeqCst), 2);
                }
                worker.join().unwrap();
            }
        }
        #[test]
        fn traversal_raw_order_content_hardlinks_symlink_and_immediate_verification() {
            let d = fixture("exdev-inventory-order");
            fs::create_dir_all(d.join("tree/a")).unwrap();
            fs::write(d.join("tree/a/z"), b"abc").unwrap();
            fs::write(d.join("tree/a-b"), b"other").unwrap();
            let raw = std::ffi::OsString::from_vec(b"raw-\xff".to_vec());
            fs::write(d.join("tree").join(&raw), b"raw").unwrap();
            fs::hard_link(d.join("tree/a/z"), d.join("tree/hardlink")).unwrap();
            symlink("a-b", d.join("tree/link")).unwrap();
            let p = parent(&d);
            let flag = AtomicBool::new(false);
            let inventory = capture(&p, OsStr::new("tree"), &flag).unwrap();
            let paths: Vec<&[u8]> = inventory
                .entries()
                .iter()
                .map(|e| e.relative.as_slice())
                .collect();
            assert!(paths.windows(2).all(|p| p[0] < p[1]));
            assert!(
                paths.iter().position(|p| *p == b"tree/a-b").unwrap()
                    < paths.iter().position(|p| *p == b"tree/a/z").unwrap()
            );
            let z = inventory
                .entries()
                .iter()
                .find(|e| e.relative == b"tree/a/z")
                .unwrap();
            let hard = inventory
                .entries()
                .iter()
                .find(|e| e.relative == b"tree/hardlink")
                .unwrap();
            assert_eq!(z.identity, hard.identity);
            assert_eq!(z.content_digest, hard.content_digest);
            assert_eq!(
                z.content_digest,
                [
                    0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea, 0x41, 0x41, 0x40, 0xde, 0x5d,
                    0xae, 0x22, 0x23, 0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c, 0xb4, 0x10,
                    0xff, 0x61, 0xf2, 0x00, 0x15, 0xad
                ]
            );
            inventory.verify(&p, OsStr::new("tree"), &flag).unwrap();
            let (_, entries) = inventory.into_parts();
            Manifest::new(
                [1; 16],
                d.as_os_str().as_bytes().to_vec(),
                b"/destination".to_vec(),
                id(&p.metadata().unwrap()).unwrap(),
                Identity {
                    device: 9,
                    inode: 11,
                    kind: Kind::Directory,
                },
                entries,
            )
            .unwrap();
            fs::remove_dir_all(d).unwrap();
        }
        #[test]
        fn atime_observation_is_stable_for_noatime_directory_and_explicit_for_symlink() {
            let d = fixture("exdev-inventory-atime");
            fs::create_dir(d.join("tree")).unwrap();
            fs::write(d.join("tree/file"), b"content").unwrap();
            symlink("file", d.join("tree/link")).unwrap();
            let times = [
                libc::timespec {
                    tv_sec: 100,
                    tv_nsec: 123,
                },
                libc::timespec {
                    tv_sec: 200,
                    tv_nsec: 456,
                },
            ];
            let dir = File::open(d.join("tree")).unwrap();
            assert_eq!(
                unsafe { libc::futimens(dir.as_raw_fd(), times.as_ptr()) },
                0
            );
            let link = c(d.join("tree/link").as_os_str().as_bytes()).unwrap();
            assert_eq!(
                unsafe {
                    libc::utimensat(
                        libc::AT_FDCWD,
                        link.as_ptr(),
                        times.as_ptr(),
                        libc::AT_SYMLINK_NOFOLLOW,
                    )
                },
                0
            );
            let before = fs::symlink_metadata(d.join("tree/link")).unwrap();
            let p = parent(&d);
            let flag = AtomicBool::new(false);
            let inventory = capture(&p, OsStr::new("tree"), &flag).unwrap();
            inventory.verify(&p, OsStr::new("tree"), &flag).unwrap();
            inventory.verify(&p, OsStr::new("tree"), &flag).unwrap();
            let after_dir = dir.metadata().unwrap();
            assert_eq!((after_dir.atime(), after_dir.atime_nsec()), (100, 123));
            let link_entry = inventory
                .entries()
                .iter()
                .find(|e| e.relative == b"tree/link")
                .unwrap();
            assert_eq!(
                link_entry.access,
                AccessTime {
                    seconds: 100,
                    nanos: 123
                }
            );
            let after = fs::symlink_metadata(d.join("tree/link")).unwrap();
            assert_eq!(
                (
                    before.mtime(),
                    before.mtime_nsec(),
                    before.ctime(),
                    before.ctime_nsec()
                ),
                (
                    after.mtime(),
                    after.mtime_nsec(),
                    after.ctime(),
                    after.ctime_nsec()
                )
            );
            assert!(after.atime() >= before.atime()); // kernel may use noatime; no promise of unchanged symlink access time
            fs::remove_dir_all(d).unwrap();
        }
        #[test]
        fn replacement_content_attribute_and_child_changes_are_rejected() {
            let d = fixture("exdev-inventory-change");
            fs::create_dir(d.join("tree")).unwrap();
            fs::write(d.join("tree/file"), b"abc").unwrap();
            let p = parent(&d);
            let flag = AtomicBool::new(false);
            let inventory = capture(&p, OsStr::new("tree"), &flag).unwrap();
            fs::write(d.join("tree/file"), b"abd").unwrap();
            assert!(inventory.verify(&p, OsStr::new("tree"), &flag).is_err());
            let inventory = capture(&p, OsStr::new("tree"), &flag).unwrap();
            let target = c(d.join("tree/file").as_os_str().as_bytes()).unwrap();
            let attr = c(b"user.files-test").unwrap();
            assert_eq!(
                unsafe {
                    libc::setxattr(
                        target.as_ptr(),
                        attr.as_ptr(),
                        b"value".as_ptr().cast(),
                        5,
                        0,
                    )
                },
                0
            );
            assert!(inventory.verify(&p, OsStr::new("tree"), &flag).is_err());
            let inventory = capture(&p, OsStr::new("tree"), &flag).unwrap();
            fs::write(d.join("tree/new"), b"x").unwrap();
            assert!(inventory.verify(&p, OsStr::new("tree"), &flag).is_err());
            let inventory = capture(&p, OsStr::new("tree"), &flag).unwrap();
            fs::rename(d.join("tree"), d.join("preserved")).unwrap();
            fs::create_dir(d.join("tree")).unwrap();
            assert!(inventory.verify(&p, OsStr::new("tree"), &flag).is_err());
            assert_eq!(fs::read(d.join("preserved/file")).unwrap(), b"abd");
            fs::remove_dir_all(d).unwrap();
        }
        #[test]
        fn depth_limit_on_real_worker_and_special_types_cancelled_before_data_open() {
            let d = fixture("exdev-inventory-depth");
            let mut deep = d.join("tree");
            fs::create_dir(&deep).unwrap();
            for _ in 0..62 {
                deep.push("d");
                fs::create_dir(&deep).unwrap();
            }
            fs::write(deep.join("leaf"), b"x").unwrap();
            let owned = d.clone();
            std::thread::Builder::new()
                .stack_size(2 * 1024 * 1024)
                .spawn(move || {
                    let p = parent(&owned);
                    let c = AtomicBool::new(false);
                    let i = capture(&p, OsStr::new("tree"), &c).unwrap();
                    i.verify(&p, OsStr::new("tree"), &c).unwrap();
                })
                .unwrap()
                .join()
                .unwrap();
            fs::create_dir(deep.join("one-more")).unwrap();
            fs::write(deep.join("one-more/leaf"), b"x").unwrap();
            assert!(capture(&parent(&d), OsStr::new("tree"), &AtomicBool::new(false)).is_err());
            let fifo = c(d.join("fifo").as_os_str().as_bytes()).unwrap();
            assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
            assert!(capture(&parent(&d), OsStr::new("fifo"), &AtomicBool::new(false)).is_err());
            assert!(capture(&parent(&d), OsStr::new("tree"), &AtomicBool::new(true)).is_err());
            fs::remove_dir_all(d).unwrap();
        }
        #[test]
        fn hash_reads_are_bounded_and_cancel_between_actual_chunks() {
            struct Cancelling {
                flag: Arc<AtomicBool>,
                reads: usize,
            }
            impl Read for Cancelling {
                fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
                    self.reads += 1;
                    b.fill(b'x');
                    self.flag.store(true, Ordering::Release);
                    Ok(b.len())
                }
            }
            let flag = Arc::new(AtomicBool::new(false));
            let mut reader = Cancelling {
                flag: flag.clone(),
                reads: 0,
            };
            let mut scratch = vec![0; BUFFER];
            assert_eq!(
                hash_exact(&mut reader, 2 * BUFFER as u64, &mut scratch, &flag)
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::Interrupted
            );
            assert_eq!(reader.reads, 1);
            assert!(
                hash_exact(
                    &mut io::Cursor::new(b"short"),
                    6,
                    &mut scratch,
                    &AtomicBool::new(false)
                )
                .is_err()
            );
            assert!(
                hash_exact(
                    &mut io::Cursor::new(b"extra"),
                    4,
                    &mut scratch,
                    &AtomicBool::new(false)
                )
                .is_err()
            );
        }
    }
}
#[cfg(target_os = "linux")]
pub use linux::{
    AliasReport, AliasScope, AppliedStageMetadata, FailedCopy, Inventory, MAX_ALIAS_ENTRIES,
    MAX_BATCH_NAMESPACE_ENTRIES, MAX_BATCH_NAMESPACE_NAMES, MAX_BATCH_NAMESPACE_SCOPES,
    MAX_BATCH_NAMESPACE_WIRE, MAX_NAMESPACE_ENTRIES, MAX_NAMESPACE_NAMES, MAX_PINNED_NODES,
    NamespaceFailure, NamespaceMove, NamespacePath, NamespaceScope, OwnedCopy, RenameChange,
    StageMetadataFailure, capture, capture_namespace, capture_namespace_controlled, copy_into,
    plan_batch_namespace_move, plan_batch_namespace_move_controlled, plan_namespace_move,
    plan_namespace_move_controlled,
};

#[cfg(target_os = "linux")]
pub(crate) use linux::{capture_controlled, capture_for_sync, copy_into_controlled};
#[cfg(target_os = "linux")]
pub(crate) use linux::{id as observed_identity, revision as observed_revision};

#[cfg(target_os = "linux")]
pub(crate) use linux::{NamespaceControl, NamespaceLog, OwnPoint};

#[cfg(target_os = "linux")]
pub(crate) use linux::StageInput;

#[cfg(target_os = "linux")]
pub(crate) use linux::{MetadataBatch, StageMetadataPoint};
