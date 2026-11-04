//! Permanent deletion (Shift+Delete) and trash administration (restore an
//! item, delete an item, empty the trash), like Nautilus.
//!
//! Every destructive request goes through an explicit confirmation that
//! names what will be lost. The work runs in its own cancellable thread
//! (never the UI thread, never the shared AuxIo worker, so a large tree does
//! not delay preferences or sessions); the UI polls counters and the result.
use crate::model::{Action, Hit, Model};
use crate::painter::{Painter, Rect};
use files_core::remove_tree::{Progress, Report};
use std::{
    ffi::OsString,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
};

#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    /// Selected entries of the current folder.
    Paths(Vec<PathBuf>),
    /// One trash item (name under `files/`) and its label for the dialog.
    TrashItem {
        name: OsString,
        label: String,
    },
    TrashEmpty {
        count: usize,
    },
    TrashRestore {
        name: OsString,
    },
    /// Shift+Ctrl+M: symbolic links to these targets inside a folder.
    Links {
        dir: PathBuf,
        targets: Vec<PathBuf>,
    },
}

#[derive(Debug)]
pub enum Outcome {
    Deleted(Report),
    Restored {
        path: PathBuf,
        cleanup: Option<String>,
    },
    Linked(Vec<Result<PathBuf, String>>),
}

pub struct Job {
    pub target: Target,
    cancel: Arc<AtomicBool>,
    progress: Arc<Progress>,
    result: Receiver<Result<Outcome, String>>,
}
impl Job {
    fn start(target: Target, trash_root: Option<PathBuf>) -> Result<Self, String> {
        let cancel = Arc::new(AtomicBool::new(false));
        let progress = Arc::new(Progress::default());
        let (tx, result) = mpsc::sync_channel(1);
        let (c, pr, t) = (cancel.clone(), progress.clone(), target.clone());
        std::thread::Builder::new()
            .name("files-delete".into())
            .spawn(move || {
                let root = || {
                    trash_root
                        .clone()
                        .ok_or_else(|| "Papelera no disponible".to_string())
                };
                let out = match t {
                    Target::Paths(paths) => Ok(Outcome::Deleted(
                        files_core::remove_tree::remove_paths(&paths, &c, &pr),
                    )),
                    Target::TrashItem { name, .. } => root().and_then(|r| {
                        files_core::trash::purge_item(&r, &name, &c, &pr)
                            .map(Outcome::Deleted)
                            .map_err(|e| e.to_string())
                    }),
                    Target::TrashEmpty { .. } => root().and_then(|r| {
                        files_core::trash::empty(&r, &c, &pr)
                            .map(Outcome::Deleted)
                            .map_err(|e| e.to_string())
                    }),
                    Target::Links { dir, targets } => {
                        Ok(Outcome::Linked(files_core::links::create(&dir, &targets)))
                    }
                    Target::TrashRestore { name } => root().and_then(|r| {
                        files_core::trash::restore_item(&r, &name, &c)
                            .map(|(path, cleanup)| Outcome::Restored { path, cleanup })
                            .map_err(|e| e.to_string())
                    }),
                };
                let _ = tx.send(out);
            })
            .map_err(|e| format!("No se pudo iniciar el borrado: {e}"))?;
        Ok(Self {
            target,
            cancel,
            progress,
            result,
        })
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }
    pub fn entries(&self) -> u64 {
        self.progress.entries.load(Ordering::Relaxed)
    }
}

#[derive(Default)]
pub struct State {
    pub confirm: Option<Target>,
    pub job: Option<Job>,
    /// Last finished trash operation, shown in the trash catalog.
    pub trash_notice: Option<String>,
    shown_entries: u64,
}
impl State {
    pub fn busy(&self) -> bool {
        self.job.is_some()
    }
}

fn is_trash(t: &Target) -> bool {
    !matches!(t, Target::Paths(_) | Target::Links { .. })
}

