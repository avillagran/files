//! Opt-in lifetime for application-owned helper processes, never user-opened apps.
//! Linux PDEATHSIG follows the creating thread: supervisors must keep that worker
//! alive through reap. It also kills the child on an abrupt whole-app exit.
//! This does not reap an orphan, interrupt kernel D-state, or journal mutations.
use std::process::Command;

pub fn arm(command: &mut Command) {
    #[cfg(target_os = "linux")]
    {
        use std::{io, os::unix::process::CommandExt};
        let owner = std::process::id() as libc::pid_t;
        // No allocation/locks/I/O in the post-fork closure. Check both sides of
        // prctl to close the race where the owner died before the signal armed.
        unsafe {
            command.pre_exec(move || {
                if libc::getppid() != owner {
                    return Err(io::Error::from_raw_os_error(libc::ECHILD));
                }
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL, 0, 0, 0) != 0 {
                    return Err(io::Error::last_os_error());
                }
                if libc::getppid() != owner {
                    return Err(io::Error::from_raw_os_error(libc::ECHILD));
                }
                Ok(())
            });
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        // Native job objects / equivalent parent-death adapters remain pending.
        let _ = command;
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::os::unix::process::CommandExt;
    #[test]
    fn signal_survives_exec_and_preserves_prior_platform_setup() {
        let mut command = Command::new("/usr/bin/python3");
        command.args(["-c", "import ctypes,json,os; s=ctypes.c_int(); assert ctypes.CDLL(None).prctl(2,ctypes.byref(s),0,0,0)==0; print(json.dumps([s.value,os.getppid(),os.umask(0o077)]))"]);
        unsafe {
            command.pre_exec(|| {
                libc::umask(0o077);
                Ok(())
            });
        }
        arm(&mut command);
        let output = command.output().unwrap();
        assert!(output.status.success());
        let values: Vec<i64> = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            values,
            [libc::SIGKILL as i64, std::process::id() as i64, 0o077]
        );
    }
}
