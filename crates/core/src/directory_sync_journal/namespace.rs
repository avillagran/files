use super::*;
use crate::exdev_source::NamespaceMove;
impl State {
    /// Diagnostic pending intent data; never authority to retry an effect.
    pub fn namespace_guard(&self) -> Option<(u32, NamespaceAction, [u8; 32])> {
        self.namespace
    }
    pub(crate) fn validate_namespace(
        &self,
        s: &Schema,
        transaction: u32,
        action: NamespaceAction,
        plan: &NamespaceMove<'_>,
    ) -> io::Result<[u8; 32]> {
        self.check(
            s,
            Event::NamespaceIntent {
                transaction,
                action,
                seal: [1; 32],
            },
        )?;
        let t = s
            .transactions
            .get(transaction as usize)
            .ok_or_else(|| bad("Transaction namespace ausente"))?;
        let workspace = *self
            .workspaces
            .get(transaction as usize)
            .ok_or_else(|| bad("Workspace ausente"))?;
        let b = plan.binding();
        let (private_root, ok) = match action {
            NamespaceAction::Preserve => (
                b.to_root,
                b.from_root.object == t.root
                    && b.from_parent == t.parent_object
                    && b.from == t.target
                    && b.to_root.object == workspace
                    && b.to_parent == workspace
                    && b.to == b"old"
                    && t.old.is_some_and(|o| same_rename(o, b.source)),
            ),
            NamespaceAction::Publish => (
                b.from_root,
                b.from_root.object == workspace
                    && b.from_parent == workspace
                    && b.from == b"new"
                    && b.to_root.object == t.root
                    && b.to_parent == t.parent_object
                    && b.to == t.target,
            ),
        };
        if !ok || private_root.revision.mode != 0o700 {
            return Err(bad("Binding físico de namespace no corresponde al plan"));
        }
        match action {
            NamespaceAction::Preserve => {
                let mut h = Sha256::new();
                h.update(b"Files:sync-old-scope:v1\0");
                for (path, o) in plan.source_items() {
                    semantic(&mut h, path, o);
                }
                let hash: [u8; 32] = h.finalize().into();
                if hash != t.old_scope {
                    return Err(bad("Scope old cambió respecto de la intención sellada"));
                }
                Ok(hash)
            }
            NamespaceAction::Publish => {
                let observed: HashMap<&[u8], Observation> = plan.source_items().collect();
                if observed.len() != t.nodes.len() {
                    return Err(bad("Scope staged incompleto o con nombres extra"));
                }
                for i in t.nodes.clone() {
                    let actual = observed
                        .get(s.nodes[i].relative.as_slice())
                        .ok_or_else(|| bad("Nodo staged ausente"))?;
                    let old = *self
                        .verified
                        .get(i)
                        .ok_or_else(|| bad("Nodo sin declarar verified"))?;
                    if !same_rename(old, *actual) {
                        return Err(bad(
                            "Datos/metadata staged no coinciden con plan verificado",
                        ));
                    }
                }
                self.staged_scope(s, transaction as usize)
            }
        }
    }
}
