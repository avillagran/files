//! Platform adapters: AT-SPI (Linux/BSD, `accesskit_unix`), UI Automation
//! (Windows, `accesskit_windows`), NSAccessibility (macOS,
//! `accesskit_macos`). Each window gets one. Activation is lazy: the
//! adapter calls back when an AT asks, and only then is a tree built.

use crate::a11y::imp::{self, Activation};
use accesskit as ak;
use fltk::widget::Widget;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::sync::Arc;

/// Collects updates instead of sending them (tests).
pub(crate) struct Recorder {
    pub log: Rc<RefCell<Vec<ak::TreeUpdate>>>,
}

pub(crate) enum Adapter {
    Recorder(Recorder),
    #[cfg(any(
        target_os = "linux",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "netbsd",
        target_os = "dragonfly"
    ))]
    Unix(accesskit_unix::Adapter),
    #[cfg(all(target_os = "windows", target_pointer_width = "64"))]
    Windows(windows::Subclass),
    #[cfg(target_os = "macos")]
    Mac(accesskit_macos::SubclassingAdapter),
}

/// Activation callbacks shared by the adapters: no tree is built here (the
/// UI thread builds it at its next loop turn); the generation tells the
/// next update to be a full tree.
pub(crate) struct Activate(Arc<Activation>);

impl ak::ActivationHandler for Activate {
    fn request_initial_tree(&mut self) -> Option<ak::TreeUpdate> {
        self.0.generation.fetch_add(1, Ordering::SeqCst);
        self.0.active.store(true, Ordering::SeqCst);
        imp::wake();
        None
    }
}

#[cfg(any(
    target_os = "linux",
    target_os = "freebsd",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "dragonfly"
))]
struct Deactivate(Arc<Activation>);

#[cfg(any(
    target_os = "linux",
    target_os = "freebsd",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "dragonfly"
))]
impl ak::DeactivationHandler for Deactivate {
    fn deactivate_accessibility(&mut self) {
        self.0.active.store(false, Ordering::SeqCst);
        imp::wake();
    }
}

/// Requests go to a queue drained on the UI thread.
struct Actions;

impl ak::ActionHandler for Actions {
    fn do_action(&mut self, request: ak::ActionRequest) {
        imp::queue_action(request);
    }
}

impl Adapter {
    #[allow(unused_variables)]
    pub(crate) fn new(win: &Widget, shared: Arc<Activation>) -> Option<Adapter> {
        #[cfg(any(
            target_os = "linux",
            target_os = "freebsd",
            target_os = "openbsd",
            target_os = "netbsd",
            target_os = "dragonfly"
        ))]
        return Some(Adapter::Unix(accesskit_unix::Adapter::new(
            Activate(shared.clone()),
            Actions,
            Deactivate(shared),
        )));
        #[cfg(all(target_os = "windows", target_pointer_width = "64"))]
        return windows::Subclass::new(win, Activate(shared)).map(Adapter::Windows);
        #[cfg(target_os = "macos")]
        {
            use fltk::prelude::WidgetExt;
            let win = win.as_window()?;
            let handle = win.raw_handle();
            if handle.is_null() {
                return None;
            }
            // SAFETY: `handle` is the live NSWindow of a shown fltk window.
            let a = unsafe {
                accesskit_macos::SubclassingAdapter::for_window(handle, Activate(shared), Actions)
            };
            return Some(Adapter::Mac(a));
        }
        #[allow(unreachable_code)]
        None
    }

    pub(crate) fn is_recorder(&self) -> bool {
        matches!(self, Adapter::Recorder(_))
    }

    pub(crate) fn update_if_active(&mut self, f: impl FnOnce() -> ak::TreeUpdate) {
        match self {
            Adapter::Recorder(r) => {
                let u = f();
                r.log.borrow_mut().push(u);
            }
            #[cfg(any(
                target_os = "linux",
                target_os = "freebsd",
                target_os = "openbsd",
                target_os = "netbsd",
                target_os = "dragonfly"
            ))]
            Adapter::Unix(a) => a.update_if_active(f),
            #[cfg(all(target_os = "windows", target_pointer_width = "64"))]
            Adapter::Windows(a) => a.update_if_active(f),
            #[cfg(target_os = "macos")]
            Adapter::Mac(a) => {
                if let Some(events) = a.update_if_active(f) {
                    events.raise();
                }
            }
        }
    }

    pub(crate) fn set_focused(&mut self, on: bool) {
        match self {
            Adapter::Recorder(_) => {}
            #[cfg(any(
                target_os = "linux",
                target_os = "freebsd",
                target_os = "openbsd",
                target_os = "netbsd",
                target_os = "dragonfly"
            ))]
            Adapter::Unix(a) => a.update_window_focus_state(on),
            // The window procedure reports WM_SETFOCUS / WM_KILLFOCUS itself.
            #[cfg(all(target_os = "windows", target_pointer_width = "64"))]
            Adapter::Windows(_) => {}
            #[cfg(target_os = "macos")]
            Adapter::Mac(a) => {
                if let Some(events) = a.update_view_focus_state(on) {
                    events.raise();
                }
            }
        }
        let _ = on;
    }

    /// Window rectangle in FLTK units and the pixels-per-unit scale.
    pub(crate) fn set_bounds(&mut self, b: (i32, i32, i32, i32), scale: f64) {
        match self {
            #[cfg(any(
                target_os = "linux",
                target_os = "freebsd",
                target_os = "openbsd",
                target_os = "netbsd",
                target_os = "dragonfly"
            ))]
            Adapter::Unix(a) => {
                let (x, y, w, h) = (
                    b.0 as f64 * scale,
                    b.1 as f64 * scale,
                    b.2 as f64 * scale,
                    b.3 as f64 * scale,
                );
                // Wayland gives no window position (0, 0): AT-SPI then reports
                // window-relative coordinates, which is what Orca needs.
                let r = ak::Rect::new(x, y, x + w, y + h);
                a.set_root_window_bounds(r, r);
            }
            _ => {}
        }
        let _ = (b, scale);
    }
}

