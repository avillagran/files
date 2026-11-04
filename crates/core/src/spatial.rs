//! Bounded, lossless per-directory icon positions. Call load/save on an I/O worker:
//! canonicalization and metadata may block on an unavailable filesystem.
use crate::{persistence, theme};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    ffi::{OsStr, OsString},
    fs, io,
    path::{Path, PathBuf},
};

pub const MAX_POSITIONS: usize = 16_384;
pub const MAX_LAYOUT_BYTES: usize = 4 * 1024 * 1024;
const MAX_NAME_BYTES: usize = 4096;
const MAX_COORDINATE: f32 = 1_000_000.0;
// Reserve header/canonical-path space before accepting additional owned names.
const HEADER_RESERVE: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Position {
    pub x: f32,
    pub y: f32,
}
impl Position {
    fn valid(self) -> bool {
        self.x.is_finite()
            && self.y.is_finite()
            && self.x.abs() <= MAX_COORDINATE
            && self.y.abs() <= MAX_COORDINATE
    }
}

#[derive(Clone, Debug)]
pub struct SpatialLayout {
    pub zoom: f32,
    positions: HashMap<OsString, Position>,
    position_budget: usize,
}
impl Default for SpatialLayout {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            positions: HashMap::new(),
            position_budget: 0,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredLayout {
    version: u32,
    directory: String,
    zoom: f32,
    positions: BTreeMap<String, Position>,
}

impl SpatialLayout {
    /// Missing layout means defaults. Corrupt, oversized, incompatible, or
    /// identity-mismatched layouts return an error and remain on disk untouched.
    pub fn load(dir: &Path) -> io::Result<Self> {
        Self::load_at(dir, &theme::config_dir().join("layouts"))
    }

    pub fn save(&self, dir: &Path) -> io::Result<()> {
        self.save_at(dir, &theme::config_dir().join("layouts"))
    }

    pub fn position(&self, name: &OsStr) -> Option<Position> {
        self.positions.get(name).copied()
    }

    pub fn set_position(&mut self, name: &OsStr, position: Position) -> io::Result<()> {
        if !position.valid() || !valid_name(name) {
            return Err(invalid("invalid position or entry name"));
        }
        if let Some(existing) = self.positions.get_mut(name) {
            *existing = position;
            return Ok(());
        }
        if self.positions.len() >= MAX_POSITIONS {
            return Err(invalid("layout position limit exceeded"));
        }
        let mut next_budget = self.position_budget;
        reserve_entry(&mut next_budget, name, MAX_LAYOUT_BYTES - HEADER_RESERVE)?;
        self.positions.insert(name.to_owned(), position);
        self.position_budget = next_budget;
        Ok(())
    }

    /// Validate the entire gesture before changing any position. Names are borrowed
    /// during preflight; only admitted new names are copied on commit.
    pub fn set_positions(&mut self, updates: &[(&OsStr, Position)]) -> io::Result<()> {
        if updates.len() > MAX_POSITIONS {
            return Err(invalid("layout update limit exceeded"));
        }
        let mut added = HashSet::new();
        let mut budget = self.position_budget;
        for &(name, position) in updates {
            if !valid_name(name) || !position.valid() {
                return Err(invalid("invalid position or entry name"));
            }
            if !self.positions.contains_key(name) && added.insert(name) {
                reserve_entry(&mut budget, name, MAX_LAYOUT_BYTES - HEADER_RESERVE)?;
                if self.positions.len() + added.len() > MAX_POSITIONS {
                    return Err(invalid("layout position limit exceeded"));
                }
            }
        }
        for &(name, position) in updates {
            if let Some(existing) = self.positions.get_mut(name) {
                *existing = position;
            } else {
                self.positions.insert(name.to_owned(), position);
            }
        }
        self.position_budget = budget;
        Ok(())
    }

    pub fn set_zoom(&mut self, zoom: f32) {
        self.zoom = bounded_zoom(zoom);
    }

    pub fn remove_position(&mut self, name: &OsStr) {
        if self.positions.remove(name).is_some() {
            let mut entry_budget = 0;
            // Every stored name already passed this check during insertion.
            if reserve_entry(&mut entry_budget, name, MAX_LAYOUT_BYTES).is_ok() {
                self.position_budget = self.position_budget.saturating_sub(entry_budget);
            }
        }
    }

    fn load_at(dir: &Path, root: &Path) -> io::Result<Self> {
        let identity = directory_identity(dir)?;
        let path = layout_path(root, &identity);
        let data = match persistence::read_bounded(&path, MAX_LAYOUT_BYTES) {
            Ok(data) => data,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e),
        };
        Self::decode_stored(&data, &identity)
    }

