//! Space quick look: text, rendered Markdown and bounded bitmap previews.
use crate::{
    model::{Action, Hit, Model},
    painter::{FltkPainter, Painter, Rect},
    text_preview_state::View,
};
use fltk::{
    app,
    enums::{Event, Key, Shortcut},
};
impl Model {
    /// Only the existing visible ordering is consulted: no filesystem work or
    /// preload. At a boundary keep the current document and its worker intact.
    pub fn preview_neighbor(&mut self, delta: i32) {
        if !self.preview.visible() || !self.loaded || self.selected.is_none() || delta == 0 {
            return;
        }
        let previous = self.selected;
        self.move_selection(delta.signum());
        if self.selected != previous {
            self.open_preview();
        }
    }
    pub fn open_preview(&mut self) {
        self.thumbnails.suspend();
        if let Some(job) = &self.du_job {
            job.cancel();
        }
        self.sizes.cancel_all();
        self.du_requested = None;
        if self.mutation_applied || self.rename_dialog.is_some() {
            return;
        }
        if let Some(entry) = self.selected.and_then(|i| self.entries.get(i)) {
            if entry.is_dir() {
                self.preview.open_directory(self.path.join(&entry.name));
            } else {
                self.preview.open(self.path.join(&entry.name));
            }
            self.text_viewport = Default::default();
            self.settings = false;
            self.hits.clear();
        }
    }
    pub fn preview_pointer_down(&mut self, x: i32, y: i32, shift: bool) {
        self.preview_pointer_press(x, y, shift, 0);
    }
    /// `clicks` is FLTK's extra-click count: double click selects a word,
    /// triple click a line, in text, Markdown and HTML previews.
    pub fn preview_pointer_press(&mut self, x: i32, y: i32, shift: bool, clicks: i32) {
        if let View::Ready(ready) = self.preview.view()
            && let Some(audio) = &ready.audio
            && audio.timeline.contains(x, y)
        {
            let fraction = f64::from(x - audio.timeline.x) / f64::from(audio.timeline.w.max(1));
            self.preview
                .seek_audio_to(audio.info.duration * fraction.clamp(0.0, 1.0));
            return;
        }
        if let View::Video(ready) = self.preview.view()
            && ready.timeline.contains(x, y)
        {
            let fraction = f64::from(x - ready.timeline.x) / f64::from(ready.timeline.w.max(1));
            self.preview
                .seek_video_to(ready.frame.duration * fraction.clamp(0.0, 1.0));
            return;
        }
        if let Some(viewport) = self.preview.bitmap_viewport_mut()
            && viewport.body.contains(x, y)
        {
            viewport.drag = Some((x, y, viewport.pan_x, viewport.pan_y));
            return;
        }
        if let View::Markdown(ready) = self.preview.view_mut()
            && ready.rendered.viewport.body.contains(x, y)
        {
            let offset = ready.rendered.offset_at(&mut FltkPainter, x, y);
            ready.rendered.viewport.selection_anchor =
                ready.rendered.document.click(offset, shift, clicks);
            ready.rendered.viewport.dragging = true;
            return;
        }
        if self.text_viewport.body.contains(x, y)
            && let View::Html(ready) = self.preview.view_mut()
        {
            let offset = ready.rendered.offset_at(&mut FltkPainter, x, y);
            self.text_viewport.selection_anchor =
                ready.rendered.document.click(offset, shift, clicks);
            self.text_viewport.dragging = true;
            return;
        }
        if self.text_viewport.body.contains(x, y)
            && let Some(ready) = self.preview.ready_mut()
        {
            let offset = self
                .text_viewport
                .offset_at(&mut FltkPainter, &ready.document, x, y);
            self.text_viewport.selection_anchor = ready.document.click(offset, shift, clicks);
            self.text_viewport.dragging = true;
        }
    }
    pub fn preview_pointer_drag(&mut self, x: i32, y: i32) {
        if let Some(viewport) = self.preview.bitmap_viewport_mut()
            && let Some((start_x, start_y, pan_x, pan_y)) = viewport.drag
        {
            viewport.pan_x = pan_x + (start_x - x) as f32;
            viewport.pan_y = pan_y + (start_y - y) as f32;
            return;
        }
        if let View::Markdown(ready) = self.preview.view_mut()
            && ready.rendered.viewport.dragging
        {
            let offset = ready.rendered.offset_at(&mut FltkPainter, x, y);
            ready
                .rendered
                .document
                .select(ready.rendered.viewport.selection_anchor, offset);
            return;
        }
        if self.text_viewport.dragging
            && let View::Html(ready) = self.preview.view_mut()
        {
            let offset = ready.rendered.offset_at(&mut FltkPainter, x, y);
            ready
                .rendered
                .document
                .select(self.text_viewport.selection_anchor, offset);
            return;
        }
        if self.text_viewport.dragging
            && let Some(ready) = self.preview.ready_mut()
        {
            let offset = self
                .text_viewport
                .offset_at(&mut FltkPainter, &ready.document, x, y);
            ready
                .document
                .select(self.text_viewport.selection_anchor, offset);
        }
    }
    pub fn follow_preview_link(&mut self, x: i32, y: i32) -> bool {
        let target = match self.preview.view_mut() {
            View::Markdown(ready) => ready.rendered.link_at(&mut FltkPainter, x, y),
            _ => None,
        };
        if let Some(target) = target {
            self.preview.follow_link(target);
            true
        } else {
            false
        }
    }
    /// Copies the selected text. An empty selection never replaces the
    /// clipboard (it used to publish "" and wipe what the user had copied).
    pub fn copy_preview(&mut self) {
        let Some(document) = self.preview.document() else {
            return;
        };
        let text = document.selected_text();
        let notice = if text.is_empty() {
            "Nada seleccionado: arrastra, doble clic (palabra), triple clic (línea) o Ctrl+A"
                .to_string()
        } else {
            app::copy(text);
            let chars = text.chars().count();
            format!(
                "Copiado: {chars} {}",
                if chars == 1 {
                    "carácter"
                } else {
                    "caracteres"
                }
            )
        };
        eprintln!("FILES_PREVIEW_COPY bytes={} notice={notice}", text.len());
        self.text_viewport.copy_notice = Some(notice);
    }
}
fn inline_notice(text: &str) -> String {
    let mut shown = String::new();
    for c in text.chars() {
        let c = if c.is_control() || matches!(c,'\u{202a}'..='\u{202e}'|'\u{2066}'..='\u{2069}') {
            ' '
        } else {
            c
        };
        if shown.len() + c.len_utf8() > 512 {
            shown.push('…');
            break;
        }
        shown.push(c);
    }
    shown
}
fn button(p: &mut dyn Painter, m: &mut Model, r: Rect, label: &str, action: Action) {
    p.rounded_rect(r, 5 * m.radius, m.palette.selection);
    p.text(
        Rect::new(r.x + 8, r.y, r.w - 16, r.h),
        label,
        11,
        m.palette.accent,
        true,
        false,
    );
    m.hits.push(Hit {
        generation: m.generation,
        rect: r,
        action,
    });
}
pub fn paint(p: &mut dyn Painter, m: &mut Model, w: i32, h: i32) {
    if !m.preview.visible() {
        return;
    }
    m.hits.clear();
    let width = (w - 32).min(1160);
    let height = (h - 44).min(800);
    let x = (w - width) / 2;
    let y = (h - height) / 2;
    let c = m.palette.clone();
    p.rounded_rect(Rect::new(x, y, width, height), 10 * m.radius, c.muted);
    p.rounded_rect(
        Rect::new(x + 1, y + 1, width - 2, height - 2),
        10 * m.radius,
        c.darker_background,
    );
    let path = m
        .preview
        .path()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let source = if m.preview.is_directory() {
        "  Carpeta · contenido"
    } else if path.to_ascii_lowercase().ends_with(".zip") {
        "  ZIP · listado"
    } else if path.to_ascii_lowercase().ends_with(".docx") {
        "  DOCX · texto extraído"
    } else if path.to_ascii_lowercase().ends_with(".pdf") {
        if m.preview.pdf_text() {
            "  PDF · texto extraído"
        } else {
            "  PDF · página"
        }
    } else if [
        ".mp3", ".wav", ".wave", ".flac", ".ogg", ".oga", ".opus", ".m4a", ".aac", ".aif", ".aiff",
        ".aifc",
    ]
    .iter()
    .any(|e| path.to_ascii_lowercase().ends_with(e))
    {
        "  Audio · preescucha"
    } else if path.to_ascii_lowercase().ends_with(".gif") {
        "  GIF · animación bajo demanda"
    } else if [
        ".mp4", ".mkv", ".webm", ".mov", ".avi", ".m4v", ".mpg", ".mpeg", ".3gp", ".ts", ".mts",
        ".m2ts",
    ]
    .iter()
    .any(|ext| path.to_ascii_lowercase().ends_with(ext))
    {
        if m.preview.video_audio() {
            "  Vídeo · audio activado"
        } else {
            "  Vídeo · silenciado"
        }
    } else if matches!(m.preview.view(), View::Image(_))
        || [".png", ".jpg", ".jpeg", ".webp"]
            .iter()
            .any(|ext| path.to_ascii_lowercase().ends_with(ext))
    {
        "Vista previa · imagen"
    } else if path.to_ascii_lowercase().ends_with(".html")
        || path.to_ascii_lowercase().ends_with(".htm")
    {
        "HTML · render experimental"
    } else if [".md", ".markdown"]
        .iter()
        .any(|e| path.to_ascii_lowercase().ends_with(e))
    {
        "Markdown · documento"
    } else {
        "Vista previa · texto"
    };
    let bitmap_controls = matches!(
        m.preview.view(),
        View::Image(_) | View::Pdf(_) | View::Video(_)
    );
    p.text(
        Rect::new(
            x + 18,
            y + 10,
            width - if bitmap_controls { 342 } else { 250 },
            25,
        ),
        source,
        15,
        c.bright_foreground,
        true,
        false,
    );
    p.text(
        Rect::new(x + 18, y + 39, width - 36, 25),
        &path,
        11,
        c.dark_foreground,
        true,
        false,
    );
    let body = Rect::new(x + 18, y + 77, width - 36, height - 146);
    let revision = m.preview.revision();
    m.preview.configure_viewport(body.w, body.h);
    // Video decodes for the device pixels of the frame area (body minus timeline).
    m.preview
        .set_video_target(crate::image_preview_view::Viewport::device_box(
            body.w,
            (body.h - 40).max(1),
        ));
    if revision != m.preview.revision() {
        m.text_viewport.dragging = false;
    }
    let playing = m.preview.video_playing();
    let video_audio = m.preview.video_audio();
    let video_audio_failed = m.preview.video_audio_failed();
    let video_has_audio = matches!(m.preview.view(),View::Video(r)if r.frame.audio.is_some());
    let video_error = matches!(m.preview.view(),View::Video(r)if r.notice.is_some());
    let audio_available = m.preview.audio_info().is_some();
    let audio_playing = m.preview.audio_playing();
    let audio_running = m.preview.audio_running();
    let audio_position = m.preview.audio_position();
    let audio_notice = m.preview.audio_notice().map(str::to_owned);
    let is_pdf_text = m.preview.pdf_text();
    let mut footer = match m.preview.view_mut() {
        View::Ready(ready) | View::PdfText(ready) => {
            let text_body = if ready.audio.is_some() {
                Rect::new(body.x, body.y, body.w, (body.h - 40).max(1))
            } else {
                body
            };
            m.text_viewport.paint(p, &ready.document, text_body, &c);
            if path.to_ascii_lowercase().ends_with(".docx") && ready.document.text().is_empty() {
                p.text(
                    body,
                    "No hay texto compatible en el cuerpo del documento",
                    13,
                    c.foreground,
                    true,
                    false,
                );
            }
            if is_pdf_text && ready.document.text().is_empty() {
                p.text(
                    body,
                    "No hay texto extraíble en esta página",
                    13,
                    c.foreground,
                    true,
                    false,
                );
            }
            if let Some(audio) = &mut ready.audio {
                let track = Rect::new(body.x + 8, body.y + body.h - 26, (body.w - 16).max(1), 18);
                audio.timeline = track;
                p.rounded_rect(track, 3, c.selection);
                let fraction = (audio_position / audio.info.duration.max(0.001)).clamp(0.0, 1.0);
                if fraction > 0.0 {
                    p.rounded_rect(
                        Rect::new(
                            track.x,
                            track.y,
                            (f64::from(track.w) * fraction) as i32,
                            track.h,
                        ),
                        3,
                        c.accent,
                    );
                }
            }
            if let Some(audio) = &ready.audio {
                if let Some(notice) = &audio_notice {
                    format!("Audio: {notice}")
                } else {
                    format!(
                        "{} / {} · {}{}",
                        video_time(audio_position),
                        video_time(audio.info.duration),
                        if audio_running {
                            "reproduciendo · posición estimada"
                        } else if audio_playing {
                            "preparando audio"
                        } else {
                            "pausado"
                        },
                        if ready.truncated {
                            " · ETIQUETAS RECORTADAS"
                        } else {
                            ""
                        }
                    )
                }
            } else {
                format!(
                    "{} · {} líneas · {} bytes seleccionados{}",
                    ready.encoding,
                    ready.document.line_count(),
                    ready.document.selected_text().len(),
                    if ready.truncated && ready.encoding == "ZIP · directorio central" {
                        " · LISTADO TRUNCADO"
                    } else if ready.truncated && ready.encoding == "Audio · metadatos" {
                        " · ETIQUETAS RECORTADAS"
                    } else if ready.truncated {
                        " · TRUNCADO (límite256KiB/2000líneas)"
                    } else {
                        ""
                    }
                )
            }
        }
        View::Markdown(ready) => {
            ready.rendered.paint(p, body, &c);
            format!(
                "{} · {} bytes seleccionados · {} destinos bloqueados · {} imágenes / {} no disponibles{}{}{}",
                ready.encoding,
                ready.rendered.document.selected_text().len(),
                ready.rendered.blocked_targets,
                ready.rendered.loaded_images,
                ready.rendered.pending_images,
                if ready.truncated { " · TRUNCADO" } else { "" },
                ready
                    .notice
                    .as_ref()
                    .map(|s| format!(" · {s}"))
                    .unwrap_or_default(),
                ready
                    .rendered
                    .layout_error
                    .as_ref()
                    .map(|s| format!(" · {s}"))
                    .unwrap_or_default()
            )
        }
        View::Html(ready) => {
            m.text_viewport.body = body;
            ready.rendered.paint(p, body, &c);
            format!(
                "Métricas aproximadas · selección {} B · CSS bloqueado: {} · imágenes bloqueadas: {}",
                ready.rendered.document.selected_text().len(),
                ready.rendered.stats.blocked_css,
                ready.rendered.stats.blocked_images
            )
        }
        View::Pdf(ready) => {
            ready.viewport.paint(p, &ready.frame.bitmap, body, &c);
            format!(
                "Página {} de {} · zoom {:.0}%",
                ready.frame.page,
                ready.frame.pages,
                ready.viewport.zoom.max(1.0) * 100.0
            )
        }
        View::Video(ready) => {
            let frame_body = Rect::new(body.x, body.y, body.w, (body.h - 40).max(1));
            ready.viewport.paint(p, &ready.frame.bitmap, frame_body, &c);
            let track = Rect::new(body.x + 8, body.y + body.h - 26, (body.w - 16).max(1), 18);
            ready.timeline = track;
            p.rounded_rect(track, 3, c.selection);
            let position = (ready.frame.position / ready.frame.duration.max(0.001)).clamp(0.0, 1.0);
            if position > 0.0 {
                p.rounded_rect(
                    Rect::new(
                        track.x,
                        track.y,
                        (track.w as f64 * position) as i32,
                        track.h,
                    ),
                    3,
                    c.accent,
                );
            }
            if let Some(notice) = &ready.notice {
                format!("Vídeo pausado: {}", inline_notice(notice))
            } else {
                format!(
                    "{} / {} · {} · {}×{}{} · {} · zoom {:.0}%",
                    video_time(ready.frame.position),
                    video_time(ready.frame.duration),
                    ready.frame.codec,
                    ready.frame.bitmap.source_width,
                    ready.frame.bitmap.source_height,
                    if playing && ready.sequence.is_none() {
                        " · preparando reproducción"
                    } else if playing {
                        " · reproduciendo"
                    } else {
                        " · pausado"
                    },
                    if ready.frame.audio.is_some() {
                        if video_audio_failed {
                            "sin audio (salida no disponible)"
                        } else if video_audio {
                            "audio activado"
                        } else {
                            "silenciado"
                        }
                    } else {
                        ready
                            .frame
                            .audio_notice
                            .as_deref()
                            .unwrap_or("sin pista de audio")
                    },
                    ready.viewport.zoom.max(1.) * 100.
                )
            }
        }
        View::Image(ready) => {
            ready.viewport.paint(p, &ready.bitmap, body, &c);
            format!(
                "{} · original {}×{} · bitmap {}×{} · zoom{:.0}% · {}",
                ready.bitmap.format,
                ready.bitmap.source_width,
                ready.bitmap.source_height,
                ready.bitmap.width,
                ready.bitmap.height,
                ready.viewport.zoom.max(1.0) * 100.0,
                if u64::from(ready.bitmap.width) * u64::from(ready.bitmap.height)
                    < u64::from(ready.bitmap.source_width) * u64::from(ready.bitmap.source_height)
                {
                    "Vista reducida"
                } else {
                    "Resolución original"
                }
            )
        }
        View::Loading => {
            p.text(
                body,
                if m.preview.is_directory() {
                    "Calculando contenido de la carpeta…"
                } else {
                    "Leyendo vista previa…"
                },
                13,
                c.foreground,
                true,
                false,
            );
            "Esc cancela y recoge el helper".into()
        }
        View::Error(error) => {
            p.text(body, error, 13, c.red, true, false);
            "No se pudo mostrar este archivo".into()
        }
        View::Closed => return,
    };
    if let Some(notice) = &m.text_viewport.copy_notice {
        footer = format!("{notice} · {footer}");
    }
    if let Some(viewport) = m.preview.bitmap_viewport_mut()
        && viewport.quarter_turns() != 0
    {
        footer.push_str(&format!(
            " · giro {}°",
            u16::from(viewport.quarter_turns()) * 90
        ));
    }
    p.text(
        Rect::new(x + 18, y + height - 62, width - 36, 24),
        &footer,
        11,
        if audio_notice.is_some() || video_error {
            c.red
        } else {
            c.dark_foreground
        },
        true,
        false,
    );
    p.text(
        Rect::new(
            x + 18,
            y + height - 34,
            width
                - if video_has_audio {
                    490
                } else if audio_available {
                    380
                } else if m.preview.pdf_available() {
                    390
                } else {
                    300
                },
            25,
        ),
        if matches!(m.preview.view(), View::Error(_) | View::Loading) {
            "Esc cierra la vista"
        } else if audio_available {
            "P reproduce/pausa · clic en barra busca"
        } else if matches!(m.preview.view(), View::PdfText(_)) {
            "Shift+flechas selecciona · Ctrl+A/C · T página · Ctrl+PgUp/PgDn"
        } else if matches!(m.preview.view(), View::Pdf(_)) {
            "T texto · ←/→ página · Ctrl+rueda o +/- zoom · arrastra · 0 ajusta"
        } else if matches!(m.preview.view(), View::Video(_)) {
            if video_has_audio {
                "Ctrl+rueda zoom · arrastra · 0 ajusta · P pausa · M sonido"
            } else {
                "Ctrl+rueda / +/- zoom · arrastra · 0 ajusta · ←/→ 5 s · P pausa"
            }
        } else if matches!(m.preview.view(), View::Image(_)) {
            "Ctrl+rueda / +/- zoom · arrastra o flechas · 0 ajusta"
        } else if matches!(m.preview.view(), View::Markdown(_)) {
            "Ctrl+clic: enlace local · Shift+flechas: selección · Ctrl+A/C · rueda desplaza"
        } else {
            "Shift+flechas selecciona · Ctrl+A/C · Alt+↑/↓ archivo · Shift+rueda horizontal"
        },
        10,
        c.dark_foreground,
        true,
        false,
    );
    if bitmap_controls {
        for (offset, label, delta) in [(312, "", -1), (266, "", 1)] {
            button(
                p,
                m,
                Rect::new(x + width - offset, y + 8, 38, 29),
                label,
                Action::PreviewRotate(delta),
            );
        }
    }
    for (offset, label, delta) in [(220, "", -1), (174, "", 1)] {
        let action = if m.loaded {
            Action::PreviewNeighbor(delta)
        } else {
            Action::None
        };
        button(
            p,
            m,
            Rect::new(x + width - offset, y + 8, 38, 29),
            label,
            action,
        );
    }
    button(
        p,
        m,
        Rect::new(x + width - 128, y + 8, 110, 29),
        "Cerrar · Esc",
        Action::ClosePreview,
    );
    if audio_available {
        button(
            p,
            m,
            Rect::new(x + width - 330, y + height - 34, 132, 25),
            if audio_playing {
                "  Pausar"
            } else {
                "  Reproducir"
            },
            Action::PreviewAudioToggle,
        );
        button(
            p,
            m,
            Rect::new(x + width - 190, y + height - 34, 80, 25),
            "−5 s",
            Action::PreviewAudioSeek(-5.0),
        );
        button(
            p,
            m,
            Rect::new(x + width - 100, y + height - 34, 82, 25),
            "+5 s",
            Action::PreviewAudioSeek(5.0),
        );
    } else if m.preview.pdf_available() {
        button(
            p,
            m,
            Rect::new(x + width - 366, y + height - 34, 110, 25),
            if is_pdf_text {
                "  Página · T"
            } else {
                "  Texto · T"
            },
            Action::PreviewPdfText,
        );
        button(
            p,
            m,
            Rect::new(x + width - 248, y + height - 34, 110, 25),
            "Anterior",
            Action::PreviewPdfPage(-1),
        );
        button(
            p,
            m,
            Rect::new(x + width - 130, y + height - 34, 112, 25),
            "Siguiente",
            Action::PreviewPdfPage(1),
        );
    } else if matches!(m.preview.view(), View::Video(_)) {
        if video_has_audio {
            button(
                p,
                m,
                Rect::new(x + width - 450, y + height - 34, 100, 25),
                if video_audio_failed {
                    "  Sin audio"
                } else if video_audio {
                    "  Audio"
                } else {
                    "  Silencio"
                },
                Action::PreviewVideoAudio,
            );
            button(
                p,
                m,
                Rect::new(x + width - 342, y + height - 34, 132, 25),
                if playing {
                    "  Pausar"
                } else {
                    "  Reproducir"
                },
                Action::PreviewVideoToggle,
            );
            button(
                p,
                m,
                Rect::new(x + width - 202, y + height - 34, 80, 25),
                "−5 s",
                Action::PreviewVideoSeek(-5.0),
            );
            button(
                p,
                m,
                Rect::new(x + width - 114, y + height - 34, 96, 25),
                "+5 s",
                Action::PreviewVideoSeek(5.0),
            );
        } else {
            button(
                p,
                m,
                Rect::new(x + width - 366, y + height - 34, 110, 25),
                if playing {
                    "  Pausar"
                } else {
                    "  Reproducir"
                },
                Action::PreviewVideoToggle,
            );
            button(
                p,
                m,
                Rect::new(x + width - 248, y + height - 34, 110, 25),
                "−5 s",
                Action::PreviewVideoSeek(-5.0),
            );
            button(
                p,
                m,
                Rect::new(x + width - 130, y + height - 34, 112, 25),
                "+5 s",
                Action::PreviewVideoSeek(5.0),
            );
        }
    } else if matches!(m.preview.view(), View::Image(_)) {
        button(
            p,
            m,
            Rect::new(x + width - 260, y + height - 34, 76, 25),
            "−",
            Action::PreviewImageZoom(1.0 / 1.2),
        );
        button(
            p,
            m,
            Rect::new(x + width - 176, y + height - 34, 76, 25),
            "Ajustar",
            Action::PreviewImageReset,
        );
        button(
            p,
            m,
            Rect::new(x + width - 92, y + height - 34, 74, 25),
            "+",
            Action::PreviewImageZoom(1.2),
        );
    } else if m.preview.document().is_some() {
        button(
            p,
            m,
            Rect::new(x + width - 290, y + height - 34, 124, 25),
            "Todo · Ctrl+A",
            Action::SelectPreview,
        );
        button(
            p,
            m,
            Rect::new(x + width - 158, y + height - 34, 140, 25),
            "Copiar · Ctrl+C",
            Action::CopyPreview,
        );
    }
}
pub fn event(m: &mut Model, ev: Event) -> bool {
    if matches!(ev, Event::Push | Event::KeyDown | Event::Shortcut) {
        m.text_viewport.copy_notice = None;
    }
    let ctrl = app::event_state().intersects(Shortcut::Ctrl | Shortcut::Meta);
    let shift = app::event_state().intersects(Shortcut::Shift);
    match ev {
        Event::Push => {
            let x = app::event_x();
            let y = app::event_y();
            if let Some(action) = m
                .hits
                .iter()
                .rev()
                .find(|h| h.generation == m.generation && h.rect.contains(x, y))
                .map(|h| h.action.clone())
            {
                m.action(action);
            } else if !(ctrl && m.follow_preview_link(x, y)) {
                m.preview_pointer_press(x, y, shift, app::event_clicks_num());
            }
        }
        Event::Drag => m.preview_pointer_drag(app::event_x(), app::event_y()),
        Event::Released => {
            m.text_viewport.dragging = false;
            if let Some(viewport) = m.preview.bitmap_viewport_mut() {
                viewport.drag = None;
            }
            if let View::Markdown(ready) = m.preview.view_mut() {
                ready.rendered.viewport.dragging = false;
            }
        }
        Event::MouseWheel => {
            if let Some(viewport) = m.preview.bitmap_viewport_mut() {
                if ctrl {
                    viewport.zoom_by(if app::event_dy_value() < 0 {
                        1.2
                    } else {
                        1.0 / 1.2
                    });
                } else if shift {
                    viewport.pan_x += app::event_dy_value() as f32 * 40.0;
                } else {
                    viewport.pan_y += app::event_dy_value() as f32 * 40.0;
                }
            }
            if let View::Markdown(ready) = m.preview.view_mut() {
                ready.rendered.scroll(app::event_dy_value());
            }
            if let View::Html(ready) = m.preview.view_mut() {
                ready.rendered.scroll(app::event_dy_value());
            }
            if let Some(ready) = m.preview.ready_mut() {
                if shift {
                    m.text_viewport.horizontal = (m.text_viewport.horizontal
                        + app::event_dy_value() as f32 * 28.0)
                        .clamp(0.0, 100_000.0);
                } else {
                    m.text_viewport
                        .scroll(app::event_dy_value(), &ready.document);
                }
            }
        }
        Event::KeyDown | Event::Shortcut => {
            let key = app::event_key();
            if key == Key::Escape || key == Key::from_char(' ') {
                m.action(Action::ClosePreview);
            } else if ctrl && key == Key::from_char('c') {
                m.copy_preview();
            } else if !ctrl
                && key == Key::from_char('r')
                && m.preview.bitmap_viewport_mut().is_some()
            {
                m.action(Action::PreviewRotate(if shift { -1 } else { 1 }));
            } else if app::event_state().intersects(Shortcut::Alt)
                && (key == Key::Up || key == Key::Down)
            {
                m.action(Action::PreviewNeighbor(if key == Key::Down {
                    1
                } else {
                    -1
                }));
            } else if !ctrl && key == Key::from_char('p') && m.preview.audio_info().is_some() {
                m.preview.toggle_audio();
            } else if !ctrl && key == Key::from_char('t') && m.preview.pdf_available() {
                m.action(Action::PreviewPdfText);
            } else if ctrl
                && (key == Key::PageUp || key == Key::PageDown)
                && m.preview.pdf_available()
            {
                m.action(Action::PreviewPdfPage(if key == Key::PageUp {
                    -1
                } else {
                    1
                }));
            } else if matches!(m.preview.view(), View::Pdf(_)) {
                if key == Key::Left || key == Key::PageUp {
                    m.preview.change_pdf_page(-1);
                } else if key == Key::Right || key == Key::PageDown {
                    m.preview.change_pdf_page(1);
                } else if key == Key::Home {
                    m.preview.change_pdf_page(i32::MIN);
                } else if key == Key::End {
                    m.preview.change_pdf_page(i32::MAX);
                } else if let Some(viewport) = m.preview.bitmap_viewport_mut() {
                    if key == Key::Up {
                        viewport.pan_y -= 40.0;
                    }
                    if key == Key::Down {
                        viewport.pan_y += 40.0;
                    }
                    if key == Key::from_char('+') || key == Key::from_char('=') {
                        viewport.zoom_by(1.2);
                    }
                    if key == Key::from_char('-') {
                        viewport.zoom_by(1.0 / 1.2);
                    }
                    if key == Key::from_char('0') {
                        viewport.reset();
                    }
                }
            } else if matches!(m.preview.view(), View::Video(_)) {
                if key == Key::from_char('p') {
                    m.preview.toggle_video();
                }
                if !ctrl && key == Key::from_char('m') {
                    m.action(Action::PreviewVideoAudio);
                }
                if key == Key::Left {
                    m.preview.seek_video_by(-5.0);
                }
                if key == Key::Right {
                    m.preview.seek_video_by(5.0);
                }
                if key == Key::Home {
                    m.preview.seek_video_to(0.0);
                }
                if key == Key::End {
                    m.preview.seek_video_to(f64::MAX);
                }
                if let Some(viewport) = m.preview.bitmap_viewport_mut() {
                    if key == Key::from_char('+') || key == Key::from_char('=') {
                        viewport.zoom_by(1.2);
                    }
                    if key == Key::from_char('-') {
                        viewport.zoom_by(1. / 1.2);
                    }
                    if key == Key::from_char('0') {
                        viewport.reset();
                    }
                    if key == Key::Up {
                        viewport.pan_y -= 40.;
                    }
                    if key == Key::Down {
                        viewport.pan_y += 40.;
                    }
                }
            } else if let View::Image(ready) = m.preview.view_mut() {
                match key {
                    Key::Up => ready.viewport.pan_y -= 40.0,
                    Key::Down => ready.viewport.pan_y += 40.0,
                    Key::Left => ready.viewport.pan_x -= 40.0,
                    Key::Right => ready.viewport.pan_x += 40.0,
                    _ => {
                        if key == Key::from_char('+') || key == Key::from_char('=') {
                            ready.viewport.zoom_by(1.2);
                        }
                        if key == Key::from_char('-') {
                            ready.viewport.zoom_by(1.0 / 1.2);
                        }
                        if key == Key::from_char('0') {
                            ready.viewport.reset();
                        }
                    }
                }
            } else if let Some(doc) = m.preview.document_mut() {
                if ctrl && key == Key::from_char('a') {
                    doc.select_all();
                } else if key == Key::Left || key == Key::Right {
                    doc.move_horizontal(key == Key::Right, shift);
                } else {
                    let (line, col) = doc.position(doc.caret());
                    let target = if ctrl && key == Key::Home {
                        Some(0)
                    } else if ctrl && key == Key::End {
                        Some(doc.text().len())
                    } else if key == Key::Home {
                        Some(doc.offset(line, 0))
                    } else if key == Key::End {
                        Some(doc.offset(line, usize::MAX))
                    } else if key == Key::Up {
                        Some(doc.offset(line.saturating_sub(1), col))
                    } else if key == Key::Down {
                        Some(doc.offset(line + 1, col))
                    } else if key == Key::PageUp {
                        Some(doc.offset(line.saturating_sub(20), col))
                    } else if key == Key::PageDown {
                        Some(doc.offset(line + 20, col))
                    } else {
                        None
                    };
                    if let Some(target) = target {
                        doc.move_to(target, shift);
                    }
                }
                match m.preview.view_mut() {
                    View::Ready(ready) | View::PdfText(ready) => {
                        m.text_viewport.reveal(&ready.document)
                    }
                    View::Markdown(ready) => ready.rendered.reveal(),
                    View::Html(ready) => ready.rendered.reveal(),
                    _ => {}
                }
            }
        }
        _ => {}
    }
    true
}

fn video_time(seconds: f64) -> String {
    let seconds = seconds.max(0.0) as u64;
    format!(
        "{}:{:02}:{:02}",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60
    )
}
