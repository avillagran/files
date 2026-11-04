//! Linux descriptor-bound streaming ZIP32 creation. No source deletion or merge.
//! Revisions detect observed changes, not an immutable filesystem snapshot.
#[cfg(target_os = "linux")]
mod linux {
    use crate::copy_engine::Control;
    use flate2::{Compression, write::DeflateEncoder};
    use std::{
        ffi::{CStr, CString, OsStr, OsString},
        fs::{File, Metadata, OpenOptions},
        io::{self, Read, Seek, Write},
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::{
                ffi::OsStrExt,
                fs::{MetadataExt, OpenOptionsExt},
            },
        },
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
    };
    const MAX_ENTRIES: usize = 4096;
    const MAX_NAMES: usize = 512 * 1024;
    const MAX_BYTES: u64 = 3 * 1024 * 1024 * 1024;
    const MAX_DEPTH: usize = 32;
    const CHUNK: usize = 64 * 1024;
    fn invalid(s: &'static str) -> io::Error {
        io::Error::other(s)
    }
    fn c(b: &[u8]) -> io::Result<CString> {
        CString::new(b).map_err(|_| invalid("NUL en ruta ZIP"))
    }
    fn owned(fd: i32) -> io::Result<File> {
        if fd < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(unsafe { File::from_raw_fd(fd) })
        }
    }
    fn directory(path: &Path) -> io::Result<File> {
        if !path.is_absolute() || path.as_os_str().as_bytes().len() > 4096 {
            return Err(invalid("ZIP requiere ruta absoluta <=4096B"));
        }
        OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
            .open(path)
    }
    #[repr(C)]
    struct How {
        flags: u64,
        mode: u64,
        resolve: u64,
    }
    fn open(root: &File, path: &[u8], flags: i32) -> io::Result<File> {
        let how = How {
            flags: (flags
                | libc::O_CLOEXEC
                | libc::O_NOFOLLOW
                | if flags & libc::O_PATH != 0 {
                    0
                } else {
                    libc::O_NONBLOCK
                }) as u64,
            mode: 0,
            resolve: 0x01 | 0x02 | 0x04 | 0x08,
        };
        let fd = unsafe {
            libc::syscall(
                libc::SYS_openat2,
                root.as_raw_fd(),
                c(path)?.as_ptr(),
                &how,
                std::mem::size_of::<How>(),
            )
        };
        owned(fd as i32)
    }
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct Revision {
        dev: u64,
        ino: u64,
        size: u64,
        mode: u32,
        links: u64,
        mtime: i64,
        mns: i64,
        ctime: i64,
        cns: i64,
    }
    impl Revision {
        fn of(m: &Metadata) -> Self {
            Self {
                dev: m.dev(),
                ino: m.ino(),
                size: m.len(),
                mode: m.mode(),
                links: m.nlink(),
                mtime: m.mtime(),
                mns: m.mtime_nsec(),
                ctime: m.ctime(),
                cns: m.ctime_nsec(),
            }
        }
        fn identity(self) -> (u64, u64) {
            (self.dev, self.ino)
        }
        fn kind(self) -> u32 {
            self.mode & libc::S_IFMT
        }
    }
    fn parent_identity(file: &File) -> io::Result<(u64, u64, u64)> {
        let expected = Revision::of(&file.metadata()?);
        let mut stat = std::mem::MaybeUninit::<libc::statx>::zeroed();
        if unsafe {
            libc::statx(
                file.as_raw_fd(),
                c"".as_ptr(),
                libc::AT_EMPTY_PATH | libc::AT_SYMLINK_NOFOLLOW,
                libc::STATX_BASIC_STATS | libc::STATX_MNT_ID,
                stat.as_mut_ptr(),
            )
        } != 0
        {
            return Err(io::Error::last_os_error());
        }
        let stat = unsafe { stat.assume_init() };
        if stat.stx_mask & libc::STATX_MNT_ID == 0
            || stat.stx_mnt_id == 0
            || libc::makedev(stat.stx_dev_major, stat.stx_dev_minor) != expected.dev
            || stat.stx_ino != expected.ino
            || u32::from(stat.stx_mode) & libc::S_IFMT != libc::S_IFDIR
        {
            return Err(invalid("ZIP: identidad/montaje de padre no verificado"));
        }
        Ok((expected.dev, expected.ino, stat.stx_mnt_id))
    }
    #[derive(Debug)]
    struct Entry {
        name: Vec<u8>,
        revision: Revision,
    }
    struct Stream(*mut libc::DIR);
    impl Drop for Stream {
        fn drop(&mut self) {
            unsafe {
                libc::closedir(self.0);
            }
        }
    }
    fn names(
        dir: &File,
        control: &Control,
        mut each: impl FnMut(&[u8]) -> io::Result<()>,
    ) -> io::Result<()> {
        let fd = unsafe {
            libc::openat(
                dir.as_raw_fd(),
                c(b".")?.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let raw = unsafe { libc::fdopendir(fd) };
        if raw.is_null() {
            let error = io::Error::last_os_error();
            unsafe {
                libc::close(fd);
            }
            return Err(error);
        }
        let stream = Stream(raw);
        loop {
            control.checkpoint()?;
            unsafe {
                *libc::__errno_location() = 0;
            }
            let ent = unsafe { libc::readdir(stream.0) };
            if ent.is_null() {
                let e = io::Error::last_os_error();
                return if e.raw_os_error() == Some(0) {
                    Ok(())
                } else {
                    Err(e)
                };
            }
            let n = unsafe { CStr::from_ptr((*ent).d_name.as_ptr()) }.to_bytes();
            if n != b"." && n != b".." {
                each(n)?;
            }
        }
    }
    #[derive(Debug)]
    pub struct Prepared {
        source: File,
        parent: File,
        source_path: PathBuf,
        parent_path: PathBuf,
        source_id: (u64, u64, u64),
        parent_id: (u64, u64, u64),
        name: OsString,
        entries: Vec<Entry>,
        bytes: u64,
        name_bytes: usize,
    }
    #[derive(Debug)]
    pub struct Completed {
        pub destination: PathBuf,
        pub bytes: u64,
        pub files: usize,
        pub directories: usize,
        pub links: usize,
    }
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Publication {
        Unpublished,
        PublishedUnsynced,
        Uncertain,
    }
    #[derive(Debug)]
    pub struct Failure {
        pub cause: io::Error,
        pub cancelled: bool,
        pub stage: Option<PathBuf>,
        pub destination: PathBuf,
        pub publication: Publication,
        pub cleanup_error: Option<io::Error>,
    }
    impl std::fmt::Display for Failure {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(
                f,
                "{}\nPublicación: {}\nDestino observado: {:?}",
                if self.cancelled {
                    "Creación cancelada".into()
                } else {
                    self.cause.to_string()
                },
                match self.publication {
                    Publication::Unpublished => "sin publicar",
                    Publication::PublishedUnsynced => "publicado; guardado padre pendiente",
                    Publication::Uncertain => "sin confirmar; conservar rutas",
                },
                self.destination
            )?;
            if self.publication == Publication::Unpublished
                && self.stage.is_none()
                && self.cleanup_error.is_none()
            {
                write!(f, "\nSin temporales retenidos")?;
            }
            if let Some(p) = &self.stage {
                write!(f, "\nRuta provisional observada (conservar): {p:?}")?;
            }
            if let Some(e) = &self.cleanup_error {
                write!(f, "\nLimpieza/guardado no confirmado: {e}")?;
                if self.stage.is_none() {
                    write!(f, "\nTemporal eliminado; guardado del padre pendiente")?;
                }
            }
            Ok(())
        }
    }
    impl Prepared {
        pub fn capture(
            source: &Path,
            selected: &[OsString],
            parent: &Path,
            name: &OsStr,
            control: &Control,
        ) -> io::Result<Self> {
            if selected.is_empty()
                || selected.len() > 128
                || name.as_bytes().is_empty()
                || name.as_bytes().len() > 255
                || name.as_bytes().contains(&b'/')
                || matches!(name.as_bytes(), b"." | b"..")
            {
                return Err(invalid("ZIP: selección o nombre inválido (128 raíces)"));
            }
            let source_pin = directory(source)?;
            let parent_pin = directory(parent)?;
            let mut p = Self {
                source_id: parent_identity(&source_pin)?,
                parent_id: parent_identity(&parent_pin)?,
                source: source_pin,
                parent: parent_pin,
                source_path: source.into(),
                parent_path: parent.into(),
                name: name.into(),
                entries: Vec::new(),
                bytes: 0,
                name_bytes: 0,
            };
            p.absent()?;
            let mut roots = std::collections::HashSet::new();
            for n in selected {
                let b = n.as_bytes();
                if b.is_empty()
                    || b.len() > 255
                    || b.contains(&b'/')
                    || b == b"."
                    || b == b".."
                    || !roots.insert(b)
                {
                    return Err(invalid("ZIP: raíz inválida o repetida"));
                }
                p.walk(b, 0, control)?;
            }
            p.entries.sort_by(|a, b| a.name.cmp(&b.name));
            // Destination may not be inside a selected directory, including aliases.
            let selected_dirs: std::collections::HashSet<_> = p
                .entries
                .iter()
                .filter(|e| e.revision.kind() == libc::S_IFDIR)
                .map(|e| e.revision.identity())
                .collect();
            let mut ancestor = p.parent.try_clone()?;
            for depth in 0..=128 {
                control.checkpoint()?;
                let id = Revision::of(&ancestor.metadata()?).identity();
                if selected_dirs.contains(&id) {
                    return Err(invalid("ZIP: destino dentro de carpeta seleccionada"));
                }
                let up = owned(unsafe {
                    libc::openat(
                        ancestor.as_raw_fd(),
                        c(b"..")?.as_ptr(),
                        libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                    )
                })?;
                if Revision::of(&up.metadata()?).identity() == id {
                    break;
                }
                if depth == 128 {
                    return Err(invalid("ZIP: ancestros destino exceden128"));
                }
                ancestor = up;
            }
            p.verify(control)?;
            Ok(p)
        }
        fn walk(&mut self, path: &[u8], depth: usize, control: &Control) -> io::Result<()> {
            control.checkpoint()?;
            if crate::archive_list::unsafe_name(path) {
                return Err(invalid("ZIP: nombre ambiguo/traversal no admitido"));
            }
            if depth > MAX_DEPTH || path.len() > 4095 || self.entries.len() >= MAX_ENTRIES {
                return Err(invalid("ZIP: límite4096 entradas/32 niveles/4095B"));
            }
            let pin = open(&self.source, path, libc::O_PATH)?;
            let rev = Revision::of(&pin.metadata()?);
            drop(pin);
            if !matches!(rev.kind(), libc::S_IFREG | libc::S_IFDIR | libc::S_IFLNK) {
                return Err(invalid(
                    "ZIP sólo admite archivos, carpetas y enlaces literales",
                ));
            }
            self.name_bytes = self
                .name_bytes
                .checked_add(path.len() + 1)
                .filter(|n| *n <= MAX_NAMES)
                .ok_or_else(|| invalid("ZIP: nombres superan512KiB"))?;
            if rev.kind() == libc::S_IFREG {
                self.bytes = self
                    .bytes
                    .checked_add(rev.size)
                    .filter(|n| *n <= MAX_BYTES)
                    .ok_or_else(|| invalid("ZIP32: contenido supera3GiB"))?;
            }
            self.entries.push(Entry {
                name: path.to_vec(),
                revision: rev,
            });
            if rev.kind() == libc::S_IFDIR {
                let dir = open(&self.source, path, libc::O_RDONLY | libc::O_DIRECTORY)?;
                if Revision::of(&dir.metadata()?) != rev {
                    return Err(invalid("ZIP: carpeta cambió durante revisión"));
                }
                names(&dir, control, |name| {
                    let len = path.len() + 1 + name.len();
                    if len > 4095 {
                        return Err(invalid("ZIP: ruta demasiado larga"));
                    }
                    let mut child = Vec::with_capacity(len);
                    child.extend_from_slice(path);
                    child.push(b'/');
                    child.extend_from_slice(name);
                    self.walk(&child, depth + 1, control)
                })?;
                if Revision::of(&dir.metadata()?) != rev {
                    return Err(invalid("ZIP: carpeta cambió durante recorrido"));
                }
            }
            Ok(())
        }
        fn parents(&self) -> io::Result<()> {
            if parent_identity(&directory(&self.source_path)?)? != self.source_id
                || parent_identity(&directory(&self.parent_path)?)? != self.parent_id
            {
                return Err(invalid("ZIP: padre sustituido"));
            }
            Ok(())
        }
        fn verify(&self, control: &Control) -> io::Result<()> {
            self.parents()?;
            for e in &self.entries {
                control.checkpoint()?;
                if Revision::of(&open(&self.source, &e.name, libc::O_PATH)?.metadata()?)
                    != e.revision
                {
                    return Err(invalid("ZIP: origen cambió desde revisión"));
                }
            }
            Ok(())
        }
        fn absent(&self) -> io::Result<()> {
            let mut s = std::mem::MaybeUninit::<libc::stat>::uninit();
            let rc = unsafe {
                libc::fstatat(
                    self.parent.as_raw_fd(),
                    c(self.name.as_bytes())?.as_ptr(),
                    s.as_mut_ptr(),
                    libc::AT_SYMLINK_NOFOLLOW,
                )
            };
            if rc == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "ZIP: destino ocupado",
                ));
            }
            let e = io::Error::last_os_error();
            if e.raw_os_error() != Some(libc::ENOENT) {
                return Err(e);
            }
            Ok(())
        }
        pub fn retained_bytes(&self) -> usize {
            std::mem::size_of::<Self>()
                + self.entries.capacity() * std::mem::size_of::<Entry>()
                + self
                    .entries
                    .iter()
                    .map(|e| e.name.capacity())
                    .sum::<usize>()
                + self.source_path.as_os_str().len()
                + self.parent_path.as_os_str().len()
                + self.name.len()
        }
        pub fn entries(&self) -> usize {
            self.entries.len()
        }
        pub fn bytes(&self) -> u64 {
            self.bytes
        }
        pub fn destination(&self) -> PathBuf {
            self.parent_path.join(&self.name)
        }
        pub fn execute(
            self,
            control: &Control,
            mut progress: impl FnMut(u64, u64),
        ) -> Result<Completed, Box<Failure>> {
            let destination = self.destination();
            let mut stage = None;
            let mut output = None;
            let mut output_id = None;
            let mut published = false;
            let mut publication = Publication::Unpublished;
            let result = (|| -> io::Result<Completed> {
                self.verify(control)?;
                self.absent()?;
                control.checkpoint()?;
                static NEXT: AtomicU64 = AtomicU64::new(1);
                for _ in 0..32 {
                    let n = NEXT.fetch_add(1, Ordering::Relaxed);
                    let name = OsString::from(format!(".files-zip-{}-{n}", std::process::id()));
                    let fd = unsafe {
                        libc::openat(
                            self.parent.as_raw_fd(),
                            c(name.as_bytes())?.as_ptr(),
                            libc::O_RDWR
                                | libc::O_CREAT
                                | libc::O_EXCL
                                | libc::O_NOFOLLOW
                                | libc::O_CLOEXEC,
                            0o600,
                        )
                    };
                    if fd >= 0 {
                        let f = owned(fd)?;
                        stage = Some(name);
                        output = Some(f);
                        output_id =
                            Some(Revision::of(&output.as_ref().unwrap().metadata()?).identity());
                        break;
                    }
                    let e = io::Error::last_os_error();
                    if e.raw_os_error() != Some(libc::EEXIST) {
                        return Err(e);
                    }
                }
                let out = output
                    .as_mut()
                    .ok_or_else(|| invalid("ZIP: no se pudo reservar temporal"))?;
                let mut central = Vec::with_capacity(self.entries.len());
                let mut total = 0u64;
                let mut files = 0;
                let mut dirs = 0;
                let mut links = 0;
                let mut buffer = [0u8; CHUNK];
                for (index, e) in self.entries.iter().enumerate() {
                    control.checkpoint()?;
                    let flags = if std::str::from_utf8(&e.name).is_ok() {
                        0x808u16
                    } else {
                        8
                    };
                    let mut name = e.name.clone();
                    if e.revision.kind() == libc::S_IFDIR {
                        name.push(b'/');
                    }
                    let offset = u32::try_from(out.stream_position()?)
                        .map_err(|_| invalid("ZIP32: offset excedido"))?;
                    let method = if e.revision.kind() == libc::S_IFDIR {
                        0
                    } else {
                        8
                    };
                    let date = dos_date(e.revision.mtime);
                    write_local(out, &name, flags, method, date)?;
                    let start = out.stream_position()?;
                    let mut hash = crc32fast::Hasher::new();
                    let mut size = 0u64;
                    if method == 8 {
                        let mut enc = DeflateEncoder::new(&mut *out, Compression::fast());
                        if e.revision.kind() == libc::S_IFLNK {
                            let pin = open(&self.source, &e.name, libc::O_PATH)?;
                            if Revision::of(&pin.metadata()?) != e.revision {
                                return Err(invalid("ZIP: enlace sustituido"));
                            }
                            let mut bytes = [0u8; 4097];
                            let n = unsafe {
                                libc::readlinkat(
                                    pin.as_raw_fd(),
                                    c(b"")?.as_ptr(),
                                    bytes.as_mut_ptr().cast(),
                                    bytes.len(),
                                )
                            };
                            if n < 0 {
                                return Err(io::Error::last_os_error());
                            }
                            let n = n as usize;
                            if n > 4096 {
                                return Err(invalid("ZIP: enlace demasiado largo"));
                            }
                            enc.write_all(&bytes[..n])?;
                            hash.update(&bytes[..n]);
                            size = n as u64;
                            links += 1;
                        } else {
                            let mut input = open(&self.source, &e.name, libc::O_RDONLY)?;
                            if Revision::of(&input.metadata()?) != e.revision {
                                return Err(invalid("ZIP: archivo sustituido"));
                            }
                            while size < e.revision.size {
                                control.checkpoint()?;
                                let count = ((e.revision.size - size).min(CHUNK as u64)) as usize;
                                let n = match input.read(&mut buffer[..count]) {
                                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                                    result => result?,
                                };
                                if n == 0 {
                                    return Err(invalid("ZIP: origen truncado"));
                                }
                                enc.write_all(&buffer[..n])?;
                                hash.update(&buffer[..n]);
                                size += n as u64;
                                total += n as u64;
                                progress(total, index as u64);
                            }
                            if Revision::of(&input.metadata()?) != e.revision {
                                return Err(invalid("ZIP: origen modificado durante lectura"));
                            }
                            files += 1;
                        }
                        enc.finish()?;
                    } else {
                        dirs += 1;
                    }
                    let packed = u32::try_from(out.stream_position()? - start)
                        .map_err(|_| invalid("ZIP32: miembro comprimido demasiado grande"))?;
                    let crc = hash.finalize();
                    let size =
                        u32::try_from(size).map_err(|_| invalid("ZIP32: tamaño excedido"))?;
                    put32(out, 0x08074b50)?;
                    put32(out, crc)?;
                    put32(out, packed)?;
                    put32(out, size)?;
                    central.push((offset, flags, method, crc, packed, size, date));
                    progress(total, index as u64 + 1);
                }
                self.verify(control)?;
                let start = u32::try_from(out.stream_position()?)
                    .map_err(|_| invalid("ZIP32: offset excedido"))?;
                for (e, record) in self.entries.iter().zip(central) {
                    control.checkpoint()?;
                    write_central(out, e, record)?;
                }
                let end = out.stream_position()?;
                let span = u32::try_from(end - u64::from(start))
                    .map_err(|_| invalid("ZIP32: catálogo excedido"))?;
                if end + 22 > u64::from(u32::MAX) {
                    return Err(invalid("ZIP32: archivo final excedido"));
                }
                put32(out, 0x06054b50)?;
                for n in [0, 0, self.entries.len() as u16, self.entries.len() as u16] {
                    put16(out, n)?;
                }
                put32(out, span)?;
                put32(out, start)?;
                put16(out, 0)?;
                out.sync_all()?;
                self.verify(control)?;
                self.absent()?;
                control.checkpoint()?;
                binding(&self.parent, stage.as_ref().unwrap(), output_id.unwrap())?;
                let rc = unsafe {
                    libc::syscall(
                        libc::SYS_renameat2,
                        self.parent.as_raw_fd(),
                        c(stage.as_ref().unwrap().as_bytes())?.as_ptr(),
                        self.parent.as_raw_fd(),
                        c(self.name.as_bytes())?.as_ptr(),
                        libc::RENAME_NOREPLACE,
                    )
                };
                if rc != 0 {
                    let error = io::Error::last_os_error();
                    if binding(&self.parent, stage.as_ref().unwrap(), output_id.unwrap()).is_err() {
                        published = true;
                        publication = Publication::Uncertain;
                    }
                    return Err(error);
                }
                published = true;
                publication = Publication::Uncertain;
                self.parents()?;
                binding(&self.parent, &self.name, output_id.unwrap())?;
                publication = Publication::PublishedUnsynced;
                self.parent.sync_all()?;
                publication = Publication::Uncertain;
                self.parents()?;
                binding(&self.parent, &self.name, output_id.unwrap())?;
                Ok(Completed {
                    destination: destination.clone(),
                    bytes: total,
                    files,
                    directories: dirs,
                    links,
                })
            })();
            result.map_err(|cause| {
                let mut cleanup_error = None;
                let mut retained = None;
                if !published && let (Some(name), Some(id)) = (&stage, output_id) {
                    let mut removed = false;
                    let cleanup = (|| {
                        binding(&self.parent, name, id)?;
                        if unsafe {
                            libc::unlinkat(self.parent.as_raw_fd(), c(name.as_bytes())?.as_ptr(), 0)
                        } != 0
                        {
                            return Err(io::Error::last_os_error());
                        }
                        removed = true;
                        self.parent.sync_all()
                    })();
                    if let Err(e) = cleanup {
                        if !removed {
                            retained = Some(self.parent_path.join(name));
                        }
                        cleanup_error = Some(e);
                    }
                } else if !published || publication == Publication::Uncertain {
                    retained = stage.as_ref().map(|n| self.parent_path.join(n));
                }
                let cancelled =
                    control.is_cancelled() && cause.kind() == io::ErrorKind::Interrupted;
                Box::new(Failure {
                    cause,
                    cancelled,
                    stage: retained,
                    destination,
                    publication,
                    cleanup_error,
                })
            })
        }
    }
    fn binding(parent: &File, name: &OsStr, id: (u64, u64)) -> io::Result<()> {
        let f = open(parent, name.as_bytes(), libc::O_PATH)?;
        let r = Revision::of(&f.metadata()?);
        if r.identity() != id || r.kind() != libc::S_IFREG || r.links != 1 {
            return Err(invalid(
                "ZIP: temporal/publicación sustituido; conservar ocupante",
            ));
        }
        Ok(())
    }
    // DOS fields use UTC, two-second precision; representable years clamp to1980/2107.
    fn dos_date(seconds: i64) -> (u16, u16) {
        let time = seconds as libc::time_t;
        let mut raw = std::mem::MaybeUninit::<libc::tm>::uninit();
        if unsafe { libc::gmtime_r(&time, raw.as_mut_ptr()) }.is_null() {
            return (0, 33);
        }
        let t = unsafe { raw.assume_init() };
        let year = t.tm_year + 1900;
        if year < 1980 {
            return (0, 33);
        }
        if year > 2107 {
            return ((23 << 11) | (59 << 5) | 29, (127 << 9) | (12 << 5) | 31);
        }
        (
            ((t.tm_hour << 11) | (t.tm_min << 5) | (t.tm_sec.min(59) / 2)) as u16,
            (((year - 1980) << 9) | ((t.tm_mon + 1) << 5) | t.tm_mday) as u16,
        )
    }
    fn put16(w: &mut impl Write, n: u16) -> io::Result<()> {
        w.write_all(&n.to_le_bytes())
    }
    fn put32(w: &mut impl Write, n: u32) -> io::Result<()> {
        w.write_all(&n.to_le_bytes())
    }
    fn write_local(
        w: &mut impl Write,
        name: &[u8],
        flags: u16,
        method: u16,
        date: (u16, u16),
    ) -> io::Result<()> {
        put32(w, 0x04034b50)?;
        for n in [20, flags, method, date.0, date.1] {
            put16(w, n)?;
        }
        for n in [0, 0, 0] {
            put32(w, n)?;
        }
        put16(w, name.len() as u16)?;
        put16(w, 0)?;
        w.write_all(name)
    }
    type Record = (u32, u16, u16, u32, u32, u32, (u16, u16));
    fn write_central(
        w: &mut impl Write,
        e: &Entry,
        (offset, flags, method, crc, packed, size, date): Record,
    ) -> io::Result<()> {
        let dir = e.revision.kind() == libc::S_IFDIR;
        put32(w, 0x02014b50)?;
        for n in [0x0314, 20, flags, method, date.0, date.1] {
            put16(w, n)?;
        }
        for n in [crc, packed, size] {
            put32(w, n)?;
        }
        for n in [(e.name.len() + usize::from(dir)) as u16, 0, 0, 0, 0] {
            put16(w, n)?;
        }
        put32(
            w,
            ((e.revision.mode & (libc::S_IFMT | 0o777)) << 16) | if dir { 0x10 } else { 0 },
        )?;
        put32(w, offset)?;
        w.write_all(&e.name)?;
        if dir {
            w.write_all(b"/")?;
        }
        Ok(())
    }
    #[cfg(test)]
    mod tests {
        use super::*;
        use std::{
            fs,
            os::unix::{
                ffi::OsStringExt,
                fs::{PermissionsExt, symlink},
            },
            sync::atomic::AtomicU64,
        };
        struct Case(PathBuf);
        impl Case {
            fn new() -> Self {
                static N: AtomicU64 = AtomicU64::new(0);
                let p = crate::theme::home().join(".cache/files-test").join(format!(
                    "create-zip-{}-{}",
                    std::process::id(),
                    N.fetch_add(1, Ordering::Relaxed)
                ));
                fs::create_dir_all(p.join("source/tree/empty")).unwrap();
                fs::create_dir(p.join("target")).unwrap();
                fs::write(p.join("source/tree/a"), b"contents").unwrap();
                fs::write(p.join("source/zero"), b"").unwrap();
                Self(p)
            }
            fn prepare(&self, names: &[&str]) -> io::Result<Prepared> {
                Prepared::capture(
                    &self.0.join("source"),
                    &names.iter().map(OsString::from).collect::<Vec<_>>(),
                    &self.0.join("target"),
                    OsStr::new("out.zip"),
                    &Control::default(),
                )
            }
            fn stages(&self) -> Vec<PathBuf> {
                fs::read_dir(self.0.join("target"))
                    .unwrap()
                    .map(|e| e.unwrap().path())
                    .filter(|p| {
                        p.file_name()
                            .unwrap()
                            .as_bytes()
                            .starts_with(b".files-zip-")
                    })
                    .collect()
            }
        }
        impl Drop for Case {
            fn drop(&mut self) {
                fs::remove_dir_all(&self.0).unwrap();
            }
        }
        #[test]
        fn streaming_zip_roundtrip_independent_python_crc_modes_links_raw() {
            let c = Case::new();
            let raw = OsString::from_vec(b"raw-\xff".to_vec());
            fs::write(c.0.join("source").join(&raw), b"raw").unwrap();
            fs::write(c.0.join("source/tree/a"), vec![b'X'; 200000]).unwrap();
            fs::set_permissions(
                c.0.join("source/tree/a"),
                fs::Permissions::from_mode(0o4751),
            )
            .unwrap();
            symlink("../../outside", c.0.join("source/link")).unwrap();
            let names = vec!["tree".into(), "zero".into(), "link".into(), raw];
            let p = Prepared::capture(
                &c.0.join("source"),
                &names,
                &c.0.join("target"),
                OsStr::new("out.zip"),
                &Control::default(),
            )
            .unwrap();
            assert!(c.stages().is_empty());
            assert!(!c.0.join("target/out.zip").exists());
            let mut steps = 0;
            let done = p.execute(&Control::default(), |_, _| steps += 1).unwrap();
            assert!(steps >= 4);
            assert_eq!(
                (done.files, done.directories, done.links, done.bytes),
                (3, 2, 1, 200003)
            );
            let script = r#"import zipfile,sys,stat
with zipfile.ZipFile(sys.argv[1]) as z:
 assert z.testzip() is None
 assert z.read('tree/a') == b'X'*200000
 assert z.read('zero') == b''
 assert z.read('link') == b'../../outside'
 assert stat.S_ISLNK(z.getinfo('link').external_attr >> 16)
 assert (z.getinfo('tree/a').external_attr >> 16) & 0o7777 == 0o751
 assert z.getinfo('tree/empty/').is_dir()
 raw = next(i for i in z.infolist() if i.filename.startswith('raw-'))
 assert raw.filename.encode('cp437') == b'raw-\xff' and z.read(raw) == b'raw'
"#;
            assert!(
                std::process::Command::new("python3")
                    .args(["-c", script])
                    .arg(done.destination)
                    .status()
                    .unwrap()
                    .success()
            );
            assert!(c.stages().is_empty());
        }
        #[test]
        fn source_replaced_after_review_fails_before_staging() {
            let c = Case::new();
            let p = c.prepare(&["tree"]).unwrap();
            fs::rename(c.0.join("source/tree/a"), c.0.join("held")).unwrap();
            fs::write(c.0.join("source/tree/a"), b"replacement").unwrap();
            let e = p.execute(&Control::default(), |_, _| {}).unwrap_err();
            assert_eq!(e.publication, Publication::Unpublished);
            assert!(c.stages().is_empty());
            assert!(!c.0.join("target/out.zip").exists());
        }
        #[test]
        fn cancellation_after_positive_progress_removes_only_owned_stage() {
            let c = Case::new();
            fs::write(c.0.join("source/tree/a"), vec![42; 200000]).unwrap();
            let p = c.prepare(&["tree"]).unwrap();
            let ctl = Control::default();
            let e = p
                .execute(&ctl, |bytes, _| {
                    if bytes > 0 {
                        ctl.cancel()
                    }
                })
                .unwrap_err();
            assert_eq!(e.cause.kind(), io::ErrorKind::Interrupted);
            assert!(c.stages().is_empty());
            assert_eq!(fs::read(c.0.join("source/tree/a")).unwrap().len(), 200000);
            assert!(!c.0.join("target/out.zip").exists());
        }
        #[test]
        fn late_destination_occupant_is_not_overwritten() {
            let c = Case::new();
            let p = c.prepare(&["tree"]).unwrap();
            let mut created = false;
            let e = p
                .execute(&Control::default(), |bytes, _| {
                    if bytes > 0 && !created {
                        created = true;
                        fs::write(c.0.join("target/out.zip"), b"foreign").unwrap();
                    }
                })
                .unwrap_err();
            assert_eq!(e.publication, Publication::Unpublished);
            assert_eq!(fs::read(c.0.join("target/out.zip")).unwrap(), b"foreign");
            assert!(c.stages().is_empty());
        }
        #[test]
        fn replaced_stage_name_is_not_cleaned() {
            let c = Case::new();
            let p = c.prepare(&["tree"]).unwrap();
            let ctl = Control::default();
            let mut moved = false;
            let e = p
                .execute(&ctl, |bytes, _| {
                    if bytes > 0 && !moved {
                        moved = true;
                        let stage = c.stages().pop().unwrap();
                        fs::rename(&stage, c.0.join("owned-held")).unwrap();
                        fs::write(stage, b"foreign").unwrap();
                        ctl.cancel();
                    }
                })
                .unwrap_err();
            assert!(e.cleanup_error.is_some());
            assert_eq!(fs::read(c.stages().pop().unwrap()).unwrap(), b"foreign");
        }
        #[test]
        fn descendant_destination_and_special_or_ambiguous_names_rejected() {
            let c = Case::new();
            assert!(
                Prepared::capture(
                    &c.0.join("source"),
                    &["tree".into()],
                    &c.0.join("source/tree/empty"),
                    OsStr::new("out.zip"),
                    &Control::default()
                )
                .is_err()
            );
            fs::write(c.0.join("source").join("..\\escape"), b"no").unwrap();
            assert!(c.prepare(&["..\\escape"]).is_err());
            let pipe = c.0.join("source/pipe");
            assert_eq!(
                unsafe {
                    libc::mkfifo(
                        super::c(pipe.as_os_str().as_bytes()).unwrap().as_ptr(),
                        0o600,
                    )
                },
                0
            );
            assert!(c.prepare(&["pipe"]).is_err());
            assert!(c.stages().is_empty());
        }
        #[test]
        fn inventory_limits_reject_before_any_output() {
            let c = Case::new();
            for i in 0..4097 {
                fs::write(c.0.join(format!("source/tree/n{i}")), []).unwrap();
            }
            assert!(c.prepare(&["tree"]).is_err());
            assert!(fs::read_dir(c.0.join("target")).unwrap().next().is_none());
        }
        #[test]
        fn source_changes_during_read_never_publish() {
            let c = Case::new();
            fs::write(c.0.join("source/tree/a"), vec![3; 200000]).unwrap();
            let p = c.prepare(&["tree"]).unwrap();
            let mut changed = false;
            let e = p
                .execute(&Control::default(), |bytes, _| {
                    if bytes > 0 && !changed {
                        changed = true;
                        fs::write(c.0.join("source/tree/new-child"), b"new").unwrap();
                    }
                })
                .unwrap_err();
            assert_eq!(e.publication, Publication::Unpublished);
            assert!(c.stages().is_empty());
            assert_eq!(fs::read(c.0.join("source/tree/new-child")).unwrap(), b"new");
        }
    }
}
#[cfg(target_os = "linux")]
pub use linux::{Completed, Failure, Prepared, Publication};
