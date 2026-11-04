//! Un único trabajador acotado para preferencias/layouts; nunca uno por gesto.
use files_core::{
    spatial::SpatialLayout,
    theme::{Corners, WindowMode},
};
use std::{
    path::PathBuf,
    sync::Arc,
    sync::mpsc::{self, Receiver, SyncSender},
    thread,
};
pub enum Request {
    TrashCatalog(u64, PathBuf, Arc<std::sync::atomic::AtomicBool>),
    PrepareCreateArchive(
        u64,
        PathBuf,
        Vec<std::ffi::OsString>,
        PathBuf,
        std::ffi::OsString,
        files_core::copy_engine::Control,
        usize,
    ),
    PrepareArchive(
        u64,
        PathBuf,
        PathBuf,
        std::ffi::OsString,
        files_core::copy_engine::Control,
        usize,
    ),
    SavedSearches(u64, Arc<files_core::saved_search::Op>),
    SftpProfiles(u64, Arc<files_core::sftp_profiles::Op>),
    SearchCapture(crate::search_transfer::Input),
    PrepareMultiMove(
        u64,
        Arc<files_core::multi_origin::Captured>,
        PathBuf,
        usize,
        usize,
        Arc<std::sync::atomic::AtomicBool>,
    ),
    SearchDate(u64, files_core::search_dates::Preset, std::time::SystemTime),
    SftpCapability(u64),
    Bookmarks(u64, Arc<files_core::bookmarks::Op>),
    FolderViews(u64, Arc<files_core::folder_views::Op>),
    NativeDrag(
        u64,
        PathBuf,
        Arc<[std::ffi::OsString]>,
        Arc<std::sync::atomic::AtomicBool>,
    ),
    NativeEncode(
        u64,
        Arc<crate::file_clipboard::Lease>,
        Arc<std::sync::atomic::AtomicBool>,
    ),
    NativeImport(
        u64,
        Arc<crate::native_transfer_io::Reader>,
        files_core::file_transfer::Format,
    ),
    SelectionCapture(
        crate::file_clipboard::Input,
        Arc<std::sync::atomic::AtomicBool>,
    ),
    MoveCatalog(u64, Arc<std::sync::atomic::AtomicBool>),
    PrepareMove(
        u64,
        Vec<files_core::copy_engine::CopyRequest>,
        Arc<std::sync::atomic::AtomicBool>,
        Option<Arc<crate::file_clipboard::Lease>>,
    ),
    SessionOpen(bool),
    SessionSave(u64, Arc<Vec<u8>>),
    Load(u64, PathBuf),
    Save(u64, PathBuf, Arc<SpatialLayout>),
    Preferences(u64, Corners, WindowMode, bool, bool),
}
pub enum Response {
    TrashCatalog(u64, Result<files_core::trash::Catalog, String>),
    ArchivePrepared(u64, Result<crate::archive_operation::Prepared, String>),
    SavedSearches(u64, Result<files_core::saved_search::Updated, String>),
    SftpProfiles(u64, Result<files_core::sftp_profiles::Updated, String>),
    SearchCaptured(u64, Result<files_core::multi_origin::Captured, String>),
    SearchDate(u64, Result<files_core::search_dates::Range, String>),
    SftpCapability(u64, Result<crate::sftp_state::Capability, String>),
    Bookmarks(u64, Result<files_core::bookmarks::Updated, String>),
    FolderViews(u64, Result<files_core::folder_views::Updated, String>),
    NativeDragPrepared(u64, Result<crate::native_transfer_io::DragPayload, String>),
    NativeEncoded(u64, Result<Vec<u8>, String>),
    NativeImported(u64, Result<crate::native_transfer_io::Imported, String>),
    SelectionCaptured(u64, Result<files_core::selection_source::Source, String>),
    MoveCatalog(u64, Result<crate::move_catalog::Catalog, String>),
    MovePrepared(u64, Result<crate::move_operation::Prepared, String>),
    SessionOpened(crate::session_store::Opened),
    SessionSaved(u64, Result<crate::session_store::Published, String>),
    Layout(u64, Result<(PathBuf, SpatialLayout), String>),
    Saved(PathBuf, u64, Result<(), String>),
    Config(u64, Result<(), String>),
}
pub struct AuxIo {
    pub tx: SyncSender<Request>,
    pub rx: Receiver<Response>,
}
impl AuxIo {
    pub fn new() -> Self {
        let (tx, input) = mpsc::sync_channel(8);
        let (output, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut session_store = None;
            while let Ok(job) = input.recv() {
                let result = match job {
                    Request::TrashCatalog(id, root, cancel) => Response::TrashCatalog(
                        id,
                        files_core::trash::catalog(&root, &cancel).map_err(|e| e.to_string()),
                    ),
                    Request::PrepareCreateArchive(
                        id,
                        source,
                        names,
                        parent,
                        name,
                        control,
                        available,
                    ) => Response::ArchivePrepared(
                        id,
                        crate::archive_operation::Prepared::capture_create(
                            &source, &names, &parent, &name, &control,
                        )
                        .and_then(|p| {
                            if control.is_cancelled() {
                                Err(std::io::Error::new(
                                    std::io::ErrorKind::Interrupted,
                                    "Revisión cancelada",
                                ))
                            } else if p.metadata_bytes().saturating_add(64 * 1024) > available {
                                Err(std::io::Error::other(
                                    "Plan creación ZIP supera presupuesto de cola",
                                ))
                            } else {
                                Ok(p)
                            }
                        })
                        .map_err(|e| e.to_string()),
                    ),
                    Request::PrepareArchive(id, source, parent, name, control, available) => {
                        Response::ArchivePrepared(
                            id,
                            crate::archive_operation::Prepared::capture(
                                &source, &parent, &name, &control,
                            )
                            .and_then(|p| {
                                if control.is_cancelled() {
                                    Err(std::io::Error::new(
                                        std::io::ErrorKind::Interrupted,
                                        "Revisión ZIP cancelada",
                                    ))
                                } else if p.metadata_bytes().saturating_add(64 * 1024) > available {
                                    Err(std::io::Error::other(
                                        "Plan ZIP supera presupuesto de cola; reduce el archivo",
                                    ))
                                } else {
                                    Ok(p)
                                }
                            })
                            .map_err(|e| e.to_string()),
                        )
                    }
                    Request::SftpProfiles(id, op) => Response::SftpProfiles(
                        id,
                        files_core::sftp_profiles::apply(&files_core::theme::config_dir(), &op)
                            .map_err(|e| e.to_string()),
                    ),
                    Request::SavedSearches(id, op) => Response::SavedSearches(
                        id,
                        files_core::saved_search::apply(&files_core::theme::config_dir(), &op)
                            .map_err(|e| e.to_string()),
                    ),
                    Request::SearchCapture(input) => Response::SearchCaptured(
                        input.id,
                        files_core::multi_origin::capture_provisional(&input.paths, &input.cancel)
                            .map_err(|e| e.to_string()),
                    ),
                    Request::PrepareMultiMove(id, proof, destination, fds, bytes, cancel) => {
                        Response::MovePrepared(
                            id,
                            crate::move_operation::prepare_captured(
                                proof,
                                &destination,
                                fds,
                                bytes,
                                &cancel,
                            ),
                        )
                    }
                    Request::SearchDate(id, preset, reference) => Response::SearchDate(
                        id,
                        files_core::search_dates::resolve(preset, reference),
                    ),
                    Request::SftpCapability(id) => {
                        Response::SftpCapability(id, crate::sftp_state::probe())
                    }
                    Request::FolderViews(id, op) => Response::FolderViews(
                        id,
                        files_core::folder_views::apply(&files_core::theme::config_dir(), &op)
                            .map_err(|e| e.to_string()),
                    ),
                    Request::Bookmarks(id, op) => Response::Bookmarks(
                        id,
                        files_core::bookmarks::apply(&files_core::theme::config_dir(), &op)
                            .map_err(|e| e.to_string()),
                    ),
                    Request::NativeDrag(id, parent, names, cancel) => Response::NativeDragPrepared(
                        id,
                        crate::native_transfer_io::capture_drag(parent, names, &cancel),
                    ),
                    Request::NativeEncode(id, lease, cancel) => Response::NativeEncoded(
                        id,
                        crate::native_transfer_io::encode(&lease, &cancel),
                    ),
                    Request::NativeImport(id, reader, format) => Response::NativeImported(
                        id,
                        crate::native_transfer_io::import(&reader, format),
                    ),
                    Request::SelectionCapture(input, cancel) => Response::SelectionCaptured(
                        input.id,
                        files_core::selection_source::Source::capture(
                            input.parent,
                            input.names,
                            &cancel,
                        )
                        .map_err(|e| e.to_string()),
                    ),
                    Request::MoveCatalog(id, cancel) => {
                        Response::MoveCatalog(id, crate::move_catalog::read(&cancel))
                    }
                    Request::PrepareMove(id, paths, cancel, clipboard) => Response::MovePrepared(
                        id,
                        crate::move_operation::prepare_checked(
                            paths,
                            clipboard.as_deref(),
                            &cancel,
                        ),
                    ),
                    Request::SessionOpen(restore) => {
                        let (store, opened) = crate::session_store::Store::open(
                            &files_core::theme::config_dir(),
                            restore,
                        );
                        session_store = Some(store);
                        Response::SessionOpened(opened)
                    }
                    Request::SessionSave(revision, bytes) => Response::SessionSaved(
                        revision,
                        session_store
                            .as_mut()
                            .ok_or_else(|| "Sesión sin iniciar".into())
                            .and_then(|s| s.save(&bytes)),
                    ),
                    Request::Load(g, p) => Response::Layout(
                        g,
                        (|| {
                            let canonical = std::fs::canonicalize(&p)?;
                            let layout = SpatialLayout::load(&canonical)?;
                            Ok((canonical, layout))
                        })()
                        .map_err(|e: std::io::Error| e.to_string()),
                    ),
                    Request::Save(g, p, l) => {
                        Response::Saved(p.clone(), g, l.save(&p).map_err(|e| e.to_string()))
                    }
                    Request::Preferences(revision, c, mode, show_hidden, operation_bar) => {
                        Response::Config(
                            revision,
                            files_core::persistence::update_config_values(
                                &files_core::theme::config_dir().join("config.toml"),
                                &[
                                    (
                                        "corners",
                                        match c {
                                            Corners::Omarchy => "omarchy",
                                            Corners::Square => "square",
                                            Corners::Rounded => "rounded",
                                        }
                                        .into(),
                                    ),
                                    ("window_mode", mode.key().into()),
                                    ("show_hidden", show_hidden.into()),
                                    ("operation_bar", operation_bar.into()),
                                ],
                            ),
                        )
                    }
                };
                if output.send(result).is_err() {
                    break;
                }
            }
        });
        Self { tx, rx }
    }
}
