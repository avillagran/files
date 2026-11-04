use std::{path::Path, time::SystemTime};
pub fn pss_kib() -> Option<u64> {
    std::fs::read_to_string("/proc/self/smaps_rollup")
        .ok()?
        .lines()
        .find_map(|l| {
            l.strip_prefix("Pss:")?
                .split_whitespace()
                .next()?
                .parse()
                .ok()
        })
}
// Los tipos de statvfs varían entre plataformas de 32 y 64 bits.
#[allow(clippy::unnecessary_cast)]
#[cfg(unix)]
pub fn free_space(path: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let p = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut s = std::mem::MaybeUninit::<libc::statvfs>::uninit(); // statvfs inicializa la estructura sólo al retornar 0.
    if unsafe { libc::statvfs(p.as_ptr(), s.as_mut_ptr()) } != 0 {
        return None;
    }
    let s = unsafe { s.assume_init() };
    Some(s.f_bavail as u64 * s.f_frsize as u64)
}
#[cfg(not(unix))]
pub fn free_space(_: &Path) -> Option<u64> {
    None
}
#[cfg(unix)]
pub fn date_group(time: SystemTime, now: SystemTime) -> usize {
    fn day(t: SystemTime) -> i64 {
        let s = t
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as libc::time_t;
        let mut tm = std::mem::MaybeUninit::<libc::tm>::uninit();
        if unsafe { libc::localtime_r(&s, tm.as_mut_ptr()) }.is_null() {
            return s as i64 / 86400;
        }
        let tm = unsafe { tm.assume_init() };
        (s as i64 + tm.tm_gmtoff) / 86400
    }
    let d = day(now) - day(time);
    if d <= 0 {
        0
    } else if d < 7 {
        1
    } else {
        2
    }
}
#[cfg(not(unix))]
pub fn date_group(time: SystemTime, now: SystemTime) -> usize {
    let d = now.duration_since(time).unwrap_or_default().as_secs() / 86400;
    if d == 0 {
        0
    } else if d < 7 {
        1
    } else {
        2
    }
}
