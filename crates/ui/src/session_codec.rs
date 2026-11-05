//! Versioned local session metadata. Every length is checked before copying;
//! no listing, bitmap, operation, helper or hidden preference is serialized.
use crate::{
    model::View,
    refresh_selection::Snapshot,
    tabs::{self, Saved},
};
use files_core::{listing::Sort, navigation_history::History};
use std::{
    collections::VecDeque,
    ffi::OsStr,
    path::{Path, PathBuf},
};
pub const MAX_BYTES: usize = 4 * 1024 * 1024;
const MAGIC: &[u8] = b"files-session\0";
const VERSION: u32 = 1;
pub struct Session {
    pub tabs: Vec<Saved>,
    pub closed: Vec<Saved>,
    pub active: usize,
}
pub struct Record<'a> {
    pub path: &'a Path,
    pub history: &'a History,
    pub selection: &'a Snapshot,
    pub view: View,
    pub sort: Sort,
    pub descending: bool,
    pub query: &'a str,
    pub scroll: i32,
    pub grid_zoom: f32,
    pub pan_x: f32,
}
impl<'a> From<&'a Saved> for Record<'a> {
    fn from(s: &'a Saved) -> Self {
        Self {
            path: &s.path,
            history: &s.history,
            selection: &s.selection,
            view: s.view,
            sort: s.sort,
            descending: s.descending,
            query: &s.query,
            scroll: s.scroll,
            grid_zoom: s.grid_zoom,
            pan_x: s.pan_x,
        }
    }
}
fn geometry(
    path: &Path,
    query: &str,
    scroll: i32,
    zoom: f32,
    pan: f32,
) -> Result<(), &'static str> {
    files_core::navigation_history::validate(path)?;
    if query.len() > 4096 || query.contains('\0') {
        return Err("Filtro de sesión inválido");
    }
    if !(0..=100_000_000).contains(&scroll)
        || !zoom.is_finite()
        || !(0.65..=1.8).contains(&zoom)
        || !pan.is_finite()
        || pan.abs() > 100_000_000.
    {
        return Err("Geometría de sesión inválida");
    }
    Ok(())
}
fn history_charge<'a>(paths: impl Iterator<Item = &'a Path>) -> usize {
    4096 + 128 * std::mem::size_of::<PathBuf>()
        + paths.map(|p| p.as_os_str().len() + 128).sum::<usize>()
}
fn record_charge(s: &Record<'_>) -> Result<usize, &'static str> {
    geometry(s.path, s.query, s.scroll, s.grid_zoom, s.pan_x)?;
    let (back, forward) = s.history.paths();
    if back.len() + forward.len() > 64 {
        return Err("Historial de sesión supera64 rutas");
    }
    let selection = Snapshot::estimate(
        s.selection.names(),
        s.selection.focus(),
        s.selection.anchor(),
        tabs::SELECTION_LIMIT,
    )?;
    let charge = selection
        + history_charge(back.iter().chain(forward).map(PathBuf::as_path))
        + s.path.as_os_str().len()
        + s.query.len()
        + 8192;
    if charge > tabs::SNAPSHOT_LIMIT {
        return Err("Estado de sesión excede1MiB");
    }
    Ok(charge)
}
struct Writer {
    count: usize,
    data: Option<Vec<u8>>,
}
impl Writer {
    fn bytes(&mut self, b: &[u8]) -> Result<(), &'static str> {
        self.count = self
            .count
            .checked_add(b.len())
            .filter(|n| *n <= MAX_BYTES)
            .ok_or("Sesión supera4MiB")?;
        if let Some(data) = &mut self.data {
            data.extend_from_slice(b);
        }
        Ok(())
    }
    fn u32(&mut self, v: u32) -> Result<(), &'static str> {
        self.bytes(&v.to_le_bytes())
    }
    fn blob(&mut self, b: &[u8]) -> Result<(), &'static str> {
        if b.len() > 4096 {
            return Err("Campo de sesión supera4096B");
        }
        self.u32(b.len() as u32)?;
        self.bytes(b)
    }
    fn optional(&mut self, n: Option<&OsStr>) -> Result<(), &'static str> {
        self.bytes(&[u8::from(n.is_some())])?;
        if let Some(n) = n {
            self.blob(n.as_encoded_bytes())?;
        }
        Ok(())
    }
    fn record(&mut self, s: &Record<'_>) -> Result<(), &'static str> {
        record_charge(s)?;
        Snapshot::estimate(
            s.selection.names(),
            s.selection.focus(),
            s.selection.anchor(),
            tabs::SELECTION_LIMIT,
        )?;
        self.blob(s.path.as_os_str().as_encoded_bytes())?;
        let (back, forward) = s.history.paths();
        if back.len() + forward.len() > 64 {
            return Err("Historial de sesión supera64 rutas");
        }
        for paths in [back, forward] {
            self.u32(paths.len() as u32)?;
            for p in paths {
                files_core::navigation_history::validate(p)?;
                self.blob(p.as_os_str().as_encoded_bytes())?;
            }
        }
        self.bytes(&[
            match s.view {
                View::Groups => 0,
                View::List => 1,
                View::Grid => 2,
                View::Spatial => 3,
                View::Columns => 5,
                View::SpaceMap => 4,
            },
            match s.sort {
                Sort::Name => 0,
                Sort::Size => 1,
                Sort::Modified => 2,
                Sort::Type => 3,
            },
            u8::from(s.descending),
        ])?;
        self.blob(s.query.as_bytes())?;
        self.bytes(&s.scroll.to_le_bytes())?;
        self.bytes(&s.grid_zoom.to_le_bytes())?;
        self.bytes(&s.pan_x.to_le_bytes())?;
        self.u32(s.selection.names().count() as u32)?;
        for n in s.selection.names() {
            valid_name(n)?;
            self.blob(n.as_encoded_bytes())?;
        }
        for n in [s.selection.focus(), s.selection.anchor()] {
            if let Some(n) = n {
                valid_name(n)?;
            }
            self.optional(n)?;
        }
        Ok(())
    }
}
pub fn encode(
    tabs: &[Record<'_>],
    closed: &[Record<'_>],
    active: usize,
) -> Result<Vec<u8>, &'static str> {
    if !cfg!(unix) {
        return Err("Sesión raw pendiente en esta plataforma");
    }
    if tabs.is_empty() || tabs.len() > 8 || closed.len() > 8 || active >= tabs.len() {
        return Err("Cantidad de pestañas inválida");
    }
    let mut stored = 0usize;
    for (i, s) in tabs.iter().chain(closed).enumerate() {
        let charge = record_charge(s)?;
        if i != active {
            stored = stored
                .checked_add(charge)
                .filter(|n| *n <= tabs::STORED_LIMIT)
                .ok_or("Estado guardado de sesión excede3MiB")?;
        }
    }
    let write = |w: &mut Writer| -> Result<(), &'static str> {
        w.bytes(MAGIC)?;
        w.u32(VERSION)?;
        w.bytes(&[1, tabs.len() as u8, closed.len() as u8, active as u8])?;
        for s in tabs.iter().chain(closed) {
            w.record(s)?;
        }
        Ok(())
    };
    let mut pre = Writer {
        count: 0,
        data: None,
    };
    write(&mut pre)?;
    let mut w = Writer {
        count: 0,
        data: Some(Vec::with_capacity(pre.count)),
    };
    write(&mut w)?;
    Ok(w.data.unwrap())
}
fn os(bytes: &[u8]) -> Result<&OsStr, &'static str> {
    if bytes.is_empty() || bytes.len() > 4096 || bytes.contains(&0) {
        return Err("Ruta/nombre raw inválido");
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        Ok(OsStr::from_bytes(bytes))
    }
    #[cfg(not(unix))]
    {
        Err("Sesión raw pendiente en esta plataforma")
    }
}
fn valid_name(n: &OsStr) -> Result<(), &'static str> {
    use std::path::Component;
    let mut parts = Path::new(n).components();
    if !matches!(parts.next(), Some(Component::Normal(_)))
        || parts.next().is_some()
        || n.as_encoded_bytes().contains(&0)
        || n.len() > 4096
    {
        return Err("Nombre de selección inválido");
    }
    Ok(())
}
struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], &'static str> {
        let end = self
            .at
            .checked_add(n)
            .filter(|n| *n <= self.data.len())
            .ok_or("Sesión truncada")?;
        let b = &self.data[self.at..end];
        self.at = end;
        Ok(b)
    }
    fn byte(&mut self) -> Result<u8, &'static str> {
        Ok(self.take(1)?[0])
    }
    fn u32(&mut self) -> Result<u32, &'static str> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn count(&mut self, limit: usize) -> Result<usize, &'static str> {
        let n = self.u32()? as usize;
        if n > limit || n > self.data.len() - self.at {
            return Err("Cantidad de sesión inválida");
        }
        Ok(n)
    }
    fn blob(&mut self) -> Result<&'a [u8], &'static str> {
        let n = self.u32()? as usize;
        if n > 4096 {
            return Err("Campo de sesión supera4096B");
        }
        self.take(n)
    }
    fn path(&mut self) -> Result<&'a Path, &'static str> {
        let p = Path::new(os(self.blob()?)?);
        files_core::navigation_history::validate(p)?;
        Ok(p)
    }
    fn optional(&mut self) -> Result<Option<&'a OsStr>, &'static str> {
        match self.byte()? {
            0 => Ok(None),
            1 => {
                let n = os(self.blob()?)?;
                valid_name(n)?;
                Ok(Some(n))
            }
            _ => Err("Flag de sesión inválido"),
        }
    }
    fn record(
        &mut self,
        remaining: usize,
        materialize: bool,
    ) -> Result<(usize, Option<Saved>), &'static str> {
        let path = self.path()?;
        let mut history_count = 0;
        let mut history_bytes = 4096 + 128 * std::mem::size_of::<PathBuf>();
        let history_start = self.at;
        for _ in 0..2 {
            let n = self.count(64 - history_count)?;
            history_count += n;
            for _ in 0..n {
                history_bytes += self.path()?.as_os_str().len() + 128;
            }
        }
        let history_end = self.at;
        let view_tag = self.byte()?;
        let view = match view_tag {
            0 => View::Groups,
            1 => View::List,
            2 => View::Grid,
            3 | 5 => View::Columns,
            4 => View::SpaceMap,
            _ => return Err("Vista de sesión inválida"),
        };
        let sort = match self.byte()? {
            0 => Sort::Name,
            1 => Sort::Size,
            2 => Sort::Modified,
            3 => Sort::Type,
            _ => return Err("Orden de sesión inválido"),
        };
        let descending = match self.byte()? {
            0 => false,
            1 => true,
            _ => return Err("Flag de sesión inválido"),
        };
        let query = std::str::from_utf8(self.blob()?).map_err(|_| "Filtro no UTF8")?;
        let scroll = i32::from_le_bytes(self.take(4)?.try_into().unwrap());
        let grid_zoom = f32::from_le_bytes(self.take(4)?.try_into().unwrap());
        let pan_x = f32::from_le_bytes(self.take(4)?.try_into().unwrap());
        geometry(path, query, scroll, grid_zoom, pan_x)?;
        let (scroll, grid_zoom, pan_x) = if view_tag == 3 {
            (0, 1.0, 0.0)
        } else {
            (scroll, grid_zoom, pan_x)
        };
        let count = self.count(100_000)?;
        let start = self.at;
        let mut selection_charge = 64 * 1024usize;
        for _ in 0..count {
            let n = os(self.blob()?)?;
            valid_name(n)?;
            selection_charge = selection_charge
                .checked_add(n.len() + std::mem::size_of::<std::ffi::OsString>() + 32)
                .filter(|n| *n <= tabs::SELECTION_LIMIT)
                .ok_or("Selección de sesión supera256KiB")?;
        }
        let names_end = self.at;
        let focus = self.optional()?;
        let anchor = self.optional()?;
        for n in [focus, anchor].into_iter().flatten() {
            selection_charge = selection_charge
                .checked_add(n.len() + 32)
                .filter(|n| *n <= tabs::SELECTION_LIMIT)
                .ok_or("Foco de sesión supera presupuesto")?;
        }
        let charge = selection_charge + history_bytes + path.as_os_str().len() + query.len() + 8192;
        if charge > tabs::SNAPSHOT_LIMIT || charge > remaining {
            return Err("Estado de sesión excede presupuesto de pestañas");
        }
        if !materialize {
            return Ok((charge, None));
        }
        // Only the second pass, after ALL records passed, may copy raw fields.
        let mut h = Reader {
            data: &self.data[history_start..history_end],
            at: 0,
        };
        let mut lists = [VecDeque::new(), VecDeque::new()];
        for paths in &mut lists {
            let n = h.count(64)?;
            for _ in 0..n {
                paths.push_back(h.path()?.to_path_buf());
            }
        }
        let [back, forward] = lists;
        let history = History::restore(back, forward)?;
        let mut names_reader = Reader {
            data: &self.data[start..names_end],
            at: 0,
        };
        let mut names = Vec::with_capacity(count);
        for _ in 0..count {
            names.push(os(names_reader.blob()?)?);
        }
        let selection = Snapshot::capture_with_anchor(
            names.iter().copied(),
            focus,
            anchor,
            tabs::SELECTION_LIMIT,
        )?;
        Ok((
            charge,
            Some(Saved {
                path: path.into(),
                history,
                selection,
                view,
                sort,
                descending,
                query: query.into(),
                scroll,
                grid_zoom,
                pan_x,
                charge,
            }),
        ))
    }
}
fn header(bytes: &[u8]) -> Result<(Reader<'_>, usize, usize, usize), &'static str> {
    if bytes.len() > MAX_BYTES {
        return Err("Sesión supera4MiB");
    }
    let mut r = Reader { data: bytes, at: 0 };
    if r.take(MAGIC.len())? != MAGIC {
        return Err("Formato de sesión inválido");
    }
    if r.u32()? != VERSION {
        return Err("Versión de sesión incompatible; archivo preservado");
    }
    if r.byte()? != 1 || !cfg!(unix) {
        return Err("Plataforma de sesión incompatible");
    }
    let count = r.byte()? as usize;
    let closed = r.byte()? as usize;
    let active = r.byte()? as usize;
    if count == 0 || count > 8 || closed > 8 || active >= count {
        return Err("Cantidad de pestañas inválida");
    }
    Ok((r, count, closed, active))
}
/// No owned paths/names/History are constructed; used for old/new-file checks.
pub fn validate(bytes: &[u8]) -> Result<(), &'static str> {
    let (mut r, count, closed, active) = header(bytes)?;
    let mut stored = 0;
    for i in 0..count + closed {
        let is_active = i == active;
        let (charge, _) = r.record(
            if is_active {
                tabs::SNAPSHOT_LIMIT
            } else {
                tabs::STORED_LIMIT - stored
            },
            false,
        )?;
        if !is_active {
            stored += charge;
        }
    }
    if r.at != bytes.len() {
        return Err("Bytes extra en sesión");
    }
    Ok(())
}
pub fn decode(bytes: &[u8]) -> Result<Session, &'static str> {
    validate(bytes)?;
    let (mut r, count, closed_count, active) = header(bytes)?;
    let mut tabs = Vec::with_capacity(count);
    let mut closed = Vec::with_capacity(closed_count);
    for i in 0..count + closed_count {
        let (_, s) = r.record(tabs::SNAPSHOT_LIMIT, true)?;
        let s = s.expect("materialized validated record");
        if i < count {
            tabs.push(s);
        } else {
            closed.push(s);
        }
    }
    Ok(Session {
        tabs,
        closed,
        active,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn saved() -> Saved {
        let mut history = History::default();
        history.visit(Path::new("/old"), Path::new("/new")).unwrap();
        Saved {
            path: "/new".into(),
            history,
            selection: Snapshot::capture_with_anchor(
                [OsStr::new("a"), OsStr::new("b")].into_iter(),
                Some(OsStr::new("b")),
                Some(OsStr::new("a")),
                tabs::SELECTION_LIMIT,
            )
            .unwrap(),
            view: View::Grid,
            sort: Sort::Modified,
            descending: true,
            query: "café".into(),
            scroll: 120,
            grid_zoom: 1.25,
            pan_x: 48.,
            charge: 0,
        }
    }
    #[test]
    fn roundtrip_preserves_state_and_rejects_truncation_trailing_version() {
        let s = saved();
        let b = encode(&[Record::from(&s)], &[], 0).unwrap();
        let d = decode(&b).unwrap();
        let t = &d.tabs[0];
        assert_eq!(t.path, s.path);
        assert_eq!(t.query, "café");
        assert_eq!(t.scroll, 120);
        assert!(t.selection.contains(OsStr::new("b")) && t.selection.anchored(OsStr::new("a")));
        assert!(t.history.can_back());
        for n in 0..b.len() {
            assert!(decode(&b[..n]).is_err());
        }
        let mut extra = b.clone();
        extra.push(0);
        assert!(decode(&extra).is_err());
        let mut future = b;
        future[MAGIC.len()] = 99;
        assert!(decode(&future).is_err());
    }
    #[test]
    fn legacy_spatial_migrates_without_icon_geometry_and_new_columns_roundtrip() {
        let mut s = saved();
        s.view = View::Spatial;
        let legacy = encode(&[Record::from(&s)], &[], 0).unwrap();
        let d = decode(&legacy).unwrap();
        let t = &d.tabs[0];
        assert!(t.view == View::Columns);
        assert_eq!(t.path, s.path);
        assert!(t.history.can_back());
        assert!(t.selection.contains(OsStr::new("a")));
        assert_eq!(t.query, s.query);
        assert_eq!((t.scroll, t.grid_zoom, t.pan_x), (0, 1., 0.));
        s.view = View::Columns;
        s.scroll = 144;
        s.grid_zoom = 1.;
        s.pan_x = 0.;
        let new = decode(&encode(&[Record::from(&s)], &[], 0).unwrap()).unwrap();
        assert!(new.tabs[0].view == View::Columns);
        assert_eq!(new.tabs[0].scroll, 144);
        assert!(new.tabs[0].selection.anchored(OsStr::new("a")));
    }
    #[test]
    fn limits_and_geometry_fail_before_output() {
        let mut s = saved();
        s.grid_zoom = f32::NAN;
        assert!(encode(&[Record::from(&s)], &[], 0).is_err());
        s.grid_zoom = 1.;
        s.query = "a".repeat(4097);
        assert!(encode(&[Record::from(&s)], &[], 0).is_err());
        assert!(decode(&vec![0; MAX_BYTES + 1]).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn raw_paths_names_survive() {
        use std::os::unix::ffi::OsStrExt;
        let mut s = saved();
        s.path = Path::new(OsStr::from_bytes(b"/raw-\xff")).into();
        s.selection =
            Snapshot::capture([OsStr::from_bytes(b"name-\xfe")].into_iter(), None).unwrap();
        let d = decode(&encode(&[Record::from(&s)], &[], 0).unwrap()).unwrap();
        assert_eq!(d.tabs[0].path, s.path);
        assert!(
            d.tabs[0]
                .selection
                .contains(OsStr::from_bytes(b"name-\xfe"))
        );
    }
    #[test]
    fn encoder_and_validator_share_aggregate_budget_including_closed() {
        let mut small = saved();
        small.selection = Snapshot::capture(std::iter::empty(), None).unwrap();
        let active = (0..8).map(|_| Record::from(&small)).collect::<Vec<_>>();
        let closed = (0..8).map(|_| Record::from(&small)).collect::<Vec<_>>();
        let bytes = encode(&active, &closed, 7).unwrap();
        assert!(validate(&bytes).is_ok());
        let parsed = decode(&bytes).unwrap();
        assert_eq!(parsed.tabs.len(), 8);
        assert_eq!(parsed.closed.len(), 8);
        assert_eq!(parsed.active, 7);
        let mut big = saved();
        let mut history = History::default();
        let a = PathBuf::from(format!("/{}", "a".repeat(4095)));
        let b = PathBuf::from(format!("/{}", "b".repeat(4095)));
        for i in 0..64 {
            if i % 2 == 0 {
                history.visit(&a, &b).unwrap();
            } else {
                history.visit(&b, &a).unwrap();
            }
        }
        big.history = history;
        let active = (0..8).map(|_| Record::from(&big)).collect::<Vec<_>>();
        assert!(decode(&encode(&active, &[], 7).unwrap()).is_ok());
        let closed = (0..8).map(|_| Record::from(&big)).collect::<Vec<_>>();
        assert!(encode(&active, &closed, 7).is_err());
    }
}