    // The same validation applies when loading and before replacing existing data.
    fn decode_stored(data: &[u8], identity: &str) -> io::Result<Self> {
        let text = std::str::from_utf8(data).map_err(|_| invalid("layout is not UTF-8 TOML"))?;
        let stored: StoredLayout = toml::from_str(text).map_err(|e| invalid(e.to_string()))?;
        if stored.version != 1
            || stored.directory != identity
            || !stored.zoom.is_finite()
            || stored.positions.len() > MAX_POSITIONS
        {
            return Err(invalid(
                "invalid layout version, identity, zoom or position count",
            ));
        }
        let mut layout = Self::default();
        layout.set_zoom(stored.zoom);
        for (key, position) in stored.positions {
            let name = decode_name(&key)?;
            if layout.positions.contains_key(&name) {
                return Err(invalid("duplicate decoded entry name"));
            }
            layout.set_position(&name, position)?;
        }
        layout.serialization_budget(identity)?;
        Ok(layout)
    }

    // Check the complete encoded size BEFORE allocating hexadecimal names,
    // the sorted serialization map, or the resulting TOML string. The per-entry
    // reserve includes table/key syntax and conservative space for both floats.
    fn serialization_budget(&self, identity: &str) -> io::Result<()> {
        let mut budget = identity
            .len()
            .checked_add(512)
            .ok_or_else(|| invalid("layout size limit exceeded"))?;
        if budget > MAX_LAYOUT_BYTES {
            return Err(invalid("layout size limit exceeded"));
        }
        for (name, position) in &self.positions {
            if !valid_name(name) || !position.valid() {
                return Err(invalid("invalid position or entry name"));
            }
            reserve_entry(&mut budget, name, MAX_LAYOUT_BYTES)?;
        }
        Ok(())
    }

