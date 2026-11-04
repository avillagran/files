//! Borrowed drop target resolution. Never selects, focuses, navigates or does I/O.
use crate::{
    model::{Action, Hit, Model, View},
    painter::Rect,
};
use files_core::listing::Entry;
use std::path::{Path, PathBuf};
#[derive(Clone, Debug)]
pub enum Scope {
    Pane(u64),
    Column(u64),
}
#[derive(Clone, Debug)]
pub struct Target {
    pub scope: Scope,
    pub path: PathBuf,
    pub owner: (u64, u64),
    pub revision: u64,
    pub generation: u64,
    pub rect: Rect,
    entry: Option<usize>,
}
fn path(
    parent: &Path,
    entry: Option<&Entry>,
    hidden: bool,
    query: &str,
) -> Result<PathBuf, &'static str> {
    let name = if let Some(e) = entry {
        if !e.is_dir() || !crate::columns::accepts(e, hidden, query) {
            return Err("Suelta sobre una carpeta o el fondo del panel");
        }
        Some(e.name.as_os_str())
    } else {
        None
    };
    let size = parent
        .as_os_str()
        .as_encoded_bytes()
        .len()
        .checked_add(name.map_or(0, |n| n.as_encoded_bytes().len().saturating_add(1)))
        .ok_or("Ruta destino excede el presupuesto")?;
    if !parent.is_absolute() || size > 4096 {
        return Err("Ruta destino excede4096B o no es absoluta");
    }
    Ok(name.map_or_else(|| parent.to_path_buf(), |n| parent.join(n)))
}
fn pane(
    m: &Model,
    id: u64,
    rev: u64,
    entry: Option<usize>,
    rect: Rect,
) -> Result<Target, &'static str> {
    let (parent, entries, query, owner, revision, loaded) = if id == m.dual.active_id {
        (
            &m.path,
            &m.entries,
            &m.query,
            m.pane_owner(),
            m.pane_revision,
            m.loaded,
        )
    } else if m.dual.enabled
        && let Some(b) = m.dual.other.as_ref().filter(|b| b.id == id)
    {
        (
            &b.path,
            &b.entries,
            &b.query,
            (b.id, b.tabs.active),
            b.revision,
            b.loaded,
        )
    } else {
        return Err("Panel destino ya no existe");
    };
    if !loaded || revision != rev {
        return Err("Listado destino cambió o sigue cargando");
    }
    let e = entry
        .map(|i| entries.get(i).ok_or("Índice destino obsoleto"))
        .transpose()?;
    Ok(Target {
        scope: Scope::Pane(id),
        path: path(parent, e, m.show_hidden, query)?,
        owner,
        revision,
        generation: m.generation,
        rect,
        entry,
    })
}
fn column(m: &Model, id: u64, entry: Option<usize>, rect: Rect) -> Result<Target, &'static str> {
    let index = m
        .columns
        .columns
        .iter()
        .position(|c| c.id == id)
        .ok_or("Columna destino obsoleta")?;
    let c = &m.columns.columns[index];
    let (parent, entries, query, loaded) = if index == m.columns.active {
        (&m.path, &m.entries, &m.query, m.loaded)
    } else {
        (&c.path, &c.data.entries, &c.data.query, c.data.loaded)
    };
    if !loaded {
        return Err("Columna destino sigue cargando");
    }
    let e = entry
        .map(|i| entries.get(i).ok_or("Índice destino obsoleto"))
        .transpose()?;
    Ok(Target {
        scope: Scope::Column(id),
        path: path(parent, e, m.show_hidden, query)?,
        owner: m.pane_owner(),
        revision: m.pane_revision,
        generation: m.generation,
        rect,
        entry,
    })
}
pub fn blocked(m: &Model) -> bool {
    m.closing
        || m.search.visible
        || m.bookmarks.panel.is_some()
        || m.commands.is_some()
        || m.size_panel.is_some()
        || m.trash_browser.is_some()
        || m.sftp.form.is_some()
        || m.editing
        || m.filter_edit.active
        || m.notice_open
        || m.actions_menu
        || m.settings
        || m.help.is_some()
        || m.context_menu.is_some()
        || m.rename_dialog.is_some()
        || m.copy_form.is_some()
        || m.properties_dialog.is_some()
        || m.preview.visible()
        || m.recovery_catalog.open
        || m.mutations.busy
        || m.mutation_applied
        || m.selection_refreshing()
}
pub fn resolve(
    m: &Model,
    x: i32,
    y: i32,
    body: Rect,
    generation: u64,
) -> Result<Target, &'static str> {
    if generation != m.generation || blocked(m) || !body.contains(x, y) {
        return Err("Destino del drop obsoleto o controles activos");
    }
    let hit = m.hits.iter().rev().find(|h| h.rect.contains(x, y));
    if let Some(Hit {
        generation: g,
        action,
        rect,
    }) = hit
    {
        if *g != generation {
            return Err("Target de otra generación");
        }
        return match action {
            Action::Entry(i) => pane(m, m.dual.active_id, m.pane_revision, Some(*i), *rect),
            Action::PaneEntry(id, rev, i) => pane(m, *id, *rev, Some(*i), *rect),
            Action::PaneBackground(id, rev) => pane(m, *id, *rev, None, *rect),
            Action::PaneFocus(id) => {
                let rev = if *id == m.dual.active_id {
                    m.pane_revision
                } else {
                    m.dual
                        .other
                        .as_ref()
                        .filter(|b| b.id == *id)
                        .ok_or("Panel obsoleto")?
                        .revision
                };
                pane(m, *id, rev, None, *rect)
            }
            Action::ColumnEntry(id, i) => column(m, *id, Some(*i), *rect),
            Action::ColumnFocus(id) => column(m, *id, None, *rect),
            _ => Err("No se puede soltar sobre este control"),
        };
    }
    if m.view == View::Columns && !m.dual.enabled {
        let a = m.columns.area;
        if !a.contains(x, y) {
            return Err("Fuera de las columnas");
        }
        let index = ((x - a.x + m.columns.horizontal) / crate::columns_view::WIDTH).max(0) as usize;
        let c = m
            .columns
            .columns
            .get(index)
            .ok_or("Fuera de las columnas cargadas")?;
        return column(m, c.id, None, body);
    }
    if m.dual.enabled {
        return Err("Fuera del fondo de un panel");
    }
    pane(m, m.dual.active_id, m.pane_revision, None, body)
}

