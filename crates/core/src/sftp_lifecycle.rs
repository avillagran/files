//! Pure single-flight mount transaction. Observations are evidence supplied by a
//! worker, never mount/unmount authority or proof of provenance by themselves.
#![cfg(target_os = "linux")]
use crate::{mount_table::Mount, sftp_target::Target};
use std::{ffi::OsString, path::PathBuf};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Mounting,
    Verifying,
    Ready,
    Reconciling,
    Retained,
    Disconnecting,
    Uncertain,
    Absent,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ticket(u64);
#[derive(Debug)]
pub struct Intent {
    pub target: Target,
    pub point: PathBuf,
    pub source_marker: OsString,
}
pub struct Transaction {
    ticket: Ticket,
    intent: Intent,
    phase: Phase,
    cancelled: bool,
    child_active: bool,
    spawn_admitted: bool,
    unmount_active: bool,
    mounted: Option<Mount>,
}
#[derive(Default)]
pub struct Controller {
    sequence: u64,
    transaction: Option<Transaction>,
}
impl Controller {
    /// Caller must first reserve resources and establish mountpoint provenance.
    pub fn begin(&mut self, intent: Intent) -> Result<Ticket, &'static str> {
        if self
            .transaction
            .as_ref()
            .is_some_and(|t| t.phase != Phase::Absent || t.child_active || t.unmount_active)
        {
            return Err("Montaje anterior pendiente de reconciliar");
        }
        if !intent.point.is_absolute()
            || intent.point.as_os_str().as_encoded_bytes().contains(&0)
            || intent.point.components().any(|part| {
                !matches!(
                    part,
                    std::path::Component::RootDir | std::path::Component::Normal(_)
                )
            })
            || intent.source_marker.as_encoded_bytes().contains(&0)
            || intent.point.as_os_str().len() > 4096
            || intent.source_marker.is_empty()
            || intent.source_marker.len() > 4096
        {
            return Err("Identidad de montaje inválida");
        }
        self.sequence = self.sequence.checked_add(1).ok_or("Identidades agotadas")?;
        let ticket = Ticket(self.sequence);
        self.transaction = Some(Transaction {
            ticket,
            intent,
            phase: Phase::Mounting,
            cancelled: false,
            child_active: false,
            spawn_admitted: false,
            unmount_active: false,
            mounted: None,
        });
        Ok(ticket)
    }
    pub fn current(&self) -> Option<&Transaction> {
        self.transaction.as_ref()
    }
    fn transaction(&mut self, ticket: Ticket) -> Result<&mut Transaction, &'static str> {
        self.transaction
            .as_mut()
            .filter(|t| t.ticket == ticket)
            .ok_or("Respuesta de otra operación")
    }
    /// Must precede spawn under the worker's admission lock. Cancellation forbids it.
    pub fn admit_spawn(&mut self, ticket: Ticket) -> Result<(), &'static str> {
        let t = self.transaction(ticket)?;
        if t.phase != Phase::Mounting || t.cancelled || t.child_active {
            return Err("Spawn no admitido");
        }
        t.spawn_admitted = true;
        t.child_active = true;
        t.phase = Phase::Verifying;
        Ok(())
    }
    /// A process result is not proof of mount absence. Spawn failure uses this too.
    pub fn child_finished(&mut self, ticket: Ticket) -> Result<(), &'static str> {
        let t = self.transaction(ticket)?;
        t.child_active = false;
        // Foreground lifetime ended: even a remaining matching mount needs
        // recovery, not a claim that its serving connection is still Ready.
        t.cancelled = true;
        if t.phase != Phase::Disconnecting {
            t.phase = Phase::Reconciling;
        }
        Ok(())
    }
    pub fn cancel(&mut self, ticket: Ticket) -> Result<(), &'static str> {
        let t = self.transaction(ticket)?;
        t.cancelled = true;
        t.phase = if t.unmount_active {
            Phase::Disconnecting
        } else if t.mounted.is_some() {
            Phase::Retained
        } else {
            Phase::Reconciling
        };
        Ok(())
    }
    /// `None` means a complete readable table proved absence; read/parse failure
    /// must call observation_failed instead. Caller retains exact mount identity.
    pub fn observe(
        &mut self,
        ticket: Ticket,
        observation: Option<Mount>,
    ) -> Result<(), &'static str> {
        let t = self.transaction(ticket)?;
        match observation {
            Some(mount) => {
                if !t.spawn_admitted
                    || mount.point != t.intent.point
                    || mount.source != t.intent.source_marker
                    || !matches!(mount.filesystem.as_slice(), b"fuse.sshfs" | b"sshfs")
                    || t.mounted.as_ref().is_some_and(|old| old != &mount)
                {
                    t.phase = Phase::Uncertain;
                    return Err("Identidad de montaje distinta; no desmontar por inferencia");
                }
                t.mounted = Some(mount);
                if !t.unmount_active {
                    t.phase = if t.cancelled {
                        Phase::Retained
                    } else {
                        Phase::Ready
                    };
                }
            }
            None => {
                // A live spawning process may still mount after this observation.
                if t.unmount_active {
                    t.phase = Phase::Disconnecting;
                } else if t.child_active {
                    t.phase = Phase::Reconciling;
                } else {
                    t.mounted = None;
                    t.phase = Phase::Absent;
                }
            }
        }
        Ok(())
    }
    pub fn observation_failed(&mut self, ticket: Ticket) -> Result<(), &'static str> {
        self.transaction(ticket)?.phase = Phase::Uncertain;
        Ok(())
    }
    /// Explicit disconnect only, with a fresh exact observation. Not atomic
    /// against hostile writers; the worker must retain provenance and recheck.
    pub fn admit_disconnect(&mut self, ticket: Ticket, fresh: &Mount) -> Result<(), &'static str> {
        let t = self.transaction(ticket)?;
        if !matches!(t.phase, Phase::Ready | Phase::Retained)
            || t.unmount_active
            || t.mounted.as_ref() != Some(fresh)
        {
            return Err("Desconexión sin identidad vigente");
        }
        t.unmount_active = true;
        t.phase = Phase::Disconnecting;
        Ok(())
    }
    pub fn unmount_finished(&mut self, ticket: Ticket, success: bool) -> Result<(), &'static str> {
        let t = self.transaction(ticket)?;
        if !t.unmount_active {
            return Err("ACK de desmontaje no admitido");
        }
        t.unmount_active = false;
        t.cancelled = true;
        t.phase = if success {
            Phase::Reconciling
        } else {
            Phase::Retained
        };
        Ok(())
    }
}
impl Transaction {
    pub fn phase(&self) -> Phase {
        self.phase
    }
    pub fn intent(&self) -> &Intent {
        &self.intent
    }
    pub fn mounted(&self) -> Option<&Mount> {
        self.mounted.as_ref()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn intent() -> Intent {
        Intent {
            target: Target::parse("host", "", "2222", "/path").unwrap(),
            point: "/owned/mount".into(),
            source_marker: "files-test".into(),
        }
    }
    fn mount(id: u64) -> Mount {
        Mount {
            id,
            parent: 1,
            device: (0, 2),
            root: "/".into(),
            point: "/owned/mount".into(),
            filesystem: b"fuse.sshfs".to_vec(),
            source: "files-test".into(),
        }
    }
    #[test]
    fn cancelled_late_mount_is_retained_and_cannot_be_replaced_by_new_intent() {
        let mut c = Controller::default();
        let t = c.begin(intent()).unwrap();
        c.admit_spawn(t).unwrap();
        c.cancel(t).unwrap();
        c.observe(t, None).unwrap();
        assert!(c.begin(intent()).is_err());
        c.observe(t, Some(mount(2))).unwrap();
        assert_eq!(c.current().unwrap().phase(), Phase::Retained);
        c.child_finished(t).unwrap();
        c.observe(t, Some(mount(2))).unwrap();
        assert!(c.begin(intent()).is_err());
    }
    #[test]
    fn exit_success_never_implies_ready_or_absent_and_busy_disconnect_retains_identity() {
        let mut c = Controller::default();
        let t = c.begin(intent()).unwrap();
        c.admit_spawn(t).unwrap();
        c.child_finished(t).unwrap();
        assert_eq!(c.current().unwrap().phase(), Phase::Reconciling);
        c.observe(t, Some(mount(2))).unwrap();
        assert_eq!(c.current().unwrap().phase(), Phase::Retained);
        c.admit_disconnect(t, &mount(2)).unwrap();
        c.unmount_finished(t, false).unwrap();
        assert_eq!(c.current().unwrap().mounted(), Some(&mount(2)));
        c.observe(t, Some(mount(2))).unwrap();
        assert_eq!(c.current().unwrap().phase(), Phase::Retained);
        c.admit_disconnect(t, &mount(2)).unwrap();
        c.unmount_finished(t, true).unwrap();
        assert!(c.begin(intent()).is_err());
        c.observe(t, None).unwrap();
        let new = c.begin(intent()).unwrap();
        assert_ne!(new, t);
        assert!(c.observe(t, Some(mount(2))).is_err());
    }
    #[test]
    fn cancellation_before_admission_prevents_spawn_and_unreadable_table_never_means_absent() {
        let mut c = Controller::default();
        let t = c.begin(intent()).unwrap();
        c.cancel(t).unwrap();
        assert!(c.admit_spawn(t).is_err());
        c.observation_failed(t).unwrap();
        assert!(c.begin(intent()).is_err());
        c.observe(t, None).unwrap();
        assert!(c.begin(intent()).is_ok());
    }
    #[test]
    fn changed_mount_identity_blocks_disconnect_and_retains_original_record() {
        let mut c = Controller::default();
        let t = c.begin(intent()).unwrap();
        c.admit_spawn(t).unwrap();
        c.observe(t, Some(mount(2))).unwrap();
        assert!(c.observe(t, Some(mount(3))).is_err());
        assert!(c.admit_disconnect(t, &mount(3)).is_err());
        assert!(c.admit_disconnect(t, &mount(2)).is_err());
        assert_eq!(c.current().unwrap().mounted(), Some(&mount(2)));
    }
    #[test]
    fn unattempted_mount_is_not_adopted_even_with_matching_marker() {
        let mut c = Controller::default();
        let t = c.begin(intent()).unwrap();
        assert!(c.observe(t, Some(mount(2))).is_err());
        assert_eq!(c.current().unwrap().phase(), Phase::Uncertain);
        assert!(c.current().unwrap().mounted().is_none());
        assert!(c.admit_disconnect(t, &mount(2)).is_err());
    }
    #[test]
    fn absence_during_unmount_cannot_replace_its_pending_ack() {
        let mut c = Controller::default();
        let t = c.begin(intent()).unwrap();
        c.admit_spawn(t).unwrap();
        c.observe(t, Some(mount(2))).unwrap();
        c.admit_disconnect(t, &mount(2)).unwrap();
        c.cancel(t).unwrap();
        assert_eq!(c.current().unwrap().phase(), Phase::Disconnecting);
        c.child_finished(t).unwrap();
        c.observe(t, None).unwrap();
        assert_eq!(c.current().unwrap().phase(), Phase::Disconnecting);
        assert!(c.begin(intent()).is_err());
        c.unmount_finished(t, true).unwrap();
        assert!(c.begin(intent()).is_err());
        c.observe(t, None).unwrap();
        assert!(c.begin(intent()).is_ok());
    }
}
