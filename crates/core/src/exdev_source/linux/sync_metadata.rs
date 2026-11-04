use super::*;
use crate::directory_sync_metadata::Values;
use std::ffi::OsString;
impl Inventory {
    pub(super) fn pin_sync_path(
        &self,
        parent: &File,
        root_name: &OsStr,
        index: usize,
        cancel: &AtomicBool,
    ) -> io::Result<(File, File, OsString)> {
        if self.sync.is_none() {
            return Err(invalid("Inventario no es sincronización"));
        }
        pin_entries(
            parent,
            root_name,
            &self.entries,
            self.mounts
                .as_deref()
                .ok_or_else(|| invalid("Sin montajes"))?,
            index,
            cancel,
        )
    }
    pub(crate) fn metadata_values_for_sync(
        &self,
        parent: &File,
        name: &OsStr,
        index: usize,
        cancel: &AtomicBool,
    ) -> io::Result<Values> {
        let original_parent = parent;
        let root_name = name;
        let (parent, pin, name) = self.pin_sync_path(parent, name, index, cancel)?;
        let e = &self.entries[index];
        let before = pin.metadata()?;
        if before.uid() != unsafe { libc::geteuid() } {
            return Err(invalid("Metadatos requieren objeto propio"));
        }
        let source = if e.identity.kind == Kind::Symlink {
            pin
        } else {
            data(
                &parent,
                &name,
                e.identity,
                e.identity.kind == Kind::Directory,
            )?
        };
        let named = if e.identity.kind == Kind::Symlink {
            let mut b = format!("/proc/self/fd/{}/", parent.as_raw_fd()).into_bytes();
            b.extend_from_slice(name.as_bytes());
            Some(c(&b)?)
        } else {
            None
        };
        let mut attrs = Vec::new();
        attributes_with(&source, named.as_ref(), cancel, |name, value| {
            attrs.push((name.as_bytes().to_vec(), value));
            Ok(())
        })?;
        stable(&parent, &name, &source, e.identity, e.revision)?;
        if mount_id(&source, None)? != self.mounts.as_ref().expect("sync")[index]
            || mount_id(&parent, Some(&name))? != self.mounts.as_ref().expect("sync")[index]
        {
            return Err(invalid("Montaje cambió al leer metadatos"));
        }
        let after = source.metadata()?;
        let access = if e.identity.kind == Kind::Symlink {
            e.access
        } else {
            AccessTime {
                seconds: after.atime(),
                nanos: u32::try_from(after.atime_nsec()).map_err(|_| invalid("Atime inválido"))?,
            }
        };
        let v = Values::captured(e, access, attrs)?;
        if v.evidence_digest() != e.metadata_digest {
            return Err(invalid("Valores de metadatos cambiaron"));
        }
        let _named_again = self.pin_sync_path(original_parent, root_name, index, cancel)?;
        check(cancel)?;
        Ok(v)
    }
}

pub(super) fn pin_entries(
    parent: &File,
    root_name: &OsStr,
    entries: &[Entry],
    mounts: &[u64],
    index: usize,
    cancel: &AtomicBool,
) -> io::Result<(File, File, OsString)> {
    pin_entries_after(parent, root_name, entries, mounts, index, cancel, None)
}
pub(super) fn pin_entries_after(
    parent: &File,
    root_name: &OsStr,
    entries: &[Entry],
    mounts: &[u64],
    index: usize,
    cancel: &AtomicBool,
    after: Option<Revision>,
) -> io::Result<(File, File, OsString)> {
    let e = entries
        .get(index)
        .ok_or_else(|| invalid("Entrada ausente"))?;

    let mut current = parent.try_clone()?;
    let mut end = 0usize;
    for (n, part) in e.relative.split(|&v| v == b'/').enumerate() {
        check(cancel)?;
        if n == 0 && part != root_name.as_bytes() {
            return Err(invalid("Raíz relativa distinta"));
        }
        end += part.len();
        let i = entries
            .binary_search_by(|v| v.relative.as_slice().cmp(&e.relative[..end]))
            .map_err(|_| invalid("Ancestro ausente"))?;
        let observed = &entries[i];
        let name = OsStr::from_bytes(part);
        let pin = open(&current, name)?;
        let expected = if i == index {
            after.unwrap_or(observed.revision)
        } else {
            observed.revision
        };
        stable(&current, name, &pin, observed.identity, expected)?;
        if mount_id(&pin, None)? != mounts[i] || mount_id(&current, Some(name))? != mounts[i] {
            return Err(invalid("Montaje cambió en metadatos"));
        }
        if end == e.relative.len() {
            return Ok((current, pin, name.to_os_string()));
        }
        if observed.identity.kind != Kind::Directory {
            return Err(invalid("Ancestro no es directorio"));
        }
        current = pin;
        end += 1;
    }
    Err(invalid("Ruta vacía"))
}

impl Inventory {
    /// Internal staging barrier: synchronize only pinned no-follow objects.
    /// The caller must rewalk all scopes after these sequential syncs.
    pub(crate) fn verify_staging(
        &self,
        parent: &File,
        name: &OsStr,
        read: u64,
        cancel: &AtomicBool,
        control: Option<&Control>,
    ) -> io::Result<u64> {
        if self.sync.is_none() || read > crate::directory_compare::MAX_READ {
            return Err(invalid("Verificación staging inválida"));
        }
        self.verify_observed(parent, name, cancel, control, Some(read))
    }
    pub(crate) fn sync_staging(
        &self,
        parent: &File,
        name: &OsStr,
        cancel: &AtomicBool,
        control: Option<&Control>,
    ) -> io::Result<()> {
        for i in (0..self.entries.len()).rev() {
            check_control(cancel, control)?;
            let e = &self.entries[i];
            let (p, pin, n) = self.pin_sync_path(parent, name, i, cancel)?;
            if e.identity.kind != Kind::Symlink {
                let fd = data(&p, &n, e.identity, e.identity.kind == Kind::Directory)?;
                fd.sync_all()?;
                stable(&p, &n, &fd, e.identity, e.revision)?;
            }
            // A symlink's containing directory occurs later in reverse path order.
            stable(&p, &n, &pin, e.identity, e.revision)?;
        }
        parent.sync_all()?;
        self.check_staging_root(parent, name, cancel)
    }
    pub(crate) fn check_staging_root(
        &self,
        parent: &File,
        name: &OsStr,
        cancel: &AtomicBool,
    ) -> io::Result<()> {
        self.pin_sync_path(parent, name, 0, cancel).map(|_| ())
    }
}
