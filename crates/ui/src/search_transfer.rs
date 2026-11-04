//! Search observations -> worker-captured multi-origin proof -> ordinary destination review.
use crate::{copy_dialog::Dialog, model::Model};
use files_core::{
    copy_engine::{CopyOptions, CopyRequest},
    multi_origin::Captured,
};
use std::{
    ffi::OsString,
    path::PathBuf,
    sync::mpsc::TrySendError,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

pub struct Input {
    pub id: u64,
    pub paths: Arc<[PathBuf]>,
    pub cancel: Arc<AtomicBool>,
}
pub struct Pending {
    input: Input,
    owner: (u64, u64),
    generation: u64,
    edit: u64,
    root: PathBuf,
    relative: Vec<PathBuf>,
    moving: bool,
    sent: bool,
    fds: usize,
    proof_reserve: usize,
}
#[derive(Default)]
pub struct State {
    pub pending: Option<Pending>,
}
impl State {
    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }
    pub fn id(&self) -> Option<u64> {
        self.pending.as_ref().map(|p| p.input.id)
    }
    pub fn fds(&self) -> usize {
        self.pending.as_ref().map_or(0, |p| p.fds)
    }
}
fn current(m: &Model, p: &Pending) -> bool {
    if m.closing
        || !m.search.visible
        || m.pane_owner() != p.owner
        || m.generation != p.generation
        || m.search.root != p.root
        || m.search.input_revision() != p.edit
    {
        return false;
    }
    if m.search.hits().is_empty() {
        return false;
    }
    if m.search.marks.explicit {
        m.search
            .marks
            .paths()
            .eq(p.relative.iter().map(PathBuf::as_path))
    } else {
        p.relative.len() == 1
            && m.search
                .hits()
                .get(m.search.selected)
                .is_some_and(|h| h.relative == p.relative[0])
    }
}
pub fn names(proof: &Captured) -> Result<Arc<[OsString]>, &'static str> {
    let mut rows = proof
        .groups()
        .iter()
        .flat_map(|g| g.indices.iter().copied().zip(g.source.names().iter()))
        .collect::<Vec<_>>();
    rows.sort_unstable_by_key(|(i, _)| *i);
    if rows.is_empty() || rows.len() > 128 || rows.iter().enumerate().any(|(i, (j, _))| i != *j) {
        return Err("Captura multipadre inválida");
    }
    Ok(rows
        .into_iter()
        .map(|(_, n)| n.clone())
        .collect::<Vec<_>>()
        .into())
}
pub fn build(
    proof: &Captured,
    destination: &std::path::Path,
    names: &Arc<[OsString]>,
    plan: &crate::copy_names::Plan,
    options: &CopyOptions,
) -> Result<Vec<CopyRequest>, &'static str> {
    if !plan.same_selection(names) {
        return Err("El plan pertenece a otra selección");
    }
    let mut roots = proof
        .groups()
        .iter()
        .flat_map(|g| {
            g.indices
                .iter()
                .copied()
                .zip(g.source.names().iter())
                .map(move |(i, n)| (i, g.source.parent(), n.as_os_str()))
        })
        .collect::<Vec<_>>();
    roots.sort_unstable_by_key(|r| r.0);
    if roots.len() != names.len()
        || roots
            .iter()
            .enumerate()
            .any(|(i, r)| r.0 != i || r.2 != names[i])
    {
        return Err("Nombres distintos de la captura multipadre");
    }
    if roots.len() > options.max_requests.min(4096) {
        return Err("Selección supera el límite conjunto de copia");
    }
    let mut path_bytes = 0usize;
    for (i, parent, name) in &roots {
        let target = plan.target(*i).ok_or("Falta nombre destino")?;
        path_bytes = path_bytes.saturating_add(
            parent.as_os_str().len()
                + name.len()
                + destination.as_os_str().len()
                + target.len()
                + 2,
        );
    }
    if path_bytes > options.max_request_path_bytes.min(4 * 1024 * 1024) {
        return Err("Rutas superan el presupuesto conjunto de copia");
    }
    for (i, parent, name) in &roots {
        crate::copy_request_builder::preflight_pairs(
            parent,
            destination,
            std::iter::once((*name, plan.target(*i).ok_or("Falta nombre destino")?)),
            options,
        )?;
    }
    let mut targets = (0..names.len())
        .map(|i| plan.target(i).ok_or("Falta nombre destino"))
        .collect::<Result<Vec<_>, _>>()?;
    targets.sort_unstable();
    if targets.windows(2).any(|p| p[0] == p[1]) {
        return Err("Dos elementos tienen el mismo nombre de destino");
    }
    Ok(roots
        .into_iter()
        .map(|(i, p, n)| CopyRequest {
            source: p.join(n),
            destination: destination.join(plan.target(i).unwrap()),
        })
        .collect())
}
impl Model {
    pub fn search_transfer_fds(&self) -> usize {
        self.archive_preparing.as_ref().map_or(0, |_| 8)
            + self
                .copy_form
                .as_ref()
                .and_then(|d| d.archive.as_ref())
                .map_or(0, |a| a.fd_count())
            + self.search_transfer.fds()
            + if self.move_preparing.is_some() {
                self.move_review_fds
            } else {
                0
            }
            + self
                .copy_form
                .as_ref()
                .and_then(|f| {
                    f.multi.as_ref().map(|p| {
                        if f.preparing {
                            0
                        } else if let Some(prepared) = &f.prepared {
                            prepared.fd_count()
                                + if matches!(
                                    prepared,
                                    crate::move_operation::Prepared::Batch { .. }
                                ) {
                                    0
                                } else {
                                    p.fd_reservation()
                                }
                        } else {
                            p.fd_reservation()
                        }
                    })
                })
                .unwrap_or(0)
    }
    pub fn begin_search_transfer(&mut self, moving: bool) {
        if self.closing
            || self.search_transfer.busy()
            || self.copy_form.is_some()
            || self.move_preparing.is_some()
            || !self.search.visible
        {
            return;
        }
        self.search.sync_order();
        let mut relative = self
            .search
            .order
            .visible(self.search.hits())
            .iter()
            .copied()
            .filter(|&i| {
                if self.search.marks.explicit {
                    self.search.marks.contains(&self.search.hits()[i].relative)
                } else {
                    i == self.search.selected
                }
            })
            .take(129)
            .map(|i| self.search.hits()[i].relative.clone())
            .collect::<Vec<_>>();
        if relative.is_empty() || relative.len() > 128 {
            self.search.error = "Selecciona1..128 resultados; vacío no usa el foco".into();
            return;
        }
        let paths: Arc<[PathBuf]> = relative
            .iter()
            .map(|p| self.search.root.join(p))
            .collect::<Vec<_>>()
            .into();
        let (proof_reserve, fds) = match files_core::multi_origin::capture_reservation(&paths) {
            Ok(v) => v,
            Err(e) => {
                self.search.error = e.to_string();
                return;
            }
        };
        relative.sort_unstable();
        let reserve = proof_reserve
            + paths
                .iter()
                .map(|p| p.capacity() + std::mem::size_of::<PathBuf>())
                .sum::<usize>()
            + relative
                .iter()
                .map(|p| p.capacity() + std::mem::size_of::<PathBuf>())
                .sum::<usize>()
            + 65536;
        self.copy.set_external_fds(
            self.clipboard_state.external_fds() + self.native_transfer.external_fds(),
        );
        if self.copy.fd_reservation().saturating_add(fds) > 256
            || self.copy.budget().available_bytes < reserve
        {
            self.search.error =
                "Espera la cola o reduce marcas: presupuesto de captura multipadre".into();
            return;
        }
        self.copy_id = self.copy_id.checked_add(1).expect("operation ID exhausted");
        self.search_transfer.pending = Some(Pending {
            input: Input {
                id: self.copy_id,
                paths,
                cancel: Arc::new(AtomicBool::new(false)),
            },
            owner: self.pane_owner(),
            generation: self.generation,
            edit: self.search.input_revision(),
            root: self.search.root.clone(),
            relative,
            moving,
            sent: false,
            fds,
            proof_reserve,
        });
        self.search.error.clear();
        self.hits.clear();
    }
    pub fn pump_search_transfer(&mut self) -> bool {
        let Some(mut p) = self.search_transfer.pending.take() else {
            return false;
        };
        if !current(self, &p) {
            p.input.cancel.store(true, Ordering::Release);
        }
        if !p.sent {
            if p.input.cancel.load(Ordering::Acquire) {
                return true;
            }
            let input = Input {
                id: p.input.id,
                paths: p.input.paths.clone(),
                cancel: p.input.cancel.clone(),
            };
            match self
                .aux
                .tx
                .try_send(crate::aux_io::Request::SearchCapture(input))
            {
                Ok(()) => p.sent = true,
                Err(TrySendError::Full(_)) => {}
                Err(TrySendError::Disconnected(_)) => {
                    self.search.error = "Trabajador de captura no disponible".into();
                    return true;
                }
            }
        }
        self.search_transfer.pending = Some(p);
        false
    }
    pub fn move_review_worker_lost(&mut self) -> bool {
        let Some((id, cancel)) = self.move_preparing.take() else {
            return false;
        };
        cancel.store(true, Ordering::Release);
        self.move_review_fds = 0;
        if let Some(form) = self.copy_form.as_mut().filter(|f| f.prepare_id == Some(id)) {
            form.preparing = false;
            form.prepare_id = None;
            form.error = "Trabajador terminó; revisión no confirmada".into();
            form.touch();
        }
        if std::env::var_os("FILES_CLOSE_TRACE").is_some() {
            eprintln!("FILES_MOVE_REVIEW_LOST id={id}");
        }
        true
    }
    pub fn search_capture_worker_lost(&mut self) -> bool {
        let Some(p) = self.search_transfer.pending.take() else {
            return false;
        };
        p.input.cancel.store(true, Ordering::Release);
        if self.search.visible && !self.closing {
            self.search.error =
                "El trabajador terminó; captura sin confirmar, no se abrió revisión".into();
        }
        if std::env::var_os("FILES_CLOSE_TRACE").is_some() {
            eprintln!("FILES_SEARCH_CAPTURE_LOST id={}", p.input.id);
        }
        true
    }
    pub fn receive_search_capture(&mut self, id: u64, result: Result<Captured, String>) {
        let Some(p) = self.search_transfer.pending.take() else {
            return;
        };
        if p.input.id != id {
            self.search_transfer.pending = Some(p);
            return;
        }
        if p.input.cancel.load(Ordering::Acquire) || !current(self, &p) {
            return;
        }
        let proof = match result {
            Ok(c) if c.retained_bytes() <= p.proof_reserve => Arc::new(c),
            Ok(_) => {
                self.search.error = "Captura supera su reserva; reduce marcas".into();
                return;
            }
            Err(e) => {
                self.search.error = e;
                return;
            }
        };
        let names = match names(&proof) {
            Ok(n) => n,
            Err(e) => {
                self.search.error = e.into();
                return;
            }
        };
        let destination = self
            .opposite_destination()
            .unwrap_or(&self.path)
            .to_path_buf();
        let mut form = Dialog {
            editor: Default::default(),
            name_field: crate::copy_name_field::Field::new(names.clone()),
            name_focus: false,
            clipboard: None,
            multi: Some(proof),
            moving: p.moving,
            extracting: false,
            creating: false,
            archive: None,
            preparing: false,
            prepare_id: None,
            prepared: None,
            source: p.root,
            names,
            destination: String::new(),
            captured_destination: None,
            replace: true,
            start_paused: false,
            options_open: false,
            publication: files_core::copy_engine::Publication::StagedRename,
            preserve_symlink_times: true,
            existing_destination: files_core::copy_engine::ExistingDestination::Stop,
            error: String::new(),
        };
        form.capture_destination(destination);
        self.text_paste.invalidate();
        self.copy_form = Some(form);
        self.hits.clear();
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use files_core::{
        listing::Kind,
        search::{Hit, Report},
    };
    use std::{fs, sync::atomic::AtomicU64};
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static ID: AtomicU64 = AtomicU64::new(0);
            let p = files_core::theme::home()
                .join(".cache/files-test")
                .join(format!(
                    "search-source-{}-{}",
                    std::process::id(),
                    ID.fetch_add(1, Ordering::Relaxed)
                ));
            for d in ["a", "b", "out"] {
                fs::create_dir_all(p.join(d)).unwrap();
            }
            fs::write(p.join("a/first"), b"one").unwrap();
            fs::write(p.join("b/second"), b"two").unwrap();
            Self(p)
        }
        fn model(&self) -> Model {
            let mut m = Model::new(self.0.clone());
            m.action(crate::model::Action::Search);
            m.search.report = Some(Report {
                complete: true,
                hits: ["a/first", "b/second"]
                    .iter()
                    .map(|p| Hit {
                        relative: p.into(),
                        kind: Kind::File,
                        content: None,
                        snippet: None,
                    })
                    .collect(),
                ..Default::default()
            });
            m.search.results_focus();
            m.search.mark_all();
            m
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn late_capture_after_marks_change_or_close_never_opens_review() {
        let f = Fixture::new();
        let mut m = f.model();
        m.begin_search_transfer(false);
        let p = m.search_transfer.pending.as_ref().unwrap();
        let id = p.input.id;
        let captured =
            files_core::multi_origin::capture_provisional(&p.input.paths, &p.input.cancel).unwrap();
        m.search.select_result(0, false, false, false);
        m.receive_search_capture(id, Ok(captured));
        assert!(m.copy_form.is_none() && !m.search_transfer.busy());
        assert!(m.search.visible);
        m.begin_search_transfer(false);
        let p = m.search_transfer.pending.as_ref().unwrap();
        let id = p.input.id;
        let captured =
            files_core::multi_origin::capture_provisional(&p.input.paths, &p.input.cancel).unwrap();
        m.search.close();
        m.receive_search_capture(id, Ok(captured));
        assert!(m.copy_form.is_none());
    }
    #[test]
    fn reviewed_capture_keeps_original_roots_and_new_destination_names() {
        let f = Fixture::new();
        let mut m = f.model();
        m.begin_search_transfer(false);
        let p = m.search_transfer.pending.as_ref().unwrap();
        let id = p.input.id;
        let captured =
            files_core::multi_origin::capture_provisional(&p.input.paths, &p.input.cancel).unwrap();
        m.receive_search_capture(id, Ok(captured));
        let form = m.copy_form.as_mut().unwrap();
        form.name_field.select_all();
        form.name_field.input("renamed-first").unwrap();
        form.name_field.commit().unwrap();
        assert!(m.search.visible && form.multi.is_some());
        assert_eq!(
            form.names.as_ref(),
            [OsString::from("first"), OsString::from("second")]
        );
        let requests = build(
            form.multi.as_ref().unwrap(),
            &f.0.join("out"),
            &form.names,
            &form.name_field.plan,
            &CopyOptions::default(),
        )
        .unwrap();
        assert_eq!(requests[0].source, f.0.join("a/first"));
        assert_eq!(requests[0].destination, f.0.join("out/renamed-first"));
        assert_eq!(requests[1].source, f.0.join("b/second"));
        assert!(
            build(
                form.multi.as_ref().unwrap(),
                &f.0.join("out"),
                &form.names,
                &form.name_field.plan,
                &CopyOptions {
                    max_requests: 1,
                    ..Default::default()
                }
            )
            .is_err()
        );
        assert!(
            build(
                form.multi.as_ref().unwrap(),
                &f.0.join("out"),
                &form.names,
                &form.name_field.plan,
                &CopyOptions {
                    max_request_path_bytes: 1,
                    ..Default::default()
                }
            )
            .is_err()
        );
        assert!(fs::read_dir(f.0.join("out")).unwrap().next().is_none());
        m.cancel_copy_dialog();
        assert!(m.search.visible && m.search.marks.len() == 2);
    }
    #[test]
    fn explicit_empty_and_duplicate_names_reject_before_any_capture_admission() {
        let f = Fixture::new();
        let mut m = f.model();
        m.search.marks.clear();
        m.begin_search_transfer(false);
        assert!(!m.search_transfer.busy() && m.search.error.contains("vacío"));
        m.search.mark_all();
        m.search.report.as_mut().unwrap().hits[1].relative = "b/first".into();
        m.search.mark_all();
        m.begin_search_transfer(false);
        assert!(!m.search_transfer.busy() && m.search.error.contains("mismo nombre"));
    }
    #[test]
    fn captured_review_paste_owns_destination_and_old_search_receipt_never_revives() {
        let f = Fixture::new();
        let mut m = f.model();
        let query = m.search.query.clone();
        let old = m.text_context();
        assert!(m.text_paste.request(old).is_some());
        m.begin_search_transfer(false);
        let p = m.search_transfer.pending.as_ref().unwrap();
        let id = p.input.id;
        let proof =
            files_core::multi_origin::capture_provisional(&p.input.paths, &p.input.cancel).unwrap();
        m.receive_search_capture(id, Ok(proof));
        assert_ne!(m.text_context(), old);
        assert!(!m.receive_text_paste("STALE"));
        assert!(m.text_paste.request(m.text_context()).is_some());
        assert!(m.receive_text_paste(f.0.join("out").to_str().unwrap()));
        assert_eq!(
            m.copy_form.as_ref().unwrap().destination_path(),
            f.0.join("out")
        );
        assert_eq!(m.search.query, query);
        assert!(m.text_paste.request(m.text_context()).is_some());
        m.cancel_copy_dialog();
        assert!(!m.receive_text_paste("STALE DESTINATION"));
        assert_eq!(m.search.query, query);
    }
    #[test]
    fn unsent_close_admits_nothing_and_sent_close_retains_reservation_until_exact_ack() {
        let f = Fixture::new();
        let mut m = f.model();
        let (tx, rx) = std::sync::mpsc::sync_channel(8);
        let (_reply, reply_rx) = std::sync::mpsc::channel();
        m.aux = crate::aux_io::AuxIo { tx, rx: reply_rx };
        m.begin_search_transfer(false);
        m.closing = true;
        m.pump_search_transfer();
        assert!(!m.search_transfer.busy());
        assert!(rx.try_recv().is_err());
        m.closing = false;
        m.begin_search_transfer(false);
        m.pump_search_transfer();
        let crate::aux_io::Request::SearchCapture(input) = rx.try_recv().unwrap() else {
            panic!("capture")
        };
        m.closing = true;
        m.pump_search_transfer();
        assert!(
            input.cancel.load(Ordering::Acquire)
                && m.search_transfer.busy()
                && m.search_transfer_fds() > 0
        );
        assert!(rx.try_recv().is_err());
        m.receive_search_capture(input.id, Err("cancelled".into()));
        assert!(!m.search_transfer.busy() && m.copy_form.is_none());
    }
    #[test]
    fn actual_shared_copy_and_move_workers_execute_captured_multi_roots_and_refresh_all_parents() {
        let f = Fixture::new();
        let mut m = f.model();
        fn finish(m: &mut Model) {
            let end = std::time::Instant::now() + std::time::Duration::from_secs(10);
            while m.copy_last.is_none() || m.copy.active_id().is_some() || m.copy.queued_count() > 0
            {
                m.pump();
                assert!(
                    std::time::Instant::now() < end,
                    "queue stalled: {:?}",
                    m.operation_message
                );
                std::thread::yield_now();
            }
        }
        fn capture(m: &mut Model, moving: bool) {
            m.begin_search_transfer(moving);
            let p = m.search_transfer.pending.as_ref().unwrap();
            let id = p.input.id;
            let proof =
                files_core::multi_origin::capture_provisional(&p.input.paths, &p.input.cancel)
                    .unwrap();
            m.receive_search_capture(id, Ok(proof));
        }
        capture(&mut m, false);
        m.copy_form
            .as_mut()
            .unwrap()
            .capture_destination(f.0.join("out"));
        m.start_copy();
        finish(&mut m);
        assert!(m.copy_last.as_ref().unwrap().result.is_ok());
        assert_eq!(fs::read(f.0.join("out/first")).unwrap(), b"one");
        assert!(f.0.join("a/first").exists());
        fs::remove_file(f.0.join("out/first")).unwrap();
        fs::remove_file(f.0.join("out/second")).unwrap();
        m.copy_last = None;
        capture(&mut m, true);
        m.copy_form
            .as_mut()
            .unwrap()
            .capture_destination(f.0.join("out"));
        m.start_copy();
        let end = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while m.copy_form.as_ref().is_some_and(|d| d.prepared.is_none()) {
            m.pump();
            assert!(
                std::time::Instant::now() < end,
                "review: {:?}",
                m.copy_form.as_ref().map(|d| &d.error)
            );
            std::thread::yield_now();
        }
        assert!(f.0.join("a/first").exists() && f.0.join("b/second").exists());
        m.start_copy();
        finish(&mut m);
        let report = m.copy_last.as_ref().unwrap().report().unwrap();
        assert_eq!(report.completed_count, 2);
        assert_eq!(report.completed.len(), 2);
        for parent in ["a", "b", "out"] {
            assert!(report.affected_directories.contains(&f.0.join(parent)));
        }
        assert!(!f.0.join("a/first").exists() && !f.0.join("b/second").exists());
        assert_eq!(fs::read(f.0.join("out/second")).unwrap(), b"two");
    }
    #[test]
    fn lost_reply_channel_clears_only_capture_as_failure_not_ack_and_never_opens_review() {
        let f = Fixture::new();
        let mut m = f.model();
        let (tx, rx) = std::sync::mpsc::sync_channel(8);
        let (reply, reply_rx) = std::sync::mpsc::channel();
        m.aux = crate::aux_io::AuxIo { tx, rx: reply_rx };
        m.begin_search_transfer(false);
        m.pump_search_transfer();
        let crate::aux_io::Request::SearchCapture(input) = rx.try_recv().unwrap() else {
            panic!("capture")
        };
        drop(reply);
        m.pump();
        assert!(
            input.cancel.load(Ordering::Acquire)
                && !m.search_transfer.busy()
                && m.copy_form.is_none()
        );
        assert!(m.search.error.contains("sin confirmar"));
    }
    #[test]
    fn cancel_review_keeps_descriptor_reservation_until_exact_worker_ack() {
        let f = Fixture::new();
        let mut m = f.model();
        m.begin_search_transfer(true);
        let p = m.search_transfer.pending.as_ref().unwrap();
        let id = p.input.id;
        let proof =
            files_core::multi_origin::capture_provisional(&p.input.paths, &p.input.cancel).unwrap();
        m.receive_search_capture(id, Ok(proof));
        m.copy_form
            .as_mut()
            .unwrap()
            .capture_destination(f.0.join("out"));
        let (tx, rx) = std::sync::mpsc::sync_channel(8);
        let (reply, reply_rx) = std::sync::mpsc::channel();
        m.aux = crate::aux_io::AuxIo { tx, rx: reply_rx };
        m.start_copy();
        let request = rx.try_recv().unwrap();
        let id = m.move_preparing.as_ref().unwrap().0;
        let reserve = m.move_review_fds;
        assert!(reserve > 0);
        m.cancel_copy_dialog();
        assert!(m.copy_form.is_none() && m.search_transfer_fds() >= reserve);
        drop(request);
        reply
            .send(crate::aux_io::Response::MovePrepared(
                id,
                Err("cancelled".into()),
            ))
            .unwrap();
        m.pump();
        assert!(m.move_preparing.is_none() && m.move_review_fds == 0);
    }
    #[test]
    fn batch_report_uses_effective_destination_after_parent_rename_and_keeps_partial_counts() {
        let f = Fixture::new();
        let paths = [f.0.join("a/first"), f.0.join("b/second")];
        let cancel = AtomicBool::new(false);
        let proof =
            Arc::new(files_core::multi_origin::capture_provisional(&paths, &cancel).unwrap());
        let token = files_core::multi_origin::prepare_move_batch(
            proof,
            &f.0.join("out"),
            256,
            2 * 1024 * 1024,
            &cancel,
        )
        .unwrap();
        let prepared = crate::move_operation::Prepared::batch(token, 2 * 1024 * 1024).unwrap();
        fs::rename(f.0.join("out"), f.0.join("renamed-out")).unwrap();
        let report = prepared
            .execute(&files_core::copy_engine::Control::default(), |_| {})
            .unwrap();
        assert_eq!(report.completed_count, 2);
        assert!(
            report
                .completed
                .iter()
                .all(|p| p.parent() == Some(f.0.join("renamed-out").as_path()))
        );
        assert!(
            report
                .affected_directories
                .contains(&f.0.join("renamed-out"))
        );
        assert!(!report.affected_directories.contains(&f.0.join("out")));
    }
    #[test]
    fn partial_batch_report_keeps_verified_first_group_and_visible_root_state_before_long_paths() {
        let f = Fixture::new();
        let paths = [f.0.join("a/first"), f.0.join("b/second")];
        let cancel = AtomicBool::new(false);
        let proof =
            Arc::new(files_core::multi_origin::capture_provisional(&paths, &cancel).unwrap());
        let token = files_core::multi_origin::prepare_move_batch(
            proof,
            &f.0.join("out"),
            256,
            2 * 1024 * 1024,
            &cancel,
        )
        .unwrap();
        let prepared = crate::move_operation::Prepared::batch(token, 2 * 1024 * 1024).unwrap();
        fs::write(f.0.join("out/second"), b"occupant").unwrap();
        let result = prepared.execute(&files_core::copy_engine::Control::default(), |_| {});
        let outcome = crate::copy_job::Outcome {
            exdev: None,
            moving: true,
            extracting: false,
            creating: false,
            archive_action: None,
            id: 1,
            result,
            progress: Default::default(),
        };
        assert_eq!(outcome.report().unwrap().completed_count, 1);
        assert_eq!(outcome.report().unwrap().completed, [f.0.join("out/first")]);
        assert!(
            outcome
                .report()
                .unwrap()
                .affected_directories
                .contains(&f.0.join("b"))
        );
        let text = crate::copy_report::describe(&outcome).text;
        assert!(text.contains("Raíz 1 · CONFIRMADA · solicitada"));
        assert!(text.contains("Raíz 2 · PENDIENTE · solicitada"));
        assert_eq!(fs::read(f.0.join("out/second")).unwrap(), b"occupant");
    }
    fn finish_retry_test(m: &mut Model, id: u64) {
        let end = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while m.copy_last.as_ref().is_none_or(|o| o.id != id) {
            m.pump();
            assert!(
                std::time::Instant::now() < end,
                "retry stalled: {:?}",
                m.operation_message
            );
            std::thread::yield_now();
        }
    }
    fn partial_retry_fixture(f: &Fixture) -> Model {
        let mut m = f.model();
        let cancel = AtomicBool::new(false);
        let proof = Arc::new(
            files_core::multi_origin::capture_provisional(
                &[f.0.join("a/first"), f.0.join("b/second")],
                &cancel,
            )
            .unwrap(),
        );
        let token = files_core::multi_origin::prepare_move_batch(
            proof,
            &f.0.join("out"),
            256,
            2 * 1024 * 1024,
            &cancel,
        )
        .unwrap();
        let prepared = crate::move_operation::Prepared::batch(token, 2 * 1024 * 1024).unwrap();
        fs::write(f.0.join("out/second"), b"occupant").unwrap();
        m.copy_id += 1;
        m.copy
            .enqueue(crate::copy_job::Request::reviewed_move(m.copy_id, prepared))
            .unwrap();
        let id = m.copy_id;
        finish_retry_test(&mut m, id);
        assert_eq!(m.pending_move_retry_count(), Some(1));
        assert!(m.notice_open && !f.0.join("a/first").exists());
        m
    }
    #[test]
    fn pending_retry_moves_only_original_pending_root_and_preserves_confirmed_replacement() {
        let f = Fixture::new();
        let mut m = partial_retry_fixture(&f);
        let confirmed = fs::metadata(f.0.join("out/first")).unwrap();
        use std::os::unix::fs::MetadataExt;
        fs::write(f.0.join("a/first"), b"new actor at confirmed source").unwrap();
        fs::remove_file(f.0.join("out/second")).unwrap();
        let old = m.copy_id;
        assert!(crate::mutations::notice_shortcut(
            &mut m,
            fltk::enums::Key::from_char('r'),
            true
        ));
        let id = m.copy_id;
        assert!(id > old && m.copy_last.is_none() && !m.notice_open);
        finish_retry_test(&mut m, id);
        let outcome = m.copy_last.as_ref().unwrap();
        assert!(outcome.result.is_ok());
        assert_eq!(outcome.report().unwrap().completed_count, 1);
        assert_eq!(fs::read(f.0.join("out/second")).unwrap(), b"two");
        assert_eq!(
            fs::read(f.0.join("a/first")).unwrap(),
            b"new actor at confirmed source"
        );
        assert_eq!(
            fs::metadata(f.0.join("out/first")).unwrap().ino(),
            confirmed.ino()
        );
        assert!(!f.0.join("b/second").exists());
    }
    #[test]
    fn pending_retry_rejects_substitution_without_recapturing_actor() {
        let f = Fixture::new();
        let mut m = partial_retry_fixture(&f);
        fs::remove_file(f.0.join("out/second")).unwrap();
        fs::rename(f.0.join("b/second"), f.0.join("b/original")).unwrap();
        fs::write(f.0.join("b/second"), b"actor").unwrap();
        m.retry_pending_move();
        let id = m.copy_id;
        finish_retry_test(&mut m, id);
        let outcome = m.copy_last.as_ref().unwrap();
        assert!(outcome.result.is_err());
        assert_eq!(outcome.report().unwrap().completed_count, 0);
        assert_eq!(fs::read(f.0.join("b/second")).unwrap(), b"actor");
        assert_eq!(fs::read(f.0.join("b/original")).unwrap(), b"two");
        assert!(!f.0.join("out/second").exists());
        assert_eq!(m.pending_move_retry_count(), Some(1));
    }
    #[test]
    fn rejected_retry_preserves_full_outcome_proof_and_reservation_then_can_retry() {
        let f = Fixture::new();
        let mut m = partial_retry_fixture(&f);
        let previous = m.copy_last.as_ref().unwrap().id;
        for _ in 0..crate::copy_state::MAX_PENDING_JOBS {
            m.copy_id += 1;
            m.copy
                .enqueue(crate::copy_job::Request::new(
                    m.copy_id,
                    vec![],
                    Default::default(),
                ))
                .unwrap();
        }
        let before = m.copy.budget();
        let fds = m.copy.fd_reservation();
        m.retry_pending_move();
        assert_eq!(m.copy_last.as_ref().unwrap().id, previous);
        assert_eq!(
            m.copy_last
                .as_ref()
                .unwrap()
                .report()
                .unwrap()
                .completed_count,
            1
        );
        assert_eq!(m.copy.budget().accounted_bytes, before.accounted_bytes);
        assert_eq!(m.copy.fd_reservation(), fds);
        assert!(m.notice_open && m.operation_details[0].contains("no encolado"));
        let diagnostic_count = m.operation_details.len();
        m.retry_pending_move();
        assert_eq!(m.operation_details.len(), diagnostic_count);
        assert_eq!(m.copy.fd_reservation(), fds);
        m.cancel_copy_pending();
        fs::remove_file(f.0.join("out/second")).unwrap();
        m.retry_pending_move();
        let id = m.copy_id;
        finish_retry_test(&mut m, id);
        assert!(m.copy_last.as_ref().unwrap().result.is_ok());
    }
    #[test]
    fn cleanup_uncertainty_blocks_retry_even_with_pending_ledger() {
        let f = Fixture::new();
        let mut m = partial_retry_fixture(&f);
        let Err(crate::copy_job::Failure::Batch(batch)) = &mut m.copy_last.as_mut().unwrap().result
        else {
            panic!("batch");
        };
        let files_core::multi_origin::BatchFailure::Engine { error, .. } = &mut batch.cause else {
            panic!("engine");
        };
        error.cleanup_unresolved_entries = 1;
        assert!(m.pending_move_retry_count().is_none());
        let id = m.copy_id;
        m.retry_pending_move();
        assert_eq!(m.copy_id, id);
        assert_eq!(fs::read(f.0.join("out/second")).unwrap(), b"occupant");
    }
    #[test]
    fn cancelling_admitted_batch_before_execution_retains_pending_proof_until_ack() {
        let f = Fixture::new();
        let mut m = partial_retry_fixture(&f);
        m.retry_pending_move();
        let queued = m.copy.cancel_pending(m.copy_id).unwrap();
        m.copy_id += 1;
        let id = m.copy_id;
        let mut queued = queued.start_paused();
        queued.id = id;
        m.copy.enqueue(queued).unwrap();
        // Dispatch paused first, then cancel via the actual shared controller.
        m.copy.pump();
        assert_eq!(m.copy.active_id(), Some(id));
        assert!(m.copy.cancel(id));
        assert!(m.copy_last.is_none());
        finish_retry_test(&mut m, id);
        assert!(m.copy_last.as_ref().unwrap().cancelled());
        assert_eq!(m.pending_move_retry_count(), Some(1));
        assert!(f.0.join("out/first").exists() && f.0.join("b/second").exists());
        assert_eq!(fs::read(f.0.join("out/second")).unwrap(), b"occupant");
        m.notice_open = false;
        assert!(!crate::mutations::notice_shortcut(
            &mut m,
            fltk::enums::Key::from_char('r'),
            true
        ));
        assert_eq!(m.copy_id, id);
    }
    #[test]
    fn unrelated_notice_never_offers_retained_batch_retry_until_its_report_is_reopened() {
        let f = Fixture::new();
        let mut m = partial_retry_fixture(&f);
        m.copy_notice_id = None;
        m.operation_message = Some("Otro aviso".into());
        assert!(m.pending_move_retry_count().is_none());
        let id = m.copy_id;
        m.retry_pending_move();
        assert_eq!(m.copy_id, id);
        m.show_copy_details();
        assert_eq!(m.pending_move_retry_count(), Some(1));
        assert!(
            m.operation_message
                .as_ref()
                .unwrap()
                .starts_with("Resultado de movimiento #")
        );
    }
}
