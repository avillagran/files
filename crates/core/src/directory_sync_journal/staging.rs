use super::*;
use crate::directory_sync_store::StagingWorkspace;
use std::os::unix::ffi::OsStrExt;
impl State {
    pub(crate) fn validate_staging(
        &self,
        s: &Schema,
        scopes: &[StagingWorkspace<'_>],
    ) -> io::Result<[u8; 32]> {
        if self.phase != Phase::Staging
            || self.workspace_pending.is_some()
            || self.workspace_synced != 0
            || self.workspaces.len() != s.transactions.len()
            || self.verified.len() != s.nodes.len()
            || scopes.len() != s.transactions.len()
        {
            return Err(bad("Barrera staging requiere todos los nodos y workspaces"));
        }
        let mut h = Sha256::new();
        h.update(b"Files:sync-staging-barrier:v1\0");
        h.update(s.proof.encode());
        for (t, scope) in scopes.iter().enumerate() {
            let spec = &s.transactions[t];
            let entries = scope.inventory.entries();
            let mounts = scope
                .inventory
                .mounts()
                .ok_or_else(|| bad("Staging sin montajes"))?;
            if entries.len() != spec.nodes.len() + 1 || mounts.len() != entries.len() {
                return Err(bad("Workspace incompleto o con nombres extra"));
            }
            let root = &entries[0];
            if (Object {
                identity: root.identity,
                mount: mounts[0],
            }) != self.workspaces[t]
                || root.relative.as_slice() != scope.name.as_bytes()
                || root.identity.kind != Kind::Directory
                || root.revision.mode != 0o700
                || root.revision.uid != unsafe { libc::geteuid() }
            {
                return Err(bad("Workspace staging no corresponde o no es privado"));
            }
            let expected: HashMap<Vec<u8>, usize> = spec
                .nodes
                .clone()
                .map(|i| {
                    let mut p = b"new".to_vec();
                    if !s.nodes[i].relative.is_empty() {
                        p.push(b'/');
                        p.extend_from_slice(&s.nodes[i].relative);
                    }
                    (p, i)
                })
                .collect();
            h.update((t as u32).to_le_bytes());
            h.update((root.relative.len() as u32).to_le_bytes());
            h.update(&root.relative);
            for (index, e) in entries.iter().enumerate() {
                let relative = if index == 0 {
                    b"".as_slice()
                } else {
                    e.relative
                        .strip_prefix(root.relative.as_slice())
                        .and_then(|p| p.strip_prefix(b"/"))
                        .ok_or_else(|| bad("Prefijo staging inválido"))?
                };
                let o = Observation {
                    object: Object {
                        identity: e.identity,
                        mount: mounts[index],
                    },
                    revision: e.revision,
                    content: e.content_digest,
                    metadata: e.metadata_digest,
                };
                if index != 0 {
                    let i = *expected
                        .get(relative)
                        .ok_or_else(|| bad("Nombre staging inesperado"))?;
                    if o != self.verified[i] {
                        return Err(bad("Staging real no coincide con nodos verificados"));
                    }
                }
                h.update((relative.len() as u32).to_le_bytes());
                h.update(relative);
                let mut bytes = Vec::with_capacity(160);
                put_observation(&mut bytes, o);
                h.update(bytes);
            }
        }
        Ok(h.finalize().into())
    }
}
impl Schema {
    pub(crate) fn batch_observation(
        &self,
        node: usize,
        inventory: &crate::exdev_source::Inventory,
        name: &std::ffi::OsStr,
    ) -> io::Result<Observation> {
        let n = self
            .nodes
            .get(node)
            .ok_or_else(|| bad("Nodo de lote fuera de rango"))?;
        let mut key = name.as_bytes().to_vec();
        key.extend_from_slice(b"/new");
        if !n.relative.is_empty() {
            key.push(b'/');
            key.extend_from_slice(&n.relative);
        }
        let i = inventory
            .entries()
            .binary_search_by(|e| e.relative.cmp(&key))
            .map_err(|_| bad("Nodo creado ausente del inventario"))?;
        let e = &inventory.entries()[i];
        let mount = *inventory
            .mounts()
            .and_then(|m| m.get(i))
            .ok_or_else(|| bad("Montaje de lote ausente"))?;
        Ok(Observation {
            object: Object {
                identity: e.identity,
                mount,
            },
            revision: e.revision,
            content: e.content_digest,
            metadata: e.metadata_digest,
        })
    }
}
