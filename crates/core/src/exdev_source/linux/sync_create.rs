use super::*;
use crate::directory_sync_journal::{Object, Observation};
use std::io::Write;
pub(crate) struct StageInput<'a> {
    inventory: &'a Inventory,
    parent: &'a File,
    name: &'a OsStr,
    index: usize,
    pub file: File,
}
impl Inventory {
    pub(crate) fn stage_input<'a>(
        &'a self,
        parent: &'a File,
        name: &'a OsStr,
        index: usize,
        cancel: &AtomicBool,
    ) -> io::Result<StageInput<'a>> {
        self.metadata_values_for_sync(parent, name, index, cancel)?;
        let (p, pin, n) = self.pin_sync_path(parent, name, index, cancel)?;
        let e = &self.entries[index];
        let file = if e.identity.kind == Kind::Symlink {
            pin
        } else {
            data(&p, &n, e.identity, e.identity.kind == Kind::Directory)?
        };
        Ok(StageInput {
            inventory: self,
            parent,
            name,
            index,
            file,
        })
    }
}
impl StageInput<'_> {
    pub(crate) fn binding(&self) -> (Object, Observation, &[u8]) {
        let e = &self.inventory.entries[self.index];
        let root = &self.inventory.entries[0];
        let mounts = self.inventory.mounts.as_ref().expect("sync");
        let relative = if self.index == 0 {
            b"".as_slice()
        } else {
            &e.relative[root.relative.len() + 1..]
        };
        (
            Object {
                identity: root.identity,
                mount: mounts[0],
            },
            Observation {
                object: Object {
                    identity: e.identity,
                    mount: mounts[self.index],
                },
                revision: e.revision,
                content: e.content_digest,
                metadata: e.metadata_digest,
            },
            relative,
        )
    }
    pub(crate) fn verify(&self, cancel: &AtomicBool) -> io::Result<()> {
        self.inventory
            .metadata_values_for_sync(self.parent, self.name, self.index, cancel)?;
        let (p, _, n) = self
            .inventory
            .pin_sync_path(self.parent, self.name, self.index, cancel)?;
        let e = &self.inventory.entries[self.index];
        stable(&p, &n, &self.file, e.identity, e.revision)
    }
    pub(crate) fn literal(&self, cancel: &AtomicBool) -> io::Result<Vec<u8>> {
        self.verify(cancel)?;
        let mut b = vec![0; MAX_PATH + 1];
        let n = unsafe {
            libc::readlinkat(
                self.file.as_raw_fd(),
                c"".as_ptr(),
                b.as_mut_ptr().cast(),
                b.len(),
            )
        };
        if n < 0 {
            return Err(io::Error::last_os_error());
        }
        if n as usize > MAX_PATH {
            return Err(invalid("Symlink excedido"));
        }
        b.truncate(n as usize);
        let h: [u8; 32] = Sha256::digest(&b).into();
        if h != self.binding().1.content {
            return Err(invalid("Literal source cambió"));
        }
        self.verify(cancel)?;
        Ok(b)
    }
    pub(crate) fn copy(
        &mut self,
        destination: &mut impl Write,
        mut checkpoint: impl FnMut() -> io::Result<()>,
    ) -> io::Result<u64> {
        let expected = self.binding().1;
        self.file.seek(SeekFrom::Start(0))?;
        let mut remaining = expected.revision.size;
        let mut buf = vec![0u8; BUFFER];
        let mut h = Sha256::new();
        while remaining != 0 {
            checkpoint()?;
            let n = self
                .file
                .read(&mut buf[..remaining.min(BUFFER as u64) as usize])?;
            if n == 0 {
                return Err(invalid("Source truncada"));
            }
            h.update(&buf[..n]);
            checkpoint()?;
            destination.write_all(&buf[..n])?;
            remaining -= n as u64;
        }
        checkpoint()?;
        if self.file.read(&mut [0; 1])? != 0 {
            return Err(invalid("Source creció"));
        }
        let hash: [u8; 32] = h.finalize().into();
        if hash != expected.content {
            return Err(invalid("Payload source cambió"));
        }
        Ok(expected.revision.size)
    }
}
