//! Linux: FLTK's Wayland backend (native gestures, glass), Hyprland's Unix
//! socket and an inotify theme watcher. On X11 (`FLTK_BACKEND=x11`, no
//! Wayland session) `fl_wl_display()` is null and every Wayland path here
//! returns "not possible", so the portable fallbacks take over.
//!
//! The Wayland parts read FLTK internals of the pinned fltk-sys 1.5.23
//! (FLTK 1.4): the `wld_window` record and the Wayland screen driver's
//! seat. The layout is checked against that version only.

use std::ffi::{c_int, c_void};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

// ------------------------------------------------------------------ Wayland

#[repr(C)]
struct WlList {
    prev: *mut c_void,
    next: *mut c_void,
}

/// Prefix of FLTK 1.4's `struct wld_window` (Fl_Wayland_Window_Driver.H).
#[repr(C)]
struct WldWindow {
    fl_win: *mut c_void,
    outputs: WlList,
    wl_surface: *mut c_void,
    frame_cb: *mut c_void,
    buffer: *mut c_void,
    xdg_surface: *mut c_void,
    kind: c_int,
    /// Union: `libdecor_frame*` when `kind == DECORATED`.
    frame: *mut c_void,
}

const DECORATED: c_int = 0;

extern "C" {
    #[link_name = "_Z13fl_wl_displayv"]
    fn fl_wl_display() -> *mut c_void;
    #[link_name = "_Z9fl_wl_xidPK9Fl_Window"]
    fn fl_wl_xid(win: *const c_void) -> *mut WldWindow;
    #[link_name = "_ZN2Fl13screen_driverEv"]
    fn fl_screen_driver() -> *mut c_void;
    #[link_name = "_ZN24Fl_Wayland_Screen_Driver10get_serialEv"]
    fn wayland_get_serial(this: *mut c_void) -> u32;
    #[link_name = "_ZN24Fl_Wayland_Screen_Driver11get_wl_seatEv"]
    fn wayland_get_wl_seat(this: *mut c_void) -> *mut c_void;
    #[link_name = "_Z18fl_wl_buffer_scaleP9Fl_Window"]
    fn fl_wl_buffer_scale(win: *mut c_void) -> c_int;
    fn fl_libdecor_frame_move(frame: *mut c_void, seat: *mut c_void, serial: u32);
    fn fl_libdecor_frame_resize(frame: *mut c_void, seat: *mut c_void, serial: u32, edge: c_int);
}

pub(super) fn wayland() -> bool {
    // SAFETY: plain FLTK query; null unless the Wayland backend is active.
    !unsafe { fl_wl_display() }.is_null()
}

pub(super) use super::generic::process_cpu_time;

pub(super) fn prefers_dark() -> Option<bool> {
    None // the Omarchy theme decides
}

pub(super) fn output_scale(win: *const c_void, shown: bool) -> Option<i32> {
    // SAFETY: FLTK calls on the UI thread; the Wayland query only runs on
    // the Wayland backend, for a shown window (its scale is the output's, or
    // the largest output's before the surface entered one).
    let scale = unsafe {
        if fl_wl_display().is_null() || !shown || win.is_null() {
            1
        } else {
            fl_wl_buffer_scale(win as *mut c_void).max(1)
        }
    };
    Some(scale)
}

pub(super) fn native_gesture(win: *const c_void, edge: Option<i32>, dry_run: bool) -> bool {
    // SAFETY: plain FLTK calls on the UI thread with a shown top-level
    // window. `fl_wl_display` is null unless the Wayland backend is active,
    // so the screen driver is the Wayland one below; during a button press
    // its seat exists.
    unsafe {
        if fl_wl_display().is_null() || win.is_null() {
            return false;
        }
        let xid = fl_wl_xid(win);
        if xid.is_null() || (*xid).kind != DECORATED || (*xid).frame.is_null() {
            return false;
        }
        let drv = fl_screen_driver();
        let seat = wayland_get_wl_seat(drv);
        let serial = wayland_get_serial(drv);
        if seat.is_null() || serial == 0 {
            return false;
        }
        if !dry_run {
            match edge {
                None => fl_libdecor_frame_move((*xid).frame, seat, serial),
                Some(e) => fl_libdecor_frame_resize((*xid).frame, seat, serial, e),
            }
        }
        true
    }
}

pub(crate) mod glass {
    use super::fl_wl_xid;
    use std::ffi::{c_int, c_void};

    const CAIRO_OPERATOR_SOURCE: c_int = 1;
    const CAIRO_FORMAT_ARGB32: c_int = 0;
    const CAIRO_SURFACE_TYPE_IMAGE: c_int = 0;

