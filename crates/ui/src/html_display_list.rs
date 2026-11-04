//! Display list for the experimental HTML preview (P05, PROBE_ONLY).
//!
//! The C++ adapter (`scripts/fixtures/html-engine/probe.cpp`) lays out a document
//! with approximate scalar monospace metrics and emits one JSON object with an
//! envelope plus `commands` (`text`/`rect`/`clip`/`unclip`). This module decodes
//! that wire with explicit bounds, paints it through `Painter`, and exposes the
//! visible text as a `TextDocument` for selection and copy.
//!
//! Bounds, checked in this order and before any proportional allocation:
//! - wire bytes ≤ [`MAX_WIRE_BYTES`];
//! - command objects ≤ [`MAX_COMMANDS`], pre-counted on the raw bytes (the probe
//!   escapes every `"` inside strings, so the literal `{"op":` only opens a
//!   command) and re-checked after parsing;
//! - coordinates finite with |v| ≤ 1e6 and w/h ≥ 0; fonts within 8..=64;
//! - clip depth ≤ [`MAX_CLIP_DEPTH`] and balanced at the end;
//! - visible text ≤ `text_document::MAX_TEXT_BYTES` / `MAX_LINES`.
//!
//! Selection text is the visible text in draw order (not DOM order, not HTML
//! source), joined by a space when a text box continues on the same line and by a
//! newline when geometry starts a new line. ASCII controls become spaces of the
//! same byte length so `TextDocument` offsets map one-to-one onto draw ranges.
//!
//! Alpha is approximated by blending over the palette background because the
//! `Painter` is opaque. Clips are intersected with the body. Layout metrics come
//! from the probe and are declared approximate: no shaping or ZWJ claims.
use crate::painter::{Painter, Rect};
use crate::text_document::{MAX_LINES, MAX_TEXT_BYTES, TextDocument};
use files_core::theme::{Color, Palette};
use serde_json::Value;
use std::ops::Range;

pub const MAX_WIRE_BYTES: usize = 2 * 1024 * 1024 + 16 * 1024;
pub const MAX_COMMANDS: usize = 8192;
pub const MAX_CLIP_DEPTH: usize = 32;
pub const MAX_SUMMARY_BYTES: usize = 512;
pub const MIN_FONT: i32 = 8;
pub const MAX_FONT: i32 = 64;
const COORD_LIMIT: f32 = 1_000_000.0;
const SCROLL_STEP: i64 = 60;
const BOLD_WEIGHT: i64 = 600;
const EXPECTED_STATUS: &str = "PROBE_ONLY";
const EXPECTED_METRICS: &str = "approximate_scalar_monospace";
const COMMAND_OPEN: &[u8] = b"{\"op\":";

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Box2 {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}
impl Box2 {
    fn screen(self, body: Rect, top: i32) -> Rect {
        let x0 = body.x + self.x.round() as i32;
        let y0 = body.y + self.y.round() as i32 - top;
        Rect::new(
            x0,
            y0,
            (self.x + self.w).round() as i32 - self.x.round() as i32,
            (self.y + self.h).round() as i32 - self.y.round() as i32,
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Text {
        rect: Box2,
        rgba: [u8; 4],
        /// Byte range inside `Document::document`.
        range: Range<usize>,
        size: i32,
        bold: bool,
        italic: bool,
    },
    Rect {
        rect: Box2,
        rgba: [u8; 4],
    },
    Clip {
        rect: Box2,
    },
    Unclip,
}

/// Counters reported by the probe envelope plus local decode counters.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stats {
    pub command_count: u64,
    pub nodes: u64,
    pub blocked_css: u64,
    pub blocked_images: u64,
    pub ignored_base: u64,
    pub unsupported_gradients: u64,
    pub approximated_borders: u64,
    pub clamped_fonts: u64,
    pub blocked_navigation: u64,
    /// Text commands whose colour was the engine default black and got recoloured
    /// to the palette foreground so dark themes keep the text visible.
    pub recolored_default_text: u64,
    /// Rect/text commands skipped because alpha was zero or the box was empty.
    pub skipped_invisible: u64,
    pub text_bytes: usize,
    pub wire_bytes: usize,
}

pub struct Document {
    /// Visible text in draw order, for selection and copy.
    pub document: TextDocument,
    /// Footer messages, Spanish, derived from the stats. Never empty: the first
    /// entry always declares the approximate metrics.
    pub warnings: Vec<String>,
    pub stats: Stats,
    commands: Vec<Command>,
    viewport: (i32, i32),
    size: (f32, f32),
    body: Rect,
    top: i32,
}

impl std::fmt::Debug for Document {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Document")
            .field("commands", &self.commands.len())
            .field("text_bytes", &self.stats.text_bytes)
            .field("viewport", &self.viewport)
            .field("size", &self.size)
            .finish()
    }
}

