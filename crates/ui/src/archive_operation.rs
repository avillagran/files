//! ZIP execution adapter for the existing global operation worker. No thread.
use files_core::copy_engine::{Control, CopyMethod, CopyProgress, CopyReport, CopyRequest};
use std::{ffi::OsStr, io, path::Path};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryAction {
    Cleanup,
    Sync,
}
impl RecoveryAction {
    pub fn title(self) -> &'static str {
        match self {
            Self::Cleanup => "Limpiar temporal ZIP",
            Self::Sync => "Confirmar guardado ZIP",
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub enum Fixture {
    PauseAfterProgress,
    FailPublishNotice,
}
#[cfg(target_os = "linux")]
#[derive(Debug)]
enum Work {
    Extract(Box<files_core::archive_extract_fs::Prepared>),
    Create(Box<files_core::archive_create::Prepared>),
    Recover {
        failure: Failure,
        action: RecoveryAction,
    },
}
#[derive(Debug)]
pub struct Prepared {
    #[cfg(target_os = "linux")]
    token: Work,
    paths: Vec<CopyRequest>,
    reservation: usize,
    outcome_bytes: usize,
    parent: Option<std::path::PathBuf>,
    fixture: Option<Fixture>,
}
impl Prepared {
    pub fn capture(
        source: &Path,
        parent: &Path,
        name: &OsStr,
        control: &Control,
    ) -> io::Result<Self> {
        #[cfg(target_os = "linux")]
        {
            let token = files_core::archive_extract_fs::Prepared::capture(
                source,
                parent,
                name,
                Default::default(),
                control,
            )?;
            // Execution is conservatively charged in full until ACK. Admission
            // also proves that the retained failure fits the existing report slot.
            let outcome_bytes = token
                .outcome_reserve()
                .ok_or_else(|| io::Error::other("Reserva del resultado ZIP fuera de rango"))?;
            let reservation = token
                .retained_bytes()
                .and_then(|n| n.checked_add(token.execution_auxiliary_reserve()))
                .and_then(|n| n.checked_add(outcome_bytes))
                .ok_or_else(|| io::Error::other("Presupuesto ZIP fuera de rango"))?;
            let paths = vec![CopyRequest {
                source: token.source().into(),
                destination: parent.join(name),
            }];
            Ok(Self {
                token: Work::Extract(Box::new(token)),
                paths,
                reservation,
                outcome_bytes,
                parent: None,
                fixture: None,
            })
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (source, parent, name, control);
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Extracción ZIP pendiente en esta plataforma",
            ))
        }
    }
    pub fn capture_create(
        source: &Path,
        names: &[std::ffi::OsString],
        parent: &Path,
        name: &OsStr,
        control: &Control,
    ) -> io::Result<Self> {
        #[cfg(target_os = "linux")]
        {
            let token = files_core::archive_create::Prepared::capture(
                source, names, parent, name, control,
            )?;
            let reservation = token
                .retained_bytes()
                .checked_add(1024 * 1024 + 256 * 1024)
                .ok_or_else(|| io::Error::other("Reserva ZIP excedida"))?;
            let paths = vec![CopyRequest {
                source: source.into(),
                destination: token.destination(),
            }];
            Ok(Self {
                token: Work::Create(Box::new(token)),
                paths,
                reservation,
                outcome_bytes: 256 * 1024,
                parent: None,
                fixture: None,
            })
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (source, names, parent, name, control);
            Err(io::Error::other("Crear ZIP pendiente en este sistema"))
        }
    }
    pub fn creating(&self) -> bool {
        #[cfg(target_os = "linux")]
        {
            matches!(self.token, Work::Create(_))
        }
        #[cfg(not(target_os = "linux"))]
        {
            false
        }
    }
    pub fn summary(&self) -> String {
        #[cfg(target_os = "linux")]
        {
            match &self.token {
                Work::Extract(token) => format!(
                    "{} archivos · {} carpetas · {} declarados.\nAplicar crea la carpeta nueva.",
                    token.plan().files,
                    token.plan().directories,
                    files_core::format_size(token.plan().bytes)
                ),
                Work::Recover { action, .. } => action.title().into(),
                Work::Create(token) => format!(
                    "{} entradas · {} de origen\nDEFLATE · enlaces literales\nUTC/2s · sin ACL/xattrs",
                    token.entries(),
                    files_core::format_size(token.bytes())
                ),
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            "Extracción pendiente en esta plataforma".into()
        }
    }
    pub fn paths(&self) -> &Vec<CopyRequest> {
        &self.paths
    }
    pub fn outcome_bytes(&self) -> usize {
        self.outcome_bytes
    }
    pub fn metadata_bytes(&self) -> usize {
        self.reservation
    }
    pub fn fd_count(&self) -> usize {
        if self.creating() {
            76
        } else {
            3 + files_core::archive_extract_fs::EXECUTION_EXTRA_FDS
        }
    }
    pub fn recovery_action(&self) -> Option<RecoveryAction> {
        #[cfg(target_os = "linux")]
        {
            if let Work::Recover { action, .. } = &self.token {
                Some(*action)
            } else {
                None
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            None
        }
    }
    pub fn parent(&self) -> Option<&Path> {
        self.parent
            .as_deref()
            .or_else(|| self.paths.first()?.destination.parent())
    }
    pub fn enable_fixture(&mut self, fixture: Fixture) -> Result<(), &'static str> {
        if std::env::var_os("FILES_TEST_COPY").as_deref() != Some(OsStr::new("1"))
            || self.recovery_action().is_some()
        {
            return Err("private copy scene required");
        }
        self.fixture = Some(fixture);
        Ok(())
    }
    pub fn recovery(failure: Failure, action: RecoveryAction) -> Result<Self, Failure> {
        if !failure.allows(action) {
            return Err(failure);
        }
        #[cfg(target_os = "linux")]
        {
            let Some(bytes) = failure.core().retained_bytes() else {
                return Err(failure);
            };
            let Some(reservation) = bytes.checked_add(128 * 1024) else {
                return Err(failure);
            };
            let Some(outcome_bytes) = bytes.checked_add(64 * 1024) else {
                return Err(failure);
            };
            let parent = failure.core().observed_parent().map(Path::to_path_buf);
            Ok(Self {
                token: Work::Recover { failure, action },
                paths: Vec::new(),
                reservation,
                outcome_bytes,
                parent,
                fixture: None,
            })
        }
        #[cfg(not(target_os = "linux"))]
        {
            Err(failure)
        }
    }
    pub fn into_recovery_failure(self) -> Option<Failure> {
        #[cfg(target_os = "linux")]
        {
            if let Work::Recover { failure, .. } = self.token {
                Some(failure)
            } else {
                None
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            None
        }
    }
    pub fn execute(
        self,
        control: &Control,
        mut progress: impl FnMut(CopyProgress<'_>),
    ) -> Result<CopyReport, Failure> {
        #[cfg(target_os = "linux")]
        {
            match self.token {
                Work::Create(token) => {
                    let path = &self.paths[0];
                    let mut paused = false;
                    let done = token
                        .execute(control, |bytes, entries| {
                            if matches!(self.fixture, Some(Fixture::PauseAfterProgress))
                                && bytes > 0
                                && !paused
                            {
                                paused = true;
                                control.pause();
                                eprintln!(
                                    "FILES_ARCHIVE_FIXTURE_PAUSED after_positive_progress=true"
                                );
                            }
                            progress(CopyProgress {
                                source: &path.source,
                                destination: &path.destination,
                                bytes_copied: bytes,
                                entries_copied: entries,
                                method: CopyMethod::Stream,
                            })
                        })
                        .map_err(Failure::Create)?;
                    Ok(CopyReport {
                        completed_count: 1,
                        completed: vec![done.destination],
                        bytes_copied: done.bytes,
                        files_copied: done.files as u64,
                        directories_copied: done.directories as u64,
                        symlinks_copied: done.links as u64,
                        ..Default::default()
                    })
                }
                Work::Extract(token) => {
                    let path = &self.paths[0];
                    let mut paused = false;
                    let result = token.execute_observing_publish(control, |p| {
                        progress(CopyProgress {source:&path.source,destination:&path.destination,bytes_copied:p.bytes,entries_copied:p.verified_files as u64,method:CopyMethod::Stream});
                        if matches!(self.fixture,Some(Fixture::PauseAfterProgress)) && p.bytes>0 && !paused {
                            paused=true; control.pause(); eprintln!("FILES_ARCHIVE_FIXTURE_PAUSED after_positive_progress=true");
                        }
                    }, || if matches!(self.fixture,Some(Fixture::FailPublishNotice)) { Err(io::Error::other("Private publish-observer failure before parent sync (not a failed fsync syscall)")) } else { Ok(()) }).map_err(Failure::Core)?;
                    Ok(CopyReport {
                        completed_count: 1,
                        completed: vec![result.destination],
                        bytes_copied: result.bytes,
                        files_copied: result.files as u64,
                        directories_copied: result.directories as u64,
                        ..Default::default()
                    })
                }
                Work::Recover { failure, action } => {
                    let error = failure.into_core();
                    match action {
                        RecoveryAction::Cleanup => {
                            let parent = self.parent;
                            error
                                .retry_cleanup_controlled(control, |removed| {
                                    if let Some(path) = &parent {
                                        progress(CopyProgress {
                                            source: path,
                                            destination: path,
                                            bytes_copied: 0,
                                            entries_copied: removed as u64,
                                            method: CopyMethod::Directory,
                                        });
                                    }
                                })
                                .map_err(|error| Failure::Recovery {
                                    error,
                                    cancelled: control.is_cancelled(),
                                })?;
                            Ok(CopyReport {
                                affected_directories: parent.into_iter().collect(),
                                ..Default::default()
                            })
                        }
                        RecoveryAction::Sync => {
                            let done = error.retry_sync_controlled(control).map_err(|error| {
                                Failure::Recovery {
                                    error,
                                    cancelled: control.is_cancelled(),
                                }
                            })?;
                            Ok(CopyReport {
                                completed_count: 1,
                                completed: vec![done.destination],
                                ..Default::default()
                            })
                        }
                    }
                }
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (control, &mut progress);
            Err(Failure::Unavailable)
        }
    }
}

#[derive(Debug)]
pub enum Failure {
    #[cfg(target_os = "linux")]
    Core(Box<files_core::archive_extract_fs::Failure>),
    #[cfg(target_os = "linux")]
    Create(Box<files_core::archive_create::Failure>),
    #[cfg(target_os = "linux")]
    Recovery {
        error: Box<files_core::archive_extract_fs::Failure>,
        cancelled: bool,
    },
    #[cfg(not(target_os = "linux"))]
    Unavailable,
}
impl Failure {
    #[cfg(target_os = "linux")]
    fn core(&self) -> &files_core::archive_extract_fs::Failure {
        match self {
            Self::Core(e) | Self::Recovery { error: e, .. } => e,
            Self::Create(_) => unreachable!("creation is not extraction recovery"),
        }
    }
    #[cfg(target_os = "linux")]
    fn into_core(self) -> Box<files_core::archive_extract_fs::Failure> {
        match self {
            Self::Core(e) | Self::Recovery { error: e, .. } => e,
            Self::Create(_) => unreachable!("creation is not extraction recovery"),
        }
    }
    pub fn allows(&self, action: RecoveryAction) -> bool {
        #[cfg(target_os = "linux")]
        {
            if matches!(self, Self::Create(_)) {
                return false;
            }
            match action {
                RecoveryAction::Cleanup => self.core().can_retry_cleanup(),
                RecoveryAction::Sync => self.core().can_retry_sync(),
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = action;
            false
        }
    }

    pub fn cancelled(&self) -> bool {
        match self {
            #[cfg(target_os = "linux")]
            Self::Core(e) => e.cause.kind() == io::ErrorKind::Interrupted,
            #[cfg(target_os = "linux")]
            Self::Recovery { cancelled, .. } => *cancelled,
            #[cfg(target_os = "linux")]
            Self::Create(e) => e.cancelled,
            #[cfg(not(target_os = "linux"))]
            Self::Unavailable => false,
        }
    }
    pub fn uncertain(&self) -> bool {
        match self {
            #[cfg(target_os = "linux")]
            Self::Create(e) => e.publication == files_core::archive_create::Publication::Uncertain,
            #[cfg(target_os = "linux")]
            Self::Core(e) | Self::Recovery { error: e, .. } => {
                e.publication == files_core::archive_extract_fs::Publication::Uncertain
            }
            #[cfg(not(target_os = "linux"))]
            Self::Unavailable => false,
        }
    }
    pub fn recovery_needed(&self) -> bool {
        match self {
            #[cfg(target_os = "linux")]
            Self::Create(e) => {
                e.stage.is_some()
                    || e.cleanup_error.is_some()
                    || e.publication != files_core::archive_create::Publication::Unpublished
            }
            #[cfg(target_os = "linux")]
            Self::Core(e) | Self::Recovery { error: e, .. } => {
                e.can_retry_cleanup()
                    || e.can_retry_sync()
                    || matches!(e.cleanup, files_core::archive_extract_fs::Cleanup::Unknown)
                    || e.publication != files_core::archive_extract_fs::Publication::Unpublished
            }
            #[cfg(not(target_os = "linux"))]
            Self::Unavailable => false,
        }
    }
}
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            #[cfg(target_os = "linux")]
            Self::Create(e) => std::fmt::Display::fmt(e, f),
            #[cfg(target_os = "linux")]
            Self::Core(e) | Self::Recovery { error: e, .. } => {
                use files_core::archive_extract_fs::{Cleanup, Publication};
                let publication = match e.publication {
                    Publication::Unpublished => "sin publicar",
                    Publication::PublishedUnsynced => {
                        "publicada; guardado del directorio pendiente"
                    }
                    Publication::Uncertain => "sin confirmar; conserva las rutas indicadas",
                };
                let cleanup = match e.cleanup {
                    Cleanup::NotNeeded => "no se crearon temporales",
                    Cleanup::Complete => "eliminados y guardado confirmado",
                    Cleanup::CompleteUnsynced => "eliminados; guardado del directorio pendiente",
                    Cleanup::Retained => "conservados para recuperación",
                    Cleanup::Unknown => "estado sin confirmar; no borrar por inferencia",
                };
                writeln!(
                    f,
                    "{}\nSalida: {publication}\nTemporales: {cleanup}\nBytes procesados (no prueban publicación): {}",
                    e.cause, e.provisional_bytes
                )?;
                if let Some(path) = &e.rescue {
                    writeln!(f, "Rescate verificado (conservar): {path:?}")?;
                }
                if let Some(path) = &e.candidate {
                    writeln!(f, "Candidato sin prueba de propiedad (no borrar): {path:?}")?;
                }
                if let Some(error) = &e.cleanup_error {
                    writeln!(f, "Error de limpieza: {error}")?;
                }
                if e.can_retry_sync() || e.can_retry_cleanup() {
                    f.write_str("Conserva las rutas de rescate. No repitas la extracción antes de revisar el resultado.")
                } else {
                    Ok(())
                }
            }
            #[cfg(not(target_os = "linux"))]
            Self::Unavailable => f.write_str("Extracción ZIP no disponible"),
        }
    }
}
impl std::error::Error for Failure {}