    fn save_at(&self, dir: &Path, root: &Path) -> io::Result<()> {
        if self.positions.len() > MAX_POSITIONS {
            return Err(invalid("layout position limit exceeded"));
        }
        let identity = directory_identity(dir)?;
        let path = layout_path(root, &identity);
        self.serialization_budget(&identity)?;
        // Validate every existing field, including future versions, before
        // replacing anything. Errors are never converted into a fresh layout.
        match persistence::read_bounded(&path, MAX_LAYOUT_BYTES) {
            Ok(bytes) => {
                Self::decode_stored(&bytes, &identity)?;
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        let positions: BTreeMap<_, _> = self
            .positions
            .iter()
            .map(|(name, position)| {
                if !valid_name(name) || !position.valid() {
                    return Err(invalid("invalid position or entry name"));
                }
                Ok((encode_os(name), *position))
            })
            .collect::<io::Result<_>>()?;
        let stored = StoredLayout {
            version: 1,
            directory: identity,
            zoom: bounded_zoom(self.zoom),
            positions,
        };
        let data = toml::to_string(&stored).map_err(|e| invalid(e.to_string()))?;
        if data.len() > MAX_LAYOUT_BYTES {
            return Err(invalid("layout size limit exceeded"));
        }
        persistence::atomic_write(&path, data.as_bytes())
    }
}

fn reserve_entry(budget: &mut usize, name: &OsStr, limit: usize) -> io::Result<()> {
    #[cfg(windows)]
    let encoded_len = {
        use std::os::windows::ffi::OsStrExt;
        name.encode_wide().count().checked_mul(4)
    };
    #[cfg(not(windows))]
    let encoded_len = name.as_encoded_bytes().len().checked_mul(2);
    let next = encoded_len
        .and_then(|n| n.checked_add(256))
        .and_then(|n| budget.checked_add(n));
    match next {
        Some(next) if next <= limit => {
            *budget = next;
            Ok(())
        }
        _ => Err(invalid("layout size limit exceeded")),
    }
}

fn bounded_zoom(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.5, 2.0)
    } else {
        1.0
    }
}
fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
fn valid_name(name: &OsStr) -> bool {
    let path = Path::new(name);
    let mut components = path.components();
    matches!(components.next(), Some(std::path::Component::Normal(_)))
        && components.next().is_none()
        && name.as_encoded_bytes().len() <= MAX_NAME_BYTES
}
fn directory_identity(dir: &Path) -> io::Result<String> {
    let canonical = fs::canonicalize(dir)?;
    let metadata = fs::metadata(&canonical)?;
    if !metadata.is_dir() {
        return Err(invalid("layout target is not a directory"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok(format!(
            "unix:{}:{}:{}",
            metadata.dev(),
            metadata.ino(),
            encode_os(canonical.as_os_str())
        ))
    }
    #[cfg(not(unix))]
    Ok(format!("path:{}", encode_os(canonical.as_os_str())))
}
fn layout_path(root: &Path, identity: &str) -> PathBuf {
    // Stable FNV-1a key; full identity is checked on read and before replacement.
    let mut hash = 0xcbf29ce484222325u64;
    for byte in identity.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    root.join(format!("{hash:016x}.toml"))
}
fn encode_os(name: &OsStr) -> String {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        name.encode_wide().map(|v| format!("{v:04x}")).collect()
    }
    #[cfg(not(windows))]
    name.as_encoded_bytes()
        .iter()
        .map(|v| format!("{v:02x}"))
        .collect()
}
fn decode_name(key: &str) -> io::Result<OsString> {
    if key.len() > MAX_NAME_BYTES * 4 || !key.is_ascii() {
        return Err(invalid("invalid encoded name"));
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStringExt;
        if !key.len().is_multiple_of(4) {
            return Err(invalid("invalid encoded name"));
        }
        let wide = (0..key.len())
            .step_by(4)
            .map(|i| {
                u16::from_str_radix(&key[i..i + 4], 16).map_err(|_| invalid("invalid encoded name"))
            })
            .collect::<io::Result<Vec<_>>>()?;
        Ok(OsString::from_wide(&wide))
    }
    #[cfg(not(windows))]
    {
        if !key.len().is_multiple_of(2) {
            return Err(invalid("invalid encoded name"));
        }
        let bytes = (0..key.len())
            .step_by(2)
            .map(|i| {
                u8::from_str_radix(&key[i..i + 2], 16).map_err(|_| invalid("invalid encoded name"))
            })
            .collect::<io::Result<Vec<_>>>()?;
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;
            Ok(OsString::from_vec(bytes))
        }
        #[cfg(not(unix))]
        Ok(OsString::from(
            String::from_utf8(bytes).map_err(|_| invalid("invalid encoded name"))?,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip_zoom_and_directory_identity() {
        let root = persistence::test_dir("spatial");
        let directory = root.join("folder");
        let layouts = root.join("layouts");
        fs::create_dir(&directory).unwrap();
        let mut layout = SpatialLayout::load_at(&directory, &layouts).unwrap();
        assert_eq!(layout.zoom, 1.0);
        layout.set_zoom(99.0);
        layout
            .set_position(OsStr::new("Álbum.png"), Position { x: 12.5, y: -7.0 })
            .unwrap();
        layout.save_at(&directory, &layouts).unwrap();
        let loaded = SpatialLayout::load_at(&directory, &layouts).unwrap();
        assert_eq!(loaded.zoom, 2.0);
        assert_eq!(
            loaded.position(OsStr::new("Álbum.png")),
            Some(Position { x: 12.5, y: -7.0 })
        );
        assert!(fs::read_dir(&directory).unwrap().next().is_none());
        assert!(
            layout
                .set_position(OsStr::new("../escape"), Position { x: 0.0, y: 0.0 })
                .is_err()
        );
        assert!(
            layout
                .set_position(
                    OsStr::new("file"),
                    Position {
                        x: f32::NAN,
                        y: 0.0
                    }
                )
                .is_err()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_names_are_distinct_and_canonical_aliases_share_layout() {
        use std::os::unix::{ffi::OsStringExt, fs::symlink};
        let root = persistence::test_dir("spatial-names");
        let directory = root.join("folder");
        let layouts = root.join("layouts");
        fs::create_dir(&directory).unwrap();
        symlink(&directory, root.join("alias")).unwrap();
        let a = OsString::from_vec(vec![b'a', 0xff]);
        let b = OsString::from_vec(vec![b'a', 0xfe]);
        let mut layout = SpatialLayout::default();
        layout
            .set_position(&a, Position { x: 1.0, y: 2.0 })
            .unwrap();
        layout
            .set_position(&b, Position { x: 3.0, y: 4.0 })
            .unwrap();
        layout.save_at(&directory, &layouts).unwrap();
        let loaded = SpatialLayout::load_at(&root.join("alias"), &layouts).unwrap();
        assert_ne!(loaded.position(&a), loaded.position(&b));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn position_and_serialized_size_caps_are_enforced() {
        let mut layout = SpatialLayout::default();
        for index in 0..MAX_POSITIONS {
            if layout
                .set_position(
                    OsStr::new(&format!("entry{index}")),
                    Position { x: 0.0, y: 0.0 },
                )
                .is_err()
            {
                break;
            }
        }
        assert!(!layout.positions.is_empty());
        assert!(layout.positions.len() <= MAX_POSITIONS);
        assert!(
            layout
                .set_position(
                    OsStr::new("overflow-longer-than-last-entry"),
                    Position { x: 0.0, y: 0.0 }
                )
                .is_err()
        );
        layout
            .set_position(OsStr::new("entry0"), Position { x: 9.0, y: 0.0 })
            .unwrap();
        let root = persistence::test_dir("spatial-size");
        let directory = root.join("folder");
        let layouts = root.join("layouts");
        fs::create_dir(&directory).unwrap();
        fs::create_dir(&layouts).unwrap();
        let path = layout_path(&layouts, &directory_identity(&directory).unwrap());
        fs::write(&path, vec![b' '; MAX_LAYOUT_BYTES + 1]).unwrap();
        assert!(SpatialLayout::load_at(&directory, &layouts).is_err());
        assert!(layout.save_at(&directory, &layouts).is_err());
        assert_eq!(
            fs::metadata(&path).unwrap().len(),
            (MAX_LAYOUT_BYTES + 1) as u64
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn encoded_budget_boundary_without_large_allocations() {
        let name = OsStr::new("entry");
        let mut entry_cost = 0;
        reserve_entry(&mut entry_cost, name, MAX_LAYOUT_BYTES).unwrap();
        let mut budget = MAX_LAYOUT_BYTES - entry_cost;
        reserve_entry(&mut budget, name, MAX_LAYOUT_BYTES).unwrap();
        assert_eq!(budget, MAX_LAYOUT_BYTES);
        assert!(reserve_entry(&mut budget, name, MAX_LAYOUT_BYTES).is_err());
        assert_eq!(budget, MAX_LAYOUT_BYTES);
        let mut overflow = usize::MAX;
        assert!(reserve_entry(&mut overflow, name, usize::MAX).is_err());
    }

    #[test]
    fn invalid_existing_fields_and_future_version_preserve_exact_bytes() {
        let root = persistence::test_dir("spatial-invalid-fields");
        let directory = root.join("folder");
        let layouts = root.join("layouts");
        fs::create_dir(&directory).unwrap();
        fs::create_dir(&layouts).unwrap();
        let identity = directory_identity(&directory).unwrap();
        let path = layout_path(&layouts, &identity);
        let valid = format!("version=1\ndirectory={identity:?}\nzoom=1.0\n[positions]\n");
        let invalid_cases = [
            valid.replacen("version=1", "version=99", 1),
            valid.replacen("zoom=1.0", "zoom=nan", 1),
            format!("{valid}[positions.6162]\nx=nan\ny=0.0\n"),
            format!("{valid}[positions.6162]\nx=1000001.0\ny=0.0\n"),
            format!("{valid}[positions.2e2e2f78]\nx=0.0\ny=0.0\n"),
            format!("{valid}[positions.zz]\nx=0.0\ny=0.0\n"),
        ];
        for bytes in invalid_cases {
            fs::write(&path, &bytes).unwrap();
            assert!(SpatialLayout::load_at(&directory, &layouts).is_err());
            assert!(
                SpatialLayout::default()
                    .save_at(&directory, &layouts)
                    .is_err()
            );
            assert_eq!(fs::read(&path).unwrap(), bytes.as_bytes());
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn malformed_truncated_and_mismatched_layouts_are_not_overwritten() {
        let root = persistence::test_dir("spatial-bad");
        let directory = root.join("folder");
        let layouts = root.join("layouts");
        fs::create_dir(&directory).unwrap();
        fs::create_dir(&layouts).unwrap();
        let path = layout_path(&layouts, &directory_identity(&directory).unwrap());
        for bad in [
            "version=",
            "version=1\ndirectory='another'\nzoom=1.0\n[positions]\n",
        ] {
            fs::write(&path, bad).unwrap();
            assert!(SpatialLayout::load_at(&directory, &layouts).is_err());
            assert!(
                SpatialLayout::default()
                    .save_at(&directory, &layouts)
                    .is_err()
            );
            assert_eq!(fs::read_to_string(&path).unwrap(), bad);
        }
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn multi_icon_update_is_unchanged_if_any_candidate_is_invalid() {
        let mut layout = SpatialLayout::default();
        layout
            .set_position(OsStr::new("one"), Position { x: 1.0, y: 2.0 })
            .unwrap();
        assert!(
            layout
                .set_positions(&[
                    (OsStr::new("one"), Position { x: 30.0, y: 20.0 }),
                    (
                        OsStr::new("two"),
                        Position {
                            x: f32::NAN,
                            y: 0.0
                        }
                    )
                ])
                .is_err()
        );
        assert_eq!(
            layout.position(OsStr::new("one")),
            Some(Position { x: 1.0, y: 2.0 })
        );
        assert!(layout.position(OsStr::new("two")).is_none());
        layout
            .set_positions(&[
                (OsStr::new("one"), Position { x: 30.0, y: 20.0 }),
                (OsStr::new("two"), Position { x: 50.0, y: 10.0 }),
            ])
            .unwrap();
        assert_eq!(
            layout.position(OsStr::new("two")),
            Some(Position { x: 50.0, y: 10.0 })
        );
    }
}
