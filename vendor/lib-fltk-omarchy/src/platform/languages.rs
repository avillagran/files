//! The OS's preferred UI languages (CoreFoundation on macOS, the Windows
//! UI-language API on Windows). Elsewhere `preferred` is empty: `i18n`
//! reads the gettext environment variables, which work on every OS.

/// Preferred languages as raw OS tags, best first; empty when the OS has
/// no API for it or the call fails.
pub fn preferred() -> Vec<String> {
    #[cfg(target_os = "macos")]
    {
        mac::preferred()
    }
    #[cfg(windows)]
    {
        win::preferred()
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        Vec::new()
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use std::ffi::{c_char, c_void, CStr};

    type CFIndex = isize;
    const UTF8: u32 = 0x0800_0100; // kCFStringEncodingUTF8

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFLocaleCopyPreferredLanguages() -> *const c_void;
        fn CFArrayGetCount(array: *const c_void) -> CFIndex;
        fn CFArrayGetValueAtIndex(array: *const c_void, index: CFIndex) -> *const c_void;
        fn CFStringGetCString(
            s: *const c_void,
            buf: *mut c_char,
            size: CFIndex,
            encoding: u32,
        ) -> u8;
        fn CFRelease(cf: *const c_void);
    }

    /// BCP 47 tags from System Settings, e.g. `["es-CL", "en-US"]`.
    pub fn preferred() -> Vec<String> {
        let mut out = Vec::new();
        // SAFETY: CoreFoundation "Copy" rule: we own the array and release
        // it once; values are borrowed from it while it is alive.
        unsafe {
            let array = CFLocaleCopyPreferredLanguages();
            if array.is_null() {
                return out;
            }
            for i in 0..CFArrayGetCount(array).max(0) {
                let s = CFArrayGetValueAtIndex(array, i);
                let mut buf = [0 as c_char; 128];
                if !s.is_null()
                    && CFStringGetCString(s, buf.as_mut_ptr(), buf.len() as CFIndex, UTF8) != 0
                {
                    let tag = CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned();
                    if !tag.is_empty() && !out.contains(&tag) {
                        out.push(tag);
                    }
                }
            }
            CFRelease(array);
        }
        out
    }
}

#[cfg(windows)]
mod win {
    const MUI_LANGUAGE_NAME: u32 = 0x8;
    const LOCALE_NAME_MAX_LENGTH: usize = 85;

    #[link(name = "kernel32")]
    extern "system" {
        fn GetUserPreferredUILanguages(
            flags: u32,
            num: *mut u32,
            buffer: *mut u16,
            len: *mut u32,
        ) -> i32;
        fn GetUserDefaultLocaleName(name: *mut u16, len: i32) -> i32;
    }

    /// UI languages (`es-CL`, `en-US`), else the user locale name.
    pub fn preferred() -> Vec<String> {
        let mut out = Vec::new();
        // SAFETY: two-call pattern; the second call writes at most `len` u16s.
        unsafe {
            let (mut num, mut len) = (0u32, 0u32);
            if GetUserPreferredUILanguages(
                MUI_LANGUAGE_NAME,
                &mut num,
                std::ptr::null_mut(),
                &mut len,
            ) != 0
                && len > 0
            {
                let mut buf = vec![0u16; len as usize];
                if GetUserPreferredUILanguages(
                    MUI_LANGUAGE_NAME,
                    &mut num,
                    buf.as_mut_ptr(),
                    &mut len,
                ) != 0
                {
                    // Double-NUL-terminated list of NUL-terminated names.
                    for name in buf.split(|c| *c == 0).filter(|s| !s.is_empty()) {
                        let tag = String::from_utf16_lossy(name);
                        if !tag.is_empty() && !out.contains(&tag) {
                            out.push(tag);
                        }
                    }
                }
            }
            if out.is_empty() {
                let mut buf = [0u16; LOCALE_NAME_MAX_LENGTH];
                let n = GetUserDefaultLocaleName(buf.as_mut_ptr(), buf.len() as i32);
                if n > 1 {
                    let tag = String::from_utf16_lossy(&buf[..(n - 1) as usize]);
                    if !tag.is_empty() {
                        out.push(tag);
                    }
                }
            }
        }
        out
    }
}
