//! Single-flight preview lifecycle. A cancelled helper must acknowledge/reap
//! before the latest requested file can start; stale results never reach the UI.
use crate::image_preview_job::{Bitmap, Job as ImageJob};
use crate::{
    text_document::TextDocument,
    text_preview_job::{Job, TextPreview},
};
use std::path::{Path, PathBuf};
pub fn thumbnail_source(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        matches!(
            e.to_ascii_lowercase().as_str(),
            "png"
                | "jpg"
                | "jpeg"
                | "webp"
                | "gif"
                | "mp4"
                | "mkv"
                | "webm"
                | "mov"
                | "avi"
                | "m4v"
                | "mpg"
                | "mpeg"
                | "3gp"
                | "ts"
                | "mts"
                | "m2ts"
        )
    })
}
pub fn audio_source(path: &Path) -> bool {
    path.extension().and_then(|s| s.to_str()).is_some_and(|e| {
        matches!(
            e.to_ascii_lowercase().as_str(),
            "mp3"
                | "wav"
                | "wave"
                | "flac"
                | "ogg"
                | "oga"
                | "opus"
                | "m4a"
                | "aac"
                | "aif"
                | "aiff"
                | "aifc"
        )
    })
}
pub fn source_required(path: &Path) -> bool {
    thumbnail_source(path)
        || audio_source(path)
        || path
            .extension()
            .is_some_and(|e| matches!(e.to_str(), Some(s) if s.eq_ignore_ascii_case("pdf") || s.eq_ignore_ascii_case("html") || s.eq_ignore_ascii_case("htm") || s.eq_ignore_ascii_case("docx")))
}
pub enum Payload {
    Text(TextPreview),
    Office(crate::office_preview_job::Ready),
    Audio(crate::audio_preview_job::Ready),
    AudioPlaybackEnd(crate::audio_preview_job::PlaybackEnd),
    Markdown(Box<crate::markdown_preview_job::Ready>),
    Html(Box<crate::html_preview_job::Ready>),
    Image(Bitmap),
    Video(crate::video_preview_job::Ready),
    Pdf(crate::pdf_preview_job::Ready),
    PdfText(crate::pdf_preview_job::TextReady),
    PlaybackFinished,
    SourceUnavailable(String),
}

trait Task {
    fn source_invalid(&self) -> bool {
        false
    }
    fn started(&self) -> Option<std::time::Instant> {
        None
    }
    fn exit_allowed(&self) -> bool {
        false
    }
    fn readonly_pending(&self) -> bool {
        false
    }
    fn recycle_rgb(&self, _rgb: Vec<u8>) {}
    fn poll_frame(&self) -> Option<crate::video_stream_job::Frame> {
        None
    }
    fn cancel(&self);
    fn poll(&self) -> Option<Result<Payload, String>>;
}
impl Task for Job {
    fn cancel(&self) {
        Job::cancel(self);
    }
    fn poll(&self) -> Option<Result<Payload, String>> {
        Job::poll(self).map(|r| r.map(Payload::Text))
    }
}
impl Task for crate::office_preview_job::Job {
    fn cancel(&self) {
        crate::office_preview_job::Job::cancel(self);
    }
    fn poll(&self) -> Option<Result<Payload, String>> {
        crate::office_preview_job::Job::poll(self).map(|r| r.map(Payload::Office))
    }
}
impl Task for crate::archive_preview_job::Job {
    fn cancel(&self) {
        crate::archive_preview_job::Job::cancel(self);
    }
    fn poll(&self) -> Option<Result<Payload, String>> {
        crate::archive_preview_job::Job::poll(self).map(|r| r.map(Payload::Text))
    }
}
impl Task for crate::audio_preview_job::PlaybackJob {
    fn cancel(&self) {
        crate::audio_preview_job::PlaybackJob::cancel(self);
    }
    fn exit_allowed(&self) -> bool {
        crate::audio_preview_job::PlaybackJob::exit_allowed(self)
    }
    fn readonly_pending(&self) -> bool {
        crate::audio_preview_job::PlaybackJob::readonly_pending(self)
    }
    fn started(&self) -> Option<std::time::Instant> {
        crate::audio_preview_job::PlaybackJob::started(self)
    }
    fn poll(&self) -> Option<Result<Payload, String>> {
        crate::audio_preview_job::PlaybackJob::poll(self).map(|r| r.map(Payload::AudioPlaybackEnd))
    }
}
impl Task for crate::audio_preview_job::Job {
    fn exit_allowed(&self) -> bool {
        crate::audio_preview_job::Job::exit_allowed(self)
    }
    fn readonly_pending(&self) -> bool {
        crate::audio_preview_job::Job::readonly_pending(self)
    }
    fn cancel(&self) {
        crate::audio_preview_job::Job::cancel(self);
    }
    fn poll(&self) -> Option<Result<Payload, String>> {
        crate::audio_preview_job::Job::poll(self).map(|r| r.map(Payload::Audio))
    }
}
fn folder_metadata((bytes, entries): (u64, u64)) -> TextPreview {
    TextPreview {
        text: format!(
            "Carpeta · resumen recursivo\n\nTamaño aparente: {} ({} bytes)\nEntradas descendientes: {}\n\nNo sigue enlaces simbólicos.\nHardlinks contados por nombre, no por bloques físicos.\nEl resultado no es un snapshot frente a cambios concurrentes.\n",
            files_core::format_size(bytes),
            bytes,
            entries
        ),
        encoding: "Carpeta · tamaño aparente".into(),
        truncated: false,
        relative_source: None,
    }
}
impl Task for crate::du_job::Job {
    fn cancel(&self) {
        crate::du_job::Job::cancel(self);
    }
    fn poll(&self) -> Option<Result<Payload, String>> {
        crate::du_job::Job::poll(self).map(|r| r.map(|usage| Payload::Text(folder_metadata(usage))))
    }
}
impl Task for ImageJob {
    fn cancel(&self) {
        ImageJob::cancel(self);
    }
    fn poll(&self) -> Option<Result<Payload, String>> {
        ImageJob::poll(self).map(|r| r.map(Payload::Image))
    }
}
impl Task for crate::markdown_preview_job::Job {
    fn cancel(&self) {
        crate::markdown_preview_job::Job::cancel(self);
    }
    fn poll(&self) -> Option<Result<Payload, String>> {
        crate::markdown_preview_job::Job::poll(self)
            .map(|r| r.map(|ready| Payload::Markdown(Box::new(ready))))
    }
}
impl Task for crate::html_preview_job::Job {
    fn source_invalid(&self) -> bool {
        crate::html_preview_job::Job::source_invalid(self)
    }
    fn exit_allowed(&self) -> bool {
        crate::html_preview_job::Job::exit_allowed(self)
    }
    fn readonly_pending(&self) -> bool {
        crate::html_preview_job::Job::readonly_pending(self)
    }
    fn cancel(&self) {
        crate::html_preview_job::Job::cancel(self);
    }
    fn poll(&self) -> Option<Result<Payload, String>> {
        crate::html_preview_job::Job::poll(self).map(|r| match r {
            Ok(ready) => Ok(Payload::Html(Box::new(ready))),
            Err(e) if self.source_invalid() => Ok(Payload::SourceUnavailable(e)),
            Err(e) => Err(e),
        })
    }
}
impl Task for crate::video_preview_job::Job {
    fn exit_allowed(&self) -> bool {
        crate::video_preview_job::Job::exit_allowed(self)
    }
    fn readonly_pending(&self) -> bool {
        crate::video_preview_job::Job::readonly_pending(self)
    }
    fn cancel(&self) {
        crate::video_preview_job::Job::cancel(self);
    }
    fn poll(&self) -> Option<Result<Payload, String>> {
        crate::video_preview_job::Job::poll(self).map(|r| r.map(Payload::Video))
    }
}
impl Task for crate::pdf_preview_job::Job {
    fn source_invalid(&self) -> bool {
        crate::pdf_preview_job::Job::source_invalid(self)
    }
    fn exit_allowed(&self) -> bool {
        crate::pdf_preview_job::Job::exit_allowed(self)
    }
    fn readonly_pending(&self) -> bool {
        crate::pdf_preview_job::Job::readonly_pending(self)
    }
    fn cancel(&self) {
        crate::pdf_preview_job::Job::cancel(self);
    }
    fn poll(&self) -> Option<Result<Payload, String>> {
        crate::pdf_preview_job::Job::poll(self).map(|r| match r {
            Ok(crate::pdf_preview_job::Output::Raster(ready)) => Ok(Payload::Pdf(ready)),
            Ok(crate::pdf_preview_job::Output::Text(ready)) => Ok(Payload::PdfText(ready)),
            Err(e) if self.source_invalid() => Ok(Payload::SourceUnavailable(e)),
            Err(e) => Err(e),
        })
    }
}
impl Task for crate::video_stream_job::Job {
    fn exit_allowed(&self) -> bool {
        crate::video_stream_job::Job::exit_allowed(self)
    }
    fn readonly_pending(&self) -> bool {
        crate::video_stream_job::Job::readonly_pending(self)
    }
    fn source_invalid(&self) -> bool {
        crate::video_stream_job::Job::source_invalid(self)
    }
    fn recycle_rgb(&self, rgb: Vec<u8>) {
        crate::video_stream_job::Job::recycle_rgb(self, rgb);
    }
    fn cancel(&self) {
        crate::video_stream_job::Job::cancel(self);
    }
    fn poll_frame(&self) -> Option<crate::video_stream_job::Frame> {
        crate::video_stream_job::Job::poll_frame(self)
    }
    fn poll(&self) -> Option<Result<Payload, String>> {
        crate::video_stream_job::Job::poll_finished(self).map(|r| match r {
            Ok(()) => Ok(Payload::PlaybackFinished),
            Err(e) if self.source_invalid() => Ok(Payload::SourceUnavailable(e)),
            Err(e) => Err(e),
        })
    }
}
pub struct PdfReady {
    pub frame: crate::pdf_preview_job::Ready,
    pub viewport: crate::image_preview_view::Viewport,
}
pub struct VideoReady {
    pub notice: Option<String>,
    pub frame: crate::video_preview_job::Ready,
    pub viewport: crate::image_preview_view::Viewport,
    pub timeline: crate::painter::Rect,
    pub sequence: Option<u64>,
}
pub struct ImageReady {
    pub bitmap: Bitmap,
    pub viewport: crate::image_preview_view::Viewport,
}
struct Active {
    revision: u64,
    task: Box<dyn Task>,
}

