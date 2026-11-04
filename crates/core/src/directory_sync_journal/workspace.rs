use super::*;
impl State {
    pub fn placement_seal(&self) -> Option<[u8; 32]> {
        self.placement_seal
    }
    pub fn workspace_create_guard(&self) -> Option<(u32, [u8; 32])> {
        self.workspace_create
    }
    pub(crate) fn workspace_spec(
        &self,
        s: &Schema,
        index: u32,
        parent: Object,
        name: &[u8],
    ) -> io::Result<[u8; 32]> {
        if self.phase != Phase::Staging || index as usize >= s.transactions.len() {
            return Err(bad("Transacción de workspace inválida"));
        }
        s.staging_admission()?;
        s.workspace_binding(index, parent, name)
    }
    /// Validate the whole batch while no workspace intent has started.
    pub(crate) fn workspace_batch(&self, s: &Schema, count: usize) -> io::Result<()> {
        if count != s.transactions.len()
            || !self.workspaces.is_empty()
            || self.workspace_create.is_some()
            || self.workspace_pending.is_some()
            || self.placement_seal.is_some()
            || self.phase != Phase::Staging
        {
            return Err(bad("Lote de workspaces incompleto o ya iniciado"));
        }
        Ok(())
    }
}

impl Schema {
    pub(crate) fn workspace_count(&self) -> usize {
        self.transactions.len()
    }
    pub(crate) fn workspace_binding(
        &self,
        index: u32,
        parent: Object,
        name: &[u8],
    ) -> io::Result<[u8; 32]> {
        if index as usize >= self.transactions.len() {
            return Err(bad("Índice de placement inválido"));
        }
        let t = &self.transactions[index as usize];
        if parent.identity.kind != Kind::Directory
            || parent.identity.device != t.device
            || parent.mount != t.mount
            || parent.identity.inode == 0
            || name.is_empty()
            || name.len() > 255
            || name.contains(&0)
            || name.contains(&b'/')
            || name == b"."
            || name == b".."
        {
            return Err(bad("Placement de workspace inválido"));
        }
        let mut h = Sha256::new();
        h.update(b"Files:workspace-create:v1\0");
        h.update(self.proof.encode());
        h.update(&self.proof.encode()[136..168]);
        h.update(index.to_le_bytes());
        let mut bytes = Vec::new();
        put_object(&mut bytes, parent);
        h.update(bytes);
        h.update((name.len() as u32).to_le_bytes());
        h.update(name);
        Ok(h.finalize().into())
    }
}
