//! Single-object properties; UI holds at most one pinned snapshot. All FS I/O
//! stays in the existing mutation worker, including current-path resolution.
use crate::{
    model::{Action, Hit, Model},
    painter::{Painter, Rect},
};
use files_core::{
    file_properties::{Kind, Properties, Snapshot, Timestamp},
    format_size,
};
use std::path::PathBuf;

pub struct Dialog {
    pub id: u64,
    pub path: PathBuf,
    pub snapshot: Option<Box<Snapshot>>,
    pub current_path: Option<PathBuf>,
    pub observed: Option<Properties>,
    pub requested: u32,
    pub focus: usize,
    pub scroll: i32,
    pub body_height: i32,
    pub body: Rect,
    pub pending: bool,
    pub applying: bool,
    pub close_after: bool,
    pub stale: bool,
    pub message: String,
    pub warnings: Vec<String>,
    layout: Option<Layout>,
    reveal_pending: bool,
}
pub const BITS: [u32; 9] = [
    0o400, 0o200, 0o100, 0o040, 0o020, 0o010, 0o004, 0o002, 0o001,
];
const PATH_LINE: i32 = 18;
struct Layout {
    width: i32,
    path: PathBuf,
    current: Option<PathBuf>,
    target: Option<std::ffi::OsString>,
    requested: Vec<String>,
    verified: Vec<String>,
    link: Vec<String>,
    info_top: i32,
    verified_top: i32,
    link_top: i32,
    permission_label: i32,
    permissions: i32,
    note: i32,
    height: i32,
}
fn wrap(text: &str, width: i32, measure: &mut impl FnMut(&str) -> f32) -> Vec<String> {
    let bounds: Vec<_> = text
        .char_indices()
        .map(|(i, _)| i)
        .chain(std::iter::once(text.len()))
        .collect();
    let mut lines = Vec::new();
    let mut start = 0;
    while start + 1 < bounds.len() {
        let (mut low, mut high) = (start + 1, bounds.len() - 1);
        let mut end = low;
        while low <= high {
            let mid = low + (high - low) / 2;
            if measure(&text[bounds[start]..bounds[mid]]) <= width.max(1) as f32 {
                end = mid;
                low = mid + 1;
            } else {
                high = mid - 1;
            }
        }
        lines.push(text[bounds[start]..bounds[end]].to_owned());
        start = end;
    }
    lines
}
fn path_lines(
    p: &mut dyn Painter,
    body: Rect,
    origin: i32,
    lines: &[String],
    color: files_core::theme::Color,
) {
    let first = ((body.y - origin).max(0) / PATH_LINE) as usize;
    for (index, line) in lines
        .iter()
        .enumerate()
        .skip(first)
        .take((body.h.max(0) / PATH_LINE + 2) as usize)
    {
        p.literal_text(
            Rect::new(body.x, origin + index as i32 * PATH_LINE, body.w, PATH_LINE),
            line,
            10,
            color,
        );
    }
}
const CONTENT_HEIGHT: i32 = 406;
impl Dialog {
    pub fn new(id: u64, path: PathBuf) -> Self {
        Self {
            id,
            path,
            snapshot: None,
            current_path: None,
            observed: None,
            requested: 0,
            focus: 0,
            scroll: 0,
            body_height: 0,
            body: Rect::new(0, 0, 0, 0),
            pending: true,
            applying: false,
            close_after: false,
            stale: false,
            message: "Leyendo propiedades…".into(),
            warnings: Vec::new(),
            layout: None,
            reveal_pending: false,
        }
    }
    fn ensure_layout(&mut self, p: &mut dyn Painter, width: i32) {
        let target = self
            .snapshot
            .as_ref()
            .and_then(|s| s.symlink_target.as_ref());
        if self.layout.as_ref().is_some_and(|l| {
            l.width == width
                && l.path == self.path
                && l.current == self.current_path
                && l.target.as_ref() == target
        }) {
            return;
        }
        let mut measure = |text: &str| p.text_width(text, 10, true);
        let requested = wrap(
            &format!(
                "Ruta solicitada: {}",
                files_core::name_label::label(self.path.as_os_str())
            ),
            width,
            &mut measure,
        );
        let current = self
            .current_path
            .as_ref()
            .map(|path| {
                format!(
                    "Ruta verificada: {}",
                    files_core::name_label::label(path.as_os_str())
                )
            })
            .unwrap_or_else(|| {
                "Ruta actual no disponible; no se inventa la ruta de rescate".into()
            });
        let verified = wrap(&current, width, &mut measure);
        let link = target
            .map(|t| {
                wrap(
                    &format!(
                        "Destino raw (no abierto): {}",
                        files_core::name_label::label(t)
                    ),
                    width,
                    &mut measure,
                )
            })
            .unwrap_or_default();
        let info_top = requested.len() as i32 * PATH_LINE + 8;
        let verified_top = info_top + 174;
        let link_top = verified_top + verified.len() as i32 * PATH_LINE + 8;
        let permission_label = link_top
            + if link.is_empty() {
                0
            } else {
                link.len() as i32 * PATH_LINE + 8
            };
        let permissions = permission_label + 28;
        let note = permissions + 104;
        let height = note + 36;
        self.layout = Some(Layout {
            width,
            path: self.path.clone(),
            current: self.current_path.clone(),
            target: target.cloned(),
            requested,
            verified,
            link,
            info_top,
            verified_top,
            link_top,
            permission_label,
            permissions,
            note,
            height,
        });
    }
    fn content_height(&self) -> i32 {
        self.layout.as_ref().map_or(CONTENT_HEIGHT, |l| {
            if self.observed.is_some() {
                l.height
            } else {
                l.info_top + 40
            }
        }) + self.warnings.len() as i32 * 28
    }
    pub fn editable(&self) -> bool {
        !self.pending
            && !self.stale
            && self.snapshot.is_some()
            && self.observed.is_some_and(|p| {
                matches!(p.identity.kind, Kind::File | Kind::Directory)
                    && p.mode & 0o7000 == 0
                    && own_uid() == Some(p.uid)
            })
    }
    pub fn changed(&self) -> bool {
        self.editable()
            && self
                .observed
                .is_some_and(|p| p.mode & 0o777 != self.requested)
    }
    pub fn toggle(&mut self, bit: usize) {
        if self.editable()
            && let Some(mask) = BITS.get(bit)
        {
            self.requested ^= mask;
            self.focus = bit;
        }
    }
    pub fn scroll_by(&mut self, delta: i32, body_height: i32) {
        if delta != 0 {
            self.reveal_pending = false;
        }
        self.scroll = self
            .scroll
            .saturating_add(delta)
            .clamp(0, (self.content_height() - body_height).max(0));
    }
    pub fn focus_next(&mut self, backwards: bool, body_height: i32) {
        self.focus = (self.focus + if backwards { 8 } else { 1 }) % 9;
        self.reveal_pending = true;
        self.reveal_focus(body_height);
    }
    fn reveal_focus(&mut self, body_height: i32) {
        let top =
            self.layout.as_ref().map_or(258, |l| l.permissions) + (self.focus / 3) as i32 * 32;
        if top < self.scroll {
            self.scroll = top;
        }
        if top + 28 > self.scroll + body_height {
            self.scroll = (top + 28 - body_height).max(0);
        }
    }
}
#[cfg(target_os = "linux")]
fn own_uid() -> Option<u32> {
    Some(unsafe { libc::geteuid() })
}
#[cfg(not(target_os = "linux"))]
fn own_uid() -> Option<u32> {
    None
}
fn date(t: Timestamp) -> String {
    format!("{}.{:09} Unix", t.seconds, t.nanoseconds)
}
fn button(
    p: &mut dyn Painter,
    m: &mut Model,
    r: Rect,
    text: &str,
    action: Action,
    enabled: bool,
    on: bool,
) {
    p.rounded_rect(
        r,
        6 * m.radius,
        if on {
            m.palette.selection
        } else {
            m.palette.lighter_background
        },
    );
    p.text(
        Rect::new(r.x + 8, r.y, r.w - 16, r.h),
        text,
        12,
        if enabled {
            m.palette.accent
        } else {
            m.palette.dark_foreground
        },
        true,
        false,
    );
    if enabled {
        m.hits.push(Hit {
            generation: m.generation,
            rect: r,
            action,
        });
    }
}
pub fn paint(p: &mut dyn Painter, m: &mut Model, d: &mut Dialog, w: i32, h: i32) {
    m.hits.clear();
    let width = (w - 36).min(780);
    let height = (h - 68).min(640);
    let x = (w - width) / 2;
    let y = (h - 32 - height) / 2;
    let c = m.palette.clone();
    p.rounded_rect(Rect::new(x, y, width, height), 10 * m.radius, c.muted);
    p.rounded_rect(
        Rect::new(x + 1, y + 1, width - 2, height - 2),
        10 * m.radius,
        c.darker_background,
    );
    p.text(
        Rect::new(x + 20, y + 8, width - 40, 30),
        "  Propiedades y permisos",
        17,
        c.bright_foreground,
        true,
        false,
    );
    p.text(
        Rect::new(x + 20, y + 40, width - 40, 24),
        "Rutas completas en detalles · Inicio/Fin · Tab permisos",
        10,
        c.dark_foreground,
        true,
        false,
    );
    let body = Rect::new(x + 20, y + 74, width - 40, (height - 176).max(0));
    d.ensure_layout(p, body.w);
    d.body_height = body.h;
    d.body = body;
    if d.reveal_pending && d.editable() {
        d.reveal_focus(body.h);
        d.reveal_pending = false;
    }
    d.scroll_by(0, body.h);
    p.clip(body);
    let origin = body.y - d.scroll;
    let layout = d.layout.as_ref().expect("properties layout prepared");
    path_lines(p, body, origin, &layout.requested, c.dark_foreground);
    let top = origin + layout.info_top;
    if let Some(info) = d.observed {
        let kind = match info.identity.kind {
            Kind::File => "Archivo",
            Kind::Directory => "Carpeta (tamaño no recursivo)",
            Kind::Symlink => "Enlace simbólico (sin seguir)",
            Kind::Other => "Objeto especial",
        };
        let lines = [
            format!("{kind} · aparente {}", format_size(info.apparent_bytes)),
            format!(
                "Asignado {} · enlaces duros {}",
                format_size(info.allocated_bytes),
                info.hardlinks
            ),
            format!(
                "UID {} · GID {} · modo {:04o}",
                info.uid, info.gid, info.mode
            ),
            format!(
                "Dispositivo {} · inode {}",
                info.identity.device, info.identity.inode
            ),
            format!("Modificado: {}", date(info.modified)),
            format!("Accedido: {}", date(info.accessed)),
            format!("Cambio de inode (no creación): {}", date(info.changed)),
        ];
        for (i, line) in lines.iter().enumerate() {
            p.text(
                Rect::new(body.x, top + i as i32 * 24, body.w, 24),
                line,
                12,
                c.foreground,
                true,
                false,
            );
        }
        path_lines(
            p,
            body,
            origin + layout.verified_top,
            &layout.verified,
            c.dark_foreground,
        );
        path_lines(p, body, origin + layout.link_top, &layout.link, c.accent);
        p.text(
            Rect::new(body.x, origin + layout.permission_label, body.w, 24),
            &format!(
                "Permisos {:04o} → {:04o} · todos los enlaces al inode",
                info.mode, d.requested
            ),
            12,
            c.bright_foreground,
            true,
            false,
        );
        for row in 0..3 {
            let by = origin + layout.permissions + row * 32;
            p.text(
                Rect::new(body.x, by, if body.w < 500 { 96 } else { 128 }, 28),
                ["Propietario", "Grupo", "Otros"][row as usize],
                12,
                c.foreground,
                true,
                false,
            );
            for col in 0..3 {
                let index = (row * 3 + col) as usize;
                let labels = if body.w < 500 { 96 } else { 128 };
                let cell = (body.w - labels - 8 - 16) / 3;
                let r = Rect::new(body.x + labels + 8 + col * (cell + 8), by, cell, 28);
                let on = d.requested & BITS[index] != 0;
                let label = format!(
                    "{} {}",
                    ["r", "w", "x"][col as usize],
                    if on { "" } else { "—" }
                );
                let whole = r.w > 0
                    && r.x >= body.x
                    && r.x + r.w <= body.x + body.w
                    && r.y >= body.y
                    && r.y + r.h <= body.y + body.h;
                button(
                    p,
                    m,
                    r,
                    &label,
                    Action::PropertyBit(index),
                    d.editable() && whole,
                    on,
                );
                if index == d.focus && whole && d.editable() {
                    for edge in [
                        Rect::new(r.x, r.y, r.w, 1),
                        Rect::new(r.x, r.y + r.h - 1, r.w, 1),
                        Rect::new(r.x, r.y, 1, r.h),
                        Rect::new(r.x + r.w - 1, r.y, 1, r.h),
                    ] {
                        p.rect(edge, c.accent);
                    }
                }
            }
        }
        for (i, warning) in d.warnings.iter().enumerate() {
            p.text(
                Rect::new(body.x, origin + layout.height + i as i32 * 28, body.w, 28),
                warning,
                11,
                c.red,
                true,
                false,
            );
        }
        let note = if d.editable() {
            "Sólo este objeto; sin recursión, ACL ni cambio de propietario."
        } else {
            "Sólo lectura o snapshot obsoleto: Recargar para volver a comprobar."
        };
        p.text(
            Rect::new(body.x, origin + layout.note, body.w, 28),
            note,
            11,
            c.dark_foreground,
            true,
            false,
        );
    } else {
        p.text(
            Rect::new(body.x, top, body.w, 40),
            if d.pending {
                "Leyendo fuera de la UI…"
            } else {
                "No hay propiedades válidas; puedes recargar."
            },
            13,
            c.foreground,
            true,
            false,
        );
    }
    p.unclip();
    let summary = &d.message;
    // Messages from the worker are bounded and flattened for a one-line status.
    p.text(
        Rect::new(x + 20, y + height - 94, width - 40, 24),
        summary,
        11,
        if d.stale { c.red } else { c.accent },
        true,
        false,
    );
    p.text(
        Rect::new(x + 20, y + height - 70, width - 40, 20),
        "Sin Deshacer · rueda/↑/↓ · Inicio/Fin · Tab permisos",
        10,
        c.dark_foreground,
        true,
        false,
    );
    button(
        p,
        m,
        Rect::new(x + 20, y + height - 42, (width - 56) / 3, 30),
        "Recargar",
        Action::PropertyReload,
        !d.pending,
        false,
    );
    button(
        p,
        m,
        Rect::new(
            x + 28 + (width - 56) / 3,
            y + height - 42,
            (width - 56) / 3,
            30,
        ),
        if d.pending {
            if width < 520 {
                "Cancelar"
            } else {
                "Cancelar tarea"
            }
        } else {
            "Cerrar"
        },
        Action::PropertyClose,
        true,
        false,
    );
    button(
        p,
        m,
        Rect::new(
            x + 36 + 2 * ((width - 56) / 3),
            y + height - 42,
            (width - 56) / 3,
            30,
        ),
        if width < 520 {
            "Aplicar"
        } else {
            "Aplicar permisos"
        },
        Action::PropertyApply,
        d.changed(),
        false,
    );
}
pub fn fixture(m: &Model) -> serde_json::Value {
    m.properties_dialog.as_ref().map_or(serde_json::Value::Null,|d|serde_json::json!({"id":d.id,"pending":d.pending,"applying":d.applying,"changed":d.changed(),"editable":d.editable(),"stale":d.stale,"requested":d.requested,"observed":d.observed.map(|p|p.mode),"path_raw":d.path.as_os_str().as_encoded_bytes(),"current_raw":d.current_path.as_ref().map(|p|p.as_os_str().as_encoded_bytes()),"scroll":d.scroll,"focus":d.focus,"body":[d.body.x,d.body.y,d.body.w,d.body.h],"content_height":d.content_height(),"paths":d.layout.as_ref().map(|l|serde_json::json!({"requested":l.requested,"verified":l.verified,"link":l.link,"permissions_top":l.permissions,"verified_top":l.verified_top,"link_top":l.link_top})),"buttons":m.hits.iter().filter_map(|h|{let (command,index)=match &h.action {Action::PropertyBit(i)=>("Bit",Some(*i)),Action::PropertyApply=>("Apply",None),Action::PropertyReload=>("Reload",None),Action::PropertyClose=>("Close",None),_=>return None};Some(serde_json::json!({"command":command,"index":index,"rect":[h.rect.x,h.rect.y,h.rect.w,h.rect.h]}))}).collect::<Vec<_>>()}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Default)]
    struct Recorder {
        text: Vec<String>,
        measured: usize,
    }
    impl Painter for Recorder {
        fn rect(&mut self, _: Rect, _: files_core::theme::Color) {}
        fn rounded_rect(&mut self, _: Rect, _: i32, _: files_core::theme::Color) {}
        fn text(
            &mut self,
            _: Rect,
            text: &str,
            _: i32,
            _: files_core::theme::Color,
            _: bool,
            _: bool,
        ) {
            self.text.push(text.into());
        }
        fn text_width(&mut self, text: &str, size: i32, _: bool) -> f32 {
            self.measured += 1;
            text.chars().count() as f32 * size as f32 * 0.6
        }
        fn image(&mut self, _: Rect, _: &crate::painter::Image<'_>) {}
        fn clip(&mut self, _: Rect) {}
        fn unclip(&mut self) {}
    }
    #[test]
    fn wrapping_keeps_all_utf8_scalars_and_each_measured_line_fits() {
        let text = "Ruta: 日本語 👩‍💻 / literal\\xFF / fin";
        let mut measure = |s: &str| {
            s.chars()
                .map(|c| if c.is_ascii() { 6.0 } else { 12.0 })
                .sum()
        };
        let lines = wrap(text, 60, &mut measure);
        assert!(lines.len() > 1);
        assert_eq!(lines.concat(), text);
        assert!(lines.iter().all(|line| measure(line) <= 60.0));
    }
    #[test]
    #[cfg(target_os = "linux")]
    fn raw_paths_are_complete_cached_and_minimum_permission_targets_do_not_overlap() {
        use std::{
            os::unix::{ffi::OsStringExt, fs::PermissionsExt},
            sync::atomic::{AtomicU64, Ordering},
        };
        static ID: AtomicU64 = AtomicU64::new(0);
        let root = files_core::theme::home()
            .join(".cache/files-test")
            .join(format!(
                "properties-layout-{}-{}",
                std::process::id(),
                ID.fetch_add(1, Ordering::Relaxed)
            ));
        std::fs::create_dir_all(&root).unwrap();
        let mut raw = b"long-".to_vec();
        raw.extend(vec![b'a'; 180]);
        raw.extend(b"\xff\n.txt");
        let path = root.join(std::ffi::OsString::from_vec(raw));
        std::fs::write(&path, b"unchanged").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let snapshot = files_core::file_properties::capture(&path).unwrap();
        let mut d = Dialog::new(1, path.clone());
        d.pending = false;
        d.observed = Some(snapshot.properties);
        d.requested = 0o640;
        d.current_path = Some(snapshot.current_path().unwrap());
        d.snapshot = Some(Box::new(snapshot));
        let mut m = Model::new(root.clone());
        let mut p = Recorder::default();
        for (w, h) in [(440, 440), (800, 440), (1000, 650)] {
            d.focus = 0;
            d.scroll = 0;
            paint(&mut p, &mut m, &mut d, w, h);
            let layout = d.layout.as_ref().unwrap();
            assert_eq!(
                layout.requested.concat(),
                format!(
                    "Ruta solicitada: {}",
                    files_core::name_label::label(path.as_os_str())
                )
            );
            let measured = p.measured;
            paint(&mut p, &mut m, &mut d, w, h);
            assert_eq!(p.measured, measured);
            d.focus_next(true, d.body_height);
            paint(&mut p, &mut m, &mut d, w, h);
            let bits: Vec<_> = m
                .hits
                .iter()
                .filter(|h| matches!(h.action, Action::PropertyBit(_)))
                .collect();
            assert_eq!(bits.len(), 9);
            for hit in bits {
                assert!(
                    hit.rect.x >= d.body.x
                        && hit.rect.x + hit.rect.w <= d.body.x + d.body.w
                        && hit.rect.y >= d.body.y
                        && hit.rect.y + hit.rect.h <= d.body.y + d.body.h
                );
            }
            let buttons: Vec<_> = m
                .hits
                .iter()
                .filter(|h| {
                    matches!(
                        h.action,
                        Action::PropertyApply | Action::PropertyClose | Action::PropertyReload
                    )
                })
                .collect();
            assert_eq!(buttons.len(), 3);
            for (i, a) in buttons.iter().enumerate() {
                assert!(a.rect.y + a.rect.h <= h - 32);
                for b in &buttons[i + 1..] {
                    assert!(a.rect.x + a.rect.w <= b.rect.x || b.rect.x + b.rect.w <= a.rect.x);
                }
            }
        }
        // Moving the leaf correctly invalidates a properties snapshot. Moving
        // the pinned parent retains its child mapping and changes its verified
        // absolute path, exercising cache invalidation without inventing a path.
        let moved = root.with_extension("moved");
        std::fs::rename(&root, &moved).unwrap();
        let renamed = moved.join(path.file_name().unwrap());
        d.current_path = Some(d.snapshot.as_ref().unwrap().current_path().unwrap());
        let measured = p.measured;
        paint(&mut p, &mut m, &mut d, 1000, 650);
        assert!(p.measured > measured);
        assert_eq!(
            d.layout.as_ref().unwrap().verified.concat(),
            format!(
                "Ruta verificada: {}",
                files_core::name_label::label(renamed.as_os_str())
            )
        );
        assert_eq!(d.path, path);
        assert_eq!(std::fs::read(&renamed).unwrap(), b"unchanged");
        assert_eq!(
            std::fs::metadata(&renamed).unwrap().permissions().mode() & 0o777,
            0o600
        );
        drop(d);
        drop(m);
        std::fs::remove_dir_all(moved).unwrap();
    }
    #[test]
    fn loading_cannot_apply_and_tab_reveals_last_permission_at_small_height() {
        let mut d = Dialog::new(1, "/fixture".into());
        assert!(!d.editable() && !d.changed());
        d.toggle(0);
        assert_eq!(d.requested, 0);
        d.focus_next(true, 228);
        assert_eq!(d.focus, 8);
        assert!(d.scroll > 0);
        d.scroll_by(i32::MAX, 228);
        assert_eq!(d.scroll, CONTENT_HEIGHT - 228);
        d.scroll_by(i32::MIN, 228);
        assert_eq!(d.scroll, 0);
    }
}
