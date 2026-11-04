//! Explicit, single-regular-file recovery. All descriptors are reopened; no
//! volatile verification/cleanup authority is inherited from the old executor.
use super::*;
use crate::move_manifest::Kind;
use std::os::unix::ffi::OsStringExt;

pub struct Recovery {
    journal: Journal,
    source_parent: File,
    destination_parent: File,
    source_stage: Option<Stage>,
    destination_stage: Option<Stage>,
    name: OsString,
    unconfirmed_stages: Vec<PathBuf>,
}
impl std::fmt::Debug for Recovery {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Recovery")
            .field("phase", &self.phase())
            .finish_non_exhaustive()
    }
}
impl Recovery {
    /// No automatic action. Caller must explicitly choose finish or restore on
    /// its worker. Parent paths must still bind to the persisted directory IDs.
    pub fn open(journals: &Path, job: [u8; 16]) -> io::Result<Self> {
        Self::from_journal(Journal::open_single_file(journals, job)?)
    }
    pub fn open_reviewed(
        journals: &Path,
        job: [u8; 16],
        proof: &crate::move_journal::IntentProof,
    ) -> io::Result<Self> {
        let journal = Journal::open_single_file(journals, job)?;
        journal.require_intent(proof)?;
        Self::from_journal(journal)
    }
    pub fn intent_proof(&self) -> io::Result<crate::move_journal::IntentProof> {
        self.journal.intent_proof()
    }
    fn from_journal(journal: Journal) -> io::Result<Self> {
        let job = journal.manifest().job();
        let manifest = journal.manifest();
        if manifest.entries().len() != 1
            || manifest.entries()[0].identity.kind != Kind::File
            || manifest.entries()[0].revision.links != 1
        {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Recovery inicial sólo archivo sin hardlinks",
            ));
        }
        let source_parent = directory(Path::new(OsStr::from_bytes(manifest.source_parent())))?;
        let destination_parent =
            directory(Path::new(OsStr::from_bytes(manifest.destination_parent())))?;
        if identity(&source_parent)? != manifest.source_identity()
            || identity(&destination_parent)? != manifest.destination_identity()
        {
            return Err(invalid(
                "Padre journal sustituido; no operar por ruta nueva",
            ));
        }
        let source_stage = reopen_stage(&source_parent, job, 's', journal.state().source_stage())?;
        let destination_stage = reopen_stage(
            &destination_parent,
            job,
            'd',
            journal.state().destination_stage(),
        )?;
        let name = OsString::from_vec(manifest.entries()[0].relative.clone());
        let mut unconfirmed_stages = Vec::new();
        for (parent, side, expected) in [
            (&source_parent, 's', journal.state().source_stage()),
            (
                &destination_parent,
                'd',
                journal.state().destination_stage(),
            ),
        ] {
            if expected.is_none() {
                let candidate = Stage::planned(job, side);
                match at(parent, &candidate.name) {
                    Ok(_) => unconfirmed_stages.push(current_path(parent)?.join(&candidate.name)),
                    Err(e) if e.kind() == io::ErrorKind::NotFound => (),
                    Err(e) => return Err(e),
                }
            }
        }
        Ok(Self {
            journal,
            source_parent,
            destination_parent,
            source_stage,
            destination_stage,
            name,
            unconfirmed_stages,
        })
    }
    /// Candidates are diagnostics only: no journal identity authorizes cleanup.
    pub fn unconfirmed_stages(&self) -> &[PathBuf] {
        &self.unconfirmed_stages
    }
    pub fn phase(&self) -> Phase {
        self.journal.state().phase()
    }
    pub fn barriers_current(&self) -> bool {
        self.journal.state().barriers_current()
    }
    /// Finish only after explicit intent to publish was recorded. Reopening a
    /// merely Prepared/SourceStaged copy does not implicitly start publication.
    pub fn finish(&mut self, control: &Control) -> io::Result<()> {
        self.finish_with(control, || Ok(()))
    }
    fn finish_with(
        &mut self,
        control: &Control,
        mut before_stage_sync: impl FnMut() -> io::Result<()>,
    ) -> io::Result<()> {
        self.journal.ensure_current()?;
        control.checkpoint()?;
        if self.phase() == Phase::Completed {
            return Ok(());
        }
        let expected = &self.journal.manifest().entries()[0];
        let destination_id = self
            .journal
            .state()
            .destinations()
            .first()
            .copied()
            .ok_or(invalid("Copia sin identidad verificada; conservar staging"))?;
        if self.phase() == Phase::PublishIntent {
            // A completed rename without ACK is identified by its recorded
            // destination inode plus full bytes/metadata. No absence inference.
            match at(&self.destination_parent, &self.name) {
                Ok(_) => {
                    observe(
                        &self.destination_parent,
                        &self.name,
                        expected,
                        destination_id,
                        expected.revision,
                        true,
                        control,
                    )?;
                }
                Err(e) if e.kind() == io::ErrorKind::NotFound => {
                    let source = self
                        .source_stage
                        .as_ref()
                        .ok_or(invalid("Sin staging origen"))?
                        .pin()?;
                    observe(
                        source,
                        &self.name,
                        expected,
                        expected.identity,
                        self.journal
                            .state()
                            .source_revision()
                            .ok_or(invalid("Sin revisión origen"))?,
                        false,
                        control,
                    )?;
                    let stage = self
                        .destination_stage
                        .as_ref()
                        .ok_or(invalid("Sin staging destino"))?
                        .pin()?;
                    let copy = observe(
                        stage,
                        &self.name,
                        expected,
                        destination_id,
                        expected.revision,
                        true,
                        control,
                    )?;
                    sync_observation(stage, &self.name, &copy)?;
                    control.checkpoint()?;
                    matches(stage, &self.name, copy.root())?;
                    self.journal.ensure_current()?;
                    rename(stage, &self.name, &self.destination_parent, &self.name)?;
                }
                Err(e) => return Err(e),
            }
            let copy = observe(
                &self.destination_parent,
                &self.name,
                expected,
                destination_id,
                expected.revision,
                true,
                control,
            )?;
            sync_observation(&self.destination_parent, &self.name, &copy)?;
            before_stage_sync()?;
            sync_stage(&self.destination_stage, &self.destination_parent, false)?;
            self.journal.record(Event::DestinationPublished {
                root_revision: copy.entries()[0].revision,
            })?;
        }
        if self.phase() == Phase::DestinationPublished {
            verify_publication(&self.journal, &self.destination_parent, &self.name, control)?;
            before_stage_sync()?;
            sync_stage(&self.destination_stage, &self.destination_parent, false)?;
            self.journal.record(Event::DestinationSynced)?;
        }
        let parent = &self.destination_parent;
        let name = &self.name;
        let destination_stage = &self.destination_stage;
        let allow_missing_stage = self.journal.state().removed() == 1;
        self.journal.renew_barriers(|m, state| {
            let copy = observe(
                parent,
                name,
                &m.entries()[0],
                state.destinations()[0],
                state
                    .destination_revision()
                    .ok_or(invalid("Sin revisión publicada"))?,
                false,
                control,
            )?;
            sync_observation(parent, name, &copy)?;
            before_stage_sync()?;
            sync_stage(destination_stage, parent, allow_missing_stage)
        })?;
        if self.journal.state().removed() == 0 {
            let source_stage = self
                .source_stage
                .as_ref()
                .ok_or(invalid("Staging origen ausente antes de Removed"))?
                .pin()?;
            let source = match at(source_stage, name) {
                Ok(_) => Some(observe(
                    source_stage,
                    name,
                    &self.journal.manifest().entries()[0],
                    self.journal.manifest().entries()[0].identity,
                    self.journal
                        .state()
                        .source_revision()
                        .ok_or(invalid("Sin revisión origen"))?,
                    false,
                    control,
                )?),
                Err(e)
                    if e.kind() == io::ErrorKind::NotFound
                        && self.journal.state().pending_delete() == Some(0) =>
                {
                    None
                }
                Err(e) => return Err(e),
            };
            // A missing source is accepted only inside the known owned stage,
            // with outstanding DeleteIntent, and AFTER renewed destination sync.
            self.journal.verify_entry(0, |_, _, _| {
                if let Some(source) = &source {
                    matches(source_stage, name, source.root())?;
                } else {
                    absent(source_stage, name)?;
                }
                Ok(())
            })?;
            if self.journal.state().pending_delete().is_none() {
                self.journal.record(Event::DeleteIntent(0))?;
            }
            if let Some(source) = &source {
                matches(source_stage, name, source.root())?;
                if revision(source.root())? != source.entries()[0].revision {
                    return Err(invalid("Origen cambió junto a unlink recovery"));
                }
                // Recheck destination after journal writes, immediately before
                // unlink. Full verification happened before current barriers.
                check_published_binding(&self.journal, parent, name)?;
                if unsafe { libc::unlinkat(source_stage.as_raw_fd(), c(name)?.as_ptr(), 0) } != 0 {
                    return Err(io::Error::last_os_error());
                }
            }
            source_stage.sync_all()?;
            self.journal.record(Event::Removed(0))?;
        }
        remove_stage(&mut self.source_stage, &self.source_parent)?;
        remove_stage(&mut self.destination_stage, &self.destination_parent)?;
        check_published_binding(&self.journal, parent, name)?;
        self.journal.record(Event::Completed)
    }
    /// Restore original source without overwriting a new occupant. Published
    /// destination is always preserved. Unverified partial copies are preserved
    /// with an error, not silently claimed by a predictable staging filename.
    pub fn restore(&mut self, control: &Control) -> io::Result<()> {
        self.journal.ensure_current()?;
        control.checkpoint()?;
        if self.phase() == Phase::Restored {
            return Ok(());
        }
        if self.journal.state().removed() != 0 {
            return Err(invalid("Origen ya eliminado; elegir finalizar"));
        }
        let expected = &self.journal.manifest().entries()[0];
        let original = match at(&self.source_parent, &self.name) {
            Ok(_) => Some(observe(
                &self.source_parent,
                &self.name,
                expected,
                expected.identity,
                expected.revision,
                matches!(self.phase(), Phase::RestoreIntent | Phase::Restored),
                control,
            )?),
            Err(e) if e.kind() == io::ErrorKind::NotFound => None,
            Err(e) => return Err(e),
        };
        let staged = if let Some(stage) = &self.source_stage {
            if stage.removed {
                None
            } else {
                match at(stage.pin()?, &self.name) {
                    Ok(_) => Some(observe(
                        stage.pin()?,
                        &self.name,
                        expected,
                        expected.identity,
                        self.journal
                            .state()
                            .source_revision()
                            .unwrap_or(expected.revision),
                        self.journal.state().source_revision().is_none(),
                        control,
                    )?),
                    Err(e) if e.kind() == io::ErrorKind::NotFound => None,
                    Err(e) => return Err(e),
                }
            }
        } else {
            None
        };
        if original.is_some() == staged.is_some() {
            return Err(invalid("Origen ausente o ambiguo; conservar para rescate"));
        }
        if self.phase() != Phase::RestoreIntent {
            let inventory = original.as_ref().or(staged.as_ref()).unwrap();
            let source_parent = if staged.is_some() {
                self.source_stage.as_ref().unwrap().pin()?
            } else {
                &self.source_parent
            };
            self.journal.verify_restore(|_, _| {
                inventory.verify_controlled(source_parent, &self.name, control)
            })?;
            self.journal.record(Event::RestoreIntent)?;
        }
        if let Some(staged) = &staged {
            let stage = self.source_stage.as_ref().unwrap().pin()?;
            matches(stage, &self.name, staged.root())?;
            self.journal.ensure_current()?;
            rename(stage, &self.name, &self.source_parent, &self.name)?;
        }
        let restored = observe(
            &self.source_parent,
            &self.name,
            &self.journal.manifest().entries()[0],
            self.journal.manifest().entries()[0].identity,
            self.journal.manifest().entries()[0].revision,
            true,
            control,
        )?;
        sync_observation(&self.source_parent, &self.name, &restored)?;
        if let Some(stage) = &self.source_stage
            && !stage.removed
        {
            stage.pin()?.sync_all()?;
        }
        // Claim a staged copy ONLY with its journal identity and full current
        // signature. A published name is never a cleanup target here.
        if let Some(stage) = &self.destination_stage
            && !stage.removed
        {
            match at(stage.pin()?, &self.name) {
                Ok(_) => {
                    let id =
                        self.journal
                            .state()
                            .destinations()
                            .first()
                            .copied()
                            .ok_or(invalid(
                                "Copia parcial sin recibo; origen restaurado, staging conservado",
                            ))?;
                    let copy = observe(
                        stage.pin()?,
                        &self.name,
                        &self.journal.manifest().entries()[0],
                        id,
                        self.journal.manifest().entries()[0].revision,
                        true,
                        control,
                    )?;
                    matches(stage.pin()?, &self.name, copy.root())?;
                    if revision(copy.root())? != copy.entries()[0].revision {
                        return Err(invalid("Copia cambió junto a cleanup recovery"));
                    }
                    if unsafe {
                        libc::unlinkat(stage.pin()?.as_raw_fd(), c(&self.name)?.as_ptr(), 0)
                    } != 0
                    {
                        return Err(io::Error::last_os_error());
                    }
                    stage.pin()?.sync_all()?;
                }
                Err(e) if e.kind() == io::ErrorKind::NotFound => (),
                Err(e) => return Err(e),
            }
        }
        if !self.unconfirmed_stages.is_empty() {
            return Err(invalid(
                "Origen restaurado; candidatos de staging sin identidad conservados para revisión",
            ));
        }
        remove_stage(&mut self.source_stage, &self.source_parent)?;
        remove_stage(&mut self.destination_stage, &self.destination_parent)?;
        self.journal.record(Event::Restored)
    }
}
fn sync_stage(stage: &Option<Stage>, parent: &File, allow_missing: bool) -> io::Result<()> {
    let stage = stage
        .as_ref()
        .ok_or(invalid("Sin identidad de staging destino"))?;
    if stage.removed {
        if !allow_missing {
            return Err(invalid("Staging destino ausente antes de Removed"));
        }
        absent(parent, &stage.name)?;
        return parent.sync_all();
    }
    matches(parent, &stage.name, stage.pin()?)?;
    stage.pin()?.sync_all()?;
    parent.sync_all()?;
    matches(parent, &stage.name, stage.pin()?)
}
fn reopen_stage(
    parent: &File,
    job: [u8; 16],
    side: char,
    expected: Option<Identity>,
) -> io::Result<Option<Stage>> {
    let Some(expected) = expected else {
        return Ok(None);
    };
    let mut stage = Stage::planned(job, side);
    let file = match directory_at(parent, &stage.name) {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            stage.mkdir_returned = true;
            stage.removed = true;
            return Ok(Some(stage));
        }
        Err(e) => return Err(e),
    };
    let m = file.metadata()?;
    if identity(&file)? != expected
        || m.uid() != unsafe { libc::geteuid() }
        || m.mode() & 0o077 != 0
    {
        return Err(invalid("Staging recovery sustituido o no privado"));
    }
    matches(parent, &stage.name, &file)?;
    stage.file = Some(file);
    stage.identity = Some(expected);
    stage.mkdir_returned = true;
    stage.confirmed = true;
    Ok(Some(stage))
}
fn directory_at(parent: &File, name: &OsStr) -> io::Result<File> {
    owned(unsafe {
        libc::openat(
            parent.as_raw_fd(),
            c(name)?.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    })
}
fn remove_stage(stage: &mut Option<Stage>, parent: &File) -> io::Result<()> {
    if let Some(stage) = stage {
        stage.remove(parent)?;
    }
    Ok(())
}
fn observe(
    parent: &File,
    name: &OsStr,
    expected: &Entry,
    expected_id: Identity,
    expected_revision: Revision,
    own_rename: bool,
    control: &Control,
) -> io::Result<Inventory> {
    let observed = exdev_source::capture_controlled(parent, name, control)?;
    if observed.entries().len() != 1 {
        return Err(invalid("Recovery esperaba un único archivo regular"));
    }
    let entry = &observed.entries()[0];
    if entry.relative != expected.relative
        || entry.identity != expected_id
        || entry.content_digest != expected.content_digest
        || entry.metadata_digest != expected.metadata_digest
        || entry.access != expected.access
        || if own_rename {
            !entry.revision.matches_after_rename(expected_revision)
        } else {
            entry.revision != expected_revision
        }
    {
        return Err(invalid(
            "Firma recovery no coincide con journal; conservar objeto",
        ));
    }
    Ok(observed)
}
fn sync_observation(parent: &File, name: &OsStr, inventory: &Inventory) -> io::Result<()> {
    let file = owned(unsafe {
        libc::openat(
            parent.as_raw_fd(),
            c(name)?.as_ptr(),
            libc::O_RDONLY
                | libc::O_NONBLOCK
                | libc::O_NOFOLLOW
                | libc::O_NOATIME
                | libc::O_CLOEXEC,
        )
    })?;
    if identity(&file)? != identity(inventory.root())?
        || revision(&file)? != inventory.entries()[0].revision
    {
        return Err(invalid("Binding cambió antes de sync recovery"));
    }
    file.sync_all()?;
    parent.sync_all()?;
    matches(parent, name, &file)?;
    if revision(&file)? != inventory.entries()[0].revision {
        return Err(invalid("Revisión cambió durante sync recovery"));
    }
    Ok(())
}
fn check_published_binding(journal: &Journal, parent: &File, name: &OsStr) -> io::Result<()> {
    let file = at(parent, name)?;
    if identity(&file)? != journal.state().destinations()[0]
        || Some(revision(&file)?) != journal.state().destination_revision()
    {
        return Err(invalid("Destino publicado sustituido/modificado"));
    }
    Ok(())
}
fn verify_publication(
    journal: &Journal,
    parent: &File,
    name: &OsStr,
    control: &Control,
) -> io::Result<()> {
    let inventory = observe(
        parent,
        name,
        &journal.manifest().entries()[0],
        journal.state().destinations()[0],
        journal
            .state()
            .destination_revision()
            .ok_or(invalid("Sin recibo publicado"))?,
        false,
        control,
    )?;
    sync_observation(parent, name, &inventory)
}

#[cfg(test)]
mod tests {
    use super::super::tests::Fixture;
    use super::*;
    #[test]
    fn failed_ack_blocks_same_instance_before_any_retry_mutation() {
        for restore in [false, true] {
            let f = Fixture::new();
            let mut transfer = f.begin(130);
            transfer
                .copy_to_staging(&Control::default(), |_| {})
                .unwrap();
            let source_path = transfer.source_location().unwrap();
            drop(transfer);
            let mut recovery = Recovery::open(&f.journals, [130; 16]).unwrap();
            let event = if restore {
                let inventory = exdev_source::capture_controlled(
                    recovery.source_stage.as_ref().unwrap().pin().unwrap(),
                    &recovery.name,
                    &Control::default(),
                )
                .unwrap();
                recovery
                    .journal
                    .verify_restore(|_, _| {
                        inventory.verify(
                            recovery.source_stage.as_ref().unwrap().pin()?,
                            &recovery.name,
                            &AtomicBool::new(false),
                        )
                    })
                    .unwrap();
                Event::RestoreIntent
            } else {
                Event::PublishIntent
            };
            assert!(recovery.journal.fail_after_record_sync(event).is_err());
            assert!(recovery.finish(&Control::default()).is_err());
            assert!(recovery.restore(&Control::default()).is_err());
            assert!(source_path.exists());
            assert!(!f.source.join("item").exists());
            assert!(!f.destination.join("item").exists());
            drop(recovery);
            let mut reopened = Recovery::open(&f.journals, [130; 16]).unwrap();
            if restore {
                reopened.restore(&Control::default()).unwrap();
                assert!(f.source.join("item").exists());
            } else {
                reopened.finish(&Control::default()).unwrap();
                assert!(f.destination.join("item").exists());
            }
        }
    }
    #[test]
    fn missing_destination_stage_sync_prevents_synced_record_and_source_deletion() {
        let f = Fixture::new();
        let mut transfer = f.begin(140);
        transfer
            .copy_to_staging(&Control::default(), |_| {})
            .unwrap();
        assert!(
            transfer
                .publish_with(&Control::default(), |point| if point == 1 {
                    Err(io::Error::other("after Published ACK"))
                } else {
                    Ok(())
                })
                .is_err()
        );
        let source = transfer.source_location().unwrap();
        drop(transfer);
        let mut recovery = Recovery::open(&f.journals, [140; 16]).unwrap();
        assert_eq!(recovery.phase(), Phase::DestinationPublished);
        assert!(
            recovery
                .finish_with(&Control::default(), || Err(io::Error::other(
                    "stage sync failed"
                )))
                .is_err()
        );
        assert_eq!(recovery.phase(), Phase::DestinationPublished);
        assert!(source.exists());
        assert!(!recovery.barriers_current());
        recovery.finish(&Control::default()).unwrap();
        assert_eq!(recovery.phase(), Phase::Completed);
    }
}
