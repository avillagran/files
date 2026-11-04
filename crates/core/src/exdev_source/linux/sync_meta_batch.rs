use super::*;
use crate::directory_sync_journal::{Object, Observation};
use crate::directory_sync_metadata::Values;
pub(crate) struct MetadataBatch<'a> {
    scopes: Vec<NamespaceScope<'a>>,
    candidates: Vec<Inventory>,
    scope: usize,
    index: usize,
    read: u64,
    changed: Option<Identity>,
}
impl<'a> MetadataBatch<'a> {
    pub(crate) fn prepare(
        scopes: Vec<NamespaceScope<'a>>,
        scope: usize,
        index: usize,
        read: u64,
        cancel: &AtomicBool,
        control: Option<&Control>,
    ) -> io::Result<Self> {
        if read > crate::directory_compare::MAX_READ || scope >= scopes.len() {
            return Err(invalid("Allowance/scope metadata inválido"));
        }
        let mut spent = 0;
        let mut candidates = Vec::with_capacity(scopes.len());
        for s in &scopes {
            let (copy, n) =
                s.inventory
                    .fork_metadata(s.parent, s.name, read - spent, cancel, control)?;
            spent += n;
            candidates.push(copy);
        }
        if spent > read - spent {
            return Err(invalid("Allowance metadata no cubre las dos pasadas"));
        }
        if index >= candidates[scope].entries.len() {
            return Err(invalid("Entrada metadata ausente"));
        }
        Ok(Self {
            scopes,
            candidates,
            scope,
            index,
            read: spent,
            changed: None,
        })
    }
    pub(crate) fn target(&self) -> (&Inventory, &File, &OsStr, usize) {
        let s = &self.scopes[self.scope];
        (s.inventory, s.parent, s.name, self.index)
    }
    pub(crate) fn observation(&self) -> Observation {
        let c = &self.candidates[self.scope];
        let e = &c.entries[self.index];
        Observation {
            object: Object {
                identity: e.identity,
                mount: c.mounts.as_ref().expect("sync")[self.index],
            },
            revision: e.revision,
            content: e.content_digest,
            metadata: e.metadata_digest,
        }
    }
    pub(crate) fn patch(
        &mut self,
        applied: &AppliedStageMetadata,
        desired: &Values,
    ) -> io::Result<()> {
        let before = self.observation();
        if before.object.identity != applied.identity()
            || before.object.mount != applied.mount()
            || applied.digest() != desired.evidence_digest()
        {
            return Err(invalid("Recibo metadata no corresponde"));
        }
        self.changed = Some(applied.identity());
        for c in &mut self.candidates {
            for (e, mount) in c.entries.iter_mut().zip(c.mounts.as_ref().expect("sync")) {
                if e.identity == applied.identity() && *mount == applied.mount() {
                    e.revision = applied.revision();
                    e.access = AccessTime {
                        seconds: desired.access().seconds,
                        nanos: desired.access().nanos,
                    };
                    e.metadata_digest = applied.digest();
                }
            }
        }
        Ok(())
    }
    fn roots(&self, cancel: &AtomicBool, control: Option<&Control>) -> io::Result<()> {
        for (s, c) in self.scopes.iter().zip(&self.candidates) {
            check_control(cancel, control)?;
            let e = &c.entries[0];
            stable(s.parent, s.name, &s.inventory.root, e.identity, e.revision)?;
            if mount_id(&s.inventory.root, None)? != c.mounts.as_ref().expect("sync")[0]
                || mount_id(s.parent, Some(s.name))? != c.mounts.as_ref().expect("sync")[0]
            {
                return Err(invalid("Raíz metadata cambió de montaje"));
            }
        }
        if let Some(identity) = self.changed {
            for (s, c) in self.scopes.iter().zip(&self.candidates) {
                for (index, _) in c
                    .entries
                    .iter()
                    .enumerate()
                    .filter(|(_, e)| e.identity == identity)
                {
                    check_control(cancel, control)?;
                    c.pin_sync_path(s.parent, s.name, index, cancel)?;
                }
            }
        }
        Ok(())
    }
    pub(crate) fn finish(
        mut self,
        limit: u64,
        cancel: &AtomicBool,
        control: Option<&Control>,
        mut confirm: impl FnMut(Option<Observation>) -> io::Result<()>,
    ) -> io::Result<u64> {
        for (s, c) in self.scopes.iter().zip(&self.candidates) {
            self.read += c.verify_staging(s.parent, s.name, limit - self.read, cancel, control)?;
        }
        self.roots(cancel, control)?;
        let c = &self.candidates[self.scope];
        let s = &self.scopes[self.scope];
        let e = &c.entries[self.index];
        let (p, pin, name) = c.pin_sync_path(s.parent, s.name, self.index, cancel)?;
        if e.identity.kind != Kind::Symlink {
            let fd = data(&p, &name, e.identity, e.identity.kind == Kind::Directory)?;
            fd.sync_all()?;
            stable(&p, &name, &fd, e.identity, e.revision)?;
        }
        let end = e
            .relative
            .iter()
            .rposition(|&b| b == b'/')
            .ok_or_else(|| invalid("Padre metadata ausente"))?;
        let parent_path = &e.relative[..end];
        let i = c
            .entries
            .binary_search_by(|v| v.relative.as_slice().cmp(parent_path))
            .map_err(|_| invalid("Padre metadata no registrado"))?;
        let (gp, _, pn) = c.pin_sync_path(s.parent, s.name, i, cancel)?;
        let fd = data(&gp, &pn, c.entries[i].identity, true)?;
        fd.sync_all()?;
        stable(&p, &name, &pin, e.identity, e.revision)?;
        self.roots(cancel, control)?;
        confirm(Some(self.observation()))?;
        self.roots(cancel, control)?;
        confirm(None)?; // All fallible owner checks precede the all-owner commit.
        for (s, c) in self.scopes.iter_mut().zip(&mut self.candidates) {
            s.inventory.entries = std::mem::take(&mut c.entries);
            s.inventory.mounts = c.mounts.take();
        }
        Ok(self.read)
    }
}
