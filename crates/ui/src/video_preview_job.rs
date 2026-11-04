//! Optional system-CLI video snapshots, NOT playback or a measured memory gate.
//!
//! Linux opens one regular file without following its final symlink, then passes
//! that pinned descriptor to ffprobe and ffmpeg sequentially. The UI must drain
//! this Job before starting another preview. No shell or nested helper is used.
//! Only the listed self-contained demuxers/codecs are admitted; playlists, URLs,
//! concat, image sequences, external MOV data references and hardware decoding
//! are disabled. This is defense in depth, not a decoder sandbox. In-place file
//! changes, unsafe codec bugs, parent symlinks and mounts are outside this bound.
//! CPU/virtual-address/FD limits and ffmpeg's per-allocation cap are containment,
//! not proof of PSS: total main+CLI PSS/Swap and large inputs remain unmeasured.
//! Orthogonal display rotation and bounded sample-aspect ratios are applied by
//! FFmpeg and the output scaling. Non-orthogonal rotation is rejected. Seeking
//! requests a new Job; continuous playback is absent.
use crate::image_preview_job::Bitmap;
use crate::readonly_gate::{Gate, Phase, admit_helper};
use std::{
    io,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
};

const MAX_METADATA: usize = 64 * 1024;
pub(crate) const MAX_DURATION: f64 = 7.0 * 24.0 * 60.0 * 60.0;
const MAX_PIXELS: u64 = 64 * 1024 * 1024;
const MAX_INPUT_BYTES: u64 = 8 * 1024 * 1024 * 1024;
pub(crate) const CODECS: &str =
    "h264,hevc,vp8,vp9,av1,mpeg4,mpeg2video,mpeg1video,mjpeg,ffv1,rawvideo";

