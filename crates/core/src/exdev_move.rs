//! M25 executable single-file EXDEV transaction, WIP until recovery/UI wiring.
//! Native move remains separate. Trees/hardlinks are rejected before staging by
//! this initial executor; the bounded inventory/copy modules support more types.
#[cfg(target_os = "linux")]
mod linux {
    use crate::{
        copy_engine::Control,
        exdev_source::{self, Inventory, OwnedCopy},
        move_journal::{Event, Journal, Phase},
        move_manifest::{Entry, Identity, Manifest, Revision},
    };
    use std::{
        ffi::{CString, OsStr, OsString},
        fs::File,
        io,
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::{ffi::OsStrExt, fs::MetadataExt},
        },
        path::{Path, PathBuf},
        sync::atomic::AtomicBool,
    };
    mod recovery;
    pub use recovery::Recovery;
    fn invalid(s: &'static str) -> io::Error {
        io::Error::new(io::ErrorKind::InvalidData, s)
    }
    fn c(p: &OsStr) -> io::Result<CString> {
        CString::new(p.as_bytes()).map_err(|_| invalid("NUL en ruta EXDEV"))
    }
    fn owned(fd: i32) -> io::Result<File> {
        if fd < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(unsafe { File::from_raw_fd(fd) })
        }
    }
    fn identity(file: &File) -> io::Result<Identity> {
        exdev_source::observed_identity(&file.metadata()?)
    }
    fn revision(file: &File) -> io::Result<Revision> {
        exdev_source::observed_revision(&file.metadata()?)
    }
    fn directory(path: &Path) -> io::Result<File> {
        if !path.is_absolute() {
            return Err(invalid("Padre EXDEV requiere ruta absoluta"));
        }
        owned(unsafe {
            libc::open(
                c(path.as_os_str())?.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        })
    }
    fn at(parent: &File, name: &OsStr) -> io::Result<File> {
        owned(unsafe {
            libc::openat(
                parent.as_raw_fd(),
                c(name)?.as_ptr(),
                libc::O_PATH | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        })
    }
    fn absent(parent: &File, name: &OsStr) -> io::Result<()> {
        match at(parent, name) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
            Ok(_) => Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Destino ocupado; sin reemplazo",
            )),
        }
    }
    fn matches(parent: &File, name: &OsStr, pin: &File) -> io::Result<()> {
        if identity(&at(parent, name)?)? != identity(pin)? {
            return Err(invalid("Binding EXDEV sustituido"));
        }
        Ok(())
    }
    fn rename(source: &File, name: &OsStr, destination: &File, target: &OsStr) -> io::Result<()> {
        if unsafe {
            libc::syscall(
                libc::SYS_renameat2,
                source.as_raw_fd(),
                c(name)?.as_ptr(),
                destination.as_raw_fd(),
                c(target)?.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        } == 0
        {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }
    fn current_path(file: &File) -> io::Result<PathBuf> {
        let path = std::fs::read_link(format!("/proc/self/fd/{}", file.as_raw_fd()))?;
        if !path.is_absolute() || path.as_os_str().as_bytes().len() > 4096 {
            return Err(invalid("Ubicación FD no confirmada"));
        }
        let m = std::fs::symlink_metadata(&path)?;
        if exdev_source::observed_identity(&m)? != identity(file)? {
            return Err(invalid("Ubicación FD no confirmada"));
        }
        Ok(path)
    }
    fn copied_entry(e: &Entry) -> Entry {
        Entry {
            relative: e.relative.clone(),
            identity: e.identity,
            revision: e.revision,
            access: e.access,
            content_digest: e.content_digest,
            metadata_digest: e.metadata_digest,
        }
    }
    pub struct PreparedFileMove {
        source_parent: File,
        destination_parent: File,
        name: OsString,
        inventory: Option<Inventory>,
        job: [u8; 16],
    }
    impl std::fmt::Debug for PreparedFileMove {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("PreparedFileMove")
                .field("job", &self.job)
                .finish_non_exhaustive()
        }
    }
    impl PreparedFileMove {
        pub fn verify_selection(
            &self,
            expected: &crate::selection_source::Source,
            cancel: &AtomicBool,
        ) -> io::Result<()> {
            if cancel.load(std::sync::atomic::Ordering::Acquire) {
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "Revisión de selección cancelada",
                ));
            }
            expected.verify_captured(
                &self.source_parent,
                &self.name,
                self.inventory
                    .as_ref()
                    .ok_or_else(|| invalid("Inventario de revisión ya consumido"))?
                    .root(),
            )
        }
        pub fn verify_selection_controlled(
            &self,
            expected: &crate::selection_source::Source,
            control: &Control,
        ) -> io::Result<()> {
            control.checkpoint()?;
            self.verify_selection(expected, control.cancellation_flag())?;
            control.checkpoint()
        }
        pub fn capture(
            source: &Path,
            destination: &Path,
            job: [u8; 16],
            cancel: &AtomicBool,
        ) -> io::Result<Self> {
            Self::capture_inner(source, destination, job, cancel, true)
        }
        fn capture_inner(
            source: &Path,
            destination: &Path,
            job: [u8; 16],
            cancel: &AtomicBool,
            require_exdev: bool,
        ) -> io::Result<Self> {
            let name = source.file_name().ok_or(invalid("Falta nombre origen"))?;
            if name
                != destination
                    .file_name()
                    .ok_or(invalid("Falta nombre destino"))?
                || name.as_bytes().len() > 255
                || job == [0; 16]
            {
                return Err(invalid("EXDEV inicial requiere mismo nombre y job válido"));
            }
            let source_parent = directory(source.parent().ok_or(invalid("Falta padre origen"))?)?;
            let destination_parent =
                directory(destination.parent().ok_or(invalid("Falta padre destino"))?)?;
            let root = at(&source_parent, name)?;
            let m = root.metadata()?;
            if !m.is_file() || m.nlink() != 1 {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "Ejecutor EXDEV inicial: archivo regular sin hardlinks; árboles/ledger de lote pendientes, origen conservado",
                ));
            }
            if require_exdev
                && source_parent.metadata()?.dev() == destination_parent.metadata()?.dev()
            {
                return Err(invalid("Usar rename nativo para mismo filesystem"));
            }
            absent(&destination_parent, name)?;
            let inventory = exdev_source::capture(&source_parent, name, cancel)?;
            if identity(inventory.root())? != identity(&root)? {
                return Err(invalid("Origen sustituido durante revisión"));
            }
            Ok(Self {
                source_parent,
                destination_parent,
                name: name.to_os_string(),
                inventory: Some(inventory),
                job,
            })
        }
        pub fn job(&self) -> [u8; 16] {
            self.job
        }
        pub fn begin(
            mut self,
            journals: &Path,
            control: &Control,
        ) -> Result<Transfer, FailedTransfer> {
            let source_inventory = self.inventory.take();
            let mut transfer = Transfer {
                prepared: self,
                source_stage: None,
                destination_stage: None,
                journal: None,
                copy: None,
                source_inventory,
                source_rename_returned: false,
                source_rename_confirmed: false,
                source_restore_returned: false,
                source_restore_confirmed: false,
                published: false,
                source_unlinked: false,
            };
            let result = transfer.start(journals, control);
            match result {
                Ok(()) => Ok(transfer),
                Err(cause) => Err(FailedTransfer {
                    cause,
                    transfer: Box::new(transfer),
                }),
            }
        }
    }
    // Stored in Transfer BEFORE mkdir. Every successful mutation has a retained
    // state, even when open/validation/fsync fails. Unconfirmed creation is a
    // diagnostic candidate, never cleanup authority by its predictable name.
    struct Stage {
        file: Option<File>,
        name: OsString,
        identity: Option<Identity>,
        mkdir_returned: bool,
        confirmed: bool,
        removed: bool,
    }
    impl Stage {
        fn planned(job: [u8; 16], side: char) -> Self {
            let mut name = String::from(".files-move-");
            for byte in job {
                use std::fmt::Write;
                write!(name, "{byte:02x}").unwrap();
            }
            name.push('-');
            name.push(side);
            Self {
                file: None,
                name: OsString::from(name),
                identity: None,
                mkdir_returned: false,
                confirmed: false,
                removed: false,
            }
        }
        fn create(&mut self, parent: &File) -> io::Result<()> {
            self.create_with(parent, |_| Ok(()))
        }
        fn create_with(
            &mut self,
            parent: &File,
            mut fault: impl FnMut(u8) -> io::Result<()>,
        ) -> io::Result<()> {
            if self.mkdir_returned {
                return Err(invalid("Staging ya intentado; no reutilizar por nombre"));
            }
            if unsafe { libc::mkdirat(parent.as_raw_fd(), c(&self.name)?.as_ptr(), 0o700) } != 0 {
                return Err(io::Error::last_os_error());
            }
            self.mkdir_returned = true;
            fault(0)?;
            self.file = Some(owned(unsafe {
                libc::openat(
                    parent.as_raw_fd(),
                    c(&self.name)?.as_ptr(),
                    libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                )
            })?);
            fault(1)?;
            let file = self.file.as_ref().unwrap();
            let observed = identity(file)?;
            let m = file.metadata()?;
            if m.uid() != unsafe { libc::geteuid() } || m.mode() & 0o077 != 0 {
                return Err(invalid("Staging privado no confirmado"));
            }
            matches(parent, &self.name, file)?;
            self.identity = Some(observed);
            self.confirmed = true;
            file.sync_all()?;
            fault(2)?;
            parent.sync_all()?;
            fault(3)?;
            matches(parent, &self.name, file)?;
            Ok(())
        }
        fn pin(&self) -> io::Result<&File> {
            if !self.confirmed || self.removed {
                return Err(invalid("Staging sin identidad confirmada o ya retirado"));
            }
            self.file.as_ref().ok_or(invalid("Staging sin FD"))
        }
        fn remove(&mut self, parent: &File) -> io::Result<()> {
            self.remove_with(parent, || Ok(()))
        }
        fn remove_with(
            &mut self,
            parent: &File,
            before_sync: impl FnOnce() -> io::Result<()>,
        ) -> io::Result<()> {
            if !self.mkdir_returned {
                // mkdir never succeeded: an existing candidate name is foreign.
                return Ok(());
            }
            if !self.removed {
                let file = self.pin()?;
                matches(parent, &self.name, file)?;
                if Some(identity(file)?) != self.identity {
                    return Err(invalid("Staging sustituido"));
                }
                if unsafe {
                    libc::unlinkat(
                        parent.as_raw_fd(),
                        c(&self.name)?.as_ptr(),
                        libc::AT_REMOVEDIR,
                    )
                } != 0
                {
                    return Err(io::Error::last_os_error());
                }
                self.removed = true;
            }
            // Retain successful unlink even if directory fsync/ACK fails. Retry
            // syncs this captured parent only, never removes a new name occupant.
            before_sync()?;
            parent.sync_all()
        }
    }
    pub struct Transfer {
        prepared: PreparedFileMove,
        source_stage: Option<Stage>,
        destination_stage: Option<Stage>,
        journal: Option<Journal>,
        copy: Option<OwnedCopy>,
        source_inventory: Option<Inventory>,
        source_rename_returned: bool,
        source_rename_confirmed: bool,
        source_restore_returned: bool,
        source_restore_confirmed: bool,
        published: bool,
        source_unlinked: bool,
    }
    impl std::fmt::Debug for Transfer {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("Transfer")
                .field("source_rename_returned", &self.source_rename_returned)
                .field("source_rename_confirmed", &self.source_rename_confirmed)
                .field("published", &self.published)
                .field("source_unlinked", &self.source_unlinked)
                .finish_non_exhaustive()
        }
    }
    pub struct FailedTransfer {
        pub cause: io::Error,
        pub transfer: Box<Transfer>,
    }
    impl std::fmt::Debug for FailedTransfer {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("FailedTransfer")
                .field("cause", &self.cause)
                .field("transfer", &self.transfer)
                .finish()
        }
    }
    impl Transfer {
        pub fn intent_proof(&self) -> io::Result<Option<crate::move_journal::IntentProof>> {
            self.journal.as_ref().map(|j| j.intent_proof()).transpose()
        }
        pub fn affected_directories(&self) -> io::Result<Vec<PathBuf>> {
            Ok(vec![
                current_path(&self.prepared.source_parent)?,
                current_path(&self.prepared.destination_parent)?,
            ])
        }
        pub fn staging_candidates(&self) -> Vec<PathBuf> {
            let mut paths = Vec::new();
            for (parent, stage) in [
                (&self.prepared.source_parent, &self.source_stage),
                (&self.prepared.destination_parent, &self.destination_stage),
            ] {
                if let Some(stage) = stage
                    && stage.mkdir_returned
                    && !stage.removed
                {
                    let actual = stage.file.as_ref().and_then(|f| current_path(f).ok());
                    if let Some(path) =
                        actual.or_else(|| current_path(parent).ok().map(|p| p.join(&stage.name)))
                    {
                        paths.push(path);
                    }
                }
            }
            paths
        }
        pub fn phase(&self) -> Option<Phase> {
            self.journal.as_ref().map(|j| j.state().phase())
        }
        pub fn journal_uncertain(&self) -> bool {
            self.journal.as_ref().is_some_and(Journal::uncertain)
        }
        pub fn published(&self) -> bool {
            self.published
        }
        pub fn source_unlinked(&self) -> bool {
            self.source_unlinked
        }
        fn inventory(&self) -> &Inventory {
            self.copy
                .as_ref()
                .map(OwnedCopy::inventory)
                .or(self.source_inventory.as_ref())
                .expect("Transfer owns source inventory")
        }
        pub fn source_location(&self) -> io::Result<PathBuf> {
            current_path(self.inventory().root())
        }
        pub fn destination_location(&self) -> io::Result<Option<PathBuf>> {
            if self.published {
                self.copy
                    .as_ref()
                    .map(|c| current_path(c.root_pin()?))
                    .transpose()
            } else {
                Ok(None)
            }
        }
        pub fn copy_to_staging(
            &mut self,
            control: &Control,
            mut progress: impl FnMut(u64),
        ) -> io::Result<()> {
            control.checkpoint()?;
            if self.phase() != Some(Phase::SourceStaged)
                || self.copy.is_some()
                || !self.source_rename_confirmed
            {
                return Err(invalid("Estado no admite copiar"));
            }
            let source = self.source_stage.as_ref().unwrap().pin()?;
            matches(source, &self.prepared.name, self.inventory().root())?;
            let destination = self
                .destination_stage
                .as_ref()
                .unwrap()
                .pin()?
                .try_clone()?;
            let inventory = self
                .source_inventory
                .take()
                .ok_or(invalid("Inventario no disponible"))?;
            match exdev_source::copy_into_controlled(
                inventory,
                source,
                destination,
                control,
                &mut progress,
            ) {
                Ok(copy) => self.copy = Some(copy),
                Err(failed) => {
                    self.copy = Some(*failed.partial);
                    return Err(failed.cause);
                }
            }
            let copy = self.copy.as_ref().unwrap();
            let root = &copy.inventory().entries()[0];
            let destination = copy
                .destination_identities()
                .next()
                .ok_or(invalid("Raíz no copiada"))?;
            self.journal.as_mut().unwrap().record(Event::EntryVerified {
                index: 0,
                destination,
                content: root.content_digest,
                metadata: root.metadata_digest,
            })
        }
        pub fn publish(&mut self, control: &Control) -> io::Result<()> {
            self.publish_with(control, |_| Ok(()))
        }
        fn publish_with(
            &mut self,
            control: &Control,
            mut fault: impl FnMut(u8) -> io::Result<()>,
        ) -> io::Result<()> {
            control.checkpoint()?;
            if self.phase() != Some(Phase::SourceStaged)
                || self.journal.as_ref().unwrap().state().verified() != 1
                || self.published
            {
                return Err(invalid("Copia no verificada para publicación"));
            }
            self.inventory().verify_controlled(
                self.source_stage.as_ref().unwrap().pin()?,
                &self.prepared.name,
                control,
            )?;
            self.copy
                .as_mut()
                .unwrap()
                .verify_and_sync_controlled(control)?;
            absent(&self.prepared.destination_parent, &self.prepared.name)?;
            self.journal
                .as_mut()
                .unwrap()
                .record(Event::PublishIntent)?;
            control.checkpoint()?;
            rename(
                self.destination_stage.as_ref().unwrap().pin()?,
                &self.prepared.name,
                &self.prepared.destination_parent,
                &self.prepared.name,
            )?;
            // Returned syscall != confirmed publication. Block any destination
            // cleanup immediately; postcheck failure preserves both objects.
            self.published = true;
            self.copy
                .as_mut()
                .unwrap()
                .note_publication(self.prepared.destination_parent.try_clone()?)?;
            fault(0)?;
            let root_revision = revision(self.copy.as_ref().unwrap().root_pin()?)?;
            self.journal
                .as_mut()
                .unwrap()
                .record(Event::DestinationPublished { root_revision })?;
            fault(1)?;
            self.copy
                .as_mut()
                .unwrap()
                .verify_and_sync_controlled(control)?;
            self.prepared.destination_parent.sync_all()?;
            self.journal
                .as_mut()
                .unwrap()
                .record(Event::DestinationSynced)
        }
        /// Complete a single regular-file removal. Every invocation renews live
        /// destination barriers; disk DestinationSynced never suffices. Once a
        /// DeleteIntent is durable, one removal can complete despite late Cancel.
        pub fn finish(&mut self, control: &Control) -> io::Result<()> {
            self.finish_with(control, |_| Ok(()))
        }
        fn finish_with(
            &mut self,
            control: &Control,
            mut fault: impl FnMut(u8) -> io::Result<()>,
        ) -> io::Result<()> {
            if self.phase() == Some(Phase::Completed) {
                return Ok(());
            }
            control.checkpoint()?;
            if !self.published
                || !matches!(
                    self.phase(),
                    Some(Phase::DestinationSynced | Phase::Deleting)
                )
            {
                return Err(invalid("Destino aún no durable"));
            }
            let expected_destination = self
                .journal
                .as_ref()
                .unwrap()
                .state()
                .destination_revision()
                .ok_or(invalid("Falta revisión publicada"))?;
            let copy = self.copy.as_mut().unwrap();
            let parent = &self.prepared.destination_parent;
            let name = &self.prepared.name;
            self.journal.as_mut().unwrap().renew_barriers(|_, _| {
                verify_destination(copy, parent, name, expected_destination, control)
            })?;
            if self.journal.as_ref().unwrap().state().removed() == 0 {
                let stage = self.source_stage.as_ref().unwrap().pin()?;
                let source_inventory = self.copy.as_ref().unwrap().inventory();
                let expected_source = self
                    .journal
                    .as_ref()
                    .unwrap()
                    .state()
                    .source_revision()
                    .ok_or(invalid("Falta revisión de staging"))?;
                let was_unlinked = self.source_unlinked;
                let destination = self.copy.as_ref().unwrap().root_pin()?;
                // The source-presence/absence check is explicit and never maps
                // an unreadable path to missing or deletes a replacement name.
                self.journal.as_mut().unwrap().verify_entry(0, |_, _, _| {
                    if was_unlinked {
                        absent(stage, name)?;
                    } else {
                        source_inventory.verify_controlled(stage, name, control)?;
                        matches(stage, name, source_inventory.root())?;
                        if revision(source_inventory.root())? != expected_source {
                            return Err(invalid("Origen cambió antes de eliminar"));
                        }
                    }
                    matches(parent, name, destination)?;
                    if revision(destination)? != expected_destination {
                        return Err(invalid("Destino cambió antes de eliminar origen"));
                    }
                    Ok(())
                })?;
                if self
                    .journal
                    .as_ref()
                    .unwrap()
                    .state()
                    .pending_delete()
                    .is_none()
                {
                    self.journal
                        .as_mut()
                        .unwrap()
                        .record(Event::DeleteIntent(0))?;
                }
                fault(0)?;
                if !self.source_unlinked {
                    matches(stage, name, source_inventory.root())?;
                    if revision(source_inventory.root())? != expected_source {
                        return Err(invalid("Origen cambió junto a unlink"));
                    }
                    matches(parent, name, destination)?;
                    if revision(destination)? != expected_destination {
                        return Err(invalid("Destino cambió junto a unlink"));
                    }
                    if unsafe { libc::unlinkat(stage.as_raw_fd(), c(name)?.as_ptr(), 0) } != 0 {
                        return Err(io::Error::last_os_error());
                    }
                    self.source_unlinked = true;
                }
                fault(1)?;
                stage.sync_all()?;
                fault(2)?;
                self.journal.as_mut().unwrap().record(Event::Removed(0))?;
                fault(3)?;
            }
            self.source_stage
                .as_mut()
                .unwrap()
                .remove(&self.prepared.source_parent)?;
            self.destination_stage
                .as_mut()
                .unwrap()
                .remove(&self.prepared.destination_parent)?;
            let root = self.copy.as_ref().unwrap().root_pin()?;
            matches(&self.prepared.destination_parent, &self.prepared.name, root)?;
            if revision(root)? != expected_destination {
                return Err(invalid("Destino cambió antes de completar"));
            }
            self.journal.as_mut().unwrap().record(Event::Completed)
        }
        /// Cancel/keep-source path. Never removes a published destination. A
        /// failed journal ACK stops mutations; recovery must reopen/reverify.
        pub fn restore(&mut self) -> io::Result<()> {
            self.restore_with(|_| Ok(()))
        }
        fn restore_with(&mut self, mut fault: impl FnMut(u8) -> io::Result<()>) -> io::Result<()> {
            if let Some(journal) = &self.journal {
                journal.ensure_current()?;
            }
            if self.source_unlinked {
                return Err(invalid(
                    "Origen ya desvinculado; requiere finalizar recuperación",
                ));
            }
            let cancel = AtomicBool::new(false);
            let staged = self.source_rename_returned && !self.source_restore_returned;
            if staged {
                let source = self
                    .source_stage
                    .as_ref()
                    .ok_or(invalid("Sin staging origen"))?
                    .pin()?;
                matches(source, &self.prepared.name, self.inventory().root())?;
                if !self.source_rename_confirmed {
                    if let Some(copy) = &mut self.copy {
                        copy.rebase_source_after_own_rename(source, &self.prepared.name, &cancel)?;
                    } else {
                        self.source_inventory
                            .as_mut()
                            .unwrap()
                            .rebase_after_own_root_rename(source, &self.prepared.name, &cancel)?;
                    }
                    self.source_rename_confirmed = true;
                } else {
                    self.inventory()
                        .verify(source, &self.prepared.name, &cancel)?;
                }
            }
            let source_parent = if staged {
                self.source_stage.as_ref().unwrap().pin()?
            } else {
                &self.prepared.source_parent
            };
            let inventory = self
                .copy
                .as_ref()
                .map(OwnedCopy::inventory)
                .or(self.source_inventory.as_ref())
                .unwrap();
            let name = &self.prepared.name;
            if let Some(journal) = &mut self.journal {
                if journal.state().phase() != Phase::RestoreIntent
                    && journal.state().phase() != Phase::Restored
                {
                    journal
                        .verify_restore(|_, _| inventory.verify(source_parent, name, &cancel))?;
                    journal.record(Event::RestoreIntent)?;
                }
            } else {
                inventory.verify(source_parent, name, &cancel)?;
            }
            if staged {
                rename(
                    self.source_stage.as_ref().unwrap().pin()?,
                    &self.prepared.name,
                    &self.prepared.source_parent,
                    &self.prepared.name,
                )?;
                self.source_restore_returned = true;
                self.source_rename_confirmed = false;
                fault(0)?;
            }
            matches(
                &self.prepared.source_parent,
                &self.prepared.name,
                self.inventory().root(),
            )?;
            if self.source_restore_returned && !self.source_restore_confirmed {
                if let Some(copy) = &mut self.copy {
                    copy.rebase_source_after_own_rename(
                        &self.prepared.source_parent,
                        &self.prepared.name,
                        &cancel,
                    )?;
                } else {
                    self.source_inventory
                        .as_mut()
                        .unwrap()
                        .rebase_after_own_root_rename(
                            &self.prepared.source_parent,
                            &self.prepared.name,
                            &cancel,
                        )?;
                }
                self.source_restore_confirmed = true;
            } else {
                self.inventory().verify(
                    &self.prepared.source_parent,
                    &self.prepared.name,
                    &cancel,
                )?;
            }
            fault(1)?;
            if self.source_restore_returned {
                let stage = self.source_stage.as_ref().unwrap();
                if !stage.removed {
                    stage.pin()?.sync_all()?;
                }
            }
            fault(2)?;
            self.prepared.source_parent.sync_all()?;
            fault(3)?;
            if !self.published
                && let Some(copy) = self.copy.take()
            {
                match copy.cleanup() {
                    Ok(inventory) => self.source_inventory = Some(inventory),
                    Err(failed) => {
                        self.copy = Some(*failed.partial);
                        return Err(failed.cause);
                    }
                }
            }
            if let Some(stage) = &mut self.source_stage {
                stage.remove_with(&self.prepared.source_parent, || fault(4))?;
            }
            if let Some(stage) = &mut self.destination_stage {
                stage.remove_with(&self.prepared.destination_parent, || fault(5))?;
            }
            if let Some(journal) = &mut self.journal
                && journal.state().phase() != Phase::Restored
            {
                journal.record(Event::Restored)?;
            }
            Ok(())
        }
        fn start(&mut self, journals: &Path, control: &Control) -> io::Result<()> {
            control.checkpoint()?;
            self.source_inventory.as_ref().unwrap().verify_controlled(
                &self.prepared.source_parent,
                &self.prepared.name,
                control,
            )?;
            let source_path = current_path(&self.prepared.source_parent)?;
            let destination_path = current_path(&self.prepared.destination_parent)?;
            let manifest = Manifest::new(
                self.prepared.job,
                source_path.as_os_str().as_bytes().to_vec(),
                destination_path.as_os_str().as_bytes().to_vec(),
                identity(&self.prepared.source_parent)?,
                identity(&self.prepared.destination_parent)?,
                vec![copied_entry(
                    &self.source_inventory.as_ref().unwrap().entries()[0],
                )],
            )
            .map_err(invalid)?;
            self.journal = Some(Journal::create(journals, manifest)?);
            self.source_stage = Some(Stage::planned(self.prepared.job, 's'));
            self.source_stage
                .as_mut()
                .unwrap()
                .create(&self.prepared.source_parent)?;
            self.destination_stage = Some(Stage::planned(self.prepared.job, 'd'));
            self.destination_stage
                .as_mut()
                .unwrap()
                .create(&self.prepared.destination_parent)?;
            let source = self.source_stage.as_ref().unwrap();
            let destination = self.destination_stage.as_ref().unwrap();
            self.journal.as_mut().unwrap().record(Event::StageIntent {
                source_stage: source.identity.unwrap(),
                destination_stage: destination.identity.unwrap(),
            })?;
            control.checkpoint()?;
            self.source_inventory.as_ref().unwrap().verify_controlled(
                &self.prepared.source_parent,
                &self.prepared.name,
                control,
            )?;
            absent(&self.prepared.destination_parent, &self.prepared.name)?;
            rename(
                &self.prepared.source_parent,
                &self.prepared.name,
                source.pin()?,
                &self.prepared.name,
            )?;
            self.source_rename_returned = true;
            matches(
                source.pin()?,
                &self.prepared.name,
                self.source_inventory.as_ref().unwrap().root(),
            )?;
            self.source_inventory.as_mut().unwrap().rebase_controlled(
                source.pin()?,
                &self.prepared.name,
                control,
            )?;
            self.source_rename_confirmed = true;
            source.pin()?.sync_all()?;
            self.prepared.source_parent.sync_all()?;
            self.journal.as_mut().unwrap().record(Event::SourceStaged {
                root_revision: self.source_inventory.as_ref().unwrap().entries()[0].revision,
            })?;
            Ok(())
        }
    }
    fn verify_destination(
        copy: &mut OwnedCopy,
        parent: &File,
        name: &OsStr,
        expected: Revision,
        control: &Control,
    ) -> io::Result<()> {
        copy.verify_and_sync_controlled(control)?;
        let pin = copy.root_pin()?;
        matches(parent, name, pin)?;
        if revision(pin)? != expected {
            return Err(invalid("Revisión publicada cambió"));
        }
        pin.sync_all()?;
        parent.sync_all()
    }
    #[cfg(test)]
    mod tests {
        use super::*;
        use std::{
            fs,
            io::Read,
            os::unix::fs::{OpenOptionsExt, PermissionsExt},
        };
        pub(super) struct Fixture {
            pub(super) path: PathBuf,
            pub(super) source: PathBuf,
            pub(super) destination: PathBuf,
            pub(super) journals: PathBuf,
        }
        impl Fixture {
            pub(super) fn new() -> Self {
                let path = crate::persistence::test_dir("exdev-executor");
                let source = path.join("source");
                let destination = path.join("destination");
                let journals = path.join("journals");
                for p in [&source, &destination, &journals] {
                    fs::create_dir(p).unwrap();
                    fs::set_permissions(p, fs::Permissions::from_mode(0o700)).unwrap();
                }
                fs::write(source.join("item"), b"original Unicode \xc3\xb1 payload").unwrap();
                Self {
                    path,
                    source,
                    destination,
                    journals,
                }
            }
            pub(super) fn begin(&self, job: u8) -> Transfer {
                PreparedFileMove::capture_inner(
                    &self.source.join("item"),
                    &self.destination.join("item"),
                    [job; 16],
                    &AtomicBool::new(false),
                    false,
                )
                .unwrap()
                .begin(&self.journals, &Control::default())
                .unwrap()
            }
            fn assert_clean_stages(&self) {
                for p in [&self.source, &self.destination] {
                    assert!(fs::read_dir(p).unwrap().all(|e| {
                        !e.unwrap()
                            .file_name()
                            .as_bytes()
                            .starts_with(b".files-move-")
                    }));
                }
            }
        }
        impl Drop for Fixture {
            fn drop(&mut self) {
                fs::remove_dir_all(&self.path).unwrap();
            }
        }
        #[test]
        fn clipboard_proof_compares_effective_exdev_token_not_path_probe() {
            let f = Fixture::new();
            let cancel = AtomicBool::new(false);
            let selection = crate::selection_source::Source::capture(
                f.source.clone(),
                vec!["item".into()].into(),
                &cancel,
            )
            .unwrap();
            let before = PreparedFileMove::capture_inner(
                &f.source.join("item"),
                &f.destination.join("item"),
                [189; 16],
                &cancel,
                false,
            )
            .unwrap();
            before.verify_selection(&selection, &cancel).unwrap();
            drop(before);
            selection.verify(&cancel).unwrap();
            fs::rename(f.source.join("item"), f.source.join("preserved")).unwrap();
            fs::write(f.source.join("item"), b"substitute").unwrap();
            let token_b = PreparedFileMove::capture_inner(
                &f.source.join("item"),
                &f.destination.join("item"),
                [190; 16],
                &cancel,
                false,
            )
            .unwrap();
            assert!(token_b.verify_selection(&selection, &cancel).is_err());
            assert_eq!(read_without_atime(f.source.join("item")), b"substitute");
            assert!(!f.destination.join("item").exists());
            f.assert_clean_stages();
        }
        fn read_without_atime(path: impl AsRef<Path>) -> Vec<u8> {
            let mut file = fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOATIME)
                .open(path)
                .unwrap();
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes).unwrap();
            bytes
        }
        fn injected() -> io::Error {
            io::Error::other("injected sync failure")
        }

        #[test]
        fn begin_copy_publish_finish_preserves_source_until_durable_delete() {
            let f = Fixture::new();
            let original = fs::read(f.source.join("item")).unwrap();
            let source_id =
                identity(&at(&directory(&f.source).unwrap(), OsStr::new("item")).unwrap()).unwrap();
            let control = Control::default();
            let mut transfer = f.begin(1);
            assert_eq!(transfer.phase(), Some(Phase::SourceStaged));
            assert!(!f.source.join("item").exists());
            assert_eq!(
                read_without_atime(transfer.source_location().unwrap()),
                original
            );
            let mut bytes = 0;
            transfer.copy_to_staging(&control, |n| bytes += n).unwrap();
            assert_eq!(bytes, original.len() as u64);
            assert!(!f.destination.join("item").exists());
            transfer.publish(&control).unwrap();
            assert_eq!(transfer.phase(), Some(Phase::DestinationSynced));
            assert_eq!(read_without_atime(f.destination.join("item")), original);
            assert_eq!(identity(transfer.inventory().root()).unwrap(), source_id);
            assert_eq!(
                read_without_atime(transfer.source_location().unwrap()),
                original
            );
            transfer.finish(&control).unwrap();
            transfer.finish(&control).unwrap();
            assert_eq!(transfer.phase(), Some(Phase::Completed));
            assert!(transfer.source_unlinked());
            f.assert_clean_stages();
            drop(transfer);
            let opened = Journal::open(&f.journals, [1; 16]).unwrap();
            assert_eq!(opened.state().phase(), Phase::Completed);
        }

        #[test]
        fn cancel_and_collision_restore_source_without_claiming_foreign_destination() {
            let f = Fixture::new();
            let mut transfer = f.begin(2);
            let control = Control::default();
            control.cancel();
            assert_eq!(
                transfer
                    .copy_to_staging(&control, |_| {})
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::Interrupted
            );
            transfer.restore().unwrap();
            assert_eq!(transfer.phase(), Some(Phase::Restored));
            assert!(f.source.join("item").exists());
            f.assert_clean_stages();
            drop(transfer);
            let mut transfer = f.begin(3);
            transfer
                .copy_to_staging(&Control::default(), |_| {})
                .unwrap();
            fs::write(f.destination.join("item"), b"foreign").unwrap();
            assert_eq!(
                transfer.publish(&Control::default()).unwrap_err().kind(),
                io::ErrorKind::AlreadyExists
            );
            transfer.restore().unwrap();
            assert_eq!(fs::read(f.destination.join("item")).unwrap(), b"foreign");
            assert!(f.source.join("item").exists());
            f.assert_clean_stages();
        }

        #[test]
        fn restore_after_publication_keeps_both_copies_and_retries_rename_sync_failures() {
            for fail in 0..6 {
                let f = Fixture::new();
                let mut transfer = f.begin(10 + fail);
                transfer
                    .copy_to_staging(&Control::default(), |_| {})
                    .unwrap();
                transfer.publish(&Control::default()).unwrap();
                assert!(
                    transfer
                        .restore_with(|phase| if phase == fail {
                            Err(injected())
                        } else {
                            Ok(())
                        })
                        .is_err()
                );
                assert!(transfer.source_restore_returned);
                let source_id = identity(
                    &at(&transfer.prepared.source_parent, &transfer.prepared.name).unwrap(),
                )
                .unwrap();
                assert_eq!(source_id, identity(transfer.inventory().root()).unwrap());
                transfer.restore().unwrap();
                assert_eq!(transfer.phase(), Some(Phase::Restored));
                assert_eq!(
                    fs::read(f.source.join("item")).unwrap(),
                    fs::read(f.destination.join("item")).unwrap()
                );
                f.assert_clean_stages();
            }
        }

        #[test]
        fn staged_or_published_content_change_prevents_source_deletion() {
            let f = Fixture::new();
            let mut transfer = f.begin(20);
            transfer
                .copy_to_staging(&Control::default(), |_| {})
                .unwrap();
            transfer.publish(&Control::default()).unwrap();
            fs::write(f.destination.join("item"), b"changed").unwrap();
            assert!(transfer.finish(&Control::default()).is_err());
            assert!(!transfer.source_unlinked());
            assert!(transfer.source_location().unwrap().exists());
            transfer.restore().unwrap();
            assert!(f.source.join("item").exists());
            assert_eq!(fs::read(f.destination.join("item")).unwrap(), b"changed");
        }

        #[test]
        fn finish_retry_after_delete_intent_unlink_sync_and_removed_ack() {
            for fail in 0..4 {
                let f = Fixture::new();
                let mut transfer = f.begin(50 + fail);
                transfer
                    .copy_to_staging(&Control::default(), |_| {})
                    .unwrap();
                transfer.publish(&Control::default()).unwrap();
                assert!(
                    transfer
                        .finish_with(&Control::default(), |point| if point == fail {
                            Err(injected())
                        } else {
                            Ok(())
                        })
                        .is_err()
                );
                assert_eq!(transfer.source_unlinked(), fail != 0);
                transfer.finish(&Control::default()).unwrap();
                assert_eq!(transfer.phase(), Some(Phase::Completed));
                f.assert_clean_stages();
            }
        }
        #[test]
        fn restore_after_delete_intent_before_unlink_renews_source_proof() {
            let f = Fixture::new();
            let mut transfer = f.begin(60);
            transfer
                .copy_to_staging(&Control::default(), |_| {})
                .unwrap();
            transfer.publish(&Control::default()).unwrap();
            assert!(
                transfer
                    .finish_with(&Control::default(), |point| if point == 0 {
                        Err(injected())
                    } else {
                        Ok(())
                    })
                    .is_err()
            );
            assert!(!transfer.source_unlinked());
            transfer.restore().unwrap();
            assert_eq!(transfer.phase(), Some(Phase::Restored));
            assert!(f.source.join("item").exists());
            assert!(f.destination.join("item").exists());
            f.assert_clean_stages();
        }
        #[test]
        #[ignore = "explicit functional fixture on an existing second filesystem; no mount or metrics"]
        fn real_exdev_transfer_on_private_runtime_fixture() {
            let f = Fixture::new();
            let runtime = std::env::var_os("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR required");
            let target = PathBuf::from(runtime).join(f.path.file_name().unwrap());
            fs::create_dir(&target).unwrap();
            fs::set_permissions(&target, fs::Permissions::from_mode(0o700)).unwrap();
            let source_device = fs::metadata(&f.source).unwrap().dev();
            let destination_device = fs::metadata(&target).unwrap().dev();
            assert_ne!(source_device, destination_device);
            eprintln!(
                "REAL_EXDEV_DEVICES source_st_dev={source_device} destination_st_dev={destination_device}"
            );
            let original = fs::read(f.source.join("item")).unwrap();
            let prepared = PreparedFileMove::capture(
                &f.source.join("item"),
                &target.join("item"),
                [70; 16],
                &AtomicBool::new(false),
            )
            .unwrap();
            let mut transfer = prepared.begin(&f.journals, &Control::default()).unwrap();
            transfer
                .copy_to_staging(&Control::default(), |_| {})
                .unwrap();
            transfer.publish(&Control::default()).unwrap();
            assert_eq!(read_without_atime(target.join("item")), original);
            assert!(transfer.source_location().unwrap().exists());
            transfer.finish(&Control::default()).unwrap();
            assert_eq!(transfer.phase(), Some(Phase::Completed));
            assert!(!f.source.join("item").exists());
            assert_eq!(read_without_atime(target.join("item")), original);
            drop(transfer);
            fs::remove_dir_all(target).unwrap();
        }

        #[test]
        fn reopen_publish_rename_without_ack_and_finish_with_fresh_barriers() {
            for fail in 0..2 {
                let f = Fixture::new();
                let mut transfer = f.begin(80 + fail);
                transfer
                    .copy_to_staging(&Control::default(), |_| {})
                    .unwrap();
                assert!(
                    transfer
                        .publish_with(&Control::default(), |point| if point == fail {
                            Err(injected())
                        } else {
                            Ok(())
                        })
                        .is_err()
                );
                assert!(f.destination.join("item").exists());
                drop(transfer);
                let mut recovery = Recovery::open(&f.journals, [80 + fail; 16]).unwrap();
                assert!(!recovery.barriers_current());
                recovery.finish(&Control::default()).unwrap();
                assert_eq!(recovery.phase(), Phase::Completed);
                assert!(!f.source.join("item").exists());
                f.assert_clean_stages();
            }
        }
        #[test]
        fn reopen_delete_intent_with_source_present_or_already_unlinked() {
            for fail in 0..4 {
                let f = Fixture::new();
                let mut transfer = f.begin(90 + fail);
                transfer
                    .copy_to_staging(&Control::default(), |_| {})
                    .unwrap();
                transfer.publish(&Control::default()).unwrap();
                assert!(
                    transfer
                        .finish_with(&Control::default(), |point| if point == fail {
                            Err(injected())
                        } else {
                            Ok(())
                        })
                        .is_err()
                );
                drop(transfer);
                let mut recovery = Recovery::open(&f.journals, [90 + fail; 16]).unwrap();
                assert!(!recovery.barriers_current());
                recovery.finish(&Control::default()).unwrap();
                assert_eq!(recovery.phase(), Phase::Completed);
                f.assert_clean_stages();
            }
        }
        #[test]
        fn reopen_restore_after_rename_and_each_sync_or_stage_cleanup_gap() {
            for fail in 0..6 {
                let f = Fixture::new();
                let mut transfer = f.begin(100 + fail);
                transfer
                    .copy_to_staging(&Control::default(), |_| {})
                    .unwrap();
                transfer.publish(&Control::default()).unwrap();
                assert!(
                    transfer
                        .restore_with(|point| if point == fail {
                            Err(injected())
                        } else {
                            Ok(())
                        })
                        .is_err()
                );
                drop(transfer);
                let mut recovery = Recovery::open(&f.journals, [100 + fail; 16]).unwrap();
                recovery.restore(&Control::default()).unwrap();
                assert_eq!(recovery.phase(), Phase::Restored);
                assert_eq!(
                    read_without_atime(f.source.join("item")),
                    read_without_atime(f.destination.join("item"))
                );
                f.assert_clean_stages();
            }
        }
        #[test]
        fn recovery_never_unlinks_new_source_or_destination_occupants() {
            for replace_source in [true, false] {
                let f = Fixture::new();
                let mut transfer = f.begin(110);
                transfer
                    .copy_to_staging(&Control::default(), |_| {})
                    .unwrap();
                transfer.publish(&Control::default()).unwrap();
                assert!(
                    transfer
                        .finish_with(&Control::default(), |point| if point == 1 {
                            Err(injected())
                        } else {
                            Ok(())
                        })
                        .is_err()
                );
                let stage_path = f.source.join(&transfer.source_stage.as_ref().unwrap().name);
                drop(transfer);
                let foreign = if replace_source {
                    stage_path.join("item")
                } else {
                    f.destination.join("item")
                };
                if !replace_source {
                    fs::rename(&foreign, f.destination.join("preserved")).unwrap();
                }
                fs::write(&foreign, b"foreign occupant").unwrap();
                let mut recovery = Recovery::open(&f.journals, [110; 16]).unwrap();
                assert!(recovery.finish(&Control::default()).is_err());
                assert_eq!(fs::read(&foreign).unwrap(), b"foreign occupant");
                assert_eq!(recovery.phase(), Phase::Deleting);
            }
        }
        #[test]
        fn recovery_restore_collision_keeps_both_original_and_foreign_name() {
            let f = Fixture::new();
            let transfer = f.begin(120);
            let staged_path = transfer.source_location().unwrap();
            drop(transfer);
            fs::write(f.source.join("item"), b"foreign").unwrap();
            let mut recovery = Recovery::open(&f.journals, [120; 16]).unwrap();
            assert!(recovery.restore(&Control::default()).is_err());
            assert!(staged_path.exists());
            assert_eq!(fs::read(f.source.join("item")).unwrap(), b"foreign");
        }

        #[test]
        #[ignore = "private subprocess entry; run only through real_recovery_across_processes"]
        fn recovery_subprocess_actor() {
            let source = PathBuf::from(
                std::env::var_os("FILES_EXDEV_FIXTURE_SOURCE").expect("private fixture source"),
            );
            let destination = PathBuf::from(
                std::env::var_os("FILES_EXDEV_FIXTURE_DESTINATION")
                    .expect("private fixture destination"),
            );
            let journals = PathBuf::from(
                std::env::var_os("FILES_EXDEV_FIXTURE_JOURNALS").expect("private fixture journal"),
            );
            let action = std::env::var("FILES_EXDEV_FIXTURE_ACTION").unwrap();
            let control = Control::default();
            if action == "finish" || action == "restore" {
                let mut recovery = Recovery::open(&journals, [150; 16]).unwrap();
                assert!(!recovery.barriers_current());
                if action == "finish" {
                    recovery.finish(&control).unwrap();
                    assert_eq!(recovery.phase(), Phase::Completed);
                } else {
                    recovery.restore(&control).unwrap();
                    assert_eq!(recovery.phase(), Phase::Restored);
                }
                eprintln!(
                    "NEW_PROCESS_RECOVERY_ACK action={action} phase={:?}",
                    recovery.phase()
                );
                return;
            }
            assert_ne!(
                fs::metadata(&source).unwrap().dev(),
                fs::metadata(&destination).unwrap().dev()
            );
            let prepared = PreparedFileMove::capture(
                &source.join("item"),
                &destination.join("item"),
                [150; 16],
                &AtomicBool::new(false),
            )
            .unwrap();
            let mut transfer = prepared.begin(&journals, &control).unwrap();
            transfer.copy_to_staging(&control, |_| {}).unwrap();
            if action == "publish-gap" {
                transfer
                    .publish_with(&control, |point| {
                        if point == 0 {
                            std::process::exit(73);
                        }
                        Ok(())
                    })
                    .unwrap();
            } else {
                transfer.publish(&control).unwrap();
                if action == "delete-gap" {
                    transfer
                        .finish_with(&control, |point| {
                            if point == 1 {
                                std::process::exit(73);
                            }
                            Ok(())
                        })
                        .unwrap();
                } else if action == "restore-gap" {
                    transfer
                        .restore_with(|point| {
                            if point == 0 {
                                std::process::exit(73);
                            }
                            Ok(())
                        })
                        .unwrap();
                } else {
                    panic!("unknown private fixture action");
                }
            }
            panic!("fixture did not reach abrupt exit point");
        }
        #[test]
        #[ignore = "explicit two-process functional fixture on second filesystem; no mount/metrics"]
        fn real_recovery_across_processes() {
            use std::{
                process::{Command, Stdio},
                time::{Duration, Instant},
            };
            fn child(f: &Fixture, target: &Path, action: &str) -> std::process::Output {
                let mut child = Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "exdev_move::linux::tests::recovery_subprocess_actor",
                        "--ignored",
                        "--nocapture",
                    ])
                    .env("FILES_EXDEV_FIXTURE_SOURCE", &f.source)
                    .env("FILES_EXDEV_FIXTURE_DESTINATION", target)
                    .env("FILES_EXDEV_FIXTURE_JOURNALS", &f.journals)
                    .env("FILES_EXDEV_FIXTURE_ACTION", action)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .unwrap();
                let deadline = Instant::now() + Duration::from_secs(15);
                while child.try_wait().unwrap().is_none() {
                    if Instant::now() >= deadline {
                        let _ = child.kill();
                        let _ = child.wait();
                        panic!("private child timed out");
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                child.wait_with_output().unwrap()
            }
            for gap in ["publish-gap", "delete-gap", "restore-gap"] {
                let f = Fixture::new();
                let runtime = PathBuf::from(
                    std::env::var_os("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR required"),
                );
                let target = runtime.join(f.path.file_name().unwrap());
                fs::create_dir(&target).unwrap();
                fs::set_permissions(&target, fs::Permissions::from_mode(0o700)).unwrap();
                let source_device = fs::metadata(&f.source).unwrap().dev();
                let destination_device = fs::metadata(&target).unwrap().dev();
                assert_ne!(source_device, destination_device);
                let original = fs::read(f.source.join("item")).unwrap();
                let first = child(&f, &target, gap);
                assert_eq!(
                    first.status.code(),
                    Some(73),
                    "{}",
                    String::from_utf8_lossy(&first.stderr)
                );
                let action = if gap == "restore-gap" {
                    "restore"
                } else {
                    "finish"
                };
                let recovered = child(&f, &target, action);
                assert!(
                    recovered.status.success(),
                    "{}",
                    String::from_utf8_lossy(&recovered.stderr)
                );
                assert_eq!(fs::read(target.join("item")).unwrap(), original);
                if gap == "restore-gap" {
                    assert_eq!(fs::read(f.source.join("item")).unwrap(), original);
                } else {
                    assert!(!f.source.join("item").exists());
                }
                assert!(fs::read_dir(&f.source).unwrap().all(|e| {
                    !e.unwrap()
                        .file_name()
                        .as_bytes()
                        .starts_with(b".files-move-")
                }));
                assert!(fs::read_dir(&target).unwrap().all(|e| {
                    !e.unwrap()
                        .file_name()
                        .as_bytes()
                        .starts_with(b".files-move-")
                }));
                eprintln!(
                    "REAL_EXDEV_PROCESS_RECOVERY gap={gap} source_st_dev={source_device} destination_st_dev={destination_device} first_exit=73 recovery_exit=0"
                );
                fs::remove_dir_all(target).unwrap();
            }
        }

        #[test]
        fn reviewed_intent_rejects_same_job_replacement_and_catalog_fd_stays_original() {
            let a = Fixture::new();
            let b = Fixture::new();
            let transfer_a = a.begin(170);
            let proof_a = transfer_a.intent_proof().unwrap().unwrap();
            drop(transfer_a);
            let transfer_b = b.begin(170);
            drop(transfer_b);
            let pinned = directory(&a.journals).unwrap();
            fs::rename(&a.journals, a.path.join("journals-old")).unwrap();
            fs::create_dir(&a.journals).unwrap();
            fs::set_permissions(&a.journals, fs::Permissions::from_mode(0o700)).unwrap();
            let journal_name = fs::read_dir(&b.journals)
                .unwrap()
                .next()
                .unwrap()
                .unwrap()
                .file_name();
            fs::copy(
                b.journals.join(&journal_name),
                a.journals.join(&journal_name),
            )
            .unwrap();
            let anchored = Journal::open_at_single_file(&pinned, [170; 16]).unwrap();
            anchored.require_intent(&proof_a).unwrap();
            drop(anchored);
            let replacement = Journal::open_single_file(&a.journals, [170; 16]).unwrap();
            assert!(replacement.require_intent(&proof_a).is_err());
            drop(replacement);
            let error = Recovery::open_reviewed(&a.journals, [170; 16], &proof_a).unwrap_err();
            assert!(error.to_string().contains("intención"));
            for f in [&a, &b] {
                let stage = Stage::planned([170; 16], 's');
                assert!(f.source.join(&stage.name).join("item").exists());
                assert!(!f.destination.join("item").exists());
            }
        }
        #[test]
        fn single_file_recovery_wire_cap_preserves_large_journal() {
            let f = Fixture::new();
            let transfer = f.begin(171);
            drop(transfer);
            let path = fs::read_dir(&f.journals)
                .unwrap()
                .next()
                .unwrap()
                .unwrap()
                .path();
            let file = fs::OpenOptions::new().write(true).open(&path).unwrap();
            file.set_len(crate::move_journal::MAX_SINGLE_FILE_JOURNAL as u64 + 1)
                .unwrap();
            assert!(Recovery::open(&f.journals, [171; 16]).is_err());
            assert_eq!(fs::metadata(&path).unwrap().len(), 65537);
        }

        #[test]
        fn partial_stage_creation_retains_provenance_and_uncreated_is_foreign() {
            let f = Fixture::new();
            let parent = directory(&f.source).unwrap();
            let mut planned = Stage::planned([30; 16], 's');
            fs::create_dir(f.source.join(&planned.name)).unwrap();
            assert!(planned.create(&parent).is_err());
            assert!(!planned.mkdir_returned);
            planned.remove(&parent).unwrap();
            assert!(f.source.join(&planned.name).exists());
            for fail in 0..4 {
                let mut stage = Stage::planned([31 + fail; 16], 's');
                assert!(
                    stage
                        .create_with(&parent, |point| if point == fail {
                            Err(injected())
                        } else {
                            Ok(())
                        })
                        .is_err()
                );
                assert!(stage.mkdir_returned);
                assert_eq!(stage.file.is_some(), fail != 0);
                assert_eq!(stage.confirmed, fail >= 2);
                if stage.confirmed {
                    stage.remove(&parent).unwrap();
                } else {
                    assert!(stage.remove(&parent).is_err());
                    assert!(f.source.join(&stage.name).exists());
                }
            }
        }

        #[test]
        fn stage_unlink_then_sync_failure_retries_without_touching_replacement() {
            let f = Fixture::new();
            let parent = directory(&f.source).unwrap();
            let mut stage = Stage::planned([40; 16], 's');
            stage.create(&parent).unwrap();
            assert!(stage.remove_with(&parent, || Err(injected())).is_err());
            assert!(stage.removed);
            fs::create_dir(f.source.join(&stage.name)).unwrap();
            stage.remove(&parent).unwrap();
            assert!(f.source.join(&stage.name).is_dir());
        }
    }
}

#[cfg(target_os = "linux")]
pub use linux::{FailedTransfer, PreparedFileMove, Recovery, Transfer};
