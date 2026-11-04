//! One owner, borrowed drawing references: no RGB clones or uncharged pinned Arc.
//! Cache RGB + active wire reservation share4MiB. Scratch has a separate explicit
//! fixed ceiling; this local ledger never claims a process-tree PSS budget.
use crate::image_preview_job::{Bitmap, THUMBNAIL_DIMENSION, THUMBNAIL_WIRE_BYTES};
use files_core::media_source::Revision;
use std::path::{Path, PathBuf};
pub const MAX_ENTRIES: usize = 32;
pub const MAX_RGB_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_KEY_BYTES: usize = 64 * 1024;
pub const SCRATCH_BYTES: usize = 160 * 160 * 3;
struct Entry {
    path: PathBuf,
    bitmap: Bitmap,
    used: u64,
}
pub struct Cache {
    entries: Vec<Entry>,
    clock: u64,
    reserved: usize,
}
impl Cache {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            clock: 0,
            reserved: 0,
        }
    }
    pub fn bytes(&self) -> usize {
        self.entries.iter().map(|e| e.bitmap.rgb.capacity()).sum()
    }
    pub fn key_bytes(&self) -> usize {
        self.entries
            .iter()
            .map(|e| e.path.capacity() + e.bitmap.format.capacity() + std::mem::size_of::<Entry>())
            .sum()
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn clear(&mut self) {
        self.entries.clear();
        self.entries.shrink_to_fit();
    }
    pub fn revision(&self, path: &Path) -> Option<Revision> {
        self.entries
            .iter()
            .find(|e| e.path == path)
            .and_then(|e| e.bitmap.source)
    }
    pub fn remove(&mut self, path: &Path) {
        self.entries.retain(|e| e.path != path);
    }
    fn evict(&mut self) -> bool {
        if let Some((i, _)) = self.entries.iter().enumerate().min_by_key(|(_, e)| e.used) {
            self.entries.swap_remove(i);
            true
        } else {
            false
        }
    }
    pub fn reserve(&mut self) -> bool {
        if self.reserved != 0 {
            return false;
        }
        while self.bytes() + THUMBNAIL_WIRE_BYTES > MAX_RGB_BYTES {
            if !self.evict() {
                return false;
            }
        }
        self.reserved = THUMBNAIL_WIRE_BYTES;
        true
    }
    pub fn release(&mut self) {
        self.reserved = 0;
    }
    pub fn insert(&mut self, path: PathBuf, bitmap: Bitmap) -> Result<(), &'static str> {
        if self.reserved != THUMBNAIL_WIRE_BYTES
            || bitmap.width == 0
            || bitmap.height == 0
            || bitmap.width > THUMBNAIL_DIMENSION
            || bitmap.height > THUMBNAIL_DIMENSION
            || bitmap.source.is_none()
            || bitmap.rgb.len() != bitmap.width as usize * bitmap.height as usize * 3
            || bitmap.rgb.capacity() > self.reserved
            || path.as_os_str().len() > 4096
        {
            return Err("Miniatura sin reserva o fuera de límite");
        }
        let keys = path.capacity() + bitmap.format.capacity() + std::mem::size_of::<Entry>();
        if keys > MAX_KEY_BYTES {
            return Err("Clave de miniatura fuera de presupuesto");
        }
        self.remove(&path);
        while self.entries.len() >= MAX_ENTRIES {
            self.evict();
        }
        while self.key_bytes() + keys > MAX_KEY_BYTES {
            if !self.evict() {
                return Err("Claves de miniaturas fuera de presupuesto");
            }
        }
        self.clock = self.clock.saturating_add(1);
        self.entries.push(Entry {
            path,
            bitmap,
            used: self.clock,
        });
        self.reserved = 0;
        Ok(())
    }
    pub fn get(&mut self, path: &Path, revision: Revision) -> Option<&Bitmap> {
        let entry = self
            .entries
            .iter_mut()
            .find(|e| e.path == path && e.bitmap.source == Some(revision))?;
        self.clock = self.clock.saturating_add(1);
        entry.used = self.clock;
        Some(&entry.bitmap)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn revision(n: u64) -> Revision {
        Revision {
            device: 1,
            inode: n,
            bytes: 1,
            modified_seconds: 0,
            modified_nanoseconds: 0,
            changed_seconds: 0,
            changed_nanoseconds: 0,
        }
    }
    fn bitmap(n: u64) -> Bitmap {
        Bitmap {
            width: 1,
            height: 1,
            source_width: 1,
            source_height: 1,
            format: "png".into(),
            rgb: vec![1, 2, 3],
            source: Some(revision(n)),
        }
    }
    #[test]
    fn requires_reservation_counts_capacity_and_replaces_identity() {
        let mut c = Cache::new();
        assert!(c.insert("/a".into(), bitmap(1)).is_err());
        assert!(c.reserve());
        let mut b = bitmap(1);
        b.rgb.reserve_exact(100);
        let cap = b.rgb.capacity();
        c.insert("/a".into(), b).unwrap();
        assert_eq!(c.bytes(), cap);
        assert!(c.get(Path::new("/a"), revision(2)).is_none());
        assert!(c.reserve());
        c.insert("/a".into(), bitmap(2)).unwrap();
        assert_eq!(c.len(), 1);
        assert_eq!(c.revision(Path::new("/a")), Some(revision(2)));
    }
    #[test]
    fn lru_entry_bound_and_no_second_ticket() {
        let mut c = Cache::new();
        for i in 0..32 {
            assert!(c.reserve());
            assert!(!c.reserve());
            c.insert(format!("/{i}").into(), bitmap(i)).unwrap();
        }
        assert!(c.get(Path::new("/0"), revision(0)).is_some());
        assert!(c.reserve());
        c.insert("/32".into(), bitmap(32)).unwrap();
        assert_eq!(c.len(), 32);
        assert!(c.revision(Path::new("/1")).is_none());
        assert!(c.revision(Path::new("/0")).is_some());
        assert!(c.bytes() + THUMBNAIL_WIRE_BYTES <= MAX_RGB_BYTES);
    }
    #[test]
    fn rejects_oversized_capacity_before_retention() {
        let mut c = Cache::new();
        assert!(c.reserve());
        let mut b = bitmap(1);
        b.rgb.reserve_exact(THUMBNAIL_WIRE_BYTES + 1);
        assert!(c.insert("/a".into(), b).is_err());
        assert_eq!(c.bytes(), 0);
        c.release();
        assert!(c.reserve());
    }
}
