//! Optional software video stream using one direct system ffmpeg process.
//!
//! Root owns the preview's single-flight lifecycle: cancel and drain Finished
//! before replacing this job. The worker owns at most a partial RGB frame and
//! one replaceable latest frame; a bitmap already taken by the UI is additional.
//! Input pacing (-re) and a 12fps output filter prevent unbounded turbo decoding.
//! Position is an estimate from the clamped seek plus output sequence/12, NOT a
//! decoded presentation timestamp. Audio is opt-in in the same child; no hardware
//! acceleration. GIF ignores source loops and plays one bounded pass.
//!
//! RLIMIT_AS768MiB, per-allocation32MiB, CPU/wall deadlines and bounded pipes are
//! containment, NOT PSS limits or proof of the application's150MB resource gate.
//! This is not a codec sandbox. Parent-directory symlinks/mounts, in-place input
//! mutation after the last revision check and decoder vulnerabilities are not
//! an atomic snapshot or sandbox guarantee. Checked FD/name revisions and the
//! visible-source watcher detect observable changes. A source
//! that cannot produce an output frame within5s is explicitly rejected, including
//! very sparse/VFR material. Seven-day metadata is accepted, but a one-day CPU
//! hard cap can stop unusually expensive long playback before its declared end.
use crate::readonly_gate::{Gate, Phase, admit_helper};
use std::{
    io,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
};

const FPS: u32 = 12;
const MAX_FRAME_BYTES: usize = 960 * 540 * 3;
const MAX_STDERR: usize = 64 * 1024;
use crate::video_preview_job::{
    CODECS, codec_whitelist, demuxer, duration_limit, input_byte_limit, pixel_limit,
};

