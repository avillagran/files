use super::*;
use crate::exdev_source::NamespaceScope;
use std::os::unix::ffi::OsStrExt;
pub(crate) struct MetadataSpec {
    pub scope: usize,
    pub entry: usize,
    pub before: Observation,
    pub seal: [u8; 32],
}
fn observed(e: &crate::move_manifest::Entry, mount: u64) -> Observation {
    Observation {
        object: Object {
            identity: e.identity,
            mount,
        },
        revision: e.revision,
        content: e.content_digest,
        metadata: e.metadata_digest,
    }
}
impl State {
    pub fn stage_metadata_guard(&self) -> Option<(u32, Observation, [u8; 32])> {
        self.stage_meta
    }
    pub fn stage_metadata_done(&self) -> usize {
        self.metadata_done
    }
    pub(crate) fn metadata_spec(
        &self,
        s: &Schema,
        node: u32,
        scopes: &[NamespaceScope<'_>],
    ) -> io::Result<MetadataSpec> {
        let n = s
            .nodes
            .get(node as usize)
            .ok_or_else(|| bad("Nodo metadata ausente"))?;
        if scopes.len() != s.transactions.len() || self.created.len() != s.nodes.len() {
            return Err(bad("Se requieren TODOS los workspaces creados"));
        }
        let mut h = Sha256::new();
        h.update(b"Files:stage-metadata-intent:v1\0");
        h.update(s.proof.encode());
        h.update(node.to_le_bytes());
        h.update(n.metadata);
        let mut selected = None;
        let mut aliases = 0;
        for (t, scope) in scopes.iter().enumerate() {
            let entries = scope.inventory.entries();
            let mounts = scope
                .inventory
                .mounts()
                .ok_or_else(|| bad("Workspace sin montajes"))?;
            let spec = &s.transactions[t];
            if entries.len() != spec.nodes.len() + 1 || mounts.len() != entries.len() {
                return Err(bad("Workspace metadata incompleto o con extras"));
            }
            let root = &entries[0];
            if root.relative != scope.name.as_bytes()
                || observed(root, mounts[0]).object != self.workspaces[t]
                || root.revision.mode != 0o700
                || root.revision.uid != unsafe { libc::geteuid() }
            {
                return Err(bad("Raíz metadata no corresponde"));
            }
            h.update((t as u32).to_le_bytes());
            h.update((root.relative.len() as u32).to_le_bytes());
            h.update(&root.relative);
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
            for (index, e) in entries.iter().enumerate() {
                let o = observed(e, mounts[index]);
                let relative = if index == 0 {
                    b"".as_slice()
                } else {
                    e.relative
                        .strip_prefix(root.relative.as_slice())
                        .and_then(|p| p.strip_prefix(b"/"))
                        .ok_or_else(|| bad("Prefijo metadata inválido"))?
                };
                if index != 0 {
                    let i = *expected
                        .get(relative)
                        .ok_or_else(|| bad("Nombre metadata desconocido"))?;
                    if o.object != self.created[i] {
                        return Err(bad("Identidad staged no corresponde"));
                    }
                    let n = &s.nodes[i];
                    if o.content != n.content
                        || o.revision.uid != n.meta.uid
                        || n.size.is_some_and(|v| v != o.revision.size)
                        || n.links.is_some_and(|v| v != o.revision.links)
                        || self
                            .metadata_after
                            .get(&id(o.object))
                            .is_some_and(|prior| *prior != o)
                    {
                        return Err(bad(
                            "Datos o metadata confirmada cambiaron antes del intent",
                        ));
                    }

                    if i == node as usize {
                        selected = Some((index, o));
                    }
                    if o.object == self.created[node as usize] {
                        aliases += 1;
                    }
                }
                h.update((relative.len() as u32).to_le_bytes());
                h.update(relative);
                let mut b = Vec::new();
                put_observation(&mut b, o);
                h.update(b);
            }
        }
        let (entry, before) = selected.ok_or_else(|| bad("Nodo metadata fuera del workspace"))?;
        if before.object.identity.kind == Kind::Directory {
            if aliases != 1 {
                return Err(bad("Directorio metadata repetido"));
            }
        } else if aliases as u64 != before.revision.links {
            return Err(bad("Alias externo o scopes incompletos"));
        }
        let seal = h.finalize().into();
        self.check(s, Event::StageMetadataIntent { node, before, seal })?;
        Ok(MetadataSpec {
            scope: n.transaction,
            entry,
            before,
            seal,
        })
    }
}
