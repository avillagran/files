use super::*;
use crate::{
    directory_sync_journal::NamespaceAction,
    exdev_source::{
        NamespaceControl, NamespaceFailure, NamespaceLog, NamespaceMove, OwnPoint, RenameChange,
    },
};
/// Positive historical fact of this API's syscall and journal acknowledgement,
/// not current source freshness, human approval or authority for another effect.
#[derive(Debug)]
pub struct OwnNamespaceRename {
    change: RenameChange,
    seal: [u8; 32],
    sequence: u64,
    transaction: u32,
    action: NamespaceAction,
}
impl OwnNamespaceRename {
    pub fn change(&self) -> &RenameChange {
        &self.change
    }
    pub fn intent_seal(&self) -> [u8; 32] {
        self.seal
    }
    pub fn sequence(&self) -> u64 {
        self.sequence
    }
    pub fn transaction(&self) -> u32 {
        self.transaction
    }
    pub fn action(&self) -> NamespaceAction {
        self.action
    }
}
fn before(cause: io::Error) -> NamespaceFailure {
    NamespaceFailure {
        cause,
        intent_recorded: false,
        syscall_attempted: false,
        syscall_succeeded: false,
    }
}
struct Log<'j, 's, 'c, F> {
    journal: &'j mut Journal<'s>,
    transaction: u32,
    action: NamespaceAction,
    scope: [u8; 32],
    cancel: &'c AtomicBool,
    hook: F,
}
impl<F: FnMut(OwnPoint) -> io::Result<()>> NamespaceLog for Log<'_, '_, '_, F> {
    fn intent(&mut self, seal: [u8; 32]) -> io::Result<()> {
        let hook = &mut self.hook;
        self.journal.append_with(
            Event::NamespaceIntent {
                transaction: self.transaction,
                action: self.action,
                seal,
            },
            self.cancel,
            |barrier| match barrier {
                Barrier::Write => hook(OwnPoint::IntentWritten),
                Barrier::FileSynced => hook(OwnPoint::IntentFileSynced),
                Barrier::Acknowledged => hook(OwnPoint::IntentAcknowledged),
                _ => Ok(()),
            },
        )
    }
    fn ready(&mut self, seal: [u8; 32]) -> io::Result<()> {
        self.journal.ensure_current()?;
        if self.journal.replay.state.namespace_guard()
            != Some((self.transaction, self.action, seal))
        {
            return Err(bad("Namespace intent no corresponde al syscall"));
        }
        Ok(())
    }
    fn confirmed(&mut self, change: &RenameChange) -> io::Result<()> {
        let observed = change.observation();
        let transaction = self.transaction;
        match self.action {
            NamespaceAction::Preserve => {
                self.journal.append(
                    Event::OldPreserved {
                        transaction,
                        observed,
                        scope: self.scope,
                    },
                    self.cancel,
                )?;
                self.journal
                    .append(Event::BackupSynced(transaction), self.cancel)
            }
            NamespaceAction::Publish => {
                self.journal.append(
                    Event::Published {
                        transaction,
                        observed,
                        scope: self.scope,
                    },
                    self.cancel,
                )?;
                self.journal
                    .append(Event::EffectSynced(transaction), self.cancel)
            }
        }
    }
    fn checkpoint(&mut self, point: OwnPoint) -> io::Result<()> {
        (self.hook)(point)?;
        self.journal.ensure_current()
    }
}
impl Journal<'_> {
    /// Trusted coordinator primitive; the caller still must establish actual
    /// global staging, live roots/helper/visible approval and absolute bindings.
    /// Installed dry-run never invokes this API. No auto retry or rollback.
    pub fn execute_namespace(
        &mut self,
        transaction: u32,
        action: NamespaceAction,
        plan: NamespaceMove<'_>,
        read: u64,
        cancel: &AtomicBool,
    ) -> Result<OwnNamespaceRename, NamespaceFailure> {
        self.execute_namespace_with(transaction, action, plan, read, cancel, |_| Ok(()))
    }
    pub(in crate::directory_sync_store) fn execute_namespace_with(
        &mut self,
        transaction: u32,
        action: NamespaceAction,
        plan: NamespaceMove<'_>,
        read: u64,
        cancel: &AtomicBool,
        hook: impl FnMut(OwnPoint) -> io::Result<()>,
    ) -> Result<OwnNamespaceRename, NamespaceFailure> {
        self.execute_namespace_inner(
            transaction,
            action,
            plan,
            read,
            NamespaceControl {
                cancel,
                cooperative: None,
            },
            hook,
        )
    }
    pub fn execute_namespace_controlled(
        &mut self,
        transaction: u32,
        action: NamespaceAction,
        plan: NamespaceMove<'_>,
        read: u64,
        control: &crate::copy_engine::Control,
    ) -> Result<OwnNamespaceRename, NamespaceFailure> {
        self.execute_namespace_inner(
            transaction,
            action,
            plan,
            read,
            NamespaceControl {
                cancel: control.cancellation_flag(),
                cooperative: Some(control),
            },
            |_| Ok(()),
        )
    }
    fn execute_namespace_inner(
        &mut self,
        transaction: u32,
        action: NamespaceAction,
        plan: NamespaceMove<'_>,
        read: u64,
        controls: NamespaceControl<'_>,
        hook: impl FnMut(OwnPoint) -> io::Result<()>,
    ) -> Result<OwnNamespaceRename, NamespaceFailure> {
        let cancel = controls.cancel;
        self.ensure_current().map_err(before)?;
        self.schema.staging_admission().map_err(before)?;
        let scope = self
            .replay
            .state
            .validate_namespace(&self.schema, transaction, action, &plan)
            .map_err(before)?;
        let seal = plan.intent_seal();
        let result = {
            let mut log = Log {
                journal: self,
                transaction,
                action,
                scope,
                cancel,
                hook,
            };
            plan.execute_own(read, controls, &mut log)
        };
        match result {
            Ok(change) => Ok(OwnNamespaceRename {
                change,
                seal,
                transaction,
                action,
                sequence: self.replay.sequence,
            }),
            Err(error) => {
                // Intent ACK failure can leave bytes even when intent_recorded is
                // false. Do not infer absence from any write/rename error.
                if error.intent_recorded
                    || error.syscall_attempted
                    || self.replay.state.namespace_guard().is_some()
                    || self.state == State::Uncertain
                {
                    self.state = State::Uncertain;
                }
                Err(error)
            }
        }
    }
}