pub struct AudioReady {
    pub info: crate::audio_preview_job::Info,
    pub timeline: crate::painter::Rect,
}
pub struct Ready {
    pub audio: Option<AudioReady>,
    pub source: Option<files_core::media_source::Revision>,
    pub document: TextDocument,
    pub encoding: String,
    pub truncated: bool,
}
pub enum View {
    Closed,
    Loading,
    Ready(Ready),
    Markdown(Box<crate::markdown_preview_job::Ready>),
    Html(Box<crate::html_preview_job::Ready>),
    Image(ImageReady),
    Video(VideoReady),
    Pdf(PdfReady),
    PdfText(Ready),
    Error(String),
}
/// Own one per window. Call pump while visible AND while closing; the window may
/// exit only after close() and is_drained(). This does not block the UI thread.
pub struct State {
    revision: u64,
    path: Option<PathBuf>,
    needs_start: bool,
    active: Option<Active>,
    view: View,
    root: Option<PathBuf>,
    relative: Option<PathBuf>,
    fragment: Option<String>,
    html_size: (i32, i32),
    audio_position: f64,
    audio_origin: f64,
    audio_playing: bool,
    audio_notice: Option<String>,
    /// Global "mute" for media previews: survives closing and opening other
    /// files (and is saved by the model). Sound is on by default.
    muted: bool,
    /// The audio output failed for this preview; it plays silently instead.
    audio_failed: bool,
    /// Videos start playing when they open (fixtures may turn it off).
    autoplay: bool,
    autoplay_pending: bool,
    /// A playback start was requested while consuming a result: start it on
    /// the next pump, whose options are computed from the new view.
    defer_start: bool,
    /// Device-pixel box the open preview shows video in (decoder target).
    video_target: (u32, u32),
    video_position: f64,
    video_playing: bool,
    video_duration: Option<f64>,
    video_transform: crate::image_preview_view::Transform,
    pdf_text: bool,
    pdf_page: u32,
    pdf_pages: Option<u32>,
    pdf_transform: crate::image_preview_view::Transform,
    directory: bool,
    source_generation: u64,
    expected_source: Option<files_core::media_source::Revision>,
    source_blocked: bool,
    pending_markdown: Option<Box<crate::markdown_preview_job::Ready>>,
    document_suspended: bool,
    document_waiting_for_watch: bool,
    stale_document_plan: Option<std::sync::Arc<crate::document_source::Plan>>,
}
impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}
impl State {
    pub fn new() -> Self {
        Self {
            revision: 0,
            path: None,
            needs_start: false,
            active: None,
            view: View::Closed,
            root: None,
            relative: None,
            fragment: None,
            html_size: (932, 460),
            audio_position: 0.0,
            audio_origin: 0.0,
            audio_playing: false,
            audio_notice: None,
            muted: false,
            audio_failed: false,
            autoplay: true,
            autoplay_pending: false,
            defer_start: false,
            video_target: (1920, 1080),
            video_position: 0.0,
            video_playing: false,
            video_duration: None,
            video_transform: Default::default(),
            pdf_text: false,
            pdf_page: 1,
            pdf_pages: None,
            pdf_transform: Default::default(),
            directory: false,
            source_generation: 0,
            expected_source: None,
            source_blocked: false,
            pending_markdown: None,
            document_suspended: false,
            document_waiting_for_watch: false,
            stale_document_plan: None,
        }
    }
    pub fn open(&mut self, path: PathBuf) {
        self.close();
        if path.as_os_str().len() > 4096 {
            self.view = View::Error("Ruta demasiado larga para la vista previa".into());
            return;
        }
        self.root = path.parent().map(Path::to_path_buf);
        self.source_blocked = source_required(&path);
        self.path = Some(path);
        self.needs_start = true;
        self.autoplay_pending = true;
        self.view = View::Loading;
    }
    pub fn open_directory(&mut self, path: PathBuf) {
        self.open(path);
        self.directory = self.path.is_some();
        self.source_blocked = false;
    }
    pub fn is_directory(&self) -> bool {
        self.directory
    }
    /// Immediately hides/releases the displayed text and discards queued intent.
    /// An existing worker remains owned until its final result is consumed.
    pub fn close(&mut self) {
        self.pending_markdown = None;
        self.stale_document_plan = None;
        self.document_suspended = false;
        self.document_waiting_for_watch = false;
        self.source_generation = self
            .source_generation
            .checked_add(1)
            .expect("media generation exhausted");
        self.expected_source = None;
        self.source_blocked = false;
        self.revision = self
            .revision
            .checked_add(1)
            .expect("preview revision exhausted");
        self.path = None;
        self.root = None;
        self.relative = None;
        self.fragment = None;
        self.audio_position = 0.0;
        self.audio_origin = 0.0;
        self.audio_playing = false;
        self.audio_notice = None;
        self.audio_failed = false;
        self.autoplay_pending = false;
        self.defer_start = false;
        self.video_position = 0.0;
        self.video_playing = false;
        self.video_duration = None;
        self.video_transform = Default::default();
        self.pdf_text = false;
        self.pdf_page = 1;
        self.pdf_pages = None;
        self.pdf_transform = Default::default();
        self.directory = false;
        self.needs_start = false;
        self.view = View::Closed;
        if let Some(active) = &self.active {
            active.task.cancel();
        }
    }
    pub fn suspend_media(&mut self) {
        if self.is_markdown_path() {
            self.invalidate_document();
            self.document_suspended = true;
            return;
        }
        if self.source_target().is_none() {
            return;
        }
        let position = self.video_position;
        self.invalidate_source();
        self.video_position = position;
        self.source_generation = self
            .source_generation
            .checked_add(1)
            .expect("media generation exhausted");
    }
    fn is_markdown_path(&self) -> bool {
        self.path
            .as_deref()
            .and_then(Path::extension)
            .and_then(|e| e.to_str())
            .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "md" | "markdown"))
    }
    pub fn resume_media(&mut self) {
        if self.document_suspended {
            self.document_suspended = false;
            self.needs_start = self.path.is_some();
        }
    }
    pub fn document_plan(
        &self,
    ) -> Option<(u64, &Path, std::sync::Arc<crate::document_source::Plan>)> {
        let plan = self
            .pending_markdown
            .as_deref()
            .and_then(|r| r.plan.as_ref())
            .or(match &self.view {
                View::Markdown(r) => r.plan.as_ref(),
                _ => None,
            })
            .or(self.stale_document_plan.as_ref())?;
        Some((self.source_generation, self.path.as_deref()?, plan.clone()))
    }
    pub fn invalidate_document(&mut self) -> bool {
        if !self.is_markdown_path() {
            return false;
        }
        if let Some((_, _, plan)) = self.document_plan() {
            self.stale_document_plan = Some(plan);
        }
        self.source_generation = self
            .source_generation
            .checked_add(1)
            .expect("document generation exhausted");
        self.revision = self
            .revision
            .checked_add(1)
            .expect("preview revision exhausted");
        self.pending_markdown = None;
        self.document_waiting_for_watch = false;
        self.view = View::Loading;
        self.needs_start = self.path.is_some();
        if let Some(active) = &self.active {
            active.task.cancel();
        }
        true
    }
    pub fn document_watch_unavailable(&mut self, message: String) -> bool {
        if self.document_waiting_for_watch && matches!(&self.view,View::Error(old) if old==&message)
        {
            return false;
        }
        self.invalidate_document();
        self.document_waiting_for_watch = true;
        self.needs_start = false;
        self.view = View::Error(message);
        true
    }
    pub fn document_unavailable(&mut self, message: String) -> bool {
        if self.pending_markdown.is_some() || matches!(self.view, View::Markdown(_)) {
            return self.invalidate_document();
        }
        // A stale manifest may keep failing while its replacement loads. Do not
        // cancel that fresh worker or create a retry loop. Metadata changes or
        // an actual directory event trigger the next attempt after a load error.
        if self.active.is_some() || self.needs_start {
            return false;
        }
        if matches!(&self.view,View::Error(old) if old==&message) {
            return false;
        }
        self.view = View::Error(message);
        true
    }
    pub fn verify_document(&mut self, source: files_core::media_source::Revision) -> bool {
        let Some(mut ready) = self.pending_markdown.take() else {
            if self.document_waiting_for_watch {
                self.document_waiting_for_watch = false;
                self.needs_start = self.path.is_some();
                self.view = View::Loading;
                return true;
            }
            return false;
        };
        if ready.source.is_none_or(|p| p.file != source) {
            self.invalidate_document();
            return true;
        }
        if let Some(fragment) = self.fragment.take()
            && !ready.rendered.jump_to_fragment(&fragment)
        {
            ready.notice = Some("No se encontró ese título".into());
        }
        self.stale_document_plan = None;
        self.view = View::Markdown(ready);
        true
    }
    pub fn source_target(&self) -> Option<(u64, &Path)> {
        self.path
            .as_deref()
            .filter(|p| source_required(p) && !self.directory)
            .map(|p| (self.source_generation, p))
    }
    /// No I/O: hide old pixels and cancel, keeping intent until a stable probe.
    pub fn invalidate_source(&mut self) -> bool {
        if self.source_blocked {
            return false;
        }
        self.revision = self
            .revision
            .checked_add(1)
            .expect("preview revision exhausted");
        self.expected_source = None;
        self.source_blocked = true;
        self.video_playing = false;
        self.audio_position = 0.0;
        self.audio_origin = 0.0;
        self.audio_playing = false;
        self.audio_notice = None;
        self.audio_failed = false;
        self.video_position = 0.0;
        self.video_duration = None;
        self.video_transform = Default::default();
        self.pdf_text = false;
        self.pdf_page = 1;
        self.pdf_pages = None;
        self.pdf_transform = Default::default();
        self.view = View::Loading;
        self.needs_start = self.path.is_some();
        if let Some(active) = &self.active {
            active.task.cancel();
        }
        true
    }
    fn reject_source(&mut self, message: String) -> View {
        // A decoder-detected failure must force a NEW watch/probe token, not
        // repeatedly retry against the old stable Verified snapshot.
        self.source_generation = self
            .source_generation
            .checked_add(1)
            .expect("media generation exhausted");
        self.invalidate_source();
        View::Error(message)
    }
    pub fn verify_source(&mut self, source: files_core::media_source::Revision) -> bool {
        if self.expected_source == Some(source) && !self.source_blocked && !self.document_suspended
        {
            return false;
        }
        self.invalidate_source();
        self.expected_source = Some(source);
        self.source_blocked = false;
        self.needs_start = self.path.is_some();
        self.view = View::Loading;
        true
    }
    pub fn source_error(&mut self, message: String) -> bool {
        self.invalidate_source();
        if matches!(&self.view, View::Error(old) if old == &message) {
            return false;
        }
        self.view = View::Error(message);
        true
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn visible(&self) -> bool {
        !matches!(self.view, View::Closed)
    }
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }
    pub fn view_mut(&mut self) -> &mut View {
        &mut self.view
    }
    pub fn view(&self) -> &View {
        &self.view
    }
    pub fn document(&self) -> Option<&TextDocument> {
        match &self.view {
            View::Ready(r) | View::PdfText(r) => Some(&r.document),
            View::Markdown(r) => Some(&r.rendered.document),
            View::Html(r) => Some(&r.rendered.document),
            _ => None,
        }
    }
    pub fn document_mut(&mut self) -> Option<&mut TextDocument> {
        match &mut self.view {
            View::Ready(r) | View::PdfText(r) => Some(&mut r.document),
            View::Markdown(r) => Some(&mut r.rendered.document),
            View::Html(r) => Some(&mut r.rendered.document),
            _ => None,
        }
    }
    pub fn ready_mut(&mut self) -> Option<&mut Ready> {
        match &mut self.view {
            View::Ready(ready) | View::PdfText(ready) => Some(ready),
            _ => None,
        }
    }
    pub fn is_drained(&self) -> bool {
        self.active.is_none() && !self.needs_start
    }
    /// Closing can abandon a readonly probe only after its gate forbids all
    /// future child spawns. It is still NOT drained/ACKed until active is None.
    pub fn exit_allowed(&self) -> bool {
        self.is_drained()
            || (self.path.is_none()
                && !self.needs_start
                && self.active.as_ref().is_some_and(|a| a.task.exit_allowed()))
    }
    pub fn readonly_pending(&self) -> bool {
        self.active
            .as_ref()
            .is_some_and(|a| a.task.readonly_pending())
    }
    pub fn configure_viewport(&mut self, width: i32, height: i32) {
        let size = (width.clamp(128, 1600), height.clamp(128, 1200));
        if self.html_size == size {
            return;
        }
        self.html_size = size;
        let html = self
            .path
            .as_deref()
            .and_then(Path::extension)
            .and_then(|e| e.to_str())
            .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "htm" | "html"));
        if html {
            self.revision = self
                .revision
                .checked_add(1)
                .expect("preview revision exhausted");
            if let Some(active) = &self.active {
                active.task.cancel();
            }
            self.needs_start = true;
            self.view = View::Loading;
        }
    }
    pub fn bitmap_viewport_mut(&mut self) -> Option<&mut crate::image_preview_view::Viewport> {
        match &mut self.view {
            View::Image(r) => Some(&mut r.viewport),
            View::Pdf(r) => Some(&mut r.viewport),
            View::Video(r) => Some(&mut r.viewport),
            _ => None,
        }
    }
    pub fn pdf_text(&self) -> bool {
        self.pdf_text
    }
    pub fn pdf_available(&self) -> bool {
        self.pdf_pages.is_some() && !self.source_blocked
    }
    pub fn pdf_page(&self) -> u32 {
        self.pdf_page
    }
    pub fn pdf_pages(&self) -> Option<u32> {
        self.pdf_pages
    }
    pub fn toggle_pdf_text(&mut self) {
        if !self.pdf_available() {
            return;
        }
        if let View::Pdf(ready) = &self.view {
            self.pdf_transform = ready.viewport.transform();
        }
        self.pdf_text = !self.pdf_text;
        self.revision = self
            .revision
            .checked_add(1)
            .expect("preview revision exhausted");
        if let Some(active) = &self.active {
            active.task.cancel();
        }
        self.needs_start = true;
        self.view = View::Loading;
    }
    pub fn change_pdf_page(&mut self, delta: i32) {
        let Some(count) = self.pdf_pages else {
            return;
        };
        let page = (i64::from(self.pdf_page) + i64::from(delta)).clamp(1, i64::from(count)) as u32;
        if page == self.pdf_page || self.path.is_none() {
            return;
        }
        if let View::Pdf(ready) = &self.view {
            self.pdf_transform = ready.viewport.transform();
        }
        self.pdf_page = page;
        self.revision = self
            .revision
            .checked_add(1)
            .expect("preview revision exhausted");
        if let Some(active) = &self.active {
            active.task.cancel();
        }
        self.needs_start = true;
        self.view = View::Loading;
    }
    pub fn audio_info(&self) -> Option<&crate::audio_preview_job::Info> {
        match &self.view {
            View::Ready(r) => r.audio.as_ref().map(|a| &a.info),
            _ => None,
        }
    }
    pub fn audio_playing(&self) -> bool {
        self.audio_playing
    }
    pub fn audio_running(&self) -> bool {
        self.audio_playing
            && self
                .active
                .as_ref()
                .is_some_and(|a| a.revision == self.revision && a.task.started().is_some())
    }
    pub fn audio_position(&self) -> f64 {
        self.audio_position
    }
    pub fn audio_notice(&self) -> Option<&str> {
        self.audio_notice.as_deref()
    }
    pub fn toggle_audio(&mut self) {
        let Some(duration) = self.audio_info().map(|a| a.duration) else {
            return;
        };
        if self.source_blocked || self.expected_source.is_none() {
            return;
        }
        self.revision = self
            .revision
            .checked_add(1)
            .expect("preview revision exhausted");
        if let Some(active) = &self.active {
            active.task.cancel();
        }
        if !self.audio_playing && self.audio_position >= duration {
            self.audio_position = 0.0;
        }
        self.audio_playing = !self.audio_playing;
        self.needs_start = self.audio_playing;
        self.audio_origin = self.audio_position;
        self.audio_notice = None;
    }
    pub fn seek_audio_by(&mut self, delta: f64) {
        self.seek_audio_to(self.audio_position + delta);
    }
    pub fn seek_audio_to(&mut self, position: f64) {
        let Some(duration) = self.audio_info().map(|a| a.duration) else {
            return;
        };
        if !position.is_finite() || self.source_blocked {
            return;
        }
        let position = position.clamp(0.0, (duration - 0.05).max(0.0));
        if (position - self.audio_position).abs() < 0.001 {
            return;
        }
        self.audio_position = position;
        self.audio_origin = position;
        self.audio_notice = None;
        self.revision = self
            .revision
            .checked_add(1)
            .expect("preview revision exhausted");
        if let Some(active) = &self.active {
            active.task.cancel();
        }
        self.needs_start = self.audio_playing;
    }
    /// Sound requested for the current video: not muted and the output works.
    pub fn video_audio(&self) -> bool {
        !self.muted && !self.audio_failed
    }
    /// The audio output failed for this preview and it plays silently.
    pub fn video_audio_failed(&self) -> bool {
        self.audio_failed
    }
    pub fn muted(&self) -> bool {
        self.muted
    }
    /// Global mute preference (from the saved configuration).
    pub fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
    }
    pub fn set_autoplay(&mut self, autoplay: bool) {
        self.autoplay = autoplay;
    }
    /// The device-pixel box the preview shows video in; the next decode uses it.
    pub fn set_video_target(&mut self, target: (u32, u32)) {
        self.video_target = (target.0.max(1), target.1.max(1));
    }
    pub fn video_target(&self) -> (u32, u32) {
        self.video_target
    }
    fn video_box(&self) -> (u32, u32) {
        if self.video_transform.quarter_turns % 2 == 1 {
            (self.video_target.1, self.video_target.0)
        } else {
            self.video_target
        }
    }
    /// Toggles the global mute. Returns true when it changed (to be saved).
    pub fn toggle_video_audio(&mut self) -> bool {
        if self.source_blocked || !matches!(&self.view,View::Video(r)if r.frame.audio.is_some()) {
            return false;
        }
        self.muted = !self.muted;
        // An explicit choice retries a failed output.
        self.audio_failed = false;
        if let View::Video(r) = &mut self.view {
            r.notice = None;
        }
        if self.video_playing {
            self.revision = self
                .revision
                .checked_add(1)
                .expect("preview revision exhausted");
            if let Some(active) = &self.active {
                active.task.cancel();
            }
            self.needs_start = true;
            if let View::Video(r) = &mut self.view {
                r.sequence = None;
            }
        }
        true
    }
    pub fn video_playing(&self) -> bool {
        self.video_playing
    }
    pub fn toggle_video(&mut self) {
        if !matches!(self.view, View::Video(_)) {
            return;
        }
        self.revision = self
            .revision
            .checked_add(1)
            .expect("preview revision exhausted");
        if let Some(active) = &self.active {
            active.task.cancel();
        }
        self.video_playing = !self.video_playing;
        self.needs_start = self.video_playing;
        if self.video_playing
            && let View::Video(ready) = &mut self.view
        {
            // Keep the paused pixels, but readiness belongs to the NEW decoder.
            ready.sequence = None;
        }
    }
    pub fn seek_video_by(&mut self, delta: f64) {
        self.seek_video_to(self.video_position + delta);
    }
    pub fn seek_video_to(&mut self, position: f64) {
        let Some(duration) = self.video_duration else {
            return;
        };
        if !position.is_finite() || self.path.is_none() {
            return;
        }
        let position = position.clamp(0.0, (duration - 0.05).max(0.0));
        if (position - self.video_position).abs() < 0.001 {
            return;
        }
        if let View::Video(ready) = &self.view {
            self.video_transform = ready.viewport.transform();
        }
        self.video_position = position;
        // Playing continues from the new position once its frame is ready.
        self.autoplay_pending = self.video_playing && self.autoplay;
        self.video_playing = false;
        self.revision = self
            .revision
            .checked_add(1)
            .expect("preview revision exhausted");
        if let Some(active) = &self.active {
            active.task.cancel();
        }
        self.needs_start = true;
        // Drop the old RGB before dispatching another decode; no frame history.
        self.view = View::Loading;
    }
    pub fn follow_link(&mut self, target: crate::preview_link::Target) {
        use crate::preview_link::Target;
        match target {
            Target::Fragment(fragment) => {
                if let View::Markdown(ready) = &mut self.view
                    && !ready.rendered.jump_to_fragment(&fragment)
                {
                    ready.notice = Some("No se encontró ese título".into());
                }
            }
            Target::Relative { path, fragment } => {
                let Some(root) = self.root.clone() else {
                    return;
                };
                let current = self.relative.as_deref().or_else(|| {
                    self.path
                        .as_deref()
                        .and_then(Path::file_name)
                        .map(Path::new)
                });
                let relative = match current
                    .and_then(|current| crate::preview_link::resolve_relative(current, &path).ok())
                {
                    Some(relative) => relative,
                    None => {
                        if let View::Markdown(ready) = &mut self.view {
                            ready.notice =
                                Some("Enlace bloqueado: fuera de la raíz o ruta no válida".into());
                        }
                        return;
                    }
                };
                let markdown = relative
                    .extension()
                    .and_then(|s| s.to_str())
                    .is_some_and(|s| matches!(s.to_ascii_lowercase().as_str(), "md" | "markdown"));
                if !markdown {
                    if let View::Markdown(ready) = &mut self.view {
                        ready.notice=Some("Este enlace local aún no tiene preview integrado; no se abrió ninguna app ni red".into());
                    }
                    return;
                }
                if root
                    .as_os_str()
                    .len()
                    .checked_add(relative.as_os_str().len())
                    .and_then(|n| n.checked_add(1))
                    .is_none_or(|n| n > 4096)
                {
                    if let View::Markdown(ready) = &mut self.view {
                        ready.notice = Some("Enlace local fuera del límite de ruta".into());
                    }
                    return;
                }
                self.close();
                self.path = Some(root.join(&relative));
                self.root = Some(root);
                self.relative = Some(relative);
                self.fragment = fragment;
                self.needs_start = true;
                self.view = View::Loading;
            }
        }
    }
    pub fn pump(&mut self) -> bool {
        let source = self
            .root
            .as_ref()
            .zip(self.relative.as_ref())
            .map(
                |(root, relative)| crate::markdown_preview_job::Source::Relative {
                    root: root.clone(),
                    relative: relative.clone(),
                },
            );
        let html_size = self.html_size;
        let video_position = self.video_position;
        let video_box = self.video_box();
        let stream = if self.video_playing && self.needs_start {
            match &self.view {
                View::Video(r) => {
                    // The source's own pixels, reduced only to the device
                    // pixels of the preview (4K cap); not the snapshot's size.
                    let (width, height) = crate::video_preview_job::fit(
                        r.frame.display_width,
                        r.frame.display_height,
                        video_box.0,
                        video_box.1,
                    );
                    Some(crate::video_stream_job::Options {
                        width,
                        height,
                        source_width: r.frame.bitmap.source_width,
                        source_height: r.frame.bitmap.source_height,
                        codec: r.frame.codec.clone(),
                        duration: r.frame.duration,
                        video_index: r.frame.video_index,
                        frame_rate: r.frame.frame_rate,
                        audio: if self.video_audio() {
                            r.frame.audio.clone()
                        } else {
                            None
                        },
                    })
                }
                _ => None,
            }
        } else {
            None
        };
        let audio_play = if self.audio_playing && self.needs_start {
            self.audio_info().cloned().map(|i| (i, self.audio_origin))
        } else {
            None
        };
        let expected = self.expected_source;
        let pdf_page = self.pdf_page;
        let pdf_text = self.pdf_text;
        let directory = self.directory;
        self.pump_with(|path| {
            let html = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "html" | "htm"));
            let image = path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
                matches!(
                    e.to_ascii_lowercase().as_str(),
                    "png" | "jpg" | "jpeg" | "webp"
                )
            });
            let markdown = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "md" | "markdown"));
            let video = path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
                matches!(
                    e.to_ascii_lowercase().as_str(),
                    "gif"
                        | "mp4"
                        | "mkv"
                        | "webm"
                        | "mov"
                        | "avi"
                        | "m4v"
                        | "mpg"
                        | "mpeg"
                        | "3gp"
                        | "ts"
                        | "mts"
                        | "m2ts"
                )
            });
            let pdf = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("pdf"));
            if directory {
                Box::new(crate::du_job::Job::start_directory(path)) as Box<dyn Task>
            } else if let Some(options) = stream {
                Box::new(crate::video_stream_job::Job::start(
                    path,
                    video_position,
                    expected,
                    options,
                )) as Box<dyn Task>
            } else if path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("zip"))
            {
                Box::new(crate::archive_preview_job::Job::start(path)) as Box<dyn Task>
            } else if let Some((info, position)) = audio_play {
                // A verified source is mandatory; no unverified playback fallback.
                Box::new(crate::audio_preview_job::PlaybackJob::start(
                    path,
                    expected.expect("guarded audio source"),
                    info,
                    position,
                )) as Box<dyn Task>
            } else if audio_source(&path) {
                Box::new(crate::audio_preview_job::Job::start(path, expected)) as Box<dyn Task>
            } else if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("docx"))
            {
                Box::new(crate::office_preview_job::Job::start(
                    path,
                    expected.expect("guarded DOCX source"),
                )) as Box<dyn Task>
            } else if pdf {
                Box::new(crate::pdf_preview_job::Job::start(
                    path,
                    pdf_page,
                    expected.expect("guarded PDF source"),
                    pdf_text,
                )) as Box<dyn Task>
            } else if video {
                Box::new(crate::video_preview_job::Job::start(
                    path,
                    video_position,
                    expected,
                    video_box,
                )) as Box<dyn Task>
            } else if html {
                Box::new(crate::html_preview_job::Job::start(
                    path,
                    html_size.0,
                    html_size.1,
                    expected.expect("guarded HTML source"),
                )) as Box<dyn Task>
            } else if markdown {
                Box::new(crate::markdown_preview_job::Job::start(
                    source.unwrap_or(crate::markdown_preview_job::Source::Direct(path)),
                )) as Box<dyn Task>
            } else if image {
                Box::new(ImageJob::start(path, expected)) as Box<dyn Task>
            } else {
                Box::new(Job::start(path)) as Box<dyn Task>
            }
        })
    }
    fn pump_with(&mut self, start: impl FnOnce(PathBuf) -> Box<dyn Task>) -> bool {
        let mut changed = false;
        // A start deferred by the previous pump uses this pump's options.
        self.defer_start = false;
        if self
            .active
            .as_ref()
            .is_some_and(|a| a.revision == self.revision && a.task.source_invalid())
        {
            self.view = self.reject_source("El original cambió durante la vista previa".into());
            changed = true;
        }
        if self.audio_playing
            && !self.source_blocked
            && let Some(duration) = self.audio_info().map(|i| i.duration)
            && let Some(active) = &self.active
            && active.revision == self.revision
            && let Some(started) = active.task.started()
        {
            let position = (self.audio_origin + started.elapsed().as_secs_f64()).min(duration);
            changed |= position.floor() != self.audio_position.floor();
            self.audio_position = position;
        }
        if let Some(active) = &self.active
            && let Some(frame) = active.task.poll_frame()
        {
            if active.revision == self.revision
                && !self.source_blocked
                && !active.task.source_invalid()
                && frame.source == self.expected_source
                && self.video_playing
                && let View::Video(ready) = &mut self.view
            {
                self.video_position = frame.position;
                ready.frame.position = frame.position;
                ready.frame.bitmap.width = frame.width;
                ready.frame.bitmap.height = frame.height;
                let previous = std::mem::replace(&mut ready.frame.bitmap.rgb, frame.rgb);
                active.task.recycle_rgb(previous);
                ready.sequence = Some(frame.sequence);
                ready.notice = None;
                ready.viewport.refresh_pixels();
            }
            changed = true;
        }
        if let Some(result) = self.active.as_ref().and_then(|active| active.task.poll()) {
            let active = self.active.take().expect("polled active task");
            if active.revision == self.revision
                && self.path.is_some()
                && !self.source_blocked
                && !self.document_suspended
            {
                let result = result.and_then(|payload| {
                    let actual = match &payload {
                        Payload::Audio(a) => Some(Some(a.source)),
                        Payload::Office(o) => Some(Some(o.source)),
                        Payload::Image(b) => Some(b.source),
                        Payload::Video(v) => Some(v.bitmap.source),
                        Payload::Pdf(p) => Some(p.bitmap.source),
                        Payload::PdfText(t) => Some(Some(t.source)),
                        Payload::Html(h) => Some(Some(h.source)),
                        _ => None,
                    };
                    if actual.is_some_and(|actual| actual != self.expected_source) {
                        Err("Se descartó un resultado de otro original".into())
                    } else {
                        Ok(payload)
                    }
                });
                let ready_audio = match &result {
                    Ok(Payload::Audio(a)) => Some(AudioReady {
                        info: a.info.clone(),
                        timeline: Default::default(),
                    }),
                    _ => None,
                };
                let was_video_playback = self.video_playing && matches!(self.view, View::Video(_));
                let sound_was_requested = was_video_playback
                    && self.video_audio()
                    && matches!(&self.view, View::Video(r) if r.frame.audio.is_some());
                let was_audio_playback = self.audio_playing && self.audio_info().is_some();
                let ready_source = match &result {
                    Ok(Payload::Audio(a)) => Some(a.source),
                    Ok(Payload::Office(o)) => Some(o.source),
                    _ => None,
                };
                self.view = match result {
                    Ok(Payload::Text(TextPreview {
                        text,
                        encoding,
                        truncated,
                        ..
                    }))
                    | Ok(Payload::Office(crate::office_preview_job::Ready {
                        preview:
                            TextPreview {
                                text,
                                encoding,
                                truncated,
                                ..
                            },
                        ..
                    }))
                    | Ok(Payload::Audio(crate::audio_preview_job::Ready {
                        preview:
                            TextPreview {
                                text,
                                encoding,
                                truncated,
                                ..
                            },
                        ..
                    })) => match TextDocument::new(text) {
                        Ok(document) => View::Ready(Ready {
                            audio: ready_audio,
                            source: ready_source,
                            document,
                            encoding,
                            truncated,
                        }),
                        Err(error) => View::Error(error.into()),
                    },
                    Ok(Payload::AudioPlaybackEnd(end)) => {
                        self.audio_playing = false;
                        match end {
                            crate::audio_preview_job::PlaybackEnd::Complete => {
                                if let Some(duration) = self.audio_info().map(|i| i.duration) {
                                    self.audio_position = duration;
                                    self.audio_origin = duration;
                                }
                                std::mem::replace(&mut self.view, View::Loading)
                            }
                            crate::audio_preview_job::PlaybackEnd::SourceUnavailable(message) => {
                                self.reject_source(message)
                            }
                        }
                    }
                    Ok(Payload::SourceUnavailable(message)) => self.reject_source(message),
                    Ok(Payload::PlaybackFinished) => {
                        self.video_playing = false;
                        std::mem::replace(&mut self.view, View::Loading)
                    }
                    Ok(Payload::PdfText(text)) => {
                        self.pdf_page = text.page;
                        self.pdf_pages = Some(text.pages);
                        match TextDocument::new(text.text) {
                            Ok(document) => View::PdfText(Ready {
                                document,
                                source: Some(text.source),
                                audio: None,
                                truncated: false,
                                encoding: format!(
                                    "PDF · texto extraído · página {} de {}",
                                    text.page, text.pages
                                ),
                            }),
                            Err(error) => View::Error(error.into()),
                        }
                    }
                    Ok(Payload::Pdf(frame)) => {
                        self.pdf_page = frame.page;
                        self.pdf_pages = Some(frame.pages);
                        let mut viewport = crate::image_preview_view::Viewport::default();
                        viewport.restore_transform(self.pdf_transform);
                        View::Pdf(PdfReady { frame, viewport })
                    }
                    Ok(Payload::Video(frame)) => {
                        self.video_duration = Some(frame.duration);
                        self.video_position = frame.position;
                        let mut viewport = crate::image_preview_view::Viewport::default();
                        viewport.restore_transform(self.video_transform);
                        if std::mem::take(&mut self.autoplay_pending) && self.autoplay {
                            self.video_playing = true;
                            self.needs_start = true;
                            self.defer_start = true;
                        }
                        View::Video(VideoReady {
                            notice: None,
                            frame,
                            viewport,
                            timeline: Default::default(),
                            sequence: None,
                        })
                    }
                    Ok(Payload::Html(ready)) => View::Html(ready),
                    Ok(Payload::Markdown(ready)) => {
                        if ready.plan.is_none() || ready.source.is_none() {
                            View::Error("Documento sin manifiesto de origen verificable".into())
                        } else {
                            self.source_generation = self
                                .source_generation
                                .checked_add(1)
                                .expect("document generation exhausted");
                            self.pending_markdown = Some(ready);
                            // Hidden until the readonly worker attaches watches and
                            // verifies every FD/name under this fresh token/epoch.
                            View::Loading
                        }
                    }
                    Ok(Payload::Image(bitmap)) => View::Image(ImageReady {
                        bitmap,
                        viewport: Default::default(),
                    }),
                    Err(error) => {
                        self.video_playing = false;
                        self.audio_playing = false;
                        if was_audio_playback {
                            self.audio_notice = Some(error);
                            std::mem::replace(&mut self.view, View::Loading)
                        } else if sound_was_requested {
                            // Audio output unavailable: keep playing, silently,
                            // for this preview only; the mute preference stays.
                            self.audio_failed = true;
                            self.video_playing = true;
                            self.needs_start = true;
                            self.defer_start = true;
                            if let View::Video(r) = &mut self.view {
                                r.sequence = None;
                                r.notice = Some(format!("sin sonido: {error}"));
                            }
                            std::mem::replace(&mut self.view, View::Loading)
                        } else if was_video_playback {
                            // A silent retry failing too means the cause was
                            // not the audio output: do not blame it.
                            self.audio_failed = false;
                            if let View::Video(r) = &mut self.view {
                                r.notice = Some(error);
                            }
                            std::mem::replace(&mut self.view, View::Loading)
                        } else {
                            View::Error(error)
                        }
                    }
                };
            }
            changed = true;
        }
        if self.active.is_none()
            && self.needs_start
            && !self.defer_start
            && !self.source_blocked
            && !self.document_suspended
        {
            let path = self.path.as_ref().expect("pending preview path").clone();
            self.active = Some(Active {
                revision: self.revision,
                task: start(path),
            });
            self.needs_start = false;
            changed = true;
        }
        changed
    }
}
impl Drop for State {
    fn drop(&mut self) {
        if let Some(active) = &self.active {
            active.task.cancel();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
    };
    #[derive(Default)]
    struct Stub {
        cancelled: Cell<bool>,
        source_bad: Cell<bool>,
        readonly: Cell<bool>,
        started: Cell<Option<std::time::Instant>>,
        result: RefCell<Option<Result<Payload, String>>>,
        frame: RefCell<Option<crate::video_stream_job::Frame>>,
        recycled: RefCell<Vec<Vec<u8>>>,
    }
    impl Task for Rc<Stub> {
        fn source_invalid(&self) -> bool {
            self.source_bad.get()
        }
        fn started(&self) -> Option<std::time::Instant> {
            self.started.get()
        }
        fn exit_allowed(&self) -> bool {
            self.readonly.get() && self.cancelled.get()
        }
        fn readonly_pending(&self) -> bool {
            self.readonly.get() && self.result.borrow().is_none()
        }
        fn recycle_rgb(&self, rgb: Vec<u8>) {
            self.recycled.borrow_mut().push(rgb);
        }
        fn poll_frame(&self) -> Option<crate::video_stream_job::Frame> {
            self.frame.borrow_mut().take()
        }
        fn cancel(&self) {
            self.cancelled.set(true);
        }
        fn poll(&self) -> Option<Result<Payload, String>> {
            self.result.borrow_mut().take()
        }
    }
    fn complete(stub: &Stub, text: &str) {
        *stub.result.borrow_mut() = Some(Ok(Payload::Text(TextPreview {
            text: text.into(),
            encoding: "UTF-8".into(),
            truncated: false,
            relative_source: None,
        })));
    }
    fn no_start(_: PathBuf) -> Box<dyn Task> {
        panic!("unexpected helper spawn")
    }
    fn signature(inode: u64) -> files_core::media_source::Revision {
        files_core::media_source::Revision {
            device: 1,
            inode,
            bytes: 3,
            modified_seconds: 4,
            modified_nanoseconds: 5,
            changed_seconds: 6,
            changed_nanoseconds: 7,
        }
    }
    fn image(source: files_core::media_source::Revision) -> Payload {
        Payload::Image(Bitmap {
            source: Some(source),
            width: 1,
            height: 1,
            source_width: 1,
            source_height: 1,
            format: "png".into(),
            rgb: vec![1, 2, 3],
        })
    }
    #[test]
    fn hidden_media_cancels_and_cannot_spawn_until_reverified() {
        let mut state = State::new();
        state.open("/fixture/image.png".into());
        state.verify_source(signature(1));
        let a = Rc::new(Stub::default());
        state.pump_with(|_| Box::new(a.clone()));
        let token = state.source_target().unwrap().0;
        state.suspend_media();
        assert!(a.cancelled.get());
        assert_ne!(state.source_target().unwrap().0, token);
        assert!(!state.pump_with(no_start));
        *a.result.borrow_mut() = Some(Ok(image(signature(1))));
        state.pump_with(no_start);
        assert!(matches!(state.view(), View::Loading));
        state.verify_source(signature(2));
        let b = Rc::new(Stub::default());
        state.pump_with(|_| Box::new(b.clone()));
        *b.result.borrow_mut() = Some(Ok(image(signature(2))));
        state.pump_with(no_start);
        assert!(matches!(state.view(), View::Image(_)));
    }
    #[test]
    fn source_gate_waits_then_discards_late_a_after_dirty_and_b() {
        let mut state = State::new();
        state.open("/fixture/image.png".into());
        assert!(!state.pump_with(no_start));
        state.verify_source(signature(1));
        let a = Rc::new(Stub::default());
        state.pump_with(|_| Box::new(a.clone()));
        state.invalidate_source();
        assert!(a.cancelled.get());
        assert!(matches!(state.view(), View::Loading));
        state.verify_source(signature(2));
        *a.result.borrow_mut() = Some(Ok(image(signature(1))));
        let b = Rc::new(Stub::default());
        state.pump_with(|_| Box::new(b.clone()));
        assert!(matches!(state.view(), View::Loading));
        *b.result.borrow_mut() = Some(Ok(image(signature(2))));
        state.pump_with(no_start);
        assert!(matches!(state.view(),View::Image(r) if r.bitmap.source==Some(signature(2))));
    }
    #[test]
    fn decoder_response_must_match_authorized_revision_even_same_generation() {
        let mut state = State::new();
        state.open("/fixture/image.png".into());
        state.verify_source(signature(1));
        let a = Rc::new(Stub::default());
        state.pump_with(|_| Box::new(a.clone()));
        *a.result.borrow_mut() = Some(Ok(image(signature(2))));
        state.pump_with(no_start);
        assert!(matches!(state.view(), View::Error(_)));
        state.invalidate_source();
        state.close();
        assert!(!state.pump_with(no_start));
    }
    #[test]
    fn directory_intent_and_apparent_metadata_reset_on_file_navigation() {
        let mut state = State::new();
        state.open_directory("/fixture/album.flac".into());
        assert!(state.is_directory());
        let summary = folder_metadata((8, 5));
        assert!(summary.text.contains("8 bytes"));
        assert!(summary.text.contains("descendientes: 5"));
        assert!(summary.text.contains("No sigue enlaces"));
        assert!(!summary.truncated);
        state.open("/fixture/song.flac".into());
        assert!(!state.is_directory());
        state.close();
        assert!(!state.is_directory());
    }
    fn complete_markdown(stub: &Stub, source: &str) {
        let proof = files_core::relative_source::Proof {
            root: files_core::relative_source::DirectoryIdentity {
                device: 1,
                inode: 2,
            },
            file: signature(9),
        };
        let resources = std::sync::Arc::new(Vec::new());
        let plan = std::sync::Arc::new(
            crate::document_source::Plan::new(
                "/fixture".into(),
                "overview.md".into(),
                proof,
                resources.clone(),
                true,
            )
            .unwrap(),
        );
        *stub.result.borrow_mut() = Some(Ok(Payload::Markdown(Box::new(
            crate::markdown_preview_job::Ready {
                source: Some(proof),
                resources,
                plan: Some(plan),
                resource_intents: std::sync::Arc::new(Vec::new()),
                rendered: crate::markdown_render::Rendered::new(
                    crate::markdown_document::Document::parse(source).unwrap(),
                )
                .unwrap(),
                encoding: "UTF-8".into(),
                truncated: false,
                notice: None,
            },
        ))));
    }
    #[test]
    fn markdown_candidate_never_publishes_until_verified_and_dirty_discards_it() {
        let mut state = State::new();
        let a = Rc::new(Stub::default());
        state.open("/fixture/overview.md".into());
        state.pump_with(|_| Box::new(a.clone()));
        complete_markdown(&a, "# A");
        state.pump_with(no_start);
        assert!(matches!(state.view(), View::Loading));
        let token = state.document_plan().unwrap().0;
        state.invalidate_document();
        assert_ne!(state.document_plan().unwrap().0, token);
        assert!(!state.verify_document(signature(9))); // old verification cannot restore discarded candidate
        let b = Rc::new(Stub::default());
        state.pump_with(|_| Box::new(b.clone()));
        complete_markdown(&b, "# B");
        state.pump_with(no_start);
        assert!(matches!(state.view(), View::Loading));
        state.verify_document(signature(9));
        assert!(
            matches!(state.view(),View::Markdown(r) if r.rendered.document.text().contains('B'))
        );
        state.close();
        assert!(state.document_plan().is_none());
        assert!(!state.verify_document(signature(9)));
        assert!(state.is_drained());
    }
    #[test]
    fn hidden_document_and_watch_failure_wait_without_spawning_retry_loop() {
        let mut state = State::new();
        let a = Rc::new(Stub::default());
        state.open("/fixture/overview.md".into());
        state.pump_with(|_| Box::new(a.clone()));
        complete_markdown(&a, "# A");
        state.pump_with(no_start);
        state.verify_document(signature(9));
        state.suspend_media();
        assert!(matches!(state.view(), View::Loading));
        state.pump_with(no_start);
        state.resume_media();
        let b = Rc::new(Stub::default());
        state.pump_with(|_| Box::new(b.clone()));
        complete_markdown(&b, "# B");
        state.pump_with(no_start);
        state.document_watch_unavailable("watch failed".into());
        state.pump_with(no_start);
        let token = state.document_plan().unwrap().0;
        assert!(!state.document_watch_unavailable("watch failed".into()));
        assert_eq!(state.document_plan().unwrap().0, token);
        // Successful subscription/proof restarts a fresh read, never publishes old RGB.
        assert!(state.verify_document(signature(9)));
        assert!(matches!(state.view(), View::Loading));
        let c = Rc::new(Stub::default());
        state.pump_with(|_| Box::new(c.clone()));
        state.close();
        complete_markdown(&c, "# late C");
        state.pump_with(no_start);
        assert!(matches!(state.view(), View::Closed));
    }
    #[test]
    fn local_markdown_links_keep_root_and_close_discards_stale_document() {
        let mut state = State::new();
        let first = Rc::new(Stub::default());
        state.open("/fixture/overview.md".into());
        state.pump_with(|_| Box::new(first.clone()));
        complete_markdown(&first, "# Aurora\n\n[guía](guide.md#detalles)");
        state.pump_with(no_start);
        state.verify_document(signature(9));
        state.follow_link(crate::preview_link::Target::Relative {
            path: "guide.md".into(),
            fragment: Some("detalles".into()),
        });
        assert_eq!(state.path(), Some(Path::new("/fixture/guide.md")));
        assert_eq!(state.root.as_deref(), Some(Path::new("/fixture")));
        let guide = Rc::new(Stub::default());
        state.pump_with(|_| Box::new(guide.clone()));
        state.open("/fixture/overview.md".into());
        assert!(guide.cancelled.get());
        complete_markdown(&guide, "# Detalles\n\nobsolete");
        let last = Rc::new(Stub::default());
        state.pump_with(|_| Box::new(last.clone()));
        assert!(matches!(state.view(), View::Loading));
        state.close();
        complete_markdown(&last, "last");
        state.pump_with(no_start);
        state.verify_document(signature(9));
        assert!(matches!(state.view(), View::Closed));
        assert!(state.is_drained());
    }
    fn complete_html(stub: &Stub, width: i32, height: i32) {
        complete_html_source(stub, width, height, signature(1));
    }
    fn complete_html_source(
        stub: &Stub,
        width: i32,
        height: i32,
        source: files_core::media_source::Revision,
    ) {
        let wire=serde_json::to_vec(&serde_json::json!({"schema":1,"status":"PROBE_ONLY","metrics":"approximate_scalar_monospace","viewport":[width,height],"document":[width,900],"command_count":0,"commands":[]})).unwrap();
        *stub.result.borrow_mut() = Some(Ok(Payload::Html(Box::new(
            crate::html_preview_job::Ready {
                source,
                rendered: crate::html_display_list::Document::decode(&wire).unwrap(),
                width,
                height,
            },
        ))));
    }
    fn complete_pdf(stub: &Stub, page: u32) {
        complete_pdf_source(stub, page, signature(1));
    }
    fn complete_pdf_source(stub: &Stub, page: u32, source: files_core::media_source::Revision) {
        *stub.result.borrow_mut() = Some(Ok(Payload::Pdf(crate::pdf_preview_job::Ready {
            bitmap: Bitmap {
                source: Some(source),
                width: 1,
                height: 1,
                source_width: 1,
                source_height: 1,
                format: "pdf".into(),
                rgb: vec![1, 2, 3],
            },
            page,
            pages: 3,
        })));
    }
    #[test]
    fn html_source_blocks_until_verified_and_drops_old_generation_and_fd() {
        let mut state = State::new();
        state.open("/fixture/layout.html".into());
        assert!(!state.pump_with(no_start));
        state.verify_source(signature(1));
        let a = Rc::new(Stub::default());
        state.pump_with(|_| Box::new(a.clone()));
        state.invalidate_source();
        state.verify_source(signature(2));
        complete_html_source(&a, 640, 480, signature(1));
        let b = Rc::new(Stub::default());
        state.pump_with(|_| Box::new(b.clone()));
        assert!(matches!(state.view(), View::Loading));
        complete_html_source(&b, 640, 480, signature(2));
        state.pump_with(no_start);
        assert!(matches!(state.view(),View::Html(h)if h.source==signature(2)));
        state.configure_viewport(800, 500);
        let c = Rc::new(Stub::default());
        state.pump_with(|_| Box::new(c.clone()));
        complete_html_source(&c, 800, 500, signature(1));
        state.pump_with(no_start);
        assert!(matches!(state.view(), View::Error(_)));
        state.close();
        assert!(state.is_drained());
    }
    #[test]
    fn office_checked_source_rejects_late_a_and_wrong_effective_revision() {
        let mut state = State::new();
        state.open("/fixture/report.docx".into());
        assert!(source_required(Path::new("/fixture/report.DOCX")));
        assert!(!state.pump_with(no_start));
        state.verify_source(signature(1));
        let a = Rc::new(Stub::default());
        state.pump_with(|_| Box::new(a.clone()));
        state.invalidate_source();
        assert!(a.cancelled.get());
        state.verify_source(signature(2));
        let complete = |stub: &Stub, source, text: &str| {
            *stub.result.borrow_mut() =
                Some(Ok(Payload::Office(crate::office_preview_job::Ready {
                    source,
                    preview: TextPreview {
                        text: text.into(),
                        encoding: "DOCX · texto extraído".into(),
                        truncated: false,
                        relative_source: None,
                    },
                })));
        };
        complete(&a, signature(1), "OLD A");
        let b = Rc::new(Stub::default());
        state.pump_with(|_| Box::new(b.clone()));
        assert!(matches!(state.view(), View::Loading));
        complete(&b, signature(2), "BETA 文档\t");
        state.pump_with(no_start);
        assert!(
            matches!(state.view(),View::Ready(r) if r.source==Some(signature(2)) && r.document.text()=="BETA 文档\t")
        );
        state.open("/fixture/other.docx".into());
        state.verify_source(signature(2));
        let c = Rc::new(Stub::default());
        state.pump_with(|_| Box::new(c.clone()));
        complete(&c, signature(1), "WRONG");
        state.pump_with(no_start);
        assert!(matches!(state.view(), View::Error(_)));
        state.close();
        assert!(state.is_drained());
    }
    #[test]
    fn pdf_source_gate_discards_stale_a_and_checks_effective_b() {
        let mut state = State::new();
        state.open("/fixture/pages.pdf".into());
        assert!(state.source_target().is_some());
        assert!(!thumbnail_source(Path::new("/fixture/pages.pdf")));
        assert!(!state.pump_with(no_start));
        state.verify_source(signature(1));
        let a = Rc::new(Stub::default());
        state.pump_with(|_| Box::new(a.clone()));
        state.invalidate_source();
        assert!(a.cancelled.get());
        state.verify_source(signature(2));
        complete_pdf_source(&a, 1, signature(1));
        let b = Rc::new(Stub::default());
        state.pump_with(|_| Box::new(b.clone()));
        assert!(matches!(state.view(), View::Loading));
        complete_pdf_source(&b, 1, signature(2));
        state.pump_with(no_start);
        assert!(
            matches!(state.view(), View::Pdf(r) if r.frame.bitmap.source == Some(signature(2)))
        );
        state.change_pdf_page(2);
        let page = Rc::new(Stub::default());
        state.pump_with(|_| Box::new(page.clone()));
        complete_pdf_source(&page, 3, signature(1));
        state.pump_with(no_start);
        assert!(matches!(state.view(), View::Error(_)));
        state.invalidate_source();
        assert_eq!(state.pdf_page, 1);
        assert_eq!(state.pdf_pages, None);
        state.close();
        assert!(state.is_drained());
    }
    #[test]
    fn pdf_text_mode_latest_only_source_checked_and_returns_to_raster() {
        let mut state = State::new();
        state.open("/fixture/pages.pdf".into());
        state.verify_source(signature(1));
        let raster = Rc::new(Stub::default());
        state.pump_with(|_| Box::new(raster.clone()));
        complete_pdf(&raster, 1);
        state.pump_with(no_start);
        state.toggle_pdf_text();
        assert!(state.pdf_text());
        assert!(state.pdf_available());
        let a = Rc::new(Stub::default());
        state.pump_with(|_| Box::new(a.clone()));
        state.change_pdf_page(1);
        assert!(a.cancelled.get());
        *a.result.borrow_mut() = Some(Ok(Payload::PdfText(crate::pdf_preview_job::TextReady {
            text: "old page1".into(),
            source: signature(1),
            page: 1,
            pages: 3,
        })));
        let b = Rc::new(Stub::default());
        state.pump_with(|_| Box::new(b.clone()));
        assert!(matches!(state.view(), View::Loading));
        *b.result.borrow_mut() = Some(Ok(Payload::PdfText(crate::pdf_preview_job::TextReady {
            text: "Texto á\r\n".into(),
            source: signature(1),
            page: 2,
            pages: 3,
        })));
        state.pump_with(no_start);
        state.document_mut().unwrap().select_all();
        assert_eq!(state.document().unwrap().selected_text(), "Texto á\r\n");
        assert!(state.ready_mut().is_some());
        state.toggle_pdf_text();
        assert!(!state.pdf_text());
        let c = Rc::new(Stub::default());
        state.pump_with(|_| Box::new(c.clone()));
        complete_pdf(&c, 2);
        state.pump_with(no_start);
        assert!(matches!(state.view(), View::Pdf(_)));
        state.toggle_pdf_text();
        let d = Rc::new(Stub::default());
        state.pump_with(|_| Box::new(d.clone()));
        state.invalidate_source();
        state.verify_source(signature(2));
        *d.result.borrow_mut() = Some(Ok(Payload::PdfText(crate::pdf_preview_job::TextReady {
            text: "STALE".into(),
            source: signature(1),
            page: 2,
            pages: 3,
        })));
        let e = Rc::new(Stub::default());
        state.pump_with(|_| Box::new(e.clone()));
        assert!(matches!(state.view(), View::Loading));
        assert!(!state.pdf_text());
        state.close();
        complete_pdf_source(&e, 1, signature(2));
        state.pump_with(no_start);
        assert!(state.is_drained());
    }
    #[test]
    fn pdf_pages_latest_only_and_close_waits_for_reap() {
        let mut state = State::new();
        let first = Rc::new(Stub::default());
        state.open("/fixture/pages.pdf".into());
        state.verify_source(signature(1));
        state.pump_with(|_| Box::new(first.clone()));
        complete_pdf(&first, 1);
        state.pump_with(no_start);
        state.bitmap_viewport_mut().unwrap().rotate_by(1);
        state.bitmap_viewport_mut().unwrap().zoom_by(2.);
        state.change_pdf_page(-1);
        assert!(state.is_drained());
        state.change_pdf_page(1);
        let middle = Rc::new(Stub::default());
        state.pump_with(|_| Box::new(middle.clone()));
        state.change_pdf_page(i32::MAX);
        assert!(middle.cancelled.get());
        assert_eq!(state.pdf_page, 3);
        complete_pdf(&middle, 2);
        let last = Rc::new(Stub::default());
        state.pump_with(|_| Box::new(last.clone()));
        assert!(matches!(state.view(), View::Loading));
        complete_pdf(&last, 3);
        state.pump_with(no_start);
        assert!(
            matches!(state.view(),View::Pdf(r) if r.frame.page==3 && r.viewport.quarter_turns()==1 && r.viewport.zoom==2.)
        );
        state.change_pdf_page(i32::MIN);
        assert_eq!(state.pdf_page, 1);
        let closing = Rc::new(Stub::default());
        state.pump_with(|_| Box::new(closing.clone()));
        state.close();
        assert!(closing.cancelled.get());
        assert!(!state.is_drained());
        complete_pdf(&closing, 1);
        state.pump_with(no_start);
        assert!(state.is_drained());
        assert!(matches!(state.view(), View::Closed));
        state.change_pdf_page(1);
        assert!(state.is_drained());
    }
    fn complete_video(stub: &Stub, position: f64) {
        *stub.result.borrow_mut() = Some(Ok(Payload::Video(crate::video_preview_job::Ready {
            display_width: 1.0,
            display_height: 1.0,
            frame_rate: None,
            video_index: 0,
            audio: None,
            audio_notice: None,
            bitmap: Bitmap {
                source: None,
                width: 1,
                height: 1,
                source_width: 1,
                source_height: 1,
                format: "video".into(),
                rgb: vec![1, 2, 3],
            },
            position,
            duration: 8.0,
            codec: "ffv1".into(),
        })));
    }
    fn complete_audio(stub: &Stub) {
        *stub.result.borrow_mut() = Some(Ok(Payload::Audio(crate::audio_preview_job::Ready {
            source: signature(1),
            info: crate::audio_preview_job::Info {
                duration: 8.0,
                codec: "flac".into(),
                sample_rate: 48000,
                channels: 1,
            },
            preview: TextPreview {
                text: "audio metadata".into(),
                encoding: "Audio · metadatos".into(),
                truncated: false,
                relative_source: None,
            },
        })));
    }
    fn audio_state() -> State {
        let mut state = State::new();
        let meta = Rc::new(Stub::default());
        state.open("/fixture/song.flac".into());
        state.verify_source(signature(1));
        state.pump_with(|_| Box::new(meta.clone()));
        complete_audio(&meta);
        state.pump_with(no_start);
        state
    }
    #[test]
    fn audio_no_autoplay_pause_resume_seek_eof_and_close_are_single_flight() {
        let mut state = audio_state();
        assert!(!state.audio_playing());
        assert!(state.is_drained());
        state.toggle_audio();
        let play = Rc::new(Stub::default());
        state.pump_with(|_| Box::new(play.clone()));
        assert!(!state.audio_running());
        play.started.set(Some(
            std::time::Instant::now() - std::time::Duration::from_secs(2),
        ));
        state.pump_with(no_start);
        assert!(state.audio_running());
        assert!(state.audio_position() >= 2.0);
        state.toggle_audio();
        assert!(play.cancelled.get());
        assert!(!state.audio_playing());
        assert!(!state.is_drained());
        state.toggle_audio();
        state.pump_with(no_start); // resume waits for previous reap
        *play.result.borrow_mut() = Some(Err("cancelled".into()));
        let resumed = Rc::new(Stub::default());
        state.pump_with(|_| Box::new(resumed.clone()));
        assert!(state.audio_playing());
        assert!(state.audio_notice().is_none());
        state.seek_audio_to(6.0);
        assert!(resumed.cancelled.get());
        assert_eq!(state.audio_position(), 6.0);
        *resumed.result.borrow_mut() = Some(Ok(Payload::AudioPlaybackEnd(
            crate::audio_preview_job::PlaybackEnd::Complete,
        )));
        let seek = Rc::new(Stub::default());
        state.pump_with(|_| Box::new(seek.clone()));
        assert_eq!(state.audio_position(), 6.0);
        *seek.result.borrow_mut() = Some(Ok(Payload::AudioPlaybackEnd(
            crate::audio_preview_job::PlaybackEnd::Complete,
        )));
        state.pump_with(no_start);
        assert_eq!(state.audio_position(), 8.0);
        assert!(!state.audio_playing());
        assert!(state.is_drained());
        assert!(state.audio_info().is_some());
        state.toggle_audio();
        assert_eq!(state.audio_position(), 0.0);
        let closing = Rc::new(Stub::default());
        state.pump_with(|_| Box::new(closing.clone()));
        state.close();
        assert!(closing.cancelled.get());
        assert!(!state.exit_allowed());
        *closing.result.borrow_mut() = Some(Err("cancelled".into()));
        state.pump_with(no_start);
        assert!(state.is_drained());
        assert!(matches!(state.view(), View::Closed));
    }
    #[test]
    fn audio_device_failure_retains_retry_but_source_failure_hides_metadata() {
        let mut state = audio_state();
        state.toggle_audio();
        let play = Rc::new(Stub::default());
        state.pump_with(|_| Box::new(play.clone()));
        *play.result.borrow_mut() = Some(Err("No audio device".into()));
        state.pump_with(no_start);
        assert!(!state.audio_playing());
        assert_eq!(state.audio_notice(), Some("No audio device"));
        assert!(state.audio_info().is_some());
        state.toggle_audio();
        let retry = Rc::new(Stub::default());
        state.pump_with(|_| Box::new(retry.clone()));
        *retry.result.borrow_mut() = Some(Ok(Payload::AudioPlaybackEnd(
            crate::audio_preview_job::PlaybackEnd::SourceUnavailable("original gone".into()),
        )));
        state.pump_with(no_start);
        assert!(matches!(state.view(), View::Error(_)));
        assert!(state.source_blocked);
        assert!(state.document().is_none());
        state.pump_with(no_start);
        state.verify_source(signature(2));
        let meta = Rc::new(Stub::default());
        state.pump_with(|_| Box::new(meta.clone()));
        state.close();
        *meta.result.borrow_mut() = Some(Err("cancelled".into()));
        state.pump_with(no_start);
        assert!(state.is_drained());
    }
    #[test]
    fn readonly_audio_close_permission_does_not_claim_drained_and_never_replaces_it() {
        let mut state = State::new();
        let blocked = Rc::new(Stub::default());
        blocked.readonly.set(true);
        state.open("/fixture/song.flac".into());
        state.verify_source(signature(1));
        state.pump_with(|_| Box::new(blocked.clone()));
        assert!(!state.exit_allowed());
        state.close();
        assert!(blocked.cancelled.get());
        assert!(state.exit_allowed());
        assert!(state.readonly_pending());
        assert!(!state.is_drained());
        state.pump_with(no_start);
        assert!(state.exit_allowed());
        assert!(!state.is_drained());
        *blocked.result.borrow_mut() = Some(Err("stopped".into()));
        state.pump_with(no_start);
        assert!(state.is_drained());
        assert!(!state.readonly_pending());
        assert!(matches!(state.view(), View::Closed));
    }
    #[test]
    fn audio_proof_is_required_and_late_replacement_never_publishes_old_tags() {
        let mut state = State::new();
        let old = Rc::new(Stub::default());
        state.open("/fixture/song.flac".into());
        assert!(!crate::thumbnail_job::supported(Path::new(
            "/fixture/song.flac"
        )));
        state.pump_with(no_start);
        assert!(state.source_blocked);
        state.verify_source(signature(1));
        state.pump_with(|_| Box::new(old.clone()));
        state.invalidate_source();
        state.verify_source(signature(2));
        let audio = |source| {
            Payload::Audio(crate::audio_preview_job::Ready {
                source,
                info: crate::audio_preview_job::Info {
                    duration: 8.0,
                    codec: "flac".into(),
                    sample_rate: 48000,
                    channels: 1,
                },
                preview: TextPreview {
                    text: "old tags".into(),
                    encoding: "Audio · metadatos".into(),
                    truncated: false,
                    relative_source: None,
                },
            })
        };
        *old.result.borrow_mut() = Some(Ok(audio(signature(1))));
        let current = Rc::new(Stub::default());
        state.pump_with(|_| Box::new(current.clone()));
        assert!(matches!(state.view(), View::Loading));
        // Worker for B returning A is rejected even with the current task revision.
        *current.result.borrow_mut() = Some(Ok(audio(signature(1))));
        state.pump_with(no_start);
        assert!(matches!(state.view(), View::Error(_)));
        state.invalidate_source();
        state.verify_source(signature(2));
        let retry = Rc::new(Stub::default());
        state.pump_with(|_| Box::new(retry.clone()));
        *retry.result.borrow_mut() = Some(Ok(audio(signature(2))));
        state.pump_with(no_start);
        assert!(matches!(state.view(),View::Ready(r) if r.source==Some(signature(2))));
        state.close();
        assert!(state.is_drained());
        assert!(matches!(state.view(), View::Closed));
    }
    /// The pre-autoplay contract (paused first frame, muted) that the
    /// lifecycle tests below were written for; scripted checks keep it too.
    fn paused_muted() -> State {
        let mut state = State::new();
        state.set_autoplay(false);
        state.set_muted(true);
        state
    }
    fn video_with_audio() -> State {
        video_with_audio_in(paused_muted())
    }
    fn video_with_audio_in(mut state: State) -> State {
        let meta = Rc::new(Stub::default());
        state.open("/fixture/clip.mkv".into());
        state.source_blocked = false;
        state.pump_with(|_| Box::new(meta.clone()));
        complete_video(&meta, 0.0);
        state.pump_with(no_start);
        if let View::Video(r) = &mut state.view {
            r.frame.video_index = 2;
            r.frame.audio = Some(crate::video_preview_job::AudioTrack {
                index: 3,
                info: crate::audio_preview_job::Info {
                    duration: 8.0,
                    codec: "flac".into(),
                    sample_rate: 48000,
                    channels: 1,
                },
            });
        }
        state
    }
    #[test]
    fn video_autoplays_after_first_frame_and_seek_keeps_playing() {
        let mut state = State::new();
        let snapshot = Rc::new(Stub::default());
        state.open("/fixture/clip.mkv".into());
        state.source_blocked = false;
        state.pump_with(|_| Box::new(snapshot.clone()));
        complete_video(&snapshot, 0.0);
        // The first frame shows at once; playback starts on the next pump
        // with options computed from that frame (never the snapshot again).
        state.pump_with(no_start);
        assert!(state.video_playing());
        assert!(matches!(state.view(), View::Video(_)));
        let stream = Rc::new(Stub::default());
        let started = Cell::new(false);
        state.pump_with(|_| {
            started.set(true);
            Box::new(stream.clone())
        });
        assert!(started.get());
        // Seeking while playing shows the new frame and keeps playing.
        state.seek_video_to(3.0);
        assert!(stream.cancelled.get());
        *stream.result.borrow_mut() = Some(Err("cancelado".into()));
        let snapshot = Rc::new(Stub::default());
        state.pump_with(|_| Box::new(snapshot.clone()));
        complete_video(&snapshot, 3.0);
        state.pump_with(no_start);
        assert!(state.video_playing());
        // Paused, a seek stays paused.
        state.toggle_video();
        assert!(!state.video_playing());
    }
    #[test]
    fn mute_is_global_sound_by_default_and_audio_failure_plays_silently() {
        let mut state = video_with_audio_in(State::new());
        // Default: sound on, playing.
        assert!(state.video_audio() && !state.muted());
        assert!(state.video_playing());
        let stream = Rc::new(Stub::default());
        state.pump_with(|_| Box::new(stream.clone()));
        // The audio output fails: keep playing, silently, only for this file.
        *stream.result.borrow_mut() = Some(Err("Pulse unavailable".into()));
        state.pump_with(no_start);
        assert!(state.video_playing());
        assert!(state.video_audio_failed() && !state.video_audio() && !state.muted());
        let retry = Rc::new(Stub::default());
        let started = Cell::new(false);
        state.pump_with(|_| {
            started.set(true);
            Box::new(retry.clone())
        });
        assert!(started.get(), "silent retry started");
        // The user mutes: global, survives closing and opening another file.
        assert!(state.toggle_video_audio());
        assert!(state.muted());
        assert!(retry.cancelled.get());
        *retry.result.borrow_mut() = Some(Err("cancelado".into()));
        state.close();
        let mut state = video_with_audio_in(state);
        assert!(state.muted() && !state.video_audio() && !state.video_audio_failed());
        assert!(state.toggle_video_audio());
        assert!(state.video_audio());
    }
    #[test]
    fn video_sound_is_opt_in_and_live_toggle_waits_for_previous_reap() {
        let mut state = video_with_audio();
        assert!(!state.video_audio());
        state.toggle_video_audio();
        assert!(state.video_audio());
        assert!(state.is_drained());
        state.toggle_video();
        let old = Rc::new(Stub::default());
        state.pump_with(|_| Box::new(old.clone()));
        state.toggle_video_audio();
        assert!(!state.video_audio());
        assert!(old.cancelled.get());
        state.pump_with(no_start);
        *old.result.borrow_mut() = Some(Err("cancelled".into()));
        let muted = Rc::new(Stub::default());
        state.pump_with(|_| Box::new(muted.clone()));
        assert!(state.video_playing());
        assert!(matches!(state.view(),View::Video(r)if r.notice.is_none()));
        *muted.result.borrow_mut() = Some(Err("Pulse unavailable".into()));
        state.pump_with(no_start);
        assert!(!state.video_playing());
        assert!(
            matches!(state.view(),View::Video(r)if r.notice.as_deref()==Some("Pulse unavailable"))
        );
        state.toggle_video();
        let retry = Rc::new(Stub::default());
        state.pump_with(|_| Box::new(retry.clone()));
        state.close();
        *retry.result.borrow_mut() = Some(Err("cancelled".into()));
        state.pump_with(no_start);
        assert!(state.is_drained());
    }
    #[test]
    fn decoder_source_failure_hides_frame_before_ack_and_forces_new_probe_token() {
        let mut state = video_with_audio();
        state.toggle_video();
        let stream = Rc::new(Stub::default());
        state.pump_with(|_| Box::new(stream.clone()));
        let token = state.source_target().unwrap().0;
        stream.source_bad.set(true);
        state.pump_with(no_start);
        assert!(stream.cancelled.get());
        assert!(matches!(state.view(), View::Error(_)));
        assert_ne!(state.source_target().unwrap().0, token);
        assert!(state.source_blocked);
        let fresh = state.source_target().unwrap().0;
        state.pump_with(no_start);
        assert_eq!(
            state.source_target().unwrap().0,
            fresh,
            "do not repeatedly rearm same rejected job"
        );
        *stream.result.borrow_mut() = Some(Ok(Payload::PlaybackFinished));
        state.pump_with(no_start);
        assert!(matches!(state.view(), View::Error(_)));
        assert!(!state.video_audio());
        state.close();
        assert!(state.is_drained());
    }
    #[test]
    fn playback_updates_latest_frame_and_eof_keeps_paused_bitmap() {
        let mut state = paused_muted();
        let snapshot = Rc::new(Stub::default());
        state.open("/fixture/clip.mkv".into());
        // Isolate playback lifecycle from the separately tested source guard.
        state.source_blocked = false;
        state.pump_with(|_| Box::new(snapshot.clone()));
        complete_video(&snapshot, 0.0);
        state.pump_with(no_start);
        state.bitmap_viewport_mut().unwrap().zoom_by(2.);
        state.bitmap_viewport_mut().unwrap().pan_x = 48.;
        state.bitmap_viewport_mut().unwrap().rotate_by(1);
        state.bitmap_viewport_mut().unwrap().pan_x = 48.;
        state.toggle_video();
        assert!(state.video_playing());
        let stream = Rc::new(Stub::default());
        state.pump_with(|_| Box::new(stream.clone()));
        assert!(!state.is_drained());
        *stream.frame.borrow_mut() = Some(crate::video_stream_job::Frame {
            source: None,
            rgb: vec![9, 8, 7],
            width: 1,
            height: 1,
            position: 0.5,
            sequence: 6,
        });
        state.pump_with(no_start);
        assert!(
            matches!(state.view(),View::Video(r) if r.frame.bitmap.rgb==[9,8,7] && r.sequence==Some(6))
        );
        assert_eq!(state.video_position, 0.5);
        assert_eq!(state.bitmap_viewport_mut().unwrap().zoom, 2.);
        assert_eq!(state.bitmap_viewport_mut().unwrap().pan_x, 48.);
        assert_eq!(state.bitmap_viewport_mut().unwrap().quarter_turns(), 1);
        assert_eq!(
            *stream.recycled.borrow(),
            vec![vec![1, 2, 3]],
            "worker receives displaced snapshot ownership"
        );
        *stream.result.borrow_mut() = Some(Ok(Payload::PlaybackFinished));
        state.pump_with(no_start);
        assert!(state.is_drained());
        assert!(!state.video_playing());
        assert!(matches!(state.view(),View::Video(r) if r.frame.position==0.5));
    }
    #[test]
    fn pause_resume_and_close_drain_old_stream_before_replacement() {
        let mut state = paused_muted();
        let snapshot = Rc::new(Stub::default());
        state.open("/fixture/clip.mkv".into());
        // Isolate playback lifecycle from the separately tested source guard.
        state.source_blocked = false;
        state.pump_with(|_| Box::new(snapshot.clone()));
        complete_video(&snapshot, 0.0);
        state.pump_with(no_start);
        state.toggle_video();
        let old = Rc::new(Stub::default());
        state.pump_with(|_| Box::new(old.clone()));
        state.toggle_video();
        assert!(!state.video_playing());
        assert!(old.cancelled.get());
        state.toggle_video();
        assert!(state.video_playing());
        assert!(matches!(state.view(),View::Video(r) if r.sequence.is_none()));
        assert!(!state.pump_with(no_start));
        *old.frame.borrow_mut() = Some(crate::video_stream_job::Frame {
            source: None,
            rgb: vec![9; 3],
            width: 1,
            height: 1,
            position: 5.0,
            sequence: 60,
        });
        *old.result.borrow_mut() = Some(Err("cancelado".into()));
        let latest = Rc::new(Stub::default());
        state.pump_with(|_| Box::new(latest.clone()));
        assert!(state.video_playing());
        assert_eq!(state.video_position, 0.0);
        state.close();
        assert!(latest.cancelled.get());
        assert!(!state.is_drained());
        *latest.frame.borrow_mut() = Some(crate::video_stream_job::Frame {
            source: None,
            rgb: vec![9; 3],
            width: 1,
            height: 1,
            position: 5.0,
            sequence: 60,
        });
        *latest.result.borrow_mut() = Some(Err("cancelado".into()));
        state.pump_with(no_start);
        assert!(state.is_drained());
        assert!(matches!(state.view(), View::Closed));
    }
    #[test]
    fn video_seek_coalesces_releases_frame_rejects_stale_and_drains_close() {
        let mut state = State::new();
        let first = Rc::new(Stub::default());
        state.open("/fixture/clip.mkv".into());
        // Isolate playback lifecycle from the separately tested source guard.
        state.source_blocked = false;
        state.pump_with(|_| Box::new(first.clone()));
        complete_video(&first, 0.0);
        state.pump_with(no_start);
        assert!(matches!(state.view(), View::Video(_)));
        state.bitmap_viewport_mut().unwrap().zoom_by(2.);
        state.bitmap_viewport_mut().unwrap().rotate_by(1);
        state.bitmap_viewport_mut().unwrap().pan_y = 36.;
        state.seek_video_by(5.0);
        assert!(matches!(state.view(), View::Loading));
        let middle = Rc::new(Stub::default());
        state.pump_with(|_| Box::new(middle.clone()));
        state.seek_video_to(1.0);
        state.seek_video_to(6.0);
        assert!(middle.cancelled.get());
        assert!(!state.pump_with(no_start));
        complete_video(&middle, 5.0);
        let latest = Rc::new(Stub::default());
        state.pump_with(|_| Box::new(latest.clone()));
        assert!(matches!(state.view(), View::Loading));
        complete_video(&latest, 6.0);
        state.pump_with(no_start);
        assert!(
            matches!(state.view(),View::Video(v) if v.frame.position==6.0 && v.viewport.zoom==2. && v.viewport.pan_y==36. && v.viewport.quarter_turns()==1)
        );
        state.seek_video_to(f64::NAN);
        assert!(state.is_drained());
        state.seek_video_to(f64::MAX);
        assert_eq!(state.video_position, 7.95);
        let closing = Rc::new(Stub::default());
        state.pump_with(|_| Box::new(closing.clone()));
        state.close();
        assert!(closing.cancelled.get());
        assert!(!state.is_drained());
        complete_video(&closing, 7.95);
        state.pump_with(no_start);
        assert!(state.is_drained());
        assert!(matches!(state.view(), View::Closed));
        state.seek_video_to(2.0);
        assert!(state.is_drained());
        state.open("/fixture/A.txt".into());
        assert!(state.video_duration.is_none());
        assert_eq!(state.video_transform, Default::default());
    }
    #[test]
    fn html_resize_coalesces_and_close_drains_without_restarting_stale_layout() {
        let mut state = State::new();
        let first = Rc::new(Stub::default());
        state.open("/fixture/layout.html".into());
        state.verify_source(signature(1));
        state.pump_with(|_| Box::new(first.clone()));
        state.configure_viewport(700, 400);
        state.configure_viewport(800, 450);
        assert!(first.cancelled.get());
        assert!(!state.pump_with(no_start));
        complete_html(&first, 932, 460);
        let latest = Rc::new(Stub::default());
        state.pump_with(|_| Box::new(latest.clone()));
        assert_eq!(state.html_size, (800, 450));
        assert!(matches!(state.view(), View::Loading));
        complete_html(&latest, 800, 450);
        state.pump_with(no_start);
        assert!(matches!(state.view(),View::Html(r) if (r.width,r.height)==(800,450)));
        state.configure_viewport(700, 400);
        let closing = Rc::new(Stub::default());
        state.pump_with(|_| Box::new(closing.clone()));
        state.close();
        assert!(closing.cancelled.get());
        assert!(!state.is_drained());
        complete_html(&closing, 700, 400);
        state.pump_with(no_start);
        assert!(state.is_drained());
        assert!(matches!(state.view(), View::Closed));
    }
    #[test]
    fn contextual_parent_links_preserve_root_and_escape_does_not_spawn_or_replace() {
        let mut state = State::new();
        let task = Rc::new(Stub::default());
        state.open("/fixture/contextual-links.md".into());
        state.pump_with(|_| Box::new(task.clone()));
        complete_markdown(&task, "# Inicio");
        state.pump_with(no_start);
        state.verify_document(signature(9));
        state.follow_link(crate::preview_link::Target::Relative {
            path: "contextual/guide.md".into(),
            fragment: None,
        });
        assert_eq!(
            state.relative.as_deref(),
            Some(Path::new("contextual/guide.md"))
        );
        state.pump_with(|_| Box::new(task.clone()));
        complete_markdown(&task, "# Guía");
        state.pump_with(no_start);
        state.verify_document(signature(9));
        let revision = state.revision();
        state.follow_link(crate::preview_link::Target::Relative {
            path: "../../outside.md".into(),
            fragment: None,
        });
        assert_eq!(state.revision(), revision);
        assert!(state.is_drained());
        assert!(
            matches!(state.view(),View::Markdown(ready) if ready.notice.as_ref().is_some_and(|s|s.contains("bloqueado")))
        );
        state.follow_link(crate::preview_link::Target::Relative {
            path: "../contextual-links.md".into(),
            fragment: None,
        });
        assert_eq!(
            state.path(),
            Some(Path::new("/fixture/contextual-links.md"))
        );
        assert_eq!(state.root.as_deref(), Some(Path::new("/fixture")));
        assert_eq!(
            state.relative.as_deref(),
            Some(Path::new("contextual-links.md"))
        );
    }
    #[test]
    fn images_from_links_remain_explicitly_pending_without_spawn() {
        let mut state = State::new();
        let task = Rc::new(Stub::default());
        state.open("/fixture/overview.md".into());
        state.pump_with(|_| Box::new(task.clone()));
        complete_markdown(&task, "![alt](palette.png)");
        state.pump_with(no_start);
        state.verify_document(signature(9));
        state.follow_link(crate::preview_link::Target::Relative {
            path: "palette.png".into(),
            fragment: None,
        });
        assert!(matches!(state.view(),View::Markdown(ready) if ready.notice.is_some()));
        assert!(state.is_drained());
        assert!(!state.pump_with(no_start));
    }
    #[test]
    fn text_to_image_to_text_waits_for_reap_and_drops_bitmap_on_close() {
        let mut state = State::new();
        let first = Rc::new(Stub::default());
        state.open("a.txt".into());
        state.pump_with(|_| Box::new(first.clone()));
        state.open("b.png".into());
        // This test isolates helper reap; source authorization has its own tests.
        state.source_blocked = false;
        assert!(first.cancelled.get());
        assert!(!state.pump_with(no_start));
        complete(&first, "stale");
        let image = Rc::new(Stub::default());
        state.pump_with(|_| Box::new(image.clone()));
        *image.result.borrow_mut() = Some(Ok(Payload::Image(Bitmap {
            source: None,
            width: 1,
            height: 1,
            source_width: 1,
            source_height: 1,
            format: "png".into(),
            rgb: vec![1, 2, 3],
        })));
        state.pump_with(no_start);
        assert!(matches!(state.view(), View::Image(_)));
        assert!(state.is_drained());
        state.close();
        assert!(matches!(state.view(), View::Closed));
        assert!(state.is_drained());
    }
    #[test]
    fn rapid_navigation_keeps_one_worker_and_only_latest_target() {
        let mut state = State::new();
        let first = Rc::new(Stub::default());
        state.open("a".into());
        state.pump_with(|path| {
            assert_eq!(path, Path::new("a"));
            Box::new(first.clone())
        });
        state.open("b".into());
        state.open("c".into());
        assert!(first.cancelled.get());
        assert!(!state.is_drained());
        assert!(!state.pump_with(no_start));
        complete(&first, "stale a");
        let last = Rc::new(Stub::default());
        state.pump_with(|path| {
            assert_eq!(path, Path::new("c"));
            Box::new(last.clone())
        });
        assert!(matches!(state.view(), View::Loading));
        complete(&last, "current c");
        state.pump_with(no_start);
        assert_eq!(state.ready_mut().unwrap().document.text(), "current c");
        assert!(state.is_drained());
        assert!(!state.pump_with(no_start));
    }
    #[test]
    fn close_waits_for_ack_without_reopening_or_displaying_stale_error() {
        let mut state = State::new();
        let task = Rc::new(Stub::default());
        state.open("a".into());
        state.pump_with(|_| Box::new(task.clone()));
        state.open("b".into());
        state.close();
        assert!(!state.visible());
        assert!(!state.is_drained());
        *task.result.borrow_mut() = Some(Err("cancelled".into()));
        state.pump_with(no_start);
        assert!(state.is_drained());
        assert!(matches!(state.view(), View::Closed));
    }
    #[test]
    fn reopening_same_path_rejects_old_generation() {
        let mut state = State::new();
        let old = Rc::new(Stub::default());
        state.open("same".into());
        state.pump_with(|_| Box::new(old.clone()));
        state.close();
        state.open("same".into());
        complete(&old, "old");
        let new = Rc::new(Stub::default());
        state.pump_with(|_| Box::new(new.clone()));
        assert!(matches!(state.view(), View::Loading));
        complete(&new, "new");
        state.pump_with(no_start);
        assert_eq!(state.ready_mut().unwrap().document.text(), "new");
    }
    #[test]
    fn current_error_and_document_limits_are_visible_and_retryable() {
        let mut state = State::new();
        let task = Rc::new(Stub::default());
        state.open("a".into());
        state.pump_with(|_| Box::new(task.clone()));
        *task.result.borrow_mut() = Some(Err("read failed".into()));
        state.pump_with(no_start);
        assert!(matches!(state.view(),View::Error(message) if message=="read failed"));
        state.open("a".into());
        state.pump_with(|_| Box::new(task.clone()));
        complete(&task, &"\n".repeat(2001));
        state.pump_with(no_start);
        assert!(matches!(state.view(), View::Error(_)));
        assert!(state.is_drained());
    }
}
