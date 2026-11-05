//! Pieces shared by every OS without Wayland or Hyprland (macOS, Windows,
//! other Unixes): no compositor gestures (the window falls back to FLTK's
//! own move/resize), opaque glass, no Hyprland, a polling theme watcher.

#![cfg_attr(target_os = "linux", allow(dead_code))]

use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

pub(crate) fn wayland() -> bool {
    false
}

pub(crate) fn native_gesture(_win: *const c_void, _edge: Option<i32>, _dry_run: bool) -> bool {
    false
}

pub(crate) fn output_scale(_win: *const c_void, _shown: bool) -> Option<i32> {
    None
}

pub(crate) fn hyprland_socket() -> Option<PathBuf> {
    None
}

pub(crate) fn ipc_request(_path: &Path, _cmd: &str, _timeout: Duration) -> Option<String> {
    None
}

/// `CLOCK_PROCESS_CPUTIME_ID` (Linux, macOS ≥ 10.12, BSDs).
#[cfg(unix)]
pub(crate) fn process_cpu_time() -> Option<Duration> {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: valid out pointer.
    let rc = unsafe { libc::clock_gettime(libc::CLOCK_PROCESS_CPUTIME_ID, &mut ts) };
    (rc == 0).then(|| Duration::new(ts.tv_sec as u64, ts.tv_nsec as u32))
}

#[cfg(not(any(unix, windows)))]
pub(crate) fn process_cpu_time() -> Option<Duration> {
    None
}

pub(crate) fn watch_theme(stop: Arc<AtomicBool>, changed: &dyn Fn()) {
    super::poll::watch(stop, changed)
}

/// No motion setting known: animations stay on (other Unixes; macOS and
/// Windows read theirs).
#[allow(dead_code)]
pub(crate) fn animations() -> Option<bool> {
    None
}

/// No change notifications: the setting is read again when an animation
/// starts (at most every 2 s, see `anim::kick`).
pub(crate) fn watch_animations(_kick: std::sync::mpsc::Sender<()>) {}

/// Glass stays opaque: [`target`] is always None.
pub(crate) mod glass {
    pub(crate) struct Target {
        pub(crate) win: usize,
        pub(crate) surface: usize,
    }

    pub(crate) fn target() -> Option<Target> {
        None
    }

    pub(crate) fn begin(_t: &Target, _r: f64, _g: f64, _b: f64, _a: f64) {}

    pub(crate) fn end(_t: &Target) {}

    pub(crate) fn current_surface(_win: usize) -> Option<usize> {
        None
    }

    pub(crate) fn set_opaque_region(_surface: usize, _opaque: bool) {}
}