impl Model {
    /// Shift+Delete: ask before deleting the selection (or focused entry).
    pub fn request_permanent_delete(&mut self) {
        if self.editing || self.closing {
            return;
        }
        if self.delete.busy() || self.mutation_applied || self.copy_busy() || self.mutations.busy {
            self.operation_message = Some("Espera la tarea actual antes de eliminar".into());
            return;
        }
        let paths: Vec<PathBuf> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(i, e)| {
                (self.selection.contains(i)
                    || self.selection.is_empty() && self.selected == Some(*i))
                    && files_core::visibility::accepts(&e.name, self.show_hidden)
            })
            .map(|(_, e)| self.path.join(&e.name))
            .collect();
        if paths.is_empty() {
            self.operation_message = Some("Selecciona algo para eliminar".into());
            return;
        }
        self.context_menu = None;
        self.actions_menu = false;
        self.hits.clear();
        self.delete.confirm = Some(Target::Paths(paths));
    }
    /// Shift+Ctrl+M: a link to each selected entry, in the current folder.
    pub fn create_links(&mut self) {
        if self.editing || self.closing || self.delete.busy() {
            return;
        }
        let targets: Vec<PathBuf> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(i, e)| {
                (self.selection.contains(i)
                    || self.selection.is_empty() && self.selected == Some(*i))
                    && files_core::visibility::accepts(&e.name, self.show_hidden)
            })
            .map(|(_, e)| self.path.join(&e.name))
            .collect();
        if targets.is_empty() {
            self.operation_message = Some("Selecciona algo para enlazar".into());
            return;
        }
        let dir = self.path.clone();
        self.start_delete(Target::Links { dir, targets });
    }
    pub fn request_trash_operation(&mut self, target: Target) {
        if self.delete.busy() {
            self.delete.trash_notice = Some("Espera a que termine la operación en curso".into());
            return;
        }
        match target {
            Target::TrashRestore { .. } => self.start_delete(target),
            _ => {
                self.hits.clear();
                self.delete.confirm = Some(target);
            }
        }
    }
    pub fn answer_delete(&mut self, confirmed: bool) {
        let Some(target) = self.delete.confirm.take() else {
            return;
        };
        self.hits.clear();
        if confirmed {
            self.start_delete(target);
        }
    }
    fn start_delete(&mut self, target: Target) {
        let root = crate::trash_history::root().ok();
        let trash = is_trash(&target);
        match Job::start(target, root) {
            Ok(job) => {
                if matches!(job.target, Target::Paths(_)) {
                    self.operation_message = Some("Eliminando…".into());
                }
                self.delete.shown_entries = 0;
                self.delete.job = Some(job);
            }
            Err(e) => {
                if trash {
                    self.delete.trash_notice = Some(e);
                } else {
                    self.operation_message = Some(e);
                }
            }
        }
    }
    pub fn cancel_delete(&mut self) {
        if let Some(job) = &self.delete.job {
            job.cancel();
        }
    }
    /// Polls the running job; true when the screen should change.
    pub fn pump_delete(&mut self) -> bool {
        let Some(job) = &self.delete.job else {
            return false;
        };
        if self.closing {
            job.cancel();
        }
        let result = match job.result.try_recv() {
            Ok(r) => r,
            Err(mpsc::TryRecvError::Empty) => {
                let n = job.entries();
                if n != self.delete.shown_entries && matches!(job.target, Target::Paths(_)) {
                    self.delete.shown_entries = n;
                    self.operation_message = Some(format!("Eliminando… {n} elementos"));
                    return true;
                }
                return false;
            }
            Err(mpsc::TryRecvError::Disconnected) => Err("El borrado terminó sin respuesta".into()),
        };
        let job = self.delete.job.take().expect("job");
        let text = match &result {
            Ok(Outcome::Deleted(report)) => report.summary(),
            Ok(Outcome::Restored { path, cleanup }) => {
                let mut s = format!(
                    "Restaurado en {}",
                    files_core::name_label::label(path.as_os_str())
                );
                if let Some(c) = cleanup {
                    s.push_str(&format!(" · metadatos no retirados: {c}"));
                }
                s
            }
            Ok(Outcome::Linked(results)) => {
                let made = results.iter().filter(|r| r.is_ok()).count();
                let mut s = if made == 1 {
                    let path = results.iter().find_map(|r| r.as_ref().ok()).expect("one");
                    format!(
                        "Enlace creado: {}",
                        files_core::name_label::label(path.file_name().unwrap_or_default())
                    )
                } else {
                    format!("{made} enlaces creados")
                };
                if let Some(Err(e)) = results.iter().find(|r| r.is_err()) {
                    s.push_str(&format!(" · {} errores: {e}", results.len() - made));
                }
                s
            }
            Err(e) => format!("No se pudo completar: {e}"),
        };
        eprintln!("FILES_DELETE_DONE target={:?} result={text}", job.target);
        if is_trash(&job.target) {
            self.delete.trash_notice = Some(text);
            self.reload_trash_browser();
        } else {
            self.operation_message = Some(text);
        }
        // The folder watcher also notices; refresh at once like Nautilus.
        self.listing_sync.retry();
        self.refresh_current();
        true
    }
}