#[derive(Clone, Debug)]
pub struct AudioTrack {
    pub index: u32,
    pub info: crate::audio_preview_job::Info,
}
/// Largest side and area of a preview frame: a 4K frame fits the 32MiB
/// per-allocation cap of the decoders. The open preview asks for the
/// source's own pixels, only reduced to the device pixels it is shown in.
pub const MAX_OUTPUT_SIDE: u32 = 3840;
pub const MAX_OUTPUT_PIXELS: u64 = 3840 * 2160;
/// Output size for a source shown in a box of device pixels: the source
/// size when it fits (never upscaled), otherwise aspect-fit to the box,
/// always within [`MAX_OUTPUT_SIDE`]/[`MAX_OUTPUT_PIXELS`].
pub fn fit(display_width: f64, display_height: f64, box_width: u32, box_height: u32) -> (u32, u32) {
    let side = f64::from(MAX_OUTPUT_SIDE);
    let (bw, bh) = (
        f64::from(box_width.max(1)).min(side),
        f64::from(box_height.max(1)).min(side),
    );
    let (w, h) = dimensions_to(display_width, display_height, bw, bh);
    let area = u64::from(w) * u64::from(h);
    if area <= MAX_OUTPUT_PIXELS {
        return (w, h);
    }
    let factor = (MAX_OUTPUT_PIXELS as f64 / area as f64).sqrt();
    dimensions_to(
        display_width,
        display_height,
        (f64::from(w) * factor).floor(),
        (f64::from(h) * factor).floor(),
    )
}
pub(crate) fn output_fits(width: u32, height: u32) -> bool {
    width > 0
        && height > 0
        && width <= MAX_OUTPUT_SIDE
        && height <= MAX_OUTPUT_SIDE
        && u64::from(width) * u64::from(height) <= MAX_OUTPUT_PIXELS
}
/// Scaler for preview frames: Lanczos with full chroma for downscaling, an
/// exact format conversion when the source is shown at its own size.
pub(crate) const SCALE_FLAGS: &str = "lanczos+accurate_rnd+full_chroma_int";
#[derive(Debug)]
pub struct Ready {
    /// Display size of the source (sample aspect and rotation applied).
    pub display_width: f64,
    pub display_height: f64,
    /// Declared frame rate (num, den) when ffprobe gives a valid one.
    pub frame_rate: Option<(u32, u32)>,
    pub video_index: u32,
    pub audio: Option<AudioTrack>,
    pub audio_notice: Option<String>,
    pub bitmap: Bitmap,
    pub duration: f64,
    /// Effective clamped seek request, NOT the decoded frame's presentation PTS.
    pub position: f64,
    pub codec: String,
}
pub struct Job {
    gate: Arc<Mutex<Gate>>,
    cancel: Arc<AtomicBool>,
    result: Receiver<Result<Ready, String>>,
}
impl Job {
    pub fn start(
        path: PathBuf,
        position: f64,
        expected: Option<files_core::media_source::Revision>,
        target: (u32, u32),
    ) -> Self {
        let gate = Arc::new(Mutex::new(Gate {
            phase: Phase::ReadOnly,
            closing: false,
        }));
        let state = gate.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cancel);
        let (tx, result) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            crate::readonly_gate::hold_fixture("video_snapshot");
            let result =
                run(path, position, expected, target, &flag, &state).map_err(|e| e.to_string());
            if let Ok(mut g) = state.lock() {
                g.phase = Phase::Stopped;
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
    pub fn poll(&self) -> Option<Result<Ready, String>> {
        match self.result.try_recv() {
            Ok(value) => Some(value),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Some(Err("La vista de vídeo terminó sin respuesta".into()))
            }
        }
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[derive(Debug)]
struct Metadata {
    audio: Option<AudioTrack>,
    audio_notice: Option<String>,
    width: u32,
    height: u32,
    display_width: f64,
    display_height: f64,
    index: u32,
    duration: f64,
    frame_interval: f64,
    frame_rate: Option<(u32, u32)>,
    codec: String,
}
fn invalid(message: &str) -> io::Error {
    io::Error::other(message)
}
pub(crate) fn demuxer(path: &Path) -> io::Result<&'static str> {
    if !path.is_absolute() || path.as_os_str().as_encoded_bytes().len() > 4096 {
        return Err(invalid("Ruta de vídeo no válida"));
    }
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .filter(|value| value.len() <= 8)
        .ok_or_else(|| invalid("Formato de vídeo no admitido"))?;
    match extension.to_ascii_lowercase().as_str() {
        "mp4" | "m4v" | "mov" | "3gp" => Ok("mov"),
        "mkv" | "webm" => Ok("matroska"),
        "avi" => Ok("avi"),
        "gif" => Ok("gif"),
        "ts" | "mts" | "m2ts" => Ok("mpegts"),
        "mpg" | "mpeg" => Ok("mpeg"),
        _ => Err(invalid(
            "Formato de vídeo no admitido; listas y red desactivadas",
        )),
    }
}
pub(crate) fn input_byte_limit(demux: &str) -> u64 {
    if demux == "gif" {
        8 * 1024 * 1024
    } else {
        MAX_INPUT_BYTES
    }
}
pub(crate) fn pixel_limit(demux: &str) -> u64 {
    if demux == "gif" {
        4 * 1024 * 1024
    } else {
        MAX_PIXELS
    }
}
pub(crate) fn duration_limit(demux: &str) -> f64 {
    if demux == "gif" { 300.0 } else { MAX_DURATION }
}
pub(crate) fn codec_whitelist(demux: &str) -> String {
    if demux == "gif" {
        "gif".into()
    } else {
        format!("{CODECS},{}", crate::audio_preview_job::CODECS)
    }
}
fn number(value: &serde_json::Value) -> Option<f64> {
    value.as_f64().or_else(|| value.as_str()?.parse().ok())
}
/// Declared rate as a rational, for the player's output clock.
fn frame_rate(stream: &serde_json::Value) -> Option<(u32, u32)> {
    let rate = |key| {
        let value = stream.get(key)?.as_str().filter(|s| s.len() <= 32)?;
        let (n, d) = value.split_once('/')?;
        let part = |p: &str| {
            (!p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
                .then(|| p.parse::<u32>().ok())
                .flatten()
                .filter(|n| *n > 0)
        };
        let (n, d) = (part(n)?, part(d)?);
        let fps = f64::from(n) / f64::from(d);
        (fps.is_finite() && (0.1..=1000.0).contains(&fps)).then_some((n, d))
    };
    rate("avg_frame_rate").or_else(|| rate("r_frame_rate"))
}
fn frame_interval(stream: &serde_json::Value) -> f64 {
    // FFprobe emits rational integers. Parse those directly rather than accepting
    // NaN/Inf, floating strings or arbitrarily large operands. VFR averages are
    // only a seek safety estimate; they do not establish the final frame's PTS.
    let interval = |key| {
        let value = stream.get(key)?.as_str().filter(|s| s.len() <= 32)?;
        let (numerator, denominator) = value.split_once('/')?;
        let integer = |part: &str| {
            if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            part.parse::<u32>().ok().filter(|n| *n > 0)
        };
        let numerator = f64::from(integer(numerator)?);
        let denominator = f64::from(integer(denominator)?);
        let interval = denominator / numerator;
        interval
            .is_finite()
            .then_some(interval)
            .filter(|n| (0.001..=3600.0).contains(n))
    };
    interval("avg_frame_rate")
        .or_else(|| interval("r_frame_rate"))
        .unwrap_or(1.0)
}
fn display_size(stream: &serde_json::Value, width: u32, height: u32) -> io::Result<(f64, f64)> {
    let aspect = match stream.get("sample_aspect_ratio").and_then(|v| v.as_str()) {
        None | Some("N/A" | "0:1") => 1.0,
        Some(value) => {
            let (numerator, denominator) = value
                .split_once(':')
                .ok_or_else(|| invalid("Relación de aspecto no válida"))?;
            let ratio_part = |part: &str| {
                part.parse::<u32>()
                    .ok()
                    .filter(|v| (1..=65535).contains(v))
                    .ok_or_else(|| invalid("Relación de aspecto no válida"))
            };
            let value = f64::from(ratio_part(numerator)?) / f64::from(ratio_part(denominator)?);
            if !(1.0 / 16.0..=16.0).contains(&value) {
                return Err(invalid("Relación de aspecto fuera del límite"));
            }
            value
        }
    };
    let mut rotation = stream.get("tags").and_then(|v| v.get("rotate"));
    if let Some(side_data) = stream.get("side_data_list") {
        let values = side_data
            .as_array()
            .filter(|values| values.len() <= 8)
            .ok_or_else(|| invalid("Demasiados datos de orientación"))?;
        for value in values {
            if value.get("rotation").is_some() {
                rotation = value.get("rotation");
            }
        }
    }
    let angle = match rotation {
        Some(value) => number(value)
            .filter(|angle| angle.is_finite() && angle.abs() <= 36000.0)
            .ok_or_else(|| invalid("Rotación de vídeo no válida"))?,
        None => 0.0,
    };
    let quarter_turns = angle / 90.0;
    if (quarter_turns - quarter_turns.round()).abs() > 0.0001 {
        return Err(invalid("La vista previa sólo admite rotación ortogonal"));
    }
    let (width, height) = (f64::from(width) * aspect, f64::from(height));
    Ok(if (quarter_turns.round() as i32).rem_euclid(2) == 1 {
        (height, width)
    } else {
        (width, height)
    })
}
fn metadata(wire: &[u8], demux: &str) -> io::Result<Metadata> {
    if wire.len() > MAX_METADATA {
        return Err(invalid("Metadatos de vídeo demasiado grandes"));
    }
    let value: serde_json::Value = serde_json::from_slice(wire)?;
    let streams = value["streams"]
        .as_array()
        .filter(|streams| !streams.is_empty() && streams.len() <= 32)
        .ok_or_else(|| invalid("El archivo no tiene una pista de vídeo compatible"))?;
    let mut indices = std::collections::HashSet::new();
    for stream in streams {
        let index = stream["index"]
            .as_u64()
            .filter(|i| *i <= 255)
            .ok_or_else(|| invalid("Índice de pista no admitido"))?;
        if !indices.insert(index) {
            return Err(invalid("Índices de pista duplicados"));
        }
    }
    let stream = streams
        .iter()
        .find(|s| {
            s["codec_type"].as_str() == Some("video")
                && s["disposition"]["attached_pic"].as_u64() == Some(0)
        })
        .ok_or_else(|| invalid("No hay una pista de vídeo independiente"))?;
    if stream["codec_type"].as_str() != Some("video")
        || stream["disposition"]["attached_pic"].as_u64() != Some(0)
    {
        return Err(invalid("No es una pista de vídeo independiente"));
    }
    let dimension = |key| {
        stream[key]
            .as_u64()
            .filter(|n| (1..=16384).contains(n))
            .map(|n| n as u32)
            .ok_or_else(|| invalid("Dimensiones de vídeo no admitidas"))
    };
    let width = dimension("width")?;
    let height = dimension("height")?;
    if u64::from(width) * u64::from(height) > pixel_limit(demux) {
        return Err(invalid("El medio supera el límite de píxeles por frame"));
    }
    let index = stream["index"]
        .as_u64()
        .filter(|n| *n <= 255)
        .ok_or_else(|| invalid("Índice de vídeo no admitido"))? as u32;
    let codec = stream["codec_name"]
        .as_str()
        .filter(|codec| {
            if demux == "gif" {
                *codec == "gif"
            } else {
                CODECS.split(',').any(|known| known == *codec)
            }
        })
        .ok_or_else(|| invalid("Códec de vídeo no admitido"))?;
    let format = value["format"]["format_name"]
        .as_str()
        .ok_or_else(|| invalid("Contenedor de vídeo desconocido"))?;
    let valid_format = match demux {
        "mov" => format == "mov,mp4,m4a,3gp,3g2,mj2",
        "matroska" => format == "matroska,webm",
        other => format == other,
    };
    if !valid_format {
        return Err(invalid(
            "El contenedor no coincide con el formato permitido",
        ));
    }
    let duration = number(&value["format"]["duration"])
        .or_else(|| number(&stream["duration"]))
        .filter(|n| n.is_finite() && *n > 0.0 && *n <= duration_limit(demux))
        .ok_or_else(|| invalid("Duración desconocida o fuera del límite de este formato"))?;
    let (audio, audio_notice) = match streams
        .iter()
        .find(|s| s["codec_type"].as_str() == Some("audio"))
    {
        None => (None, None),
        Some(track) => match crate::audio_preview_job::track_info(track, duration) {
            Ok(info) => (
                Some(AudioTrack {
                    index: track["index"].as_u64().unwrap() as u32,
                    info,
                }),
                None,
            ),
            Err(_) => (
                None,
                Some("Primera pista de audio no compatible con los límites".into()),
            ),
        },
    };
    if demux == "gif" && streams.iter().any(|s| s["codec_type"] == "audio") {
        return Err(invalid("GIF no admite pistas de audio"));
    }
    let (display_width, display_height) = display_size(stream, width, height)?;
    Ok(Metadata {
        audio,
        audio_notice,
        width,
        height,
        display_width,
        display_height,
        index,
        duration,
        frame_interval: frame_interval(stream),
        frame_rate: frame_rate(stream),
        codec: codec.into(),
    })
}
#[cfg(test)]
fn dimensions(width: f64, height: f64) -> (u32, u32) {
    dimensions_to(width, height, 960., 540.)
}
fn dimensions_to(width: f64, height: f64, max_width: f64, max_height: f64) -> (u32, u32) {
    let divisor = (width / max_width).max(height / max_height).max(1.0);
    (
        (width / divisor).floor().max(1.0) as u32,
        (height / divisor).floor().max(1.0) as u32,
    )
}
fn position(requested: f64, duration: f64, frame_interval: f64) -> io::Result<f64> {
    if !requested.is_finite() || requested < 0.0 {
        return Err(invalid("Posición de vídeo no válida"));
    }
    // EOF minus a millisecond is after the final frame in e.g. a 5fps clip.
    // Retain one declared interval (or one second if FPS is unknown). This is a
    // bounded seek estimate; variable-rate files can still have irregular tails.
    Ok(requested.min((duration - frame_interval).max(0.0)))
}
fn frame(rgb: Vec<u8>, source: &Metadata, width: u32, height: u32) -> io::Result<Bitmap> {
    if !output_fits(width, height) {
        return Err(invalid("Dimensiones del frame fuera del límite"));
    }
    if rgb.len() != width as usize * height as usize * 3 {
        return Err(invalid("Frame de vídeo incompleto o con bytes adicionales"));
    }
    Ok(Bitmap {
        source: None,
        width,
        height,
        source_width: source.width,
        source_height: source.height,
        format: "video".into(),
        rgb,
    })
}

#[cfg(target_os = "linux")]
fn command(tool: &str, fd: std::os::fd::RawFd) -> io::Result<std::process::Command> {
    use std::os::unix::process::CommandExt;
    if !(3..256).contains(&fd) {
        return Err(invalid(
            "Demasiados descriptores abiertos para la vista de vídeo",
        ));
    }
    let mut command = std::process::Command::new(tool);
    command.args(["-v", "error", "-max_alloc", "33554432"]);
    // The original FD is already occupied in the parent, so it cannot collide
    // with Rust's later exec-error pipe. Clear only its CLOEXEC in the child.
    unsafe {
        command.pre_exec(move || {
            let flags = libc::fcntl(fd, libc::F_GETFD);
            if flags < 0 || libc::fcntl(fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC) < 0 {
                return Err(io::Error::last_os_error());
            }
            for (resource, value) in [
                (libc::RLIMIT_AS, 768 * 1024 * 1024),
                (libc::RLIMIT_CPU, 8),
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
fn input_options(command: &mut std::process::Command, demux: &str) {
    command.args([
        "-protocol_whitelist",
        "file,pipe",
        "-format_whitelist",
        demux,
        "-probesize",
        "5242880",
        "-analyzeduration",
        "2000000",
        "-max_pixels",
        &pixel_limit(demux).to_string(),
        "-f",
        demux,
    ]);
    if demux == "mov" {
        command.args(["-enable_drefs", "0", "-use_absolute_path", "0"]);
    } else if demux == "gif" {
        command.args(["-ignore_loop", "1"]);
    }
}
#[cfg(target_os = "linux")]
fn execute(
    command: std::process::Command,
    label: &str,
    cancel: &AtomicBool,
    timeout: u64,
    limit: usize,
    gate: Option<&Mutex<Gate>>,
) -> io::Result<Vec<u8>> {
    if let Some(gate) = gate {
        admit_helper(gate)?;
        if std::env::var_os("FILES_TEST_PREVIEW").is_some() {
            eprintln!("FILES_VIDEO_CHILD_ADMIT role=video_snapshot");
        }
    }
    let result = crate::text_preview_job::run_command_program(
        command,
        &[],
        label,
        cancel,
        std::time::Duration::from_secs(timeout),
        limit,
        None,
    )
    .map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            invalid("Vista de vídeo no disponible: requiere ffprobe y ffmpeg del sistema")
        } else {
            error
        }
    });
    // The common supervisor returns only after kill/wait on every path.
    if let Some(gate) = gate
        && let Ok(mut g) = gate.lock()
    {
        g.phase = Phase::ReadOnly;
    }
    result
}
#[cfg(target_os = "linux")]
fn run(
    path: PathBuf,
    requested: f64,
    expected: Option<files_core::media_source::Revision>,
    target: (u32, u32),
    cancel: &AtomicBool,
    gate: &Mutex<Gate>,
) -> io::Result<Ready> {
    run_to(
        path,
        requested,
        expected,
        cancel,
        f64::from(target.0),
        f64::from(target.1),
        Some(gate),
    )
}
#[cfg(target_os = "linux")]
pub(crate) fn thumbnail(
    path: PathBuf,
    expected: files_core::media_source::Revision,
    cancel: &AtomicBool,
) -> io::Result<Bitmap> {
    Ok(run_to(path, 0., Some(expected), cancel, 160., 160., None)?.bitmap)
}
#[cfg(not(target_os = "linux"))]
pub(crate) fn thumbnail(
    _: PathBuf,
    _: files_core::media_source::Revision,
    _: &AtomicBool,
) -> io::Result<Bitmap> {
    Err(invalid("Miniaturas de vídeo pendientes en esta plataforma"))
}
#[cfg(target_os = "linux")]
fn run_to(
    path: PathBuf,
    requested: f64,
    expected: Option<files_core::media_source::Revision>,
    cancel: &AtomicBool,
    max_width: f64,
    max_height: f64,
    gate: Option<&Mutex<Gate>>,
) -> io::Result<Ready> {
    use std::{fs::OpenOptions, os::fd::AsRawFd, os::unix::fs::OpenOptionsExt};
    let demux = demuxer(&path)?;
    if !requested.is_finite() || requested < 0.0 {
        return Err(invalid("Posición de vídeo no válida"));
    }
    if cancel.load(Ordering::Acquire) {
        return Err(invalid("Vista de vídeo cancelada"));
    }
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(&path)?;
    let revision = files_core::media_source::Revision::from_file(&file)?;
    if expected.is_some_and(|expected| expected != revision) {
        return Err(invalid("El vídeo cambió antes de abrir el decoder"));
    }
    revision.revalidate(&file, &path)?;
    let status = file.metadata()?;
    if !status.is_file() || status.len() == 0 || status.len() > input_byte_limit(demux) {
        return Err(invalid(if demux == "gif" {
            "GIF debe ser regular no vacío de hasta8MiB"
        } else {
            "Vídeo debe ser regular no vacío de hasta8GiB"
        }));
    }
    let fd = file.as_raw_fd();
    let pinned = format!("/proc/self/fd/{fd}");
    let mut probe = command("ffprobe", fd)?;
    input_options(&mut probe, demux);
    probe.args([
        "-threads", "1", "-codec_whitelist", &codec_whitelist(demux), "-show_entries",
        "format=format_name,duration:stream=index,codec_type,codec_name,width,height,duration,sample_rate,channels,sample_aspect_ratio,avg_frame_rate,r_frame_rate:stream_disposition=attached_pic:stream_side_data=rotation:stream_tags=rotate",
        "-of", "json", "-i", &pinned,
    ]);
    let source = metadata(
        &execute(probe, "preview_video_probe", cancel, 4, MAX_METADATA, gate)?,
        demux,
    )?;
    let position = position(requested, source.duration, source.frame_interval)?;
    let (width, height) = fit(
        source.display_width,
        source.display_height,
        max_width as u32,
        max_height as u32,
    );
    let mut decoder = command("ffmpeg", fd)?;
    decoder.args([
        "-nostdin",
        "-threads",
        "1",
        "-filter_threads",
        "1",
        "-hwaccel",
        "none",
    ]);
    input_options(&mut decoder, demux);
    decoder.args([
        "-codec_whitelist",
        &codec_whitelist(demux),
        "-ss",
        &format!("{position:.6}"),
        "-i",
        &pinned,
    ]);
    decoder.args([
        "-map",
        &format!("0:{}", source.index),
        "-an",
        "-sn",
        "-dn",
        "-frames:v",
        "1",
        "-vf",
        &format!("scale={width}:{height}:flags={SCALE_FLAGS},setsar=1"),
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
    let rgb = execute(
        decoder,
        "preview_video_frame",
        cancel,
        8,
        width as usize * height as usize * 3,
        gate,
    )?;
    if cancel.load(Ordering::Acquire) {
        return Err(invalid("Vista de vídeo cancelada"));
    }
    let mut bitmap = frame(rgb, &source, width, height)?;
    revision.revalidate(&file, &path)?;
    bitmap.source = Some(revision);
    Ok(Ready {
        display_width: source.display_width,
        display_height: source.display_height,
        frame_rate: source.frame_rate,
        video_index: source.index,
        audio: source.audio,
        audio_notice: source.audio_notice,
        bitmap,
        duration: source.duration,
        position,
        codec: source.codec,
    })
}
#[cfg(not(target_os = "linux"))]
fn run(
    _: PathBuf,
    _: f64,
    _: Option<files_core::media_source::Revision>,
    _: (u32, u32),
    _: &AtomicBool,
    _: &Mutex<Gate>,
) -> io::Result<Ready> {
    Err(invalid("Vista de vídeo pendiente en esta plataforma"))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn wire() -> serde_json::Value {
        serde_json::json!({"streams":[{"index":0,"codec_type":"video","codec_name":"h264","width":1920,"height":1080,"disposition":{"attached_pic":0}}],"format":{"format_name":"mov,mp4,m4a,3gp,3g2,mj2","duration":"12.5"}})
    }
    fn parse(value: serde_json::Value) -> io::Result<Metadata> {
        metadata(&serde_json::to_vec(&value).unwrap(), "mov")
    }
    #[test]
    fn first_independent_video_and_first_audio_keep_actual_indices_and_caps() {
        let mut v = wire();
        v["streams"][0]["index"] = 2.into();
        v["streams"].as_array_mut().unwrap().push(serde_json::json!({"index":5,"codec_type":"audio","codec_name":"flac","sample_rate":"48000","channels":2}));
        let meta = parse(v.clone()).unwrap();
        assert_eq!(meta.index, 2);
        let audio = meta.audio.unwrap();
        assert_eq!(audio.index, 5);
        assert_eq!(audio.info.codec, "flac");
        assert!(meta.audio_notice.is_none());
        // Do not select a later compatible audio stream over an unsupported first.
        v["streams"][1]["codec_name"] = "dts".into();
        v["streams"].as_array_mut().unwrap().push(serde_json::json!({"index":6,"codec_type":"audio","codec_name":"flac","sample_rate":"48000","channels":2}));
        let meta = parse(v.clone()).unwrap();
        assert!(meta.audio.is_none());
        assert!(meta.audio_notice.is_some());
        v["streams"][1]["codec_name"] = "flac".into();
        v["streams"][1]["channels"] = 33.into();
        assert!(parse(v.clone()).unwrap().audio.is_none());
        v["streams"][1]["index"] = 2.into();
        assert!(parse(v).is_err());
    }
    #[test]
    fn attached_picture_does_not_replace_video_and_track_count_is_bounded() {
        let mut v = wire();
        let mut cover = v["streams"][0].clone();
        cover["index"] = 1.into();
        cover["disposition"]["attached_pic"] = 1.into();
        v["streams"].as_array_mut().unwrap().insert(0, cover);
        assert_eq!(parse(v.clone()).unwrap().index, 0);
        v["streams"] = serde_json::json!(
            (0..33)
                .map(|i| serde_json::json!({"index":i}))
                .collect::<Vec<_>>()
        );
        assert!(parse(v).is_err());
    }
    #[test]
    fn metadata_requires_finite_bounded_duration_dimensions_and_codec() {
        let parsed = parse(wire()).unwrap();
        assert_eq!(
            (parsed.width, parsed.height, parsed.duration),
            (1920, 1080, 12.5)
        );
        for duration in ["NaN", "inf", "0", "-1", "604801"] {
            let mut v = wire();
            v["format"]["duration"] = duration.into();
            assert!(parse(v).is_err());
        }
        for (key, value) in [
            ("width", serde_json::json!(16385)),
            ("height", serde_json::json!(0)),
            ("width", serde_json::json!(1.5)),
            ("codec_name", serde_json::json!("avisynth")),
        ] {
            let mut v = wire();
            v["streams"][0][key] = value;
            assert!(parse(v).is_err());
        }
        let mut v = wire();
        v["streams"][0]["width"] = 16384.into();
        v["streams"][0]["height"] = 16384.into();
        assert!(parse(v).is_err());
    }
    #[test]
    fn metadata_rejects_attachment_missing_tracks_container_and_wire_caps() {
        for value in [
            serde_json::json!({}),
            serde_json::json!({"streams":[]}),
            serde_json::json!({"streams":[{},{}]}),
        ] {
            assert!(parse(value).is_err());
        }
        let mut v = wire();
        v["streams"][0]["disposition"]["attached_pic"] = 1.into();
        assert!(parse(v).is_err());
        let mut v = wire();
        v["format"]["format_name"] = "hls".into();
        assert!(parse(v).is_err());
        assert!(metadata(&vec![b' '; MAX_METADATA + 1], "mov").is_err());
        assert!(metadata(b"{", "mov").is_err());
    }
    #[test]
    fn open_preview_keeps_source_pixels_up_to_the_device_box_and_4k() {
        // Shown at its own size when it fits: the original pixels.
        assert_eq!(fit(1920., 1080., 2320, 1520), (1920, 1080));
        assert_eq!(fit(640., 360., 2320, 1520), (640, 360));
        // Larger than the device pixels it is shown in: aspect-fit.
        assert_eq!(fit(3840., 2160., 2320, 1520), (2320, 1305));
        // A huge box never exceeds the 4K area/side caps.
        let (w, h) = fit(7680., 4320., 8000, 8000);
        assert!(output_fits(w, h), "{w}x{h}");
        assert_eq!(fit(3840., 2160., 3840, 3840), (3840, 2160));
        let (w, h) = fit(2160., 3840., 3840, 3840);
        assert!(output_fits(w, h) && h == 3840 && w == 2160);
        assert!(!output_fits(3841, 1) && !output_fits(3840, 2161) && !output_fits(0, 1));
    }
    #[test]
    fn frame_rate_is_a_bounded_rational() {
        let mut v = wire();
        v["streams"][0]["avg_frame_rate"] = "30000/1001".into();
        assert_eq!(parse(v).unwrap().frame_rate, Some((30000, 1001)));
        let mut v = wire();
        v["streams"][0]["avg_frame_rate"] = "0/0".into();
        v["streams"][0]["r_frame_rate"] = "25/1".into();
        assert_eq!(parse(v).unwrap().frame_rate, Some((25, 1)));
        let mut v = wire();
        v["streams"][0]["avg_frame_rate"] = "1/0".into();
        assert_eq!(parse(v).unwrap().frame_rate, None);
    }
    #[test]
    fn scaling_never_upscales_or_exceeds_frame_bounds() {
        for (source, expected) in [
            ((1920, 1080), (960, 540)),
            ((1080, 1920), (303, 540)),
            ((100, 50), (100, 50)),
            ((16384, 1), (960, 1)),
            ((1, 16384), (1, 540)),
        ] {
            assert_eq!(
                dimensions(f64::from(source.0), f64::from(source.1)),
                expected
            );
        }
    }
    #[test]
    fn display_rotation_and_sample_aspect_preserve_portrait_and_anamorphic_ratio() {
        let mut value = wire();
        value["streams"][0]["side_data_list"] = serde_json::json!([{"rotation": -90}]);
        let source = parse(value).unwrap();
        assert_eq!(
            dimensions(source.display_width, source.display_height),
            (303, 540)
        );
        let mut value = wire();
        value["streams"][0]["width"] = 720.into();
        value["streams"][0]["height"] = 576.into();
        value["streams"][0]["sample_aspect_ratio"] = "16:15".into();
        let source = parse(value).unwrap();
        assert_eq!(
            dimensions(source.display_width, source.display_height),
            (720, 540)
        );
        for bad in ["1:0", "1:99999999", "1:-1", "1:17", "abc"] {
            let mut value = wire();
            value["streams"][0]["sample_aspect_ratio"] = bad.into();
            assert!(parse(value).is_err());
        }
        for rotation in [
            serde_json::json!(45),
            serde_json::json!("NaN"),
            serde_json::json!(36001),
        ] {
            let mut value = wire();
            value["streams"][0]["side_data_list"] = serde_json::json!([{"rotation": rotation}]);
            assert!(parse(value).is_err());
        }
    }
    #[test]
    fn raw_frame_requires_exact_length_and_reuses_allocation() {
        let source = parse(wire()).unwrap();
        let rgb = vec![123; 6];
        let pointer = rgb.as_ptr();
        let bitmap = frame(rgb, &source, 2, 1).unwrap();
        assert_eq!(bitmap.rgb.as_ptr(), pointer);
        assert_eq!(bitmap.source_width, 1920);
        for len in [0, 5, 7] {
            assert!(frame(vec![0; len], &source, 2, 1).is_err());
        }
        assert!(frame(Vec::new(), &source, 961, 1).is_err());
    }
    #[test]
    fn local_demuxers_only_and_seek_clamps_without_nonfinite_values() {
        assert_eq!(demuxer(Path::new("/fixture/clip.MP4")).unwrap(), "mov");
        for path in [
            "relative.mp4",
            "https://host/clip.mp4",
            "/clip.m3u8",
            "/clip.concat",
            "/clip.%04d.png",
        ] {
            assert!(demuxer(Path::new(path)).is_err());
        }
        assert_eq!(position(20.0, 10.0, 1.0).unwrap(), 9.0);
        assert_eq!(position(0.0, 10.0, 1.0).unwrap(), 0.0);
        for p in [f64::NAN, f64::INFINITY, -1.0] {
            assert!(position(p, 10.0, 1.0).is_err());
        }
    }
    #[test]
    fn eof_seek_retains_a_frame_interval_with_bounded_rational_rate_fallback() {
        let mut value = wire();
        value["format"]["duration"] = "8.0".into();
        value["streams"][0]["avg_frame_rate"] = "5/1".into();
        let source = parse(value.clone()).unwrap();
        assert_eq!(source.frame_interval, 0.2);
        assert_eq!(
            position(7.95, source.duration, source.frame_interval).unwrap(),
            7.8
        );
        assert_eq!(
            position(8.0, source.duration, source.frame_interval).unwrap(),
            7.8
        );
        value["streams"][0]["avg_frame_rate"] = "0/0".into();
        value["streams"][0]["r_frame_rate"] = "30000/1001".into();
        assert_eq!(
            parse(value.clone()).unwrap().frame_interval,
            1001.0 / 30000.0
        );
        for rate in [
            "0/0",
            "NaN",
            "1/0",
            "-5/1",
            "5.0/1",
            "4294967296/1",
            "1/999999",
            "1001/1",
        ] {
            value["streams"][0]["avg_frame_rate"] = rate.into();
            value["streams"][0]["r_frame_rate"] = rate.into();
            let source = parse(value.clone()).unwrap();
            assert_eq!(source.frame_interval, 1.0);
            assert_eq!(
                position(8.0, source.duration, source.frame_interval).unwrap(),
                7.0
            );
        }
        let unknown = parse(wire()).unwrap();
        assert_eq!(unknown.frame_interval, 1.0);
        assert_eq!(position(0.5, 0.5, 1.0).unwrap(), 0.0);
    }
    #[test]
    fn gif_profile_limits_and_forced_demux_do_not_expand_other_codecs() {
        assert_eq!(demuxer(Path::new("/fixture/animation.GIF")).unwrap(), "gif");
        assert_eq!(input_byte_limit("gif"), 8 * 1024 * 1024);
        assert_eq!(pixel_limit("gif"), 4 * 1024 * 1024);
        assert_eq!(duration_limit("gif"), 300.);
        assert_eq!(codec_whitelist("gif"), "gif");
        assert!(!codec_whitelist("mov").split(',').any(|c| c == "gif"));
        let mut v = wire();
        v["format"]["format_name"] = "gif".into();
        v["streams"][0]["codec_name"] = "gif".into();
        assert!(metadata(&serde_json::to_vec(&v).unwrap(), "gif").is_ok());
        v["format"]["duration"] = "301".into();
        assert!(metadata(&serde_json::to_vec(&v).unwrap(), "gif").is_err());
        v["format"]["duration"] = "8".into();
        v["streams"][0]["width"] = 4096.into();
        v["streams"][0]["height"] = 4096.into();
        assert!(metadata(&serde_json::to_vec(&v).unwrap(), "gif").is_err());
        v["streams"][0]["width"] = 320.into();
        v["streams"][0]["height"] = 180.into();
        v["streams"][0]["codec_name"] = "h264".into();
        assert!(metadata(&serde_json::to_vec(&v).unwrap(), "gif").is_err());
    }
    #[test]
    #[cfg(target_os = "linux")]
    fn gif_tools_ignore_infinite_source_loop_and_limit_decoder_pixels() {
        for program in ["ffprobe", "ffmpeg"] {
            let mut cmd = std::process::Command::new(program);
            input_options(&mut cmd, "gif");
            let args = cmd
                .get_args()
                .map(|v| v.to_string_lossy())
                .collect::<Vec<_>>();
            assert!(args.windows(2).any(|a| a == ["-ignore_loop", "1"]));
            assert!(args.windows(2).any(|a| a == ["-max_pixels", "4194304"]));
        }
    }
    #[test]
    fn selected_snapshot_readonly_permission_is_not_receiver_ack() {
        let gate = Arc::new(Mutex::new(Gate {
            phase: Phase::ReadOnly,
            closing: false,
        }));
        let (tx, result) = mpsc::sync_channel(1);
        let job = Job {
            gate: gate.clone(),
            cancel: Arc::new(AtomicBool::new(false)),
            result,
        };
        assert!(job.readonly_pending());
        assert!(!job.exit_allowed());
        job.cancel();
        assert!(job.exit_allowed());
        assert!(job.poll().is_none());
        assert!(admit_helper(&gate).is_err());
        gate.lock().unwrap().phase = Phase::Helper;
        assert!(!job.exit_allowed());
        gate.lock().unwrap().phase = Phase::ReadOnly;
        tx.send(Err("after reap".into())).unwrap();
        assert!(job.poll().unwrap().is_err());
        assert!(job.exit_allowed());
    }
}