    extern "C" {
        #[link_name = "_Z13fl_wl_surfaceP10wld_window"]
        fn fl_wl_surface(xid: *mut c_void) -> *mut c_void;
        #[link_name = "_Z8fl_wl_gcv"]
        fn fl_wl_gc() -> *mut c_void;
        #[link_name = "_Z16fl_wl_compositorv"]
        fn fl_wl_compositor() -> *mut c_void;
        #[link_name = "_ZN9Fl_Window7currentEv"]
        fn fl_window_current() -> *mut c_void;

        fn cairo_save(cr: *mut c_void);
        fn cairo_restore(cr: *mut c_void);
        fn cairo_set_operator(cr: *mut c_void, op: c_int);
        fn cairo_set_source_rgba(cr: *mut c_void, r: f64, g: f64, b: f64, a: f64);
        fn cairo_get_target(cr: *mut c_void) -> *mut c_void;
        fn cairo_surface_get_type(s: *mut c_void) -> c_int;
        fn cairo_image_surface_get_format(s: *mut c_void) -> c_int;

        static wl_region_interface: c_void;
        fn wl_proxy_get_version(proxy: *mut c_void) -> u32;
        fn wl_proxy_marshal_flags(
            proxy: *mut c_void,
            opcode: u32,
            interface: *const c_void,
            version: u32,
            flags: u32,
            ...
        ) -> *mut c_void;
    }

    /// The window being drawn: its Cairo context, `Fl_Window*` and `wl_surface*`.
    pub(crate) struct Target {
        cr: *mut c_void,
        pub(crate) win: usize,
        pub(crate) surface: usize,
    }

    /// Some when the current drawing goes to a shown Wayland window buffer
    /// (ARGB32). Call inside a draw callback with the Wayland backend active.
    pub(crate) fn target() -> Option<Target> {
        if !super::wayland() {
            return None;
        }
        // SAFETY: FLTK calls on the UI thread inside a draw callback; the
        // Wayland backend is active, so these return FLTK's objects or null.
        unsafe {
            let cr = fl_wl_gc();
            if cr.is_null() {
                return None;
            }
            let t = cairo_get_target(cr);
            if t.is_null()
                || cairo_surface_get_type(t) != CAIRO_SURFACE_TYPE_IMAGE
                || cairo_image_surface_get_format(t) != CAIRO_FORMAT_ARGB32
            {
                return None; // image/offscreen surfaces are RGB24: keep them opaque
            }
            let win = fl_window_current();
            if win.is_null() {
                return None;
            }
            let surface = current_surface(win as usize)?;
            Some(Target {
                cr,
                win: win as usize,
                surface,
            })
        }
    }

    /// Following FLTK fills replace the pixels with `rgb` at alpha `a`.
    pub(crate) fn begin(t: &Target, r: f64, g: f64, b: f64, a: f64) {
        // SAFETY: `t.cr` is FLTK's live Cairo context for the window being
        // drawn; save/restore (in `end`) leave FLTK's state as it was.
        unsafe {
            cairo_save(t.cr);
            cairo_set_operator(t.cr, CAIRO_OPERATOR_SOURCE);
            cairo_set_source_rgba(t.cr, r, g, b, a);
        }
    }

    pub(crate) fn end(t: &Target) {
        // SAFETY: pairs with `begin`.
        unsafe { cairo_restore(t.cr) };
    }

    /// The `wl_surface` of the FLTK window `win` (null when hidden).
    pub(crate) fn current_surface(win: usize) -> Option<usize> {
        // SAFETY: `win` is a live Fl_Window (callers check FLTK's window list
        // or take it from Fl_Window::current()).
        unsafe {
            let xid = fl_wl_xid(win as *const c_void);
            if xid.is_null() {
                return None;
            }
            let s = fl_wl_surface(xid.cast());
            (!s.is_null()).then_some(s as usize)
        }
    }

    // Opcodes from wayland.xml (the inline wrappers of wayland-client-protocol.h).
    const WL_COMPOSITOR_CREATE_REGION: u32 = 1;
    const WL_REGION_DESTROY: u32 = 0;
    const WL_REGION_ADD: u32 = 1;
    const WL_SURFACE_SET_OPAQUE_REGION: u32 = 4;
    const WL_MARSHAL_FLAG_DESTROY: u32 = 1;

