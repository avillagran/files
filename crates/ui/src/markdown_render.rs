//! Native, bounded Markdown projection. Source ranges and arena offsets never
//! become selection offsets: selection indexes the constructed visible text.
//! Reflow resumes across ~4 ms steps, capped at 32768 runs / 2001 logical lines;
//! parsing/projection belong on the preview worker. Glyph hit testing uses shaped
//! prefixes but scalar boundaries, not full grapheme/ZWJ cluster segmentation.
use crate::{
    image_preview_job::Bitmap,
    image_preview_view,
    markdown_document::{Atom, Container, Document},
    painter::{Painter, Rect},
    preview_link::Target,
    text_document::TextDocument,
};
use files_core::theme::Palette;
use pulldown_cmark::Alignment;
use std::{
    ops::Range,
    sync::Arc,
    time::{Duration, Instant},
};

const MAX_RUNS: usize = 32_768;
const MAX_SPANS: usize = 16_384;
const MAX_BYTES: usize = 384 * 1024;
const MAX_CHUNK: usize = 256;
const PAD: i32 = 16;
const MAX_IMAGES: usize = 8;
const MAX_IMAGE_BYTES: usize = 4 * 1024 * 1024;
#[derive(Clone, Copy, Default, Debug)]
struct Style {
    size: i32,
    bold: bool,
    italic: bool,
    code: bool,
    strike: bool,
    quote: bool,
    link: Option<usize>,
}
#[derive(Debug)]
struct Span {
    range: Range<usize>,
    style: Style,
}
struct Cell {
    range: Range<usize>,
    alignment: Alignment,
}
struct Line {
    range: Range<usize>,
    indent: i32,
    cells: Vec<Cell>,
    rule: bool,
    images: Vec<usize>,
}
struct Run {
    range: Range<usize>,
    rect: Rect,
    style: Style,
    column: usize,
}
#[derive(Default)]
pub struct Viewport {
    pub body: Rect,
    pub dragging: bool,
    pub selection_anchor: usize,
    pub top: i32,
    pub content_height: i32,
}
pub struct Rendered {
    pub document: TextDocument,
    pub viewport: Viewport,
    pub layout_error: Option<String>,
    pub blocked_targets: usize,
    pub pending_images: usize,
    pub loaded_images: usize,
    /// Sum of retained Bitmap structs plus RGB and format allocation capacities.
    pub image_bytes: usize,
    /// Highest referenced byte of the original Markdown, not a visible offset.
    pub source_bytes: usize,
    /// Largest observed cooperative reflow step, including width-change reset.
    pub max_layout_step_us: u128,
    spans: Vec<Span>,
    lines: Vec<Line>,
    links: Vec<Target>,
    runs: Vec<Run>,
    width: i32,
    rules: Vec<Rect>,
    headings: Vec<(String, usize)>,
    pending_reveal: Option<usize>,
    layout_line: usize,
    layout_cell: usize,
    layout_y: i32,
    layout_row_height: i32,
    active: Option<RangeLayout>,
    active_run_start: usize,
    images: Vec<EmbeddedImage>,
    placements: Vec<PlacedImage>,
    image_viewport: image_preview_view::Viewport,
    painted_image: Option<usize>,
}
struct Frame<'a> {
    kind: &'a Container,
    counter: u64,
    link: Option<usize>,
    start: usize,
    image: Option<usize>,
}
struct Builder {
    text: String,
    spans: Vec<Span>,
    lines: Vec<Line>,
    start: usize,
    indent: i32,
    cells: Vec<Cell>,
    cell_start: Option<usize>,
    alignments: Vec<Alignment>,
    links: Vec<Target>,
    images: Vec<usize>,
}
struct EmbeddedImage {
    target: Option<Target>,
    bitmap: Option<Arc<Bitmap>>,
    status: ImageStatus,
}
enum ImageStatus {
    Pending,
    Loaded,
    Blocked,
    Limit,
    Error(String),
}
struct PlacedImage {
    index: usize,
    status: Rect,
    bitmap: Option<Rect>,
}
fn copy_target(target: &Target) -> Target {
    match target {
        Target::Fragment(value) => Target::Fragment(value.clone()),
        Target::Relative { path, fragment } => Target::Relative {
            path: path.clone(),
            fragment: fragment.clone(),
        },
    }
}
impl Builder {
    fn append(&mut self, text: &str, style: Style) -> Result<(), String> {
        if self
            .text
            .len()
            .checked_add(text.len())
            .is_none_or(|n| n > MAX_BYTES)
        {
            return Err("Markdown: límite de texto visible (384 KiB)".into());
        }
        for part in text.split_inclusive('\n') {
            let value = part.strip_suffix('\n').unwrap_or(part);
            let start = self.text.len();
            self.text.push_str(value);
            if !value.is_empty() {
                if self.spans.len() >= MAX_SPANS {
                    return Err("Markdown: demasiados fragmentos de estilo".into());
                }
                self.spans.push(Span {
                    range: start..self.text.len(),
                    style,
                });
            }
            if part.ends_with('\n') {
                self.newline(true)?;
            }
        }
        Ok(())
    }
    fn newline(&mut self, force: bool) -> Result<(), String> {
        if !force && self.start == self.text.len() && self.cells.is_empty() {
            return Ok(());
        }
        if self.lines.len() >= 2000 || self.text.len() == MAX_BYTES {
            return Err("Markdown: límite de filas (2001)".into());
        }
        self.lines.push(Line {
            range: self.start..self.text.len(),
            indent: self.indent,
            cells: std::mem::take(&mut self.cells),
            rule: false,
            images: std::mem::take(&mut self.images),
        });
        self.text.push('\n');
        self.start = self.text.len();
        Ok(())
    }
}
impl Rendered {
    pub fn new(ir: Document) -> Result<Self, String> {
        let mut b = Builder {
            text: String::new(),
            spans: Vec::new(),
            lines: Vec::new(),
            start: 0,
            indent: 0,
            cells: Vec::new(),
            cell_start: None,
            alignments: Vec::new(),
            links: Vec::new(),
            images: Vec::new(),
        };
        let mut headings = Vec::new();
        let mut pending_images = 0usize;
        let mut source_bytes = 0usize;
        let mut images = Vec::new();
        let mut requested_images = 0usize;
        let mut stack: Vec<Frame<'_>> = Vec::new();
        for item in ir.items() {
            if item.source.start > item.source.end
                || item.source.end > crate::markdown_document::MAX_SOURCE_BYTES
            {
                return Err("Markdown: rango de origen inválido".into());
            }
            source_bytes = source_bytes.max(item.source.end);
            let style = current_style(&stack);
            match &item.atom {
                Atom::Open(kind) => {
                    let mut link = None;
                    let mut image = None;
                    match kind {
                        Container::Paragraph
                            if stack
                                .last()
                                .is_some_and(|f| matches!(f.kind, Container::Item)) => {}
                        Container::Heading(_)
                        | Container::Paragraph
                        | Container::Quote
                        | Container::CodeBlock { .. }
                        | Container::List { .. }
                        | Container::Table(_)
                        | Container::TableHead
                        | Container::TableRow => {
                            b.newline(false)?;
                        }
                        _ => {}
                    }
                    match kind {
                        Container::CodeBlock { language } => {
                            ir.text(language.clone())
                                .ok_or("Markdown: idioma inválido")?;
                        }
                        Container::Table(alignments) => b.alignments = alignments.clone(),
                        Container::TableCell => {
                            if !b.cells.is_empty() {
                                b.append("\t", style)?;
                            }
                            b.cell_start = Some(b.text.len());
                        }
                        Container::Item => {
                            b.newline(false)?;
                            let depth = stack
                                .iter()
                                .filter(|f| matches!(f.kind, Container::List { .. }))
                                .count();
                            b.indent = depth.saturating_sub(1) as i32 * 20;
                            let label = if let Some(list) = stack
                                .iter_mut()
                                .rev()
                                .find(|f| matches!(f.kind, Container::List { .. }))
                            {
                                if matches!(list.kind, Container::List { first: Some(_) }) {
                                    let label = format!("{}. ", list.counter);
                                    list.counter = list.counter.saturating_add(1);
                                    label
                                } else {
                                    "• ".into()
                                }
                            } else {
                                "• ".into()
                            };
                            b.append(&label, style)?;
                        }
                        Container::Link {
                            target: Some(target),
                        } => {
                            link = Some(b.links.len());
                            b.links.push(copy_target(target));
                        }
                        Container::Image { target } => {
                            pending_images += 1;
                            if !stack.iter().any(|f| matches!(f.kind, Container::TableCell)) {
                                b.newline(false)?;
                            }
                            let eligible = matches!(target, Some(Target::Relative { .. }));
                            let status = if !eligible {
                                ImageStatus::Blocked
                            } else if requested_images >= MAX_IMAGES {
                                ImageStatus::Limit
                            } else {
                                requested_images += 1;
                                ImageStatus::Pending
                            };
                            image = Some(images.len());
                            images.push(EmbeddedImage {
                                target: target.as_ref().map(copy_target),
                                bitmap: None,
                                status,
                            });
                            b.append("[imagen: ", style)?;
                        }
                        _ => {}
                    }
                    let counter = match kind {
                        Container::List { first: Some(n) } => *n,
                        _ => 0,
                    };
                    stack.push(Frame {
                        kind,
                        counter,
                        link,
                        start: b.text.len(),
                        image,
                    });
                }
                Atom::Close => {
                    let frame = stack.pop().ok_or("Markdown: estructura inválida")?;
                    if matches!(frame.kind, Container::Heading(_)) {
                        headings.push((slug(&b.text[frame.start..]), frame.start));
                    }
                    match frame.kind {
                        Container::TableCell => {
                            let col = b.cells.len();
                            b.cells.push(Cell {
                                range: b.cell_start.take().unwrap_or(b.text.len())..b.text.len(),
                                alignment: b
                                    .alignments
                                    .get(col)
                                    .copied()
                                    .unwrap_or(Alignment::None),
                            });
                        }
                        Container::TableHead | Container::TableRow => b.newline(false)?,
                        Container::Image { .. } => {
                            b.append("]", style)?;
                            if let Some(index) = frame.image {
                                b.images.push(index);
                            }
                            if !stack.iter().any(|f| matches!(f.kind, Container::TableCell)) {
                                b.newline(false)?;
                            }
                        }
                        Container::Paragraph
                        | Container::Heading(_)
                        | Container::CodeBlock { .. }
                        | Container::Quote
                        | Container::Item
                        | Container::List { .. }
                        | Container::LiteralHtml => {
                            b.newline(false)?;
                            if matches!(
                                frame.kind,
                                Container::Paragraph
                                    | Container::Heading(_)
                                    | Container::CodeBlock { .. }
                            ) {
                                b.newline(true)?;
                            }
                            b.indent = stack
                                .iter()
                                .filter(|f| matches!(f.kind, Container::List { .. }))
                                .count()
                                .saturating_sub(1) as i32
                                * 20;
                        }
                        Container::Table(_) => {
                            b.newline(false)?;
                            b.alignments.clear();
                        }
                        _ => {}
                    }
                }
                Atom::Text(range) => b.append(
                    ir.text(range.clone()).ok_or("Markdown: rango inválido")?,
                    style,
                )?,
                Atom::InlineCode(range) | Atom::LiteralHtml(range) => {
                    b.append(
                        ir.text(range.clone()).ok_or("Markdown: rango inválido")?,
                        Style {
                            code: true,
                            ..style
                        },
                    )?;
                }
                Atom::SoftBreak => b.append(" ", style)?,
                Atom::HardBreak => b.newline(true)?,
                Atom::Task(checked) => b.append(if *checked { "[x] " } else { "[ ] " }, style)?,
                Atom::Rule => {
                    b.newline(false)?;
                    b.newline(true)?;
                    if let Some(line) = b.lines.last_mut() {
                        line.rule = true;
                    }
                }
            }
        }
        if !stack.is_empty() {
            return Err("Markdown: estructura sin cerrar".into());
        }
        b.lines.push(Line {
            range: b.start..b.text.len(),
            indent: 0,
            cells: Vec::new(),
            rule: false,
            images: std::mem::take(&mut b.images),
        });
        let document = TextDocument::new(b.text).map_err(str::to_owned)?;
        Ok(Self {
            document,
            viewport: Viewport::default(),
            layout_error: None,
            blocked_targets: ir.blocked_targets,
            pending_images,
            loaded_images: 0,
            image_bytes: 0,
            source_bytes,
            max_layout_step_us: 0,
            spans: b.spans,
            lines: b.lines,
            links: b.links,
            runs: Vec::new(),
            width: -1,
            rules: Vec::new(),
            headings,
            pending_reveal: None,
            layout_line: 0,
            layout_cell: 0,
            layout_y: PAD,
            layout_row_height: 24,
            active: None,
            active_run_start: 0,
            images,
            placements: Vec::new(),
            image_viewport: image_preview_view::Viewport::default(),
            painted_image: None,
        })
    }
    /// Up to eight eligible local resources. No I/O is performed here. The
    /// caller resolves relative paths against a pinned helper root, not the UI.
    pub fn image_requests(&self) -> Vec<(usize, Target)> {
        self.images
            .iter()
            .enumerate()
            .filter_map(|(index, image)| {
                if matches!(image.status, ImageStatus::Pending) {
                    image
                        .target
                        .as_ref()
                        .map(|target| (index, copy_target(target)))
                } else {
                    None
                }
            })
            .take(MAX_IMAGES)
            .collect()
    }
    /// ONE software sampling buffer, independent of the retained Arc ledger.
    pub fn image_buffer_capacity(&self) -> usize {
        self.image_viewport.buffer_capacity()
    }
    /// There may still be room for a new unique bitmap. Duplicate Arc payloads
    /// are accepted by set_image_shared even when the unique ledger is full.
    pub fn can_load_image(&self) -> bool {
        self.image_bytes < MAX_IMAGE_BYTES
    }
    /// Convenience ownership adapter; never clones RGB. Sharing callers should
    /// use set_image_shared to reuse a decoded resource across occurrences.
    #[cfg(test)]
    pub fn set_image(
        &mut self,
        index: usize,
        result: Result<Bitmap, String>,
    ) -> Result<(), String> {
        self.set_image_shared(index, result.map(Arc::new))
    }
    /// The 4 MiB ledger counts each unique Arc payload once: RGB/format capacity,
    /// Bitmap plus Arc counters. It excludes metadata, the ONE viewport sample
    /// buffer (<=640*320*3), decoder/helper and backend image copies. A caller's
    /// cache must not retain rejected payloads outside this ledger.
    pub fn set_image_shared(
        &mut self,
        index: usize,
        result: Result<Arc<Bitmap>, String>,
    ) -> Result<(), String> {
        let image = self
            .images
            .get(index)
            .ok_or("Markdown: índice de imagen inválido")?;
        if !matches!(image.status, ImageStatus::Pending) {
            return Err("Markdown: imagen no pendiente".into());
        }
        match result {
            Err(error) => self.images[index].status = ImageStatus::Error(bounded_error(&error)),
            Ok(bitmap) => {
                let expected = (bitmap.width as usize)
                    .checked_mul(bitmap.height as usize)
                    .and_then(|n| n.checked_mul(3));
                let shared = self
                    .images
                    .iter()
                    .filter_map(|image| image.bitmap.as_ref())
                    .any(|other| Arc::ptr_eq(other, &bitmap));
                let cost = if shared {
                    Some(0)
                } else {
                    bitmap
                        .rgb
                        .capacity()
                        .checked_add(bitmap.format.capacity())
                        .and_then(|n| {
                            n.checked_add(
                                std::mem::size_of::<Bitmap>() + 2 * std::mem::size_of::<usize>(),
                            )
                        })
                };
                let failure = if bitmap.width == 0
                    || bitmap.height == 0
                    || bitmap.width > 1024
                    || bitmap.height > 1024
                    || bitmap.source_width == 0
                    || bitmap.source_height == 0
                    || bitmap.source_width > 8192
                    || bitmap.source_height > 8192
                    || expected != Some(bitmap.rgb.len())
                    || !matches!(bitmap.format.as_str(), "png" | "jpeg" | "webp")
                {
                    Some("Markdown: bitmap inválido")
                } else if cost
                    .and_then(|n| self.image_bytes.checked_add(n))
                    .is_none_or(|n| n > MAX_IMAGE_BYTES)
                {
                    Some("Markdown: límite de bitmaps retenidos (4 MiB)")
                } else {
                    None
                };
                if let Some(error) = failure {
                    self.images[index].status = ImageStatus::Error(error.into());
                    return Err(error.into());
                }
                self.image_bytes += cost.expect("validated cost");
                self.images[index].bitmap = Some(bitmap);
                self.images[index].status = ImageStatus::Loaded;
                self.loaded_images += 1;
                self.pending_images = self.pending_images.saturating_sub(1);
                self.width = -1;
                self.image_viewport.reset_content();
                self.painted_image = None;
            }
        }
        Ok(())
    }
    /// A step budget is cooperative between bounded runs. Individual backend
    /// shaping calls cannot be preempted, so 4 ms is a scheduling target, not a
    /// worst-case latency guarantee. Call paint again while needs_layout().
    pub fn layout_ready(&self) -> bool {
        self.width >= 0 && (self.layout_line >= self.lines.len() || self.layout_error.is_some())
    }
    pub fn needs_layout(&self) -> bool {
        !self.layout_ready()
    }
    fn ensure_layout(&mut self, p: &mut dyn Painter, width: i32) {
        let started = Instant::now();
        let width = width.max(80);
        if self.width != width {
            self.width = width;
            self.runs.clear();
            self.rules.clear();
            self.placements.clear();
            self.layout_error = None;
            self.layout_line = 0;
            self.layout_cell = 0;
            self.layout_y = PAD;
            self.layout_row_height = 24;
            self.active = None;
        }
        if self.layout_ready() {
            return;
        }
        let deadline = Instant::now() + Duration::from_millis(4);
        while let Some(line) = self.lines.get(self.layout_line) {
            if self.active.is_none() {
                if line.rule {
                    if self.runs.len() >= MAX_RUNS {
                        self.layout_error =
                            Some("Markdown: límite de layout (32768 fragmentos)".into());
                        break;
                    }
                    self.rules.push(Rect::new(
                        PAD,
                        self.layout_y + 9,
                        (width - PAD * 2).max(1),
                        1,
                    ));
                    self.runs.push(Run {
                        range: line.range.clone(),
                        rect: Rect::new(PAD, self.layout_y, 0, 22),
                        style: Style {
                            size: 14,
                            ..Style::default()
                        },
                        column: 0,
                    });
                    self.layout_y += 22;
                    self.layout_line += 1;
                    if Instant::now() >= deadline {
                        break;
                    }
                    continue;
                }
                let (range, region) = if line.cells.is_empty() {
                    (
                        line.range.clone(),
                        Rect::new(
                            PAD + line.indent,
                            self.layout_y,
                            (width - PAD * 2 - line.indent).max(30),
                            0,
                        ),
                    )
                } else {
                    if line.cells.len() as i32 * 24 > width - PAD * 2 {
                        self.layout_error =
                            Some("Markdown: tabla demasiado ancha; amplía la vista".into());
                        break;
                    }
                    let cell_width = ((width - PAD * 2) / line.cells.len() as i32).max(24);
                    let cell = &line.cells[self.layout_cell];
                    (
                        cell.range.clone(),
                        Rect::new(
                            PAD + self.layout_cell as i32 * cell_width + 6,
                            self.layout_y,
                            (cell_width - 12).max(12),
                            0,
                        ),
                    )
                };
                self.active_run_start = self.runs.len();
                self.active = Some(RangeLayout::new(range, region, &self.spans));
            }
            let state = self.active.as_mut().expect("range initialized");
            match state.step(p, self.document.text(), &self.spans, &mut self.runs) {
                Err(error) => {
                    self.layout_error = Some(error.into());
                    break;
                }
                Ok(Some(height)) => {
                    self.active = None;
                    if line.cells.is_empty() {
                        self.layout_y += height.max(22);
                        place_images(
                            &line.images,
                            &self.images,
                            &mut self.placements,
                            &mut self.layout_y,
                            width,
                        );
                        self.layout_line += 1;
                    } else {
                        let cell_width = ((width - PAD * 2) / line.cells.len() as i32).max(24);
                        align_cell(
                            &mut self.runs[self.active_run_start..],
                            line.cells[self.layout_cell].alignment,
                            PAD + (self.layout_cell as i32 + 1) * cell_width - 6,
                        );
                        self.layout_row_height = self.layout_row_height.max(height);
                        self.layout_cell += 1;
                        if self.layout_cell == line.cells.len() {
                            self.layout_y += self.layout_row_height + 7;
                            place_images(
                                &line.images,
                                &self.images,
                                &mut self.placements,
                                &mut self.layout_y,
                                width,
                            );
                            self.layout_line += 1;
                            self.layout_cell = 0;
                            self.layout_row_height = 24;
                        }
                    }
                }
                Ok(None) => {}
            }
            if Instant::now() >= deadline {
                break;
            }
        }
        let active_bottom = self
            .active
            .as_ref()
            .map_or(self.layout_y, |a| a.y + a.height);
        self.viewport.content_height = self.layout_y.max(active_bottom) + PAD;
        if let Some(offset) = self.pending_reveal
            && let Some(run) = self
                .runs
                .iter()
                .find(|r| r.range.start <= offset && offset <= r.range.end)
        {
            self.viewport.top = run.rect.y;
            self.pending_reveal = None;
        }
        self.viewport.top = self.viewport.top.clamp(
            0,
            (self.viewport.content_height - self.viewport.body.h).max(0),
        );
        self.max_layout_step_us = self.max_layout_step_us.max(started.elapsed().as_micros());
    }
    pub fn paint(&mut self, p: &mut dyn Painter, body: Rect, c: &Palette) {
        self.viewport.body = body;
        self.ensure_layout(p, body.w);
        self.viewport.top = self
            .viewport
            .top
            .clamp(0, (self.viewport.content_height - body.h).max(0));
        p.clip(body);
        for rule in &self.rules {
            p.rect(
                Rect::new(
                    body.x + rule.x,
                    body.y + rule.y - self.viewport.top,
                    rule.w,
                    rule.h,
                ),
                c.muted,
            );
        }
        let selection = self.document.selection_range();
        for run in &self.runs {
            let mut rect = run.rect;
            rect.x += body.x;
            rect.y += body.y - self.viewport.top;
            if rect.y + rect.h < body.y || rect.y > body.y + body.h {
                continue;
            }
            let source = &self.document.text()[run.range.clone()];
            let display = Display::new_at(source, run.column);
            if run.style.code {
                p.rect(rect, c.darker_background);
            }
            if run.style.quote {
                p.rect(Rect::new(body.x + 6, rect.y, 2, rect.h), c.muted);
            }
            if !selection.is_empty()
                && selection.start < run.range.end
                && selection.end > run.range.start
            {
                let left = display.advance(
                    p,
                    selection.start.saturating_sub(run.range.start),
                    run.style,
                );
                let right =
                    display.advance(p, selection.end.saturating_sub(run.range.start), run.style);
                p.rect(
                    Rect::new(
                        rect.x + left.round() as i32,
                        rect.y,
                        (right - left).ceil().max(1.0) as i32,
                        rect.h,
                    ),
                    c.selection,
                );
            }
            let color = if run.style.link.is_some() {
                c.accent
            } else if run.style.quote {
                c.dark_foreground
            } else {
                c.foreground
            };
            p.styled_text(
                Rect::new(rect.x, rect.y, rect.w + 2, rect.h),
                &display.text,
                run.style.size,
                color,
                run.style.bold,
                run.style.italic,
            );
            if run.style.link.is_some() || run.style.strike {
                p.rect(
                    Rect::new(
                        rect.x,
                        rect.y
                            + if run.style.strike {
                                rect.h / 2
                            } else {
                                rect.h - 3
                            },
                        rect.w,
                        1,
                    ),
                    color,
                );
            }
            if self.document.caret() >= run.range.start && self.document.caret() <= run.range.end {
                let x = display.advance(p, self.document.caret() - run.range.start, run.style);
                p.rect(
                    Rect::new(
                        rect.x + x.round() as i32,
                        rect.y + 2,
                        1,
                        (rect.h - 4).max(1),
                    ),
                    c.accent,
                );
            }
        }
        for placed in &self.placements {
            let mut status = placed.status;
            status.x += body.x;
            status.y += body.y - self.viewport.top;
            let bottom = placed.bitmap.map_or(status.y + status.h, |r| {
                body.y + r.y - self.viewport.top + r.h
            });
            if bottom < body.y || status.y > body.y + body.h {
                continue;
            }
            let image = &self.images[placed.index];
            let label = match &image.status {
                ImageStatus::Pending => "Imagen local pendiente",
                ImageStatus::Loaded => "Imagen local",
                ImageStatus::Blocked => "Imagen bloqueada: sólo rutas locales relativas",
                ImageStatus::Limit => "Imagen pendiente: límite de 8 recursos por documento",
                ImageStatus::Error(error) => error,
            };
            p.text(
                status,
                label,
                11,
                if matches!(image.status, ImageStatus::Error(_) | ImageStatus::Blocked) {
                    c.red
                } else {
                    c.dark_foreground
                },
                true,
                false,
            );
            if let (Some(bitmap), Some(mut rect)) = (&image.bitmap, placed.bitmap) {
                rect.x += body.x;
                rect.y += body.y - self.viewport.top;
                if self.painted_image != Some(placed.index) {
                    self.image_viewport.reset_content();
                    self.painted_image = Some(placed.index);
                }
                self.image_viewport.paint(p, bitmap, rect, c);
            }
        }
        if let Some(error) = &self.layout_error {
            p.text(
                Rect::new(body.x + PAD, body.y + body.h - 28, body.w - PAD * 2, 25),
                error,
                12,
                c.red,
                true,
                false,
            );
        }
        p.unclip();
    }
    pub fn offset_at(&mut self, p: &mut dyn Painter, x: i32, y: i32) -> usize {
        self.ensure_layout(p, self.viewport.body.w);
        let x = x - self.viewport.body.x;
        let y = y - self.viewport.body.y + self.viewport.top;
        let Some(run) = self.runs.iter().min_by_key(|r| {
            let dy = if y < r.rect.y {
                r.rect.y - y
            } else {
                (y - r.rect.y - r.rect.h).max(0)
            };
            let dx = if x < r.rect.x {
                r.rect.x - x
            } else {
                (x - r.rect.x - r.rect.w).max(0)
            };
            (dy, dx)
        }) else {
            return 0;
        };
        let display = Display::new_at(&self.document.text()[run.range.clone()], run.column);
        run.range.start + display.offset_at(p, (x - run.rect.x) as f32, run.style)
    }
    pub fn point_for_offset(&mut self, p: &mut dyn Painter, offset: usize) -> (i32, i32) {
        self.ensure_layout(p, self.viewport.body.w);
        let offset = self.document.boundary(offset);
        let Some(run) = self
            .runs
            .iter()
            .find(|r| r.range.start <= offset && offset <= r.range.end)
        else {
            if self.needs_layout() {
                self.pending_reveal = Some(offset);
            }
            return (
                self.viewport.body.x + PAD,
                self.viewport.body.y
                    + (self.viewport.content_height - self.viewport.top - PAD)
                        .clamp(PAD, self.viewport.body.h.max(PAD)),
            );
        };
        let display = Display::new_at(&self.document.text()[run.range.clone()], run.column);
        (
            self.viewport.body.x
                + run.rect.x
                + display
                    .advance(p, offset - run.range.start, run.style)
                    .round() as i32,
            self.viewport.body.y + run.rect.y + run.rect.h / 2 - self.viewport.top,
        )
    }
    pub fn link_at(&mut self, p: &mut dyn Painter, x: i32, y: i32) -> Option<Target> {
        if !self.viewport.body.contains(x, y) {
            return None;
        }
        self.ensure_layout(p, self.viewport.body.w);
        let x = x - self.viewport.body.x;
        let y = y - self.viewport.body.y + self.viewport.top;
        self.runs
            .iter()
            .find(|r| r.rect.contains(x, y))
            .and_then(|r| r.style.link)
            .and_then(|index| self.links.get(index))
            .map(copy_target)
    }
    /// Heading IDs use lowercase Unicode alphanumerics and whitespace dashes.
    pub fn jump_to_fragment(&mut self, fragment: &str) -> bool {
        let Some((_, offset)) = self.headings.iter().find(|(id, _)| id == fragment) else {
            return false;
        };
        if let Some(run) = self
            .runs
            .iter()
            .find(|r| r.range.start <= *offset && *offset <= r.range.end)
        {
            self.viewport.top = run.rect.y.clamp(
                0,
                (self.viewport.content_height - self.viewport.body.h).max(0),
            );
        } else {
            self.pending_reveal = Some(*offset);
        }
        true
    }
    pub fn scroll(&mut self, delta: i32) {
        self.viewport.top = (self.viewport.top as i64 + delta as i64 * 60).clamp(
            0,
            (self.viewport.content_height - self.viewport.body.h).max(0) as i64,
        ) as i32;
    }
    pub fn reveal(&mut self) {
        if let Some(run) = self.runs.iter().find(|r| {
            r.range.start <= self.document.caret() && self.document.caret() <= r.range.end
        }) {
            if run.rect.y < self.viewport.top {
                self.viewport.top = run.rect.y;
            }
            if run.rect.y + run.rect.h > self.viewport.top + self.viewport.body.h {
                self.viewport.top = (run.rect.y + run.rect.h - self.viewport.body.h).max(0);
            }
        } else if self.needs_layout() {
            self.pending_reveal = Some(self.document.caret());
        }
    }
}
fn current_style(stack: &[Frame<'_>]) -> Style {
    let mut style = Style {
        size: 14,
        ..Style::default()
    };
    for frame in stack {
        match frame.kind {
            Container::Heading(level) => {
                style.size = match level {
                    1 => 27,
                    2 => 23,
                    3 => 20,
                    _ => 17,
                };
                style.bold = true;
            }
            Container::Emphasis => style.italic = true,
            Container::Strong | Container::TableHead => style.bold = true,
            Container::Strike => style.strike = true,
            Container::CodeBlock { .. } | Container::LiteralHtml => style.code = true,
            Container::Quote => style.quote = true,
            _ => {}
        }
        if frame.link.is_some() {
            style.link = frame.link;
        }
    }
    style
}
/// Per-run display transformation, at most 256 source bytes. Original tabs and
/// controls remain in document selection/copy; measurement/drawing share bytes.
struct Display {
    text: String,
    boundaries: Vec<(usize, usize)>,
}
impl Display {
    fn new_at(source: &str, initial_column: usize) -> Self {
        let mut text = String::new();
        let mut boundaries = vec![(0, 0)];
        let mut column = initial_column;
        for (offset, ch) in source.char_indices() {
            if ch == '\t' {
                let spaces = 8 - column % 8;
                text.extend(std::iter::repeat_n(' ', spaces));
                column += spaces;
            } else if ch.is_ascii_control() {
                text.push('^');
                text.push((ch as u8 ^ 0x40) as char);
                column += 2;
            } else {
                text.push(ch);
                column += 1;
            }
            boundaries.push((offset + ch.len_utf8(), text.len()));
        }
        Self { text, boundaries }
    }
    fn advance(&self, p: &mut dyn Painter, offset: usize, style: Style) -> f32 {
        let index = self
            .boundaries
            .partition_point(|(source, _)| *source <= offset)
            .saturating_sub(1);
        p.styled_width(
            &self.text[..self.boundaries[index].1],
            style.size,
            style.bold,
            style.italic,
        )
    }
    fn offset_at(&self, p: &mut dyn Painter, hit: f32, style: Style) -> usize {
        let mut low = 0;
        let mut high = self.boundaries.len() - 1;
        while low < high {
            let mid = (low + high) / 2;
            let a = p.styled_width(
                &self.text[..self.boundaries[mid].1],
                style.size,
                style.bold,
                style.italic,
            );
            let b = p.styled_width(
                &self.text[..self.boundaries[mid + 1].1],
                style.size,
                style.bold,
                style.italic,
            );
            if hit > (a + b) / 2.0 {
                low = mid + 1;
            } else {
                high = mid;
            }
        }
        self.boundaries[low].0
    }
}
fn bounded_error(value: &str) -> String {
    let mut end = value.len().min(512);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    let mut text = String::from("Imagen no disponible: ");
    text.push_str(&value[..end]);
    if end < value.len() {
        text.push('…');
    }
    text
}
fn place_images(
    indices: &[usize],
    images: &[EmbeddedImage],
    placements: &mut Vec<PlacedImage>,
    y: &mut i32,
    width: i32,
) {
    for &index in indices {
        let status = Rect::new(PAD, *y, (width - PAD * 2).max(1), 24);
        *y += 24;
        let bitmap = images[index].bitmap.as_ref().map(|bitmap| {
            let scale = (((width - PAD * 2).clamp(1, 640)) as f32 / bitmap.width as f32)
                .min(320.0 / bitmap.height as f32)
                .min(1.0);
            let rect = Rect::new(
                PAD,
                *y,
                (bitmap.width as f32 * scale).floor().max(1.0) as i32,
                (bitmap.height as f32 * scale).floor().max(1.0) as i32,
            );
            *y += rect.h;
            rect
        });
        *y += 8;
        placements.push(PlacedImage {
            index,
            status,
            bitmap,
        });
    }
}
struct RangeLayout {
    range: Range<usize>,
    region: Rect,
    span: usize,
    offset: usize,
    x: i32,
    y: i32,
    height: i32,
    column: usize,
    emitted: bool,
}
impl RangeLayout {
    fn new(range: Range<usize>, region: Rect, spans: &[Span]) -> Self {
        Self {
            span: spans.partition_point(|span| span.range.end <= range.start),
            offset: range.start,
            range,
            region,
            x: region.x,
            y: region.y,
            height: 22,
            column: 0,
            emitted: false,
        }
    }
    /// Emits at most one bounded glyph run, making even a single 256 KiB
    /// paragraph resumable. No retained copy of its text is created.
    fn step(
        &mut self,
        p: &mut dyn Painter,
        text: &str,
        spans: &[Span],
        runs: &mut Vec<Run>,
    ) -> Result<Option<i32>, &'static str> {
        while let Some(span) = spans.get(self.span) {
            if span.range.start >= self.range.end {
                break;
            }
            let start = self.offset.max(span.range.start).max(self.range.start);
            let end = span.range.end.min(self.range.end);
            if start >= end {
                self.span += 1;
                continue;
            }
            let source = &text[start..end];
            let mut count = 0;
            let mut saw_space = false;
            for (index, ch) in source.char_indices() {
                if index >= MAX_CHUNK || (saw_space && !ch.is_whitespace()) {
                    break;
                }
                count = index + ch.len_utf8();
                saw_space |= ch.is_whitespace();
            }
            let mut stop = start + count;
            let mut display = Display::new_at(&text[start..stop], self.column);
            let mut advance = p.styled_width(
                &display.text,
                span.style.size,
                span.style.bold,
                span.style.italic,
            );
            let row_height = (span.style.size + 10).max(22);
            if self.x > self.region.x
                && self.x as f32 + advance > (self.region.x + self.region.w) as f32
            {
                self.y += self.height;
                self.x = self.region.x;
                self.height = 22;
                self.column = 0;
                display = Display::new_at(&text[start..stop], self.column);
                advance = p.styled_width(
                    &display.text,
                    span.style.size,
                    span.style.bold,
                    span.style.italic,
                );
            }
            if advance > self.region.w as f32 {
                let mut low = 1usize;
                let mut high = display.boundaries.len() - 1;
                while low < high {
                    let mid = (low + high).div_ceil(2);
                    let width = p.styled_width(
                        &display.text[..display.boundaries[mid].1],
                        span.style.size,
                        span.style.bold,
                        span.style.italic,
                    );
                    if width <= self.region.w as f32 {
                        low = mid;
                    } else {
                        high = mid - 1;
                    }
                }
                stop = start + display.boundaries[low].0;
                display = Display::new_at(&text[start..stop], self.column);
                advance = p.styled_width(
                    &display.text,
                    span.style.size,
                    span.style.bold,
                    span.style.italic,
                );
            }
            if runs.len() >= MAX_RUNS {
                return Err("Markdown: límite de layout (32768 fragmentos)");
            }
            runs.push(Run {
                range: start..stop,
                rect: Rect::new(self.x, self.y, advance.ceil() as i32, row_height),
                style: span.style,
                column: self.column,
            });
            self.emitted = true;
            for ch in text[start..stop].chars() {
                self.column += if ch == '\t' {
                    8 - self.column % 8
                } else if ch.is_ascii_control() {
                    2
                } else {
                    1
                };
            }
            self.x += advance.ceil() as i32;
            self.height = self.height.max(row_height);
            self.offset = stop;
            if stop == end {
                self.span += 1;
            }
            return Ok(None);
        }
        if !self.emitted {
            if runs.len() >= MAX_RUNS {
                return Err("Markdown: límite de layout (32768 fragmentos)");
            }
            runs.push(Run {
                range: self.range.clone(),
                rect: Rect::new(self.region.x, self.y, 0, 22),
                style: Style {
                    size: 14,
                    ..Style::default()
                },
                column: 0,
            });
            self.emitted = true;
        }
        Ok(Some(self.y - self.region.y + self.height))
    }
}
fn align_cell(runs: &mut [Run], alignment: Alignment, available_right: i32) {
    let mut first = 0;
    while first < runs.len() {
        let row_y = runs[first].rect.y;
        let end = first
            + runs[first..]
                .iter()
                .take_while(|r| r.rect.y == row_y)
                .count();
        let right = runs[first..end]
            .iter()
            .map(|r| r.rect.x + r.rect.w)
            .max()
            .unwrap_or(0);
        let remaining = (available_right - right).max(0);
        let shift = match alignment {
            Alignment::Center => remaining / 2,
            Alignment::Right => remaining,
            _ => 0,
        };
        for run in &mut runs[first..end] {
            run.rect.x += shift;
        }
        first = end;
    }
}