impl Document {
    /// Decode one probe envelope. Fails closed: no partial document is returned.
    pub fn decode(wire: &[u8]) -> Result<Self, String> {
        if wire.len() > MAX_WIRE_BYTES {
            return Err(format!(
                "Vista HTML fuera del límite: {} bytes > {}",
                wire.len(),
                MAX_WIRE_BYTES
            ));
        }
        let estimated = count_occurrences(wire, COMMAND_OPEN);
        if estimated > MAX_COMMANDS {
            return Err(format!(
                "Demasiados comandos de dibujo: {estimated} > {MAX_COMMANDS}"
            ));
        }
        structural_preflight(wire)?;
        let root: Value =
            serde_json::from_slice(wire).map_err(|e| format!("JSON inválido: {e}"))?;
        let object = root.as_object().ok_or("La raíz debe ser un objeto")?;
        if object.get("schema").and_then(Value::as_u64) != Some(1) {
            return Err("Schema no soportado".into());
        }
        if object.get("status").and_then(Value::as_str) != Some(EXPECTED_STATUS) {
            return Err("Estado del motor no es PROBE_ONLY".into());
        }
        let metrics = object.get("metrics").and_then(Value::as_str).unwrap_or("");
        let viewport = object
            .get("viewport")
            .and_then(Value::as_array)
            .filter(|v| v.len() == 2)
            .ok_or("viewport inválido")?;
        let vw = int_in(&viewport[0], 128, 1600, "viewport ancho")?;
        let vh = int_in(&viewport[1], 128, 1200, "viewport alto")?;
        let size = object
            .get("document")
            .and_then(Value::as_array)
            .filter(|v| v.len() == 2)
            .ok_or("document inválido")?;
        let dw = finite(&size[0], "document ancho")?;
        let dh = finite(&size[1], "document alto")?;
        if dw < 0.0 || dh < 0.0 {
            return Err("Tamaño de documento negativo".into());
        }
        let raw = object
            .get("commands")
            .and_then(Value::as_array)
            .ok_or("commands inválido")?;
        if raw.len() > MAX_COMMANDS {
            return Err(format!(
                "Demasiados comandos de dibujo: {} > {MAX_COMMANDS}",
                raw.len()
            ));
        }
        let mut stats = Stats {
            command_count: counter(object, "command_count"),
            nodes: counter(object, "nodes"),
            blocked_css: counter(object, "blocked_css"),
            blocked_images: counter(object, "blocked_images"),
            ignored_base: counter(object, "ignored_base"),
            unsupported_gradients: counter(object, "unsupported_gradients"),
            approximated_borders: counter(object, "approximated_borders"),
            clamped_fonts: counter(object, "clamped_fonts"),
            blocked_navigation: counter(object, "blocked_navigation"),
            wire_bytes: wire.len(),
            ..Stats::default()
        };
        if stats.command_count != raw.len() as u64 {
            return Err("command_count no coincide con commands".into());
        }

        let mut commands = Vec::with_capacity(raw.len());
        let mut text = String::new();
        let mut lines = 1usize;
        let mut depth = 0usize;
        let mut last_text: Option<Box2> = None;
        for (index, item) in raw.iter().enumerate() {
            let item = item
                .as_object()
                .ok_or_else(|| format!("Comando {index} no es un objeto"))?;
            let op = item.get("op").and_then(Value::as_str).unwrap_or("");
            match op {
                "rect" => {
                    let rect = rect_of(item, index)?;
                    let rgba = rgba_of(item, index)?;
                    if rgba[3] == 0 || rect.w <= 0.0 || rect.h <= 0.0 {
                        stats.skipped_invisible += 1;
                        continue;
                    }
                    commands.push(Command::Rect { rect, rgba });
                }
                "text" => {
                    let rect = rect_of(item, index)?;
                    let rgba = rgba_of(item, index)?;
                    let source = item
                        .get("text")
                        .and_then(Value::as_str)
                        .ok_or_else(|| format!("Comando {index}: text inválido"))?;
                    let size = item
                        .get("font_size")
                        .map(|v| finite(v, "font_size"))
                        .transpose()?
                        .ok_or_else(|| format!("Comando {index}: font_size ausente"))?;
                    let size = size.round() as i32;
                    if !(MIN_FONT..=MAX_FONT).contains(&size) {
                        return Err(format!(
                            "Comando {index}: font_size {size} fuera de {MIN_FONT}..{MAX_FONT}"
                        ));
                    }
                    let weight = item.get("weight").and_then(Value::as_i64).unwrap_or(400);
                    let style = item.get("style").and_then(Value::as_i64).unwrap_or(0);
                    if rgba[3] == 0 || source.is_empty() {
                        stats.skipped_invisible += 1;
                        continue;
                    }
                    let separator = match last_text {
                        None => None,
                        Some(prev) if same_line(prev, rect) => {
                            let adjacent = rect.x <= prev.x + prev.w + 0.5;
                            let has_space =
                                text.chars().next_back().is_some_and(char::is_whitespace)
                                    || source.chars().next().is_some_and(char::is_whitespace);
                            if adjacent || has_space {
                                None
                            } else {
                                Some(' ')
                            }
                        }
                        Some(_) => Some('\n'),
                    };
                    let added = separator.map_or(0, char::len_utf8) + source.len();
                    if text.len() + added > MAX_TEXT_BYTES {
                        return Err("Texto visible fuera del límite de vista previa".into());
                    }
                    if separator == Some('\n') {
                        lines += 1;
                        if lines > MAX_LINES {
                            return Err("Demasiadas líneas para la vista previa".into());
                        }
                    }
                    if let Some(sep) = separator {
                        text.push(sep);
                    }
                    let start = text.len();
                    for ch in source.chars() {
                        text.push(if ch.is_ascii_control() { ' ' } else { ch });
                    }
                    let range = start..text.len();
                    last_text = Some(rect);
                    commands.push(Command::Text {
                        rect,
                        rgba,
                        range,
                        size,
                        bold: weight >= BOLD_WEIGHT,
                        italic: style != 0,
                    });
                }
                "clip" => {
                    depth += 1;
                    if depth > MAX_CLIP_DEPTH {
                        return Err(format!(
                            "Comando {index}: profundidad de clip > {MAX_CLIP_DEPTH}"
                        ));
                    }
                    commands.push(Command::Clip {
                        rect: rect_of(item, index)?,
                    });
                }
                "unclip" => {
                    depth = depth
                        .checked_sub(1)
                        .ok_or_else(|| format!("Comando {index}: unclip sin clip"))?;
                    commands.push(Command::Unclip);
                }
                other => return Err(format!("Comando {index}: op desconocida {other:?}")),
            }
        }
        if depth != 0 {
            return Err("Pila de clips desbalanceada".into());
        }
        stats.text_bytes = text.len();
        let document = TextDocument::new(text).map_err(str::to_owned)?;
        let mut warnings = vec![String::from(
            "Métricas aproximadas: layout monoespaciado escalar, sin shaping",
        )];
        if metrics != EXPECTED_METRICS {
            warnings.push(format!("Métricas no reconocidas: {metrics:?}"));
        }
        push_count(
            &mut warnings,
            stats.blocked_css,
            "hojas CSS externas bloqueadas",
        );
        push_count(&mut warnings, stats.blocked_images, "imágenes bloqueadas");
        push_count(
            &mut warnings,
            stats.blocked_navigation,
            "navegaciones bloqueadas",
        );
        push_count(&mut warnings, stats.ignored_base, "<base> ignorados");
        push_count(
            &mut warnings,
            stats.unsupported_gradients,
            "gradientes no soportados",
        );
        push_count(
            &mut warnings,
            stats.approximated_borders,
            "bordes aproximados",
        );
        push_count(
            &mut warnings,
            stats.clamped_fonts,
            "fuentes recortadas por el motor",
        );
        Ok(Self {
            document,
            warnings,
            stats,
            commands,
            viewport: (vw, vh),
            size: (dw, dh),
            body: Rect::default(),
            top: 0,
        })
    }