    /// `wl_surface_set_opaque_region`: the whole surface (as FLTK sets it for
    /// rectangular windows) or none. Takes effect on FLTK's next commit.
    /// `surface` must be a live `wl_surface` of this FLTK display (from
    /// [`target`] or [`current_surface`]), on the UI thread.
    pub(crate) fn set_opaque_region(surface: usize, opaque: bool) {
        let surface = surface as *mut c_void;
        let null: *mut c_void = std::ptr::null_mut();
        // SAFETY: see above; requests are built exactly as libwayland's
        // inline wrappers build them.
        unsafe {
            let version = wl_proxy_get_version(surface);
            if !opaque {
                wl_proxy_marshal_flags(
                    surface,
                    WL_SURFACE_SET_OPAQUE_REGION,
                    std::ptr::null(),
                    version,
                    0,
                    null,
                );
                return;
            }
            let compositor = fl_wl_compositor();
            if compositor.is_null() {
                return;
            }
            let iface = std::ptr::addr_of!(wl_region_interface);
            let region = wl_proxy_marshal_flags(
                compositor,
                WL_COMPOSITOR_CREATE_REGION,
                iface,
                wl_proxy_get_version(compositor),
                0,
                null,
            );
            if region.is_null() {
                return;
            }
            let rv = wl_proxy_get_version(region);
            wl_proxy_marshal_flags(
                region,
                WL_REGION_ADD,
                std::ptr::null(),
                rv,
                0,
                0 as c_int,
                0 as c_int,
                1_000_000 as c_int,
                1_000_000 as c_int,
            );
            wl_proxy_marshal_flags(
                surface,
                WL_SURFACE_SET_OPAQUE_REGION,
                std::ptr::null(),
                version,
                0,
                region,
            );
            wl_proxy_marshal_flags(
                region,
                WL_REGION_DESTROY,
                std::ptr::null(),
                rv,
                WL_MARSHAL_FLAG_DESTROY,
            );
        }
    }
}

// ----------------------------------------------------------------- Hyprland

pub(super) fn hyprland_socket() -> Option<PathBuf> {
    let sig = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE")?;
    let dir = std::env::var_os("XDG_RUNTIME_DIR")?;
    let p = PathBuf::from(dir)
        .join("hypr")
        .join(sig)
        .join(".socket.sock");
    p.exists().then_some(p)
}

pub(super) fn ipc_request(path: &Path, cmd: &str, timeout: Duration) -> Option<String> {
    use std::io::{Read, Write};
    let mut s = std::os::unix::net::UnixStream::connect(path).ok()?;
    s.set_read_timeout(Some(timeout)).ok()?;
    s.write_all(cmd.as_bytes()).ok()?;
    let mut out = String::new();
    s.read_to_string(&mut out).ok()?;
    Some(out)
}

// ------------------------------------------------------------ motion setting

/// Hyprland's `animations:enabled` (Omarchy's Animations switch), else
/// GNOME's `enable-animations` on GNOME-family desktops, else unknown.
pub(super) fn animations() -> Option<bool> {
    if let Some(sock) = hyprland_socket() {
        let out = ipc_request(
            &sock,
            "j/getoption animations:enabled",
            Duration::from_millis(300),
        )?;
        return crate::anim::parse_hyprland_option(&out);
    }
    let desktop = std::env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .to_ascii_lowercase();
    let gnome_like = ["gnome", "unity", "budgie", "pantheon", "cinnamon", "mate"]
        .iter()
        .any(|d| desktop.split(':').any(|p| p.contains(d)));
    if !gnome_like {
        return None;
    }
    // One short process on the watcher thread (startup and kicks only).
    let out = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "enable-animations"])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    match String::from_utf8_lossy(&out.stdout).trim() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// Hyprland announces reloads (`omarchy toggle animations`, config edits)
/// on its event socket; `hyprctl keyword` does not, so focus changes (the
/// user coming back from a settings panel) also trigger a re-read. The
/// thread blocks in `read`: no CPU while nothing happens.
pub(super) fn watch_animations(kick: std::sync::mpsc::Sender<()>) {
    let Some(sock) = hyprland_socket() else {
        return;
    };
    let Some(dir) = sock.parent().map(|d| d.join(".socket2.sock")) else {
        return;
    };
    let _ = std::thread::Builder::new()
        .name("omarchy-motion-events".into())
        .spawn(move || {
            use std::io::BufRead;
            let Ok(s) = std::os::unix::net::UnixStream::connect(&dir) else {
                return;
            };
            for line in std::io::BufReader::new(s).lines() {
                let Ok(line) = line else { return };
                if hyprland_event_rereads(&line) && kick.send(()).is_err() {
                    return;
                }
            }
        });
}

