//! Bounded placement history. Durable/readable data never grants native approval.
use super::workspace_batch::PlacementSlot;
use super::*;
use crate::directory_sync_journal::{Object, Schema};
use sha2::{Digest, Sha256};
use std::os::unix::fs::FileExt;
const MAGIC: &[u8; 8] = b"FLSPLC\0\x01";
pub const MAX_PLACEMENT_BYTES: u64 = 128 * 1024 * 1024;
const HEADER: usize = 256;
const MAX_ROW: usize = 2 + 4096 + 2 + 129 * 24 + 32;
#[derive(Debug)]
pub struct Placement {
    index: usize,
    path: PathBuf,
    name: OsString,
    chain: Vec<Node>,
    seal: [u8; 32],
}
impl Placement {
    pub fn transaction(&self) -> usize {
        self.index
    }
    pub fn parent_path(&self) -> &Path {
        &self.path
    }
    pub fn name(&self) -> &std::ffi::OsStr {
        &self.name
    }
    pub fn workspace_path(&self) -> PathBuf {
        self.path.join(&self.name)
    }
    pub fn seal(&self) -> [u8; 32] {
        self.seal
    }
    pub fn ancestors(&self) -> impl Iterator<Item = (u64, u64, u64)> + '_ {
        self.chain.iter().map(|n| (n.device, n.inode, n.mount))
    }
    /// Fresh pathname/ancestor/private-container check only, not execution authority.
    pub fn open_parent(&self) -> io::Result<File> {
        let current = Directory::open(&self.path)?;
        if current.chain != self.chain {
            return Err(bad("Ancestros del placement cambiaron"));
        }
        current.check()?;
        Ok(current.file)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct At {
    offset: u64,
    len: usize,
    hash: [u8; 32],
}
pub struct PlacementRegistry<'a> {
    store: &'a Store,
    held: Held,
    name: CString,
    identity: Node,
    stamp: Stamp,
    seal: [u8; 32],
    index: Vec<At>,
    state: State,
}
fn filename(proof: &Proof) -> io::Result<CString> {
    let mut text = String::from("sync-placements-");
    for b in &proof.encode()[56..72] {
        use std::fmt::Write;
        write!(&mut text, "{b:02x}").expect("string");
    }
    text.push_str(".registry");
    c(text.as_bytes())
}
pub(super) fn workspace_name(proof: &Proof, index: usize) -> OsString {
    let mut text = String::with_capacity(46);
    text.push_str("sync-");
    use std::fmt::Write;
    for b in &proof.encode()[56..72] {
        write!(&mut text, "{b:02x}").expect("string");
    }
    write!(&mut text, "-{index:08x}").expect("string");
    text.into()
}
fn parts(path: &[u8]) -> io::Result<usize> {
    Directory::path_shape(Path::new(std::ffi::OsStr::from_bytes(path))).map(|(count, _)| count)
}

