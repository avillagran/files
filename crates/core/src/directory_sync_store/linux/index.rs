use super::*;
use std::os::unix::fs::FileExt;
/// One original verified record. Data only: no source freshness, storage ACK or
/// operator approval. The directory flag preserves its explicit mtime policy.
pub struct ValueRecord {
    target: Target,
    after_children: bool,
    values: Values,
}
impl ValueRecord {
    pub fn target(&self) -> Target {
        self.target
    }
    pub fn after_children(&self) -> bool {
        self.after_children
    }
    pub fn values(&self) -> &Values {
        &self.values
    }
    pub fn into_values(self) -> Values {
        self.values
    }
}
struct Slot {
    offset: u64,
    digest: [u8; 32],
    length: u32,
    after_children: bool,
}
pub(super) struct Index {
    slots: Vec<Slot>,
    stages: usize,
    directories: usize,
    prefix: u64,
}
fn target_at(n: usize, stages: usize) -> Target {
    if n < stages {
        Target::Stage(n)
    } else {
        let relative = n - stages;
        let i = relative / 2;
        if relative.is_multiple_of(2) {
            Target::UndoDirectory(i)
        } else {
            Target::Directory(i)
        }
    }
}
impl Index {
    /// Local slots are published only after full semantic/whole-stream checks.
    pub(super) fn collect(
        physical: &Physical<'_, '_>,
        reader: &mut impl Read,
        prefix: u64,
        total: u64,
    ) -> io::Result<(metadata::StoredValues, Self)> {
        let stages = physical.nodes().len();
        let directories = physical.directory_metadata().len();
        let count = stages + 2 * directories;
        if count > metadata::MAX_RECORDS {
            return Err(bad("Índice de valores excede límite"));
        }
        let end = total
            .checked_sub(32)
            .ok_or_else(|| bad("Archivo truncado"))?;
        let mut slots = Vec::with_capacity(count);
        let mut next = 132;
        let values = metadata::inspect_indexed(physical, reader, |target, flag, _value, info| {
            let absolute_end = prefix
                .checked_add(info.offset)
                .and_then(|n| n.checked_add(u64::from(info.length)))
                .ok_or_else(|| bad("Offset de índice desbordado"))?;
            if slots.len() >= count
                || target != target_at(slots.len(), stages)
                || info.offset != next
                || !(49..=10 + metadata::MAX_VALUE_WIRE as u32).contains(&info.length)
                || absolute_end > end
            {
                return Err(bad("Índice de valores incoherente"));
            }
            next = info.offset + u64::from(info.length);
            slots.push(Slot {
                offset: info.offset,
                digest: info.digest,
                length: info.length,
                after_children: flag,
            });
            Ok(())
        })?;
        if slots.len() != count
            || values.records() != count
            || prefix.checked_add(values.bytes()) != Some(total)
            || next + 32 != values.bytes()
        {
            return Err(bad("Índice/longitud de valores incompleto"));
        }
        Ok((
            values,
            Self {
                slots,
                stages,
                directories,
                prefix,
            },
        ))
    }
    fn ordinal(&self, target: Target) -> io::Result<usize> {
        match target {
            Target::Stage(i) if i < self.stages => Ok(i),
            Target::UndoDirectory(i) if i < self.directories => Ok(self.stages + 2 * i),
            Target::Directory(i) if i < self.directories => Ok(self.stages + 2 * i + 1),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Objeto fuera del índice de metadatos",
            )),
        }
    }
    pub(super) fn read(&self, file: &File, target: Target) -> io::Result<ValueRecord> {
        self.read_with(target, |buffer, offset| file.read_exact_at(buffer, offset))
    }
    pub(super) fn read_with(
        &self,
        target: Target,
        mut read: impl FnMut(&mut [u8], u64) -> io::Result<()>,
    ) -> io::Result<ValueRecord> {
        let slot = &self.slots[self.ordinal(target)?];
        let mut buffer = vec![0; slot.length as usize];
        read(&mut buffer, self.prefix + slot.offset)?;
        let head: &[u8; 10] = buffer[..10]
            .try_into()
            .map_err(|_| bad("Registro indexado truncado"))?;
        let body = &buffer[10..];
        let (tag, i) = match target {
            Target::Stage(i) => (0, i),
            Target::Directory(i) => (1, i),
            Target::UndoDirectory(i) => (2, i),
        };
        if head[0] != tag
            || head[1..5] != (i as u32).to_le_bytes()
            || head[5] != u8::from(slot.after_children)
            || head[6..10] != (body.len() as u32).to_le_bytes()
            || metadata::record_digest(slot.offset, head, body) != slot.digest
        {
            return Err(bad("Registro indexado modificado o fuera de su contexto"));
        }
        let values = Values::decode(body)?;
        Ok(ValueRecord {
            target,
            after_children: slot.after_children,
            values,
        })
    }
    #[cfg(test)]
    pub(super) fn retained_slots(&self) -> (usize, usize) {
        (
            self.slots.len(),
            self.slots.capacity() * std::mem::size_of::<Slot>(),
        )
    }
}
