use super::*;
use crate::exdev_source::Inventory;
use std::ffi::OsStr;
/// One complete private workspace, in physical transaction order.
pub struct StagingWorkspace<'a> {
    pub inventory: &'a Inventory,
    pub parent: &'a File,
    pub name: &'a OsStr,
}
/// Historical verification/sync/ACK from this call; not source freshness or approval.
#[derive(Debug)]
pub struct StagingReceipt {
    seal: [u8; 32],
    sequence: u64,
    payload_read: u64,
}
impl StagingReceipt {
    pub fn seal(&self) -> [u8; 32] {
        self.seal
    }
    pub fn sequence(&self) -> u64 {
        self.sequence
    }
    pub fn payload_read(&self) -> u64 {
        self.payload_read
    }
}
#[derive(Clone, Copy)]
pub(in crate::directory_sync_store) enum StagingPoint {
    Verified,
    Synced,
    Reverified,
    WorkspaceAcknowledged,
    Acknowledged,
}
impl Journal<'_> {
    /// Verify ALL declared workspaces against disk, fsync, reverify, then ACK
    /// WorkspaceSynced/StagingComplete. No user-data creation or publication.
    /// Sequential checks are not a filesystem snapshot or native approval.
    pub fn confirm_staging(
        &mut self,
        scopes: &[StagingWorkspace<'_>],
        read: u64,
        cancel: &AtomicBool,
    ) -> io::Result<StagingReceipt> {
        self.confirm_staging_with(scopes, read, cancel, |_| Ok(()))
    }
    pub fn confirm_staging_controlled(
        &mut self,
        scopes: &[StagingWorkspace<'_>],
        read: u64,
        control: &crate::copy_engine::Control,
    ) -> io::Result<StagingReceipt> {
        self.confirm_staging_inner(
            scopes,
            read,
            control.cancellation_flag(),
            Some(control),
            |_| Ok(()),
        )
    }
    pub(in crate::directory_sync_store) fn confirm_staging_with(
        &mut self,
        scopes: &[StagingWorkspace<'_>],
        read: u64,
        cancel: &AtomicBool,
        hook: impl FnMut(StagingPoint) -> io::Result<()>,
    ) -> io::Result<StagingReceipt> {
        self.confirm_staging_inner(scopes, read, cancel, None, hook)
    }
    fn confirm_staging_inner(
        &mut self,
        scopes: &[StagingWorkspace<'_>],
        read: u64,
        cancel: &AtomicBool,
        control: Option<&crate::copy_engine::Control>,
        mut hook: impl FnMut(StagingPoint) -> io::Result<()>,
    ) -> io::Result<StagingReceipt> {
        let checkpoint = || {
            check_cancel(cancel)?;
            if let Some(control) = control {
                control.checkpoint()?;
            }
            Ok::<(), io::Error>(())
        };
        checkpoint()?;
        self.ensure_current()?;
        self.schema.staging_admission()?;
        if read > crate::directory_compare::MAX_READ {
            return Err(bad("Allowance staging inválido"));
        }
        let seal = self.replay.state.validate_staging(&self.schema, scopes)?;
        let mut spent = 0;
        let mut writing = false;
        let result = (|| {
            for s in scopes {
                spent +=
                    s.inventory
                        .verify_staging(s.parent, s.name, read - spent, cancel, control)?;
            }
            hook(StagingPoint::Verified)?;
            for s in scopes {
                s.inventory
                    .sync_staging(s.parent, s.name, cancel, control)?;
            }
            hook(StagingPoint::Synced)?;
            for s in scopes {
                spent +=
                    s.inventory
                        .verify_staging(s.parent, s.name, read - spent, cancel, control)?;
            }
            hook(StagingPoint::Reverified)?;
            for s in scopes {
                s.inventory.check_staging_root(s.parent, s.name, cancel)?;
            }
            self.ensure_current()?;
            checkpoint()?;
            writing = true;
            for i in 0..scopes.len() {
                checkpoint()?;
                self.append(Event::WorkspaceSynced(i as u32), cancel)?;
                hook(StagingPoint::WorkspaceAcknowledged)?;
            }
            checkpoint()?;
            self.append(Event::StagingComplete, cancel)?;
            hook(StagingPoint::Acknowledged)?;
            for s in scopes {
                s.inventory.check_staging_root(s.parent, s.name, cancel)?;
            }
            self.ensure_current()?;
            Ok(StagingReceipt {
                seal,
                sequence: self.replay.sequence,
                payload_read: spent,
            })
        })();
        if result.is_err() && writing {
            self.state = State::Uncertain;
        }
        result
    }
}