    pub fn commands(&self) -> &[Command] {
        &self.commands
    }
    #[cfg(test)]
    pub fn viewport(&self) -> (i32, i32) {
        self.viewport
    }
    /// Layout viewport (width, height) the engine used, to validate against the
    /// job request before accepting a payload.
    pub fn viewport_dimensions(&self) -> (i32, i32) {
        self.viewport
    }
    /// Footer line, Spanish, at most [`MAX_SUMMARY_BYTES`] bytes on a char
    /// boundary: approximate metrics, blocked resources and engine warnings.
    pub fn summary(&self) -> String {
        let mut out = format!(
            "HTML experimental · métricas aproximadas · {} comandos · {} B texto",
            self.commands.len(),
            self.stats.text_bytes
        );
        for warning in self.warnings.iter().skip(1) {
            out.push_str(" · ");
            out.push_str(warning);
        }
        if out.len() > MAX_SUMMARY_BYTES {
            let mut cut = MAX_SUMMARY_BYTES - '…'.len_utf8();
            while !out.is_char_boundary(cut) {
                cut -= 1;
            }
            out.truncate(cut);
            out.push('…');
        }
        out
    }
    /// Document size reported by the engine, in layout pixels.
    pub fn size(&self) -> (f32, f32) {
        self.size
    }
    pub fn top(&self) -> i32 {
        self.top
    }
    fn content_height(&self) -> i32 {
        self.size.1.ceil() as i32
    }
    fn max_top(&self) -> i32 {
        (self.content_height() - self.body.h).max(0)
    }

