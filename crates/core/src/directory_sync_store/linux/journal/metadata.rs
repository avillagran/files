use super::*;
use crate::exdev_source::{
    AppliedStageMetadata, MetadataBatch, NamespaceScope, StageMetadataPoint,
};
#[derive(Debug)]
pub struct OwnStageMetadata {
    node: u32,
    seal: [u8; 32],
    sequence: u64,
    read: u64,
    applied: AppliedStageMetadata,
}
impl OwnStageMetadata {
    pub fn node(&self) -> u32 {
        self.node
    }
    pub fn seal(&self) -> [u8; 32] {
        self.seal
    }
    pub fn sequence(&self) -> u64 {
        self.sequence
    }
    pub fn payload_read(&self) -> u64 {
        self.read
    }
    pub fn applied(&self) -> &AppliedStageMetadata {
        &self.applied
    }
}
#[derive(Debug)]
pub struct JournalMetadataFailure {
    pub cause: io::Error,
    pub intent_acknowledged: bool,
    pub mutation_attempted: bool,
    pub applied_acknowledged: bool,
}
impl std::fmt::Display for JournalMetadataFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.cause.fmt(f)
    }
}
impl std::error::Error for JournalMetadataFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.cause)
    }
}
#[derive(Clone, Copy)]
pub(in crate::directory_sync_store) enum MetadataPoint {
    IntentWritten,
    IntentFileSynced,
    IntentAcknowledged,
    IntentDurable,
    ValuesPrepared,
    BeforeMutation,
    Owner,
    Writable,
    Attributes,
    Mode,
    Times,
    ValuesVerified,
    AppliedWritten,
    AppliedFileSynced,
    AppliedAcknowledged,
    Confirmed,
    BeforeOwnerCommit,
}
impl Journal<'_> {
    /// Explicit trusted-coordinator step over ALL private staging owners.
    /// No native approval, workspace provenance or automatic recovery supplied.
    pub fn apply_stage_metadata(
        &mut self,
        node: u32,
        scopes: Vec<NamespaceScope<'_>>,
        read: u64,
        cancel: &AtomicBool,
    ) -> Result<OwnStageMetadata, JournalMetadataFailure> {
        self.stage_metadata_inner(node, scopes, read, cancel, None, |_| Ok(()))
    }
    pub fn apply_stage_metadata_controlled(
        &mut self,
        node: u32,
        scopes: Vec<NamespaceScope<'_>>,
        read: u64,
        control: &crate::copy_engine::Control,
    ) -> Result<OwnStageMetadata, JournalMetadataFailure> {
        self.stage_metadata_inner(
            node,
            scopes,
            read,
            control.cancellation_flag(),
            Some(control),
            |_| Ok(()),
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub(in crate::directory_sync_store) fn stage_metadata_inner(
        &mut self,
        node: u32,
        scopes: Vec<NamespaceScope<'_>>,
        read: u64,
        cancel: &AtomicBool,
        control: Option<&crate::copy_engine::Control>,
        mut hook: impl FnMut(MetadataPoint) -> io::Result<()>,
    ) -> Result<OwnStageMetadata, JournalMetadataFailure> {
        let mut acknowledged = false;
        let mut attempted = false;
        let mut confirmed = false;
        let mut writing = false;
        let result = (|| {
            check_cancel(cancel)?;
            if let Some(c) = control {
                c.checkpoint()?;
            }
            self.ensure_current()?;
            self.schema.staging_admission()?;
            let spec = self
                .replay
                .state
                .metadata_spec(&self.schema, node, &scopes)?;
            let expected = StoredProof::decode(&self.store.proof.encode())?;
            let value = self.store.value(&expected, Target::Stage(node as usize))?;
            if value.after_children() {
                return Err(bad("Stage metadata no acepta AfterOwnChildren"));
            }
            let desired = value.values();
            let mut batch =
                MetadataBatch::prepare(scopes, spec.scope, spec.entry, read, cancel, control)?;
            writing = true;
            self.append_with(
                Event::StageMetadataIntent {
                    node,
                    before: spec.before,
                    seal: spec.seal,
                },
                cancel,
                |at| match at {
                    Barrier::Write => hook(MetadataPoint::IntentWritten),
                    Barrier::FileSynced => hook(MetadataPoint::IntentFileSynced),
                    Barrier::Acknowledged => hook(MetadataPoint::IntentAcknowledged),
                    _ => Ok(()),
                },
            )?;
            acknowledged = true;
            hook(MetadataPoint::IntentDurable)?;
            let ready = |journal: &Self| -> io::Result<()> {
                journal.ensure_current()?;
                if journal.replay.state.stage_metadata_guard()
                    != Some((node, spec.before, spec.seal))
                {
                    return Err(bad("Guard metadata cambió"));
                }
                Ok(())
            };
            ready(self)?;
            let (inventory, parent, name, index) = batch.target();
            let applied = match inventory.apply_stage_inner(
                parent,
                name,
                index,
                desired,
                cancel,
                control,
                |point| {
                    let point = match point {
                        StageMetadataPoint::Prepared => MetadataPoint::ValuesPrepared,
                        StageMetadataPoint::BeforeMutation => MetadataPoint::BeforeMutation,
                        StageMetadataPoint::Owner => MetadataPoint::Owner,
                        StageMetadataPoint::Writable => MetadataPoint::Writable,
                        StageMetadataPoint::Attributes => MetadataPoint::Attributes,
                        StageMetadataPoint::Mode => MetadataPoint::Mode,
                        StageMetadataPoint::Times => MetadataPoint::Times,
                        StageMetadataPoint::Verified => MetadataPoint::ValuesVerified,
                    };
                    hook(point)?;
                    ready(self)
                },
            ) {
                Ok(v) => {
                    attempted = true;
                    v
                }
                Err(e) => {
                    attempted = e.mutation_attempted;
                    return Err(e.cause);
                }
            };
            batch.patch(&applied, desired)?;
            let bytes = batch.finish(read, cancel, control, |after| {
                if let Some(after) = after {
                    ready(self)?;
                    self.append_with(Event::StageMetadataApplied { node, after }, cancel, |at| {
                        match at {
                            Barrier::Write => hook(MetadataPoint::AppliedWritten),
                            Barrier::FileSynced => hook(MetadataPoint::AppliedFileSynced),
                            Barrier::Acknowledged => hook(MetadataPoint::AppliedAcknowledged),
                            _ => Ok(()),
                        }
                    })?;
                    confirmed = true;
                    hook(MetadataPoint::Confirmed)?;
                } else {
                    hook(MetadataPoint::BeforeOwnerCommit)?;
                }
                self.ensure_current()
            })?;
            Ok(OwnStageMetadata {
                node,
                seal: spec.seal,
                sequence: self.replay.sequence,
                read: bytes,
                applied,
            })
        })();
        if result.is_err() && (writing || attempted) {
            self.state = State::Uncertain;
        }
        result.map_err(|cause| JournalMetadataFailure {
            cause,
            intent_acknowledged: acknowledged,
            mutation_attempted: attempted,
            applied_acknowledged: confirmed,
        })
    }
}