impl Target {
    fn still_points_to(&self, parent: &Path, entries: &[Entry], query: &str, hidden: bool) -> bool {
        match self.entry {
            None => self.path == parent,
            Some(index) => entries.get(index).is_some_and(|entry| {
                entry.is_dir()
                    && crate::columns::accepts(entry, hidden, query)
                    && self.path.parent() == Some(parent)
                    && self.path.file_name() == Some(entry.name.as_os_str())
            }),
        }
    }
    pub fn current(&self, m: &Model) -> bool {
        if self.generation != m.generation || blocked(m) {
            return false;
        }
        match self.scope {
            Scope::Pane(id) if id == m.dual.active_id => {
                self.owner == m.pane_owner()
                    && self.revision == m.pane_revision
                    && m.loaded
                    && self.still_points_to(&m.path, &m.entries, &m.query, m.show_hidden)
            }
            Scope::Pane(id) => {
                m.dual.enabled
                    && m.dual.other.as_ref().is_some_and(|b| {
                        b.id == id
                            && (b.id, b.tabs.active) == self.owner
                            && b.revision == self.revision
                            && b.loaded
                            && self.still_points_to(&b.path, &b.entries, &b.query, m.show_hidden)
                    })
            }
            Scope::Column(id) => {
                m.view == View::Columns
                    && !m.dual.enabled
                    && m.pane_owner() == self.owner
                    && m.pane_revision == self.revision
                    && m.columns.columns.iter().enumerate().any(|(i, c)| {
                        c.id == id
                            && if i == m.columns.active {
                                m.loaded
                                    && self.still_points_to(
                                        &m.path,
                                        &m.entries,
                                        &m.query,
                                        m.show_hidden,
                                    )
                            } else {
                                c.data.loaded
                                    && self.still_points_to(
                                        &c.path,
                                        &c.data.entries,
                                        &c.data.query,
                                        m.show_hidden,
                                    )
                            }
                    })
            }
        }
    }
}
#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use files_core::listing::Kind;
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    fn entry(name: &str, kind: Kind) -> Entry {
        Entry {
            name: name.into(),
            kind,
            metadata: None,
            stat_attempted: false,
        }
    }
    fn model() -> Model {
        let mut m = Model::new(PathBuf::from("/native-drop-fixture-A"));
        m.view = View::List;
        m.loaded = true;
        m.show_hidden = false;
        m.entries = vec![
            entry("folder", Kind::Directory),
            entry("file", Kind::File),
            entry(".hidden", Kind::Directory),
        ];
        m.order = vec![0, 1];
        m.selection.insert(1);
        m.selected = Some(1);
        m.anchor = Some(1);
        m.hits.clear();
        m
    }
    const BODY: Rect = Rect {
        x: 0,
        y: 0,
        w: 800,
        h: 500,
    };
    fn hit(m: &mut Model, action: Action) {
        m.hits = vec![Hit {
            generation: m.generation,
            rect: BODY,
            action,
        }];
    }
    #[test]
    fn hover_resolves_raw_without_focus_selection_or_navigation_and_rejects_stale() {
        let mut m = model();
        m.entries[0].name = OsString::from_vec(vec![b'f', 0xff]);
        hit(&mut m, Action::Entry(0));
        let before = (
            m.path.clone(),
            m.pane_owner(),
            m.selection.clone(),
            m.selected,
            m.anchor,
            m.generation,
        );
        let t = resolve(&m, 10, 10, BODY, m.generation).unwrap();
        assert_eq!(t.path, m.path.join(&m.entries[0].name));
        assert!(t.current(&m));
        m.entries[0].name = OsString::from("replacement");
        assert!(
            !t.current(&m),
            "an unchanged index cannot authorize a different name"
        );
        m.entries[0].name = OsString::from_vec(vec![b'f', 0xff]);
        m.query = "no-match".into();
        assert!(
            !t.current(&m),
            "a newly filtered target cannot remain active"
        );
        m.query.clear();
        assert!(t.current(&m));
        assert_eq!(
            before,
            (
                m.path.clone(),
                m.pane_owner(),
                m.selection.clone(),
                m.selected,
                m.anchor,
                m.generation
            )
        );
        m.pane_revision += 1;
        assert!(!t.current(&m));
        m.hits[0].generation = m.generation.wrapping_sub(1);
        assert!(resolve(&m, 10, 10, BODY, m.generation).is_err());
        hit(&mut m, Action::Entry(1));
        assert!(resolve(&m, 10, 10, BODY, m.generation).is_err());
        hit(&mut m, Action::Entry(2));
        assert!(resolve(&m, 10, 10, BODY, m.generation).is_err());
        hit(&mut m, Action::Settings);
        assert!(resolve(&m, 10, 10, BODY, m.generation).is_err());
        m.hits.clear();
        assert_eq!(
            resolve(&m, 10, 10, BODY, m.generation).unwrap().path,
            m.path
        );
    }
    #[test]
    fn other_pane_and_columns_do_not_activate_and_ack_rechecks_their_revision() {
        let mut m = model();
        m.toggle_dual();
        assert!(m.dual.enabled);
        let b = m.dual.other.as_mut().unwrap();
        b.path = PathBuf::from("/native-drop-fixture-B");
        b.loaded = true;
        b.entries = vec![entry("folder", Kind::Directory)];
        b.order = vec![0];
        let (id, rev) = (b.id, b.revision);
        hit(&mut m, Action::PaneEntry(id, rev, 0));
        let owner = m.pane_owner();
        let selection = m.selection.clone();
        let t = resolve(&m, 20, 20, BODY, m.generation).unwrap();
        assert_eq!(t.path, Path::new("/native-drop-fixture-B/folder"));
        assert_eq!(owner, m.pane_owner());
        assert_eq!(selection, m.selection);
        assert!(t.current(&m));
        m.dual.other.as_mut().unwrap().revision += 1;
        assert!(!t.current(&m));
        assert!(resolve(&m, 20, 20, BODY, m.generation).is_err());
        m.toggle_dual();
        m.view = View::Columns;
        m.columns.active = 0;
        m.columns.columns = vec![
            crate::columns::Column {
                id: 101,
                watch_id: 101,
                path: m.path.clone(),
                data: Default::default(),
            },
            crate::columns::Column {
                id: 102,
                watch_id: 102,
                path: PathBuf::from("/native-drop-fixture-parent"),
                data: Default::default(),
            },
        ];
        m.columns.columns[1].data.loaded = true;
        hit(&mut m, Action::ColumnFocus(102));
        let t = resolve(&m, 10, 10, BODY, m.generation).unwrap();
        assert_eq!(t.path, Path::new("/native-drop-fixture-parent"));
        assert_eq!(m.columns.active, 0);
        assert!(t.current(&m));
        m.columns.columns.remove(1);
        assert!(!t.current(&m));
    }
}

pub fn paint(p: &mut dyn crate::painter::Painter, m: &Model) {
    if let Some(t) = m.native_transfer.hover().filter(|t| t.current(m)) {
        let r = t.rect;
        let c = m.palette.accent;
        for edge in [
            Rect::new(r.x, r.y, r.w, 2),
            Rect::new(r.x, r.y + r.h - 2, r.w, 2),
            Rect::new(r.x, r.y, 2, r.h),
            Rect::new(r.x + r.w - 2, r.y, 2, r.h),
        ] {
            p.rect(edge, c);
        }
        if r.w >= 120 && r.h >= 28 {
            p.rect(
                Rect::new(r.x + 3, r.y + 3, (r.w - 6).min(320), 24),
                m.palette.darker_background,
            );
            p.text(
                Rect::new(r.x + 8, r.y + 3, (r.w - 16).min(310), 24),
                "  Soltar para revisar copia",
                11,
                c,
                true,
                false,
            );
        }
    }
}