#[cfg(all(target_os = "windows", target_pointer_width = "64"))]
mod windows {
    //! Window-procedure subclass that answers `WM_GETOBJECT` with the
    //! AccessKit UIA provider. AccessKit's own `SubclassingAdapter` must be
    //! created before the window is first shown, but fltk only creates the
    //! HWND when it shows the window, so the subclass is installed here.
    use super::*;
    use accesskit_windows::{HWND, LPARAM, LRESULT, WPARAM};
    use fltk::prelude::WidgetExt;
    use std::collections::HashMap;

    const GWLP_WNDPROC: i32 = -4;
    const WM_SETFOCUS: u32 = 0x0007;
    const WM_KILLFOCUS: u32 = 0x0008;
    const WM_GETOBJECT: u32 = 0x003D;
    const WM_NCDESTROY: u32 = 0x0082;

    #[link(name = "user32")]
    extern "system" {
        fn SetWindowLongPtrW(hwnd: HWND, index: i32, value: isize) -> isize;
        fn CallWindowProcW(
            prev: isize,
            hwnd: HWND,
            msg: u32,
            wparam: WPARAM,
            lparam: LPARAM,
        ) -> LRESULT;
    }

    struct State {
        adapter: RefCell<accesskit_windows::Adapter>,
        activation: RefCell<Activate>,
        prev: isize,
    }

    thread_local! {
        static STATES: RefCell<HashMap<isize, Rc<State>>> = RefCell::new(HashMap::new());
    }

    pub(crate) struct Subclass {
        hwnd: HWND,
        state: Rc<State>,
    }

    extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        let key = hwnd.0 as isize;
        let Some(state) = STATES.with(|s| s.borrow().get(&key).cloned()) else {
            return LRESULT(0);
        };
        match msg {
            WM_GETOBJECT => {
                if let (Ok(mut a), Ok(mut act)) = (
                    state.adapter.try_borrow_mut(),
                    state.activation.try_borrow_mut(),
                ) {
                    if let Some(r) = a.handle_wm_getobject(wparam, lparam, &mut *act) {
                        drop((a, act));
                        return r.into();
                    }
                }
            }
            WM_SETFOCUS | WM_KILLFOCUS => {
                let events = state
                    .adapter
                    .try_borrow_mut()
                    .ok()
                    .and_then(|mut a| a.update_window_focus_state(msg == WM_SETFOCUS));
                if let Some(e) = events {
                    e.raise();
                }
            }
            WM_NCDESTROY => {
                STATES.with(|s| s.borrow_mut().remove(&key));
            }
            _ => {}
        }
        // SAFETY: `prev` is the window procedure this subclass replaced.
        unsafe { CallWindowProcW(state.prev, hwnd, msg, wparam, lparam) }
    }

    impl Subclass {
        pub(crate) fn new(win: &Widget, activation: Activate) -> Option<Subclass> {
            let raw = win.as_window()?.raw_handle();
            if raw.is_null() {
                return None;
            }
            let hwnd = HWND(raw as _);
            let adapter = accesskit_windows::Adapter::new(hwnd, false, Actions);
            // SAFETY: `hwnd` is the live window of this thread; the previous
            // procedure is kept and called for every message.
            let prev = unsafe { SetWindowLongPtrW(hwnd, GWLP_WNDPROC, proc as *const () as isize) };
            if prev == 0 {
                return None;
            }
            let state = Rc::new(State {
                adapter: RefCell::new(adapter),
                activation: RefCell::new(activation),
                prev,
            });
            STATES.with(|s| s.borrow_mut().insert(hwnd.0 as isize, state.clone()));
            Some(Subclass { hwnd, state })
        }

        pub(crate) fn update_if_active(&mut self, f: impl FnOnce() -> ak::TreeUpdate) {
            let events = self
                .state
                .adapter
                .try_borrow_mut()
                .ok()
                .and_then(|mut a| a.update_if_active(f));
            if let Some(e) = events {
                e.raise();
            }
        }
    }

    impl Drop for Subclass {
        fn drop(&mut self) {
            let key = self.hwnd.0 as isize;
            if STATES.with(|s| s.borrow_mut().remove(&key)).is_some() {
                // SAFETY: restores the procedure replaced in `new` (the window
                // still exists: WM_NCDESTROY would have removed the entry).
                unsafe { SetWindowLongPtrW(self.hwnd, GWLP_WNDPROC, self.state.prev) };
            }
        }
    }
}