fn button(p: &mut dyn Painter, m: &mut Model, r: Rect, label: &str, action: Action, danger: bool) {
    let c = m.palette.clone();
    p.rounded_rect(r, 6 * m.radius, if danger { c.red } else { c.selection });
    p.text(
        Rect::new(r.x + 10, r.y, r.w - 20, r.h),
        label,
        12,
        if danger { c.background } else { c.accent },
        true,
        false,
    );
    m.hits.push(Hit {
        generation: m.generation,
        rect: r,
        action,
    });
}

/// Confirmation dialog, painted above every other layer.
pub fn paint(p: &mut dyn Painter, m: &mut Model, w: i32, h: i32) {
    let Some(target) = m.delete.confirm.clone() else {
        return;
    };
    m.hits.clear();
    let c = m.palette.clone();
    let cw = (w - 40).clamp(200, 560);
    let ch = 230;
    let (x, y) = ((w - cw) / 2, (h - ch) / 2);
    p.rounded_rect(
        Rect::new(x - 1, y - 1, cw + 2, ch + 2),
        10 * m.radius,
        c.red,
    );
    p.rounded_rect(Rect::new(x, y, cw, ch), 10 * m.radius, c.darker_background);
    let (title, detail): (String, Vec<String>) = match &target {
        Target::Paths(paths) => (
            if paths.len() == 1 {
                "¿Eliminar permanentemente este elemento?".into()
            } else {
                format!("¿Eliminar permanentemente {} elementos?", paths.len())
            },
            paths
                .iter()
                .take(3)
                .map(|p| {
                    files_core::name_label::label(p.file_name().unwrap_or(p.as_os_str()))
                        .into_owned()
                })
                .chain((paths.len() > 3).then(|| format!("y {} más", paths.len() - 3)))
                .collect(),
        ),
        Target::TrashItem { label, .. } => (
            "¿Eliminar permanentemente de la Papelera?".into(),
            vec![label.clone()],
        ),
        Target::TrashEmpty { count } => (
            "¿Vaciar la Papelera?".into(),
            vec![format!(
                "Se eliminarán {count} elementos de forma permanente"
            )],
        ),
        Target::TrashRestore { .. } | Target::Links { .. } => (String::new(), Vec::new()),
    };
    p.text(
        Rect::new(x + 20, y + 16, cw - 40, 30),
        &title,
        16,
        c.bright_foreground,
        true,
        false,
    );
    for (i, line) in detail.iter().enumerate() {
        p.text(
            Rect::new(x + 20, y + 54 + i as i32 * 22, cw - 40, 22),
            line,
            12,
            c.foreground,
            false,
            false,
        );
    }
    p.text(
        Rect::new(x + 20, y + ch - 82, cw - 40, 22),
        "No se puede deshacer. Las carpetas montadas dentro se conservan.",
        11,
        c.dark_foreground,
        false,
        false,
    );
    let by = y + ch - 48;
    button(
        p,
        m,
        Rect::new(x + cw - 300, by, 130, 30),
        "Cancelar · Esc",
        Action::DeleteAnswer(false),
        false,
    );
    button(
        p,
        m,
        Rect::new(x + cw - 160, by, 140, 30),
        "Eliminar · Enter",
        Action::DeleteAnswer(true),
        true,
    );
}

/// Keys while the confirmation is open: Enter confirms, Esc cancels.
pub fn event(m: &mut Model, ev: fltk::enums::Event) -> bool {
    use fltk::{
        app,
        enums::{Event, Key},
    };
    if m.delete.confirm.is_none() {
        return false;
    }
    match ev {
        Event::KeyDown | Event::Shortcut => match app::event_key() {
            Key::Enter | Key::KPEnter => m.answer_delete(true),
            Key::Escape => m.answer_delete(false),
            _ => {}
        },
        Event::Push => m.pointer_down(app::event_x(), app::event_y(), false, false, false),
        _ => {}
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_deletes_paths_and_reports() {
        let d = std::env::temp_dir().join(format!("files-delete-ui-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("t/x")).unwrap();
        std::fs::write(d.join("t/x/f"), b"1234").unwrap();
        let job = Job::start(Target::Paths(vec![d.join("t")]), None).unwrap();
        let out = job
            .result
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap()
            .unwrap();
        let Outcome::Deleted(r) = out else { panic!() };
        assert!(r.complete() && r.removed == 3 && r.bytes == 4);
        assert!(!d.join("t").exists());
        let job = Job::start(Target::TrashEmpty { count: 0 }, None).unwrap();
        assert!(
            job.result
                .recv_timeout(std::time::Duration::from_secs(10))
                .unwrap()
                .is_err()
        );
        let _ = std::fs::remove_dir_all(&d);
    }
}
