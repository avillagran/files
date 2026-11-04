use super::*;
#[derive(Clone)]
pub(crate) struct StageSpec {
    pub source: Observation,
    pub source_root: Object,
    pub source_index: usize,
    pub source_relative: Vec<u8>,
    pub workspace: Object,
    pub parents: Vec<(Vec<u8>, Object)>,
    pub name: Vec<u8>,
    pub destination: Object,
    pub leader: Option<Object>,
    pub seal: [u8; 32],
}
impl State {
    pub fn node_create_guard(&self) -> Option<(u32, [u8; 32])> {
        self.node_create
    }
    pub(crate) fn stage_spec(&self, s: &Schema, node: u32) -> io::Result<StageSpec> {
        self.check(
            s,
            Event::NodeCreateIntent {
                node,
                seal: [1; 32],
            },
        )?;
        let n = &s.nodes[node as usize];
        let workspace = self.workspaces[n.transaction];
        let mut parents = Vec::new();
        let mut parent = n.parent;
        while let Some(p) = parent {
            let spec = &s.nodes[p];
            parents.push((
                spec.relative
                    .rsplit(|&b| b == b'/')
                    .next()
                    .unwrap_or(b"new")
                    .to_vec(),
                self.created[p],
            ));
            if spec.parent.is_none() {
                parents.last_mut().expect("parent").0 = b"new".to_vec();
            }
            parent = spec.parent;
        }
        parents.reverse();
        let name = if n.parent.is_none() {
            b"new".to_vec()
        } else {
            n.relative
                .rsplit(|&b| b == b'/')
                .next()
                .ok_or_else(|| bad("Nombre staged ausente"))?
                .to_vec()
        };
        let leader = n.group.and_then(|g| self.groups.get(&g).copied());
        let mut h = Sha256::new();
        h.update(b"Files:stage-create:v1\0");
        h.update(s.proof.encode());
        h.update(node.to_le_bytes());
        let mut b = Vec::new();
        put_object(&mut b, workspace);
        put_object(&mut b, n.object);
        put_object(&mut b, n.source_root);
        put_observation(&mut b, n.source);
        h.update(b);
        for (path, o) in parents
            .iter()
            .chain(std::iter::once(&(name.clone(), n.object)))
        {
            h.update((path.len() as u32).to_le_bytes());
            h.update(path);
            let mut b = Vec::new();
            put_object(&mut b, *o);
            h.update(b);
        }
        h.update((n.source_index as u32).to_le_bytes());
        h.update((n.source_relative.len() as u32).to_le_bytes());
        h.update(&n.source_relative);
        if let Some(o) = leader {
            let mut b = Vec::new();
            put_object(&mut b, o);
            h.update(b);
        }
        Ok(StageSpec {
            source: n.source,
            source_root: n.source_root,
            source_index: n.source_index,
            source_relative: n.source_relative.clone(),
            workspace,
            parents,
            name,
            destination: n.object,
            leader,
            seal: h.finalize().into(),
        })
    }
    pub(crate) fn known_links(&self, object: Object) -> u64 {
        self.created.iter().filter(|&&o| o == object).count() as u64
    }
}
