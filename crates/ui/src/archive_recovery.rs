//! Explicit recovery review of an existing result; display paths are never input.
use crate::{
    archive_operation::{Prepared, RecoveryAction},
    copy_job::{self, Request},
    model::Model,
};
use std::sync::atomic::{AtomicU64, Ordering};
#[derive(Clone, Copy, Debug)]
pub struct Confirmation {
    pub stamp: u64,
    pub outcome: u64,
    pub action: RecoveryAction,
    pub paused: bool,
}
fn stamp() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
        .expect("archive review stamps exhausted")
}
impl Model {
    pub fn archive_recovery_actions(&self) -> (bool, bool) {
        if self.closing
            || self.copy.active_id().is_some()
            || self.copy_form.is_some()
            || self.archive_preparing.is_some()
            || self.move_preparing.is_some()
            || self.mutation_applied
        {
            return (false, false);
        }
        let Some(outcome) = self.notice_copy_outcome() else {
            return (false, false);
        };
        if let Err(copy_job::Failure::Archive(error)) = &outcome.result {
            (
                error.allows(RecoveryAction::Cleanup),
                error.allows(RecoveryAction::Sync),
            )
        } else {
            (false, false)
        }
    }
    pub fn review_archive_recovery(&mut self, id: u64, action: RecoveryAction) {
        if self.archive_confirmation.is_some() || self.copy_notice_id != Some(id) {
            return;
        }
        let flags = self.archive_recovery_actions();
        if !(if action == RecoveryAction::Cleanup {
            flags.0
        } else {
            flags.1
        }) {
            return;
        }
        self.archive_confirmation = Some(Confirmation {
            stamp: stamp(),
            outcome: id,
            action,
            paused: false,
        });
        self.notice_scroll = 0;
        self.notice_pan = 0;
        self.hits.clear();
    }
    pub fn back_archive_recovery(&mut self, expected: u64) {
        if self
            .archive_confirmation
            .is_some_and(|c| c.stamp == expected)
        {
            self.archive_confirmation = None;
            self.hits.clear();
        }
    }
    pub fn pause_archive_recovery(&mut self, expected: u64) {
        if let Some(c) = self
            .archive_confirmation
            .as_mut()
            .filter(|c| c.stamp == expected)
        {
            c.paused = !c.paused;
            c.stamp = stamp();
            self.hits.clear();
        }
    }
    pub fn clear_before_archive_recovery(&mut self, expected: u64) {
        if self
            .archive_confirmation
            .is_some_and(|c| c.stamp == expected)
        {
            self.cancel_copy_pending();
            if let Some(c) = &mut self.archive_confirmation {
                c.stamp = stamp();
            }
            self.hits.clear();
        }
    }
    pub fn commit_archive_recovery(&mut self, expected: u64) {
        let Some(review) = self
            .archive_confirmation
            .filter(|c| c.stamp == expected && self.copy_notice_id == Some(c.outcome))
        else {
            return;
        };
        let flags = self.archive_recovery_actions();
        if !(if review.action == RecoveryAction::Cleanup {
            flags.0
        } else {
            flags.1
        }) {
            return;
        }
        self.copy.set_external_fds(
            self.clipboard_state.external_fds()
                + self.native_transfer.external_fds()
                + self.search_transfer_fds(),
        );
        let mut old = self.copy_last.take().expect("reviewed archive result");
        let result = std::mem::replace(&mut old.result, Err(copy_job::Failure::WorkerLost));
        let Err(copy_job::Failure::Archive(error)) = result else {
            unreachable!("eligibility checked");
        };
        let prepared = match Prepared::recovery(error, review.action) {
            Ok(prepared) => prepared,
            Err(error) => {
                old.result = Err(copy_job::Failure::Archive(error));
                self.copy_last = Some(old);
                self.archive_confirmation = None;
                return;
            }
        };
        let parent = prepared.parent().map(std::path::Path::to_path_buf);
        self.copy_id = self
            .copy_id
            .checked_add(1)
            .expect("operation ids exhausted");
        let id = self.copy_id;
        let request = Request::reviewed_extract(id, prepared);
        let request = if review.paused {
            request.start_paused()
        } else {
            request
        };
        match self.copy.start_recovery(request) {
            Ok(()) => {
                self.copy_owners.insert(id, self.pane_owner());
                if let Some(parent) = parent {
                    self.copy_locations.insert(id, parent);
                }
                self.copy_notice_id = None;
                self.archive_confirmation = None;
                self.notice_open = false;
                self.copy.hold_queue(false);
                self.hits.clear();
                self.operation_message = Some(format!("{} #{id}", review.action.title()));
            }
            Err(mut rejected) => {
                let error = rejected
                    .request
                    .take_archive()
                    .and_then(Prepared::into_recovery_failure)
                    .expect("rejected recovery returns original capability");
                old.result = Err(copy_job::Failure::Archive(error));
                self.copy_last = Some(old);
                self.archive_confirmation = None;
                self.notice_open = true;
                self.operation_message =
                    Some(format!("Recuperación no admitida: {}", rejected.reason));
                self.hits.clear();
            }
        }
    }
}
