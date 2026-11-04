//! Optional ffprobe audio metadata, not waveform decoding or playback.
//!
//! Linux pins a regular source FD and launches one direct, reaped process. Forced
//! self-contained demuxers, protocol/codec allowlists and disabled MOV data
//! references avoid playlists/URLs; they are defense in depth, not a filesystem
//! or decoder sandbox. In-place changes, parent symlinks/mounts and codec bugs
//! remain outside this guarantee. No external player or network request is made.
//! RLIMIT_AS/per-allocation limits are containment, NOT PSS/resource evidence.
use crate::text_preview_job::TextPreview;
use std::{
    io,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
};
const MAX_INPUT_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const MAX_METADATA: usize = 64 * 1024;
const MAX_DURATION: f64 = 7.0 * 24.0 * 60.0 * 60.0;
const MAX_TEXT: usize = 16 * 1024;
const MAX_TAG: usize = 2048;
pub(crate) const CODECS: &str = "aac,aac_fixed,mp3,mp3float,mp3adu,mp3adufloat,mp3on4,mp3on4float,flac,opus,libopus,vorbis,libvorbis,pcm_s8,pcm_u8,pcm_s16le,pcm_s16be,pcm_u16le,pcm_u16be,pcm_s24le,pcm_s24be,pcm_u24le,pcm_u24be,pcm_s32le,pcm_s32be,pcm_u32le,pcm_u32be,pcm_s64le,pcm_s64be,pcm_f32le,pcm_f32be,pcm_f64le,pcm_f64be,pcm_alaw,pcm_mulaw";