fn slug(value: &str) -> String {
    let mut result = String::new();
    let mut separator = false;
    for ch in value.chars() {
        if ch.is_alphanumeric() {
            if separator && !result.is_empty() {
                result.push('-');
            }
            result.extend(ch.to_lowercase());
            separator = false;
        } else if ch.is_whitespace() {
            separator = true;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::painter::Image;
    use files_core::theme::Color;
    #[derive(Default)]
    struct Recorder {
        rectangles: Vec<(Rect, Color)>,
        text: Vec<(String, i32, bool, bool)>,
        slow_width: bool,
        images: Vec<(Rect, [u8; 3])>,
    }
    impl Painter for Recorder {
        fn rect(&mut self, r: Rect, c: Color) {
            self.rectangles.push((r, c));
        }
        fn rounded_rect(&mut self, r: Rect, _: i32, c: Color) {
            self.rect(r, c);
        }
        fn text(&mut self, _: Rect, text: &str, size: i32, _: Color, _: bool, _: bool) {
            self.text.push((text.into(), size, false, false));
        }
        fn styled_text(
            &mut self,
            _: Rect,
            text: &str,
            size: i32,
            _: Color,
            bold: bool,
            italic: bool,
        ) {
            self.text.push((text.into(), size, bold, italic));
        }
        fn styled_width(&mut self, text: &str, size: i32, _: bool, _: bool) -> f32 {
            if self.slow_width {
                std::thread::sleep(Duration::from_millis(1));
            }
            text.chars().count() as f32 * size as f32 * 0.6
        }
        fn image(&mut self, r: Rect, image: &Image<'_>) {
            self.images
                .push((r, [image.rgb[0], image.rgb[1], image.rgb[2]]));
        }
        fn clip(&mut self, _: Rect) {}
        fn unclip(&mut self) {}
    }
    fn rendered(source: &str) -> Rendered {
        Rendered::new(Document::parse(source).unwrap()).unwrap()
    }
    #[test]
    fn selection_uses_visible_text_and_excludes_language_metadata() {
        let mut r = rendered(
            "# Título\n\n**fuerte** y *suave* `inline`\n\n3. tres\n4. cuatro\n\n- [x] hecho\n\n```secretlanguage\na\tb\n```\n",
        );
        r.document.select_all();
        let copy = r.document.selected_text();
        assert!(copy.contains("Título"));
        assert!(copy.contains("3. tres"));
        assert!(copy.contains("4. cuatro"));
        assert!(copy.contains("[x] hecho"));
        assert!(copy.contains("a\tb"));
        assert!(!copy.contains("secretlanguage"));
        assert!(!copy.contains("**"));
        let mut p = Recorder::default();
        r.paint(&mut p, Rect::new(0, 0, 600, 1000), &Palette::default());
        assert!(p.text.iter().any(|(_, size, bold, _)| *size == 27 && *bold));
        assert!(p.text.iter().any(|(s, _, bold, _)| s == "fuerte" && *bold));
        assert!(
            p.text
                .iter()
                .any(|(s, _, _, italic)| s == "suave" && *italic)
        );
        assert!(
            r.runs
                .iter()
                .all(|run| r.document.text().get(run.range.clone()).is_some())
        );
    }
    #[test]
    fn empty_selection_does_not_draw_highlight_and_unicode_survives() {
        let mut r = rendered("área 漢字 e\u{301} 👩\u{200d}💻\n");
        r.document.move_to(2, false);
        let mut p = Recorder::default();
        let colors = Palette::default();
        r.paint(&mut p, Rect::new(0, 0, 600, 400), &colors);
        assert!(!p.rectangles.iter().any(|(_, c)| c.0 == colors.selection.0
            && c.1 == colors.selection.1
            && c.2 == colors.selection.2));
        r.document.select_all();
        assert!(r.document.selected_text().contains("e\u{301} 👩\u{200d}💻"));
    }
    #[test]
    fn wrap_resize_and_hit_testing_keep_visible_utf8_ranges() {
        let mut r = rendered("Primera palabra segunda palabra tercera palabra 漢字 final.\n");
        let mut p = Recorder::default();
        r.paint(&mut p, Rect::new(10, 20, 190, 400), &Palette::default());
        let narrow_height = r.viewport.content_height;
        assert!(r.runs.iter().map(|run| run.rect.y).max().unwrap() > PAD);
        for run in &r.runs {
            assert!(r.document.text().is_char_boundary(run.range.start));
            assert!(r.document.text().is_char_boundary(run.range.end));
            assert!(run.rect.x >= PAD);
        }
        let offset = r.document.text().find("segunda").unwrap() + 3;
        let (x, y) = r.point_for_offset(&mut p, offset);
        assert_eq!(r.offset_at(&mut p, x, y), offset);
        r.paint(&mut p, Rect::new(10, 20, 900, 400), &Palette::default());
        assert!(r.viewport.content_height < narrow_height);
    }
    #[test]
    fn tables_align_cells_and_copy_tab_delimiters() {
        let mut r =
            rendered("| Izquierda | Centro | Derecha |\n| :-- | :--: | --: |\n| a | b | c |\n");
        let mut p = Recorder::default();
        r.paint(&mut p, Rect::new(0, 0, 700, 400), &Palette::default());
        assert!(r.document.text().contains("a\tb\tc"));
        let position = |value: &str| {
            r.runs
                .iter()
                .find(|run| &r.document.text()[run.range.clone()] == value)
                .unwrap()
                .rect
                .x
        };
        assert!(position("a") < position("b") && position("b") < position("c"));
        assert!(position("c") > 600);
    }
    #[test]
    fn local_links_and_pre_layout_fragments_are_explicit() {
        let mut r = rendered(
            "# Sección Uno!\n\n[guía](guide.md#top) ![paleta](palette.png) [no](https://example.invalid)\n",
        );
        assert_eq!(r.pending_images, 1);
        assert_eq!(r.blocked_targets, 1);
        assert!(r.jump_to_fragment("sección-uno"));
        assert!(!r.jump_to_fragment("missing"));
        let mut p = Recorder::default();
        r.paint(&mut p, Rect::new(0, 0, 700, 400), &Palette::default());
        let run = r.runs.iter().find(|r| r.style.link.is_some()).unwrap();
        let (x, y) = (run.rect.x + 2, run.rect.y + 5);
        assert!(
            matches!(r.link_at(&mut p,x,y), Some(Target::Relative{path,..}) if path == std::path::Path::new("guide.md"))
        );
        assert!(r.document.text().contains("[imagen: paleta]"));
        assert!(
            p.text
                .iter()
                .any(|(text, _, _, _)| text == "Imagen local pendiente")
        );
    }
    #[test]
    fn tab_expansion_and_hit_map_share_the_same_display() {
        let d = Display::new_at("a\tb\r", 0);
        assert_eq!(d.text, "a       b^M");
        let mut p = Recorder::default();
        let style = Style {
            size: 10,
            ..Style::default()
        };
        assert_eq!(d.advance(&mut p, 2, style), 48.0);
        assert_eq!(d.offset_at(&mut p, 49.0, style), 2);
        assert_eq!(Display::new_at("\tb", 3).text, "     b");
    }
    #[test]
    fn line_budget_is_an_error_and_html_stays_literal() {
        assert!(Rendered::new(Document::parse(&"x\n\n".repeat(1001)).unwrap()).is_err());
        let r = rendered("<script>bad()</script>\n\n![a](https://example.invalid/a.png)");
        assert!(r.document.text().contains("<script>bad()</script>"));
        assert_eq!(r.pending_images, 1);
    }
    fn finish(r: &mut Rendered, p: &mut Recorder, body: Rect) {
        for _ in 0..4096 {
            r.paint(p, body, &Palette::default());
            if r.layout_ready() {
                assert!(r.layout_error.is_none(), "{:?}", r.layout_error);
                return;
            }
        }
        panic!("layout did not finish");
    }
    #[test]
    fn blank_and_final_row_anchors_survive_end_movement_and_resize() {
        let mut r = rendered("uno\n\ndos\n");
        let mut p = Recorder::default();
        finish(&mut r, &mut p, Rect::new(10, 20, 400, 200));
        let blank_offsets: Vec<_> = r
            .lines
            .iter()
            .filter(|l| l.range.is_empty())
            .map(|l| l.range.start)
            .collect();
        assert!(blank_offsets.len() >= 2);
        for offset in blank_offsets {
            r.document.move_to(offset, false);
            r.reveal();
            let (x, y) = r.point_for_offset(&mut p, offset);
            assert_eq!(r.offset_at(&mut p, x, y), offset);
            assert!(r.runs.iter().any(|run| run.range == (offset..offset)));
        }
        let end = r.document.text().len();
        r.document.select(1, end);
        let before = r.document.selected_text().to_owned();
        finish(&mut r, &mut p, Rect::new(10, 20, 130, 100));
        r.reveal();
        let (x, y) = r.point_for_offset(&mut p, end);
        assert_eq!(r.offset_at(&mut p, x, y), end);
        assert_eq!(r.document.selected_text(), before);
        assert!((20..120).contains(&y));
    }
    #[test]
    fn loose_ordered_list_keeps_prefix_on_content_row() {
        let r = rendered("3. tres\n\n4. cuatro\n");
        assert!(r.document.text().contains("3. tres"));
        assert!(r.document.text().contains("4. cuatro"));
        assert!(!r.document.text().contains("3. \n"));
        assert!(!r.document.text().contains("4. \n"));
    }
    #[test]
    fn source_metadata_is_distinct_from_visible_text_and_image_policy() {
        let source =
            "```rust\nx\n```\n\n![local](p.png) ![blocked](https://invalid.example/x.png)\n";
        let r = rendered(source);
        assert!(r.source_bytes <= source.len() && r.source_bytes > 0);
        assert!(r.document.text().contains("[imagen: local]"));
        assert!(r.document.text().contains("[imagen: blocked]"));
        assert!(!r.document.text().contains("rust"));
        assert_eq!(r.pending_images, 2);
    }
    #[test]
    fn long_single_paragraph_reflows_across_cooperative_steps() {
        let mut r = rendered(&"palabra ".repeat(180));
        let mut p = Recorder {
            slow_width: true,
            ..Recorder::default()
        };
        r.paint(&mut p, Rect::new(0, 0, 300, 300), &Palette::default());
        assert!(r.needs_layout());
        assert!(!r.runs.is_empty());
        assert!(r.runs.len() < 180);
        p.slow_width = false;
        finish(&mut r, &mut p, Rect::new(0, 0, 300, 300));
        assert!(r.layout_ready());
        assert!(r.max_layout_step_us > 0);
        assert_eq!(
            r.runs
                .iter()
                .filter(|run| !run.range.is_empty())
                .map(|run| run.range.len())
                .sum::<usize>(),
            r.document.text().trim_end_matches('\n').len()
        );
    }
    fn bitmap(width: u32, height: u32, color: [u8; 3]) -> Bitmap {
        let mut rgb = vec![0; width as usize * height as usize * 3];
        for pixel in rgb.as_chunks_mut::<3>().0 {
            pixel.copy_from_slice(&color);
        }
        Bitmap {
            source: None,
            width,
            height,
            source_width: width,
            source_height: height,
            format: "png".into(),
            rgb,
        }
    }
    #[test]
    fn embedded_bitmaps_are_visible_and_equal_size_colors_do_not_go_stale() {
        let mut r = rendered("![rojo](red.png)\n\n![azul](blue.png)\n");
        let red = bitmap(2, 1, [255, 0, 0]);
        let owned = red.rgb.as_ptr();
        r.set_image(0, Ok(red)).unwrap();
        assert_eq!(r.images[0].bitmap.as_ref().unwrap().rgb.as_ptr(), owned);
        r.set_image(1, Ok(bitmap(2, 1, [0, 0, 255]))).unwrap();
        assert_eq!(r.loaded_images, 2);
        assert_eq!(r.pending_images, 0);
        assert!(r.image_requests().is_empty());
        let mut p = Recorder::default();
        finish(&mut r, &mut p, Rect::new(0, 0, 500, 700));
        assert!(p.images.iter().any(|(_, rgb)| *rgb == [255, 0, 0]));
        assert!(p.images.iter().any(|(_, rgb)| *rgb == [0, 0, 255]));
        p.images.clear();
        r.paint(&mut p, Rect::new(0, 0, 500, 700), &Palette::default());
        assert_eq!(
            p.images.iter().map(|(_, rgb)| *rgb).collect::<Vec<_>>(),
            [[255, 0, 0], [0, 0, 255]]
        );
        assert!(r.image_buffer_capacity() <= 640 * 320 * 3);
        assert!(!r.document.text().contains("no cargado"));
        assert!(r.document.text().contains("[imagen: rojo]"));
    }
    #[test]
    fn embedded_budget_counts_all_capacities_and_keeps_previous_bitmaps() {
        let mut r = rendered("![a](a.png) ![b](b.png) ![c](c.png)");
        r.set_image(0, Ok(bitmap(700, 700, [1, 2, 3]))).unwrap();
        r.set_image(1, Ok(bitmap(700, 700, [4, 5, 6]))).unwrap();
        let before = r.image_bytes;
        assert!(r.set_image(2, Ok(bitmap(700, 700, [7, 8, 9]))).is_err());
        assert_eq!(r.loaded_images, 2);
        assert_eq!(r.pending_images, 1);
        assert_eq!(r.image_bytes, before);
        assert!(r.image_bytes <= MAX_IMAGE_BYTES);
        assert!(matches!(r.images[2].status, ImageStatus::Error(_)));
        let mut cap = rendered("![cap](cap.png)");
        let mut large = bitmap(1, 1, [0, 0, 0]);
        large.rgb.reserve_exact(MAX_IMAGE_BYTES);
        assert!(large.rgb.capacity() > MAX_IMAGE_BYTES);
        assert!(cap.set_image(0, Ok(large)).is_err());
        assert_eq!(cap.image_bytes, 0);
        let mut format = bitmap(1, 1, [0, 0, 0]);
        format.format.reserve_exact(MAX_IMAGE_BYTES);
        let mut cap = rendered("![cap](cap.png)");
        assert!(cap.set_image(0, Ok(format)).is_err());
        assert_eq!(cap.loaded_images, 0);
    }
    #[test]
    fn image_count_policy_alt_errors_and_malformed_bitmaps_are_explicit() {
        let mut source = String::from("![remota](https://invalid.example/a.png) ![fragment](#x)\n");
        for index in 0..10 {
            source.push_str(&format!("![alt{index}](p{index}.png)\n"));
        }
        let mut r = rendered(&source);
        assert_eq!(r.image_requests().len(), MAX_IMAGES);
        assert_eq!(r.pending_images, 12);
        r.set_image(2, Err("no existe".into())).unwrap();
        assert_eq!(r.image_requests().len(), 7);
        assert!(r.set_image(0, Ok(bitmap(1, 1, [0, 0, 0]))).is_err());
        let mut invalid = bitmap(2, 2, [0, 0, 0]);
        invalid.rgb.pop();
        assert!(r.set_image(3, Ok(invalid)).is_err());
        let mut p = Recorder::default();
        finish(&mut r, &mut p, Rect::new(0, 0, 600, 2400));
        assert!(
            p.text
                .iter()
                .any(|(text, _, _, _)| text.contains("bloqueada"))
        );
        assert!(
            p.text
                .iter()
                .any(|(text, _, _, _)| text.contains("no existe"))
        );
        assert!(
            p.text
                .iter()
                .any(|(text, _, _, _)| text.contains("límite de 8"))
        );
        assert!(r.document.text().contains("alt9"));
        assert_eq!(r.loaded_images, 0);
    }
    #[test]
    fn embedded_layout_resizes_with_caret_and_height_only_resize_clamps_scroll() {
        let mut r = rendered("antes\n\n![panorama](wide.png)\n\ndespués\n");
        r.set_image(0, Ok(bitmap(1000, 500, [12, 34, 56]))).unwrap();
        let mut p = Recorder::default();
        finish(&mut r, &mut p, Rect::new(10, 20, 800, 150));
        let initial = r.placements[0].bitmap.unwrap();
        assert_eq!((initial.w, initial.h), (640, 320));
        let selected_start = r.document.text().find("después").unwrap();
        r.document.select(selected_start, r.document.text().len());
        let selected = r.document.selected_text().to_owned();
        finish(&mut r, &mut p, Rect::new(10, 20, 320, 150));
        let narrow = r.placements[0].bitmap.unwrap();
        assert_eq!((narrow.w, narrow.h), (288, 144));
        r.reveal();
        let (x, y) = r.point_for_offset(&mut p, selected_start + 2);
        assert_eq!(r.offset_at(&mut p, x, y), selected_start + 2);
        assert_eq!(r.document.selected_text(), selected);
        r.scroll(1000);
        assert!(r.viewport.top > 0);
        // Width unchanged: cached layout must still clamp for taller viewport.
        r.paint(&mut p, Rect::new(10, 20, 320, 2000), &Palette::default());
        assert_eq!(r.viewport.top, 0);
        assert!(r.placements[0].bitmap.unwrap().w <= 640);
    }
    #[test]
    fn duplicate_resources_share_rgb_and_are_charged_once() {
        let mut r = rendered("![uno](a.png) ![dos](a.png) ![tres](a.png)");
        let shared = Arc::new(bitmap(700, 700, [40, 50, 60]));
        r.set_image_shared(0, Ok(shared.clone())).unwrap();
        let cost = r.image_bytes;
        r.set_image_shared(1, Ok(shared.clone())).unwrap();
        r.set_image_shared(2, Ok(shared.clone())).unwrap();
        assert_eq!(r.image_bytes, cost);
        assert_eq!(r.loaded_images, 3);
        assert_eq!(r.pending_images, 0);
        assert!(Arc::ptr_eq(
            r.images[0].bitmap.as_ref().unwrap(),
            r.images[2].bitmap.as_ref().unwrap()
        ));
        assert_eq!(
            r.images[0].bitmap.as_ref().unwrap().rgb.as_ptr(),
            shared.rgb.as_ptr()
        );
    }
}
