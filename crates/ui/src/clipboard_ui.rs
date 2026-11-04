//! Private files clipboard routing. All filesystem work uses existing AuxIo/queue.
use crate::{
    file_clipboard::Kind,
    model::{Action, Model},
};
use std::{sync::Arc, sync::mpsc::TrySendError};
impl Model {
    pub fn capture_file_clipboard(&mut self, kind: Kind) {
        if self.closing
            || self.size_panel.is_some()
            || self.trash_browser.is_some()
            || self.sftp.form.is_some()
            || self.editing
            || self.filter_edit.active
            || self.search.visible
            || self.bookmarks.panel.is_some()
            || self.commands.is_some()
            || self.size_panel.is_some()
            || self.trash_browser.is_some()
            || self.sftp.form.is_some()
            || self.rename_dialog.is_some()
            || self.copy_form.is_some()
            || self.mutation_applied
            || self.properties_dialog.is_some()
            || self.preview.visible()
        {
            return;
        }
        self.sanitize_selection();
        let indices = self
            .order
            .iter()
            .copied()
            .filter(|i| {
                self.selection.contains(i) || self.selection.is_empty() && self.selected == Some(*i)
            })
            .take(129)
            .collect::<Vec<_>>();
        if indices.is_empty() || indices.len() > 128 {
            self.operation_message =
                Some("Portapapeles de files: selecciona1..128 elementos visibles".into());
            return;
        }
        let options = files_core::copy_engine::CopyOptions::default();
        if let Err(e) = crate::copy_request_builder::preflight(
            &self.path,
            &self.path,
            indices.iter().map(|&i| self.entries[i].name.as_os_str()),
            &options,
        ) {
            self.operation_message = Some(e.into());
            return;
        }
        let reserve = 64 * 1024
            + self.path.as_os_str().len() * 2
            + indices
                .iter()
                .map(|&i| self.entries[i].name.len() * 2 + 128)
                .sum::<usize>();
        if !self.clipboard_state.can_capture(reserve) {
            self.operation_message =
                Some("Portapapeles de files: presupuesto agotado; espera la cola".into());
            return;
        }
        let names = indices
            .iter()
            .map(|&i| self.entries[i].name.clone())
            .collect::<Vec<_>>()
            .into();
        if let Err(e) = self
            .clipboard_state
            .capture(kind, self.path.clone(), names, reserve)
        {
            self.operation_message = Some(e.into());
            return;
        }
        self.native_transfer
            .capture_publication(self.clipboard_state.intent_generation());
        self.settings = false;
        self.actions_menu = false;
        self.operation_message =
            Some("Capturando selección · Portapapeles de files (interno a esta ventana)".into());
    }
    pub fn pump_clipboard(&mut self) -> bool {
        if self.closing {
            self.clipboard_state.begin_close();
        } else {
            self.clipboard_state.reopen();
        }
        self.copy.set_external_fds(
            self.clipboard_state.external_fds()
                + self.native_transfer.external_fds()
                + self.search_transfer_fds(),
        );
        let Some((input, cancel)) = self.clipboard_state.take_request() else {
            return false;
        };
        match self
            .aux
            .tx
            .try_send(crate::aux_io::Request::SelectionCapture(input, cancel))
        {
            Ok(()) => true,
            Err(TrySendError::Full(crate::aux_io::Request::SelectionCapture(input, _))) => {
                self.clipboard_state.unsent(input);
                false
            }
            Err(TrySendError::Disconnected(crate::aux_io::Request::SelectionCapture(input, _))) => {
                self.clipboard_state.failed_send(
                    input.id,
                    "Trabajador I/O no disponible; buffer anterior conservado".into(),
                );
                self.operation_message = Some(self.clipboard_state.error.clone());
                true
            }
            Err(_) => unreachable!("selection request returned intact"),
        }
    }
    pub fn paste_file_clipboard(&mut self) {
        if self.notice_open || self.help.is_some() || self.recovery_catalog.open {
            return;
        }
        if self.closing
            || self.size_panel.is_some()
            || self.trash_browser.is_some()
            || self.sftp.form.is_some()
            || self.editing
            || self.filter_edit.active
            || self.search.visible
            || self.bookmarks.panel.is_some()
            || self.commands.is_some()
            || self.size_panel.is_some()
            || self.trash_browser.is_some()
            || self.sftp.form.is_some()
            || self.rename_dialog.is_some()
            || self.copy_form.is_some()
            || self.mutation_applied
            || self.properties_dialog.is_some()
            || self.preview.visible()
        {
            return;
        }
        let lease = match self.clipboard_state.lease() {
            Ok(l) => l,
            Err(e) => {
                self.operation_message = Some(e.into());
                return;
            }
        };
        let destination = self.path.clone();
        self.open_clipboard_lease(lease, destination);
    }
    pub fn open_clipboard_lease(
        &mut self,
        lease: Arc<crate::file_clipboard::Lease>,
        destination: std::path::PathBuf,
    ) {
        self.settings = false;
        self.actions_menu = false;
        self.notice_open = false;
        let mut form = crate::copy_dialog::Dialog {
            editor: Default::default(),
            name_field: crate::copy_name_field::Field::new(lease.names.clone()),
            name_focus: false,
            clipboard: Some(lease.clone()),
            multi: None,
            moving: lease.kind == Kind::Cut,
            extracting: false,
            creating: false,
            archive: None,
            preparing: false,
            prepare_id: None,
            prepared: None,
            source: lease.source.parent().to_path_buf(),
            names: lease.names.clone(),
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
        self.copy_form = Some(form);
        self.hits.clear();
    }
    pub fn clipboard_action(&mut self, action: Action) {
        match action {
            Action::ClipboardCopy => self.capture_file_clipboard(Kind::Copy),
            Action::ClipboardCut => self.capture_file_clipboard(Kind::Cut),
            Action::ClipboardPaste => self.paste_file_clipboard(),
            _ => {}
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextTarget {
    Comparison(u64, usize, u64),
    Sftp(u64),
    Bookmarks(u64),
    SavedSearches(u64),
    Search(u64),
    Commands(u64),
    Omnibar(u64),
    Filter(u64),
    Rename(u64, usize),
    Destination(u64),
    DestinationName(u64),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextKey {
    owner: (u64, u64),
    generation: u64,
    target: TextTarget,
}
impl Model {
    pub fn text_context(&self) -> Option<TextKey> {
        if self.closing
            || self.size_panel.is_some()
            || self.trash_browser.is_some()
            || self.notice_open
            || self.preview.visible()
            || self.mutation_applied
            || self.properties_dialog.is_some()
        {
            return None;
        }
        let target = if let Some(p) = &self.comparison {
            let (stamp, side, revision) = p.text_key()?;
            TextTarget::Comparison(stamp, side, revision)
        } else if let Some(f) = &self.sftp.form {
            if f.report.is_some() {
                return None;
            }
            TextTarget::Sftp(f.edit_epoch)
        } else if let Some(p) = &self.saved_searches.panel {
            if !p.editing() || !p.current(self) {
                return None;
            }
            TextTarget::SavedSearches(p.edit_epoch)
        } else if let Some(p) = &self.bookmarks.panel {
            if !p.input {
                return None;
            }
            TextTarget::Bookmarks(p.edit_epoch)
        } else if let Some(s) = &self.commands {
            TextTarget::Commands(s.epoch)
        } else if let Some(d) = &self.rename_dialog {
            if self.mutations.busy {
                return None;
            }
            TextTarget::Rename(d.id, d.focus)
        } else if let Some(d) = &self.copy_form {
            if d.preparing || d.options_open {
                return None;
            }
            if d.name_focus && !d.moving {
                TextTarget::DestinationName(d.name_field.editor.epoch)
            } else {
                TextTarget::Destination(d.editor.epoch)
            }
        } else if self.search.visible {
            if self.search.calendar_open {
                return None;
            }
            TextTarget::Search(self.search.input_revision())
        } else if self.filter_edit.active
            && self.filter_edit.current(self)
            && self.context_menu.is_none()
        {
            TextTarget::Filter(self.filter_edit.epoch)
        } else if self.editing {
            TextTarget::Omnibar(self.path_edit.cursor.revision)
        } else {
            return None;
        };
        Some(TextKey {
            owner: self.pane_owner(),
            generation: self.generation,
            target,
        })
    }
    pub fn receive_text_paste(&mut self, text: &str) -> bool {
        let context = self.text_context();
        if !self.text_paste.receive(&context) {
            return false;
        }
        match context.map(|c| c.target) {
            Some(TextTarget::Comparison(..)) => self.comparison_input(text),
            Some(TextTarget::Sftp(_)) => self.sftp_input(text),
            Some(TextTarget::SavedSearches(_)) => self.saved_search_input(text),
            Some(TextTarget::Bookmarks(_)) => self.bookmark_input(text),
            Some(TextTarget::Commands(_)) => {
                if let Some(s) = &mut self.commands {
                    s.input(text);
                }
            }
            Some(TextTarget::Search(_)) => self.search.input(text),
            Some(TextTarget::Omnibar(_)) => crate::input(self, text),
            Some(TextTarget::Filter(_)) => {
                if let Err(e) = self.filter_input(text) {
                    self.error = e.into();
                }
            }
            Some(TextTarget::Rename(_, _)) => {
                if let Some(d) = &mut self.rename_dialog {
                    d.input(text);
                }
            }
            Some(TextTarget::DestinationName(_)) => {
                if let Some(d) = &mut self.copy_form {
                    if let Err(e) = d.name_field.input(text) {
                        d.error = e.into();
                    } else {
                        d.error.clear();
                    }
                }
            }
            Some(TextTarget::Destination(_)) => {
                if let Some(d) = &mut self.copy_form {
                    d.input(text);
                }
            }
            None => return false,
        }
        true
    }
}
pub fn request_paste(model: &std::rc::Rc<std::cell::RefCell<Model>>, frame: &fltk::frame::Frame) {
    use fltk::{app, prelude::*};
    let context = {
        let Ok(m) = model.try_borrow() else {
            return;
        };
        m.text_context()
    };
    if context.is_none() {
        if let Ok(mut m) = model.try_borrow_mut() {
            let system = m.native_transfer.enabled
                && crate::native_transfer::prefer_system_paste(m.clipboard_state.count());
            eprintln_paste(system);
            m.action(if system {
                Action::NativePaste
            } else {
                Action::ClipboardPaste
            });
        }
        let mut f = frame.clone();
        f.redraw();
        return;
    }
    // Backend query and paste both occur WITHOUT Model RefMut: clipboard paste
    // is reentrant/synchronous on the pinned Wayland implementation.
    let compatible = text_available();
    let receipt = {
        let Ok(mut m) = model.try_borrow_mut() else {
            return;
        };
        if m.text_context() != context {
            return;
        }
        let r = m.text_paste.request_if_compatible(context, compatible);
        if r.is_none() {
            m.operation_message = Some(
                if compatible {
                    "Pegado de texto previo pendiente; espera su respuesta"
                } else {
                    "Clipboard del sistema sin texto compatible"
                }
                .into(),
            );
        }
        r
    };
    if let Some(receipt) = receipt {
        app::paste_text(frame);
        let contract = completion_contract();
        if let Ok(mut m) = model.try_borrow_mut() {
            m.text_paste.returned(receipt, contract);
        }
    }
    let mut f = frame.clone();
    f.redraw();
}
fn completion_contract() -> crate::text_paste_gate::CompletionContract {
    #[cfg(target_os = "linux")]
    {
        unsafe extern "C" {
            fn Fl_using_wayland() -> std::ffi::c_int;
        }
        if unsafe { Fl_using_wayland() } != 0 {
            return crate::text_paste_gate::CompletionContract::PinnedWaylandSynchronous;
        }
    }
    crate::text_paste_gate::CompletionContract::MayBeAsynchronous
}

/// Canonical MIME constant on the pinned Linux adapter. This is a compatibility
/// query only, never a MIME file transport or a focus/serial workaround.
pub fn text_available() -> bool {
    #[cfg(target_os = "linux")]
    {
        unsafe extern "C" {
            fn files_clipboard_has_text() -> std::ffi::c_int;
        }
        unsafe { files_clipboard_has_text() != 0 }
    }
    #[cfg(not(target_os = "linux"))]
    fltk::app::clipboard_contains(fltk::app::ClipboardContent::Text)
}

fn eprintln_paste(system: bool) {
    if std::env::var_os("FILES_PASTE_TRACE").is_some() {
        eprintln!(
            "FILES_PASTE_SOURCE {}",
            if system { "system" } else { "files" }
        );
    }
}