#[derive(Clone, Debug)]
pub struct Info {
    pub duration: f64,
    pub codec: String,
    pub sample_rate: u32,
    pub channels: u8,
}
pub struct Ready {
    pub preview: TextPreview,
    pub source: files_core::media_source::Revision,
    pub info: Info,
}
use crate::readonly_gate::{Gate, Phase, admit_helper};
pub struct Job {
    gate: Arc<Mutex<Gate>>,
    cancel: Arc<AtomicBool>,
    result: Receiver<Result<Ready, String>>,
}
impl Job {
    pub fn start(path: PathBuf, expected: Option<files_core::media_source::Revision>) -> Self {
        let gate = Arc::new(Mutex::new(Gate {
            phase: Phase::ReadOnly,
            closing: false,
        }));
        let state = gate.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cancel);
        let (tx, result) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let result = run(path, expected, &flag, &state).map_err(|error| error.to_string());
            if let Ok(mut gate) = state.lock() {
                gate.phase = Phase::Stopped;
            }
            let _ = tx.send(result);
        });
        Self {
            gate,
            cancel,
            result,
        }
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
        if let Ok(mut gate) = self.gate.lock() {
            gate.closing = true;
        }
    }
    /// Permission only, never a claim that an uninterruptible readonly syscall ACKed.
    pub fn exit_allowed(&self) -> bool {
        self.gate
            .lock()
            .is_ok_and(|gate| gate.closing && gate.phase != Phase::Helper)
    }
    pub fn readonly_pending(&self) -> bool {
        self.gate
            .lock()
            .is_ok_and(|gate| gate.phase == Phase::ReadOnly)
    }
    pub fn poll(&self) -> Option<Result<Ready, String>> {
        match self.result.try_recv() {
            Ok(value) => Some(value),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Some(Err("La vista de audio terminó sin respuesta".into()))
            }
        }
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
fn demuxer(path: &Path) -> io::Result<&'static str> {
    let bytes = path.as_os_str().as_encoded_bytes();
    if !path.is_absolute() || bytes.len() > 4096 || bytes.contains(&0) {
        return Err(invalid("Ruta de audio no válida"));
    }
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .filter(|value| value.len() <= 8)
        .ok_or_else(|| invalid("Formato de audio no admitido"))?;
    match extension.to_ascii_lowercase().as_str() {
        "mp3" => Ok("mp3"),
        "wav" | "wave" => Ok("wav"),
        "flac" => Ok("flac"),
        "ogg" | "oga" | "opus" => Ok("ogg"),
        "m4a" => Ok("mov"),
        "aac" => Ok("aac"),
        "aif" | "aiff" | "aifc" => Ok("aiff"),
        _ => Err(invalid("Audio no admitido; listas y red desactivadas")),
    }
}
fn number(value: &serde_json::Value) -> Option<f64> {
    value.as_f64().or_else(|| {
        let value = value.as_str().filter(|value| value.len() <= 64)?;
        value.parse().ok()
    })
}
fn integer(value: &serde_json::Value) -> Option<u64> {
    value.as_u64().or_else(|| {
        let value = value.as_str().filter(|value| {
            !value.is_empty()
                && value.len() <= 20
                && value.bytes().all(|byte| byte.is_ascii_digit())
        })?;
        value.parse().ok()
    })
}
fn tag<'a>(value: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    value
        .as_object()?
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(key))?
        .1
        .as_str()
}
/// Sanitize user-supplied metadata without ever cloning the full tag. Controls
/// (including CR/LF) and bidi overrides/isolation markers cannot inject rows.
fn append_tag(output: &mut String, label: &str, value: &str) -> bool {
    output.push_str(label);
    output.push_str(": ");
    let mut bytes: usize = 0;
    let mut truncated = false;
    for character in value.chars() {
        let character = if character.is_control()
            || matches!(character, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        {
            ' '
        } else {
            character
        };
        let Some(next) = bytes.checked_add(character.len_utf8()) else {
            truncated = true;
            break;
        };
        if next > MAX_TAG {
            truncated = true;
            break;
        }
        output.push(character);
        bytes = next;
    }
    if truncated {
        output.push('…');
    }
    output.push('\n');
    truncated
}
pub(crate) fn track_info(stream: &serde_json::Value, duration: f64) -> io::Result<Info> {
    if !duration.is_finite() || duration <= 0.0 || duration > MAX_DURATION {
        return Err(invalid("Duración de audio fuera del límite"));
    }
    if stream["codec_type"].as_str() != Some("audio") {
        return Err(invalid("La pista seleccionada no es audio"));
    }
    let codec = stream["codec_name"]
        .as_str()
        .filter(|codec| CODECS.split(',').any(|allowed| allowed == *codec))
        .ok_or_else(|| invalid("Códec de audio no admitido"))?;
    let rate = integer(&stream["sample_rate"])
        .filter(|value| (1..=384_000).contains(value))
        .ok_or_else(|| invalid("Frecuencia de audio fuera del límite"))?;
    let channels = integer(&stream["channels"])
        .filter(|value| (1..=32).contains(value))
        .ok_or_else(|| invalid("Número de canales fuera del límite"))?;
    Ok(Info {
        duration,
        codec: codec.into(),
        sample_rate: rate as u32,
        channels: channels as u8,
    })
}
fn parse_metadata(wire: &[u8], demux: &str) -> io::Result<(TextPreview, Info)> {
    // Enforced again here before serde_json allocates a Value. Unknown fields,
    // nested tags and escaped strings remain bounded by this wire cap; this is
    // not a whole-process allocation bound for ffprobe or serde_json.
    if wire.len() > MAX_METADATA {
        return Err(invalid("Metadatos de audio demasiado grandes"));
    }
    let value: serde_json::Value = serde_json::from_slice(wire)?;
    let streams = value["streams"]
        .as_array()
        .filter(|streams| streams.len() == 1)
        .ok_or_else(|| invalid("No hay una pista de audio compatible"))?;
    let stream = &streams[0];
    let format = value["format"]["format_name"]
        .as_str()
        .ok_or_else(|| invalid("Contenedor de audio desconocido"))?;
    let expected = if demux == "mov" {
        "mov,mp4,m4a,3gp,3g2,mj2"
    } else {
        demux
    };
    if format != expected {
        return Err(invalid(
            "El contenedor de audio no coincide con el permitido",
        ));
    }
    let duration = number(&value["format"]["duration"])
        .or_else(|| number(&stream["duration"]))
        .filter(|value| value.is_finite() && *value > 0.0 && *value <= MAX_DURATION)
        .ok_or_else(|| invalid("Duración desconocida o superior a siete días"))?;
    let info = track_info(stream, duration)?;
    let codec = info.codec.as_str();
    let rate = info.sample_rate;
    let channels = info.channels;
    let seconds = duration.floor() as u64;
    let mut text = String::with_capacity(1024);
    text.push_str("Audio · metadatos\nReproducción a demanda\n\n");
    let mut truncated = false;
    for (key, label) in [("title", "Título"), ("artist", "Autor"), ("album", "Álbum")] {
        let selected = tag(&stream["tags"], key).or_else(|| tag(&value["format"]["tags"], key));
        if let Some(value) = selected.filter(|value| !value.is_empty()) {
            truncated |= append_tag(&mut text, label, value);
        }
    }
    use std::fmt::Write;
    let _ = writeln!(
        text,
        "Duración: {:02}:{:02}:{:02} ({duration:.3} s)",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60
    );
    let _ = writeln!(
        text,
        "Códec: {codec}\nFrecuencia: {rate} Hz\nCanales: {channels}"
    );
    if let Some(bits) = integer(&stream["bit_rate"])
        .or_else(|| integer(&value["format"]["bit_rate"]))
        .filter(|value| (1..=1_000_000_000).contains(value))
    {
        let _ = writeln!(text, "Tasa de bits declarada: {} kbit/s", bits / 1000);
    }
    if truncated {
        text.push_str("\nEtiquetas recortadas al límite de la vista previa.\n");
    }
    if text.len() > MAX_TEXT || text.lines().count() > 256 {
        return Err(invalid("Vista de metadatos de audio fuera del límite"));
    }
    Ok((
        TextPreview {
            text,
            encoding: "Audio · metadatos".into(),
            truncated,
            relative_source: None,
        },
        info,
    ))
}
#[cfg(target_os = "linux")]
fn command(fd: std::os::fd::RawFd) -> io::Result<std::process::Command> {
    command_for("ffprobe", fd, 3, 256 * 1024 * 1024)
}
#[cfg(target_os = "linux")]
fn command_for(
    program: &str,
    fd: std::os::fd::RawFd,
    cpu_seconds: u64,
    address_space: u64,
) -> io::Result<std::process::Command> {
    use std::{os::unix::process::CommandExt, process::Command};
    if !(3..256).contains(&fd) {
        return Err(invalid("Descriptor de audio no válido; requiere FD3..255"));
    }
    let mut command = Command::new(program);
    command
        .args(["-v", "error", "-max_alloc", "33554432"])
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .env("OMP_NUM_THREADS", "1")
        .env("OPENBLAS_NUM_THREADS", "1");
    unsafe {
        command.pre_exec(move || {
            // This FD is occupied already, so it cannot collide with Rust's
            // later exec-error pipe. Parent ownership/CLOEXEC remains intact.
            let flags = libc::fcntl(fd, libc::F_GETFD);
            if flags < 0 || libc::fcntl(fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC) < 0 {
                return Err(io::Error::last_os_error());
            }
            for (resource, value) in [
                (libc::RLIMIT_AS, address_space),
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
fn checked_source(
    path: &Path,
    expected: Option<files_core::media_source::Revision>,
) -> io::Result<(std::fs::File, files_core::media_source::Revision)> {
    let expected = expected.ok_or_else(|| invalid("El audio aún no tiene origen verificado"))?;
    let (file, revision) = files_core::media_source::open(path)?;
    if revision != expected {
        return Err(invalid("El audio cambió antes de leer sus metadatos"));
    }
    if revision.bytes == 0 || revision.bytes > MAX_INPUT_BYTES {
        return Err(invalid("El audio debe ser archivo regular de hasta8GiB"));
    }
    Ok((file, revision))
}
#[cfg(target_os = "linux")]
fn finish_metadata(
    file: &std::fs::File,
    path: &Path,
    revision: files_core::media_source::Revision,
    wire: &[u8],
    demux: &str,
) -> io::Result<Ready> {
    let (preview, info) = parse_metadata(wire, demux)?;
    revision.revalidate(file, path)?;
    Ok(Ready {
        preview,
        source: revision,
        info,
    })
}
#[cfg(target_os = "linux")]
fn run(
    path: PathBuf,
    expected: Option<files_core::media_source::Revision>,
    cancel: &AtomicBool,
    gate: &Mutex<Gate>,
) -> io::Result<Ready> {
    use std::os::fd::AsRawFd;
    let demux = demuxer(&path)?;
    if cancel.load(Ordering::Acquire) {
        return Err(invalid("Vista de audio cancelada"));
    }
    let (file, revision) = checked_source(&path, expected)?;
    let fd = file.as_raw_fd();
    let pinned = format!("/proc/self/fd/{fd}");
    let mut probe = command(fd)?;
    probe.args([
        "-protocol_whitelist",
        "file,pipe",
        "-format_whitelist",
        demux,
        "-codec_whitelist",
        CODECS,
        "-probesize",
        "5242880",
        "-analyzeduration",
        "2000000",
        "-f",
        demux,
    ]);
    if demux == "mov" {
        probe.args(["-enable_drefs", "0", "-use_absolute_path", "0"]);
    }
    probe.args([
        "-threads", "1", "-select_streams", "a:0", "-show_entries",
        "format=format_name,duration,bit_rate:format_tags=title,artist,album:stream=codec_type,codec_name,sample_rate,channels,duration,bit_rate:stream_tags=title,artist,album",
        "-of", "json", "-i", &pinned,
    ]);
    // Close may allow abandonment of ReadOnly, so admission and closing must
    // share this gate BEFORE spawn; no child may appear after that permission.
    admit_helper(gate)?;
    let output = crate::text_preview_job::run_command_program(
        probe,
        &[],
        "preview_audio_metadata",
        cancel,
        std::time::Duration::from_secs(5),
        MAX_METADATA,
        None,
    )
    .map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            invalid("Vista de audio no disponible: requiere ffprobe del sistema")
        } else {
            error
        }
    })?;
    if cancel.load(Ordering::Acquire) {
        return Err(invalid("Vista de audio cancelada"));
    }
    // The direct process has ACKed reap; subsequent FS checks are readonly.
    if let Ok(mut gate) = gate.lock() {
        gate.phase = Phase::ReadOnly;
    }
    finish_metadata(&file, &path, revision, &output, demux)
}
#[cfg(not(target_os = "linux"))]
fn run(
    _: PathBuf,
    _: Option<files_core::media_source::Revision>,
    _: &AtomicBool,
    _: &Mutex<Gate>,
) -> io::Result<Ready> {
    Err(invalid(
        "Vista de audio segura pendiente en esta plataforma",
    ))
}

#[cfg(test)]
fn metadata(wire: &[u8], demux: &str) -> io::Result<TextPreview> {
    parse_metadata(wire, demux).map(|(preview, _)| preview)
}

pub enum PlaybackEnd {
    Complete,
    SourceUnavailable(String),
}
pub struct PlaybackJob {
    gate: Arc<Mutex<Gate>>,
    cancel: Arc<AtomicBool>,
    started: Arc<Mutex<Option<std::time::Instant>>>,
    result: Receiver<Result<PlaybackEnd, String>>,
}
impl PlaybackJob {
    pub fn start(
        path: PathBuf,
        expected: files_core::media_source::Revision,
        info: Info,
        position: f64,
    ) -> Self {
        let gate = Arc::new(Mutex::new(Gate {
            phase: Phase::ReadOnly,
            closing: false,
        }));
        let state = gate.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let started = Arc::new(Mutex::new(None));
        let signal = started.clone();
        let (tx, result) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let result = play(path, expected, info, position, &flag, &state, &signal)
                .map_err(|e| e.to_string());
            if let Ok(mut gate) = state.lock() {
                gate.phase = Phase::Stopped;
            }
            let _ = tx.send(result);
        });
        Self {
            gate,
            cancel,
            started,
            result,
        }
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
        if let Ok(mut gate) = self.gate.lock() {
            gate.closing = true;
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
    pub fn started(&self) -> Option<std::time::Instant> {
        self.started.lock().ok().and_then(|t| *t)
    }
    pub fn poll(&self) -> Option<Result<PlaybackEnd, String>> {
        match self.result.try_recv() {
            Ok(v) => Some(v),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Some(Err("Audio terminó sin confirmación del trabajador".into()))
            }
        }
    }
}
impl Drop for PlaybackJob {
    fn drop(&mut self) {
        self.cancel();
    }
}
pub(crate) fn validate_play(info: &Info, position: f64) -> io::Result<()> {
    if !info.duration.is_finite()
        || info.duration <= 0.0
        || info.duration > MAX_DURATION
        || !position.is_finite()
        || position < 0.0
        || position >= info.duration
        || !CODECS.split(',').any(|c| c == info.codec)
        || !(1..=384000).contains(&info.sample_rate)
        || !(1..=32).contains(&info.channels)
    {
        return Err(invalid("Parámetros de reproducción de audio no válidos"));
    }
    Ok(())
}
#[cfg(target_os = "linux")]
fn play(
    path: PathBuf,
    expected: files_core::media_source::Revision,
    info: Info,
    position: f64,
    cancel: &AtomicBool,
    gate: &Mutex<Gate>,
    started: &Mutex<Option<std::time::Instant>>,
) -> io::Result<PlaybackEnd> {
    use std::os::fd::AsRawFd;
    validate_play(&info, position)?;
    if cancel.load(Ordering::Acquire) {
        return Err(invalid("Audio cancelado"));
    }
    let demux = demuxer(&path)?;
    let (file, revision) = checked_source(&path, Some(expected))?;
    let fd = file.as_raw_fd();
    let remaining = info.duration - position;
    let cpu = ((remaining.ceil() * 2.0 + 10.0) as u64).clamp(10, 86400);
    // Pulse context reserves virtual mappings beyond512MiB here; same AS cap
    // as video. This is NOT a PSS allowance or a change to the150MB tree ceiling.
    let mut player = command_for("ffmpeg", fd, cpu, 768 * 1024 * 1024)?;
    player.env_remove("FFREPORT").current_dir("/").args([
        "-hide_banner",
        "-nostdin",
        "-threads",
        "1",
        "-filter_threads",
        "1",
        "-re",
        "-protocol_whitelist",
        "file,pipe",
        "-format_whitelist",
        demux,
        "-codec_whitelist",
        CODECS,
        "-probesize",
        "5242880",
        "-analyzeduration",
        "2000000",
        "-f",
        demux,
    ]);
    if demux == "mov" {
        player.args(["-enable_drefs", "0", "-use_absolute_path", "0"]);
    }
    player.args([
        "-c:a",
        &info.codec,
        "-ss",
        &format!("{position:.6}"),
        "-i",
        &format!("/proc/self/fd/{fd}"),
    ]);
    append_output(&mut player, None, remaining)?;
    admit_helper(gate)?;
    crate::text_preview_job::run_command_program_observed(
        player,
        &[],
        "preview_audio_playback",
        cancel,
        std::time::Duration::from_secs_f64(remaining + 10.0),
        4096,
        None,
        Some(crate::text_preview_job::ProgramObservation {
            started,
            strict_errors: true,
        }),
    )
    .map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            invalid("Reproducción no disponible: requiere ffmpeg con salida PulseAudio del sistema")
        } else {
            invalid(&format!(
                "No se pudo reproducir audio: {error}. Comprueba el dispositivo de salida."
            ))
        }
    })?;
    if let Ok(mut gate) = gate.lock() {
        gate.phase = Phase::ReadOnly;
    }
    if cancel.load(Ordering::Acquire) {
        return Err(invalid("Audio cancelado"));
    }
    match revision.revalidate(&file, &path) {
        Ok(()) => Ok(PlaybackEnd::Complete),
        Err(e) => Ok(PlaybackEnd::SourceUnavailable(e.to_string())),
    }
}
/// Appends one bounded, forced audio output to the existing decoder command.
/// None selects the first audio stream; an index comes from the bounded probe.
#[cfg(target_os = "linux")]
/// Explicit fixture-only destination. Invalid names fail closed instead of
/// letting a PCM check accidentally reach a hardware/default output.
fn fixture_sink(preview: bool, value: Option<&std::ffi::OsStr>) -> io::Result<Option<&str>> {
    if !preview {
        return Ok(None);
    }
    let Some(value) = value else {
        return Ok(None);
    };
    if value.as_encoded_bytes().len() > 64 {
        return Err(invalid("Sink privado de fixture fuera del límite"));
    }
    let value = value
        .to_str()
        .ok_or_else(|| invalid("Sink privado de fixture no válido"))?;
    if !value.starts_with("files_test_")
        || value.len() <= "files_test_".len()
        || !value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_')
    {
        return Err(invalid(
            "El fixture sólo admite un sink files_test_ privado",
        ));
    }
    Ok(Some(value))
}
pub(crate) fn append_output(
    command: &mut std::process::Command,
    index: Option<u32>,
    remaining: f64,
) -> io::Result<()> {
    if index.is_some_and(|i| i > 255)
        || !remaining.is_finite()
        || remaining <= 0.0
        || remaining > MAX_DURATION
    {
        return Err(invalid("Salida de audio fuera del límite"));
    }
    let map = index
        .map(|i| format!("0:{i}"))
        .unwrap_or_else(|| "0:a:0".into());
    command.args([
        "-map",
        &map,
        "-vn",
        "-sn",
        "-dn",
        "-t",
        &format!("{remaining:.6}"),
        "-af",
        "volume=0.5,aformat=sample_fmts=s16:sample_rates=48000:channel_layouts=stereo",
        "-c:a",
        "pcm_s16le",
        "-threads",
        "1",
    ]);
    let null_fixture = std::env::var_os("FILES_TEST_PREVIEW").as_deref()
        == Some(std::ffi::OsStr::new("1"))
        && std::env::var_os("FILES_TEST_AUDIO_NULL").as_deref() == Some(std::ffi::OsStr::new("1"));
    let sink_env = std::env::var_os("FILES_TEST_AUDIO_SINK");
    let sink = fixture_sink(
        std::env::var_os("FILES_TEST_PREVIEW").as_deref() == Some(std::ffi::OsStr::new("1")),
        sink_env.as_deref(),
    )?;
    if null_fixture && sink.is_some() {
        return Err(invalid("Fixture null y sink privado son excluyentes"));
    }
    if null_fixture {
        command.args(["-f", "null", "-"]);
    } else {
        if let Some(sink) = sink {
            command.args(["-device", sink]);
        }
        command.args([
            "-f",
            "pulse",
            "-buffer_duration",
            "100",
            "-name",
            "files",
            "-stream_name",
            "Vista previa",
            "files",
        ]);
    }
    Ok(())
}
#[cfg(not(target_os = "linux"))]
fn play(
    _: PathBuf,
    _: files_core::media_source::Revision,
    _: Info,
    _: f64,
    _: &AtomicBool,
    _: &Mutex<Gate>,
    _: &Mutex<Option<std::time::Instant>>,
) -> io::Result<PlaybackEnd> {
    Err(invalid(
        "Reproducción segura de audio pendiente en esta plataforma",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn wire() -> serde_json::Value {
        serde_json::json!({"streams":[{"codec_type":"audio","codec_name":"flac","sample_rate":"48000","channels":2,"bit_rate":"1000000","tags":{"TITLE":"Canción 日本語","artist":"Autora"}}],"format":{"format_name":"flac","duration":"61.25","tags":{"album":"Álbum"}}})
    }
    fn parse(value: serde_json::Value) -> io::Result<TextPreview> {
        metadata(&serde_json::to_vec(&value).unwrap(), "flac")
    }
    #[test]
    fn actual_fields_are_rendered_as_labeled_metadata_not_source() {
        let ready = parse(wire()).unwrap();
        assert_eq!(ready.encoding, "Audio · metadatos");
        assert!(!ready.truncated);
        for expected in [
            "Reproducción a demanda",
            "Título: Canción 日本語",
            "Autor: Autora",
            "Álbum: Álbum",
            "00:01:01 (61.250 s)",
            "Códec: flac",
            "Frecuencia: 48000 Hz",
            "Canales: 2",
            "1000 kbit/s",
        ] {
            assert!(ready.text.contains(expected), "{expected}");
        }
    }
    #[test]
    fn malformed_binary_large_wire_and_invalid_metadata_fail_closed() {
        assert!(metadata(b"\xff\0", "flac").is_err());
        assert!(metadata(&vec![b' '; MAX_METADATA + 1], "flac").is_err());
        for (key, value) in [
            ("sample_rate", serde_json::json!("384001")),
            ("sample_rate", serde_json::json!("0")),
            ("channels", serde_json::json!(33)),
            ("channels", serde_json::json!(0)),
            ("codec_name", serde_json::json!("h264")),
            ("codec_type", serde_json::json!("video")),
        ] {
            let mut data = wire();
            data["streams"][0][key] = value;
            assert!(parse(data).is_err(), "{key}");
        }
        for value in ["NaN", "inf", "Infinity", "-1", "0", "604801", "N/A"] {
            let mut data = wire();
            data["format"]["duration"] = serde_json::json!(value);
            assert!(parse(data).is_err(), "{value}");
        }
        let mut data = wire();
        data["format"]["format_name"] = serde_json::json!("hls");
        assert!(parse(data).is_err());
        let mut data = wire();
        data["streams"] = serde_json::json!([]);
        assert!(parse(data).is_err());
    }
    #[test]
    fn tags_are_capped_utf8_sanitized_and_truncation_is_visible() {
        let mut data = wire();
        data["streams"][0]["tags"]["TITLE"] =
            serde_json::json!(format!("línea\r\n\t\0\u{202e}{}", "漢".repeat(3000)));
        let result = parse(data).unwrap();
        assert!(result.truncated);
        assert!(result.text.contains('漢'));
        assert!(result.text.contains("Etiquetas recortadas"));
        assert!(!result.text.contains(['\r', '\0', '\t', '\u{202e}']));
        assert!(result.text.len() <= MAX_TEXT);
        assert!(result.text.lines().count() < 256);
        let mut value = String::new();
        assert!(!append_tag(&mut value, "Título", &"é".repeat(MAX_TAG / 2)));
        assert!(append_tag(
            &mut String::new(),
            "Título",
            &"é".repeat(MAX_TAG / 2 + 1)
        ));
    }
    #[test]
    #[cfg(target_os = "linux")]
    fn stdio_descriptor_aliases_are_rejected_before_exec() {
        for fd in [-1, 0, 1, 2, 256] {
            assert!(command_for("ffmpeg", fd, 3, 512 * 1024 * 1024).is_err());
        }
    }
    #[test]
    fn playback_caps_reject_unknown_codecs_and_bad_transport_parameters() {
        let base = Info {
            duration: 8.0,
            codec: "flac".into(),
            sample_rate: 48000,
            channels: 1,
        };
        for position in [f64::NAN, f64::INFINITY, -1.0, 8.0] {
            assert!(validate_play(&base, position).is_err());
        }
        for duration in [f64::NAN, f64::INFINITY, 0.0, -1.0, MAX_DURATION + 1.0] {
            let mut info = base.clone();
            info.duration = duration;
            assert!(validate_play(&info, 0.0).is_err());
        }
        for codec in ["h264", "http", "flac,pipe", ""] {
            let mut info = base.clone();
            info.codec = codec.into();
            assert!(validate_play(&info, 0.0).is_err());
        }
        let mut info = base.clone();
        info.channels = 33;
        assert!(validate_play(&info, 0.0).is_err());
        info.channels = 1;
        info.sample_rate = 384001;
        assert!(validate_play(&info, 0.0).is_err());
        assert!(validate_play(&base, 7.99).is_ok());
    }
    #[test]
    fn readonly_close_blocks_future_spawn_and_helper_phase_requires_ack() {
        let gate = Mutex::new(Gate {
            phase: Phase::ReadOnly,
            closing: true,
        });
        assert!(admit_helper(&gate).is_err());
        assert!(gate.lock().unwrap().phase == Phase::ReadOnly);
        let gate = Mutex::new(Gate {
            phase: Phase::ReadOnly,
            closing: false,
        });
        admit_helper(&gate).unwrap();
        gate.lock().unwrap().closing = true;
        assert!(gate.lock().unwrap().phase == Phase::Helper);
        // Only after supervisor returned following reap may readonly checks resume.
        gate.lock().unwrap().phase = Phase::ReadOnly;
        assert!(admit_helper(&gate).is_err());
    }
    #[test]
    #[cfg(target_os = "linux")]
    fn expected_fd_rejects_replace_before_probe_and_effective_name_after_probe() {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = files_core::theme::home()
            .join(".cache/files-test")
            .join(format!(
                "audio-source-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("source.flac");
        std::fs::write(&path, b"audio-a").unwrap();
        let (_, a) = files_core::media_source::open(&path).unwrap();
        std::fs::write(root.join("next"), b"audio-b").unwrap();
        std::fs::rename(root.join("next"), &path).unwrap();
        assert!(checked_source(&path, Some(a)).is_err());
        assert!(checked_source(&path, None).is_err());
        let (_, b) = files_core::media_source::open(&path).unwrap();
        let (file, b) = checked_source(&path, Some(b)).unwrap();
        let wire = serde_json::to_vec(&wire()).unwrap();
        let ready = finish_metadata(&file, &path, b, &wire, "flac").unwrap();
        assert_eq!(ready.source, b);
        std::fs::write(root.join("next"), b"audio-c").unwrap();
        std::fs::rename(root.join("next"), &path).unwrap();
        assert!(finish_metadata(&file, &path, b, &wire, "flac").is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn forced_containers_reject_playlists_urls_and_bad_paths() {
        for (path, expected) in [
            ("/a.MP3", "mp3"),
            ("/a.opus", "ogg"),
            ("/a.m4a", "mov"),
            ("/a.aif", "aiff"),
        ] {
            assert_eq!(demuxer(Path::new(path)).unwrap(), expected);
        }
        for path in [
            "https://host/a.mp3",
            "relative.mp3",
            "/a.m3u",
            "/a.m3u8",
            "/a.pls",
            "/a.mka",
            "/a.mp4",
            "/a",
        ] {
            assert!(demuxer(Path::new(path)).is_err(), "{path}");
        }
        assert!(demuxer(Path::new(&format!("/{}.mp3", "a".repeat(4096)))).is_err());
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            assert!(demuxer(Path::new(std::ffi::OsStr::from_bytes(b"/x\0.mp3"))).is_err());
            assert_eq!(
                demuxer(Path::new(std::ffi::OsStr::from_bytes(b"/x\xff.mp3"))).unwrap(),
                "mp3"
            );
        }
    }
    #[test]
    fn private_audio_sink_is_fixture_only_bounded_and_no_fallback() {
        use std::ffi::OsStr;
        assert_eq!(
            fixture_sink(false, Some(OsStr::new("hardware"))).unwrap(),
            None
        );
        assert_eq!(fixture_sink(true, None).unwrap(), None);
        assert_eq!(
            fixture_sink(true, Some(OsStr::new("files_test_123_A"))).unwrap(),
            Some("files_test_123_A")
        );
        for name in [
            "",
            "files_test_",
            "hardware",
            "files_test_x/other",
            "files_test_x@remote",
            "files_test_x\nnext",
            "files_test_á",
        ] {
            assert!(fixture_sink(true, Some(OsStr::new(name))).is_err());
        }
        assert!(fixture_sink(true, Some(OsStr::new(&"x".repeat(1024 * 1024)))).is_err());
    }
}
