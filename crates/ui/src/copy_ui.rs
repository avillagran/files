//! Local copy destination, bounded queue controls and complete retained results.
use crate::{
    copy_dialog::Dialog,
    copy_job::Request,
    copy_state::{Completion, Controller, Phase},
    model::{Action, Hit, Model},
    painter::{Painter, Rect},
};
use files_core::copy_engine::CopyOptions;
use fltk::{
    app,
    enums::{Event, Key, Shortcut},
};
#[cfg(test)]
use std::path::PathBuf;
impl Model {
    pub fn pending_move_retry_count(&self) -> Option<usize> {
        if !self.notice_open
            || self.closing
            || self.copy.active_id().is_some()
            || self.copy_form.is_some()
            || self.move_preparing.is_some()
            || self.mutation_applied
        {
            return None;
        }
        self.copy_last.as_ref().and_then(|o| match &o.result {
            Err(crate::copy_job::Failure::Batch(batch))
                if self.copy_notice_id == Some(o.id) && batch.retryable() =>
            {
                Some(
                    batch
                        .ledger
                        .states()
                        .iter()
                        .filter(|s| **s == files_core::multi_origin::RootState::Pending)
                        .count(),
                )
            }
            _ => None,
        })
    }
    pub fn retry_pending_move(&mut self) {
        let Some(pending) = self.pending_move_retry_count() else {
            return;
        };
        self.copy.set_external_fds(
            self.clipboard_state.external_fds()
                + self.native_transfer.external_fds()
                + self.search_transfer_fds(),
        );
        let mut old = self.copy_last.take().expect("eligible retained batch");
        let previous_id = old.id;
        let result = std::mem::replace(&mut old.result, Err(crate::copy_job::Failure::WorkerLost));
        let Err(crate::copy_job::Failure::Batch(batch)) = result else {
            unreachable!("eligibility checked without yielding");
        };
        let crate::move_operation::BatchFailure {
            token,
            ledger,
            paths,
            cause,
            report,
        } = *batch;
        self.copy_id = self.copy_id.checked_add(1).expect("operation ID exhausted");
        let request = Request::reviewed_move(
            self.copy_id,
            crate::move_operation::Prepared::Batch {
                token,
                ledger,
                paths,
            },
        );
        match self.copy.requeue_held_batch(previous_id, request) {
            Ok(()) => {
                self.copy_owners.insert(self.copy_id, self.pane_owner());
                self.notice_open = false;
                self.copy.hold_queue(false);
                self.operation_details.clear();
                self.operation_message = Some(format!(
                    "Reintento #{} de #{previous_id}: {pending} pendientes; no repite confirmadas",
                    self.copy_id
                ));
                self.can_undo = false;
                self.can_redo = false;
            }
            Err(mut rejected) => {
                let Some(crate::move_operation::Prepared::Batch {
                    token,
                    ledger,
                    paths,
                }) = rejected.request.take_prepared()
                else {
                    unreachable!("original request returned");
                };
                old.result = Err(crate::copy_job::Failure::Batch(Box::new(
                    crate::move_operation::BatchFailure {
                        token,
                        ledger,
                        paths,
                        cause,
                        report,
                    },
                )));
                self.copy_last = Some(old);
                self.notice_open = true;
                self.copy.hold_queue(true);
                let message = format!(
                    "Reintento no encolado: {}; firmas y resultado anterior conservados",
                    rejected.reason
                );
                if let Some(first) = self
                    .operation_details
                    .first_mut()
                    .filter(|line| line.starts_with("Reintento no encolado:"))
                {
                    *first = message;
                } else {
                    self.operation_details.insert(0, message);
                }
            }
        }
    }
    pub fn recovery_context(&self) -> Option<&crate::move_operation::Context> {
        self.recovery_context.as_ref().or_else(|| {
            self.copy_last
                .as_ref()
                .and_then(|o| o.exdev_error())
                .filter(|e| e.recovery_needed)
                .map(|e| &e.context)
        })
    }
    pub fn recovery_actions(&self) -> (bool, bool) {
        use files_core::move_journal::Phase;
        let Some(context) = self.recovery_context().filter(|c| c.proof.is_some()) else {
            return (false, false);
        };
        match context.reviewed_phase {
            Some(
                Phase::Prepared | Phase::StageIntent | Phase::SourceStaged | Phase::RestoreIntent,
            ) => (false, true),
            Some(Phase::PublishIntent | Phase::DestinationPublished | Phase::DestinationSynced) => {
                (true, true)
            }
            Some(Phase::Deleting) => (true, false),
            _ => (false, false),
        }
    }
    pub fn request_move_recovery(&mut self, action: crate::move_operation::RecoveryAction) {
        if self.copy_busy() || self.move_preparing.is_some() || self.mutation_applied {
            self.operation_details
                .push("Espera la operación/revisión activa antes de recuperar".into());
            return;
        }
        let actions = self.recovery_actions();
        if (action == crate::move_operation::RecoveryAction::Finish && !actions.0)
            || (action == crate::move_operation::RecoveryAction::Restore && !actions.1)
        {
            return;
        }
        let Some(context) = self.recovery_context().cloned() else {
            return;
        };
        if context.proof.is_none() {
            self.operation_details.push(
                "Abre Movimientos interrumpidos para revisar la intención antes de actuar".into(),
            );
            return;
        }
        let mut old = self.copy_last.take();
        let previous = old.as_mut().and_then(|o| {
            let result =
                std::mem::replace(&mut o.result, Err(crate::copy_job::Failure::WorkerLost));
            match result {
                Err(crate::copy_job::Failure::Exdev(e)) => Some(e),
                other => {
                    o.result = other;
                    None
                }
            }
        });
        self.copy_id = self.copy_id.checked_add(1).expect("operation ID exhausted");
        let prepared = crate::move_operation::Prepared::recovery(context.clone(), action, previous);
        let request = Request::reviewed_move(self.copy_id, prepared);
        match self.copy.enqueue(request) {
            Ok(()) => {
                if let Some(tag) = self
                    .clipboard_recovery
                    .take()
                    .filter(|t| t.matches(&context))
                {
                    self.clipboard_jobs.insert(self.copy_id, tag.tag);
                }
                self.copy_owners.insert(self.copy_id, self.pane_owner());
                self.copy_locations.insert(
                    self.copy_id,
                    context.destination.parent().unwrap().to_path_buf(),
                );
                self.copy_sources
                    .insert(self.copy_id, context.source.parent().unwrap().to_path_buf());
                self.recovery_context = None;
                self.notice_open = false;
                self.recovery_catalog.close();
                self.copy.hold_queue(false);
                self.operation_message =
                    Some("Recuperación explícita encolada; verificando journal y archivos".into());
            }
            Err(mut rejected) => {
                if let Some(previous) = rejected.request.take_previous()
                    && let Some(old) = old.as_mut()
                {
                    old.result = Err(crate::copy_job::Failure::Exdev(previous));
                }
                self.copy_last = old;
                self.operation_details.push(format!(
                    "Recuperación no encolada: {}; owner/contexto conservados",
                    rejected.reason
                ));
                self.notice_open = true;
            }
        }
    }
    pub fn open_copy(&mut self) {
        self.open_copy_mode(false);
    }
    pub fn cancel_copy_dialog(&mut self) {
        if let Some((_, cancel)) = &self.archive_preparing {
            cancel.cancel();
        }
        if let Some((_, cancel)) = &self.move_preparing {
            cancel.store(true, std::sync::atomic::Ordering::Release);
        }
        self.copy_form = None;
    }
    pub fn open_copy_mode(&mut self, moving: bool) {
        self.copy_notice_id = None;
        if let Some(why) = crate::copy_review::pending_reason(self) {
            self.error = why.into();
            return;
        }
        if moving
            && self
                .selection
                .len()
                .max(usize::from(self.selected.is_some()))
                > files_core::move_engine::MAX_ROOTS
        {
            self.error = "Mover admite128 raíces; reduce selección".into();
            return;
        }
        let names = || {
            self.entries
                .iter()
                .enumerate()
                .filter(|(i, _)| {
                    self.selection.contains(i)
                        || self.selection.is_empty() && self.selected == Some(*i)
                })
                .map(|(i, e)| (i, e.name.as_os_str()))
        };
        if let Err(error) = crate::copy_request_builder::preflight(
            &self.path,
            &self.path,
            names().map(|(_, name)| name),
            &CopyOptions::default(),
        ) {
            self.operation_message = Some(error.into());
            self.operation_details.clear();
            self.notice_open = true;
            return;
        }
        let mut indices = names().map(|(i, _)| i).collect::<Vec<_>>();
        indices.sort_by_cached_key(|i| {
            files_core::natural_sort::NaturalKeyRef::new(&self.entries[*i].name)
        });
        let selected: std::sync::Arc<[std::ffi::OsString]> = indices
            .into_iter()
            .map(|i| self.entries[i].name.clone())
            .collect::<Vec<_>>()
            .into();
        self.preview.close();
        self.cancel_rename();
        self.settings = false;
        self.notice_open = false;
        let destination = self
            .opposite_destination()
            .unwrap_or(&self.path)
            .to_path_buf();
        let capture = self.dual.enabled
            || destination
                .to_str()
                .is_none_or(|s| s.chars().any(char::is_control));
        self.copy_form = Some(Dialog {
            editor: Default::default(),
            name_field: crate::copy_name_field::Field::new(selected.clone()),
            name_focus: false,
            clipboard: None,
            multi: None,
            moving,
            extracting: false,
            creating: false,
            archive: None,
            preparing: false,
            prepare_id: None,
            prepared: None,
            source: self.path.clone(),
            names: selected,
            destination: destination
                .to_str()
                .map(str::to_owned)
                .unwrap_or_else(|| format!("{destination:?}")),
            captured_destination: None,
            replace: true,
            start_paused: false,
            options_open: false,
            publication: files_core::copy_engine::Publication::StagedRename,
            preserve_symlink_times: true,
            existing_destination: files_core::copy_engine::ExistingDestination::Stop,
            error: String::new(),
        });
        if capture {
            self.copy_form
                .as_mut()
                .expect("new dialog")
                .capture_destination(destination);
        }
        self.hits.clear();
    }
    pub fn start_copy(&mut self) {
        let Some(mut form) = self.copy_form.take() else {
            return;
        };
        if form.extracting {
            self.start_extract(form);
            return;
        }
        if form.preparing || form.options_open {
            form.error = "Espera revisión o cancela".into();
            self.copy_form = Some(form);
            return;
        }
        if !form.moving
            && let Err(error) = form.name_field.commit()
        {
            form.error = error.into();
            self.copy_form = Some(form);
            return;
        }
        self.copy.set_external_fds(
            self.clipboard_state.external_fds()
                + self.native_transfer.external_fds()
                + self.search_transfer_fds(),
        );
        let destination = form.destination_path();
        let options = CopyOptions {
            publication: if form.moving {
                files_core::copy_engine::Publication::StagedRename
            } else {
                form.publication
            },
            preserve_symlink_times: form.moving || form.preserve_symlink_times,
            existing_destination: if !form.moving {
                form.existing_destination
            } else {
                files_core::copy_engine::ExistingDestination::Stop
            },
            ..CopyOptions::default()
        };
        let request = if form.moving && form.prepared.is_some() {
            Request::reviewed_move(
                form.prepare_id.expect("reviewed move id"),
                form.prepared.take().unwrap(),
            )
        } else {
            let built = if let Some(proof) = &form.multi {
                crate::search_transfer::build(
                    proof,
                    &destination,
                    &form.names,
                    &form.name_field.plan,
                    &options,
                )
            } else if form.moving {
                crate::copy_request_builder::build(
                    &form.source,
                    &destination,
                    form.names.iter().map(|n| n.as_os_str()),
                    &options,
                )
            } else {
                crate::copy_request_builder::build_named(
                    &form.source,
                    &destination,
                    &form.names,
                    &form.name_field.plan,
                    &options,
                )
            };
            let paths = match built {
                Ok(p) => p,
                Err(e) => {
                    form.error = e.into();
                    self.copy_form = Some(form);
                    return;
                }
            };
            self.copy_id = self.copy_id.checked_add(1).expect("operation ID exhausted");
            if form.moving {
                if self.move_preparing.is_some() {
                    form.error = "Una revisión sigue pendiente".into();
                    self.copy_form = Some(form);
                    return;
                }
                if paths.len() > files_core::move_engine::MAX_ROOTS {
                    form.error = "Mover admite128 raíces; reduce selección".into();
                    self.copy_form = Some(form);
                    return;
                }
                let review_fds = form.multi.as_ref().map_or_else(
                    || (paths.len() + 2).max(crate::move_operation::EXDEV_FD_RESERVATION),
                    |p| {
                        p.fd_reservation()
                            + (paths.len() + p.groups().len() * 2)
                                .max(crate::move_operation::EXDEV_FD_RESERVATION)
                    },
                );
                if self.copy.fd_reservation().saturating_add(review_fds) > 256 {
                    form.error = "Espera trabajos pendientes: revisión y cola superan256FD".into();
                    self.copy_form = Some(form);
                    return;
                }
                let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                let request = if let Some(proof) = &form.multi {
                    crate::aux_io::Request::PrepareMultiMove(
                        self.copy_id,
                        proof.clone(),
                        destination.clone(),
                        256usize.saturating_sub(self.copy.fd_reservation()),
                        self.copy.budget().available_bytes,
                        cancel.clone(),
                    )
                } else {
                    crate::aux_io::Request::PrepareMove(
                        self.copy_id,
                        paths,
                        cancel.clone(),
                        form.clipboard.clone(),
                    )
                };
                match self.aux.tx.try_send(request) {
                    Ok(()) => {
                        self.move_preparing = Some((self.copy_id, cancel));
                        self.move_review_fds = review_fds;
                        form.prepare_id = Some(self.copy_id);
                        form.preparing = true;
                        form.touch();
                        form.error.clear();
                    }
                    Err(_) => form.error = "Trabajador ocupado; reintenta la revisión".into(),
                }
                self.copy_form = Some(form);
                return;
            }
            Request::new(self.copy_id, paths, options)
        };
        let request = request
            .with_clipboard(form.clipboard.clone())
            .with_multi(if form.moving {
                None
            } else {
                form.multi.clone()
            });
        let id = request.id;
        let request = if form.start_paused {
            request.start_paused()
        } else {
            request
        };
        match self.copy.enqueue(request) {
            Ok(()) => {
                if let Some(lease) = &form.clipboard {
                    self.clipboard_jobs.insert(
                        id,
                        crate::file_clipboard::JobTag {
                            lease: lease.clone(),
                            destination: destination.clone(),
                        },
                    );
                }
                self.copy_owners.insert(id, self.pane_owner());
                self.copy_locations.insert(id, destination);
                if form.moving {
                    self.copy_sources.insert(id, form.source.clone());
                }
                self.operation_message = Some(format!(
                    "{} #{id} encolado",
                    if form.moving { "Movimiento" } else { "Copia" }
                ));
                self.can_undo = false;
                self.can_redo = false;
            }
            Err(mut rejected) => {
                if let Some(prepared) = rejected.request.take_prepared() {
                    form.prepared = Some(prepared);
                }
                let budget = self.copy.budget();
                form.error = format!(
                    "{} ({} bytes de presupuesto de cola disponibles)",
                    format_args!(
                        "{} ({} raíces no admitidas)",
                        rejected.reason,
                        rejected.request.paths().len()
                    ),
                    budget.available_bytes
                );
                self.copy_form = Some(form);
            }
        }
    }
    pub fn copy_busy(&self) -> bool {
        self.copy.active_id().is_some() || self.copy.queued_count() != 0
    }
    pub fn pump_copy(&mut self) -> bool {
        if self.closing {
            self.copy.begin_close();
        } else if self.copy.is_closed() {
            self.copy = Controller::new();
        }
        self.copy.hold_queue(self.notice_open);
        if !self.closing
            && !self.notice_open
            && self.copy.phase() == Phase::Queued
            && self.copy.active_id().is_none()
        {
            // Release the old external outcome BEFORE a worker can build another.
            let old_id = self.copy_last.as_ref().map(|outcome| outcome.id);
            self.copy_last = None;
            self.copy_notice_id = None;
            if let Some(id) = old_id {
                self.copy.release_held_batch(id);
            }
            self.copy_report_truncated = false;
            self.operation_details.clear();
        }
        let state = self.copy.pump();
        let mut dirty = state.progress.is_some()
            || state.dispatched.is_some()
            || state.completion.is_some()
            || state.closed;
        if let Some(progress) = state.progress {
            if std::env::var_os("FILES_TEST_COPY").is_some() {
                eprintln!(
                    "FILES_COPY_PROGRESS id={} bytes={} entries={} method={:?} stats={:?}",
                    progress.id,
                    progress.bytes_copied,
                    progress.entries_copied,
                    progress.method,
                    progress.stats
                );
            }
            self.copy_path = progress.source;
            self.copy_destination = progress.destination;
        }
        if let Some(completion) = state.completion {
            let id = completion.id();
            let unknown = completion.outcome().is_some_and(|o| {
                o.supervisor_state_unknown()
                    || matches!(&o.result, Err(crate::copy_job::Failure::Archive(e)) if e.uncertain())
                    || matches!(&o.result, Err(crate::copy_job::Failure::Batch(batch)) if batch.ledger.states().contains(&files_core::multi_origin::RootState::Uncertain))
                    || o.moving
                        && o.engine_error()
                            .is_some_and(|e| e.cleanup_unresolved_entries > 0)
            });
            if std::env::var_os("FILES_TEST_COPY").is_some() {
                eprintln!(
                    "FILES_COPY_FINISHED id={id} cancelled_pending={} progress_stats={:?} cancelled={} report_bytes={} report_files={} completed={}",
                    state.cancelled_pending_total,
                    completion.outcome().map(|o| o.progress),
                    completion.outcome().is_some_and(|o| o.cancelled()),
                    completion
                        .outcome()
                        .and_then(|o| o.report())
                        .map_or(0, |r| r.bytes_copied),
                    completion
                        .outcome()
                        .and_then(|o| o.report())
                        .map_or(0, |r| r.files_copied),
                    completion
                        .outcome()
                        .and_then(|o| o.report())
                        .map_or(0, |r| r.completed_count)
                );
            }
            match completion {
                Completion::Finished(outcome) => {
                    self.archive_confirmation = None;
                    self.copy_notice_id = Some(outcome.id);
                    self.invalidate_sizes();
                    if let Some(tag) = self.clipboard_jobs.remove(&outcome.id) {
                        self.clipboard_state.acknowledge(&tag, &outcome);
                        self.clipboard_recovery = outcome
                            .exdev_error()
                            .filter(|e| e.recovery_needed)
                            .and_then(|e| {
                                e.context.proof.clone().map(|proof| {
                                    crate::file_clipboard::RecoveryTag {
                                        job: e.context.job,
                                        proof,
                                        tag,
                                    }
                                })
                            });
                    } else {
                        self.clipboard_recovery = None;
                    }
                    self.recovery_context = None;
                    let destination = self.copy_locations.remove(&outcome.id);
                    let source = self.copy_sources.remove(&outcome.id);
                    self.dual.refresh_other(
                        source.iter().chain(destination.iter()).cloned().chain(
                            outcome
                                .report()
                                .into_iter()
                                .flat_map(|r| r.affected_directories.iter().cloned()),
                        ),
                    );
                    self.copy_owners.remove(&outcome.id);
                    let refresh = source.as_ref().is_some_and(|p| self.path.starts_with(p))
                        || outcome.report().is_some_and(|r| {
                            r.affected_directories
                                .iter()
                                .any(|p| self.path.starts_with(p))
                        })
                        || destination
                            .as_ref()
                            .is_some_and(|d| self.path.starts_with(d))
                        || outcome.report().is_some_and(|r| {
                            r.completed.iter().any(|p| {
                                p.parent() == Some(self.path.as_path())
                                    || p.parent() == self.layout_path.as_deref()
                            })
                        });
                    if refresh && !self.closing {
                        self.refresh_current();
                    }
                    let report = crate::copy_report::describe(&outcome);
                    let error = outcome.result.is_err();
                    let cleanup = outcome.report().is_some_and(|r| {
                        !r.cleanup_paths.is_empty()
                            || !r.cleanup_errors.is_empty()
                            || r.cleanup_paths_omitted > 0
                            || r.cleanup_errors_omitted > 0
                    });
                    let operation = if let Some(action) = outcome.archive_action {
                        match action {
                            crate::archive_operation::RecoveryAction::Cleanup => "Limpieza ZIP",
                            crate::archive_operation::RecoveryAction::Sync => "Guardado ZIP",
                        }
                    } else if outcome.creating {
                        "Creación ZIP"
                    } else if outcome.extracting {
                        "Extracción ZIP"
                    } else if outcome.moving {
                        "Movimiento"
                    } else {
                        "Copia"
                    };
                    self.operation_message = Some(if unknown {
                        format!("{operation} #{id}: estado de archivos sin confirmar")
                    } else if error {
                        format!("{operation} #{id}: revisa el resultado")
                    } else if let Some(r) = outcome.report().filter(|r| r.renamed_count > 0) {
                        format!(
                            "{} publicadas · {} con otro nombre · {operation} #{id}",
                            r.completed_count, r.renamed_count
                        )
                    } else if let Some(r) = outcome.report().filter(|r| r.skipped_count > 0) {
                        format!(
                            "{} publicadas · {} omitidas · {operation} #{id}",
                            r.completed_count, r.skipped_count
                        )
                    } else {
                        format!("{operation} #{id} terminado")
                    });
                    self.operation_details = report.text.lines().map(str::to_owned).collect();
                    if state.cancelled_pending_total > 0 {
                        self.operation_details.push(format!(
                            "Trabajos pendientes cancelados explícitamente: {}",
                            state.cancelled_pending_total
                        ));
                    }
                    self.copy_report_truncated = report.truncated;
                    self.notice_scroll = 0;
                    self.notice_pan = 0;
                    let recovery = matches!(&outcome.result, Err(crate::copy_job::Failure::Archive(e)) if e.recovery_needed())
                        || outcome.exdev_error().is_some_and(|e| e.recovery_needed)
                        || outcome.engine_error().is_some_and(|e| {
                            !e.rescue_paths.is_empty()
                                || !e.cleanup_errors.is_empty()
                                || e.cleanup_unresolved_entries > 0
                                || e.rescue_paths_omitted > 0
                                || e.cleanup_errors_omitted > 0
                                || e.details_truncated > 0
                        });
                    let clean_close_cancel = self.closing
                        && outcome.cancelled()
                        && !recovery
                        && !cleanup
                        && outcome.report().is_none_or(|r| r.completed_count == 0);
                    self.notice_open =
                        (error && !clean_close_cancel) || cleanup || recovery || report.truncated;
                    self.copy_last = Some(outcome); // Exactly one full result, never an unbounded history.
                    if self.notice_open {
                        self.copy.hold_queue(true);
                        self.closing = false;
                    }
                }
                Completion::Rejected(rejected) => {
                    self.copy_notice_id = None;
                    self.operation_message =
                        Some(format!("Copia #{id} rechazada: {}", rejected.reason));
                    self.operation_details = vec![format!(
                        "{} raíces no aceptadas por el trabajador. No se ejecutarán automáticamente; vuelve a seleccionar y encolar.",
                        rejected.request.paths().len()
                    )];
                    self.notice_open = true;
                    self.copy.hold_queue(true);
                    self.closing = false;
                }
            }
        }
        if !self.closing
            && !self.notice_open
            && self.copy.phase() == Phase::WorkerUnavailable
            && self.copy.queued_count() == 0
        {
            self.copy.begin_close();
            dirty = true;
        }
        let copy = &self.copy;
        self.clipboard_jobs
            .retain(|id, _| copy.active_id() == Some(*id) || copy.queued_ids().any(|q| q == *id));
        self.copy_locations
            .retain(|id, _| copy.active_id() == Some(*id) || copy.queued_ids().any(|q| q == *id));
        self.copy_owners
            .retain(|id, _| copy.active_id() == Some(*id) || copy.queued_ids().any(|q| q == *id));
        self.copy_sources
            .retain(|id, _| copy.active_id() == Some(*id) || copy.queued_ids().any(|q| q == *id));
        dirty
    }
    pub fn cancel_copy_pending(&mut self) {
        let ids = self.copy.queued_ids().collect::<Vec<_>>();
        for id in ids {
            self.copy.cancel_pending(id);
            self.copy_locations.remove(&id);
            self.copy_sources.remove(&id);
        }
        self.operation_message = Some(format!(
            "Pendientes cancelados en esta cola: {}",
            self.copy.cancelled_pending_count()
        ));
    }
    pub fn show_copy_details(&mut self) {
        if let Some(outcome) = &self.copy_last {
            self.copy_notice_id = Some(outcome.id);
            let report = crate::copy_report::describe(outcome);
            self.operation_message = Some(format!(
                "Resultado de {} #{}",
                if outcome.moving {
                    "movimiento"
                } else {
                    "copia"
                },
                outcome.id
            ));
            self.operation_details = report.text.lines().map(str::to_owned).collect();
            self.copy_report_truncated = report.truncated;
            self.notice_scroll = 0;
            self.notice_open = true;
        }
    }
    pub fn notice_copy_outcome(&self) -> Option<&crate::copy_job::Outcome> {
        self.copy_last
            .as_deref()
            .filter(|outcome| self.notice_open && self.copy_notice_id == Some(outcome.id))
    }
    pub fn copy_full_report(&mut self) {
        if let Some(outcome) = self.notice_copy_outcome() {
            let report = crate::copy_report::full_text(outcome);
            if report.truncated {
                self.operation_message = Some(
                    "El informe completo excede8MiB; no se copió una versión incompleta".into(),
                );
                self.notice_open = true;
            } else {
                app::copy(&report.text);
            }
        }
    }
}
fn button(p: &mut dyn Painter, m: &mut Model, r: Rect, label: &str, action: Action) {
    p.rounded_rect(r, 5 * m.radius, m.palette.selection);
    p.text(
        Rect::new(r.x + 8, r.y, r.w - 16, r.h),
        label,
        11,
        m.palette.accent,
        true,
        false,
    );
    m.hits.push(Hit {
        generation: m.generation,
        rect: r,
        action,
    });
}
pub struct ProgressLayout {
    pub rect: Rect,
    pub narrow: bool,
    pub button_y: i32,
    pub clear_y: i32,
    pub path_y: i32,
}
pub fn progress_layout(m: &Model, w: i32, h: i32) -> ProgressLayout {
    let x = if m.rail { 186 } else { 68 };
    let inspector = !m.dual.enabled && m.inspector && w >= 800;
    let width = (w - x - 24 - if inspector { 240 } else { 0 }).max(0);
    let narrow = width < 340;
    let height = if narrow { 138 } else { 88 };
    let y = crate::operation_bar::browser_height(m, h) - height - 44;
    ProgressLayout {
        rect: Rect::new(x, y, width, height),
        narrow,
        button_y: y + if narrow { 70 } else { 54 },
        clear_y: y + if narrow { 104 } else { 54 },
        path_y: y + if narrow { 46 } else { 30 },
    }
}
pub fn paint(p: &mut dyn Painter, m: &mut Model, w: i32, h: i32) {
    let phase = m.copy.phase();
    if m.copy_busy() || m.copy.queue_held() && m.copy.queued_count() > 0 {
        let layout = progress_layout(m, w, h);
        let Rect { x, y, w: width, .. } = layout.rect;
        let c = m.palette.clone();
        p.rounded_rect(layout.rect, 8 * m.radius, c.darker_background);
        let progress = m.copy.progress().unwrap_or_default();
        if std::env::var_os("FILES_TEST_COPY").is_some() {
            eprintln!(
                "FILES_COPY_VIEW id={} method={:?} progress_stats={:?}",
                progress.id, progress.method, progress.stats
            );
        }
        let label = match phase {
            _ if m.copy.archive_action().is_some()
                && !matches!(phase, Phase::Paused | Phase::Cancelling | Phase::Held) =>
            {
                match m.copy.archive_action().unwrap() {
                    crate::archive_operation::RecoveryAction::Cleanup => "Limpiando temporal ZIP",
                    crate::archive_operation::RecoveryAction::Sync => "Confirmando guardado ZIP",
                }
            }
            _ if m.copy.creating()
                && !matches!(phase, Phase::Paused | Phase::Cancelling | Phase::Held) =>
            {
                "Creando ZIP"
            }
            _ if m.copy.extracting()
                && !matches!(phase, Phase::Paused | Phase::Cancelling | Phase::Held) =>
            {
                "Extrayendo ZIP"
            }
            Phase::Paused => "En pausa",
            Phase::Cancelling => "Cancelando · esperando resultado",
            Phase::Held => "Cola detenida: revisa el resultado",
            _ if m
                .copy
                .active_id()
                .is_some_and(|id| m.copy_sources.contains_key(&id)) =>
            {
                "Moviendo (rename nativo)"
            }
            _ => "Copiando",
        };
        let short = match label {
            "Cancelando · esperando resultado" => "Cancelando",
            "Cola detenida: revisa el resultado" => "Cola detenida",
            "Moviendo (rename nativo)" => "Moviendo",
            "Confirmando guardado ZIP" => "Confirmando ZIP",
            other => other,
        };
        let summary = if layout.narrow {
            format!("{short} · {} en cola", m.copy.queued_count())
        } else {
            format!(
                "{label} · {} · {} entradas · {} pendientes",
                files_core::format_size(progress.bytes_copied),
                progress.entries_copied,
                m.copy.queued_count()
            )
        };
        p.text(
            Rect::new(
                x + 12,
                y + 4,
                width - 24,
                if layout.narrow { 22 } else { 26 },
            ),
            &summary,
            11,
            c.bright_foreground,
            true,
            false,
        );
        if layout.narrow {
            p.text(
                Rect::new(x + 12, y + 26, width - 24, 20),
                &format!(
                    "{} · {} entradas",
                    files_core::format_size(progress.bytes_copied),
                    progress.entries_copied
                ),
                10,
                c.dark_foreground,
                false,
                false,
            );
        }
        if let Some(path) = &m.copy_path {
            p.literal_text(
                Rect::new(x + 12, layout.path_y, width - 24, 20),
                &files_core::name_label::label(path.as_os_str()),
                10,
                c.dark_foreground,
            );
        }
        if let Some(id) = m.copy.active_id() {
            button(
                p,
                m,
                Rect::new(x + 12, layout.button_y, 80, 27),
                if phase == Phase::Paused {
                    "Reanudar"
                } else {
                    "Pausa"
                },
                Action::CopyPause(id, phase != Phase::Paused),
            );
            let available = (width - if layout.narrow { 112 } else { 258 }).max(0);
            let full_width = p.text_width("Cancelar trabajo", 11, true).ceil() as i32 + 16;
            let label = if full_width <= available {
                "Cancelar trabajo"
            } else {
                "Cancelar"
            };
            let cancel_width = (p.text_width(label, 11, true).ceil() as i32 + 16).min(available);
            button(
                p,
                m,
                Rect::new(x + 100, layout.button_y, cancel_width, 27),
                label,
                Action::CopyCancel(id),
            );
        }
        button(
            p,
            m,
            Rect::new(
                if layout.narrow {
                    x + 12
                } else {
                    x + width - 150
                },
                layout.clear_y,
                if layout.narrow { width - 24 } else { 138 },
                27,
            ),
            "Vaciar pendientes",
            Action::CopyClearPending,
        );
    } else if m.copy_last.is_some() && !m.notice_open && m.rename_dialog.is_none() {
        button(
            p,
            m,
            Rect::new(
                if m.rail { 186 } else { 68 },
                crate::operation_bar::browser_height(m, h) - 72,
                180,
                28,
            ),
            "Resultado de operación",
            Action::CopyDetails,
        );
    }
    if let Some(mut form) = m.copy_form.take() {
        crate::copy_dialog::paint(p, m, &mut form, w, h);
        m.copy_form = Some(form);
    }
}
pub fn destination_pointer(
    m: &mut Model,
    x: i32,
    y: i32,
    down: bool,
    drag: bool,
    shift: bool,
    double: bool,
) -> bool {
    let Some(d) = m
        .copy_form
        .as_mut()
        .filter(|d| !d.preparing && !d.options_open)
    else {
        return false;
    };
    if !down && !drag {
        d.editor.dragging = false;
        d.name_field.editor.dragging = false;
        return false;
    }
    if !d.moving
        && !d.extracting
        && (drag && d.name_focus && d.name_field.editor.dragging
            || down && d.name_field.editor.geometry.contains(x, y))
    {
        let n = &mut d.name_field;
        if let Some(at) = n.editor.geometry.position(x, n.editor.epoch) {
            if down && double {
                n.editor.cursor.component(&n.draft, at);
            } else {
                let anchor = if drag || shift {
                    n.editor.cursor.anchor
                } else {
                    at
                };
                n.editor.cursor.select(&n.draft, anchor, at);
            }
            n.editor.dragging = true;
            n.editor.touch();
            d.editor.dragging = false;
            d.name_focus = true;
            d.editor.touch();
        }
        return true;
    }
    d.ensure_editor();
    if !down && !drag {
        d.editor.dragging = false;
        return false;
    }
    if !(drag && !d.name_focus && d.editor.dragging || down && d.editor.geometry.contains(x, y)) {
        return false;
    }
    if let Some(at) = d.editor.geometry.position(x, d.editor.epoch) {
        if down && double {
            d.editor.cursor.component(&d.destination, at);
        } else {
            let anchor = if drag || shift {
                d.editor.cursor.anchor
            } else {
                at
            };
            d.editor.cursor.select(&d.destination, anchor, at);
        }
        d.replace = false;
        d.editor.dragging = true;
        d.name_focus = false;
        d.name_field.editor.dragging = false;
        d.name_field.editor.touch();
        d.touch();
    }
    true
}
pub fn destination_key(m: &mut Model, key: Key, mods: Shortcut, text: &str) {
    use crate::line_edit::Motion;
    let ctrl = mods.intersects(Shortcut::Ctrl | Shortcut::Meta);
    let shift = mods.intersects(Shortcut::Shift);
    if key == Key::F3 {
        m.action(Action::CopyOptions);
        return;
    }
    if m.copy_form.as_ref().is_some_and(|d| d.options_open) {
        if matches!(key, Key::Escape | Key::Enter | Key::KPEnter) {
            m.action(Action::CopyOptions);
        } else if key == Key::from_char('1') {
            m.action(Action::CopyVisiblePublication);
        } else if key == Key::from_char('2') {
            m.action(Action::CopySymlinkTimes);
        }
        return;
    }
    if key == Key::Escape {
        m.cancel_copy_dialog();
        return;
    }
    if key == Key::F4 {
        m.action(Action::CopySkipExisting);
        return;
    }
    if matches!(key, Key::Enter | Key::KPEnter) {
        m.start_copy();
        return;
    }
    if key == Key::F2 {
        if let Some(d) = m.copy_form.as_ref().filter(|d| !d.moving && !d.extracting) {
            let e = d.name_field.editor.epoch;
            m.action(Action::CopyNameFocus(e));
        }
        return;
    }
    if mods.intersects(Shortcut::Alt) && matches!(key, Key::Up | Key::Down) {
        if let Some(d) = m.copy_form.as_ref().filter(|d| !d.moving && !d.extracting) {
            let e = d.name_field.editor.epoch;
            m.action(Action::CopyNameStep(e, if key == Key::Up { -1 } else { 1 }));
        }
        return;
    }
    if ctrl && key == Key::from_char('l') {
        m.action(Action::CopyDestination);
        return;
    }
    let Some(d) = m
        .copy_form
        .as_mut()
        .filter(|d| !d.preparing && !d.options_open)
    else {
        return;
    };
    if d.name_focus && !d.moving && !d.extracting {
        let n = &mut d.name_field;
        let result = if ctrl && key == Key::from_char('a') {
            n.select_all();
            Ok(())
        } else if ctrl && (key == Key::from_char('c') || key == Key::from_char('x')) {
            let t = n.selected_text().to_owned();
            if !t.is_empty() {
                app::copy(&t);
                if key == Key::from_char('x') {
                    n.delete(false, false)
                } else {
                    Ok(())
                }
            } else {
                Ok(())
            }
        } else if ctrl && (key == Key::from_char('z') || key == Key::from_char('y')) {
            n.undo(shift || key == Key::from_char('y'));
            Ok(())
        } else if matches!(key, Key::Left | Key::Right | Key::Home | Key::End) {
            n.motion(
                match key {
                    Key::Left if ctrl => Motion::WordLeft,
                    Key::Right if ctrl => Motion::WordRight,
                    Key::Left => Motion::Left,
                    Key::Right => Motion::Right,
                    Key::Home => Motion::Start,
                    _ => Motion::End,
                },
                shift,
            );
            Ok(())
        } else if matches!(key, Key::Delete | Key::BackSpace) {
            n.delete(key == Key::BackSpace, ctrl)
        } else if !ctrl && !text.is_empty() {
            n.input(text)
        } else {
            Ok(())
        };
        match result {
            Ok(()) => d.error.clear(),
            Err(e) => d.error = e.into(),
        };
        return;
    }
    d.ensure_editor();
    if ctrl && key == Key::from_char('a') {
        d.select_all();
    } else if ctrl && matches!(key, k if k==Key::from_char('c')||k==Key::from_char('x')) {
        let selected = d.selected_text().to_owned();
        if !selected.is_empty() {
            app::copy(&selected);
            if key == Key::from_char('x') {
                d.delete(false, false);
            }
        }
    } else if ctrl && (key == Key::from_char('z') || key == Key::from_char('y')) {
        d.undo(shift || key == Key::from_char('y'));
    } else if matches!(key, Key::Left | Key::Right | Key::Home | Key::End) {
        d.motion(
            match key {
                Key::Left if ctrl => Motion::WordLeft,
                Key::Right if ctrl => Motion::WordRight,
                Key::Left => Motion::Left,
                Key::Right => Motion::Right,
                Key::Home => Motion::Start,
                _ => Motion::End,
            },
            shift,
        );
    } else if matches!(key, Key::Delete | Key::BackSpace) {
        d.delete(key == Key::BackSpace, ctrl);
    } else if !ctrl && !text.is_empty() {
        d.input(text);
    }
}
pub fn event(m: &mut Model, ev: Event) -> bool {
    match ev {
        Event::Push if app::event_button() == 1 => {
            if !destination_pointer(
                m,
                app::event_x(),
                app::event_y(),
                true,
                false,
                app::event_state().intersects(Shortcut::Shift),
                app::event_clicks(),
            ) && let Some(action) = m
                .hits
                .iter()
                .rev()
                .find(|h| {
                    h.generation == m.generation && h.rect.contains(app::event_x(), app::event_y())
                })
                .map(|h| h.action.clone())
            {
                m.action(action);
            }
        }
        Event::Drag => {
            destination_pointer(m, app::event_x(), app::event_y(), false, true, false, false);
        }
        Event::Released => {
            destination_pointer(m, 0, 0, false, false, false, false);
        }
        Event::KeyDown | Event::Shortcut => {
            destination_key(m, app::event_key(), app::event_state(), &app::event_text())
        }
        // Paste is admitted exclusively by the receipt gate in the outer handler.
        _ => {}
    }
    true
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use files_core::listing::{Entry, Kind};
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
        time::{Duration, Instant},
    };
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let dir = files_core::theme::home()
                .join(".cache/files-test")
                .join(format!(
                    "copy-ui-unit-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    fn entry(name: &str) -> Entry {
        Entry {
            name: name.into(),
            kind: Kind::File,
            metadata: None,
            stat_attempted: false,
        }
    }
    fn drain(m: &mut Model) {
        let until = Instant::now() + Duration::from_secs(5);
        while m.copy_busy() {
            m.pump();
            assert!(Instant::now() < until, "copy timed out");
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    #[test]
    fn reviewed_visible_policy_survives_paused_queue_and_reports_explicit_omission() {
        use files_core::copy_engine::Publication;
        use std::os::unix::fs::symlink;
        let dir = Fixture::new();
        let source = dir.0.join("source");
        let target = dir.0.join("target");
        fs::create_dir(&source).unwrap();
        fs::create_dir(&target).unwrap();
        let external = dir.0.join("outside");
        fs::write(&external, b"never change").unwrap();
        fs::File::open(&external)
            .unwrap()
            .set_times(
                fs::FileTimes::new()
                    .set_modified(std::time::UNIX_EPOCH + Duration::from_secs(1000)),
            )
            .unwrap();
        let before = fs::metadata(&external).unwrap().modified().unwrap();
        fs::write(source.join("data"), b"payload").unwrap();
        symlink(&external, source.join("link")).unwrap();
        let mut m = Model::new(source.clone());
        m.loaded = true;
        m.entries = vec![
            entry("data"),
            Entry {
                kind: Kind::Symlink,
                ..entry("link")
            },
        ];
        m.rebuild();
        m.select_entry(0, false, false);
        m.select_entry(1, true, false);
        m.open_copy();
        m.copy_form
            .as_mut()
            .unwrap()
            .capture_destination(target.clone());
        m.action(Action::CopyOptions);
        m.action(Action::CopyVisiblePublication);
        m.action(Action::CopySymlinkTimes);
        m.action(Action::CopyOptions);
        m.action(Action::CopyStartPaused);
        m.start_copy();
        assert!(m.copy_form.is_none());
        let until = Instant::now() + Duration::from_secs(5);
        while m.copy.phase() != Phase::Paused {
            m.pump();
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(fs::read_dir(&target).unwrap().count(), 0);
        m.copy.resume(m.copy.active_id().unwrap());
        drain(&mut m);
        let outcome = m.copy_last.as_ref().unwrap();
        let report = outcome.result.as_ref().unwrap();
        assert_eq!(report.publication, Publication::VisibleExclusive);
        assert_eq!(report.visible_completed_count, 2);
        assert_eq!(report.symlink_times_omitted, 1);
        assert_eq!(fs::read(target.join("data")).unwrap(), b"payload");
        assert_eq!(fs::read_link(target.join("link")).unwrap(), external);
        assert_eq!(fs::metadata(&external).unwrap().modified().unwrap(), before);
        assert_eq!(fs::read(source.join("data")).unwrap(), b"payload");
        let details = crate::copy_report::describe(outcome).text;
        assert!(details.contains("parciales se conservan") && details.contains("omitidos: 1"));
    }
    #[test]
    fn visible_copy_never_falls_back_or_silently_omits_link_times() {
        use files_core::copy_engine::Publication;
        let dir = Fixture::new();
        let target = dir.0.join("target");
        fs::create_dir(&target).unwrap();
        fs::write(dir.0.join("data"), b"original").unwrap();
        std::os::unix::fs::symlink("data", dir.0.join("link")).unwrap();
        let mut m = Model::new(dir.0.clone());
        m.loaded = true;
        m.entries = vec![Entry {
            kind: Kind::Symlink,
            ..entry("link")
        }];
        m.rebuild();
        m.select_entry(0, false, false);
        m.open_copy();
        m.copy_form
            .as_mut()
            .unwrap()
            .capture_destination(target.clone());
        m.action(Action::CopyOptions);
        m.action(Action::CopyVisiblePublication);
        m.action(Action::CopyOptions);
        m.start_copy();
        drain(&mut m);
        let outcome = m.copy_last.as_ref().unwrap();
        assert!(outcome.result.is_err());
        let report = outcome.report().unwrap();
        assert_eq!(report.publication, Publication::VisibleExclusive);
        assert_eq!(report.symlink_times_omitted, 0);
        assert_eq!(fs::read_dir(&target).unwrap().count(), 0);
        assert_eq!(fs::read(dir.0.join("data")).unwrap(), b"original");
    }
    #[test]
    fn consecutive_refreshes_keep_lossless_multi_selection_focus_filter_and_scroll() {
        let dir = Fixture::new();
        for name in ["first.txt", "second.txt", "other.txt"] {
            fs::write(dir.0.join(name), name).unwrap();
        }
        let mut m = Model::new(dir.0.clone());
        m.view = crate::model::View::List;
        m.entries = vec![entry("first.txt"), entry("second.txt"), entry("other.txt")];
        m.rebuild();
        m.select_entry(0, false, false);
        m.select_entry(1, true, false);
        m.query = "txt".into();
        m.scroll = 123;
        m.pan_x = 17.0;
        m.refresh_current();
        assert!(
            m.refresh_selection
                .as_ref()
                .unwrap()
                .contains(std::ffi::OsStr::new("first.txt"))
        );
        // Another completion before a single reload batch can be consumed.
        m.refresh_current();
        assert!(
            m.refresh_selection
                .as_ref()
                .unwrap()
                .contains(std::ffi::OsStr::new("second.txt"))
        );
        let until = Instant::now() + Duration::from_secs(5);
        while !m.loaded || m.refresh_selection.is_some() {
            m.pump();
            assert!(Instant::now() < until, "listing error: {}", m.error);
            std::thread::sleep(Duration::from_millis(1));
        }
        let mut selected = m
            .selection
            .iter()
            .map(|i| m.entries[*i].name.clone())
            .collect::<Vec<_>>();
        selected.sort();
        assert_eq!(
            selected,
            vec![
                std::ffi::OsString::from("first.txt"),
                std::ffi::OsString::from("second.txt")
            ]
        );
        assert_eq!(m.entries[m.selected.unwrap()].name, "second.txt");
        assert_eq!(m.query, "txt");
        assert_eq!(m.scroll, 123);
        assert_eq!(m.pan_x, 17.0);
    }
    #[test]
    fn partial_reload_blocks_selection_until_snapshot_restored() {
        let dir = Fixture::new();
        fs::write(dir.0.join("keep.txt"), b"keep").unwrap();
        let mut m = Model::new(dir.0.clone());
        m.entries = vec![entry("keep.txt")];
        m.rebuild();
        m.select_entry(0, false, false);
        m.refresh_current();
        // A visible partial batch must not accept indices into a changing list.
        m.entries.push(entry("partial.txt"));
        m.rebuild();
        m.selection.clear();
        m.selected = None;
        m.select_entry(0, false, false);
        m.action(Action::Selection(crate::selection_tools::Command::All));
        m.action(Action::Selection(crate::selection_tools::Command::Invert));
        assert!(m.selection_refreshing());
        assert!(m.selection.is_empty());
        assert!(m.selected.is_none());
        assert!(
            m.refresh_selection
                .as_ref()
                .unwrap()
                .contains(std::ffi::OsStr::new("keep.txt"))
        );
        // Restart discards the synthetic partial batch; retained snapshot survives.
        m.refresh_current();
        let until = Instant::now() + Duration::from_secs(5);
        while !m.loaded || m.refresh_selection.is_some() {
            m.pump();
            assert!(Instant::now() < until, "reload failed: {}", m.error);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(m.entries[m.selected.unwrap()].name, "keep.txt");
        assert_eq!(m.selection.len(), 1);
    }
    #[test]
    fn refresh_budget_rejection_preserves_state_and_can_retry_after_reducing_selection() {
        let dir = Fixture::new();
        fs::write(dir.0.join("keep.txt"), b"keep").unwrap();
        let mut m = Model::new(dir.0.clone());
        // Aggregate exceeds8MiB. Synthetic in-memory names exercise admission;
        // no such overlong components are created on the filesystem.
        m.entries = (0..2048).map(|_| entry(&"x".repeat(4096))).collect();
        m.selection.extend(0..m.entries.len());
        m.selected = Some(0);
        m.scroll = 91;
        let generation = m.generation;
        m.refresh_current();
        assert!(m.refresh_deferred);
        assert!(m.error.contains("8MiB"));
        assert_eq!(m.generation, generation);
        assert_eq!(m.selection.len(), 2048);
        assert_eq!(m.entries.len(), 2048);
        assert_eq!(m.scroll, 91);
        m.entries = vec![entry("keep.txt")];
        m.selection.clear();
        m.selection.insert(0);
        m.selected = Some(0);
        m.refresh_current();
        assert!(!m.refresh_deferred);
        let until = Instant::now() + Duration::from_secs(5);
        while !m.loaded || m.refresh_selection.is_some() {
            m.pump();
            assert!(Instant::now() < until, "retry failed: {}", m.error);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(m.error.is_empty());
        assert_eq!(m.entries[m.selected.unwrap()].name, "keep.txt");
        assert_eq!(m.selection.len(), 1);
        assert_eq!(m.scroll, 91);
    }
    #[test]
    fn destination_reload_and_outcome_release_before_next_dispatch() {
        let fixture = Fixture::new();
        let source = fixture.0.join("source");
        let target = fixture.0.join("target");
        fs::create_dir(&source).unwrap();
        fs::create_dir(&target).unwrap();
        fs::write(source.join("a.txt"), b"bytes").unwrap();
        let mut m = Model::new(source.clone());
        m.entries = vec![entry("a.txt")];
        m.rebuild();
        m.select_entry(0, false, false);
        m.open_copy();
        m.copy_form.as_mut().unwrap().destination = target.to_str().unwrap().into();
        m.start_copy();
        // User browses to target while the background job is queued/running.
        m.navigate(target.clone());
        drain(&mut m);
        let until = Instant::now() + Duration::from_secs(5);
        while !m.loaded || !m.entries.iter().any(|e| e.name == "a.txt") {
            m.pump();
            assert!(Instant::now() < until, "listing error: {}", m.error);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(fs::read(target.join("a.txt")).unwrap(), b"bytes");
        assert!(m.copy_last.is_some());
        let target2 = fixture.0.join("next");
        fs::create_dir(&target2).unwrap();
        let request = Request::new(
            2,
            vec![files_core::copy_engine::CopyRequest {
                source: source.join("a.txt"),
                destination: target2.join("a.txt"),
            }],
            CopyOptions::default(),
        )
        .start_paused();
        assert!(m.copy.enqueue(request).is_ok());
        m.pump_copy();
        assert!(
            m.copy_last.is_none(),
            "old report released before next worker builds another"
        );
        let id = m.copy.active_id().unwrap();
        m.copy.cancel(id);
        drain(&mut m);
    }
    #[test]
    fn paused_copy_close_drains_without_touching_source_or_destination() {
        let dir = Fixture::new();
        let source = dir.0.join("source");
        let target = dir.0.join("target");
        fs::create_dir(&source).unwrap();
        fs::create_dir(&target).unwrap();
        fs::write(source.join("a.txt"), b"keep").unwrap();
        let mut m = Model::new(source.clone());
        m.entries = vec![entry("a.txt")];
        m.rebuild();
        m.select_entry(0, false, false);
        m.open_copy();
        let form = m.copy_form.as_mut().unwrap();
        form.destination = target.to_str().unwrap().into();
        form.start_paused = true;
        m.start_copy();
        m.pump_copy();
        m.closing = true;
        m.discard_close = true;
        assert!(!m.close_ready());
        let until = Instant::now() + Duration::from_secs(5);
        while !m.copy.is_closed() {
            m.pump();
            assert!(Instant::now() < until, "listing error: {}", m.error);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(!m.notice_open);
        assert!(m.close_ready());
        assert_eq!(fs::read(source.join("a.txt")).unwrap(), b"keep");
        assert_eq!(fs::read_dir(target).unwrap().count(), 0);
    }
    #[test]
    fn closing_signals_copy_and_preview_while_review_ack_is_pending() {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        };
        let mut m = Model::new("/owned-move-close-fixture-not-created".into());
        let flag = Arc::new(AtomicBool::new(false));
        m.move_preparing = Some((999, flag.clone()));
        m.copy
            .enqueue(
                Request::new(
                    1,
                    vec![files_core::copy_engine::CopyRequest {
                        source: "/owned-no-source".into(),
                        destination: "/owned-no-destination".into(),
                    }],
                    CopyOptions::default(),
                )
                .start_paused(),
            )
            .unwrap();
        m.pump_copy();
        m.preview.open("/owned-no-preview".into());
        m.closing = true;
        assert!(!m.close_ready());
        assert!(flag.load(Ordering::Acquire));
        assert!(!m.preview.visible());
        assert!(matches!(m.copy.phase(), Phase::Closing | Phase::Closed));
        m.move_preparing = None;
        let until = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while !m.close_ready() && std::time::Instant::now() < until {
            m.pump();
            std::thread::yield_now();
        }
        assert!(m.close_ready());
    }
    #[test]
    fn reviewed_move_refreshes_both_parents_without_touching_other_tab_state() {
        for visible in ["A", "B", "C"] {
            let f = Fixture::new();
            for dir in ["A", "B", "C"] {
                fs::create_dir(f.0.join(dir)).unwrap();
                fs::write(f.0.join(dir).join("untouched.txt"), dir).unwrap();
            }
            fs::write(f.0.join("A/move.txt"), b"move").unwrap();
            let source = f.0.join("A");
            let target = f.0.join("B");
            let mut m = Model::new(f.0.join(visible));
            let until = Instant::now() + Duration::from_secs(5);
            while !m.loaded && Instant::now() < until {
                m.pump();
                std::thread::sleep(Duration::from_millis(2));
            }
            assert!(m.loaded);
            m.query = "untouched".into();
            m.rebuild();
            let focused = m.selected;
            let generation = m.generation;
            let prepared = files_core::move_engine::PreparedMove::capture(
                vec![files_core::copy_engine::CopyRequest {
                    source: source.join("move.txt"),
                    destination: target.join("move.txt"),
                }],
                &std::sync::atomic::AtomicBool::new(false),
            )
            .unwrap();
            m.copy.enqueue(Request::native_move(1, prepared)).unwrap();
            m.copy_locations.insert(1, target.clone());
            m.copy_sources.insert(1, source.clone());
            while m.copy_last.is_none() && Instant::now() < until {
                m.pump();
                std::thread::sleep(Duration::from_millis(2));
            }
            assert!(m.copy_last.as_ref().unwrap().result.is_ok());
            assert!(!source.join("move.txt").exists());
            assert_eq!(fs::read(target.join("move.txt")).unwrap(), b"move");
            if visible == "C" {
                assert_eq!(m.generation, generation);
                assert_eq!(m.selected, focused);
            } else {
                assert!(m.generation > generation);
            }
            while !m.loaded && Instant::now() < until {
                m.pump();
                std::thread::sleep(Duration::from_millis(2));
            }
            assert_eq!(m.query, "untouched");
            assert!(
                m.selected
                    .is_some_and(|i| m.entries[i].name == "untouched.txt")
            );
            assert!(m.copy_sources.is_empty() && m.copy_locations.is_empty());
        }
    }
}