    /// Paint every command in draw order. Clips are intersected with `body` and
    /// any clip still open after a mid-list error is closed before returning.
    pub fn paint(&mut self, p: &mut dyn Painter, body: Rect, palette: &Palette) {
        self.body = body;
        self.top = self.top.clamp(0, self.max_top());
        let top = self.top;
        let selection = self.document.selection_range();
        p.clip(body);
        let mut clips: Vec<Rect> = vec![body];
        let mut open = 0usize;
        for command in &mut self.commands {
            match command {
                Command::Rect { rect, rgba } => {
                    let r = rect.screen(body, top);
                    if visible(r, body) {
                        p.rect(r, blend(*rgba, palette.background));
                    }
                }
                Command::Clip { rect } => {
                    let current = *clips.last().unwrap_or(&body);
                    let r = intersect(rect.screen(body, top), current);
                    clips.push(r);
                    p.clip(r);
                    open += 1;
                }
                Command::Unclip => {
                    if open > 0 {
                        open -= 1;
                        clips.pop();
                        p.unclip();
                    }
                }
                Command::Text {
                    rect,
                    rgba,
                    range,
                    size,
                    bold,
                    italic,
                } => {
                    let r = rect.screen(body, top);
                    if !visible(r, body) {
                        continue;
                    }
                    let text = &self.document.text()[range.clone()];
                    if !selection.is_empty()
                        && selection.start < range.end
                        && selection.end > range.start
                    {
                        let left = p.styled_width(
                            &text[..selection.start.saturating_sub(range.start)],
                            *size,
                            *bold,
                            *italic,
                        );
                        let right = p.styled_width(
                            &text[..(selection.end - range.start).min(range.len())],
                            *size,
                            *bold,
                            *italic,
                        );
                        p.rect(
                            Rect::new(
                                r.x + left.round() as i32,
                                r.y,
                                (right - left).ceil().max(1.0) as i32,
                                r.h,
                            ),
                            palette.selection,
                        );
                    }
                    let color = if *rgba == [0, 0, 0, 255] {
                        self.stats.recolored_default_text += 1;
                        palette.foreground
                    } else {
                        blend(*rgba, palette.background)
                    };
                    p.styled_text(
                        Rect::new(r.x, r.y, r.w + 2, r.h),
                        text,
                        *size,
                        color,
                        *bold,
                        *italic,
                    );
                }
            }
        }
        for _ in 0..open {
            p.unclip();
        }
        p.unclip();
    }

    /// Scroll by wheel steps; positive moves the content up.
    pub fn scroll(&mut self, delta: i32) {
        self.top =
            (self.top as i64 + delta as i64 * SCROLL_STEP).clamp(0, self.max_top() as i64) as i32;
    }

    /// Nearest text box to a screen point, then the closest scalar boundary.
    pub fn offset_at(&self, p: &mut dyn Painter, x: i32, y: i32) -> usize {
        let body = self.body;
        let top = self.top;
        let Some((r, range, size, bold, italic)) = self
            .commands
            .iter()
            .filter_map(|c| match c {
                Command::Text {
                    rect,
                    range,
                    size,
                    bold,
                    italic,
                    ..
                } => Some((rect.screen(body, top), range.clone(), *size, *bold, *italic)),
                _ => None,
            })
            .min_by_key(|(r, ..)| {
                let dy = if y < r.y {
                    r.y - y
                } else {
                    (y - r.y - r.h).max(0)
                };
                let dx = if x < r.x {
                    r.x - x
                } else {
                    (x - r.x - r.w).max(0)
                };
                (dy, dx)
            })
        else {
            return 0;
        };
        let text = &self.document.text()[range.clone()];
        let hit = (x - r.x) as f32;
        let boundaries: Vec<usize> = text
            .char_indices()
            .map(|(i, _)| i)
            .chain(std::iter::once(text.len()))
            .collect();
        let mut low = 0;
        let mut high = boundaries.len() - 1;
        while low < high {
            let mid = (low + high) / 2;
            let a = p.styled_width(&text[..boundaries[mid]], size, bold, italic);
            let b = p.styled_width(&text[..boundaries[mid + 1]], size, bold, italic);
            if hit > (a + b) / 2.0 {
                low = mid + 1;
            } else {
                high = mid;
            }
        }
        range.start + boundaries[low]
    }

    /// Screen point (x, vertical centre) for a byte offset, after the last paint.
    pub fn point_for_offset(&self, p: &mut dyn Painter, offset: usize) -> (i32, i32) {
        let offset = self.document.boundary(offset);
        let body = self.body;
        let top = self.top;
        match self.text_run(offset) {
            Some((rect, range, size, bold, italic)) => {
                let r = rect.screen(body, top);
                let text = &self.document.text()[range.clone()];
                let advance = p.styled_width(&text[..offset - range.start], size, bold, italic);
                (r.x + advance.round() as i32, r.y + r.h / 2)
            }
            None => (
                body.x,
                body.y + (self.content_height() - top).clamp(0, body.h.max(0)),
            ),
        }
    }

