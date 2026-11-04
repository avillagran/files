//! One bounded mutation worker. Filesystem probes and planning never run on UI.
use files_core::{
    rename_plan::{self, RenameOptions},
    rename_transaction::{PreparedRename, TransactionError, UndoToken},
};
use std::{
    ffi::OsString,
    fs,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
};
const MAX_NAMES: usize = 100_000;
const MAX_INPUT_BYTES: usize = 8 * 1024 * 1024;

pub enum Request {
    Inspect {
        id: u64,
        path: PathBuf,
    },
    Permissions {
        id: u64,
        snapshot: Box<files_core::file_properties::Snapshot>,
        mode: u32,
    },
    Plan {
        id: u64,
        directory: PathBuf,
        selected: Arc<[OsString]>,
        options: RenameOptions,
    },
    Apply {
        id: u64,
        prepared: PreparedRename,
    },
    History {
        id: u64,
        redo: bool,
    },
    Trash {
        id: u64,
        directory: PathBuf,
        names: Arc<[OsString]>,
        root: PathBuf,
    },
    Create {
        id: u64,
        directory: PathBuf,
        name: String,
        folder: bool,
    },
}
impl Request {
    fn id(&self) -> u64 {
        match self {
            Self::Inspect { id, .. }
            | Self::Permissions { id, .. }
            | Self::Plan { id, .. }
            | Self::Apply { id, .. }
            | Self::History { id, .. }
            | Self::Create { id, .. }
            | Self::Trash { id, .. } => *id,
        }
    }
}
pub struct Outcome {
    pub success: bool,
    pub message: String,
    pub details: Vec<String>,
    pub undo: bool,
    pub redo: bool,
}
pub enum Response {
    Properties(u64, Box<crate::properties_job::Reply>),
    Planned(u64, Result<PreparedRename, String>),
    Finished(u64, Outcome, Option<PathBuf>),
}
pub struct Worker {
    tx: Option<SyncSender<Request>>,
    rx: Option<Receiver<Response>>,
    cancel: Arc<AtomicBool>,
    pub busy: bool,
}
impl Worker {
    pub fn new() -> Self {
        Self {
            tx: None,
            rx: None,
            cancel: Arc::new(AtomicBool::new(false)),
            busy: false,
        }
    }
    fn start(&mut self) {
        let (tx, input) = mpsc::sync_channel::<Request>(1);
        let (output, rx) = mpsc::sync_channel(1);
        let flag = self.cancel.clone();
        std::thread::spawn(move || {
            // A single token bounds retained history, including its directory FD.
            let mut history: Option<(UndoToken, bool)> = None;
            let mut history_directory: Option<PathBuf> = None;
            let mut trash_history = crate::trash_history::History::default();
            while let Ok(request) = input.recv() {
                let id = request.id();
                let refresh_directory = match &request {
                    Request::Apply { prepared, .. } => Some(prepared.directory().to_path_buf()),
                    Request::Create { directory, .. } | Request::Trash { directory, .. } => {
                        Some(directory.clone())
                    }
                    Request::History { .. } => history_directory.clone(),
                    _ => None,
                };
                let response = match request {
                    Request::Inspect { path, .. } => Response::Properties(
                        id,
                        Box::new(crate::properties_job::inspect(path, &flag)),
                    ),
                    Request::Permissions { snapshot, mode, .. } => {
                        let reply = crate::properties_job::apply(snapshot, mode, &flag);
                        if reply.changed {
                            history_directory = None;
                            history = None;
                            trash_history.clear();
                        }
                        Response::Properties(id, Box::new(reply))
                    }
                    Request::Plan {
                        directory,
                        selected,
                        options,
                        ..
                    } => Response::Planned(id, plan(directory, &selected, &options, &flag)),
                    Request::Apply { prepared, .. } => match prepared.execute_cancellable(&flag) {
                        Ok(token) => {
                            let details = cleanup_details(&token);
                            trash_history.clear();
                            history_directory = refresh_directory.clone();
                            history = Some((token, false));
                            Response::Finished(
                                id,
                                Outcome {
                                    success: true,
                                    message: "Nombres actualizados".into(),
                                    details,
                                    undo: true,
                                    redo: false,
                                },
                                refresh_directory,
                            )
                        }
                        Err(error) => {
                            history = None;
                            let mut outcome = failed(error);
                            if trash_history.available() {
                                outcome.undo = true;
                                outcome.redo = false;
                            }
                            Response::Finished(id, outcome, refresh_directory)
                        }
                    },
                    Request::History { redo, .. } => {
                        let outcome = if trash_history.available() {
                            if redo {
                                Outcome {success:false,message:"Rehacer papelera no está disponible; Deshacer conserva los elementos pendientes".into(),details:Vec::new(),undo:true,redo:false}
                            } else {
                                trash_history.restore(&flag)
                            }
                        } else {
                            run_history(&mut history, redo, &flag)
                        };
                        Response::Finished(id, outcome, refresh_directory)
                    }
                    Request::Trash {
                        directory,
                        names,
                        root,
                        ..
                    } => {
                        let mut outcome =
                            trash_history.send(&directory, &names, &root, &flag, || {
                                history = None;
                                history_directory = Some(directory.clone());
                            });
                        if !trash_history.available() {
                            let (undo, redo) = history
                                .as_ref()
                                .map_or((false, false), |(_, redo)| (!*redo, *redo));
                            outcome.undo = undo;
                            outcome.redo = redo;
                        }
                        Response::Finished(id, outcome, refresh_directory)
                    }
                    Request::Create {
                        directory,
                        name,
                        folder,
                        ..
                    } => {
                        let result = create(&directory, &name, folder, &flag);
                        let outcome = match result {
                            Ok(()) => {
                                history_directory = None;
                                history = None;
                                trash_history.clear();
                                Outcome {
                                    success: true,
                                    message: format!("Creado: {name}"),
                                    details: Vec::new(),
                                    undo: false,
                                    redo: false,
                                }
                            }
                            Err(e) => {
                                let (undo, redo) = history
                                    .as_ref()
                                    .map_or((false, false), |(_, is_redo)| (!*is_redo, *is_redo));
                                Outcome {
                                    success: false,
                                    message: format!("No se pudo crear: {e}"),
                                    details: Vec::new(),
                                    undo: undo || trash_history.available(),
                                    redo: redo && !trash_history.available(),
                                }
                            }
                        };
                        Response::Finished(id, outcome, refresh_directory)
                    }
                };
                if output.send(response).is_err() {
                    break;
                }
            }
        });
        self.tx = Some(tx);
        self.rx = Some(rx);
    }
    /// Busy includes an unread response: callers cannot accumulate prepared plans.
    pub fn submit(&mut self, request: Request) -> Result<(), Box<Request>> {
        if self.busy {
            return Err(Box::new(request));
        }
        self.cancel.store(false, Ordering::Release);
        if self.tx.is_none() {
            self.start();
        }
        match self.tx.as_ref().expect("worker started").try_send(request) {
            Ok(()) => {
                self.busy = true;
                Ok(())
            }
            Err(mpsc::TrySendError::Full(request) | mpsc::TrySendError::Disconnected(request)) => {
                Err(Box::new(request))
            }
        }
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }
    pub fn poll(&mut self) -> Option<Response> {
        let rx = self.rx.as_ref()?;
        match rx.try_recv() {
            Ok(value) => {
                self.busy = false;
                Some(value)
            }
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) if self.busy => {
                self.busy = false;
                Some(Response::Finished(0, Outcome { success: false, message: "El trabajador terminó sin respuesta; revisa las rutas antes de continuar".into(), details: Vec::new(), undo: false, redo: false },None))
            }
            _ => None,
        }
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.cancel();
    }
}
fn check(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Acquire) {
        Err("Operación cancelada".into())
    } else {
        Ok(())
    }
}
fn plan(
    directory: PathBuf,
    selected: &[OsString],
    options: &RenameOptions,
    cancel: &AtomicBool,
) -> Result<PreparedRename, String> {
    plan_with_entry_limit(directory, selected, options, cancel, MAX_NAMES)
}
fn plan_with_entry_limit(
    directory: PathBuf,
    selected: &[OsString],
    options: &RenameOptions,
    cancel: &AtomicBool,
    max_entries: usize,
) -> Result<PreparedRename, String> {
    check(cancel)?;
    if selected.len() > MAX_NAMES {
        return Err("Reduce el lote: máximo 100000 nombres".into());
    }
    let mut budget = MAX_INPUT_BYTES;
    let mut names = Vec::with_capacity(selected.len());
    for name in selected {
        check(cancel)?;
        let bytes = name.as_encoded_bytes().len();
        if bytes > 4096 {
            return Err("Nombre demasiado largo".into());
        }
        budget = budget
            .checked_sub(bytes)
            .ok_or("Reduce el lote: presupuesto de nombres excedido")?;
        names.push(
            name.to_str()
                .ok_or(
                    "El lote seleccionado contiene nombres no UTF-8; no se convertirán con pérdida",
                )?
                .to_owned(),
        );
    }
    let mut existing = Vec::new();
    for (seen, entry) in fs::read_dir(&directory)
        .map_err(|e| e.to_string())?
        .enumerate()
    {
        check(cancel)?;
        if seen >= max_entries {
            return Err(
                "Carpeta mayor al límite de 100000 entradas (incluye nombres no UTF-8)".into(),
            );
        }
        let name = entry.map_err(|e| e.to_string())?.file_name();
        budget = budget
            .checked_sub(name.as_encoded_bytes().len())
            .ok_or("Reduce el lote: presupuesto de nombres excedido")?;
        // Invalid UTF-8 cannot equal any generated UTF-8 byte name. Preserve such
        // unselected entries; kernel no-clobber still validates actual targets.
        if let Ok(name) = name.into_string() {
            existing.push(name);
        }
    }
    let rows = rename_plan::plan_cancellable(&names, &existing, options, cancel)?;
    check(cancel)?;
    PreparedRename::capture(
        directory,
        rows.into_iter()
            .map(|r| (r.from.into(), r.to.into()))
            .collect(),
    )
    .map_err(|e| e.to_string())
}
fn run_history(history: &mut Option<(UndoToken, bool)>, redo: bool, flag: &AtomicBool) -> Outcome {
    let result = match history.as_ref() {
        Some((token, is_redo)) if *is_redo == redo => token.undo_cancellable(flag),
        _ => Err(
            std::io::Error::other("No hay una operación compatible para deshacer/rehacer").into(),
        ),
    };
    match result {
        Ok(token) => {
            let details = cleanup_details(&token);
            *history = Some((token, !redo));
            Outcome {
                success: true,
                message: if redo {
                    "Renombrado rehecho"
                } else {
                    "Renombrado deshecho"
                }
                .into(),
                details,
                undo: redo,
                redo: !redo,
            }
        }
        Err(error) => {
            let restored = error.preserved_object_count == 0
                && error.unresolved_preserved_objects == 0
                && error.rollback_errors.is_empty()
                && error.omitted_rollback_errors == 0;
            let mut outcome = failed(error);
            if restored {
                if let Some((_, is_redo)) = history {
                    outcome.undo = !*is_redo;
                    outcome.redo = *is_redo;
                }
            } else {
                *history = None;
            }
            outcome
        }
    }
}
fn cleanup_details(token: &UndoToken) -> Vec<String> {
    token
        .cleanup_errors
        .iter()
        .map(|e| format!("Limpieza pendiente: {e}"))
        .chain(
            token
                .cleanup_paths
                .iter()
                .map(|p| format!("Ruta de limpieza: {p:?}")),
        )
        .collect()
}
fn failed(error: TransactionError) -> Outcome {
    let mut details = error.rollback_errors;
    details.extend(
        error
            .preserved_paths
            .into_iter()
            .map(|p| format!("Rescate verificado: {p:?}")),
    );
    details.extend(
        error
            .unverified_parent_paths
            .into_iter()
            .map(|p| format!("Ubicación anterior NO verificada: {p:?}")),
    );
    details.push(format!("No restaurados: {}; sin ubicación verificada: {}; rutas agrupadas: {}; errores omitidos/truncados: {}/{}", error.preserved_object_count, error.unresolved_preserved_objects, error.aggregated_preserved_paths, error.omitted_rollback_errors, error.truncated_rollback_errors));
    Outcome {
        success: false,
        message: format!("No se pudo completar: {}", error.cause),
        details,
        undo: false,
        redo: false,
    }
}
fn create(
    directory: &std::path::Path,
    name: &str,
    folder: bool,
    cancel: &AtomicBool,
) -> std::io::Result<()> {
    check(cancel).map_err(std::io::Error::other)?;
    rename_plan::valid_name(name, true).map_err(std::io::Error::other)?;
    if name.len() > 255 {
        return Err(std::io::Error::other(
            "Nombre demasiado largo (máximo 255 bytes)",
        ));
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::{fd::AsRawFd, unix::fs::OpenOptionsExt};
        let canonical = directory.canonicalize()?;
        let parent = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(canonical)?;
        let component = std::ffi::CString::new(name).map_err(std::io::Error::other)?;
        if folder {
            if unsafe { libc::mkdirat(parent.as_raw_fd(), component.as_ptr(), 0o777) } != 0 {
                return Err(std::io::Error::last_os_error());
            }
        } else {
            let fd = unsafe {
                libc::openat(
                    parent.as_raw_fd(),
                    component.as_ptr(),
                    libc::O_CREAT
                        | libc::O_EXCL
                        | libc::O_WRONLY
                        | libc::O_CLOEXEC
                        | libc::O_NOFOLLOW,
                    0o666,
                )
            };
            if fd < 0 {
                return Err(std::io::Error::last_os_error());
            }
            unsafe {
                libc::close(fd);
            }
        }
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let target = directory.join(name);
        if folder {
            fs::create_dir(target)
        } else {
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(target)
                .map(drop)
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU64;
    use std::time::{Duration, Instant};

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "files-rename-worker-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    fn receive(worker: &mut Worker) -> Response {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(response) = worker.poll() {
                return response;
            }
            assert!(Instant::now() < deadline, "worker response timed out");
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    fn run(worker: &mut Worker, request: Request) -> Outcome {
        let expected = request.id();
        assert!(worker.submit(request).is_ok());
        match receive(worker) {
            Response::Finished(id, outcome, _) => {
                assert_eq!(id, expected);
                outcome
            }
            Response::Planned(_, _) | Response::Properties(_, _) => {
                panic!("expected finished mutation")
            }
        }
    }

    #[test]
    fn trash_create_failure_conflict_and_restore_retry_keep_history() {
        let dir = Fixture::new();
        let root = dir.0.join("Trash");
        fs::write(dir.0.join("before"), b"original").unwrap();
        let mut worker = Worker::new();
        let result = run(
            &mut worker,
            Request::Trash {
                id: 1,
                directory: dir.0.clone(),
                names: vec!["before".into()].into(),
                root: root.clone(),
            },
        );
        assert!(
            result.success && result.undo && !result.redo,
            "{} {:?}",
            result.message,
            result.details
        );
        assert!(!dir.0.join("before").exists());
        assert_eq!(fs::read_dir(root.join("files")).unwrap().count(), 1);
        fs::write(dir.0.join("occupied"), b"unchanged").unwrap();
        let failed = run(
            &mut worker,
            Request::Create {
                id: 2,
                directory: dir.0.clone(),
                name: "occupied".into(),
                folder: false,
            },
        );
        assert!(!failed.success && failed.undo && !failed.redo);
        fs::write(dir.0.join("before"), b"replacement").unwrap();
        let conflict = run(&mut worker, Request::History { id: 3, redo: false });
        assert!(!conflict.success && conflict.undo);
        assert_eq!(fs::read(dir.0.join("before")).unwrap(), b"replacement");
        assert!(
            conflict
                .details
                .iter()
                .any(|d| d.starts_with("Conservado:")),
            "{:?}",
            conflict.details
        );
        fs::remove_file(dir.0.join("before")).unwrap();
        let result = run(&mut worker, Request::History { id: 4, redo: false });
        assert!(
            result.success && !result.undo && !result.redo,
            "{} {:?}",
            result.message,
            result.details
        );
        assert_eq!(fs::read(dir.0.join("before")).unwrap(), b"original");
        assert_eq!(fs::read(dir.0.join("occupied")).unwrap(), b"unchanged");
        assert_eq!(fs::read_dir(root.join("files")).unwrap().count(), 0);
        assert_eq!(fs::read_dir(root.join("info")).unwrap().count(), 0);
    }
    #[test]
    fn trash_failure_before_commit_preserves_rename_undo() {
        let dir = Fixture::new();
        fs::write(dir.0.join("before"), b"keep").unwrap();
        let mut worker = Worker::new();
        let prepared =
            PreparedRename::capture(dir.0.clone(), vec![("before".into(), "after".into())])
                .unwrap();
        assert!(run(&mut worker, Request::Apply { id: 1, prepared }).success);
        let result = run(
            &mut worker,
            Request::Trash {
                id: 2,
                directory: dir.0.clone(),
                names: vec!["missing".into()].into(),
                root: dir.0.join("Trash"),
            },
        );
        assert!(!result.success && result.undo && !result.redo);
        assert!(run(&mut worker, Request::History { id: 3, redo: false }).success);
        assert_eq!(fs::read(dir.0.join("before")).unwrap(), b"keep");
    }

    #[test]
    fn failed_creation_keeps_undo_and_redo_accessible() {
        let dir = Fixture::new();
        fs::write(dir.0.join("before"), b"preserved contents").unwrap();
        let mut worker = Worker::new();
        let prepared =
            PreparedRename::capture(dir.0.clone(), vec![("before".into(), "after".into())])
                .unwrap();
        let result = run(&mut worker, Request::Apply { id: 1, prepared });
        assert!(result.success && result.undo && !result.redo);
        let result = run(
            &mut worker,
            Request::Create {
                id: 2,
                directory: dir.0.clone(),
                name: "after".into(),
                folder: false,
            },
        );
        assert!(!result.success && result.undo && !result.redo);
        assert_eq!(
            fs::read(dir.0.join("after")).unwrap(),
            b"preserved contents"
        );
        let result = run(&mut worker, Request::History { id: 3, redo: false });
        assert!(result.success && !result.undo && result.redo);
        let result = run(
            &mut worker,
            Request::Create {
                id: 4,
                directory: dir.0.clone(),
                name: "before".into(),
                folder: true,
            },
        );
        assert!(!result.success && !result.undo && result.redo);
        let result = run(&mut worker, Request::History { id: 5, redo: true });
        assert!(result.success && result.undo && !result.redo);
        assert_eq!(
            fs::read(dir.0.join("after")).unwrap(),
            b"preserved contents"
        );
    }

    #[test]
    fn cancelled_history_retains_token_for_retry() {
        let dir = Fixture::new();
        fs::write(dir.0.join("before"), b"data").unwrap();
        let token = PreparedRename::capture(dir.0.clone(), vec![("before".into(), "after".into())])
            .unwrap()
            .execute_cancellable(&AtomicBool::new(false))
            .unwrap();
        let mut history = Some((token, false));
        let result = run_history(&mut history, false, &AtomicBool::new(true));
        assert!(!result.success && result.undo && !result.redo);
        assert!(dir.0.join("after").exists());
        let result = run_history(&mut history, false, &AtomicBool::new(false));
        assert!(result.success && !result.undo && result.redo);
        assert_eq!(fs::read(dir.0.join("before")).unwrap(), b"data");
    }

    #[test]
    fn prepared_response_must_be_consumed_before_next_request() {
        let dir = Fixture::new();
        fs::write(dir.0.join("a"), b"a").unwrap();
        let mut worker = Worker::new();
        assert!(
            worker
                .submit(Request::Plan {
                    id: 7,
                    directory: dir.0.clone(),
                    selected: vec![OsString::from("a")].into(),
                    options: RenameOptions::default()
                })
                .is_ok()
        );
        assert!(
            worker
                .submit(Request::History { id: 8, redo: false })
                .is_err()
        );
        match receive(&mut worker) {
            Response::Planned(id, plan) => {
                assert_eq!(id, 7);
                assert!(plan.is_ok());
            }
            _ => panic!("expected plan"),
        }
        assert!(!worker.busy);
    }

    #[test]
    fn creation_rejects_reserved_names_and_never_clobbers() {
        let dir = Fixture::new();
        let cancel = AtomicBool::new(false);
        for name in ["CON.txt", "LPT1", "../outside", "a/b"] {
            assert!(create(&dir.0, name, false, &cancel).is_err());
        }
        assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 0);
        create(&dir.0, "valid.txt", false, &cancel).unwrap();
        fs::write(dir.0.join("valid.txt"), b"keep").unwrap();
        assert!(create(&dir.0, "valid.txt", false, &cancel).is_err());
        assert_eq!(fs::read(dir.0.join("valid.txt")).unwrap(), b"keep");
        cancel.store(true, Ordering::Release);
        assert!(create(&dir.0, "cancelled", true, &cancel).is_err());
        assert!(!dir.0.join("cancelled").exists());
    }
    #[test]
    fn every_directory_entry_counts_toward_limit_even_when_not_utf8() {
        use std::os::unix::ffi::OsStringExt;
        let dir = Fixture::new();
        fs::write(dir.0.join("a"), b"a").unwrap();
        fs::write(dir.0.join(OsString::from_vec(vec![0xff])), b"keep").unwrap();
        let flag = AtomicBool::new(false);
        let selected = vec![OsString::from("a")];
        assert!(
            plan_with_entry_limit(
                dir.0.clone(),
                &selected,
                &RenameOptions::default(),
                &flag,
                2
            )
            .is_ok()
        );
        fs::write(dir.0.join(OsString::from_vec(vec![0xfe])), b"keep").unwrap();
        let error = plan_with_entry_limit(
            dir.0.clone(),
            &selected,
            &RenameOptions::default(),
            &flag,
            2,
        )
        .err()
        .unwrap();
        assert!(error.contains("incluye nombres no UTF-8"));
        assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 3);
    }
}
