//! Static bounded shortcut help. No unimplemented action is advertised.
use crate::{
    model::{Action, Hit, Model},
    painter::{Painter, Rect},
};
use fltk::enums::Key;
const ROW: i32 = 30;
pub const ITEMS: &[(&str, &str)] = &[
    ("", "NAVEGACIÓN"),
    ("Alt+← / Alt+→", "Atrás / Adelante en el historial"),
    ("Enter / doble clic", "Abrir elemento o entrar en carpeta"),
    ("Backspace", "Subir a la carpeta superior"),
    ("Ctrl+T / Ctrl+W", "Nueva pestaña / cerrar activa"),
    ("Ctrl+Tab / Ctrl+Shift+Tab", "Pestaña siguiente / anterior"),
    ("Ctrl+Shift+T", "Reabrir pestaña cerrada"),
    ("Ctrl+L", "Editar ruta: selecciona todo el campo"),
    (">comando / :lugar", "En ruta + Enter: buscar en catálogos"),
    (
        "←/→, Home/End, Shift",
        "Cursor y selección: ruta, filtro o buscador",
    ),
    (
        "Ctrl+←/→ / doble clic",
        "Recorrer / seleccionar componente en campo",
    ),
    (
        "Ctrl+Z / Ctrl+Shift+Z",
        "En campos: deshacer / rehacer texto",
    ),
    (
        "Escribir / Ctrl+F / ",
        "Editar filtro; Enter vuelve a la lista",
    ),
    (
        "F6 en Buscar",
        "Alternar nombre/texto literal (UTF8 exacto)",
    ),
    (
        "Ctrl+D / Ctrl+Alt+D",
        "Guardar carpeta / abrir favoritos y lugares",
    ),
    (
        "F2 / Supr en lugares",
        "Etiqueta / quitar favorito; carpeta intacta",
    ),
    ("Ctrl+Shift+F", "Buscar nombres y contenido en subcarpetas"),
    (
        "Ctrl+Shift+P",
        "Buscar comandos y herramientas; Enter ejecuta",
    ),
    (
        "F4 / Tab / Shift+Tab",
        "En buscador: filtros de tamaño/fecha UTC y campos",
    ),
    (
        "Enter / Alt+Enter",
        "En buscador: abrir / localizar resultado",
    ),
    (
        "report 2026 !draft",
        "Combinar palabras y excluir coincidencias",
    ),
    (
        "*.pdf / ext:pdf,md",
        "Comodines o extensiones (sin distinguir caso)",
    ),
    (
        "type:dir / file / link",
        "Filtrar carpetas, archivos o enlaces",
    ),
    ("\"informe anual\"", "Buscar una frase literal en el nombre"),
    ("Esc", "Cancelar edición o quitar filtro"),
    ("Ctrl+R", "Actualizar conservando la selección"),
    ("Ctrl+H", "Mostrar / ocultar ocultos (global)"),
    ("", "VISTAS"),
    ("Ctrl+1 / Ctrl+2", "Grupos / Lista"),
    (
        "Ctrl+3 / Ctrl+4",
        "Columnas tipo Finder / Espacio de tamaños",
    ),
    ("Ctrl+5", "Grilla de iconos"),
    (
        "Ctrl+Shift+P → ordenar",
        "Nombre/tamaño/fecha; repetir invierte el panel activo",
    ),
    (
        "Cabeceras del listado",
        "Orden del panel pulsado; flecha muestra dirección",
    ),
    (
        "Ctrl+Shift+P → terminal",
        "Abrir terminal del sistema en la carpeta actual",
    ),
    ("F9 / Tab", "Doble panel / cambiar foco fuera de inputs"),
    ("Ctrl+rueda / Ctrl+±", "Zoom en Grilla"),
    ("Flechas", "Elemento; ←/→ columna en Columnas"),
    ("Shift+flechas", "Extender o reducir un rango visible"),
    ("Ctrl+flechas", "Mover foco conservando las marcas"),
    ("Ctrl+Shift+flechas", "Añadir rango a las marcas existentes"),
    ("Home / End", "Primero / último del listado visible"),
    ("Shift+Home / Shift+End", "Extender rango hasta un extremo"),
    ("PageUp / PageDown", "Recorrer páginas de elementos"),
    ("", "SELECCIÓN Y OPERACIONES"),
    ("Insert", "Marcar / desmarcar y avanzar (estilo MC)"),
    ("Ctrl+clic", "Añadir / quitar de la selección"),
    ("Shift+clic", "Seleccionar un rango"),
    ("Ctrl+A / Ctrl+I", "Seleccionar todo / invertir selección"),
    ("Ctrl+Shift+A", "Marcar filtrados y mostrar todo"),
    ("Ctrl+Shift+D", "Limpiar marcas y foco operativo"),
    ("Botón derecho", "Menú de elemento, lote o fondo"),
    ("Shift+F10 / Menú", "Menú del elemento enfocado"),
    ("F2", "Renombrar con vista previa"),
    ("Ctrl+C / Ctrl+X", "Copiar / cortar en files (esta ventana)"),
    ("Ctrl+V", "Pegar en files; texto en campos editables"),
    ("Ctrl+Shift+V", "Pegar MIME del sistema (con foco real)"),
    ("Ctrl+Alt+B", "Mostrar / ocultar barra de operaciones MC"),
    ("F5", "Copiar en listados; Espacio: recalcular"),
    (
        "↑↓ PgUp/PgDn · Informe",
        "Leer líneas/páginas; Home/End inicio/fin",
    ),
    (
        "Ctrl+R · Informe",
        "Reintentar sólo pendientes de Move multipadre revisado",
    ),
    ("F6", "Mover la selección: revisar y confirmar"),
    ("Supr / F8", "Enviar a Papelera"),
    ("F3 / Espacio", "Vista previa del elemento"),
    ("Ctrl+Alt+Espacio", "Gestor de tamaños por carpeta"),
    (
        "Paleta → SHA-256",
        "Calcular archivos elegidos, comparar y copiar informe",
    ),
    (
        "Comandos → sftp",
        "Montaje SFTP; F2 perfiles, F4 informe, CtrlEnter conectar",
    ),
    (
        "Espacio / Insert · Buscar",
        "Marcar; Insert avanza, Shift extiende rango",
    ),
    (
        "Ctrl+A / Ctrl+D · Buscar",
        "Marcar todos / vaciar marcas (en resultados)",
    ),
    (
        "F5 / Shift+F6 · Buscar",
        "Copiar / mover resultados: capturar y revisar",
    ),
    (
        "Ctrl+Enter · Buscar",
        "Ubicaciones marcadas en pestañas de fondo",
    ),
    (
        "F2 / Ctrl+S · Buscar",
        "Búsquedas guardadas / guardar criterios actuales",
    ),
    (
        "Alt+↑/→/← · Buscar",
        "Subir / carpeta del resultado / raíz anterior; Enter busca",
    ),
    ("F7 · Buscar", "Ordenar resultados: ruta, nombre o tipo"),
    (
        "F9 · Buscar",
        "Nuevo recorrido por páginas; Enter repite ese modo",
    ),
    (
        "Ctrl+PgUp / PgDn · Buscar",
        "Página anterior conservada / siguiente; marcas por página",
    ),
    ("F8 · Buscar", "Fechas locales o últimas1/24/168horas"),
    (
        "O / Shift+O · Tamaños",
        "Olvidar fila / revisar y olvidar medidas; no borra archivos",
    ),
    ("Alt+Enter · Tamaños", "Abrir carpeta seleccionada"),
    (
        "F5 · Tamaños",
        "Recalcular sólo la carpeta medida seleccionada",
    ),
    (
        "I · Tamaños",
        "Informe; F5 actualiza, Ctrl+C solicita copia",
    ),
    ("Ctrl+Espacio", "Calcular / cancelar tamaño de carpeta"),
    (
        "Ctrl+Shift+Espacio",
        "Calcular / parar carpetas del listado",
    ),
    (
        "∑ / ≥ en Tamaño",
        "Total medido / avance parcial; Ctrl+R invalida",
    ),
    ("Ctrl+N", "Crear archivo"),
    ("F7 / Ctrl+Shift+N", "Crear carpeta"),
    ("Ctrl+Z", "Deshacer cuando esté disponible"),
    ("Ctrl+Shift+Z", "Rehacer renombrado disponible"),
    ("Alt+Enter", "Propiedades y permisos"),
    ("", "PREVIEW Y CONFIGURACIÓN"),
    ("Space / Esc", "Abrir / cerrar vista previa"),
    ("Ctrl+A / Ctrl+C", "Seleccionar / copiar texto en preview"),
    ("Alt+↑ / Alt+↓", "Elemento anterior / siguiente en preview"),
    (
        "Ctrl+rueda / +/- / 0",
        "Zoom / ajustar imagen, PDF, vídeo o GIF",
    ),
    ("R / Shift+R", "Girar sólo la vista a derecha / izquierda"),
    ("P", "Reproducir / pausar audio, vídeo o GIF"),
    ("M", "Activar / silenciar audio de vídeo"),
    ("T", "Alternar página / texto extraído del PDF"),
    ("Ctrl+PgUp / PgDn", "Página PDF anterior / siguiente"),
    ("Ctrl+,", "Preferencias"),
    ("F10", "Acciones sobre archivos"),
    ("F1 / Esc", "Abrir / cerrar esta ayuda"),
];
#[derive(Default)]
pub struct State {
    pub top: usize,
    pub page: usize,
}
impl State {
    pub fn scroll(&mut self, delta: i32) {
        self.top = (self.top as i64 + i64::from(delta))
            .clamp(0, ITEMS.len().saturating_sub(self.page.max(1)) as i64)
            as usize;
    }
}
pub fn key(m: &mut Model, key: Key) {
    if matches!(key, Key::Escape | Key::F1) {
        m.action(Action::HelpClose);
        return;
    }
    if let Some(s) = &mut m.help {
        let d = if key == Key::Down {
            1
        } else if key == Key::Up {
            -1
        } else if key == Key::PageDown {
            s.page as i32
        } else if key == Key::PageUp {
            -(s.page as i32)
        } else if key == Key::Home {
            i32::MIN
        } else if key == Key::End {
            i32::MAX
        } else {
            0
        };
        s.scroll(d);
    }
}
pub fn panel(w: i32, h: i32) -> Rect {
    let width = (w - 32).clamp(1, 860);
    let height = (h - 40).clamp(1, 650);
    Rect::new((w - width) / 2, (h - height) / 2, width, height)
}
fn button(p: &mut dyn Painter, m: &mut Model, r: Rect, label: &str, action: Action) {
    p.rounded_rect(r, 5 * m.radius, m.palette.selection);
    p.text(r, label, 12, m.palette.foreground, true, false);
    m.hits.push(Hit {
        rect: r,
        action,
        generation: m.generation,
    });
}
pub fn paint(p: &mut dyn Painter, m: &mut Model, w: i32, h: i32) {
    if m.help.is_none() {
        return;
    }
    m.hits.clear();
    let r = panel(w, h);
    let c = m.palette.clone();
    p.rounded_rect(r, 10 * m.radius, c.muted);
    p.rounded_rect(
        Rect::new(r.x + 1, r.y + 1, r.w - 2, r.h - 2),
        10 * m.radius,
        c.darker_background,
    );
    p.clip(Rect::new(
        r.x + 1,
        r.y + 1,
        (r.w - 2).max(0),
        (r.h - 2).max(0),
    ));
    p.text(
        Rect::new(r.x + 20, r.y + 10, r.w - 160, 30),
        "  Ayuda · Atajos",
        17,
        c.foreground,
        true,
        false,
    );
    button(
        p,
        m,
        Rect::new(r.x + r.w - 120, r.y + 10, 100, 30),
        "Cerrar · Esc",
        Action::HelpClose,
    );
    p.text(
        Rect::new(r.x + 20, r.y + 45, r.w - 40, 24),
        "Flechas o rueda para desplazar · PageUp / PageDown",
        11,
        c.foreground,
        true,
        false,
    );
    let body = Rect::new(r.x + 16, r.y + 78, r.w - 32, (r.h - 136).max(0));
    let page = (body.h / ROW).max(1) as usize;
    let state = m.help.as_mut().unwrap();
    state.page = page;
    state.scroll(0);
    let top = state.top;
    p.clip(body);
    for (rank, (keys, description)) in ITEMS.iter().enumerate().skip(top).take(page) {
        let y = body.y + (rank - top) as i32 * ROW;
        if keys.is_empty() {
            p.text(
                Rect::new(body.x + 4, y, body.w - 8, ROW),
                description,
                11,
                c.accent,
                true,
                false,
            );
        } else {
            p.text(
                Rect::new(body.x + 4, y, 182, ROW),
                keys,
                11,
                c.bright_foreground,
                true,
                false,
            );
            p.text(
                Rect::new(body.x + 196, y, body.w - 200, ROW),
                description,
                11,
                c.foreground,
                true,
                false,
            );
        }
    }
    p.unclip();
    p.text(
        Rect::new(r.x + 20, r.y + r.h - 45, 180, 28),
        &format!(
            "{}–{} / {}",
            top + 1,
            (top + page).min(ITEMS.len()),
            ITEMS.len()
        ),
        11,
        c.foreground,
        true,
        false,
    );
    button(
        p,
        m,
        Rect::new(r.x + r.w - 248, r.y + r.h - 45, 108, 28),
        "  Anterior",
        Action::HelpScroll(-(page as i32)),
    );
    button(
        p,
        m,
        Rect::new(r.x + r.w - 132, r.y + r.h - 45, 112, 28),
        "  Siguiente",
        Action::HelpScroll(page as i32),
    );
    p.unclip();
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scrolling_is_bounded_and_resize_reclamps() {
        let mut s = State { top: 0, page: 10 };
        s.scroll(i32::MAX);
        assert_eq!(s.top, ITEMS.len() - 10);
        s.page = 20;
        s.scroll(0);
        assert_eq!(s.top, ITEMS.len() - 20);
        s.scroll(i32::MIN);
        assert_eq!(s.top, 0);
    }
    #[test]
    fn help_fits_supported_sizes_and_only_advertises_implemented_views() {
        for (w, h) in [(700, 440), (1000, 560), (1600, 1000)] {
            let r = panel(w, h);
            assert!(r.x >= 0 && r.y >= 0 && r.x + r.w <= w && r.y + r.h <= h);
            assert!((r.h - 136) / ROW >= 8);
        }
        assert!(ITEMS.iter().any(|(k, _)| *k == "Ctrl+5"));
        assert!(!ITEMS.iter().any(|(_, d)| d.contains("F6")));
        assert!(
            ITEMS
                .iter()
                .filter(|(_, d)| d.contains("SFTP"))
                .all(|(_, d)| d.starts_with("Montaje SFTP"))
        );
        assert!(
            !ITEMS
                .iter()
                .any(|(_, d)| d.contains("Conectar SFTP") || d.contains("Montar SFTP"))
        );
    }
}