    /// Scroll so the caret's text box is inside the body.
    pub fn reveal(&mut self) {
        let caret = self.document.caret();
        let Some((rect, ..)) = self.text_run(caret) else {
            return;
        };
        let y0 = rect.y.round() as i32;
        let y1 = (rect.y + rect.h).round() as i32;
        if y0 < self.top {
            self.top = y0;
        }
        if y1 > self.top + self.body.h {
            self.top = (y1 - self.body.h).max(0);
        }
        self.top = self.top.clamp(0, self.max_top());
    }

    fn text_run(&self, offset: usize) -> Option<(Box2, Range<usize>, i32, bool, bool)> {
        self.commands.iter().find_map(|c| match c {
            Command::Text {
                rect,
                range,
                size,
                bold,
                italic,
                ..
            } if range.start <= offset && offset <= range.end => {
                Some((*rect, range.clone(), *size, *bold, *italic))
            }
            _ => None,
        })
    }
}

// Count structures outside JSON strings too: whitespace/reordered keys must
// not bypass the producer's compact {"op": preflight. The wire cap remains
// necessary; this is not a whole-process allocation/PSS guarantee.
fn structural_preflight(wire: &[u8]) -> Result<(), String> {
    let (mut string, mut escape, mut objects, mut arrays) = (false, false, 0usize, 0usize);
    for &byte in wire {
        if string {
            if escape {
                escape = false;
            } else if byte == b'\\' {
                escape = true;
            } else if byte == b'"' {
                string = false;
            }
        } else {
            match byte {
                b'"' => string = true,
                b'{' => {
                    objects += 1;
                    if objects > MAX_COMMANDS + 1 {
                        return Err("Demasiados objetos de comandos HTML".into());
                    }
                }
                b'[' => {
                    arrays += 1;
                    if arrays > MAX_COMMANDS * 3 + 8 {
                        return Err("Demasiados arrays del display list HTML".into());
                    }
                }
                _ => {}
            }
        }
    }
    Ok(())
}
fn count_occurrences(haystack: &[u8], needle: &[u8]) -> usize {
    haystack
        .windows(needle.len())
        .filter(|w| *w == needle)
        .count()
}
fn counter(object: &serde_json::Map<String, Value>, key: &str) -> u64 {
    object.get(key).and_then(Value::as_u64).unwrap_or(0)
}
fn push_count(warnings: &mut Vec<String>, count: u64, label: &str) {
    if count > 0 {
        warnings.push(format!("{count} {label}"));
    }
}
fn finite(value: &Value, what: &str) -> Result<f32, String> {
    let v = value
        .as_f64()
        .ok_or_else(|| format!("{what} no es numérico"))?;
    if !v.is_finite() || v.abs() > COORD_LIMIT as f64 {
        return Err(format!("{what} fuera de rango"));
    }
    Ok(v as f32)
}
fn int_in(value: &Value, min: i64, max: i64, what: &str) -> Result<i32, String> {
    let v = value
        .as_i64()
        .ok_or_else(|| format!("{what} no es entero"))?;
    if v < min || v > max {
        return Err(format!("{what} fuera de {min}..{max}"));
    }
    Ok(v as i32)
}
fn rect_of(item: &serde_json::Map<String, Value>, index: usize) -> Result<Box2, String> {
    let arr = item
        .get("rect")
        .and_then(Value::as_array)
        .filter(|a| a.len() == 4)
        .ok_or_else(|| format!("Comando {index}: rect inválido"))?;
    let x = finite(&arr[0], "x")?;
    let y = finite(&arr[1], "y")?;
    let w = finite(&arr[2], "ancho")?;
    let h = finite(&arr[3], "alto")?;
    if w < 0.0 || h < 0.0 {
        return Err(format!("Comando {index}: tamaño negativo"));
    }
    Ok(Box2 { x, y, w, h })
}
fn rgba_of(item: &serde_json::Map<String, Value>, index: usize) -> Result<[u8; 4], String> {
    let arr = item
        .get("rgba")
        .and_then(Value::as_array)
        .filter(|a| a.len() == 4)
        .ok_or_else(|| format!("Comando {index}: rgba inválido"))?;
    let mut out = [0u8; 4];
    for (slot, v) in out.iter_mut().zip(arr) {
        let n = v
            .as_u64()
            .filter(|n| *n <= 255)
            .ok_or_else(|| format!("Comando {index}: canal fuera de 0..255"))?;
        *slot = n as u8;
    }
    Ok(out)
}
fn same_line(prev: Box2, next: Box2) -> bool {
    let prev_mid = prev.y + prev.h / 2.0;
    next.y < prev_mid && next.y + next.h > prev_mid
}
fn blend(rgba: [u8; 4], over: Color) -> Color {
    let a = rgba[3] as u32;
    let mix = |c: u8, b: u8| ((c as u32 * a + b as u32 * (255 - a) + 127) / 255) as u8;
    Color(
        mix(rgba[0], over.0),
        mix(rgba[1], over.1),
        mix(rgba[2], over.2),
    )
}
fn intersect(a: Rect, b: Rect) -> Rect {
    let x0 = a.x.max(b.x);
    let y0 = a.y.max(b.y);
    let x1 = (a.x + a.w).min(b.x + b.w);
    let y1 = (a.y + a.h).min(b.y + b.h);
    Rect::new(x0, y0, (x1 - x0).max(0), (y1 - y0).max(0))
}
fn visible(r: Rect, body: Rect) -> bool {
    r.w > 0 && r.h > 0 && r.y + r.h > body.y && r.y < body.y + body.h
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::painter::Image;

    #[derive(Default)]
    struct Recorder {
        rectangles: Vec<(Rect, Color)>,
        text: Vec<(Rect, String, i32, Color, bool, bool)>,
        clips: Vec<Rect>,
        depth: i32,
        max_depth: i32,
    }
    impl Painter for Recorder {
        fn rect(&mut self, r: Rect, c: Color) {
            self.rectangles.push((r, c));
        }
        fn rounded_rect(&mut self, r: Rect, _: i32, c: Color) {
            self.rect(r, c);
        }
        fn text(&mut self, r: Rect, text: &str, size: i32, c: Color, _: bool, _: bool) {
            self.text.push((r, text.into(), size, c, false, false));
        }
        fn styled_text(&mut self, r: Rect, text: &str, size: i32, c: Color, b: bool, i: bool) {
            self.text.push((r, text.into(), size, c, b, i));
        }
        fn styled_width(&mut self, text: &str, size: i32, _: bool, _: bool) -> f32 {
            text.chars().count() as f32 * size as f32 * 0.5
        }
        fn image(&mut self, _: Rect, _: &Image<'_>) {}
        fn clip(&mut self, r: Rect) {
            self.clips.push(r);
            self.depth += 1;
            self.max_depth = self.max_depth.max(self.depth);
        }
        fn unclip(&mut self) {
            self.depth -= 1;
        }
    }

    fn xywh(r: Rect) -> (i32, i32, i32, i32) {
        (r.x, r.y, r.w, r.h)
    }
    fn envelope(commands: &str, count: usize) -> Vec<u8> {
        format!(
            "{{\"schema\":1,\"status\":\"PROBE_ONLY\",\"numeric_contract\":true,\
             \"metrics\":\"approximate_scalar_monospace\",\"font_family\":\"mono\",\
             \"viewport\":[640,480],\"document\":[640.000000,900.000000],\"nodes\":7,\
             \"command_count\":{count},\"blocked_css\":1,\"blocked_images\":2,\"ignored_base\":0,\
             \"unsupported_gradients\":0,\"approximated_borders\":0,\"clamped_fonts\":0,\
             \"blocked_navigation\":0,\"commands\":[{commands}]}}\n"
        )
        .into_bytes()
    }
    fn sample() -> Vec<u8> {
        envelope(
            concat!(
                "{\"op\":\"rect\",\"rect\":[0.000000,0.000000,640.000000,40.000000],\"rgba\":[255,255,255,128]},",
                "{\"op\":\"clip\",\"rect\":[8.000000,8.000000,600.000000,800.000000]},",
                "{\"op\":\"text\",\"rect\":[8.000000,10.000000,80.000000,18.000000],\"rgba\":[0,0,0,255],\"text\":\"Hola\",\"font_size\":14.000000,\"weight\":700,\"style\":0},",
                "{\"op\":\"text\",\"rect\":[96.000000,10.000000,80.000000,18.000000],\"rgba\":[200,30,30,255],\"text\":\"mundo \\\"x\\\"\",\"font_size\":14.000000,\"weight\":400,\"style\":1},",
                "{\"op\":\"text\",\"rect\":[8.000000,40.000000,120.000000,18.000000],\"rgba\":[0,0,0,255],\"text\":\"segunda\\tlínea\",\"font_size\":14.000000,\"weight\":400,\"style\":0},",
                "{\"op\":\"text\",\"rect\":[8.000000,850.000000,120.000000,18.000000],\"rgba\":[0,0,0,0],\"text\":\"invisible\",\"font_size\":14.000000,\"weight\":400,\"style\":0},",
                "{\"op\":\"unclip\"}"
            ),
            7,
        )
    }

    #[test]
    fn decodes_draw_order_text_and_separators() {
        let doc = Document::decode(&sample()).unwrap();
        assert_eq!(doc.document.text(), "Hola mundo \"x\"\nsegunda línea");
        assert_eq!(doc.document.line_count(), 2);
        assert_eq!(doc.commands().len(), 6);
        assert_eq!(doc.stats.skipped_invisible, 1);
        assert_eq!(doc.stats.blocked_css, 1);
        assert!(doc.warnings[0].contains("aproximadas"));
        assert!(
            doc.warnings
                .iter()
                .any(|w| w == "1 hojas CSS externas bloqueadas")
        );
        assert!(doc.warnings.iter().any(|w| w == "2 imágenes bloqueadas"));
        assert_eq!(doc.viewport(), (640, 480));
        assert_eq!(doc.viewport_dimensions(), (640, 480));
        let summary = doc.summary();
        assert!(summary.starts_with("HTML experimental · métricas aproximadas · 6 comandos"));
        assert!(summary.contains("1 hojas CSS externas bloqueadas"));
        assert!(summary.contains("2 imágenes bloqueadas"));
        assert!(!summary.contains("sin shaping"));
        assert!(summary.len() <= MAX_SUMMARY_BYTES);
        assert_eq!(doc.size(), (640.0, 900.0));
        let Command::Text { bold, italic, .. } = &doc.commands()[2] else {
            panic!()
        };
        assert!(*bold && !*italic);
    }

    #[test]
    fn paints_with_blend_recolor_and_bounded_clips() {
        let mut doc = Document::decode(&sample()).unwrap();
        let mut p = Recorder::default();
        let palette = Palette::default();
        doc.paint(&mut p, Rect::new(100, 50, 300, 200), &palette);
        assert_eq!(p.depth, 0);
        assert_eq!(p.max_depth, 2);
        assert_eq!(xywh(p.clips[0]), (100, 50, 300, 200));
        assert_eq!(xywh(p.clips[1]), (108, 58, 292, 192));
        let bg = palette.background;
        let (_, blended) = p.rectangles[0];
        assert!(blended.0 > bg.0 && blended.0 < 255);
        assert_eq!(p.text.len(), 3);
        assert_eq!(p.text[0].3, palette.foreground);
        assert_eq!(p.text[1].3, Color(200, 30, 30));
        assert!(p.text[1].5);
        assert_eq!(xywh(p.text[0].0), (108, 60, 82, 18));
        assert_eq!(doc.stats.recolored_default_text, 2);
    }

    #[test]
    fn selection_highlight_scroll_and_hit_testing() {
        let mut doc = Document::decode(&sample()).unwrap();
        let mut p = Recorder::default();
        let body = Rect::new(0, 0, 640, 100);
        doc.document.select(2, 11);
        doc.paint(&mut p, body, &Palette::default());
        let highlights: Vec<_> = p
            .rectangles
            .iter()
            .filter(|(_, c)| *c == Palette::default().selection)
            .collect();
        assert_eq!(highlights.len(), 2);
        assert_eq!(xywh(highlights[0].0), (8 + 14, 10, 14, 18));
        assert_eq!(doc.document.selected_text(), "la mundo ");

        assert_eq!(doc.offset_at(&mut p, 8 + 7 * 2, 15), 2);
        assert_eq!(doc.offset_at(&mut p, 5000, 15), 14);
        assert_eq!(doc.point_for_offset(&mut p, 2), (8 + 14, 19));
        let second = doc.document.line_range(1).unwrap().start;
        assert_eq!(doc.offset_at(&mut p, 0, 90), second);

        doc.scroll(100);
        assert_eq!(doc.top(), 800);
        doc.scroll(-1000);
        assert_eq!(doc.top(), 0);
        doc.scroll(1);
        doc.paint(&mut p, body, &Palette::default());
        assert_eq!(doc.top(), 60);
        doc.document.select(0, 0);
        doc.reveal();
        assert_eq!(doc.top(), 10);
        doc.scroll(10);
        doc.document.select(second, second);
        doc.reveal();
        assert_eq!(doc.top(), 40);
    }

    #[test]
    fn rejects_bounds_before_parsing() {
        let big = vec![b' '; MAX_WIRE_BYTES + 1];
        assert!(Document::decode(&big).unwrap_err().contains("límite"));
        let mut many = String::new();
        for i in 0..=MAX_COMMANDS {
            if i > 0 {
                many.push(',');
            }
            many.push_str("{\"op\":\"unclip\"}");
        }
        let err = Document::decode(&envelope(&many, MAX_COMMANDS + 1)).unwrap_err();
        assert!(err.contains("Demasiados comandos"));
        assert_eq!(
            count_occurrences(b"{\"op\":\"text\",\"text\":\"{\\\"op\\\":\"}", COMMAND_OPEN),
            1
        );
    }

    #[test]
    fn adjacent_inline_styles_and_explicit_space_boxes_do_not_duplicate_spaces() {
        let commands = r#"{"op":"text","rect":[0,0,10,20],"rgba":[1,1,1,255],"text":"A","font_size":14},
        {"op":"text","rect":[10,0,8,20],"rgba":[1,1,1,255],"text":",","font_size":14},
        {"op":"text","rect":[18,0,8,20],"rgba":[1,1,1,255],"text":" ","font_size":14},
        {"op":"text","rect":[26,0,10,20],"rgba":[1,1,1,255],"text":"B","font_size":14}"#;
        let doc = Document::decode(&envelope(commands, 4)).unwrap();
        assert_eq!(doc.document.text(), "A, B");
    }
    #[test]
    fn whitespace_or_reordered_keys_cannot_bypass_structure_preflight() {
        let commands = std::iter::repeat_n("{ \"other\":0, \"op\": \"unclip\" }", MAX_COMMANDS + 1)
            .collect::<Vec<_>>()
            .join(",");
        assert!(
            Document::decode(&envelope(&commands, MAX_COMMANDS + 1))
                .unwrap_err()
                .contains("objetos")
        );
        assert!(structural_preflight(br#"{"text":"{[\"{}"}"#).is_ok());
    }
    #[test]
    fn rejects_invalid_commands_and_envelopes() {
        let cases = [
            ("{\"op\":\"unclip\"}", 1, "sin clip"),
            ("{\"op\":\"clip\",\"rect\":[0,0,1,1]}", 1, "desbalanceada"),
            ("{\"op\":\"blob\"}", 1, "desconocida"),
            (
                "{\"op\":\"rect\",\"rect\":[0,0,-1,1],\"rgba\":[1,1,1,255]}",
                1,
                "negativo",
            ),
            (
                "{\"op\":\"rect\",\"rect\":[0,0,1,2000000],\"rgba\":[1,1,1,255]}",
                1,
                "fuera de rango",
            ),
            (
                "{\"op\":\"rect\",\"rect\":[0,0,1,1],\"rgba\":[1,1,1,300]}",
                1,
                "canal",
            ),
            (
                "{\"op\":\"text\",\"rect\":[0,0,1,1],\"rgba\":[1,1,1,255],\"text\":\"a\",\"font_size\":65,\"weight\":400,\"style\":0}",
                1,
                "font_size",
            ),
            ("{\"op\":\"unclip\"}", 2, "command_count"),
        ];
        for (commands, count, needle) in cases {
            let err = Document::decode(&envelope(commands, count)).unwrap_err();
            assert!(err.contains(needle), "{commands}: {err}");
        }
        let mut deep = String::new();
        for i in 0..=MAX_CLIP_DEPTH {
            if i > 0 {
                deep.push(',');
            }
            deep.push_str("{\"op\":\"clip\",\"rect\":[0,0,1,1]}");
        }
        assert!(
            Document::decode(&envelope(&deep, MAX_CLIP_DEPTH + 1))
                .unwrap_err()
                .contains("profundidad")
        );
        let other = String::from_utf8(sample())
            .unwrap()
            .replace("PROBE_ONLY", "FULL");
        assert!(
            Document::decode(other.as_bytes())
                .unwrap_err()
                .contains("PROBE_ONLY")
        );
        let bad_viewport = String::from_utf8(sample())
            .unwrap()
            .replace("[640,480]", "[64,480]");
        assert!(
            Document::decode(bad_viewport.as_bytes())
                .unwrap_err()
                .contains("viewport")
        );
        assert!(Document::decode(b"[]").is_err());
        assert!(Document::decode(b"{\"schema\":1").is_err());
    }

    #[test]
    fn summary_is_bounded_on_char_boundary() {
        let mut doc = Document::decode(&sample()).unwrap();
        doc.warnings
            .extend(std::iter::repeat_n(String::from("ñandú muy largo"), 60));
        let summary = doc.summary();
        assert!(summary.len() <= MAX_SUMMARY_BYTES, "{}", summary.len());
        assert!(summary.ends_with('…'));
        assert!(std::str::from_utf8(summary.as_bytes()).is_ok());
    }

    #[test]
    fn text_cap_is_enforced_before_text_document() {
        let chunk = "x".repeat(1024);
        let mut commands = String::new();
        let count = MAX_TEXT_BYTES / 1024 + 1;
        for i in 0..count {
            if i > 0 {
                commands.push(',');
            }
            commands.push_str(&format!(
                "{{\"op\":\"text\",\"rect\":[0,{},10,10],\"rgba\":[0,0,0,255],\"text\":\"{chunk}\",\"font_size\":12,\"weight\":400,\"style\":0}}",
                i * 12
            ));
        }
        let err = Document::decode(&envelope(&commands, count)).unwrap_err();
        assert!(err.contains("Texto visible"));
    }
}