#[derive(Debug)]
pub struct Frame {
    pub rgb: Vec<u8>,
    pub source: Option<files_core::media_source::Revision>,
    /// Estimated requested timeline position, not the source frame's PTS.
    pub position: f64,
    pub sequence: u64,
}
enum Slot {
    Ready(Frame),
    Recycled(Vec<u8>),
}
impl Slot {
    fn into_rgb(self) -> Vec<u8> {
        match self {
            Self::Ready(frame) => frame.rgb,
            Self::Recycled(rgb) => rgb,
        }
    }
}
pub struct Job {
    gate: Arc<Mutex<Gate>>,
    cancel: Arc<AtomicBool>,
    latest: Arc<Mutex<Option<Slot>>>,
    frame_bytes: usize,
    finished: Receiver<Result<(), String>>,
    reported_finished: AtomicBool,
    source_invalid: Arc<AtomicBool>,
}
pub struct Options {
    pub width: u32,
    pub height: u32,
    pub source_width: u32,
    pub source_height: u32,
    pub codec: String,
    pub duration: f64,
    pub video_index: u32,
    pub audio: Option<crate::video_preview_job::AudioTrack>,
}
struct Input {
    video_index: u32,
    audio: Option<crate::video_preview_job::AudioTrack>,
    expected: Option<files_core::media_source::Revision>,
    path: PathBuf,
    position: f64,
    width: u32,
    height: u32,
    source_width: u32,
    source_height: u32,
    codec: String,
    duration: f64,
}
impl Job {
    pub fn start(
        path: PathBuf,
        position: f64,
        expected: Option<files_core::media_source::Revision>,
        options: Options,
    ) -> Self {
        let Options {
            width,
            height,
            source_width,
            source_height,
            codec,
            duration,
            video_index,
            audio,
        } = options;
        let gate = Arc::new(Mutex::new(Gate {
            phase: Phase::ReadOnly,
            closing: false,
        }));
        let state = gate.clone();
        let source_invalid = Arc::new(AtomicBool::new(false));
        let source_flag = source_invalid.clone();
        let frame_bytes = frame_bytes(width, height).unwrap_or(0);
        let cancel = Arc::new(AtomicBool::new(false));
        let latest = Arc::new(Mutex::new(None));
        let flag = Arc::clone(&cancel);
        let slot = Arc::clone(&latest);
        let (tx, finished) = mpsc::sync_channel(1);
        let input = Input {
            video_index,
            audio,
            expected,
            path,
            position,
            width,
            height,
            source_width,
            source_height,
            codec,
            duration,
        };
        std::thread::spawn(move || {
            crate::readonly_gate::hold_fixture("video_stream");
            let result =
                run(input, &flag, &slot, &source_flag, &state).map_err(|error| error.to_string());
            // Error/cancel ACK is after reap; release an unpublished frame too.
            if result.is_err()
                && let Ok(mut latest) = slot.lock()
            {
                *latest = None;
            }
            if let Ok(mut g) = state.lock() {
                g.phase = Phase::Stopped;
            }
            let _ = tx.send(result);
        });
        Self {
            gate,
            cancel,
            latest,
            frame_bytes,
            finished,
            reported_finished: AtomicBool::new(false),
            source_invalid,
        }
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
        if let Ok(mut g) = self.gate.lock() {
            g.closing = true;
        }
    }
    pub fn exit_allowed(&self) -> bool {
        self.gate
            .lock()
            .is_ok_and(|g| g.closing && g.phase != Phase::Helper)
    }
    pub fn readonly_pending(&self) -> bool {
        self.gate.lock().is_ok_and(|g| g.phase == Phase::ReadOnly)
    }
    pub fn source_invalid(&self) -> bool {
        self.source_invalid.load(Ordering::Acquire)
    }
    pub fn poll_frame(&self) -> Option<Frame> {
        if self.source_invalid() {
            return None;
        }
        take_ready(&self.latest)
    }
    /// Return the displaced UI allocation without retaining a second slot.
    /// If a newer ready frame won the race, keep it and drop this allocation.
    pub fn recycle_rgb(&self, rgb: Vec<u8>) {
        recycle_rgb(&self.latest, rgb, self.frame_bytes);
    }
    pub fn poll_finished(&self) -> Option<Result<(), String>> {
        if self.reported_finished.load(Ordering::Acquire) {
            return None;
        }
        let result = match self.finished.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Err("La reproducción terminó sin confirmación del trabajador".into())
            }
        };
        self.reported_finished.store(true, Ordering::Release);
        Some(result)
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancel();
    }
}
fn invalid(message: &str) -> io::Error {
    io::Error::other(message)
}
fn frame_bytes(width: u32, height: u32) -> io::Result<usize> {
    if width == 0 || height == 0 || width > 960 || height > 540 {
        return Err(invalid("Frame de vídeo fuera del límite960×540"));
    }
    (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(3))
        .filter(|n| *n <= MAX_FRAME_BYTES)
        .ok_or_else(|| invalid("Tamaño de frame no válido"))
}
fn validate(input: &Input) -> io::Result<(&'static str, usize)> {
    let demuxer = demuxer(&input.path)?;
    let bytes = frame_bytes(input.width, input.height)?;
    if input.source_width == 0
        || input.source_height == 0
        || input.source_width > 16384
        || input.source_height > 16384
        || u64::from(input.source_width) * u64::from(input.source_height) > pixel_limit(demuxer)
    {
        return Err(invalid("Dimensiones de origen fuera del límite"));
    }
    if !(if demuxer == "gif" {
        input.codec == "gif"
    } else {
        CODECS.split(',').any(|codec| codec == input.codec)
    }) {
        return Err(invalid("Códec de reproducción no admitido"));
    }
    if !input.duration.is_finite()
        || input.duration <= 0.0
        || input.duration > duration_limit(demuxer)
        || !input.position.is_finite()
        || input.position < 0.0
        || input.position >= input.duration
    {
        return Err(invalid("Duración o posición de reproducción no válida"));
    }
    if input.video_index > 255 {
        return Err(invalid("Índice de vídeo fuera del límite"));
    }
    if demuxer == "gif" && input.audio.is_some() {
        return Err(invalid("GIF no admite pistas de audio"));
    }
    if let Some(audio) = &input.audio {
        if audio.index > 255 || audio.index == input.video_index {
            return Err(invalid("Índice de audio no válido"));
        }
        crate::audio_preview_job::validate_play(&audio.info, 0.0)?;
    }
    Ok((demuxer, bytes))
}
/// Owns exactly one partial frame. New buffers are admitted before allocation.
struct Assembler {
    rgb: Vec<u8>,
    filled: usize,
    expected: usize,
}
impl Assembler {
    fn new(width: u32, height: u32) -> io::Result<Self> {
        let expected = frame_bytes(width, height)?;
        let mut result = Self {
            rgb: Vec::new(),
            filled: 0,
            expected,
        };
        result.recycle(None)?;
        Ok(result)
    }
    fn recycle(&mut self, previous: Option<Vec<u8>>) -> io::Result<()> {
        self.rgb = previous.unwrap_or_default();
        if self.rgb.is_empty() {
            self.rgb
                .try_reserve_exact(self.expected)
                .map_err(|_| invalid("No hay memoria para el frame de vídeo"))?;
            self.rgb.resize(self.expected, 0);
        }
        if self.rgb.len() != self.expected {
            return Err(invalid("Buffer de reproducción no válido"));
        }
        self.filled = 0;
        Ok(())
    }
    fn advance(&mut self, bytes: usize) -> io::Result<bool> {
        self.filled = self
            .filled
            .checked_add(bytes)
            .filter(|n| *n <= self.expected)
            .ok_or_else(|| invalid("Frame de vídeo demasiado grande"))?;
        Ok(self.filled == self.expected)
    }
    fn eof(&self) -> io::Result<()> {
        if self.filled == 0 {
            Ok(())
        } else {
            Err(invalid("Frame de vídeo incompleto al terminar"))
        }
    }
    fn take(&mut self, position: f64, sequence: u64) -> Frame {
        Frame {
            source: None,
            rgb: std::mem::take(&mut self.rgb),
            position,
            sequence,
        }
    }
}
fn take_ready(latest: &Mutex<Option<Slot>>) -> Option<Frame> {
    let mut slot = latest.lock().ok()?;
    match slot.take() {
        Some(Slot::Ready(frame)) => Some(frame),
        recycled => {
            *slot = recycled;
            None
        }
    }
}
fn recycle_rgb(latest: &Mutex<Option<Slot>>, rgb: Vec<u8>, expected: usize) {
    if expected == 0 || rgb.len() != expected || rgb.capacity() > MAX_FRAME_BYTES {
        return;
    }
    if let Ok(mut slot) = latest.lock()
        && slot.is_none()
    {
        *slot = Some(Slot::Recycled(rgb));
    }
}
fn publish(latest: &Mutex<Option<Slot>>, frame: Frame) -> io::Result<Option<Vec<u8>>> {
    latest
        .lock()
        .map_err(|_| invalid("El buffer de vídeo no está disponible"))
        .map(|mut slot| slot.replace(Slot::Ready(frame)).map(Slot::into_rgb))
}
fn source_result<T>(flag: &AtomicBool, result: io::Result<T>) -> io::Result<T> {
    if result.is_err() {
        flag.store(true, Ordering::Release);
    }
    result
}
fn cancelled(cancel: &AtomicBool) -> io::Result<()> {
    if cancel.load(Ordering::Acquire) {
        Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "Reproducción cancelada",
        ))
    } else {
        Ok(())
    }
}
#[cfg(target_os = "linux")]
fn nonblocking(fd: std::os::fd::RawFd) -> io::Result<()> {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
#[cfg(target_os = "linux")]
fn command(
    input: &Input,
    demuxer: &str,
    fd: std::os::fd::RawFd,
) -> io::Result<std::process::Command> {
    use std::os::unix::process::CommandExt;
    if !(3..256).contains(&fd) {
        return Err(invalid("Demasiados descriptores para reproducir vídeo"));
    }
    let cpu_seconds =
        (((input.duration - input.position).ceil() * 2.0 + 10.0) as u64).clamp(10, 86400);
    let mut command = std::process::Command::new("ffmpeg");
    // Bound glibc arena reservations in this child so AV+Pulse fits the SAME
    // virtual AS cap. No claim about whole-process/tree PSS or throughput.
    command.env_remove("FFREPORT").env("MALLOC_ARENA_MAX", "2");
    command.args([
        "-v",
        "error",
        "-nostdin",
        "-max_alloc",
        "33554432",
        "-threads",
        "1",
        "-filter_threads",
        "1",
        "-hwaccel",
        "none",
        "-re",
        "-protocol_whitelist",
        "file,pipe",
        "-format_whitelist",
        demuxer,
        "-probesize",
        "5242880",
        "-analyzeduration",
        "2000000",
        "-max_pixels",
        &pixel_limit(demuxer).to_string(),
        "-codec_whitelist",
        &codec_whitelist(demuxer),
        "-f",
        demuxer,
    ]);
    if demuxer == "mov" {
        command.args(["-enable_drefs", "0", "-use_absolute_path", "0"]);
    } else if demuxer == "gif" {
        command.args(["-ignore_loop", "1"]);
    }
    command.args([&format!("-c:{}", input.video_index), &input.codec]);
    if let Some(audio) = &input.audio {
        command.args([&format!("-c:{}", audio.index), &audio.info.codec]);
    }
    command.args([
        "-ss",
        &format!("{:.6}", input.position),
        "-i",
        &format!("/proc/self/fd/{fd}"),
        "-map",
        &format!("0:{}", input.video_index),
        "-an",
        "-sn",
        "-dn",
        "-t",
        &format!("{:.6}", input.duration - input.position),
        "-vf",
        &format!(
            "fps={FPS},scale={}:{}:flags=bilinear,setsar=1",
            input.width, input.height
        ),
        "-threads",
        "1",
        "-c:v",
        "rawvideo",
        "-pix_fmt",
        "rgb24",
        "-f",
        "rawvideo",
        "pipe:1",
    ]);
    if let Some(audio) = &input.audio {
        crate::audio_preview_job::append_output(
            &mut command,
            Some(audio.index),
            input.duration - input.position,
        )?;
    }
    // Only the existing pinned FD crosses exec; no shell/secondary process.
    unsafe {
        command.pre_exec(move || {
            let flags = libc::fcntl(fd, libc::F_GETFD);
            if flags < 0 || libc::fcntl(fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC) < 0 {
                return Err(io::Error::last_os_error());
            }
            for (resource, value) in [
                (libc::RLIMIT_AS, 768 * 1024 * 1024),
                (libc::RLIMIT_CPU, cpu_seconds),
                (libc::RLIMIT_NOFILE, u64::from(fd as u32 + 8).max(64)),
                (libc::RLIMIT_CORE, 0),
            ] {
                let limit = libc::rlimit {
                    rlim_cur: value,
                    rlim_max: value,
                };
                if libc::setrlimit(resource, &limit) != 0 {
                    return Err(io::Error::last_os_error());
                }
            }
            Ok(())
        });
    }
    Ok(command)
}
#[cfg(target_os = "linux")]
fn failure(stderr: &[u8], truncated: bool) -> io::Error {
    // Bounded UI diagnostic, no unbounded lossy conversion of hostile output.
    let text = String::from_utf8_lossy(&stderr[..stderr.len().min(4096)]);
    let mut message = String::from("No se pudo reproducir el vídeo: ");
    for ch in text.chars() {
        message.push(if ch.is_control() && !matches!(ch, '\n' | '\t') {
            ' '
        } else {
            ch
        });
    }
    if truncated || stderr.len() > 4096 {
        message.push_str(" [diagnóstico truncado]");
    }
    invalid(&message)
}
#[cfg(target_os = "linux")]
fn run(
    input: Input,
    cancel: &AtomicBool,
    latest: &Mutex<Option<Slot>>,
    source_invalid: &AtomicBool,
    gate: &Mutex<Gate>,
) -> io::Result<()> {
    use std::{
        fs::OpenOptions,
        io::Read,
        os::{fd::AsRawFd, unix::fs::OpenOptionsExt},
        process::Stdio,
        time::{Duration, Instant},
    };
    let (demuxer, _) = validate(&input)?;
    cancelled(cancel)?;
    if input
        .expected
        .is_some_and(|r| r.bytes == 0 || r.bytes > input_byte_limit(demuxer))
    {
        return Err(invalid("Medio fuera del límite de bytes"));
    }
    let file = source_result(
        source_invalid,
        OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
            .open(&input.path),
    )?;
    let revision = source_result(
        source_invalid,
        files_core::media_source::Revision::from_file(&file),
    )?;
    if input.expected.is_some_and(|expected| expected != revision) {
        source_invalid.store(true, Ordering::Release);
        return Err(invalid("El vídeo cambió antes de reproducir"));
    }
    source_result(source_invalid, revision.revalidate(&file, &input.path))?;
    let status = source_result(source_invalid, file.metadata())?;
    if !status.is_file() || status.len() == 0 || status.len() > input_byte_limit(demuxer) {
        source_invalid.store(true, Ordering::Release);
        return Err(invalid(
            "Medio vacío, no regular o sobre su límite de bytes",
        ));
    }
    cancelled(cancel)?;
    let mut decoder = command(&input, demuxer, file.as_raw_fd())?;
    admit_helper(gate)?;
    if std::env::var_os("FILES_TEST_PREVIEW").is_some() {
        eprintln!("FILES_VIDEO_CHILD_ADMIT role=video_stream");
    }
    crate::owned_child::arm(&mut decoder);
    let mut child = decoder
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                invalid("Reproducción no disponible: requiere ffmpeg del sistema")
            } else {
                error
            }
        })?;
    if std::env::var_os("FILES_TEST_PREVIEW").is_some() {
        let stat = std::fs::read_to_string(format!("/proc/{}/stat", child.id())).ok();
        let start = stat
            .as_deref()
            .and_then(|s| s.rsplit_once(')'))
            .and_then(|(_, s)| s.split_whitespace().nth(19))
            .unwrap_or("unknown");
        eprintln!(
            "FILES_PREVIEW_HELPER pid={} start={} command=preview_video_stream audio={} video_index={} audio_index={:?}",
            child.id(),
            start,
            input.audio.is_some(),
            input.video_index,
            input.audio.as_ref().map(|a| a.index)
        );
    }
    let result = (|| {
        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| invalid("Sin frames de vídeo"))?;
        let mut stderr = child
            .stderr
            .take()
            .ok_or_else(|| invalid("Sin diagnóstico de vídeo"))?;
        nonblocking(stdout.as_raw_fd())?;
        nonblocking(stderr.as_raw_fd())?;
        let mut assembler = Assembler::new(input.width, input.height)?;
        let mut diagnostic = Vec::new();
        diagnostic
            .try_reserve_exact(MAX_STDERR)
            .map_err(|_| invalid("No hay memoria para el diagnóstico de vídeo"))?;
        let mut diagnostic_truncated = false;
        let (mut output_eof, mut stderr_eof) = (false, false);
        let mut exited = None;
        let started = Instant::now();
        let mut last_frame = started;
        let mut sequence = 0u64;
        let frame_limit = ((input.duration - input.position) * f64::from(FPS)).ceil() as u64 + 2;
        loop {
            cancelled(cancel)?;
            if started.elapsed().as_secs_f64() > input.duration - input.position + 10.0 {
                return Err(invalid("La reproducción superó su duración permitida"));
            }
            if last_frame.elapsed() >= Duration::from_secs(5) {
                return Err(invalid(if sequence == 0 {
                    "El vídeo no produjo un frame en5s"
                } else {
                    "El vídeo dejó de producir frames durante5s"
                }));
            }
            // Fair quotas avoid stdout or a stderr flood starving the other pipe,
            // cancellation checks, child status or the monotonic deadlines.
            for _ in 0..16 {
                if stderr_eof {
                    break;
                }
                cancelled(cancel)?;
                let mut bytes = [0u8; 4096];
                match stderr.read(&mut bytes) {
                    Ok(0) => {
                        stderr_eof = true;
                        break;
                    }
                    Ok(n) => {
                        let retained = n.min(MAX_STDERR - diagnostic.len());
                        diagnostic.extend_from_slice(&bytes[..retained]);
                        diagnostic_truncated |= retained < n;
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(error) => return Err(error),
                }
            }
            for _ in 0..16 {
                if output_eof {
                    break;
                }
                cancelled(cancel)?;
                match stdout.read(&mut assembler.rgb[assembler.filled..]) {
                    Ok(0) => {
                        output_eof = true;
                        break;
                    }
                    Ok(n) => {
                        if assembler.advance(n)? {
                            if sequence >= frame_limit {
                                return Err(invalid("El vídeo produjo demasiados frames"));
                            }
                            let position = (input.position + sequence as f64 / f64::from(FPS))
                                .min(input.duration);
                            source_result(source_invalid, revision.revalidate(&file, &input.path))?;
                            let mut frame = assembler.take(position, sequence);
                            frame.source = Some(revision);
                            let previous = publish(latest, frame)?;
                            assembler.recycle(previous)?;
                            sequence += 1;
                            last_frame = Instant::now();
                        }
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(error) => return Err(error),
                }
            }
            if exited.is_none() {
                exited = child.try_wait()?;
            }
            if output_eof
                && stderr_eof
                && let Some(status) = exited
            {
                if !status.success() {
                    return Err(failure(&diagnostic, diagnostic_truncated));
                }
                // try_wait yielded Some(status), so the child is already reaped.
                if let Ok(mut g) = gate.lock() {
                    g.phase = Phase::ReadOnly;
                }
                assembler.eof()?;
                return if sequence == 0 {
                    Err(invalid("El vídeo terminó sin frames"))
                } else {
                    source_result(source_invalid, revision.revalidate(&file, &input.path))
                };
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    })();
    // Even failure during pipe setup/read/cancel reaps the direct process. The
    // result is published only after this ACK boundary, never from Drop alone.
    let _ = child.kill();
    child.wait()?;
    if let Ok(mut g) = gate.lock() {
        g.phase = Phase::ReadOnly;
    }
    result
}
#[cfg(not(target_os = "linux"))]
fn run(
    _: Input,
    _: &AtomicBool,
    _: &Mutex<Option<Slot>>,
    _: &AtomicBool,
    _: &Mutex<Gate>,
) -> io::Result<()> {
    Err(invalid(
        "Reproducción de vídeo pendiente en esta plataforma",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::video_preview_job::MAX_DURATION;
    fn input() -> Input {
        Input {
            video_index: 0,
            audio: None,
            expected: None,
            path: PathBuf::from("/fixture/clip.mp4"),
            position: 0.0,
            width: 2,
            height: 1,
            source_width: 1920,
            source_height: 1080,
            codec: "h264".into(),
            duration: 8.0,
        }
    }
    #[test]
    fn caps_reject_before_frame_allocation() {
        for (width, height) in [(0, 1), (1, 0), (961, 1), (1, 541), (u32::MAX, u32::MAX)] {
            assert!(Assembler::new(width, height).is_err());
        }
        assert_eq!(frame_bytes(960, 540).unwrap(), MAX_FRAME_BYTES);
        for duration in [f64::NAN, f64::INFINITY, 0.0, -1.0, MAX_DURATION + 1.0] {
            let mut i = input();
            i.duration = duration;
            assert!(validate(&i).is_err());
        }
        for position in [f64::NAN, f64::INFINITY, -1.0, 8.0] {
            let mut i = input();
            i.position = position;
            assert!(validate(&i).is_err());
        }
        let mut i = input();
        i.codec = "avisynth".into();
        assert!(validate(&i).is_err());
        let mut i = input();
        i.source_width = 16384;
        i.source_height = 16384;
        assert!(validate(&i).is_err());
        for path in [
            "relative.mp4",
            "https://host/clip.mp4",
            "/fixture/clip.m3u8",
            "/fixture/clip.concat",
        ] {
            let mut i = input();
            i.path = path.into();
            assert!(validate(&i).is_err());
        }
    }
    #[test]
    fn assembler_partial_frames_exact_eof_and_overflow() {
        let mut assembler = Assembler::new(2, 1).unwrap();
        assert!(assembler.eof().is_ok());
        assert!(!assembler.advance(1).unwrap());
        assert!(assembler.eof().is_err());
        assert!(!assembler.advance(2).unwrap());
        assert!(assembler.advance(3).unwrap());
        let frame = assembler.take(2.0, 4);
        assert_eq!(frame.rgb.len(), 6);
        assert_eq!((frame.position, frame.sequence), (2.0, 4));
        assembler.recycle(Some(frame.rgb)).unwrap();
        assert!(assembler.eof().is_ok());
        assert!(assembler.advance(7).is_err());
    }
    #[test]
    fn latest_slot_replaces_recycles_and_never_accumulates_frames() {
        let latest = Mutex::new(None);
        let mut assembler = Assembler::new(2, 1).unwrap();
        let first_pointer = assembler.rgb.as_ptr();
        assembler.advance(6).unwrap();
        let previous = publish(&latest, assembler.take(0.0, 0)).unwrap();
        assert!(previous.is_none());
        assembler.recycle(previous).unwrap();
        assembler.advance(6).unwrap();
        let previous = publish(&latest, assembler.take(1.0 / 12.0, 1)).unwrap();
        assert_eq!(previous.as_ref().unwrap().as_ptr(), first_pointer);
        assembler.recycle(previous).unwrap();
        assert_eq!(assembler.rgb.as_ptr(), first_pointer);
        assert_eq!(take_ready(&latest).unwrap().sequence, 1);
        assert!(latest.lock().unwrap().is_none());
    }
    #[test]
    fn ui_returns_displaced_buffer_and_reuses_it_without_losing_newer_frames() {
        let latest = Mutex::new(None);
        let mut assembler = Assembler::new(2, 1).unwrap();
        let mut displayed = vec![42; 6];
        let mut pointers = std::collections::HashSet::new();
        pointers.insert(displayed.as_ptr() as usize);
        for sequence in 0..100 {
            pointers.insert(assembler.rgb.as_ptr() as usize);
            assembler.advance(6).unwrap();
            let previous = publish(&latest, assembler.take(sequence as f64, sequence)).unwrap();
            assembler.recycle(previous).unwrap();
            let frame = take_ready(&latest).unwrap();
            let old = std::mem::replace(&mut displayed, frame.rgb);
            recycle_rgb(&latest, old, 6);
            assert!(
                take_ready(&latest).is_none(),
                "returned RGB is not a new frame"
            );
        }
        assert_eq!(
            pointers.len(),
            3,
            "partial + slot + displayed; no per-frame allocations"
        );
        // A frame published before the return wins; recycling never erases it.
        assembler.advance(6).unwrap();
        let previous = publish(&latest, assembler.take(101.0, 101)).unwrap();
        assembler.recycle(previous).unwrap();
        recycle_rgb(&latest, displayed, 6);
        assert_eq!(take_ready(&latest).unwrap().sequence, 101);
        // Reject wrong dimensions/capacity before retaining a returned buffer.
        recycle_rgb(&latest, vec![0; 3], 6);
        assert!(latest.lock().unwrap().is_none());
        let mut oversized = Vec::with_capacity(MAX_FRAME_BYTES + 1);
        oversized.resize(6, 0);
        recycle_rgb(&latest, oversized, 6);
        assert!(latest.lock().unwrap().is_none());
    }
    #[test]
    #[cfg(target_os = "linux")]
    fn av_command_maps_validated_absolute_tracks_and_has_one_process_two_outputs() {
        let mut i = input();
        i.video_index = 2;
        i.audio = Some(crate::video_preview_job::AudioTrack {
            index: 3,
            info: crate::audio_preview_job::Info {
                duration: 8.0,
                codec: "flac".into(),
                sample_rate: 48000,
                channels: 1,
            },
        });
        assert!(validate(&i).is_ok());
        let cmd = command(&i, "mov", 10).unwrap();
        let args = cmd
            .get_args()
            .map(|v| v.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(args.windows(2).any(|w| w == ["-c:2", "h264"]));
        assert!(args.windows(2).any(|w| w == ["-c:3", "flac"]));
        assert!(args.windows(2).any(|w| w == ["-map", "0:2"]));
        assert!(args.windows(2).any(|w| w == ["-map", "0:3"]));
        assert!(args.windows(2).any(|w| w == ["-f", "pulse"]));
        assert_eq!(args.iter().filter(|v| *v == "pipe:1").count(), 1);
        i.audio = None;
        let cmd = command(&i, "mov", 10).unwrap();
        assert!(!cmd.get_args().any(|a| a == "pulse"));
        i.audio = Some(crate::video_preview_job::AudioTrack {
            index: 2,
            info: crate::audio_preview_job::Info {
                duration: 8.0,
                codec: "flac".into(),
                sample_rate: 48000,
                channels: 1,
            },
        });
        assert!(validate(&i).is_err());
        for fd in [0, 1, 2, 256] {
            assert!(command(&i, "mov", fd).is_err());
        }
    }
    #[test]
    fn source_errors_have_an_explicit_flag_without_parsing_diagnostics() {
        let flag = AtomicBool::new(false);
        assert_eq!(source_result(&flag, Ok(4)).unwrap(), 4);
        assert!(!flag.load(Ordering::Acquire));
        assert!(source_result::<()>(&flag, Err(invalid("source"))).is_err());
        assert!(flag.load(Ordering::Acquire));
    }
    #[test]
    fn cancellation_rejects_progress_without_reinterpreting_as_eof() {
        let flag = AtomicBool::new(false);
        assert!(cancelled(&flag).is_ok());
        flag.store(true, Ordering::Release);
        assert_eq!(
            cancelled(&flag).unwrap_err().kind(),
            io::ErrorKind::Interrupted
        );
        let mut assembler = Assembler::new(2, 1).unwrap();
        assembler.advance(1).unwrap();
        assert!(assembler.eof().is_err());
    }
    #[test]
    fn gif_stream_matches_snapshot_profile_and_has_finite_loop_policy() {
        let mut i = input();
        i.path = "/fixture/animation.gif".into();
        i.codec = "gif".into();
        i.source_width = 320;
        i.source_height = 180;
        assert!(validate(&i).is_ok());
        let cmd = command(&i, "gif", 10).unwrap();
        let args = cmd
            .get_args()
            .map(|v| v.to_string_lossy())
            .collect::<Vec<_>>();
        assert!(args.windows(2).any(|a| a == ["-ignore_loop", "1"]));
        assert!(args.windows(2).any(|a| a == ["-codec_whitelist", "gif"]));
        i.duration = 301.;
        assert!(validate(&i).is_err());
        i.duration = 8.;
        i.source_width = 4096;
        i.source_height = 4096;
        assert!(validate(&i).is_err());
        i.source_width = 320;
        i.source_height = 180;
        i.codec = "h264".into();
        assert!(validate(&i).is_err());
    }
    #[test]
    fn selected_stream_readonly_close_does_not_claim_finished_or_allow_late_spawn() {
        let gate = Arc::new(Mutex::new(Gate {
            phase: Phase::ReadOnly,
            closing: false,
        }));
        let (tx, finished) = mpsc::sync_channel(1);
        let job = Job {
            gate: gate.clone(),
            cancel: Arc::new(AtomicBool::new(false)),
            latest: Arc::new(Mutex::new(None)),
            frame_bytes: 6,
            finished,
            reported_finished: AtomicBool::new(false),
            source_invalid: Arc::new(AtomicBool::new(false)),
        };
        assert!(job.readonly_pending());
        job.cancel();
        assert!(job.exit_allowed());
        assert!(job.poll_finished().is_none());
        assert!(admit_helper(&gate).is_err());
        gate.lock().unwrap().phase = Phase::Helper;
        assert!(!job.exit_allowed());
        // Only after direct-child reap can the worker enter readonly again.
        gate.lock().unwrap().phase = Phase::ReadOnly;
        tx.send(Ok(())).unwrap();
        assert!(job.poll_finished().unwrap().is_ok());
        assert!(job.exit_allowed());
    }
}
