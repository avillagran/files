//! Native clipboard state. UI-thread Wayland calls; filesystem/FD work in AuxIo.
//! A private handler cannot construct a gesture permit or authorize a receive.
use crate::{
    aux_io::{Request, Response},
    file_clipboard::Lease,
    model::Model,
};
use std::{
    cell::RefCell,
    path::PathBuf,
    rc::{Rc, Weak},
    sync::mpsc::TrySendError,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
struct DragEvent {
    sequence: u64,
    offer: u64,
    pane: u64,
    tab: u64,
    generation: u64,
    kind: i32,
    x: i32,
    y: i32,
}
#[derive(Default)]
pub struct State {
    enabled: bool,
    gesture: Option<u64>,
    wanted: Option<(u64, u64)>,
    next: u64,
    flight: Option<Job>,
    status: i32,
    pub notice: Option<String>,
    body: crate::painter::Rect,
    last_drag: u64,
    hover: Option<crate::native_drop::Target>,
    fixture_id: Option<u64>,
    fixture_ack: Option<bool>,
    fixture_sender_id: Option<u64>,
    origin: crate::native_drag_origin::Origin,
    drag_input: Option<(PathBuf, Arc<[std::ffi::OsString]>)>,
    drag_published: bool,
}
struct Job {
    id: u64,
    cancel: Arc<AtomicBool>,
    request: Option<Request>,
    stage: Stage,
}
enum Stage {
    Drag(crate::native_drag_origin::Started),
    Encode {
        permit: u64,
    },
    Import {
        reader: Option<Arc<crate::native_transfer_io::Reader>>,
        offer: u64,
        path: PathBuf,
        owner: (u64, u64),
        generation: u64,
        target: Option<crate::native_drop::Target>,
    },
}
#[cfg(target_os = "linux")]
mod platform {
    use std::ffi::{c_int, c_void};
    unsafe extern "C" {
        fn files_transfer_window_event(window: *mut c_void, event: c_int) -> c_int;
        fn files_transfer_init();
        fn files_transfer_register_window(window: *mut c_void) -> c_int;
        fn files_transfer_gesture(window: *mut c_void, event: c_int) -> u64;
        fn files_transfer_invalidate();
        fn files_transfer_hold_drag(permit: u64) -> c_int;
        fn files_transfer_cancel_drag();
        fn files_transfer_start_drag(permit: u64, bytes: *const u8, size: usize) -> c_int;
        fn files_transfer_outgoing_active() -> c_int;
        fn files_transfer_publish(permit: u64, bytes: *const u8, size: usize) -> c_int;
        fn files_transfer_offer(flags: *mut u32) -> u64;
        fn files_transfer_receive(permit: u64, id: u64, format: u32) -> c_int;
        fn files_transfer_receive_kde(permit: u64, id: u64, metadata_fd: *mut c_int) -> c_int;
        fn files_transfer_read_ack(id: u64, accepted: c_int) -> c_int;
        fn files_transfer_drag_context(pane: u64, tab: u64, generation: u64);
        fn files_transfer_drag_event(out: *mut super::DragEvent) -> c_int;
        fn files_transfer_receive_drop(id: u64) -> c_int;
        fn files_transfer_reject_drop(id: u64);
        fn files_transfer_pump() -> c_int;
        fn files_transfer_cancel_io();
        fn files_transfer_shutdown();
    }
    pub fn window_lost(window: &fltk::window::Window, event: fltk::enums::Event) -> bool {
        use fltk::prelude::*;
        unsafe { files_transfer_window_event(window.as_widget_ptr().cast(), event.bits()) == 1 }
    }
    pub fn init(window: &fltk::window::Window) {
        use fltk::prelude::*;
        unsafe {
            files_transfer_init();
            files_transfer_register_window(window.as_widget_ptr().cast());
        }
    }
    pub fn gesture(window: &fltk::window::Window, event: fltk::enums::Event) -> u64 {
        use fltk::prelude::*;
        unsafe { files_transfer_gesture(window.as_widget_ptr().cast(), event.bits()) }
    }
    pub fn invalidate() {
        unsafe { files_transfer_invalidate() }
    }
    pub fn hold_drag(permit: u64) -> bool {
        unsafe { files_transfer_hold_drag(permit) == 1 }
    }
    pub fn cancel_drag() {
        unsafe { files_transfer_cancel_drag() }
    }
    pub fn start_drag(permit: u64, bytes: &[u8]) -> i32 {
        unsafe { files_transfer_start_drag(permit, bytes.as_ptr(), bytes.len()) }
    }
    pub fn outgoing_active() -> bool {
        unsafe { files_transfer_outgoing_active() == 1 }
    }
    pub fn publish(permit: u64, bytes: &[u8]) -> i32 {
        unsafe { files_transfer_publish(permit, bytes.as_ptr(), bytes.len()) }
    }
    pub fn receive(
        permit: u64,
    ) -> Result<
        (
            u64,
            std::fs::File,
            Option<std::fs::File>,
            files_core::file_transfer::Format,
        ),
        String,
    > {
        use std::os::fd::FromRawFd;
        let mut flags = 0;
        let id = unsafe { files_transfer_offer(&mut flags) };
        let (flag, format, kde) = if flags & 2 != 0 {
            (
                2,
                files_core::file_transfer::Format::GnomeCopiedFiles,
                false,
            )
        } else if flags & 5 == 5 {
            (1, files_core::file_transfer::Format::UriList, true)
        } else if flags & 1 != 0 && flags & 4 == 0 {
            (1, files_core::file_transfer::Format::UriList, false)
        } else {
            return Err("Clipboard sin MIME de archivos compatible".into());
        };
        let mut metadata_fd = -1;
        let fd = unsafe {
            if kde {
                files_transfer_receive_kde(permit, id, &mut metadata_fd)
            } else {
                files_transfer_receive(permit, id, flag)
            }
        };
        if fd < 0 {
            return Err(format!(
                "Lectura nativa rechazada ({fd}); gesto/oferta no vigentes"
            ));
        }
        if kde && metadata_fd < 0 {
            // Defensive contract check: own the valid URI fd before returning.
            drop(unsafe { std::fs::File::from_raw_fd(fd) });
            unsafe {
                files_transfer_read_ack(id, 0);
            }
            return Err("Metadata KDE no disponible; ninguna operación iniciada".into());
        }
        let metadata = if kde {
            Some(unsafe { std::fs::File::from_raw_fd(metadata_fd) })
        } else {
            None
        };
        Ok((
            id,
            unsafe { std::fs::File::from_raw_fd(fd) },
            metadata,
            format,
        ))
    }
    pub fn ack(id: u64, accepted: bool) -> bool {
        unsafe { files_transfer_read_ack(id, accepted.into()) == 1 }
    }
    pub fn drag_context(owner: (u64, u64), generation: u64) {
        unsafe {
            files_transfer_drag_context(owner.0, owner.1, generation);
        }
    }
    pub(super) fn drag_event() -> Option<super::DragEvent> {
        let mut event = super::DragEvent::default();
        (unsafe { files_transfer_drag_event(&mut event) } != 0).then_some(event)
    }
    pub fn receive_drop(id: u64) -> Result<std::fs::File, String> {
        use std::os::fd::FromRawFd;
        let fd = unsafe { files_transfer_receive_drop(id) };
        if fd < 0 {
            Err(format!("Drop obsoleto/no disponible ({fd}); sin operación"))
        } else {
            Ok(unsafe { std::fs::File::from_raw_fd(fd) })
        }
    }
    pub fn reject_drop(id: u64) {
        unsafe { files_transfer_reject_drop(id) }
    }

    pub fn pump() -> i32 {
        unsafe { files_transfer_pump() }
    }
    pub fn cancel() {
        unsafe { files_transfer_cancel_io() }
    }
    pub fn shutdown() {
        unsafe { files_transfer_shutdown() }
    }
}
#[cfg(not(target_os = "linux"))]
mod platform {
    pub fn window_lost(_: &fltk::window::Window, _: fltk::enums::Event) -> bool {
        false
    }
    pub fn init(_: &fltk::window::Window) {}
    pub fn gesture(_: &fltk::window::Window, _: fltk::enums::Event) -> u64 {
        0
    }
    pub fn invalidate() {}
    pub fn hold_drag(_: u64) -> bool {
        false
    }
    pub fn cancel_drag() {}
    pub fn start_drag(_: u64, _: &[u8]) -> i32 {
        -1
    }
    pub fn outgoing_active() -> bool {
        false
    }
    pub fn publish(_: u64, _: &[u8]) -> i32 {
        -1
    }
    pub fn receive(
        _: u64,
    ) -> Result<
        (
            u64,
            std::fs::File,
            Option<std::fs::File>,
            files_core::file_transfer::Format,
        ),
        String,
    > {
        Err("MIME nativo pendiente en esta plataforma".into())
    }
    pub fn drag_context(_: (u64, u64), _: u64) {}
    pub(super) fn drag_event() -> Option<super::DragEvent> {
        None
    }
    pub fn receive_drop(_: u64) -> Result<std::fs::File, String> {
        Err("DnD nativo pendiente en esta plataforma".into())
    }
    pub fn reject_drop(_: u64) {}
    pub fn ack(_: u64, _: bool) -> bool {
        false
    }
    pub fn pump() -> i32 {
        0
    }
    pub fn cancel() {}
    pub fn shutdown() {}
}
impl State {
    pub fn enable(&mut self, window: &fltk::window::Window) {
        self.enabled = true;
        platform::init(window);
    }
    pub fn external_fds(&self) -> usize {
        // Max per phase: four bounded outgoing sends plus four transient pipe2
        // ends while opening a KDE pair = 8. Once opened only two reader FDs
        // remain; both are dropped before Source capture (parent + scratch).
        // Clipboard/queue descriptors have independent reservations.
        if self.enabled { 8 } else { 0 }
    }
    pub fn is_drained(&self) -> bool {
        self.flight.is_none()
    }
    pub fn refresh_blocked(&self) -> bool {
        self.origin.active()
            || self.hover.is_some()
            || self.wanted.is_some()
            || self.flight.is_some()
    }
    pub fn painted(&mut self, owner: (u64, u64), generation: u64, body: crate::painter::Rect) {
        self.body = body;
        if self.enabled {
            platform::drag_context(owner, generation);
        }
    }
    pub fn hover(&self) -> Option<&crate::native_drop::Target> {
        self.hover.as_ref()
    }
    pub fn fixture_ack(&self) -> Option<bool> {
        self.fixture_ack
    }

    pub fn cancel_pending(&mut self) {
        self.origin.cancel();
        self.drag_input = None;
        if self.enabled {
            platform::cancel_drag();
        }
        self.wanted = None;
        if let Some(job) = &self.flight {
            job.cancel.store(true, Ordering::Release);
            if let Stage::Import {
                reader: Some(reader),
                ..
            } = &job.stage
            {
                reader.cancel();
            }
        }
    }
    fn lost_window_focus(&mut self) {
        self.hover = None;
        self.cancel_pending();
        self.gesture = None;
    }
    pub fn begin_close(&mut self) {
        self.hover = None;
        self.cancel_pending();
        self.gesture = None;
        if self.enabled {
            platform::cancel();
        }
    }
    pub fn shutdown(&mut self) {
        if self.enabled {
            platform::shutdown();
            self.enabled = false;
        }
    }
    pub fn capture_publication(&mut self, generation: u64) {
        self.notice = None;
        self.wanted = self.gesture.take().map(|permit| (generation, permit));
        if self.enabled && self.wanted.is_none() {
            self.notice =
                Some("Buffer de files listo; sin gesto válido para publicarlo al sistema".into());
        }
    }
    fn id(&mut self) -> u64 {
        self.next = self
            .next
            .checked_add(1)
            .expect("native operation ID exhausted");
        self.next
    }
}
/// Created only by the actual canvas handler, before its Model mutable borrow.
/// Drop clears an unused permit even on an early return/reentrant callback.
pub struct EventGuard {
    model: Weak<RefCell<Model>>,
}
impl EventGuard {
    pub fn begin(
        model: &Rc<RefCell<Model>>,
        window: &fltk::window::Window,
        event: fltk::enums::Event,
    ) -> Self {
        let enabled = model.try_borrow().is_ok_and(|m| m.native_transfer.enabled);
        if enabled
            && matches!(
                event,
                fltk::enums::Event::Push
                    | fltk::enums::Event::KeyDown
                    | fltk::enums::Event::Shortcut
            )
        {
            let permit = platform::gesture(window, event);
            if let Ok(mut m) = model.try_borrow_mut() {
                m.cancel_outgoing_drag();
                m.native_transfer.cancel_pending();
                m.native_transfer.gesture = (permit != 0).then_some(permit);
            }
        }
        Self {
            model: Rc::downgrade(model),
        }
    }
}
pub fn window_event(
    model: &Rc<RefCell<Model>>,
    window: &fltk::window::Window,
    event: fltk::enums::Event,
) {
    if event == fltk::enums::Event::Released {
        // Native invalidation must precede any potentially reentrant borrow.
        platform::cancel_drag();
        if let Ok(mut m) = model.try_borrow_mut() {
            m.cancel_outgoing_drag();
        }
        return;
    }
    if !matches!(
        event,
        fltk::enums::Event::Unfocus | fltk::enums::Event::Hide
    ) {
        return;
    }
    // Invalidate the native epoch even when a reentrant callback holds Model.
    // The bridge is inert before initialization; Rust state can catch up at ACK.
    if platform::window_lost(window, event)
        && let Ok(mut m) = model.try_borrow_mut()
    {
        m.cancel_outgoing_drag();
        m.native_transfer.lost_window_focus();
    }
}
pub fn release_before_borrow(model: &Rc<RefCell<Model>>) {
    platform::cancel_drag();
    if let Ok(mut m) = model.try_borrow_mut() {
        m.cancel_outgoing_drag();
    }
}
impl Drop for EventGuard {
    fn drop(&mut self) {
        if let Some(model) = self.model.upgrade()
            && let Ok(mut m) = model.try_borrow_mut()
            && m.native_transfer.gesture.take().is_some()
        {
            platform::invalidate();
        }
    }
}
impl Model {
    fn drag_context(&self) -> crate::native_drag_origin::Context {
        crate::native_drag_origin::Context {
            owner: self.pane_owner(),
            generation: self.generation,
        }
    }
    pub fn cancel_outgoing_drag(&mut self) {
        self.native_transfer.origin.cancel();
        self.native_transfer.drag_input = None;
        if let Some(job) = &self.native_transfer.flight
            && matches!(job.stage, Stage::Drag(_))
        {
            job.cancel.store(true, Ordering::Release);
        } else if !self.native_transfer.drag_published {
            self.clipboard_state.release_external();
        }
    }
    pub fn drag_selected_hit(&self, x: i32, y: i32) -> Option<usize> {
        self.hits
            .iter()
            .rev()
            .find(|h| h.generation == self.generation && h.rect.contains(x, y))
            .and_then(|h| match h.action {
                crate::model::Action::Entry(i) => Some(i),
                crate::model::Action::PaneEntry(id, revision, i)
                    if id == self.dual.active_id && revision == self.pane_revision =>
                {
                    Some(i)
                }
                crate::model::Action::ColumnEntry(id, i)
                    if self
                        .columns
                        .columns
                        .get(self.columns.active)
                        .is_some_and(|c| c.id == id) =>
                {
                    Some(i)
                }
                _ => None,
            })
            .filter(|i| self.selection.contains(i) || self.selected == Some(*i))
    }
    /// Arm only a real press on the active selection. No files are opened here.
    pub fn arm_outgoing_drag(&mut self, x: i32, y: i32) -> bool {
        if !self.native_transfer.enabled
            || crate::native_drop::blocked(self)
            || self.native_transfer.flight.is_some()
            || !self.loaded
            || self.drag_selected_hit(x, y).is_none()
        {
            return false;
        }
        // Freeze the raw visible names at PRESS, before another batch can
        // reorder the model. All bounds are checked while still borrowed.
        let indices: Vec<_> = self
            .order
            .iter()
            .copied()
            .filter(|i| {
                self.selection.contains(i) || self.selection.is_empty() && self.selected == Some(*i)
            })
            .take(129)
            .collect();
        let options = files_core::copy_engine::CopyOptions::default();
        if indices.len() > 128
            || crate::copy_request_builder::preflight(
                &self.path,
                &self.path,
                indices.iter().map(|&i| self.entries[i].name.as_os_str()),
                &options,
            )
            .is_err()
        {
            self.operation_message = Some("Selección de arrastre fuera de límites".into());
            return false;
        }
        let payload_bound = indices.iter().try_fold(5usize, |total, &i| {
            let path = self
                .path
                .as_os_str()
                .len()
                .checked_add(1)?
                .checked_add(self.entries[i].name.len())?;
            total.checked_add(path.checked_mul(3)?.checked_add(2)?)
        });
        let Some(payload_bound) = payload_bound else {
            return false;
        };
        if payload_bound > files_core::file_transfer::MAX_WIRE {
            return false;
        }
        let reserve = 64 * 1024
            + self.path.as_os_str().len() * 2
            + indices
                .iter()
                .map(|&i| self.entries[i].name.len() * 2 + 128)
                .sum::<usize>()
            + payload_bound * 3;
        if !self.clipboard_state.reserve_external(reserve) {
            self.operation_message = Some("Presupuesto de arrastre agotado".into());
            return false;
        }
        let names = indices
            .iter()
            .map(|&i| self.entries[i].name.clone())
            .collect::<Vec<_>>()
            .into();
        let Some(permit) = self.native_transfer.gesture.take() else {
            self.clipboard_state.release_external();
            return false;
        };
        if !platform::hold_drag(permit) {
            self.clipboard_state.release_external();
            return false;
        }
        self.native_transfer
            .origin
            .press(permit, self.drag_context(), x, y);
        self.native_transfer.drag_input = Some((self.path.clone(), names));
        true
    }
    pub fn outgoing_drag_motion(&mut self, x: i32, y: i32) -> bool {
        let context = self.drag_context();
        let Some(started) = self.native_transfer.origin.motion(context, x, y) else {
            return self.native_transfer.origin.started();
        };
        let Some((parent, names)) = self.native_transfer.drag_input.take() else {
            self.cancel_outgoing_drag();
            platform::cancel_drag();
            return true;
        };
        let id = self.native_transfer.id();
        let cancel = Arc::new(AtomicBool::new(false));
        self.native_transfer.flight = Some(Job {
            id,
            cancel: cancel.clone(),
            request: Some(Request::NativeDrag(id, parent, names, cancel)),
            stage: Stage::Drag(started),
        });
        self.operation_message = Some("Preparando arrastre de copia…".into());
        true
    }
    pub fn native_paste(&mut self) {
        if self.closing
            || self.bookmarks.panel.is_some()
            || self.search.visible
            || self.notice_open
            || self.help.is_some()
            || self.recovery_catalog.open
            || self.mutations.busy
            || self.editing
            || self.rename_dialog.is_some()
            || self.copy_form.is_some()
            || self.properties_dialog.is_some()
            || self.preview.visible()
            || self.mutation_applied
        {
            return;
        }
        let Some(permit) = self
            .native_transfer
            .gesture
            .take()
            .filter(|_| self.native_transfer.enabled)
        else {
            self.operation_message=Some("Pegar del sistema requiere gesto/foco real en esta ventana; buffer interno no usado como sustituto".into());
            return;
        };
        if self.native_transfer.flight.is_some() {
            self.operation_message = Some("Espera ACK del transporte MIME anterior".into());
            return;
        }
        match platform::receive(permit) {
            Ok((offer, file, metadata, format)) => {
                let id = self.native_transfer.id();
                let cancel = Arc::new(AtomicBool::new(false));
                let reader = Arc::new(if let Some(metadata) = metadata {
                    crate::native_transfer_io::Reader::new_kde(file, metadata, cancel.clone())
                } else {
                    crate::native_transfer_io::Reader::new(file, cancel.clone())
                });
                self.native_transfer.flight = Some(Job {
                    id,
                    cancel: cancel.clone(),
                    request: Some(Request::NativeImport(id, reader.clone(), format)),
                    stage: Stage::Import {
                        reader: Some(reader),
                        offer,
                        path: self.path.clone(),
                        owner: self.pane_owner(),
                        generation: self.generation,
                        target: None,
                    },
                });
                self.operation_message =
                    Some("Leyendo MIME del sistema; sin iniciar operaciones…".into());
            }
            Err(e) => self.operation_message = Some(e),
        }
    }
    pub fn pump_native_transfer(&mut self) -> bool {
        if !self.native_transfer.enabled {
            return false;
        }
        let mut dirty = false;
        if self.closing {
            self.cancel_outgoing_drag();
            self.native_transfer.begin_close();
        }
        let status = platform::pump();
        if self.native_transfer.drag_published && !platform::outgoing_active() {
            self.native_transfer.drag_published = false;
            self.clipboard_state.release_external();
            dirty = true;
        }
        dirty |= self.pump_native_drag();
        if status != self.native_transfer.status {
            self.native_transfer.status = status;
            dirty = true;
            if status < 0 {
                self.native_transfer.notice = Some(format!(
                    "Clipboard nativo no disponible ({status}); buffer de files conservado"
                ));
            }
        }
        if let Some(job) = &self.native_transfer.flight
            && let Stage::Import {
                reader: Some(reader),
                ..
            } = &job.stage
            && reader.expire_at(std::time::Instant::now())
        {
            self.native_transfer.notice =
                Some("Lectura MIME agotó tiempo de espera; ninguna operación iniciada".into());
            dirty = true;
        }
        // Discard unsent ownership locally, but a sent job must return its ACK.
        if self
            .native_transfer
            .flight
            .as_ref()
            .is_some_and(|j| j.cancel.load(Ordering::Acquire) && j.request.is_some())
        {
            let job = self.native_transfer.flight.take().unwrap();
            if matches!(job.stage, Stage::Drag(_)) {
                self.clipboard_state.release_external();
            }
            if let Stage::Import { offer, .. } = job.stage {
                platform::ack(offer, false);
            }
            dirty = true;
        }
        if self.native_transfer.flight.is_none()
            && !self.closing
            && let Some((generation, permit)) = self.native_transfer.wanted
        {
            if self.clipboard_state.generation() == Some(generation) {
                self.native_transfer.wanted = None;
                match self.clipboard_state.lease() {
                    Ok(lease) => {
                        let id = self.native_transfer.id();
                        let cancel = Arc::new(AtomicBool::new(false));
                        self.native_transfer.flight = Some(Job {
                            id,
                            cancel: cancel.clone(),
                            request: Some(Request::NativeEncode(id, lease, cancel)),
                            stage: Stage::Encode { permit },
                        });
                    }
                    Err(e) => self.native_transfer.notice = Some(e.into()),
                }
                dirty = true;
            } else if !self.clipboard_state.busy() {
                self.native_transfer.wanted = None;
                self.native_transfer.notice =
                    Some("Captura falló; clipboard del sistema no modificado".into());
                dirty = true;
            }
        }
        if let Some(job) = self.native_transfer.flight.as_mut()
            && let Some(request) = job.request.take()
        {
            match self.aux.tx.try_send(request) {
                Ok(()) => dirty = true,
                Err(TrySendError::Full(request)) => job.request = Some(request),
                Err(TrySendError::Disconnected(request)) => {
                    drop(request);
                    let job = self.native_transfer.flight.take().unwrap();
                    if matches!(job.stage, Stage::Drag(_)) {
                        self.clipboard_state.release_external();
                    }
                    if let Stage::Import { offer, .. } = job.stage {
                        platform::ack(offer, false);
                    }
                    self.native_transfer.notice =
                        Some("AuxIo no disponible; ninguna operación MIME iniciada".into());
                    dirty = true;
                }
            }
        }
        if dirty && let Some(notice) = &self.native_transfer.notice {
            self.operation_message = Some(notice.clone());
        }
        dirty
    }
    fn pump_native_drag(&mut self) -> bool {
        let Some(event) = platform::drag_event() else {
            return false;
        };
        self.native_drag_with(event, platform::receive_drop, platform::reject_drop)
    }
    /// Existing framebuffer fixture only: a private pipe, no native offer or
    /// synthesized serial. The real AuxIo/Source/review/queue still execute.
    #[cfg(target_os = "linux")]
    pub(crate) fn fixture_drag(&mut self, file: Option<std::fs::File>) -> Result<(), String> {
        if self.native_transfer.enabled
            || std::env::var_os("FILES_TEST_ISOLATE_INPUT").as_deref() != Some("1".as_ref())
            || std::env::var_os("FILES_TEST_HIDDEN_CONTEXT").as_deref() != Some("1".as_ref())
        {
            return Err("Fixture DnD requires isolated test input".into());
        }
        let body = self.native_transfer.body;
        if body.w < 20 || body.h < 20 {
            return Err("Fixture DnD body unavailable".into());
        }
        let event = DragEvent {
            sequence: self.native_transfer.last_drag.saturating_add(1),
            offer: 77,
            pane: self.pane_owner().0,
            tab: self.pane_owner().1,
            generation: self.generation,
            kind: if file.is_some() { 2 } else { 1 },
            x: body.x + 8,
            y: body.y + body.h - 8,
        };
        let is_drop = file.is_some();
        self.native_drag_with(
            event,
            |_| file.ok_or_else(|| "Missing private pipe".into()),
            |_| {},
        );
        if !is_drop {
            return Ok(());
        }
        let job = self.native_transfer.flight.as_mut().ok_or_else(|| {
            self.operation_message
                .clone()
                .unwrap_or_else(|| "Fixture drop rejected".into())
        })?;
        let request = job.request.take().ok_or("Fixture request missing")?;
        let id = job.id;
        match self.aux.tx.try_send(request) {
            Ok(()) => {
                self.native_transfer.fixture_id = Some(id);
                self.native_transfer.fixture_ack = None;
                Ok(())
            }
            Err(error) => {
                self.native_transfer.flight = None;
                Err(format!("Fixture AuxIo unavailable: {error}"))
            }
        }
    }
    /// Isolated sender scene: actual selection proof and AuxIo encoding, with
    /// a stub final dispatch. It never enables or supplies a Wayland serial.
    #[cfg(target_os = "linux")]
    pub(crate) fn fixture_outgoing_drag(&mut self) -> Result<(), String> {
        if self.native_transfer.enabled
            || std::env::var_os("FILES_TEST_ISOLATE_INPUT").as_deref() != Some("1".as_ref())
            || std::env::var_os("FILES_TEST_HIDDEN_CONTEXT").as_deref() != Some("1".as_ref())
            || self.native_transfer.flight.is_some()
        {
            return Err("Fixture emisor requiere entrada aislada y canal libre".into());
        }
        let indices: Vec<_> = self
            .order
            .iter()
            .copied()
            .filter(|i| self.selection.contains(i))
            .take(129)
            .collect();
        if indices.is_empty() || indices.len() > 128 {
            return Err("Selecciona entre 1 y 128 elementos".into());
        }
        crate::copy_request_builder::preflight(
            &self.path,
            &self.path,
            indices.iter().map(|&i| self.entries[i].name.as_os_str()),
            &files_core::copy_engine::CopyOptions::default(),
        )
        .map_err(str::to_string)?;
        if !self.clipboard_state.reserve_external(512 * 1024) {
            return Err("Reserva fixture agotada".into());
        }
        let names = indices
            .iter()
            .map(|&i| self.entries[i].name.clone())
            .collect::<Vec<_>>()
            .into();
        let context = self.drag_context();
        self.native_transfer.origin.press(1, context, 0, 0); // Local test token, not a Wayland serial.
        let started = self.native_transfer.origin.motion(context, 8, 0).unwrap();
        let id = self.native_transfer.id();
        let cancel = Arc::new(AtomicBool::new(false));
        let request = Request::NativeDrag(id, self.path.clone(), names, cancel.clone());
        self.native_transfer.flight = Some(Job {
            id,
            cancel,
            request: None,
            stage: Stage::Drag(started),
        });
        match self.aux.tx.try_send(request) {
            Ok(()) => {
                self.native_transfer.fixture_sender_id = Some(id);
                self.operation_message =
                    Some("Preparando arrastre de copia (fixture aislada)…".into());
                Ok(())
            }
            Err(_) => {
                self.native_transfer.flight = None;
                self.cancel_outgoing_drag();
                Err("AuxIo no disponible para fixture".into())
            }
        }
    }
    /// Isolated KDE fixture: real pair of pipes and worker, simulated ACK.
    #[cfg(target_os = "linux")]
    pub(crate) fn fixture_kde_import(
        &mut self,
        uri: std::fs::File,
        metadata: std::fs::File,
    ) -> Result<(), String> {
        if self.native_transfer.enabled
            || self.native_transfer.flight.is_some()
            || std::env::var_os("FILES_TEST_ISOLATE_INPUT").as_deref() != Some("1".as_ref())
            || std::env::var_os("FILES_TEST_HIDDEN_CONTEXT").as_deref() != Some("1".as_ref())
        {
            return Err("Fixture KDE requiere entrada aislada y canal libre".into());
        }
        let id = self.native_transfer.id();
        let cancel = Arc::new(AtomicBool::new(false));
        let reader = Arc::new(crate::native_transfer_io::Reader::new_kde(
            uri,
            metadata,
            cancel.clone(),
        ));
        let request = Request::NativeImport(
            id,
            reader.clone(),
            files_core::file_transfer::Format::UriList,
        );
        self.native_transfer.flight = Some(Job {
            id,
            cancel,
            request: None,
            stage: Stage::Import {
                reader: Some(reader),
                offer: 88,
                path: self.path.clone(),
                owner: self.pane_owner(),
                generation: self.generation,
                target: None,
            },
        });
        match self.aux.tx.try_send(request) {
            Ok(()) => {
                self.native_transfer.fixture_id = Some(id);
                self.native_transfer.fixture_ack = None;
                Ok(())
            }
            Err(_) => {
                self.native_transfer.flight = None;
                Err("AuxIo no disponible para fixture KDE".into())
            }
        }
    }
    fn native_drag_with(
        &mut self,
        event: DragEvent,
        receive: impl FnOnce(u64) -> Result<std::fs::File, String>,
        reject: impl FnOnce(u64),
    ) -> bool {
        if event.sequence == self.native_transfer.last_drag {
            return false;
        }
        self.native_transfer.last_drag = event.sequence;
        self.native_transfer.hover = None;
        if event.kind == 3 {
            return true;
        }
        let resolved = if self.pane_owner() == (event.pane, event.tab) {
            crate::native_drop::resolve(
                self,
                event.x,
                event.y,
                self.native_transfer.body,
                event.generation,
            )
        } else {
            Err("El panel/pestaña cambió antes del drop")
        };
        if event.kind == 1 {
            self.native_transfer.hover = resolved.ok();
            return true;
        }
        if event.kind != 2 {
            return true;
        }
        let target = match resolved {
            Ok(target) if self.native_transfer.flight.is_none() => target,
            Ok(_) => {
                reject(event.offer);
                self.operation_message = Some(
                    "Transporte ocupado; vuelve a soltar cuando termine el ACK anterior".into(),
                );
                return true;
            }
            Err(error) => {
                reject(event.offer);
                self.operation_message = Some(error.into());
                return true;
            }
        };
        match receive(event.offer) {
            Ok(file) => {
                let id = self.native_transfer.id();
                let cancel = Arc::new(AtomicBool::new(false));
                let reader = Arc::new(crate::native_transfer_io::Reader::new(file, cancel.clone()));
                self.native_transfer.flight = Some(Job {
                    id,
                    cancel,
                    request: Some(Request::NativeImport(
                        id,
                        reader.clone(),
                        files_core::file_transfer::Format::UriList,
                    )),
                    stage: Stage::Import {
                        reader: Some(reader),
                        offer: event.offer,
                        path: target.path.clone(),
                        owner: self.pane_owner(),
                        generation: self.generation,
                        target: Some(target),
                    },
                });
                self.operation_message = Some(
                    "Leyendo archivos soltados para revisión de copia; todavía sin operación"
                        .into(),
                );
            }
            Err(error) => {
                reject(event.offer);
                self.operation_message = Some(error);
            }
        }
        true
    }
    fn native_review_admission(&self, lease: &Lease, limit: usize) -> Result<(), &'static str> {
        let recovery_bytes = self
            .clipboard_recovery
            .as_ref()
            .filter(|r| r.tag.lease.system)
            .map_or(0, |r| r.tag.lease.retained_bytes());
        if lease.retained_bytes() > limit
            || lease.retained_bytes().saturating_add(recovery_bytes) > 4 * 1024 * 1024
        {
            Err("Origen MIME excede reserva de metadata; sin operación")
        } else {
            Ok(())
        }
    }
    fn admit_native_review(
        &mut self,
        lease: Arc<Lease>,
        path: PathBuf,
        admission: Result<(), &'static str>,
    ) {
        if let Err(error) = admission {
            self.native_transfer.notice = Some(error.into());
            self.operation_message = self.native_transfer.notice.clone();
        } else {
            self.open_clipboard_lease(lease, path);
            self.native_transfer.notice = None;
            self.operation_message =
                Some("Revisa origen y destino antes de pegar archivos del sistema".into());
        }
    }
    pub fn native_response(&mut self, response: Response) {
        let id = match &response {
            Response::NativeDragPrepared(id, _)
            | Response::NativeEncoded(id, _)
            | Response::NativeImported(id, _) => *id,
            _ => return,
        };
        if !self.native_transfer.enabled && self.native_transfer.fixture_sender_id == Some(id) {
            // Only the isolated fixture sets this ID; no Wayland serial or
            // proxy is fabricated and no actual start_drag is called.
            self.native_response_using(
                response,
                |_, _| false,
                |_, _| -1,
                |_, _| 1,
                2 * 1024 * 1024,
            );
            self.native_transfer.fixture_sender_id = None;
            self.native_transfer.drag_published = false;
            self.clipboard_state.release_external();
            if self.native_transfer.notice.as_deref()
                == Some("Arrastre de copia solicitado al compositor")
            {
                self.native_transfer.notice =
                    Some("Arrastre de copia preparado (fixture aislada)".into());
                self.operation_message = self.native_transfer.notice.clone();
            }
        } else if !self.native_transfer.enabled && self.native_transfer.fixture_id == Some(id) {
            // Only fixture_drag() can create this ID, with a private pipe and
            // isolated-input mode. Never accept a native Wayland offer here.
            let mut ack = None;
            self.native_response_with(
                response,
                |_, accepted| {
                    ack = Some(accepted);
                    true
                },
                |_, _| -1,
            );
            self.native_transfer.fixture_id = None;
            self.native_transfer.fixture_ack = ack;
        } else {
            self.native_response_with(response, platform::ack, platform::publish);
        }
    }
    fn native_response_with(
        &mut self,
        response: Response,
        acknowledge: impl FnMut(u64, bool) -> bool,
        publish: impl FnMut(u64, &[u8]) -> i32,
    ) {
        self.native_response_with_limit(response, acknowledge, publish, 2 * 1024 * 1024);
    }
    fn native_response_with_limit(
        &mut self,
        response: Response,
        acknowledge: impl FnMut(u64, bool) -> bool,
        publish: impl FnMut(u64, &[u8]) -> i32,
        limit: usize,
    ) {
        self.native_response_using(response, acknowledge, publish, platform::start_drag, limit)
    }
    fn native_response_using(
        &mut self,
        response: Response,
        mut acknowledge: impl FnMut(u64, bool) -> bool,
        mut publish: impl FnMut(u64, &[u8]) -> i32,
        mut start_drag: impl FnMut(u64, &[u8]) -> i32,
        limit: usize,
    ) {
        let id = match &response {
            Response::NativeDragPrepared(id, _)
            | Response::NativeEncoded(id, _)
            | Response::NativeImported(id, _) => *id,
            _ => return,
        };
        if !self
            .native_transfer
            .flight
            .as_ref()
            .is_some_and(|j| j.id == id)
        {
            return;
        }
        let job = self.native_transfer.flight.take().unwrap();
        let active = !self.closing && !job.cancel.load(Ordering::Acquire);
        match (job.stage, response) {
            (Stage::Drag(started), Response::NativeDragPrepared(_, result)) => {
                let context = crate::native_drag_origin::Context {
                    owner: self.pane_owner(),
                    generation: self.generation,
                };
                if !active || !self.native_transfer.origin.accepts(started, context) {
                    self.clipboard_state.release_external();
                    return;
                }
                self.native_transfer.origin.cancel();
                self.native_transfer.notice = Some(match result {
                    Ok(payload) => {
                        // Keep the captured Source proof pinned through the native request.
                        let _proof = &payload.lease;
                        let sent = start_drag(started.permit, &payload.bytes);
                        if sent == 1 {
                            self.native_transfer.drag_published = true;
                            "Arrastre de copia solicitado al compositor".into()
                        } else {
                            platform::cancel_drag();
                            self.clipboard_state.release_external();
                            format!("Arrastre no iniciado ({sent}); vuelve a intentarlo")
                        }
                    }
                    Err(error) => {
                        platform::cancel_drag();
                        self.clipboard_state.release_external();
                        format!("Arrastre no iniciado: {error}")
                    }
                });
            }
            (Stage::Encode { permit }, Response::NativeEncoded(_, result)) => {
                if !active {
                    return;
                }
                self.native_transfer.notice = Some(match result {
                    Ok(bytes) => {
                        let sent = publish(permit, &bytes);
                        if sent == 1 {
                            "Publicación MIME solicitada al sistema; no es ACK de transferencia"
                                .into()
                        } else {
                            format!(
                                "Buffer de files conservado; sistema no publicado ({sent}). Vuelve a copiar con foco"
                            )
                        }
                    }
                    Err(e) => format!("Sistema no publicado: {e}"),
                });
            }
            (
                Stage::Import {
                    offer,
                    path,
                    owner,
                    generation,
                    target,
                    ..
                },
                Response::NativeImported(_, result),
            ) => {
                let target_current = target
                    .as_ref()
                    .map_or(self.path == path, |t| t.current(self));
                let result = result.map(|imported| {
                    let source = Arc::new(imported.source);
                    Arc::new(Lease {
                        system: true,
                        generation: id,
                        kind: imported.kind,
                        names: source.names().clone(),
                        source,
                    })
                });
                // Compute the COMPLETE retained lease charge exactly once,
                // before protocol finish; the same admission reaches the form.
                let admission = result
                    .as_ref()
                    .map_or(Err("Importación MIME fallida"), |lease| {
                        self.native_review_admission(lease, limit)
                    });
                let accepted = active
                    && self.pane_owner() == owner
                    && self.generation == generation
                    && target_current
                    && self.bookmarks.panel.is_none()
                    && !self.search.visible
                    && self.copy_form.is_none()
                    && self.rename_dialog.is_none()
                    && admission.is_ok();
                let current = acknowledge(offer, accepted);
                if !active
                    || !current
                    || self.pane_owner() != owner
                    || self.generation != generation
                    || !target_current
                    || self.bookmarks.panel.is_some()
                    || self.search.visible
                    || self.copy_form.is_some()
                    || self.rename_dialog.is_some()
                {
                    if !self.closing {
                        self.native_transfer.notice=Some("Lectura MIME descartada: oferta/destino/intención cambiaron; sin operación".into());
                    }
                } else {
                    match result {
                        Ok(lease) => {
                            self.admit_native_review(lease, path, admission);
                        }
                        Err(e) => {
                            self.native_transfer.notice =
                                Some(format!("No se importó ningún grupo: {e}"))
                        }
                    }
                }
            }
            _ => {}
        }
        if let Some(notice) = &self.native_transfer.notice {
            self.operation_message = Some(notice.clone());
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::{fs, sync::atomic::AtomicU64};
    struct Fixture {
        root: PathBuf,
        a: PathBuf,
        b: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let root = files_core::theme::home()
                .join(".cache/files-test")
                .join(format!(
                    "native-state-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
            let a = root.join("A");
            let b = root.join("B");
            fs::create_dir_all(&a).unwrap();
            fs::create_dir(&b).unwrap();
            fs::write(a.join("same"), b"A").unwrap();
            fs::write(b.join("same"), b"B").unwrap();
            Self { root, a, b }
        }
        fn source(&self, path: &std::path::Path) -> files_core::selection_source::Source {
            files_core::selection_source::Source::capture(
                path.to_path_buf(),
                vec![std::ffi::OsString::from("same")].into(),
                &AtomicBool::new(false),
            )
            .unwrap()
        }
        fn receiving(&self) -> Model {
            let mut m = Model::new(self.a.clone());
            m.native_transfer.flight = Some(Job {
                id: 1,
                cancel: Arc::new(AtomicBool::new(false)),
                request: None,
                stage: Stage::Import {
                    reader: None,
                    offer: 77,
                    path: self.a.clone(),
                    owner: m.pane_owner(),
                    generation: m.generation,
                    target: None,
                },
            });
            m
        }
        fn imported(&self) -> Response {
            Response::NativeImported(
                1,
                Ok(crate::native_transfer_io::Imported {
                    source: self.source(&self.b),
                    kind: crate::file_clipboard::Kind::Cut,
                }),
            )
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.root).unwrap();
        }
    }
    #[test]
    fn released_drag_drops_late_proof_without_touching_cut_or_files() {
        let f = Fixture::new();
        let mut m = Model::new(f.a.clone());
        let context = m.drag_context();
        m.native_transfer.origin.press(42, context, 0, 0);
        let started = m.native_transfer.origin.motion(context, 8, 0).unwrap();
        let payload = crate::native_transfer_io::capture_drag(
            f.a.clone(),
            vec![std::ffi::OsString::from("same")].into(),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(payload.lease.kind, crate::file_clipboard::Kind::Copy);
        assert!(payload.bytes.starts_with(b"copy\n"));
        m.native_transfer.flight = Some(Job {
            id: 14,
            cancel: Arc::new(AtomicBool::new(false)),
            request: None,
            stage: Stage::Drag(started),
        });
        m.cancel_outgoing_drag();
        m.native_response_with(
            Response::NativeDragPrepared(14, Ok(payload)),
            |_, _| panic!("receiver ACK"),
            |_, _| panic!("clipboard publish"),
        );
        assert!(m.native_transfer.flight.is_none());
        assert_eq!(fs::read(f.a.join("same")).unwrap(), b"A");
        assert!(m.operation_message.is_none());
    }
    #[test]
    fn action_releases_unstarted_reservation_but_sent_job_waits_for_ack() {
        let f = Fixture::new();
        let mut m = Model::new(f.a.clone());
        assert!(m.clipboard_state.reserve_external(1024));
        m.action(crate::model::Action::Help);
        assert!(m.clipboard_state.reserve_external(1024));
        let context = m.drag_context();
        m.native_transfer.origin.press(10, context, 0, 0);
        let started = m.native_transfer.origin.motion(context, 8, 0).unwrap();
        m.native_transfer.flight = Some(Job {
            id: 17,
            cancel: Arc::new(AtomicBool::new(false)),
            request: None,
            stage: Stage::Drag(started),
        });
        m.action(crate::model::Action::Help);
        assert!(!m.clipboard_state.reserve_external(1024));
        m.native_response_with(
            Response::NativeDragPrepared(17, Err("cancelled".into())),
            |_, _| false,
            |_, _| -1,
        );
        assert!(m.clipboard_state.reserve_external(1024));
    }
    #[test]
    fn drag_reservation_counts_desired_clipboard_before_admission() {
        let f = Fixture::new();
        let mut m = Model::new(f.a.clone());
        m.clipboard_state
            .capture(
                crate::file_clipboard::Kind::Copy,
                f.a.clone(),
                vec!["same".into()].into(),
                crate::file_clipboard::LIMIT - 4096 - 100,
            )
            .unwrap();
        assert!(!m.clipboard_state.reserve_external(101));
        assert!(m.clipboard_state.reserve_external(100));
        m.clipboard_state.release_external();
        assert!(m.clipboard_state.reserve_external(100));
    }
    #[test]
    fn native_import_opens_system_review_without_mutating_or_consuming_private_cut() {
        let f = Fixture::new();
        let mut m = f.receiving();
        m.clipboard_state
            .capture(
                crate::file_clipboard::Kind::Cut,
                f.a.clone(),
                vec![std::ffi::OsString::from("same")].into(),
                0,
            )
            .unwrap();
        let (input, _) = m.clipboard_state.take_request().unwrap();
        m.clipboard_state.receive(input.id, Ok(f.source(&f.a)));
        let mut ack = Vec::new();
        m.native_response_with(
            f.imported(),
            |id, _accepted| {
                ack.push(id);
                true
            },
            |_, _| panic!("unexpected publication"),
        );
        assert_eq!(ack, [77]);
        let form = m.copy_form.as_ref().unwrap();
        let lease = form.clipboard.clone().unwrap();
        assert!(lease.system && form.moving);
        assert_eq!(form.source, f.b);
        assert_eq!(form.destination_path(), f.a);
        assert!(lease.retained_bytes() <= 2 * 1024 * 1024);
        // Native ID1 collides numerically with private generation1; no authority.
        let outcome = crate::copy_job::Outcome {
            id: 1,
            moving: true,
            extracting: false,
            creating: false,
            archive_action: None,
            exdev: None,
            result: Ok(files_core::copy_engine::CopyReport {
                completed_count: 1,
                ..Default::default()
            }),
            progress: Default::default(),
        };
        m.clipboard_state.acknowledge(
            &crate::file_clipboard::JobTag {
                lease,
                destination: f.a.clone(),
            },
            &outcome,
        );
        assert_eq!(m.clipboard_state.count(), 1);
        assert_eq!(fs::read(f.a.join("same")).unwrap(), b"A");
        assert_eq!(fs::read(f.b.join("same")).unwrap(), b"B");
        assert!(!m.copy_busy());
    }
    #[test]
    fn offer_owner_path_cancel_and_close_changes_always_ack_and_discard_late_source() {
        let f = Fixture::new();
        for case in 0..5 {
            let mut m = f.receiving();
            match case {
                0 => {}
                1 => m.dual.active_id += 1,
                2 => m.navigate(f.b.clone()),
                3 => m.native_transfer.cancel_pending(),
                _ => m.closing = true,
            }
            let mut ack = 0;
            m.native_response_with(
                f.imported(),
                |id, _accepted| {
                    assert_eq!(id, 77);
                    ack += 1;
                    case != 0
                },
                |_, _| panic!("unexpected publication"),
            );
            assert_eq!(ack, 1);
            assert!(m.copy_form.is_none() && m.native_transfer.is_drained() && !m.copy_busy());
        }
    }
    #[test]
    fn cancelled_encode_does_not_publish_or_release_singleflight_before_ack() {
        let f = Fixture::new();
        let mut m = Model::new(f.a.clone());
        let cancel = Arc::new(AtomicBool::new(false));
        m.native_transfer.flight = Some(Job {
            id: 2,
            cancel: cancel.clone(),
            request: None,
            stage: Stage::Encode { permit: 900 },
        });
        m.native_transfer.cancel_pending();
        assert!(cancel.load(Ordering::Acquire));
        assert!(!m.native_transfer.is_drained());
        m.native_response_with(
            Response::NativeEncoded(2, Ok(b"cut\nfile:///a\r\n".to_vec())),
            |_, _| panic!("unexpected read ACK"),
            |_, _| panic!("cancelled publication"),
        );
        assert!(m.native_transfer.is_drained());
        m.native_paste();
        assert!(m.copy_form.is_none() && m.native_transfer.flight.is_none());
        assert!(m.operation_message.as_ref().unwrap().contains("gesto"));
    }
    #[test]
    fn rejected_lease_budget_keeps_visible_notice_and_never_opens_form() {
        let f = Fixture::new();
        let mut m = Model::new(f.a.clone());
        let source = Arc::new(f.source(&f.b));
        let lease = Arc::new(Lease {
            system: true,
            generation: 7,
            kind: crate::file_clipboard::Kind::Cut,
            names: source.names().clone(),
            source,
        });
        let limit = lease.source.retained_bytes();
        assert!(lease.retained_bytes() > limit);
        let admission = m.native_review_admission(&lease, limit);
        assert!(admission.is_err());
        m.admit_native_review(lease, f.a.clone(), admission);
        assert!(m.copy_form.is_none() && !m.copy_busy());
        assert!(
            m.native_transfer
                .notice
                .as_ref()
                .unwrap()
                .contains("metadata")
        );
        assert_eq!(m.operation_message, m.native_transfer.notice);
        assert_eq!(fs::read(f.b.join("same")).unwrap(), b"B");
    }
    #[test]
    fn complete_lease_budget_is_checked_before_ack_and_used_for_review() {
        let f = Fixture::new();
        let mut m = f.receiving();
        let source = f.source(&f.b);
        let source_only_limit = source.retained_bytes();
        let response = Response::NativeImported(
            1,
            Ok(crate::native_transfer_io::Imported {
                source,
                kind: crate::file_clipboard::Kind::Copy,
            }),
        );
        let mut ack = None;
        m.native_response_with_limit(
            response,
            |id, accepted| {
                ack = Some((id, accepted));
                true
            },
            |_, _| panic!("unexpected publication"),
            source_only_limit,
        );
        assert_eq!(ack, Some((77, false)));
        assert!(m.copy_form.is_none() && m.native_transfer.is_drained());
        assert!(m.operation_message.as_deref().unwrap().contains("metadata"));
        assert_eq!(fs::read(f.b.join("same")).unwrap(), b"B");
    }
    #[test]
    fn lost_window_focus_never_revives_consumed_permit_when_focus_returns() {
        let f = Fixture::new();
        let mut m = Model::new(f.a.clone());
        m.native_transfer.gesture = Some(900);
        m.native_transfer.capture_publication(1);
        assert!(m.native_transfer.gesture.is_none());
        assert_eq!(m.native_transfer.wanted, Some((1, 900)));
        let cancel = Arc::new(AtomicBool::new(false));
        m.native_transfer.flight = Some(Job {
            id: 2,
            cancel: cancel.clone(),
            request: None,
            stage: Stage::Encode { permit: 900 },
        });
        m.native_transfer.lost_window_focus();
        assert!(cancel.load(Ordering::Acquire));
        assert!(m.native_transfer.wanted.is_none());
        // Focus gain never calls gesture() or rebuilds the old permit.
        assert!(m.native_transfer.gesture.is_none());
        m.native_response_with(
            Response::NativeEncoded(2, Ok(b"copy\nfile:///a\r\n".to_vec())),
            |_, _| panic!("unexpected read ACK"),
            |_, _| panic!("publication after focus loss"),
        );
        assert!(m.native_transfer.is_drained());
        assert!(m.copy_form.is_none());
    }
    fn private_drop(m: &mut Model, source: &std::path::Path) -> u64 {
        use std::{io::Write, os::fd::FromRawFd};
        let wire = files_core::file_transfer::encode(
            files_core::file_transfer::Format::UriList,
            files_core::file_transfer::Intent::Copy,
            &[source.to_path_buf()],
        )
        .unwrap();
        let mut fds = [0; 2];
        assert_eq!(unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) }, 0);
        let reader = unsafe { std::fs::File::from_raw_fd(fds[0]) };
        let mut writer = unsafe { std::fs::File::from_raw_fd(fds[1]) };
        let flags = unsafe { libc::fcntl(fds[0], libc::F_GETFL) };
        assert!(
            flags >= 0
                && unsafe { libc::fcntl(fds[0], libc::F_SETFL, flags | libc::O_NONBLOCK) } == 0
        );
        writer.write_all(&wire).unwrap();
        drop(writer);
        m.hits.clear();
        m.native_transfer.body = crate::painter::Rect::new(0, 0, 300, 200);
        let e = DragEvent {
            sequence: 1,
            offer: 77,
            pane: m.pane_owner().0,
            tab: m.pane_owner().1,
            generation: m.generation,
            kind: 2,
            x: 10,
            y: 10,
        };
        assert!(m.native_drag_with(e, |_| Ok(reader), |_| panic!("unexpected rejection")));
        let job = m.native_transfer.flight.as_mut().unwrap();
        let id = job.id;
        let request = job.request.take().unwrap();
        m.aux.tx.send(request).unwrap();
        id
    }
    fn private_response(m: &mut Model, id: u64) -> Response {
        let end = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let response = m
                .aux
                .rx
                .recv_timeout(end.saturating_duration_since(std::time::Instant::now()))
                .unwrap();
            if matches!(&response,Response::NativeImported(i,_) if *i==id) {
                return response;
            }
        }
    }
    fn wait_loaded(m: &mut Model) {
        let end = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !m.loaded {
            m.pump();
            assert!(std::time::Instant::now() < end);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }
    #[test]
    fn private_pipe_auxio_review_queue_and_close_preserve_source() {
        let f = Fixture::new();
        fs::write(f.a.join("drag"), b"private-drag").unwrap();
        let mut m = Model::new(f.b.clone());
        m.view = crate::model::View::List;
        wait_loaded(&mut m);
        let id = private_drop(&mut m, &f.a.join("drag"));
        let response = private_response(&mut m, id);
        let mut ack = None;
        m.native_response_with(
            response,
            |offer, accepted| {
                ack = Some((offer, accepted));
                true
            },
            |_, _| panic!("unexpected publication"),
        );
        assert_eq!(ack, Some((77, true)));
        let form = m.copy_form.as_ref().unwrap();
        assert!(!form.moving && form.clipboard.as_ref().unwrap().system);
        assert_eq!(form.source, f.a);
        assert_eq!(form.destination_path(), f.b);
        assert!(!f.b.join("drag").exists());
        m.start_copy();
        let id = m.copy_id;
        let end = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !m.copy_last.as_ref().is_some_and(|o| o.id == id) || m.copy_busy() {
            m.pump();
            assert!(std::time::Instant::now() < end);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert!(m.copy_last.as_ref().unwrap().result.is_ok());
        assert_eq!(fs::read(f.b.join("drag")).unwrap(), b"private-drag");
        assert_eq!(fs::read(f.a.join("drag")).unwrap(), b"private-drag");
        m.closing = true;
        while !m.close_ready() {
            m.pump();
            assert!(std::time::Instant::now() < end);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert!(m.native_transfer.is_drained());
        drop(m);
    }
    #[test]
    fn private_pipe_stale_target_cancel_and_close_ack_without_review() {
        let f = Fixture::new();
        fs::write(f.a.join("drag"), b"source").unwrap();
        for case in 0..3 {
            let mut m = Model::new(f.b.clone());
            m.view = crate::model::View::List;
            wait_loaded(&mut m);
            let id = private_drop(&mut m, &f.a.join("drag"));
            let response = private_response(&mut m, id);
            match case {
                0 => m.pane_revision += 1,
                1 => m.native_transfer.cancel_pending(),
                _ => {
                    m.closing = true;
                    m.native_transfer.begin_close();
                }
            }
            let mut ack = None;
            m.native_response_with(
                response,
                |offer, accepted| {
                    ack = Some((offer, accepted));
                    true
                },
                |_, _| panic!("unexpected publication"),
            );
            assert_eq!(ack, Some((77, false)));
            assert!(m.copy_form.is_none() && m.native_transfer.is_drained());
            assert!(!f.b.join("drag").exists());
            assert_eq!(fs::read(f.a.join("drag")).unwrap(), b"source");
            drop(m);
        }
    }
}
