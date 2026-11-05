//! Captura explícita y restauración ligada al propietario de navegación.
use crate::model::{Model, View};
use files_core::{
    folder_views::{Entry, Op, Settings},
    listing::Sort,
};
use std::path::PathBuf;

pub struct Restore {
    owner: (u64, u64),
    generation: u64,
    path: PathBuf,
    baseline: Settings,
}
impl Model {
    fn folder_settings(&self) -> Option<Settings> {
        let view = match self.view {
            View::Groups => 0,
            View::List => 1,
            View::Columns => 2,
            View::Grid => 3,
            View::SpaceMap => 4,
            View::Spatial => return None,
        };
        Some(Settings {
            view,
            sort: match self.sort {
                Sort::Name => 0,
                Sort::Size => 1,
                Sort::Modified => 2,
                Sort::Type => 3,
            },
            descending: self.descending,
            zoom: (self.grid_zoom * 100.).round().clamp(65., 180.) as u8,
            scroll: self.scroll.clamp(0, 100_000_000) as u32,
        })
    }
    pub(crate) fn arm_folder_restore(&mut self) {
        self.folder_restore = self
            .folder_settings()
            .filter(|_| self.path.is_absolute() && self.path.as_os_str().len() <= 4096)
            .map(|baseline| Restore {
                owner: self.pane_owner(),
                generation: self.generation,
                path: self.path.clone(),
                baseline,
            });
        if !self.folder_views.loaded && self.folder_views.can_request() {
            self.folder_views.request(Op::Read);
        }
    }
    pub(crate) fn pump_folder_restore(&mut self) -> bool {
        let Some(target) = self.folder_restore.as_ref() else {
            return false;
        };
        if self.closing
            || !self.error.is_empty()
            || target.owner != self.pane_owner()
            || target.generation != self.generation
            || target.path != self.path
            || Some(target.baseline) != self.folder_settings()
        {
            self.folder_restore = None;
            return false;
        }
        if !self.loaded || !self.folder_views.loaded {
            return false;
        }
        let target = self.folder_restore.take().unwrap();
        let Some(settings) = self
            .folder_views
            .document
            .get(&target.path)
            .map(|e| e.settings)
        else {
            return false;
        };
        let view = match settings.view {
            0 => View::Groups,
            1 => View::List,
            2 => View::Columns,
            3 => View::Grid,
            _ => View::SpaceMap,
        };
        // El modo de dos paneles es del grupo, nunca de la preferencia de carpeta.
        if !self.dual.enabled {
            if view == View::Columns
                && crate::columns::charge(&self.entries, &self.order, &self.selection)
                    > crate::columns::DATA_LIMIT
            {
                self.folder_views.error =
                    "Vista de columnas guardada supera el presupuesto del listado actual".into();
                return true;
            }
            self.view = view;
        }
        self.sort = match settings.sort {
            0 => Sort::Name,
            1 => Sort::Size,
            3 => Sort::Type,
            _ => Sort::Modified,
        };
        self.descending = settings.descending;
        self.grid_zoom = f32::from(settings.zoom) / 100.;
        self.reset_space_map();
        self.rebuild();
        self.scroll = settings.scroll as i32;
        self.visible.clear();
        self.hits.clear();
        self.request_stats();
        true
    }
    pub(crate) fn save_folder_view(&mut self, forget: bool) {
        self.folder_restore = None;
        if self.closing || !self.loaded || self.selection_refreshing() || !self.error.is_empty() {
            return;
        }
        if forget && !self.folder_views.loaded {
            self.folder_views.notice =
                "Espera la carga de vistas antes de olvidar esta carpeta".into();
            return;
        }
        let op = if forget {
            let Some(entry) = self.folder_views.document.get(&self.path) else {
                self.folder_views.notice = "Esta carpeta no tiene una vista guardada".into();
                return;
            };
            Op::Forget(entry.clone())
        } else {
            let Some(settings) = self.folder_settings() else {
                return;
            };
            Op::Save(Entry {
                path: self.path.clone(),
                settings,
            })
        };
        if !self.folder_views.request(op) {
            self.folder_views.notice = "Espera el ACK o reintenta/omite la vista pendiente".into();
        }
    }
}
