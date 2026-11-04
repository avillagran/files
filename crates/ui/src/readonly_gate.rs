//! Admission gate shared by readonly source probes and direct child supervisors.
//! Close may abandon a stuck readonly syscall only after forbidding any later
//! spawn. Helper phase never grants exit until the supervisor has reaped it.
//! Permission to exit is NOT a claim of worker ACK or interruptible filesystem I/O.
use std::{io, sync::Mutex};
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Phase {
    ReadOnly,
    Helper,
    Stopped,
}
pub(crate) struct Gate {
    pub phase: Phase,
    pub closing: bool,
}
pub(crate) fn admit_helper(gate: &Mutex<Gate>) -> io::Result<()> {
    let mut gate = gate
        .lock()
        .map_err(|_| io::Error::other("Estado de vista previa no disponible"))?;
    if gate.closing {
        return Err(io::Error::other("Vista previa cancelada"));
    }
    gate.phase = Phase::Helper;
    Ok(())
}

/// Fixture-only simulation of an unacknowledged readonly syscall. No mutex is
/// held; normal app code never sleeps here. This is NOT a real kernel-D test.
pub(crate) fn hold_fixture(role: &str) {
    if std::env::var_os("FILES_TEST_PREVIEW").as_deref() == Some(std::ffi::OsStr::new("1"))
        && std::env::var_os("FILES_TEST_READONLY_HOLD_ROLE").as_deref()
            == Some(std::ffi::OsStr::new(role))
    {
        eprintln!("FILES_READONLY_HELD role={role} simulated=true");
        std::thread::sleep(std::time::Duration::from_secs(30));
    }
}
