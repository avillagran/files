//! Single-flight space-map lifecycle. Open replaces only desired intent; no
//! replacement helper starts until the cancelled helper's reap/result ACK has
//! been consumed AND a subsequent pump runs. Call pump while closing as well.
use crate::space_map_job::Job;
use files_core::space_map::SpaceReport;
use std::path::{Path, PathBuf};
trait Task {
    fn cancel(&self);
    fn poll(&self) -> Option<Result<SpaceReport, String>>;
}
impl Task for Job {
    fn cancel(&self) {
        Job::cancel(self);
    }
    fn poll(&self) -> Option<Result<SpaceReport, String>> {
        Job::poll(self)
    }
}
#[derive(Debug)]
pub enum View {
    Closed,
    Loading,
    Ready(SpaceReport),
    Error(String),
}
struct Active {
    revision: u64,
    task: Box<dyn Task>,
}
pub struct State {
    revision: u64,
    path: Option<PathBuf>,
    needs_start: bool,
    active: Option<Active>,
    view: View,
    changed: bool,
    watch_gap: bool,
}
impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}
impl State {
    pub fn new() -> Self {
        Self {
            revision: 0,
            path: None,
            needs_start: false,
            active: None,
            view: View::Closed,
            changed: false,
            watch_gap: false,
        }
    }
    #[cfg(test)]
    pub(crate) fn ready_for_test(path: PathBuf, report: SpaceReport) -> Self {
        let mut state = Self::new();
        state.path = Some(path);
        state.view = View::Ready(report);
        state
    }
    pub fn open(&mut self, path: PathBuf) {
        self.close();
        if !crate::space_map_job::valid_path(&path) {
            self.view =
                View::Error("Espacio requiere una ruta absoluta de hasta4096bytes, sin NUL".into());
            return;
        }
        self.path = Some(path);
        self.needs_start = true;
        self.view = View::Loading;
    }
    /// Drop displayed data immediately, cancel active work, discard queued intent.
    /// Actual process/thread ownership remains until pump consumes the result.
    pub fn close(&mut self) {
        self.revision = self
            .revision
            .checked_add(1)
            .expect("space-map revision exhausted");
        self.path = None;
        self.needs_start = false;
        self.view = View::Closed;
        if let Some(active) = &self.active {
            active.task.cancel();
        }
    }
    /// A hint does not cancel work, discard the report, or queue another scan.
    /// Sticky across completion/cancellation; only a new observation intent clears it.
    pub fn invalidate(&mut self, watch_gap: bool) -> bool {
        let before = (self.changed, self.watch_gap);
        if watch_gap {
            self.watch_gap = true;
        } else {
            self.changed = true;
        }
        before != (self.changed, self.watch_gap)
    }
    pub fn changed(&self) -> bool {
        self.changed
    }
    pub fn watch_gap(&self) -> bool {
        self.watch_gap
    }
    pub fn clear_invalidation(&mut self) {
        self.changed = false;
        self.watch_gap = false;
    }
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn view(&self) -> &View {
        &self.view
    }
    pub fn visible(&self) -> bool {
        !matches!(self.view, View::Closed)
    }
    /// True is lifecycle drain, not evidence that the scan's measurements were
    /// complete. A Ready(report) may correctly describe partial filesystem data.
    pub fn is_drained(&self) -> bool {
        self.active.is_none() && !self.needs_start
    }
    pub fn pump(&mut self) -> bool {
        self.pump_with(|path| Job::start(path).map(|job| Box::new(job) as Box<dyn Task>))
    }
    fn pump_with(&mut self, start: impl FnOnce(PathBuf) -> Result<Box<dyn Task>, String>) -> bool {
        if let Some(active) = &self.active {
            if let Some(result) = active.task.poll() {
                let revision = active.revision;
                self.active = None;
                if revision == self.revision && self.path.is_some() && !self.needs_start {
                    self.view = match result {
                        Ok(report) => View::Ready(report),
                        Err(error) => View::Error(error),
                    };
                }
                // Never replace on this ACK pump: drop/release the result/old
                // task first, then allow dispatch on the following UI turn.
                return true;
            }
            return false;
        }
        if self.needs_start {
            self.needs_start = false;
            let path = self.path.as_ref().expect("pending space-map path").clone();
            match start(path) {
                Ok(task) => {
                    self.active = Some(Active {
                        revision: self.revision,
                        task,
                    })
                }
                Err(error) => self.view = View::Error(error),
            }
            return true;
        }
        false
    }
}
impl Drop for State {
    fn drop(&mut self) {
        // Drop is cancellation only. Window/application shutdown must call
        // close()+pump until is_drained(), never rely on Drop to wait for reap.
        if let Some(active) = &self.active {
            active.task.cancel();
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
    };
    #[derive(Default)]
    struct Stub {
        cancelled: Cell<bool>,
        result: RefCell<Option<Result<SpaceReport, String>>>,
    }
    impl Task for Rc<Stub> {
        fn cancel(&self) {
            self.cancelled.set(true);
        }
        fn poll(&self) -> Option<Result<SpaceReport, String>> {
            self.result.borrow_mut().take()
        }
    }
    fn report(bytes: u64) -> SpaceReport {
        SpaceReport {
            nodes: vec![files_core::space_map::SpaceNode {
                name: b"fixture".to_vec(),
                kind: files_core::space_map::NodeKind::File,
                bytes: Some(bytes),
                observed_bytes: bytes,
                files: 1,
                directories: 0,
                status: files_core::space_map::NodeStatus::Complete,
                error: None,
            }],
            total_bytes: bytes,
            observed_bytes: bytes,
            visited: 1,
            complete: true,
            reason: None,
            omitted_children: Some(0),
        }
    }
    fn no_start(_: PathBuf) -> Result<Box<dyn Task>, String> {
        panic!("unexpected replacement helper")
    }
    fn launch(state: &mut State, expected: &Path, stub: Rc<Stub>) {
        assert!(state.pump_with(|path| {
            assert_eq!(path, expected);
            Ok(Box::new(stub))
        }));
    }
    #[test]
    fn rapid_a_b_c_keeps_only_latest_and_separates_ack_from_dispatch() {
        let mut state = State::new();
        let a = Rc::new(Stub::default());
        state.open("/fixture/a".into());
        launch(&mut state, Path::new("/fixture/a"), Rc::clone(&a));
        state.open("/fixture/b".into());
        state.open("/fixture/c".into());
        assert!(a.cancelled.get());
        assert!(!state.is_drained());
        assert_eq!(state.path(), Some(Path::new("/fixture/c")));
        assert!(!state.pump_with(no_start));
        *a.result.borrow_mut() = Some(Ok(report(10)));
        assert!(state.pump_with(no_start)); // ACK: must not start C here
        assert!(matches!(state.view(), View::Loading));
        let c = Rc::new(Stub::default());
        launch(&mut state, Path::new("/fixture/c"), Rc::clone(&c));
        *c.result.borrow_mut() = Some(Ok(report(30)));
        assert!(state.pump_with(no_start));
        assert!(matches!(state.view(), View::Ready(report) if report.total_bytes == 30));
        assert!(state.is_drained());
    }
    #[test]
    fn close_discards_stale_success_error_and_queued_replacement() {
        for result in [Ok(report(1)), Err("cancelled old helper".into())] {
            let mut state = State::new();
            let stub = Rc::new(Stub::default());
            state.open("/fixture/a".into());
            launch(&mut state, Path::new("/fixture/a"), Rc::clone(&stub));
            state.open("/fixture/b".into());
            state.close();
            assert!(stub.cancelled.get());
            assert!(!state.visible());
            assert!(!state.is_drained());
            *stub.result.borrow_mut() = Some(result);
            assert!(state.pump_with(no_start));
            assert!(state.is_drained());
            assert!(!state.pump_with(no_start));
            assert!(state.path().is_none());
            assert!(matches!(state.view(), View::Closed));
        }
    }
    #[test]
    fn reopening_same_path_rejects_prior_revision_and_start_errors_allow_retry() {
        let mut state = State::new();
        let first = Rc::new(Stub::default());
        state.open("/fixture/same".into());
        launch(&mut state, Path::new("/fixture/same"), Rc::clone(&first));
        let revision = state.revision();
        state.open("/fixture/same".into());
        assert!(state.revision() > revision);
        *first.result.borrow_mut() = Some(Err("stale".into()));
        assert!(state.pump_with(no_start));
        assert!(matches!(state.view(), View::Loading));
        assert!(state.pump_with(|_| Err("capacidad no disponible".into())));
        assert!(matches!(state.view(), View::Error(_)));
        assert!(state.is_drained());
        state.open("/fixture/same".into());
        let next = Rc::new(Stub::default());
        launch(&mut state, Path::new("/fixture/same"), Rc::clone(&next));
        *next.result.borrow_mut() = Some(Ok(report(2)));
        state.pump_with(no_start);
        assert!(matches!(state.view(), View::Ready(report) if report.total_bytes == 2));
    }
    #[test]
    fn invalid_intent_cancels_current_work_without_starting_another() {
        let mut state = State::new();
        let current = Rc::new(Stub::default());
        state.open("/fixture/a".into());
        launch(&mut state, Path::new("/fixture/a"), Rc::clone(&current));
        state.open("relative".into());
        assert!(current.cancelled.get());
        assert!(matches!(state.view(), View::Error(_)));
        assert!(!state.is_drained());
        *current.result.borrow_mut() = Some(Ok(report(4)));
        assert!(state.pump_with(no_start));
        assert!(state.is_drained());
        assert!(matches!(state.view(), View::Error(_)));
    }
    #[test]
    fn invalidation_is_sticky_across_result_cancel_and_queued_start_without_restarting() {
        let mut state = State::new();
        let task = Rc::new(Stub::default());
        state.open("/fixture/a".into());
        launch(&mut state, Path::new("/fixture/a"), Rc::clone(&task));
        let revision = state.revision();
        assert!(state.invalidate(false));
        assert!(!state.invalidate(false));
        assert!(!task.cancelled.get());
        assert_eq!(state.revision(), revision);
        *task.result.borrow_mut() = Some(Ok(report(12)));
        assert!(state.pump_with(no_start));
        assert!(state.changed() && state.is_drained());
        assert!(matches!(state.view(), View::Ready(r) if r.total_bytes == 12));
        assert!(!state.pump_with(no_start));
        state.close();
        assert!(state.changed());
        assert!(state.invalidate(true));
        assert!(state.watch_gap());
        state.clear_invalidation();
        state.open("/fixture/a".into());
        // An event after explicit intent but before dispatch must survive open/start.
        state.invalidate(false);
        let next = Rc::new(Stub::default());
        launch(&mut state, Path::new("/fixture/a"), Rc::clone(&next));
        *next.result.borrow_mut() = Some(Ok(report(20)));
        state.pump_with(no_start);
        assert!(state.changed() && !state.watch_gap());
        assert!(matches!(state.view(), View::Ready(r) if r.total_bytes == 20));
    }
}