/// Events after which `animations:enabled` may have changed.
pub(crate) fn hyprland_event_rereads(line: &str) -> bool {
    let name = line.split(">>").next().unwrap_or("");
    matches!(name, "configreloaded" | "activewindowv2")
}

// ------------------------------------------------------------------ inotify

/// Follows `omarchy-theme-set` (it renames `next-theme` over `theme`), edits
/// of `colors.toml`/`shell.toml`, `~/.config/omarchy/shell.toml` and the
/// Glassy windows toggles. Blocks in `poll` (zero CPU while idle; it wakes
/// every 250 ms only to see `stop`) and calls `changed` debounced by 150 ms.
pub(super) fn watch_theme(stop: Arc<AtomicBool>, changed: &dyn Fn()) {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let fd = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
    if fd < 0 {
        return super::poll::watch(stop, changed);
    }
    let mask = libc::IN_CLOSE_WRITE
        | libc::IN_MOVED_TO
        | libc::IN_CREATE
        | libc::IN_DELETE
        | libc::IN_DELETE_SELF
        | libc::IN_MOVE_SELF;
    let add = |p: &Path| -> i32 {
        let c = CString::new(p.as_os_str().as_bytes()).unwrap_or_default();
        unsafe { libc::inotify_add_watch(fd, c.as_ptr(), mask) }
    };
    let state = crate::theme::omarchy_state_dir();
    let user = crate::theme::user_shell_toml();
    add(&state);
    let mut theme_path = crate::theme::theme_dir();
    let mut theme_wd = add(&theme_path);
    if let Some(dir) = user.parent() {
        add(dir);
    }
    // Omarchy's shared tooltip settings (`tooltips.json`), usually the same
    // directory as the user's shell.toml.
    let tooltips = crate::tooltip::settings_path();
    if let Some(dir) = tooltips.parent().filter(|d| Some(*d) != user.parent()) {
        add(dir);
    }
    // Omarchy's window transparency toggles (Glassy windows). The directory
    // only exists once a toggle was set, so its parents are watched too.
    let toggles = crate::glass::toggles_dir();
    let mut toggles_wd = add(&toggles);
    let toggle_parents: Vec<i32> = toggles.ancestors().skip(1).take(2).map(add).collect();
    let mut buf = [0u8; 4096];
    let mut pending: Option<Instant> = None;
    let head = std::mem::size_of::<libc::inotify_event>();
    while !stop.load(Ordering::Relaxed) {
        let mut pfd = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        let wait_ms = if pending.is_some() { 30 } else { 250 };
        let ready = unsafe { libc::poll(&mut pfd, 1, wait_ms) };
        if ready > 0 {
            loop {
                let n = unsafe { libc::read(fd, buf.as_mut_ptr().cast(), buf.len()) };
                if n <= 0 {
                    break;
                }
                let mut off = 0usize;
                while off < n as usize {
                    // SAFETY: the kernel writes whole events; `off` walks them.
                    let ev = unsafe { &*(buf.as_ptr().add(off) as *const libc::inotify_event) };
                    let name_bytes = &buf[off + head..off + head + ev.len as usize];
                    let name = String::from_utf8_lossy(name_bytes)
                        .trim_end_matches('\0')
                        .to_string();
                    let relevant = matches!(
                        name.as_str(),
                        "theme" | "theme.name" | "colors.toml" | "shell.toml" | "tooltips.json"
                    ) || ev.wd == theme_wd
                        || ev.wd == toggles_wd
                        || (toggle_parents.contains(&ev.wd)
                            && matches!(name.as_str(), "toggles" | "hypr"));
                    if relevant {
                        pending = Some(Instant::now());
                    }
                    off += head + ev.len as usize;
                }
            }
        }
        // `theme::set_theme_dir` may point somewhere else now: watch it.
        let now = crate::theme::theme_dir();
        if now != theme_path {
            unsafe { libc::inotify_rm_watch(fd, theme_wd) };
            theme_wd = add(&now);
            theme_path = now;
        }
        if let Some(t0) = pending {
            if t0.elapsed() >= Duration::from_millis(150) {
                pending = None;
                // The theme directory may have been replaced: watch the new one.
                unsafe { libc::inotify_rm_watch(fd, theme_wd) };
                theme_path = crate::theme::theme_dir();
                theme_wd = add(&theme_path);
                if toggles_wd < 0 {
                    toggles_wd = add(&toggles);
                }
                changed();
            }
        }
    }
    unsafe { libc::close(fd) };
}
