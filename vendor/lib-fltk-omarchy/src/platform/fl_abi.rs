//! FLTK's event fields (`Fl::e_keysym`, `Fl::e_text`…) as linkable
//! statics, for `focus::press` / `focus::click_at` key and click injection.
//! They are C++ static members, so their symbol names follow the
//! compiler's C++ ABI: Itanium on Linux, macOS and MinGW, Microsoft's on
//! MSVC (64-bit `PEAD` pointers).

use std::ffi::c_char;

#[cfg(not(target_env = "msvc"))]
extern "C" {
    #[link_name = "_ZN2Fl8e_keysymE"]
    pub(crate) static mut FL_E_KEYSYM: i32;
    #[link_name = "_ZN2Fl17e_original_keysymE"]
    pub(crate) static mut FL_E_ORIGINAL_KEYSYM: i32;
    #[link_name = "_ZN2Fl7e_stateE"]
    pub(crate) static mut FL_E_STATE: i32;
    #[link_name = "_ZN2Fl6e_textE"]
    pub(crate) static mut FL_E_TEXT: *mut c_char;
    #[link_name = "_ZN2Fl8e_lengthE"]
    pub(crate) static mut FL_E_LENGTH: i32;
    #[link_name = "_ZN2Fl3e_xE"]
    pub(crate) static mut FL_E_X: i32;
    #[link_name = "_ZN2Fl3e_yE"]
    pub(crate) static mut FL_E_Y: i32;
    #[link_name = "_ZN2Fl8e_x_rootE"]
    pub(crate) static mut FL_E_X_ROOT: i32;
    #[link_name = "_ZN2Fl8e_y_rootE"]
    pub(crate) static mut FL_E_Y_ROOT: i32;
    #[link_name = "_ZN2Fl8e_clicksE"]
    pub(crate) static mut FL_E_CLICKS: i32;
}

#[cfg(target_env = "msvc")]
extern "C" {
    #[link_name = "?e_keysym@Fl@@2HA"]
    pub(crate) static mut FL_E_KEYSYM: i32;
    #[link_name = "?e_original_keysym@Fl@@2HA"]
    pub(crate) static mut FL_E_ORIGINAL_KEYSYM: i32;
    #[link_name = "?e_state@Fl@@2HA"]
    pub(crate) static mut FL_E_STATE: i32;
    #[cfg_attr(target_pointer_width = "64", link_name = "?e_text@Fl@@2PEADEA")]
    #[cfg_attr(target_pointer_width = "32", link_name = "?e_text@Fl@@2PADA")]
    pub(crate) static mut FL_E_TEXT: *mut c_char;
    #[link_name = "?e_length@Fl@@2HA"]
    pub(crate) static mut FL_E_LENGTH: i32;
    #[link_name = "?e_x@Fl@@2HA"]
    pub(crate) static mut FL_E_X: i32;
    #[link_name = "?e_y@Fl@@2HA"]
    pub(crate) static mut FL_E_Y: i32;
    #[link_name = "?e_x_root@Fl@@2HA"]
    pub(crate) static mut FL_E_X_ROOT: i32;
    #[link_name = "?e_y_root@Fl@@2HA"]
    pub(crate) static mut FL_E_Y_ROOT: i32;
    #[link_name = "?e_clicks@Fl@@2HA"]
    pub(crate) static mut FL_E_CLICKS: i32;
}

// Plain C functions of cfltk (no C++ mangling): the same name on every OS.
extern "C" {
    /// `Fl::belowmouse(w)`; null clears it (fltk-rs only sets a widget).
    pub(crate) fn Fl_set_belowmouse(w: *mut std::ffi::c_void);
}
