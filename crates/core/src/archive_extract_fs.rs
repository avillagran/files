//! Descriptor-bound ZIP extraction to an exclusive new child tree. Linux only.
//! No UI/thread/spawn, source removal, merge/overwrite or crash journal. A report
//! records observations, not an atomic snapshot against arbitrary concurrent actors.
/// Conservative transient FD reservation in addition to prepared retained handles.
pub const EXECUTION_EXTRA_FDS: usize = 12;
pub fn supported() -> bool {
    cfg!(all(
        target_os = "linux",
        any(target_arch = "aarch64", target_arch = "x86_64")
    ))
}
#[cfg(target_os = "linux")]
mod linux {
    use crate::{
        archive_extract::{self, Budget, Plan},
        archive_member::{self, Limits},
        copy_engine::Control,
        media_source::Revision,
    };
    use std::{
        collections::BTreeMap,
        ffi::{CString, OsStr, OsString},
        fs::{File, OpenOptions},
        io::{self, Write},
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::{
                ffi::{OsStrExt, OsStringExt},
                fs::{MetadataExt, OpenOptionsExt},
            },
        },
        path::{Path, PathBuf},
    };
    const MAX_PATH: usize = 4096;
    const MAX_AUX: usize = 6 * 1024 * 1024;
    const ROOT: u16 = u16::MAX;
    #[repr(C)]
    struct How {
        flags: u64,
        mode: u64,
        resolve: u64,
    }
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct Identity {
        device: u64,
        inode: u64,
        mount: u64,
        kind: u32,
    }
    fn invalid(s: &'static str) -> io::Error {
        io::Error::new(io::ErrorKind::InvalidData, s)
    }
    fn c(bytes: &[u8]) -> io::Result<CString> {
        CString::new(bytes).map_err(|_| invalid("NUL en ruta ZIP"))
    }
    fn owned(fd: i32) -> io::Result<File> {
        if fd < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(unsafe { File::from_raw_fd(fd) })
        }
    }
    fn strict(root: &File, path: &[u8], flags: i32, mode: u32) -> io::Result<File> {
        let h = How {
            flags: (flags | libc::O_CLOEXEC | libc::O_NOFOLLOW) as u64,
            mode: u64::from(mode),
            resolve: 0x01 | 0x02 | 0x04 | 0x08,
        };
        let result = unsafe {
            libc::syscall(
                libc::SYS_openat2,
                root.as_raw_fd(),
                c(path)?.as_ptr(),
                &h,
                std::mem::size_of::<How>(),
            )
        };
        if result < 0 {
            let e = io::Error::last_os_error();
            if matches!(e.raw_os_error(), Some(libc::ENOSYS | libc::EINVAL)) {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "Extracción ZIP requiere openat2/BENEATH/NO_SYMLINKS/NO_XDEV",
                ));
            }
            return Err(e);
        }
        owned(result as i32)
    }
    fn open_dir(path: &Path) -> io::Result<File> {
        let b = path.as_os_str().as_bytes();
        let end = b.iter().rposition(|&x| x != b'/').map_or(1, |i| i + 1);
        OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
            .open(Path::new(OsStr::from_bytes(&b[..end])))
    }
    fn at(parent: &File, name: &OsStr) -> io::Result<File> {
        owned(unsafe {
            libc::openat(
                parent.as_raw_fd(),
                c(name.as_bytes())?.as_ptr(),
                libc::O_PATH | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        })
    }
    fn identity(file: &File) -> io::Result<Identity> {
        let before = file.metadata()?;
        let mut value = std::mem::MaybeUninit::<libc::statx>::zeroed();
        if unsafe {
            libc::statx(
                file.as_raw_fd(),
                c"".as_ptr(),
                libc::AT_EMPTY_PATH | libc::AT_SYMLINK_NOFOLLOW,
                libc::STATX_BASIC_STATS | libc::STATX_MNT_ID,
                value.as_mut_ptr(),
            )
        } != 0
        {
            return Err(io::Error::last_os_error());
        }
        let value = unsafe { value.assume_init() };
        let after = file.metadata()?;
        if value.stx_mask & libc::STATX_MNT_ID == 0
            || value.stx_mnt_id == 0
            || before.dev() != after.dev()
            || before.ino() != after.ino()
            || before.mode() != after.mode()
        {
            return Err(invalid("Identidad/montaje ZIP no confirmado"));
        }
        Ok(Identity {
            device: after.dev(),
            inode: after.ino(),
            mount: value.stx_mnt_id,
            kind: after.mode() & libc::S_IFMT,
        })
    }
    fn named(parent: &File, name: &OsStr, wanted: Identity) -> io::Result<()> {
        if identity(&at(parent, name)?)? != wanted {
            return Err(invalid("Nombre ZIP fue sustituido"));
        }
        Ok(())
    }
    fn absent(parent: &File, name: &OsStr) -> io::Result<()> {
        match at(parent, name) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
            Ok(_) => Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Destino ocupado; no se sobrescribe",
            )),
        }
    }
    fn path_valid(path: &Path) -> io::Result<()> {
        let b = path.as_os_str().as_bytes();
        if !path.is_absolute() || b.len() > MAX_PATH || b.contains(&0) {
            return Err(invalid("Ruta ZIP inválida o superior a4096 bytes"));
        }
        Ok(())
    }
    fn component(name: &OsStr) -> io::Result<()> {
        let b = name.as_bytes();
        if b.is_empty()
            || b.len() > 255
            || b.contains(&0)
            || b.contains(&b'/')
            || matches!(b, b"." | b"..")
        {
            return Err(invalid("Nombre destino ZIP inválido"));
        }
        Ok(())
    }
    fn current_path(file: &File, wanted: Identity) -> Option<PathBuf> {
        let link = CString::new(format!("/proc/self/fd/{}", file.as_raw_fd())).ok()?;
        let mut bytes = vec![0; MAX_PATH + 1];
        let n = unsafe { libc::readlink(link.as_ptr(), bytes.as_mut_ptr().cast(), bytes.len()) };
        if n < 0 || n as usize > MAX_PATH {
            return None;
        }
        bytes.truncate(n as usize);
        let path = PathBuf::from(OsString::from_vec(bytes));
        let parent = open_dir(path.parent()?).ok()?;
        if identity(&at(&parent, path.file_name()?).ok()?).ok()? != wanted {
            return None;
        }
        Some(path)
    }
    #[derive(Debug)]
    pub struct Prepared {
        source_path: PathBuf,
        source_parent_path: PathBuf,
        source_name: OsString,
        destination: PathBuf,
        name: OsString,
        source_parent: File,
        source: File,
        parent: File,
        source_parent_id: Identity,
        parent_id: Identity,
        source_id: Identity,
        revision: Revision,
        plan: Plan,
        budget: Budget,
        auxiliary: usize,
    }
    impl Prepared {
        pub fn capture(
            source: &Path,
            destination: &Path,
            name: &OsStr,
            budget: Budget,
            control: &Control,
        ) -> io::Result<Self> {
            if !super::supported() {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "Extracción ZIP por FD no implementada en esta plataforma",
                ));
            }
            path_valid(source)?;
            path_valid(destination)?;
            component(name)?;
            if destination.as_os_str().len() + 1 + name.len() > MAX_PATH
                || destination.as_os_str().len() + 65 > MAX_PATH
            {
                return Err(invalid("Destino ZIP no deja margen para staging/rescate"));
            }
            let source_name = source
                .file_name()
                .ok_or_else(|| invalid("Origen ZIP requiere nombre"))?;
            component(source_name)?;
            let source_parent_path = source
                .parent()
                .ok_or_else(|| invalid("Origen ZIP requiere padre"))?;
            control.checkpoint()?;
            let source_parent = open_dir(source_parent_path)?;
            let source_parent_id = identity(&source_parent)?;
            let source_fd = owned(unsafe {
                libc::openat(
                    source_parent.as_raw_fd(),
                    c(source_name.as_bytes())?.as_ptr(),
                    libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
                )
            })?;
            let source_id = identity(&source_fd)?;
            let revision = Revision::from_file(&source_fd)?;
            let parent = open_dir(destination)?;
            let parent_id = identity(&parent)?;
            absent(&parent, name)?;
            let probe = strict(&parent, b".", libc::O_PATH | libc::O_DIRECTORY, 0)?;
            if identity(&probe)? != parent_id {
                return Err(invalid("Padre ZIP cambió al comprobar capacidades"));
            }
            drop(probe);
            let mut source_fd = source_fd;
            let plan =
                archive_extract::prepare(&mut source_fd, budget, control.cancellation_flag())?;
            let nodes = plan
                .files
                .checked_add(plan.directories)
                .ok_or_else(|| invalid("Contador ZIP fuera de rango"))?;
            let auxiliary = plan
                .retained_bytes()
                .and_then(|n| {
                    nodes
                        .checked_mul(std::mem::size_of::<Target>() * 2 + 128)
                        .and_then(|r| n.checked_add(r))
                })
                .and_then(|n| n.checked_add(64 * 1024))
                .filter(|&n| n <= MAX_AUX)
                .ok_or_else(|| {
                    invalid("Plan/registro ZIP supera6MiB auxiliares; reduce el archivo")
                })?;
            let p = Self {
                source_path: source.into(),
                source_parent_path: source_parent_path.into(),
                source_name: source_name.into(),
                destination: destination.into(),
                name: name.into(),
                source_parent,
                source: source_fd,
                parent,
                source_parent_id,
                parent_id,
                source_id,
                revision,
                plan,
                budget,
                auxiliary,
            };
            p.verify()?;
            control.checkpoint()?;
            Ok(p)
        }
        pub fn plan(&self) -> &Plan {
            &self.plan
        }
        pub fn source(&self) -> &Path {
            &self.source_path
        }
        pub fn destination(&self) -> &Path {
            &self.destination
        }
        pub fn name(&self) -> &OsStr {
            &self.name
        }
        pub fn pinned_fds(&self) -> usize {
            3
        }
        pub fn retained_bytes(&self) -> Option<usize> {
            self.plan
                .retained_bytes()?
                .checked_add(self.source_path.capacity())?
                .checked_add(self.source_parent_path.capacity())?
                .checked_add(self.destination.capacity())?
                .checked_add(self.source_name.capacity())?
                .checked_add(self.name.capacity())?
                .checked_add(std::mem::size_of::<Self>())
        }
        pub fn execution_auxiliary_reserve(&self) -> usize {
            self.auxiliary
        }
        /// Reserve before queue admission, independently of execution scratch.
        /// A retained failure owns the plan/registry until an explicit ACK/retry.
        pub fn outcome_reserve(&self) -> Option<usize> {
            self.plan
                .retained_bytes()?
                .checked_add(
                    self.plan
                        .files
                        .checked_add(self.plan.directories)?
                        .checked_mul(2 * std::mem::size_of::<Target>())?,
                )?
                .checked_add(64 * 1024)
        }
        fn verify(&self) -> io::Result<()> {
            if identity(&open_dir(&self.source_parent_path)?)? != self.source_parent_id
                || identity(&open_dir(&self.destination)?)? != self.parent_id
                || identity(&self.source_parent)? != self.source_parent_id
                || identity(&self.parent)? != self.parent_id
            {
                return Err(invalid("Padre de origen/destino ZIP fue sustituido"));
            }
            named(&self.source_parent, &self.source_name, self.source_id)?;
            self.revision.revalidate(&self.source, &self.source_path)
        }
        pub fn execute(
            self,
            control: &Control,
            progress: impl FnMut(Progress),
        ) -> Result<Completed, Box<Failure>> {
            self.execute_observing_publish(control, progress, || Ok(()))
        }
        /// Notification after verified publication, before parent sync. An error
        /// retains PublishedUnsynced authority; Ok NEVER skips the real sync or
        /// post-sync binding check. Useful for durable operation observers.
        pub fn execute_observing_publish(
            self,
            control: &Control,
            progress: impl FnMut(Progress),
            mut published: impl FnMut() -> io::Result<()>,
        ) -> Result<Completed, Box<Failure>> {
            self.run(control, progress, |point, _| {
                if matches!(point, Point::ParentSync) {
                    published().map_err(bound_observer_error)
                } else {
                    Ok(())
                }
            })
        }
    }
    #[derive(Clone, Copy, Debug)]
    pub struct Progress {
        pub bytes: u64,
        pub expected_bytes: u64,
        pub verified_files: usize,
        pub directories: usize,
        pub total_nodes: usize,
    }
    #[derive(Clone, Debug)]
    pub struct Completed {
        pub destination: PathBuf,
        pub bytes: u64,
        pub files: usize,
        pub directories: usize,
    }
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Publication {
        Unpublished,
        PublishedUnsynced,
        Uncertain,
    }
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Cleanup {
        NotNeeded,
        Complete,
        CompleteUnsynced,
        Retained,
        Unknown,
    }
    #[derive(Debug)]
    pub struct Failure {
        pub cause: io::Error,
        pub publication: Publication,
        pub cleanup: Cleanup,
        pub rescue: Option<PathBuf>,
        pub candidate: Option<PathBuf>,
        pub provisional_bytes: u64,
        pub cleanup_error: Option<String>,
        recovery: Option<Recovery>,
    }
    impl std::fmt::Display for Failure {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(
                f,
                "{}; publicación {:?}; limpieza {:?}",
                self.cause, self.publication, self.cleanup
            )
        }
    }
    impl std::error::Error for Failure {}
    #[derive(Debug)]
    enum Recovery {
        Stage(Stage),
        Published { stage: Stage, report: Completed },
    }
    impl Failure {
        pub fn pinned_fds(&self) -> usize {
            if self.recovery.is_some() { 2 } else { 0 }
        }
        /// Capacity accounting for the retained result, not a PSS measurement.
        /// The 8KiB envelope includes the bounded core error and allocator slack.
        pub fn retained_bytes(&self) -> Option<usize> {
            let stage = match self.recovery.as_ref() {
                Some(Recovery::Stage(s) | Recovery::Published { stage: s, .. }) => Some(s),
                None => None,
            };
            let mut bytes = std::mem::size_of::<Self>().checked_add(8192)?;
            for p in [self.rescue.as_ref(), self.candidate.as_ref()]
                .into_iter()
                .flatten()
            {
                bytes = bytes.checked_add(p.capacity())?;
            }
            bytes = bytes.checked_add(self.cleanup_error.as_ref().map_or(0, String::capacity))?;
            if let Some(stage) = stage {
                bytes = bytes
                    .checked_add(stage.plan.retained_bytes()?)?
                    .checked_add(
                        stage
                            .targets
                            .capacity()
                            .checked_mul(std::mem::size_of::<Target>())?,
                    )?
                    .checked_add(stage.parent_path.capacity())?
                    .checked_add(stage.name.capacity())?
                    .checked_add(std::mem::size_of::<Recovery>())?;
            }
            if let Some(Recovery::Published { report, .. }) = self.recovery.as_ref() {
                bytes = bytes.checked_add(report.destination.capacity())?;
            }
            Some(bytes)
        }
        pub fn can_retry_sync(&self) -> bool {
            matches!(self.recovery, Some(Recovery::Published { .. }))
        }
        pub fn can_retry_cleanup(&self) -> bool {
            matches!(self.recovery, Some(Recovery::Stage(_)))
        }
        /// Historical parent path, for display/refresh only; not authority.
        pub fn observed_parent(&self) -> Option<&Path> {
            match self.recovery.as_ref()? {
                Recovery::Stage(stage) | Recovery::Published { stage, .. } => {
                    Some(&stage.parent_path)
                }
            }
        }
        pub fn retry_sync(self: Box<Self>) -> Result<Completed, Box<Self>> {
            self.retry_sync_controlled(&Control::default())
        }
        pub fn retry_sync_controlled(
            mut self: Box<Self>,
            control: &Control,
        ) -> Result<Completed, Box<Self>> {
            if control.checkpoint().is_err() {
                return Err(self);
            }

            let Some(Recovery::Published { stage, report }) = self.recovery.take() else {
                return Err(self);
            };
            let result = (|| {
                if let Err(e) = stage.published_binding(&report.destination) {
                    self.publication = Publication::Uncertain;
                    return Err(e);
                }
                stage.parent.sync_all()?;
                if let Err(e) = stage.published_binding(&report.destination) {
                    self.publication = Publication::Uncertain;
                    return Err(e);
                }
                Ok(())
            })();
            match result {
                Ok(()) => Ok(report),
                Err(e) => {
                    self.cause = e;
                    self.recovery = Some(Recovery::Published { stage, report });
                    Err(self)
                }
            }
        }
        pub fn retry_cleanup(self: Box<Self>) -> Result<(), Box<Self>> {
            self.retry_cleanup_controlled(&Control::default(), |_| {})
        }
        pub fn retry_cleanup_controlled(
            mut self: Box<Self>,
            control: &Control,
            progress: impl FnMut(usize),
        ) -> Result<(), Box<Self>> {
            let Some(Recovery::Stage(mut stage)) = self.recovery.take() else {
                return Err(self);
            };
            match stage.cleanup_controlled(control, progress) {
                Ok(()) => Ok(()),
                Err(e) => {
                    self.cleanup = if stage.root_removed {
                        Cleanup::CompleteUnsynced
                    } else {
                        Cleanup::Retained
                    };
                    self.cleanup_error = Some(short(e.to_string()));
                    self.rescue = stage.location();
                    self.recovery = Some(Recovery::Stage(stage));
                    Err(self)
                }
            }
        }
    }
    fn bound_observer_error(error: io::Error) -> io::Error {
        use std::fmt::Write;
        struct Text(String);
        impl std::fmt::Write for Text {
            fn write_str(&mut self, value: &str) -> std::fmt::Result {
                let available = 509usize.saturating_sub(self.0.len());
                if value.len() <= available {
                    self.0.push_str(value);
                    return Ok(());
                }
                let mut n = available;
                while !value.is_char_boundary(n) {
                    n -= 1;
                }
                self.0.push_str(&value[..n]);
                self.0.push('…');
                Err(std::fmt::Error)
            }
        }
        let kind = error.kind();
        let mut text = Text(String::with_capacity(512));
        let _ = write!(&mut text, "{error}");
        io::Error::new(kind, text.0)
    }
    fn short(mut s: String) -> String {
        if s.len() > 512 {
            let mut n = 509;
            while !s.is_char_boundary(n) {
                n -= 1;
            }
            s.truncate(n);
            s.push('…');
        }
        s
    }
    #[derive(Debug)]
    struct Target {
        owner: u16,
        end: u16,
        parent: u16,
        entry: Option<u16>,
        directory: bool,
        identity: Option<Identity>,
        revision: Option<Revision>,
        removed: bool,
    }
    impl Target {
        fn relative<'a>(&self, plan: &'a Plan) -> &'a [u8] {
            &plan.listing().entries[usize::from(self.owner)].name[..usize::from(self.end)]
        }
    }
    fn targets(plan: &Plan) -> io::Result<Vec<Target>> {
        let mut map = BTreeMap::<&[u8], Target>::new();
        for (index, e) in plan.listing().entries.iter().enumerate() {
            let name = if e.directory {
                e.name.strip_suffix(b"/").unwrap_or(&e.name)
            } else {
                &e.name
            };
            for end in name
                .iter()
                .enumerate()
                .filter_map(|(i, &b)| (b == b'/').then_some(i))
                .chain(std::iter::once(name.len()))
            {
                let key = &name[..end];
                let is_last = end == name.len();
                let t = map.entry(key).or_insert(Target {
                    owner: index as u16,
                    end: end as u16,
                    parent: ROOT,
                    entry: None,
                    directory: !is_last || e.directory,
                    identity: None,
                    revision: None,
                    removed: false,
                });
                if is_last {
                    t.entry = Some(index as u16);
                }
            }
        }
        let mut table: Vec<_> = map.into_values().collect();
        if table.len() > archive_extract::MAX_NODES
            || table.len() != plan.files + plan.directories
            || table.capacity() > table.len().saturating_mul(2)
        {
            return Err(invalid("Registro de destinos ZIP no coincide con el plan"));
        }
        for i in 0..table.len() {
            let relative = table[i].relative(plan);
            if let Some(at) = relative.iter().rposition(|&b| b == b'/') {
                let parent = table
                    .binary_search_by(|n| n.relative(plan).cmp(&relative[..at]))
                    .map_err(|_| invalid("Padre ZIP ausente del plan"))?;
                if parent >= i || !table[parent].directory {
                    return Err(invalid("Padre ZIP inválido"));
                }
                table[i].parent = parent as u16;
            }
        }
        Ok(table)
    }
    #[derive(Debug)]
    struct Stage {
        parent: File,
        parent_path: PathBuf,
        parent_id: Identity,
        name: OsString,
        root: File,
        root_id: Identity,
        plan: Plan,
        targets: Vec<Target>,
        root_removed: bool,
    }
    fn random_name() -> io::Result<OsString> {
        let mut data = [0u8; 16];
        let mut at = 0;
        while at < data.len() {
            let n = unsafe { libc::getrandom(data[at..].as_mut_ptr().cast(), data.len() - at, 0) };
            if n < 0 {
                let e = io::Error::last_os_error();
                if e.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(e);
            }
            if n == 0 {
                return Err(invalid("No se obtuvo nombre privado ZIP"));
            }
            at += n as usize;
        }
        use std::fmt::Write;
        let mut name = String::from(".files-extract-");
        for b in data {
            write!(name, "{b:02x}").expect("String format");
        }
        Ok(name.into())
    }
    impl Stage {
        fn verify_root(&self) -> io::Result<()> {
            if identity(&self.parent)? != self.parent_id
                || identity(&open_dir(&self.parent_path)?)? != self.parent_id
                || identity(&self.root)? != self.root_id
            {
                return Err(invalid("Raíz/padre staging ZIP cambió"));
            }
            named(&self.parent, &self.name, self.root_id)
        }
        fn verify_private_root(&self) -> io::Result<()> {
            self.verify_root()?;
            if self.root.metadata()?.mode() & 0o777 != 0o700 {
                return Err(invalid("Permisos de raíz staging ZIP cambiaron"));
            }
            Ok(())
        }
        fn location(&self) -> Option<PathBuf> {
            current_path(&self.root, self.root_id)
        }
        fn relative(&self, index: usize) -> &[u8] {
            self.targets[index].relative(&self.plan)
        }
        fn parent_of(&self, index: usize) -> io::Result<File> {
            let parent = self.targets[index].parent;
            if parent == ROOT {
                return strict(&self.root, b".", libc::O_RDONLY | libc::O_DIRECTORY, 0);
            }
            self.verify_node(usize::from(parent), false)?;
            strict(
                &self.root,
                self.relative(usize::from(parent)),
                libc::O_RDONLY | libc::O_DIRECTORY,
                0,
            )
        }
        fn verify_node(&self, index: usize, sealed: bool) -> io::Result<()> {
            let target = &self.targets[index];
            if target.removed {
                return Err(invalid("Nodo ZIP ya eliminado"));
            }
            let wanted = target
                .identity
                .ok_or_else(|| invalid("Nodo ZIP no es propio"))?;
            let file = strict(&self.root, self.relative(index), libc::O_PATH, 0)?;
            if identity(&file)? != wanted {
                return Err(invalid("Nodo staging ZIP sustituido"));
            }
            let m = file.metadata()?;
            if target.directory {
                if m.mode() & 0o777 != 0o700 {
                    return Err(invalid("Permisos de directorio staging ZIP cambiaron"));
                }
            } else {
                if m.nlink() != 1 {
                    return Err(invalid("Archivo staging ZIP adquirió enlaces externos"));
                }
                if sealed
                    && target
                        .revision
                        .is_none_or(|r| Revision::from_file(&file).ok() != Some(r))
                {
                    return Err(invalid("Contenido staging ZIP cambió después del CRC"));
                }
            }
            Ok(())
        }
        fn create(&mut self, index: usize) -> io::Result<File> {
            self.verify_private_root()?;
            let parent = self.parent_of(index)?;
            let relative = self.relative(index);
            let basename = relative
                .rsplit(|&b| b == b'/')
                .next()
                .expect("planned component");
            let file = if self.targets[index].directory {
                if unsafe { libc::mkdirat(parent.as_raw_fd(), c(basename)?.as_ptr(), 0o700) } != 0 {
                    return Err(io::Error::last_os_error());
                }
                // O_PATH permits descriptor-bound chmod even under a restrictive umask.
                strict(&self.root, relative, libc::O_PATH | libc::O_DIRECTORY, 0)?
            } else {
                strict(
                    &self.root,
                    relative,
                    libc::O_CREAT | libc::O_EXCL | libc::O_RDWR | libc::O_NONBLOCK,
                    0o600,
                )?
            };
            let id = identity(&file)?;
            if id.mount != self.root_id.mount
                || id.device != self.root_id.device
                || id.kind
                    != if self.targets[index].directory {
                        libc::S_IFDIR
                    } else {
                        libc::S_IFREG
                    }
            {
                return Err(invalid("Tipo/montaje staging ZIP no coincide"));
            }
            self.targets[index].identity = Some(id);
            crate::file_properties::set_descriptor_mode(
                &file,
                if self.targets[index].directory {
                    0o700
                } else {
                    0o600
                },
            )?;
            self.verify_node(index, false)?;
            Ok(file)
        }
        fn verify_tree(&self, sealed: bool, control: Option<&Control>) -> io::Result<()> {
            self.verify_private_root()?;
            let mut seen = 0usize;
            let mut read_bytes = 0usize;
            let byte_limit = (self.targets.len() * 3 + 2) * 512;
            for parent in
                std::iter::once(None).chain(self.targets.iter().enumerate().filter_map(|(i, n)| {
                    (n.directory && n.identity.is_some() && !n.removed).then_some(Some(i))
                }))
            {
                if let Some(c) = control {
                    c.checkpoint()?;
                }
                let relative = parent.map_or(&b"."[..], |i| self.relative(i));
                if let Some(i) = parent {
                    self.verify_node(i, sealed)?;
                }
                let directory =
                    strict(&self.root, relative, libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
                let expected = parent.map_or(self.root_id, |i| self.targets[i].identity.unwrap());
                if identity(&directory)? != expected {
                    return Err(invalid("Directorio ZIP cambió al enumerar"));
                }
                let mut buffer = [0u8; 8192];
                loop {
                    if let Some(c) = control {
                        c.checkpoint()?;
                    }
                    let n = unsafe {
                        libc::syscall(
                            libc::SYS_getdents64,
                            directory.as_raw_fd(),
                            buffer.as_mut_ptr(),
                            buffer.len(),
                        )
                    };
                    if n < 0 {
                        let e = io::Error::last_os_error();
                        if e.kind() == io::ErrorKind::Interrupted {
                            continue;
                        }
                        return Err(e);
                    }
                    if n == 0 {
                        break;
                    }
                    let n = n as usize;
                    read_bytes = read_bytes.saturating_add(n);
                    if read_bytes > byte_limit {
                        return Err(invalid("READDIR ZIP supera el inventario acotado"));
                    }
                    if n > buffer.len() {
                        return Err(invalid("READDIR ZIP fuera de rango"));
                    }
                    let mut at = 0;
                    while at < n {
                        if n - at < 20 {
                            return Err(invalid("READDIR ZIP incompleto"));
                        }
                        let size = usize::from(u16::from_ne_bytes(
                            buffer[at + 16..at + 18].try_into().unwrap(),
                        ));
                        if size < 20 || at + size > n {
                            return Err(invalid("READDIR ZIP inválido"));
                        }
                        let bytes = &buffer[at + 19..at + size];
                        let end = bytes
                            .iter()
                            .position(|&b| b == 0)
                            .ok_or_else(|| invalid("Nombre READDIR ZIP sin terminador"))?;
                        let name = &bytes[..end];
                        at += size;
                        if matches!(name, b"." | b"..") {
                            continue;
                        }
                        let mut path = Vec::with_capacity(relative.len() + name.len() + 1);
                        if parent.is_some() {
                            path.extend_from_slice(relative);
                            path.push(b'/');
                        }
                        path.extend_from_slice(name);
                        let i = self
                            .targets
                            .binary_search_by(|n| n.relative(&self.plan).cmp(&path))
                            .map_err(|_| {
                                invalid("Entrada ajena en staging ZIP; no limpiar por inferencia")
                            })?;
                        if self.targets[i].identity.is_none()
                            || self.targets[i].removed
                            || self.targets[i].parent != parent.map_or(ROOT, |i| i as u16)
                        {
                            return Err(invalid("Entrada ZIP no creada por este trabajo"));
                        }
                        self.verify_node(i, sealed)?;
                        seen += 1;
                        if seen > self.targets.len() {
                            return Err(invalid("READDIR ZIP repite entradas fuera del plan"));
                        }
                    }
                }
            }
            if seen
                != self
                    .targets
                    .iter()
                    .filter(|n| n.identity.is_some() && !n.removed)
                    .count()
            {
                return Err(invalid("Faltan nodos staging ZIP"));
            }
            Ok(())
        }
        fn remove_owned(&mut self, i: usize) -> io::Result<()> {
            self.verify_root()?;
            self.verify_node(i, false)?;
            let parent = self.parent_of(i)?;
            let name = self.relative(i).rsplit(|&b| b == b'/').next().unwrap();
            let flags = if self.targets[i].directory {
                libc::AT_REMOVEDIR
            } else {
                0
            };
            if unsafe { libc::unlinkat(parent.as_raw_fd(), c(name)?.as_ptr(), flags) } != 0 {
                return Err(io::Error::last_os_error());
            }
            self.targets[i].removed = true;
            Ok(())
        }
        fn cleanup(&mut self) -> io::Result<()> {
            self.cleanup_with_sync(File::sync_all)
        }
        fn cleanup_with_sync(
            &mut self,
            sync: impl FnOnce(&File) -> io::Result<()>,
        ) -> io::Result<()> {
            self.cleanup_internal(None, sync, |_| {})
        }
        fn cleanup_controlled(
            &mut self,
            control: &Control,
            progress: impl FnMut(usize),
        ) -> io::Result<()> {
            self.cleanup_internal(Some(control), File::sync_all, progress)
        }
        fn cleanup_internal(
            &mut self,
            control: Option<&Control>,
            sync: impl FnOnce(&File) -> io::Result<()>,
            mut progress: impl FnMut(usize),
        ) -> io::Result<()> {
            if let Some(c) = control {
                c.checkpoint()?;
            }
            if self.root_removed {
                return sync(&self.parent);
            }
            self.verify_tree(false, control)?;
            let mut removed = 0;
            for i in (0..self.targets.len()).rev() {
                if let Some(c) = control {
                    c.checkpoint()?;
                }
                if self.targets[i].identity.is_some() && !self.targets[i].removed {
                    self.remove_owned(i)?;
                    removed += 1;
                    progress(removed);
                }
            }
            if let Some(c) = control {
                c.checkpoint()?;
            }
            self.verify_root()?;
            if unsafe {
                libc::unlinkat(
                    self.parent.as_raw_fd(),
                    c(self.name.as_bytes())?.as_ptr(),
                    libc::AT_REMOVEDIR,
                )
            } != 0
            {
                return Err(io::Error::last_os_error());
            }
            self.root_removed = true;
            sync(&self.parent)
        }
        fn published_binding(&self, path: &Path) -> io::Result<()> {
            if path.parent() != Some(self.parent_path.as_path())
                || identity(&open_dir(&self.parent_path)?)? != self.parent_id
            {
                return Err(invalid("Padre publicado ZIP cambió"));
            }
            named(
                &self.parent,
                path.file_name()
                    .ok_or_else(|| invalid("Nombre publicado inválido"))?,
                self.root_id,
            )
        }
    }
    struct Sink<'a> {
        file: &'a mut File,
        stage: &'a Stage,
        index: usize,
        total: &'a mut u64,
        cap: u64,
        control: &'a Control,
    }
    impl Write for Sink<'_> {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.control.checkpoint()?;
            self.stage.verify_private_root()?;
            self.stage.verify_node(self.index, false)?;
            if self
                .total
                .checked_add(bytes.len() as u64)
                .is_none_or(|n| n > self.cap)
            {
                return Err(invalid("Extracción ZIP excede cuota real"));
            }
            let n = self.file.write(bytes)?;
            *self.total += n as u64;
            self.control.checkpoint()?;
            Ok(n)
        }
        fn flush(&mut self) -> io::Result<()> {
            self.file.flush()
        }
    }
    #[derive(Clone, Copy)]
    enum Point {
        BeforePublish,
        RenameCall,
        AfterRename,
        ParentSync,
    }
    fn early(error: io::Error) -> Box<Failure> {
        Box::new(Failure {
            cause: error,
            publication: Publication::Unpublished,
            cleanup: Cleanup::NotNeeded,
            rescue: None,
            candidate: None,
            provisional_bytes: 0,
            cleanup_error: None,
            recovery: None,
        })
    }
    fn failed(
        stage: Stage,
        error: io::Error,
        total: u64,
        publication: Publication,
        candidate: Option<PathBuf>,
    ) -> Box<Failure> {
        failed_with_cleanup(stage, error, total, publication, candidate, Stage::cleanup)
    }
    fn failed_with_cleanup(
        mut stage: Stage,
        error: io::Error,
        total: u64,
        publication: Publication,
        candidate: Option<PathBuf>,
        cleanup: impl FnOnce(&mut Stage) -> io::Result<()>,
    ) -> Box<Failure> {
        let cleanup = if publication == Publication::Unpublished {
            cleanup(&mut stage)
        } else {
            Err(invalid(
                "Publicación incierta: no limpiar ni relocalizar por inferencia",
            ))
        };
        match cleanup {
            Ok(()) => Box::new(Failure {
                cause: error,
                publication,
                cleanup: Cleanup::Complete,
                rescue: None,
                candidate,
                provisional_bytes: total,
                cleanup_error: None,
                recovery: None,
            }),
            Err(cleanup_error) => {
                let rescue = stage.location();
                Box::new(Failure {
                    cause: error,
                    publication,
                    cleanup: if stage.root_removed {
                        Cleanup::CompleteUnsynced
                    } else {
                        Cleanup::Retained
                    },
                    rescue,
                    candidate,
                    provisional_bytes: total,
                    cleanup_error: Some(short(cleanup_error.to_string())),
                    recovery: Some(Recovery::Stage(stage)),
                })
            }
        }
    }
    impl Prepared {
        fn run(
            self,
            control: &Control,
            mut progress: impl FnMut(Progress),
            mut hook: impl FnMut(Point, &Stage) -> io::Result<()>,
        ) -> Result<Completed, Box<Failure>> {
            control
                .checkpoint()
                .and_then(|_| self.verify())
                .and_then(|_| absent(&self.parent, &self.name))
                .map_err(early)?;
            let table = targets(&self.plan).map_err(early)?;
            control.checkpoint().map_err(early)?;
            self.verify().map_err(early)?;
            let name = random_name().map_err(early)?;
            if name == self.name {
                return Err(early(invalid(
                    "Nombre de staging coincide con destino; reintenta",
                )));
            }
            let candidate = self.destination.join(&name);
            if unsafe {
                libc::mkdirat(
                    self.parent.as_raw_fd(),
                    c(name.as_bytes()).map_err(early)?.as_ptr(),
                    0o700,
                )
            } != 0
            {
                let mut f = early(io::Error::last_os_error());
                if at(&self.parent, &name).is_ok() {
                    f.cleanup = Cleanup::Unknown;
                    f.candidate = Some(candidate);
                }
                return Err(f);
            }
            let root = match strict(
                &self.parent,
                name.as_bytes(),
                libc::O_PATH | libc::O_DIRECTORY,
                0,
            ) {
                Ok(f) => f,
                Err(e) => {
                    let mut f = early(e);
                    f.cleanup = Cleanup::Unknown;
                    f.candidate = Some(candidate);
                    return Err(f);
                }
            };
            let root_id = match identity(&root) {
                Ok(id) => id,
                Err(e) => {
                    let mut f = early(e);
                    f.cleanup = Cleanup::Unknown;
                    f.candidate = Some(candidate);
                    return Err(f);
                }
            };
            let Self {
                mut source,
                source_path,
                source_parent_path,
                source_name,
                source_parent,
                destination,
                parent,
                parent_id,
                source_parent_id,
                source_id,
                revision,
                plan,
                budget,
                name: target_name,
                ..
            } = self;
            let mut stage = Stage {
                parent,
                parent_path: destination,
                parent_id,
                name,
                root,
                root_id,
                plan,
                targets: table,
                root_removed: false,
            };
            let mut total = 0u64;
            let (mut files, mut dirs) = (0, 0);
            let source_verify = |source: &File| -> io::Result<()> {
                if identity(&open_dir(&source_parent_path)?)? != source_parent_id {
                    return Err(invalid("Padre ZIP de origen cambió"));
                }
                named(&source_parent, &source_name, source_id)?;
                revision.revalidate(source, &source_path)
            };
            let work = (|| -> io::Result<()> {
                stage.verify_root()?;
                crate::file_properties::set_descriptor_mode(&stage.root, 0o700)?;
                for i in 0..stage.targets.len() {
                    control.checkpoint()?;
                    stage.verify_root()?;
                    let member = stage.targets[i].entry.map(usize::from);
                    if stage.targets[i].directory {
                        if let Some(index) = member {
                            archive_member::copy_to(
                                &mut source,
                                stage.plan.listing(),
                                index,
                                Limits {
                                    compressed: u64::from(u32::MAX - 1),
                                    output: 0,
                                },
                                &mut io::sink(),
                                control.cancellation_flag(),
                                |_| {},
                            )?;
                        }
                        stage.create(i)?;
                        dirs += 1;
                    } else {
                        let mut file = stage.create(i)?;
                        let index = member.ok_or_else(|| invalid("Archivo ZIP sin miembro"))?;
                        let before = total;
                        {
                            let mut sink = Sink {
                                file: &mut file,
                                stage: &stage,
                                index: i,
                                total: &mut total,
                                cap: budget.total,
                                control,
                            };
                            archive_member::copy_to(
                                &mut source,
                                stage.plan.listing(),
                                index,
                                Limits {
                                    compressed: u64::from(u32::MAX - 1),
                                    output: budget.member,
                                },
                                &mut sink,
                                control.cancellation_flag(),
                                |bytes| {
                                    progress(Progress {
                                        bytes: before + bytes,
                                        expected_bytes: stage.plan.bytes,
                                        verified_files: files,
                                        directories: dirs,
                                        total_nodes: stage.targets.len(),
                                    })
                                },
                            )?;
                        }
                        let mode = 0o600
                            | u32::from(
                                stage.plan.listing().entries[index].unix_mode.unwrap_or(0) & 0o100,
                            );
                        crate::file_properties::set_descriptor_mode(&file, mode)?;
                        file.sync_all()?;
                        stage.verify_node(i, false)?;
                        stage.targets[i].revision = Some(Revision::from_file(&file)?);
                        files += 1;
                    }
                    progress(Progress {
                        bytes: total,
                        expected_bytes: stage.plan.bytes,
                        verified_files: files,
                        directories: dirs,
                        total_nodes: stage.targets.len(),
                    });
                }
                if total != stage.plan.bytes
                    || files != stage.plan.files
                    || dirs != stage.plan.directories
                {
                    return Err(invalid("Totales ZIP no coinciden con el plan"));
                }
                stage.verify_tree(true, Some(control))?;
                for i in (0..stage.targets.len()).rev() {
                    if stage.targets[i].directory {
                        control.checkpoint()?;
                        stage.verify_node(i, true)?;
                        strict(
                            &stage.root,
                            stage.relative(i),
                            libc::O_RDONLY | libc::O_DIRECTORY,
                            0,
                        )?
                        .sync_all()?;
                    }
                }
                strict(&stage.root, b".", libc::O_RDONLY | libc::O_DIRECTORY, 0)?.sync_all()?;
                hook(Point::BeforePublish, &stage)?;
                control.checkpoint()?;
                source_verify(&source)?;
                stage.verify_tree(true, Some(control))?;
                absent(&stage.parent, &target_name)?;
                hook(Point::RenameCall, &stage)?;
                control.checkpoint()?;
                Ok(())
            })();
            if let Err(e) = work {
                return Err(failed(stage, e, total, Publication::Unpublished, None));
            }
            let report = Completed {
                destination: stage.parent_path.join(&target_name),
                bytes: total,
                files,
                directories: dirs,
            };
            let renamed = unsafe {
                libc::syscall(
                    libc::SYS_renameat2,
                    stage.parent.as_raw_fd(),
                    c(stage.name.as_bytes()).map_err(early)?.as_ptr(),
                    stage.parent.as_raw_fd(),
                    c(target_name.as_bytes()).map_err(early)?.as_ptr(),
                    libc::RENAME_NOREPLACE,
                )
            };
            let rename_error = (renamed != 0).then(io::Error::last_os_error);
            let after = hook(Point::AfterRename, &stage);
            let published = stage.published_binding(&report.destination).is_ok()
                && absent(&stage.parent, &stage.name).is_ok();
            if !published {
                let known_unpublished = named(&stage.parent, &stage.name, stage.root_id).is_ok()
                    && absent(&stage.parent, &target_name).is_ok();
                let error = rename_error
                    .unwrap_or_else(|| invalid("Publicación ZIP sin identidad confirmada"));
                return Err(failed(
                    stage,
                    error,
                    total,
                    if known_unpublished {
                        Publication::Unpublished
                    } else {
                        Publication::Uncertain
                    },
                    Some(report.destination),
                ));
            }
            let sync = rename_error
                .map_or(after, Err)
                .and_then(|_| hook(Point::ParentSync, &stage))
                .and_then(|_| stage.parent.sync_all())
                .and_then(|_| stage.published_binding(&report.destination));
            match sync {
                Ok(()) => Ok(report),
                Err(e) => {
                    let rescue = stage.location();
                    Err(Box::new(Failure {
                        cause: e,
                        publication: if stage.published_binding(&report.destination).is_ok() {
                            Publication::PublishedUnsynced
                        } else {
                            Publication::Uncertain
                        },
                        cleanup: Cleanup::NotNeeded,
                        rescue,
                        candidate: Some(report.destination.clone()),
                        provisional_bytes: total,
                        cleanup_error: None,
                        recovery: Some(Recovery::Published { stage, report }),
                    }))
                }
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::{
            archive_extract::Budget,
            archive_member::fixtures::{Spec, zip},
            copy_engine::Control,
        };
        use std::{
            ffi::{OsStr, OsString},
            fs,
            os::unix::{
                ffi::OsStringExt,
                fs::{MetadataExt, PermissionsExt, symlink},
            },
            path::PathBuf,
            sync::{
                Arc,
                atomic::{AtomicBool, Ordering},
            },
            thread,
            time::Duration,
        };
        struct Case {
            base: PathBuf,
            source: PathBuf,
            dest: PathBuf,
        }
        impl Case {
            fn new(label: &str, data: Vec<u8>) -> Self {
                let base = crate::persistence::test_dir(label);
                let source = base.join("input.zip");
                fs::write(&source, data).unwrap();
                let dest = base.join("dest");
                fs::create_dir(&dest).unwrap();
                Self { base, source, dest }
            }
            fn prepare(&self) -> Prepared {
                Prepared::capture(
                    &self.source,
                    &self.dest,
                    OsStr::new("out"),
                    Budget::default(),
                    &Control::default(),
                )
                .unwrap()
            }
            fn stages(&self) -> Vec<PathBuf> {
                fs::read_dir(&self.dest)
                    .unwrap()
                    .map(|e| e.unwrap().path())
                    .filter(|p| {
                        p.file_name()
                            .unwrap()
                            .as_encoded_bytes()
                            .starts_with(b".files-extract-")
                    })
                    .collect()
            }
        }
        impl Drop for Case {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.base);
            }
        }
        fn archive(data: &[u8]) -> Vec<u8> {
            zip(&[
                Spec::file(b"a/one", data),
                Spec::file(b"a/raw-\xff", b"raw bytes"),
                {
                    let mut e = Spec::file(b"empty/", b"");
                    e.method = 0;
                    e.mode = 0o040700;
                    e
                },
            ])
        }
        #[test]
        fn prepare_does_not_create_output_and_success_publishes_crc_checked_tree() {
            let bytes = archive(b"contents");
            let c = Case::new("archive-exec-success", bytes.clone());
            let prepared = c.prepare();
            assert!(c.stages().is_empty());
            assert!(!c.dest.join("out").exists());
            assert_eq!(prepared.pinned_fds(), 3);
            assert!(prepared.retained_bytes().unwrap() < prepared.execution_auxiliary_reserve());
            let mut seen = 0;
            let done = prepared
                .execute(&Control::default(), |p| {
                    assert!(p.bytes >= seen);
                    seen = p.bytes;
                })
                .unwrap();
            assert_eq!(done.files, 2);
            assert_eq!(done.directories, 2);
            assert_eq!(done.bytes, 17);
            assert_eq!(fs::read(c.dest.join("out/a/one")).unwrap(), b"contents");
            assert_eq!(
                fs::read(
                    c.dest
                        .join("out/a")
                        .join(OsString::from_vec(b"raw-\xff".to_vec()))
                )
                .unwrap(),
                b"raw bytes"
            );
            assert!(c.dest.join("out/empty").is_dir());
            assert!(c.stages().is_empty());
            assert_eq!(fs::read(&c.source).unwrap(), bytes);
            assert_eq!(
                fs::metadata(c.dest.join("out")).unwrap().mode() & 0o777,
                0o700
            );
            assert_eq!(
                fs::metadata(c.dest.join("out/a/one")).unwrap().mode() & 0o777,
                0o600
            );
        }
        #[test]
        fn opaque_source_destination_and_output_names_remain_exact() {
            let c = Case::new("archive-exec-raw-path", archive(b"raw"));
            let rawsource = c.base.join(OsString::from_vec(b"input-\xfe.zip".to_vec()));
            fs::rename(&c.source, &rawsource).unwrap();
            let rawdest = c.base.join(OsString::from_vec(b"dest-\xfe".to_vec()));
            fs::rename(&c.dest, &rawdest).unwrap();
            let name = OsString::from_vec(b"out-\xff".to_vec());
            let p = Prepared::capture(
                &rawsource,
                &rawdest,
                &name,
                Budget::default(),
                &Control::default(),
            )
            .unwrap();
            p.execute(&Control::default(), |_| {}).unwrap();
            assert_eq!(fs::read(rawdest.join(name).join("a/one")).unwrap(), b"raw");
        }
        #[test]
        fn occupied_symlink_and_directory_targets_reject_without_mutation() {
            let c = Case::new("archive-exec-occupied", archive(b"data"));
            let other = c.base.join("other");
            fs::write(&other, b"occupant").unwrap();
            for kind in 0..3 {
                let target = c.dest.join("out");
                match kind {
                    0 => fs::write(&target, b"occupant").unwrap(),
                    1 => symlink(&other, &target).unwrap(),
                    _ => fs::create_dir(&target).unwrap(),
                };
                assert!(
                    Prepared::capture(
                        &c.source,
                        &c.dest,
                        OsStr::new("out"),
                        Budget::default(),
                        &Control::default()
                    )
                    .is_err()
                );
                assert!(c.stages().is_empty());
                if kind == 2 {
                    fs::remove_dir(target).unwrap()
                } else {
                    fs::remove_file(target).unwrap()
                }
            }
            assert_eq!(fs::read(other).unwrap(), b"occupant");
        }
        #[test]
        fn changed_source_or_parent_between_review_and_apply_is_rejected_before_staging() {
            let c = Case::new("archive-exec-replaced", archive(b"aaaa"));
            let p = c.prepare();
            let old = c.base.join("old.zip");
            fs::rename(&c.source, &old).unwrap();
            fs::write(&c.source, archive(b"bbbb")).unwrap();
            assert!(p.execute(&Control::default(), |_| {}).is_err());
            assert!(c.stages().is_empty());
            assert!(!c.dest.join("out").exists());
            let p = c.prepare();
            let oldparent = c.base.join("old-dest");
            fs::rename(&c.dest, &oldparent).unwrap();
            fs::create_dir(&c.dest).unwrap();
            assert!(p.execute(&Control::default(), |_| {}).is_err());
            assert!(fs::read_dir(oldparent).unwrap().next().is_none());
        }
        #[test]
        fn final_source_and_destination_parent_symlinks_are_rejected_including_trailing_slash() {
            let c = Case::new("archive-exec-symlinks", archive(b"x"));
            let link = c.base.join("link.zip");
            symlink(&c.source, &link).unwrap();
            assert!(
                Prepared::capture(
                    &link,
                    &c.dest,
                    OsStr::new("out"),
                    Budget::default(),
                    &Control::default()
                )
                .is_err()
            );
            let link = c.base.join("dest-link");
            symlink(&c.dest, &link).unwrap();
            assert!(
                Prepared::capture(
                    &c.source,
                    &link,
                    OsStr::new("out"),
                    Budget::default(),
                    &Control::default()
                )
                .is_err()
            );
            let trailing = PathBuf::from(format!("{}/", link.display()));
            assert!(
                Prepared::capture(
                    &c.source,
                    &trailing,
                    OsStr::new("out"),
                    Budget::default(),
                    &Control::default()
                )
                .is_err()
            );
        }
        #[test]
        fn bad_crc_and_runtime_cancel_remove_only_own_unpublished_staging() {
            let mut e = Spec::file(b"bad", b"data");
            e.crc = Some(1);
            let c = Case::new("archive-exec-crc", zip(&[e]));
            let err = c
                .prepare()
                .execute(&Control::default(), |_| {})
                .unwrap_err();
            assert_eq!(err.publication, Publication::Unpublished);
            assert_eq!(err.cleanup, Cleanup::Complete);
            assert!(c.stages().is_empty());
            assert!(!c.dest.join("out").exists());
            let data = vec![b'a'; 200000];
            let c = Case::new("archive-exec-cancel", archive(&data));
            let control = Control::default();
            let error = c
                .prepare()
                .execute(&control, |p| {
                    if p.bytes > 0 {
                        control.cancel()
                    }
                })
                .unwrap_err();
            assert_eq!(error.cause.kind(), std::io::ErrorKind::Interrupted);
            assert_eq!(error.cleanup, Cleanup::Complete);
            assert!(error.provisional_bytes > 0);
            assert!(c.stages().is_empty());
            assert!(!c.dest.join("out").exists());
        }
        #[test]
        fn cancellation_before_apply_and_pause_then_cancel_do_not_create_staging() {
            let c = Case::new("archive-exec-paused", archive(b"data"));
            let p = c.prepare();
            let control = Control::default();
            control.pause();
            let copy = control.clone();
            let returned = Arc::new(AtomicBool::new(false));
            let done = returned.clone();
            let t = thread::spawn(move || {
                let e = p.execute(&copy, |_| {}).unwrap_err();
                done.store(true, Ordering::Release);
                e
            });
            thread::sleep(Duration::from_millis(20));
            assert!(!returned.load(Ordering::Acquire));
            assert!(c.stages().is_empty());
            control.cancel();
            assert_eq!(
                t.join().unwrap().cause.kind(),
                std::io::ErrorKind::Interrupted
            );
            assert!(c.stages().is_empty());
        }
        #[test]
        fn owner_execute_is_retained_but_privileged_and_group_modes_are_not() {
            let mut e = Spec::file(b"run", b"#!/bin/sh\n");
            e.mode = 0o107777;
            let c = Case::new("archive-exec-mode", zip(&[e]));
            c.prepare().execute(&Control::default(), |_| {}).unwrap();
            assert_eq!(
                fs::metadata(c.dest.join("out/run")).unwrap().mode() & 0o7777,
                0o700
            );
        }
        #[test]
        fn occupant_created_after_review_is_not_overwritten_and_staging_is_clean() {
            let c = Case::new("archive-exec-late-collision", archive(b"data"));
            let p = c.prepare();
            let error = p
                .run(
                    &Control::default(),
                    |_| {},
                    |point, _| {
                        if matches!(point, Point::BeforePublish) {
                            fs::write(c.dest.join("out"), b"new occupant")?;
                        }
                        Ok(())
                    },
                )
                .unwrap_err();
            assert_eq!(fs::read(c.dest.join("out")).unwrap(), b"new occupant");
            assert_eq!(error.publication, Publication::Unpublished);
            assert_eq!(error.cleanup, Cleanup::Complete);
            assert!(c.stages().is_empty());
        }
        #[test]
        fn source_replaced_during_stream_keeps_new_original_and_never_publishes() {
            let old = archive(&vec![b'a'; 200000]);
            let new = archive(&vec![b'b'; 200000]);
            let c = Case::new("archive-exec-stream-source", old);
            let mut changed = false;
            let error = c
                .prepare()
                .execute(&Control::default(), |p| {
                    if p.bytes > 0 && !changed {
                        changed = true;
                        let replacement = c.base.join("replacement");
                        fs::write(&replacement, &new).unwrap();
                        fs::rename(replacement, &c.source).unwrap();
                    }
                })
                .unwrap_err();
            assert!(changed);
            assert_eq!(fs::read(&c.source).unwrap(), new);
            assert!(!c.dest.join("out").exists());
            assert_eq!(error.cleanup, Cleanup::Complete);
            assert!(c.stages().is_empty());
        }
        #[test]
        fn symlink_replacement_of_active_output_never_redirects_bytes_or_cleanup() {
            let c = Case::new("archive-exec-output-replaced", archive(&vec![b'x'; 200000]));
            let outside = c.base.join("outside");
            fs::write(&outside, b"untouched").unwrap();
            let mut changed = false;
            let mut error = c
                .prepare()
                .execute(&Control::default(), |p| {
                    if p.bytes > 0 && !changed {
                        changed = true;
                        let stage = c.stages().pop().unwrap();
                        fs::rename(stage.join("a/one"), stage.join("a/held")).unwrap();
                        symlink(&outside, stage.join("a/one")).unwrap();
                    }
                })
                .unwrap_err();
            assert_eq!(fs::read(&outside).unwrap(), b"untouched");
            assert_eq!(error.cleanup, Cleanup::Retained);
            let root = error.rescue.clone().unwrap();
            assert!(root.join("a/one").is_symlink());
            assert!(root.join("a/held").exists());
            assert!(!c.dest.join("out").exists());
            fs::remove_file(root.join("a/one")).unwrap();
            fs::rename(root.join("a/held"), root.join("a/one")).unwrap();
            assert!(error.can_retry_cleanup());
            error = match error.retry_cleanup() {
                Ok(()) => return,
                Err(e) => e,
            };
            panic!("{error}");
        }
        #[test]
        fn unknown_foreign_entry_blocks_cleanup_and_retry_after_removal_is_explicit() {
            let c = Case::new("archive-exec-foreign", archive(&vec![b'x'; 200000]));
            let control = Control::default();
            let mut changed = false;
            let prepared = c.prepare();
            let reserve = prepared.outcome_reserve().unwrap();
            let error = prepared
                .execute(&control, |p| {
                    if p.bytes > 0 && !changed {
                        changed = true;
                        let stage = c.stages().pop().unwrap();
                        fs::write(stage.join("foreign"), b"not owned").unwrap();
                        control.cancel();
                    }
                })
                .unwrap_err();
            assert_eq!(error.cleanup, Cleanup::Retained);
            assert_eq!(error.pinned_fds(), 2);
            assert!(error.retained_bytes().unwrap() <= reserve);
            let root = error.rescue.clone().unwrap();
            assert_eq!(fs::read(root.join("foreign")).unwrap(), b"not owned");
            assert!(error.can_retry_cleanup());
            fs::remove_file(root.join("foreign")).unwrap();
            error.retry_cleanup().unwrap();
            assert!(c.stages().is_empty());
        }
        #[test]
        fn partial_cleanup_ledger_survives_an_error_and_does_not_delete_new_occupant() {
            let c = Case::new("archive-exec-partial-cleanup", archive(b"data"));
            let error = c
                .prepare()
                .run(
                    &Control::default(),
                    |_| {},
                    |point, stage| {
                        if matches!(point, Point::BeforePublish) {
                            fs::write(stage.location().unwrap().join("foreign"), b"not owned")?;
                            return Err(std::io::Error::other("synthetic precommit failure"));
                        }
                        Ok(())
                    },
                )
                .unwrap_err();
            let mut error = error;
            let root = error.rescue.clone().unwrap();
            fs::remove_file(root.join("foreign")).unwrap();
            let Some(Recovery::Stage(stage)) = &mut error.recovery else {
                panic!()
            };
            let index = stage.targets.iter().position(|n| !n.directory).unwrap();
            let path = root.join(OsStr::from_bytes(stage.relative(index)));
            stage.remove_owned(index).unwrap();
            assert!(!path.exists());
            fs::write(&path, b"new occupant").unwrap();
            let error = error.retry_cleanup().unwrap_err();
            assert_eq!(fs::read(&path).unwrap(), b"new occupant");
            fs::remove_file(path).unwrap();
            error.retry_cleanup().unwrap();
            assert!(c.stages().is_empty());
        }
        #[test]
        fn changed_destination_parent_during_stream_stops_and_retains_exact_stage() {
            let c = Case::new(
                "archive-exec-parent-in-stream",
                archive(&vec![b'x'; 200000]),
            );
            let old = c.base.join("moved-dest");
            let mut changed = false;
            let error = c
                .prepare()
                .execute(&Control::default(), |p| {
                    if p.bytes > 0 && !changed {
                        changed = true;
                        fs::rename(&c.dest, &old).unwrap();
                        fs::create_dir(&c.dest).unwrap();
                    }
                })
                .unwrap_err();
            assert_eq!(error.cleanup, Cleanup::Retained);
            assert!(fs::read_dir(&c.dest).unwrap().next().is_none());
            let root = error.rescue.clone().unwrap();
            assert!(root.starts_with(&old));
            assert!(!c.dest.join("out").exists());
        }
        #[test]
        fn replacement_in_rename_gap_is_uncertain_without_relocating_foreign_as_rollback() {
            let c = Case::new("archive-exec-rename-gap", archive(b"data"));
            let held = c.base.join("held-stage");
            let error = c
                .prepare()
                .run(
                    &Control::default(),
                    |_| {},
                    |point, stage| {
                        if matches!(point, Point::RenameCall) {
                            let root = stage.location().unwrap();
                            fs::rename(&root, &held)?;
                            fs::create_dir(&root)?;
                            fs::write(root.join("foreign"), b"new inode")?;
                        }
                        Ok(())
                    },
                )
                .unwrap_err();
            assert_eq!(error.publication, Publication::Uncertain);
            assert_eq!(error.cleanup, Cleanup::Retained);
            assert_eq!(fs::read(c.dest.join("out/foreign")).unwrap(), b"new inode");
            assert_eq!(fs::read(held.join("a/one")).unwrap(), b"data");
            assert_eq!(error.rescue, Some(held));
        }
        #[test]
        fn recovery_sync_needs_no_archive_and_bounds_observer_diagnostic() {
            let c = Case::new("archive-recovery-no-source", archive(b"data"));
            let error = c
                .prepare()
                .execute_observing_publish(
                    &Control::default(),
                    |_| {},
                    || Err(std::io::Error::other("é".repeat(4096))),
                )
                .unwrap_err();
            assert!(error.cause.to_string().len() <= 512);
            assert!(error.can_retry_sync());
            let inode = fs::metadata(c.dest.join("out")).unwrap().ino();
            fs::remove_file(&c.source).unwrap();
            let cancel = Control::default();
            cancel.cancel();
            let error = error.retry_sync_controlled(&cancel).unwrap_err();
            assert!(error.can_retry_sync());
            error.retry_sync().unwrap();
            assert_eq!(fs::metadata(c.dest.join("out")).unwrap().ino(), inode);
            assert_eq!(fs::read(c.dest.join("out/a/one")).unwrap(), b"data");
        }
        #[test]
        fn recovery_cleanup_cancel_after_unlink_keeps_ledger_for_retry() {
            let c = Case::new("archive-recovery-cleanup-cancel", archive(b"data"));
            let error = c
                .prepare()
                .run(
                    &Control::default(),
                    |_| {},
                    |point, stage| {
                        if matches!(point, Point::BeforePublish) {
                            fs::write(stage.location().unwrap().join("foreign"), b"keep")?;
                            return Err(std::io::Error::other("private prepublish failure"));
                        }
                        Ok(())
                    },
                )
                .unwrap_err();
            let root = error.rescue.clone().unwrap();
            let held = c.dest.join("foreign-preserved");
            fs::rename(root.join("foreign"), &held).unwrap();
            let control = Control::default();
            let mut removed = 0;
            let error = error
                .retry_cleanup_controlled(&control, |n| {
                    removed = n;
                    control.cancel();
                })
                .unwrap_err();
            assert_eq!(removed, 1);
            assert!(root.exists());
            assert!(error.can_retry_cleanup());
            error.retry_cleanup().unwrap();
            assert!(!root.exists());
            assert_eq!(fs::read(held).unwrap(), b"keep");
            assert!(!c.dest.join("out").exists());
        }
        #[test]
        fn post_publish_sync_error_is_not_replayed_or_cleaned_and_sync_retry_is_only_sync() {
            let c = Case::new("archive-exec-unsynced", archive(b"data"));
            let source = fs::read(&c.source).unwrap();
            let error = c
                .prepare()
                .run(
                    &Control::default(),
                    |_| {},
                    |point, _| {
                        if matches!(point, Point::ParentSync) {
                            Err(std::io::Error::other(
                                "synthetic before-parent-sync failure",
                            ))
                        } else {
                            Ok(())
                        }
                    },
                )
                .unwrap_err();
            assert_eq!(error.publication, Publication::PublishedUnsynced);
            assert_eq!(error.cleanup, Cleanup::NotNeeded);
            assert!(error.can_retry_sync() && !error.can_retry_cleanup());
            let ino = fs::metadata(c.dest.join("out")).unwrap().ino();
            let done = error.retry_sync().unwrap();
            assert_eq!(done.bytes, 13);
            assert_eq!(fs::metadata(c.dest.join("out")).unwrap().ino(), ino);
            assert_eq!(fs::read(&c.source).unwrap(), source);
            assert!(c.stages().is_empty());
        }
        #[test]
        fn sync_retry_refuses_replaced_published_name_without_touching_occupant() {
            let c = Case::new("archive-exec-unsynced-replaced", archive(b"data"));
            let error = c
                .prepare()
                .run(
                    &Control::default(),
                    |_| {},
                    |point, _| {
                        if matches!(point, Point::ParentSync) {
                            Err(std::io::Error::other("synthetic sync fail"))
                        } else {
                            Ok(())
                        }
                    },
                )
                .unwrap_err();
            let held = c.dest.join("held");
            fs::rename(c.dest.join("out"), &held).unwrap();
            fs::create_dir(c.dest.join("out")).unwrap();
            fs::write(c.dest.join("out/foreign"), b"untouched").unwrap();
            let error = error.retry_sync().unwrap_err();
            assert_eq!(fs::read(c.dest.join("out/foreign")).unwrap(), b"untouched");
            assert!(held.join("a/one").exists());
            assert!(error.can_retry_sync());
        }
        #[test]
        fn removed_stage_pending_directory_sync_never_deletes_reused_stage_name() {
            let c = Case::new("archive-exec-cleanup-sync", archive(b"data"));
            let mut error = c
                .prepare()
                .run(
                    &Control::default(),
                    |_| {},
                    |point, stage| {
                        if matches!(point, Point::BeforePublish) {
                            fs::write(stage.location().unwrap().join("foreign"), b"keep")?;
                            return Err(std::io::Error::other("synthetic precommit"));
                        }
                        Ok(())
                    },
                )
                .unwrap_err();
            let root = error.rescue.clone().unwrap();
            fs::remove_file(root.join("foreign")).unwrap();
            let Some(Recovery::Stage(stage)) = error.recovery.take() else {
                panic!()
            };
            let error = failed_with_cleanup(
                stage,
                std::io::Error::other("test failure"),
                0,
                Publication::Unpublished,
                None,
                |s| {
                    s.cleanup_with_sync(|_| {
                        Err(std::io::Error::other(
                            "synthetic cleanup parent sync failure",
                        ))
                    })
                },
            );
            assert_eq!(error.cleanup, Cleanup::CompleteUnsynced);
            assert!(!root.exists());
            assert!(error.rescue.is_none());
            fs::create_dir(&root).unwrap();
            fs::write(root.join("new occupant"), b"untouched").unwrap();
            error.retry_cleanup().unwrap();
            assert_eq!(fs::read(root.join("new occupant")).unwrap(), b"untouched");
        }
        #[test]
        fn malformed_archive_and_unsafe_names_never_create_destination_staging() {
            let c = Case::new(
                "archive-exec-unsafe-before-stage",
                zip(&[Spec::file(b"../escape", b"x")]),
            );
            assert!(
                Prepared::capture(
                    &c.source,
                    &c.dest,
                    OsStr::new("out"),
                    Budget::default(),
                    &Control::default()
                )
                .is_err()
            );
            assert!(fs::read_dir(&c.dest).unwrap().next().is_none());
            fs::write(&c.source, b"not ZIP").unwrap();
            assert!(
                Prepared::capture(
                    &c.source,
                    &c.dest,
                    OsStr::new("out"),
                    Budget::default(),
                    &Control::default()
                )
                .is_err()
            );
            assert!(fs::read_dir(&c.dest).unwrap().next().is_none());
        }
        #[test]
        fn root_permission_change_prevents_publish_and_no_implicit_permission_repair() {
            let c = Case::new("archive-exec-root-mode", archive(b"data"));
            let error = c
                .prepare()
                .run(
                    &Control::default(),
                    |_| {},
                    |point, stage| {
                        if matches!(point, Point::BeforePublish) {
                            fs::set_permissions(
                                stage.location().unwrap(),
                                fs::Permissions::from_mode(0o755),
                            )?;
                        }
                        Ok(())
                    },
                )
                .unwrap_err();
            assert_eq!(error.cleanup, Cleanup::Retained);
            let root = error.rescue.clone().unwrap();
            assert_eq!(fs::metadata(&root).unwrap().mode() & 0o777, 0o755);
            assert!(!c.dest.join("out").exists());
            fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
            error.retry_cleanup().unwrap();
        }
    }
}
#[cfg(target_os = "linux")]
pub use linux::{Cleanup, Completed, Failure, Prepared, Progress, Publication};
