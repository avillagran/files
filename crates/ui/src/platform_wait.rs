//! FLTK 1.5.23's Rust wrapper rejects poll counts >=2. Preserve the C contract.
use std::io;
unsafe extern "C" {
    #[link_name = "Fl_wait_for"]
    fn wait_raw(seconds: f64) -> f64;
}
#[derive(Debug, PartialEq)]
pub enum Outcome {
    Ready { raw: f64, errno: i32 },
    Interrupted,
}
fn classify(raw: f64, errno: i32) -> io::Result<Outcome> {
    if !raw.is_finite() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "FLTK wait returned non-finite value",
        ));
    }
    if raw >= 0.0 {
        return Ok(Outcome::Ready { raw, errno });
    }
    if errno == libc::EINTR {
        return Ok(Outcome::Interrupted);
    }
    if errno != 0 {
        Err(io::Error::from_raw_os_error(errno))
    } else {
        Err(io::Error::other(format!(
            "FLTK wait failed with raw={raw}, without errno"
        )))
    }
}
pub fn wait(seconds: f64) -> io::Result<Outcome> {
    debug_assert!(fltk::app::is_ui_thread());
    // App and display were initialized by run_since. This is the same cfltk
    // entry point already linked by fltk-rs; no registry patch or new backend.
    let raw = unsafe { wait_raw(seconds) };
    let errno = io::Error::last_os_error().raw_os_error().unwrap_or(0);
    classify(raw, errno)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn poll_counts_are_success_and_stale_errno_is_not_an_error() {
        for raw in [0.0, 1.0, 2.0, 99.0] {
            assert_eq!(
                classify(raw, libc::EAGAIN).unwrap(),
                Outcome::Ready {
                    raw,
                    errno: libc::EAGAIN
                }
            );
        }
    }
    #[test]
    fn only_negative_eintr_retries_other_failures_remain_errors() {
        assert_eq!(classify(-1.0, libc::EINTR).unwrap(), Outcome::Interrupted);
        assert_eq!(
            classify(-1.0, libc::EIO).unwrap_err().raw_os_error(),
            Some(libc::EIO)
        );
        assert!(classify(-1.0, 0).is_err());
        assert!(classify(f64::NAN, libc::EINTR).is_err());
    }
}
