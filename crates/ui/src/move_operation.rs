//! Native/EXDEV move adapter for the ONE global copy worker. No I/O in UI.
//! The EXDEV branch is intentionally single regular file nlink1 until the tree
//! deletion/batch ledger exists. No CopyReport path authorizes source deletion.
use files_core::{
    copy_engine::{Control, CopyMethod, CopyProgress, CopyReport, CopyRequest},
    exdev_move::{PreparedFileMove, Recovery, Transfer},
    move_engine::PreparedMove as Native,
    move_journal::Phase,
};
use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
};
pub const EXDEV_FD_RESERVATION: usize = 24;
// Single-file retained manifest/state/path objects, xattr work <=128KiB,
// sequential64KiB hash/copy scratch and temporary metadata collections. This is
// conservative local admission accounting, NOT a process/PSS measurement.
pub const EXDEV_METADATA_RESERVATION: usize = 1024 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryAction {
    Finish,
    Restore,
}
#[derive(Clone, Debug)]
pub struct Context {
    pub job: [u8; 16],
    pub proof: Option<files_core::move_journal::IntentProof>,
    pub journals: PathBuf,
    pub source: PathBuf,
    pub destination: PathBuf,
    pub recovery: Option<RecoveryAction>,
    pub reviewed_phase: Option<Phase>,
}
impl Context {
    pub fn job_label(&self) -> String {
        self.job.iter().map(|b| format!("{b:02x}")).collect()
    }
}
#[derive(Debug)]
pub enum Prepared {
    Native(Native),
    Batch {
        token: files_core::multi_origin::PreparedMoveBatch,
        ledger: files_core::multi_origin::MoveLedger,
        paths: Vec<CopyRequest>,
    },
    Exdev {
        token: PreparedFileMove,
        paths: Vec<CopyRequest>,
        context: Context,
    },
    Recovery {
        paths: Vec<CopyRequest>,
        context: Context,
        previous: Option<Box<ExdevFailure>>,
    },
}
impl Prepared {
    /// The token already includes an admission estimate for its captured data.
    /// Queue accounting separately charges these display/refresh paths.
    pub fn batch(
        token: files_core::multi_origin::PreparedMoveBatch,
        available_bytes: usize,
    ) -> io::Result<Self> {
        let count = token.requests().count();
        let mut estimate = token
            .metadata_bytes()
            .checked_add(count * (std::mem::size_of::<CopyRequest>() * 2 + 256))
            .ok_or_else(|| io::Error::other("Presupuesto multipadre excedido"))?;
        for path in token
            .requests()
            .flat_map(|request| [&request.source, &request.destination])
        {
            estimate = estimate
                .checked_add(path.as_os_str().len().saturating_mul(4) + 128)
                .ok_or_else(|| io::Error::other("Presupuesto multipadre excedido"))?;
        }
        if estimate > available_bytes {
            return Err(io::Error::other(
                "Presupuesto multipadre insuficiente para revisión y cola",
            ));
        }
        let ledger = token.new_ledger();
        let mut paths = Vec::with_capacity(count);
        for index in 0..count {
            paths.push(
                token
                    .request_at_original(index)
                    .ok_or_else(|| io::Error::other("Raíz multipadre ausente"))?
                    .clone(),
            );
        }
        Ok(Self::Batch {
            token,
            ledger,
            paths,
        })
    }
    pub fn verify_selection(
        &self,
        source: &files_core::selection_source::Source,
        cancel: &AtomicBool,
    ) -> io::Result<()> {
        match self {
            Self::Native(n) => n.verify_selection(source, cancel),
            Self::Batch { .. } => Err(io::Error::other("El lote requiere sus firmas multipadre")),
            Self::Exdev { token, .. } => token.verify_selection(source, cancel),
            Self::Recovery { .. } => Err(io::Error::other(
                "Recuperación se autoriza por journal, no clipboard",
            )),
        }
    }
    pub fn verify_selection_controlled(
        &self,
        source: &files_core::selection_source::Source,
        control: &Control,
    ) -> io::Result<()> {
        match self {
            Self::Native(n) => n.verify_selection_controlled(source, control),
            Self::Batch { .. } => Err(io::Error::other("El lote requiere sus firmas multipadre")),
            Self::Exdev { token, .. } => token.verify_selection_controlled(source, control),
            Self::Recovery { .. } => Err(io::Error::other("Recuperación requiere journal")),
        }
    }
    pub fn paths(&self) -> &Vec<CopyRequest> {
        match self {
            Self::Native(n) => n.paths(),
            Self::Batch { paths, .. } => paths,
            Self::Exdev { paths, .. } | Self::Recovery { paths, .. } => paths,
        }
    }
    pub fn fd_count(&self) -> usize {
        match self {
            Self::Native(n) => n.fd_count(),
            Self::Batch { token, .. } => token.fd_count(),
            _ => EXDEV_FD_RESERVATION,
        }
    }
    pub fn metadata_bytes(&self) -> usize {
        match self {
            Self::Native(n) => n.metadata_bytes(),
            Self::Batch { token, .. } => token.metadata_bytes(),
            _ => EXDEV_METADATA_RESERVATION,
        }
    }
    pub fn context(&self) -> Option<&Context> {
        match self {
            Self::Native(_) | Self::Batch { .. } => None,
            Self::Exdev { context, .. } | Self::Recovery { context, .. } => Some(context),
        }
    }
    pub fn recovery(
        context: Context,
        action: RecoveryAction,
        previous: Option<Box<ExdevFailure>>,
    ) -> Self {
        let paths = vec![CopyRequest {
            source: context.source.clone(),
            destination: context.destination.clone(),
        }];
        Self::Recovery {
            paths,
            previous,
            context: Context {
                recovery: Some(action),
                ..context
            },
        }
    }
    pub fn take_previous(&mut self) -> Option<Box<ExdevFailure>> {
        if let Self::Recovery { previous, .. } = self {
            previous.take()
        } else {
            None
        }
    }
    pub fn execute(
        self,
        control: &Control,
        progress: impl FnMut(CopyProgress<'_>),
    ) -> Result<CopyReport, crate::copy_job::Failure> {
        use crate::copy_job::Failure;
        match self {
            Self::Native(native) => native
                .execute(control, progress)
                .map_err(|e| Failure::Engine(Box::new(e))),
            Self::Batch {
                token,
                mut ledger,
                paths,
            } => {
                let mut report = CopyReport {
                    completed: Vec::with_capacity(files_core::multi_origin::MAX_ROOTS),
                    affected_directories: Vec::with_capacity(
                        files_core::multi_origin::MAX_PARENTS * 2,
                    ),
                    auxiliary_accounted_peak_bytes: token.metadata_bytes(),
                    ..Default::default()
                };
                match token.execute_pending_reports(&mut ledger, control, progress, |part| {
                    merge_native_report(&mut report, part)
                }) {
                    Ok(()) => Ok(report),
                    Err(cause) => Err(Failure::Batch(Box::new(BatchFailure {
                        token,
                        ledger,
                        paths,
                        cause,
                        report,
                    }))),
                }
            }
            Self::Exdev { token, context, .. } => {
                execute_exdev(token, context, control, progress).map_err(Failure::Exdev)
            }
            Self::Recovery {
                context, previous, ..
            } => {
                // Transfer owner/lock is released on this SAME worker, after
                // queue admission and immediately before opening fresh FDs.
                drop(previous);
                execute_recovery(context, control).map_err(Failure::Exdev)
            }
        }
    }
}

/// Retains the original proof and exact per-root outcomes after an incomplete
/// batch. The queue must account for this owner until the UI discards or retries it.
// Native-only producer: at most128 completed roots plus64 observed parent paths;
// original engine errors remain intact. This is below the queue's2MiB report reserve.
fn merge_native_report(total: &mut CopyReport, part: &CopyReport) {
    total.completed_count += part.completed_count;
    total.completed_omitted += part.completed_omitted;
    total.files_copied += part.files_copied;
    total.directories_copied += part.directories_copied;
    total.symlinks_copied += part.symlinks_copied;
    total.bytes_copied += part.bytes_copied;
    total.details_truncated += part.details_truncated;
    total.auxiliary_accounted_peak_bytes = total
        .auxiliary_accounted_peak_bytes
        .max(part.auxiliary_accounted_peak_bytes);
    for p in &part.completed {
        if total.completed.len() < files_core::multi_origin::MAX_ROOTS {
            total.completed.push(p.clone());
        } else {
            total.completed_omitted += 1;
        }
    }
    for p in &part.affected_directories {
        if !total.affected_directories.contains(p) {
            if total.affected_directories.len() < files_core::multi_origin::MAX_PARENTS * 2 {
                total.affected_directories.push(p.clone());
            } else {
                total.details_truncated += 1;
            }
        }
    }
}
#[derive(Debug)]
pub struct BatchFailure {
    pub token: files_core::multi_origin::PreparedMoveBatch,
    pub ledger: files_core::multi_origin::MoveLedger,
    pub paths: Vec<CopyRequest>,
    pub cause: files_core::multi_origin::BatchFailure,
    pub report: CopyReport,
}
impl BatchFailure {
    /// Pure eligibility only; all original pending identities are revalidated on
    /// the operation worker. Never turn uncertain publication into a retry.
    pub fn retryable(&self) -> bool {
        use files_core::multi_origin::RootState;
        self.ledger.states().contains(&RootState::Pending)
            && !self.ledger.states().contains(&RootState::Uncertain)
            && match &self.cause {
                files_core::multi_origin::BatchFailure::Review(_) => true,
                files_core::multi_origin::BatchFailure::Engine { error, .. } => {
                    error.rescue_paths.is_empty()
                        && error.rescue_paths_omitted == 0
                        && error.cleanup_errors.is_empty()
                        && error.cleanup_errors_omitted == 0
                        && error.cleanup_unresolved_entries == 0
                        && error.details_truncated == 0
                }
            }
    }
}
pub fn prepare_captured(
    proof: Arc<files_core::multi_origin::Captured>,
    destination: &Path,
    available_fds: usize,
    available_metadata: usize,
    cancel: &AtomicBool,
) -> Result<Prepared, String> {
    proof.verify(cancel).map_err(|e| e.to_string())?;
    if proof.groups().len() == 1 {
        let group = &proof.groups()[0];
        let lease = crate::file_clipboard::Lease {
            system: false,
            generation: 0,
            kind: crate::file_clipboard::Kind::Cut,
            source: group.source.clone(),
            names: group.source.names().clone(),
        };
        let paths = crate::copy_request_builder::build(
            group.source.parent(),
            destination,
            lease.names.iter().map(|n| n.as_os_str()),
            &files_core::copy_engine::CopyOptions::default(),
        )
        .map_err(str::to_string)?;
        let prepared = prepare_checked(paths, Some(&lease), cancel)?;
        if prepared.fd_count() + proof.fd_reservation() > available_fds
            || prepared.metadata_bytes() + proof.retained_bytes() > available_metadata
        {
            return Err("Presupuesto insuficiente para revisión capturada".into());
        }
        Ok(prepared)
    } else {
        files_core::multi_origin::prepare_move_batch(
            proof,
            destination,
            available_fds,
            available_metadata,
            cancel,
        )
        .and_then(|token| Prepared::batch(token, available_metadata))
        .map_err(|e| e.to_string())
    }
}
pub fn prepare_checked(
    paths: Vec<CopyRequest>,
    clipboard: Option<&crate::file_clipboard::Lease>,
    cancel: &AtomicBool,
) -> Result<Prepared, String> {
    if let Some(c) = clipboard {
        c.source
            .verify_subset(c.names.iter().map(|n| n.as_os_str()), cancel)
            .map_err(|e| e.to_string())?;
    }
    let token = prepare(paths, cancel)?;
    if let Some(c) = clipboard {
        token
            .verify_selection(&c.source, cancel)
            .map_err(|e| e.to_string())?;
    }
    Ok(token)
}
pub fn prepare(paths: Vec<CopyRequest>, cancel: &AtomicBool) -> Result<Prepared, String> {
    if paths.is_empty() || paths.len() > files_core::move_engine::MAX_ROOTS {
        return Err("Lote fuera de límite de revisión".into());
    }
    let mut bytes = 0usize;
    for p in &paths {
        for path in [&p.source, &p.destination] {
            if !path.is_absolute() || path.as_os_str().len() > 4096 || path.capacity() > 8192 {
                return Err("Ruta fuera de límite de revisión".into());
            }
            bytes = bytes
                .checked_add(path.capacity())
                .filter(|n| *n <= 4 * 1024 * 1024)
                .ok_or("Rutas exceden presupuesto de revisión")?;
        }
    }
    if paths.len() != 1 {
        return Native::capture(paths,cancel).map(Prepared::Native).map_err(|e| if e.raw_os_error()==Some(libc::EXDEV){"EXDEV: este bloque admite un archivo regular sin hardlinks; árboles/lotes pendientes, origen conservado".into()}else{e.to_string()});
    }
    // Caller has preflighted every path <=4096B before allocation. One backup
    // pair <=8KiB lets native validation choose EXDEV without losing raw paths.
    match Native::capture(paths.clone(), cancel) {
        Ok(native) => Ok(Prepared::Native(native)),
        Err(e) if e.raw_os_error() == Some(libc::EXDEV) => {
            let job = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_nanos()
                .to_be_bytes();
            let journals = files_core::theme::config_dir().join("operations");
            if journals.as_os_str().len() > 4096 {
                return Err("Ruta de journals demasiado larga".into());
            }
            let token =
                PreparedFileMove::capture(&paths[0].source, &paths[0].destination, job, cancel)
                    .map_err(|e| e.to_string())?;
            let context = Context {
                job,
                proof: None,
                journals,
                source: paths[0].source.clone(),
                destination: paths[0].destination.clone(),
                recovery: None,
                reviewed_phase: None,
            };
            Ok(Prepared::Exdev {
                token,
                paths,
                context,
            })
        }
        Err(e) => Err(e.to_string()),
    }
}
#[derive(Debug)]
enum Retained {
    Transfer(Box<Transfer>),
    Recovery(Box<Recovery>),
}
#[derive(Debug)]
pub struct ExdevFailure {
    pub cause: io::Error,
    pub context: Context,
    pub phase: Option<Phase>,
    pub report: CopyReport,
    pub source_location: Option<PathBuf>,
    pub staging_candidates: Vec<PathBuf>,
    pub destination_location: Option<PathBuf>,
    pub recovery_needed: bool,
    retained: Option<Retained>,
}
impl ExdevFailure {
    pub fn retained_phase(&self) -> Option<Phase> {
        match &self.retained {
            Some(Retained::Transfer(t)) => t.phase(),
            Some(Retained::Recovery(r)) => Some(r.phase()),
            None => None,
        }
    }
}
impl std::fmt::Display for ExdevFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} · journal {} · fase {:?}",
            self.cause,
            self.context.job_label(),
            self.phase
        )
    }
}
impl std::error::Error for ExdevFailure {}
fn report(context: &Context) -> CopyReport {
    CopyReport {
        affected_directories: vec![
            context.source.parent().unwrap().to_path_buf(),
            context.destination.parent().unwrap().to_path_buf(),
        ],
        ..Default::default()
    }
}
fn failure(
    mut context: Context,
    cause: io::Error,
    retained: Option<Retained>,
) -> Box<ExdevFailure> {
    let mut staging_candidates = Vec::new();
    if let Some(Retained::Transfer(t)) = &retained {
        context.proof = t.intent_proof().ok().flatten().or(context.proof);
        staging_candidates = t.staging_candidates();
    } else if let Some(Retained::Recovery(r)) = &retained {
        context.proof = r.intent_proof().ok().or(context.proof);
        staging_candidates = r.unconfirmed_stages().to_vec();
    }

    let (phase, source_location, destination_location, recovery_needed) = match &retained {
        Some(Retained::Transfer(t)) => (
            t.phase(),
            t.source_location().ok(),
            t.destination_location().ok().flatten(),
            t.phase()
                .is_some_and(|p| !matches!(p, Phase::Completed | Phase::Restored)),
        ),
        Some(Retained::Recovery(r)) => (
            Some(r.phase()),
            None,
            None,
            !matches!(r.phase(), Phase::Completed | Phase::Restored),
        ),
        None => (None, None, None, context.recovery.is_some()),
    };
    context.reviewed_phase = phase.or(context.reviewed_phase);
    let mut report = report(&context);
    if let Some(Retained::Transfer(t)) = &retained
        && let Ok(actual) = t.affected_directories()
    {
        report.affected_directories = actual;
    }
    Box::new(ExdevFailure {
        report,
        context,
        cause,
        phase,
        source_location,
        destination_location,
        staging_candidates,
        recovery_needed,
        retained,
    })
}
fn private_journals(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt};
    match fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)
    {
        Ok(()) => (),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(e),
    }
    let m = fs::symlink_metadata(path)?;
    if !m.is_dir() || m.uid() != unsafe { libc::geteuid() } || m.mode() & 0o077 != 0 {
        return Err(io::Error::other(
            "Journals requieren directorio privado propio, sin symlink",
        ));
    }
    Ok(())
}
fn execute_exdev(
    token: PreparedFileMove,
    context: Context,
    control: &Control,
    mut progress: impl FnMut(CopyProgress<'_>),
) -> Result<CopyReport, Box<ExdevFailure>> {
    private_journals(&context.journals).map_err(|e| failure(context.clone(), e, None))?;
    let mut transfer = token.begin(&context.journals, control).map_err(|e| {
        failure(
            context.clone(),
            e.cause,
            Some(Retained::Transfer(e.transfer)),
        )
    })?;
    let mut bytes = 0;
    let result = transfer
        .copy_to_staging(control, |n| {
            bytes += n;
            progress(CopyProgress {
                source: &context.source,
                destination: &context.destination,
                bytes_copied: bytes,
                entries_copied: 0,
                method: CopyMethod::Stream,
            })
        })
        .and_then(|_| transfer.publish(control))
        .and_then(|_| transfer.finish(control));
    if let Err(cause) = result {
        // Runtime rollback retains its FDs. Never blind-clean a published copy,
        // uncertain journal, or an original whose unlink has already returned.
        let restore_error = if !transfer.source_unlinked() && !transfer.journal_uncertain() {
            transfer.restore().err()
        } else {
            None
        };
        let cause = if let Some(restore) = restore_error {
            io::Error::other(format!(
                "{cause}; restauración requiere revisión: {restore}"
            ))
        } else {
            cause
        };
        let mut failed = failure(context, cause, Some(Retained::Transfer(Box::new(transfer))));
        failed.report.bytes_copied = bytes;
        return Err(failed);
    }
    let mut report = report(&context);
    if let Ok(actual) = transfer.affected_directories() {
        report.affected_directories = actual;
    }
    report.completed.push(context.destination);
    report.completed_count = 1;
    report.files_copied = 1;
    report.stream_files = 1;
    report.bytes_copied = bytes;
    Ok(report)
}
fn execute_recovery(context: Context, control: &Control) -> Result<CopyReport, Box<ExdevFailure>> {
    let proof = context.proof.as_ref().ok_or_else(|| {
        failure(
            context.clone(),
            io::Error::other(
                "Sin prueba de intención; abre Movimientos interrumpidos para revisar de nuevo",
            ),
            None,
        )
    })?;
    let mut recovery = Recovery::open_reviewed(&context.journals, context.job, proof)
        .map_err(|e| failure(context.clone(), e, None))?;
    let result = match context.recovery.expect("recovery request action") {
        RecoveryAction::Finish => recovery.finish(control),
        RecoveryAction::Restore => recovery.restore(control),
    };
    if let Err(cause) = result {
        return Err(failure(
            context,
            cause,
            Some(Retained::Recovery(Box::new(recovery))),
        ));
    }
    let mut report = report(&context);
    if context.recovery == Some(RecoveryAction::Finish) {
        report.completed.push(context.destination);
        report.completed_count = 1;
        report.files_copied = 1;
        report.stream_files = 1;
    }
    Ok(report)
}

#[cfg(all(test, target_os = "linux"))]
mod tests;
