//! Windows: FLTK's WinAPI backend. Window gestures use FLTK's portable
//! fallback, glass stays opaque, the theme watcher polls. The appearance
//! comes from the per-user `AppsUseLightTheme` registry value.

pub(super) use super::generic::*;
use std::ffi::c_void;
use std::time::Duration;

type Hkey = *mut c_void;
const HKEY_CURRENT_USER: Hkey = 0x8000_0001_usize as Hkey;
const RRF_RT_REG_DWORD: u32 = 0x0000_0010;

#[link(name = "advapi32")]
extern "system" {
    fn RegGetValueW(
        key: Hkey,
        sub_key: *const u16,
        value: *const u16,
        flags: u32,
        kind: *mut u32,
        data: *mut c_void,
        len: *mut u32,
    ) -> i32;
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Dark when `AppsUseLightTheme` is 0; None when the value is missing
/// (Windows before 10 1809).
pub(super) fn prefers_dark() -> Option<bool> {
    let sub = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");
    let name = wide("AppsUseLightTheme");
    let mut value: u32 = 1;
    let mut len = std::mem::size_of::<u32>() as u32;
    // SAFETY: valid NUL-terminated UTF-16 strings and a DWORD-sized buffer.
    let rc = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            sub.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            (&mut value as *mut u32).cast(),
            &mut len,
        )
    };
    (rc == 0).then_some(value == 0)
}

#[repr(C)]
#[derive(Default)]
struct FileTime {
    low: u32,
    high: u32,
}

#[link(name = "kernel32")]
extern "system" {
    fn GetCurrentProcess() -> *mut c_void;
    fn GetProcessTimes(
        process: *mut c_void,
        creation: *mut FileTime,
        exit: *mut FileTime,
        kernel: *mut FileTime,
        user: *mut FileTime,
    ) -> i32;
}

/// Kernel + user time from `GetProcessTimes` (100 ns units).
pub(super) fn process_cpu_time() -> Option<Duration> {
    let (mut c, mut e, mut k, mut u) = (
        FileTime::default(),
        FileTime::default(),
        FileTime::default(),
        FileTime::default(),
    );
    // SAFETY: pseudo-handle of this process and valid out pointers.
    let ok = unsafe { GetProcessTimes(GetCurrentProcess(), &mut c, &mut e, &mut k, &mut u) };
    let ticks = |t: &FileTime| (u64::from(t.high) << 32) | u64::from(t.low);
    (ok != 0).then(|| Duration::from_nanos((ticks(&k) + ticks(&u)) * 100))
}

const SPI_GETCLIENTAREAANIMATION: u32 = 0x1042;

#[link(name = "user32")]
extern "system" {
    fn SystemParametersInfoW(action: u32, param: u32, pv: *mut c_void, win_ini: u32) -> i32;
}

/// Settings > Accessibility > Visual effects > Animation effects
/// (`SPI_GETCLIENTAREAANIMATION`, Windows 7+).
pub(super) fn animations() -> Option<bool> {
    let mut on: i32 = 1;
    // SAFETY: the action takes a BOOL out pointer.
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            (&mut on as *mut i32).cast(),
            0,
        )
    };
    (ok != 0).then_some(on != 0)
}