fn encode(path: &Path, chain: &[Node], seal: [u8; 32]) -> io::Result<Vec<u8>> {
    let raw = path.as_os_str().as_bytes();
    if chain.len() != parts(raw)? + 1
        || chain.iter().any(|n| n.inode == 0 || n.mount == 0)
        || seal == [0; 32]
    {
        return Err(bad("Cadena de placement inválida"));
    }
    let mut bytes = Vec::with_capacity(4 + raw.len() + 24 * chain.len() + 32);
    bytes.extend_from_slice(&(raw.len() as u16).to_le_bytes());
    bytes.extend_from_slice(raw);
    bytes.extend_from_slice(&(chain.len() as u16).to_le_bytes());
    for n in chain {
        for v in [n.device, n.inode, n.mount] {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
    }
    bytes.extend_from_slice(&seal);
    Ok(bytes)
}
fn read_row(
    reader: &mut impl Read,
    index: usize,
    proof: &Proof,
) -> io::Result<(Placement, Vec<u8>)> {
    let mut small = [0; 2];
    reader.read_exact(&mut small)?;
    let len = u16::from_le_bytes(small) as usize;
    if len == 0 || len > 4096 {
        return Err(bad("Ruta de placement excedida"));
    }
    let mut bytes = Vec::with_capacity(MAX_ROW);
    bytes.extend_from_slice(&small);
    let at = bytes.len();
    bytes.resize(at + len, 0);
    reader.read_exact(&mut bytes[at..])?;
    let path = PathBuf::from(OsString::from_vec(bytes[at..].to_vec()));
    let count = parts(path.as_os_str().as_bytes())? + 1;
    reader.read_exact(&mut small)?;
    if u16::from_le_bytes(small) as usize != count {
        return Err(bad("Conteo de ancestros de placement distinto"));
    }
    bytes.extend_from_slice(&small);
    let at = bytes.len();
    bytes.resize(at + count * 24 + 32, 0);
    reader.read_exact(&mut bytes[at..])?;
    let mut chain = Vec::with_capacity(count);
    for chunk in bytes[at..at + count * 24].as_chunks::<24>().0 {
        let n = Node {
            device: number(chunk, 0),
            inode: number(chunk, 8),
            mount: number(chunk, 16),
        };
        if n.inode == 0 || n.mount == 0 {
            return Err(bad("Identidad de placement inválida"));
        }
        chain.push(n);
    }
    let seal = bytes[bytes.len() - 32..].try_into().expect("seal");
    if seal == [0; 32] {
        return Err(bad("Sello de placement vacío"));
    }
    Ok((
        Placement {
            index,
            path,
            name: workspace_name(proof, index),
            chain,
            seal,
        },
        bytes,
    ))
}
fn scan(file: &mut File, schema: &Schema, cancel: &AtomicBool) -> io::Result<([u8; 32], Vec<At>)> {
    let len = file.metadata()?.len();
    if !(HEADER as u64 + 32..=MAX_PLACEMENT_BYTES).contains(&len) {
        return Err(bad("Registro de placements excedido/truncado"));
    }
    file.seek(SeekFrom::Start(0))?;
    let mut header = [0; HEADER];
    file.read_exact(&mut header)?;
    let count = u32::from_le_bytes(header[252..256].try_into().expect("count")) as usize;
    if &header[..8] != MAGIC
        || header[8..220] != schema.proof().encode()
        || header[220..252] != schema.proof().encode()[136..168]
        || count != schema.workspace_count()
        || count > crate::directory_sync_physical::MAX_TRANSACTIONS
    {
        return Err(bad("Registro de placements pertenece a otro lote"));
    }
    let mut hash = Sha256::new();
    hash.update(header);
    let mut index = Vec::with_capacity(count);
    let mut offset = HEADER as u64;
    for i in 0..count {
        check_cancel(cancel)?;
        let (p, bytes) = read_row(file, i, schema.proof())?;
        let n = p.chain.last().expect("parent");
        let expected = schema.workspace_binding(
            i as u32,
            Object {
                identity: crate::move_manifest::Identity {
                    device: n.device,
                    inode: n.inode,
                    kind: Kind::Directory,
                },
                mount: n.mount,
            },
            p.name.as_bytes(),
        )?;
        if p.seal != expected {
            return Err(bad("Sello de placement distinto del lote"));
        }
        let end = offset
            .checked_add(bytes.len() as u64)
            .filter(|&n| n <= len - 32)
            .ok_or_else(|| bad("Longitud de placement excedida"))?;
        index.push(At {
            offset,
            len: bytes.len(),
            hash: Sha256::digest(&bytes).into(),
        });
        hash.update(bytes);
        offset = end;
    }
    let mut seal = [0; 32];
    file.read_exact(&mut seal)?;
    if offset + 32 != len || hash.finalize().as_slice() != seal || file.read(&mut [0; 1])? != 0 {
        return Err(bad("Readback/sello de placements distinto"));
    }
    Ok((seal, index))
}
impl<'a> PlacementRegistry<'a> {
    pub fn path(&self) -> PathBuf {
        self.store
            .directory
            .path
            .join(OsString::from_vec(self.name.as_bytes().to_vec()))
    }
    pub fn len(&self) -> usize {
        self.index.len()
    }
    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }
    pub fn seal(&self) -> [u8; 32] {
        self.seal
    }
    pub fn durability(&self) -> State {
        self.state
    }
    pub fn entry(&self, index: usize) -> io::Result<Placement> {
        if self.state == State::Uncertain {
            return Err(bad("Placements inciertos: requiere reapertura"));
        }
        self.binding()?;
        let at = self
            .index
            .get(index)
            .ok_or_else(|| bad("Índice de placement fuera de rango"))?;
        let mut bytes = vec![0; at.len];
        self.held.file.read_exact_at(&mut bytes, at.offset)?;
        if Sha256::digest(&bytes).as_slice() != at.hash {
            return Err(bad("Placement indexado modificado"));
        }
        let (entry, _) = read_row(&mut bytes.as_slice(), index, self.store.proof())?;
        self.binding()?;
        Ok(entry)
    }
    pub(super) fn matches(&self, proof: &Proof, seal: [u8; 32]) -> io::Result<()> {
        self.ensure_current()?;
        if self.store.proof().encode() != proof.encode() || self.seal != seal {
            return Err(bad("Registro de placements no coincide con journal"));
        }
        Ok(())
    }
    pub fn ensure_current(&self) -> io::Result<()> {
        if self.state != State::Durable {
            return Err(bad("Registro de placements sin ACK durable"));
        }
        self.store.ensure_current()?;
        self.binding()
    }
    fn binding(&self) -> io::Result<()> {
        self.store.binding()?;
        check_binding(
            &self.store.directory,
            &self.held,
            &self.name,
            self.identity,
            Some(self.stamp),
        )
        .map(|_| ())
    }
    pub fn renew(&mut self, physical: &Physical<'_, '_>, cancel: &AtomicBool) -> io::Result<()> {
        let schema = Schema::derive(self.store, physical)?;
        self.renew_with(&schema, cancel, &mut |_| Ok(()))
    }
    pub(super) fn renew_with(
        &mut self,
        schema: &Schema,
        cancel: &AtomicBool,
        hook: &mut impl FnMut(Barrier) -> io::Result<()>,
    ) -> io::Result<()> {
        if self.state == State::Uncertain {
            return Err(bad("Placements inciertos: requiere reapertura"));
        }
        self.store.ensure_current()?;
        self.binding()?;
        self.state = State::Uncertain;
        let (seal, index) = scan(&mut self.held.file, schema, cancel)?;
        if seal != self.seal || index != self.index {
            return Err(bad("Registro de placements cambió"));
        }
        self.binding()?;
        hook(Barrier::ReadBack)?;
        self.confirm(cancel, hook)
    }
    fn confirm(
        &mut self,
        cancel: &AtomicBool,
        hook: &mut impl FnMut(Barrier) -> io::Result<()>,
    ) -> io::Result<()> {
        self.state = State::Uncertain;
        check_cancel(cancel)?;
        self.binding()?;
        self.held.file.sync_all()?;
        hook(Barrier::FileSynced)?;
        self.store.directory.sync_namespace(hook)?;
        hook(Barrier::Acknowledged)?;
        check_cancel(cancel)?;
        self.store.ensure_current()?;
        self.binding()?;
        self.state = State::Durable;
        Ok(())
    }
    pub(super) fn create_with(
        store: &'a Store,
        schema: &Schema,
        placements: &[PlacementSlot],
        cancel: &AtomicBool,
        mut hook: impl FnMut(Barrier) -> io::Result<()>,
    ) -> io::Result<Self> {
        check_cancel(cancel)?;
        store.ensure_current()?;
        if placements.len() != schema.workspace_count() {
            return Err(bad("Placements incompletos"));
        }
        let mut length = HEADER as u64 + 32;
        for (i, (d, name, seal)) in placements.iter().enumerate() {
            if *name != workspace_name(schema.proof(), i) {
                return Err(bad("Nombre de placement distinto"));
            }
            d.check()?;
            let n = d.chain.last().expect("parent");
            let expected = schema.workspace_binding(
                i as u32,
                Object {
                    identity: crate::move_manifest::Identity {
                        device: n.device,
                        inode: n.inode,
                        kind: Kind::Directory,
                    },
                    mount: n.mount,
                },
                name.as_bytes(),
            )?;
            if expected != *seal {
                return Err(bad("Placement distinto del lote"));
            }
            let bytes = encode(&d.path, &d.chain, *seal)?;
            length = length
                .checked_add(bytes.len() as u64)
                .filter(|&n| n <= MAX_PLACEMENT_BYTES)
                .ok_or_else(|| bad("Placements exceden presupuesto"))?;
        }
        let name = filename(store.proof())?;
        let file = owned(unsafe {
            libc::openat(
                store.directory.file.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDWR
                    | libc::O_CREAT
                    | libc::O_EXCL
                    | libc::O_NOFOLLOW
                    | libc::O_NONBLOCK
                    | libc::O_NOATIME
                    | libc::O_CLOEXEC,
                0o600,
            )
        })?;
        if unsafe { libc::fchmod(file.as_raw_fd(), 0o600) } != 0 {
            return Err(io::Error::last_os_error());
        }
        let held = Held::lock(file)?;
        let identity = node(&held.file)?;
        let stamp = check_binding(&store.directory, &held, &name, identity, None)?;
        let mut registry = Self {
            store,
            held,
            name,
            identity,
            stamp,
            seal: [0; 32],
            index: Vec::new(),
            state: State::Uncertain,
        };
        hook(Barrier::Created)?;
        let mut hash = Sha256::new();
        let mut header = Vec::with_capacity(HEADER);
        header.extend_from_slice(MAGIC);
        header.extend_from_slice(&schema.proof().encode());
        header.extend_from_slice(&schema.proof().encode()[136..168]);
        header.extend_from_slice(&(placements.len() as u32).to_le_bytes());
        let write = |held: &mut Held,
                     bytes: &[u8],
                     hash: &mut Sha256,
                     hook: &mut dyn FnMut(Barrier) -> io::Result<()>|
         -> io::Result<()> {
            check_cancel(cancel)?;
            held.owner()?;
            private_file(&held.file)?;
            hook(Barrier::Write)?;
            held.owner()?;
            held.file.write_all(bytes)?;
            hash.update(bytes);
            Ok(())
        };
        write(&mut registry.held, &header, &mut hash, &mut hook)?;
        for (d, _, seal) in placements {
            write(
                &mut registry.held,
                &encode(&d.path, &d.chain, *seal)?,
                &mut hash,
                &mut hook,
            )?;
        }
        let expected: [u8; 32] = hash.finalize().into();
        registry.held.owner()?;
        registry.held.file.write_all(&expected)?;
        registry.stamp = check_binding(
            &store.directory,
            &registry.held,
            &registry.name,
            identity,
            None,
        )?;
        if registry.stamp.length != length {
            return Err(bad("Longitud de placements distinta"));
        }
        hook(Barrier::ValuesWritten)?;
        let (seal, index) = scan(&mut registry.held.file, schema, cancel)?;
        if seal != expected {
            return Err(bad("Placements reescritos"));
        }
        registry.seal = seal;
        registry.index = index;
        registry.binding()?;
        hook(Barrier::ReadBack)?;
        registry.confirm(cancel, &mut hook)?;
        Ok(registry)
    }
    pub(super) fn open(store: &'a Store, schema: &Schema, cancel: &AtomicBool) -> io::Result<Self> {
        store.binding()?;
        check_cancel(cancel)?;
        let name = filename(store.proof())?;
        let pin = owned(unsafe {
            libc::openat(
                store.directory.file.as_raw_fd(),
                name.as_ptr(),
                libc::O_PATH | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        })?;
        private_file(&pin)?;
        let path = c(format!("/proc/self/fd/{}", pin.as_raw_fd()).as_bytes())?;
        let file = owned(unsafe {
            libc::open(
                path.as_ptr(),
                libc::O_RDWR | libc::O_NOATIME | libc::O_NONBLOCK | libc::O_CLOEXEC,
            )
        })?;
        if node(&file)? != node(&pin)? {
            return Err(bad("Placements sustituidos"));
        }
        let held = Held::lock(file)?;
        let identity = node(&held.file)?;
        let stamp = check_binding(&store.directory, &held, &name, identity, None)?;
        let mut registry = Self {
            store,
            held,
            name,
            identity,
            stamp,
            seal: [0; 32],
            index: Vec::new(),
            state: State::History,
        };
        let (seal, index) = scan(&mut registry.held.file, schema, cancel)?;
        registry.seal = seal;
        registry.index = index;
        registry.binding()?;
        Ok(registry)
    }
}
impl Store {
    /// Read history without opening any saved parent or workspace path.
    pub fn open_placements(
        &self,
        physical: &Physical<'_, '_>,
        cancel: &AtomicBool,
    ) -> io::Result<PlacementRegistry<'_>> {
        PlacementRegistry::open(self, &Schema::derive(self, physical)?, cancel)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn placement_codec_maximum_row_and_first_excess_are_bounded() {
        let components = vec![vec![b'a'; 31]; 128];
        let path = PathBuf::from(OsString::from_vec(
            [b"/".as_slice(), &components.join(&b'/')].concat(),
        ));
        assert_eq!(path.as_os_str().as_bytes().len(), 4096);
        let chain: Vec<_> = (1..=129)
            .map(|inode| Node {
                device: 7,
                inode,
                mount: 9,
            })
            .collect();
        let proof = Proof { bytes: [0; 212] }; // Codec data only; never a live Store proof.
        let bytes = encode(&path, &chain, [1; 32]).unwrap();
        assert_eq!(bytes.len(), MAX_ROW);
        let (p, canonical) = read_row(&mut bytes.as_slice(), 16383, &proof).unwrap();
        assert_eq!(p.path, path);
        assert_eq!(p.chain, chain);
        assert_eq!(canonical, bytes);
        assert_eq!(p.name.as_bytes().len(), 46);
        assert!(encode(&path.join("x"), &chain, [1; 32]).is_err());
        let mut excessive = chain.clone();
        excessive.push(chain[0]);
        assert!(encode(&path, &excessive, [1; 32]).is_err());
        assert!(
            HEADER as u64
                + 32
                + crate::directory_sync_physical::MAX_TRANSACTIONS as u64 * bytes.len() as u64
                <= MAX_PLACEMENT_BYTES
        );
    }
    #[test]
    fn placement_codec_rejects_malformed_paths_chain_ids_and_truncations() {
        for path in [
            b"".as_slice(),
            b"relative",
            b"//x",
            b"/x/",
            b"/x/../y",
            b"/x/./y",
            b"/x\0y",
        ] {
            assert!(parts(path).is_err());
        }
        let chain = [
            Node {
                device: 1,
                inode: 2,
                mount: 3,
            },
            Node {
                device: 1,
                inode: 4,
                mount: 3,
            },
        ];
        let bytes = encode(Path::new("/x"), &chain, [5; 32]).unwrap();
        let proof = Proof { bytes: [0; 212] };
        for end in 0..bytes.len() {
            assert!(read_row(&mut &bytes[..end], 0, &proof).is_err());
        }
        let mut wrong_count = bytes.clone();
        wrong_count[4..6].copy_from_slice(&130u16.to_le_bytes());
        assert!(read_row(&mut wrong_count.as_slice(), 0, &proof).is_err());
        let mut zero_id = bytes;
        zero_id[14..22].fill(0);
        assert!(read_row(&mut zero_id.as_slice(), 0, &proof).is_err());
    }
}
