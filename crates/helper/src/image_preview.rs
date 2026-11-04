//! One bounded static raster preview. No UI, cache, animation, network or GPU.
use image::{GenericImageView, ImageDecoder, ImageFormat, ImageReader, metadata::Orientation};
use std::io::{self, Cursor, Read};
use std::path::Path;

pub const MAX_INPUT_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_SOURCE_DIMENSION: u32 = 8192;
pub const MAX_SOURCE_PIXELS: u64 = 4 * 1024 * 1024;
pub const MAX_DECODED_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_OUTPUT_DIMENSION: u32 = 1024;
pub const MAX_OUTPUT_BYTES: usize = 1024 * 1024 * 3;
const DECODER_ALLOC_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Debug)]
pub struct Preview {
    pub width: u32,
    pub height: u32,
    pub source_width: u32,
    pub source_height: u32,
    pub format: &'static str,
    pub rgb: Vec<u8>,
    pub source: Option<files_core::media_source::Revision>,
    pub relative_source: Option<files_core::relative_source::Proof>,
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn decoder_error(_: image::ImageError) -> io::Error {
    // Do not echo arbitrarily long metadata/decoder diagnostics into IPC.
    invalid("Imagen dañada, no admitida o fuera del presupuesto del decodificador")
}

pub fn load(path: &Path) -> io::Result<Preview> {
    load_expected(path, None, MAX_OUTPUT_DIMENSION)
}
pub fn load_checked(
    path: &Path,
    expected: files_core::media_source::Revision,
) -> io::Result<Preview> {
    load_expected(path, Some(expected), MAX_OUTPUT_DIMENSION)
}
pub fn thumbnail(
    path: &Path,
    expected: files_core::media_source::Revision,
    max_dimension: u32,
) -> io::Result<Preview> {
    if !(32..=160).contains(&max_dimension) {
        return Err(invalid("Miniatura fuera del límite 32..160px"));
    }
    load_expected(path, Some(expected), max_dimension)
}
fn load_expected(
    path: &Path,
    expected: Option<files_core::media_source::Revision>,
    max_dimension: u32,
) -> io::Result<Preview> {
    if path.as_os_str().as_encoded_bytes().len() > 4096 {
        return Err(invalid("Ruta de imagen demasiado larga"));
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW);
    }
    #[cfg(not(unix))]
    if std::fs::symlink_metadata(path)?.file_type().is_symlink() {
        return Err(invalid("No se siguen enlaces para la vista de imagen"));
    }
    let mut file = options.open(path)?;
    let metadata = file.metadata()?;
    let source_revision = files_core::media_source::Revision::from_metadata(&metadata)?;
    if expected.is_some_and(|expected| expected != source_revision) {
        return Err(invalid(
            "El original fue reemplazado antes de abrir la vista previa",
        ));
    }
    source_revision.revalidate(&file, path)?;
    if !metadata.is_file() {
        return Err(invalid("La vista de imagen requiere un archivo regular"));
    }
    let size = usize::try_from(metadata.len()).map_err(|_| invalid("Imagen demasiado grande"))?;
    if size > MAX_INPUT_BYTES {
        return Err(invalid("Imagen comprimida mayor de 8 MiB"));
    }
    // Fixed exact allocation from opened-file metadata; reject growth, never
    // read_to_end into an unbounded/geometrically expanding Vec.
    let mut data = Vec::new();
    data.try_reserve_exact(size)
        .map_err(|_| invalid("No hay memoria para leer la imagen"))?;
    data.resize(size, 0);
    file.read_exact(&mut data)?;
    let mut sentinel = [0];
    if file.read(&mut sentinel)? != 0 {
        return Err(invalid("La imagen cambió durante la lectura"));
    }
    let mut decoded = decode_to(&data, max_dimension)?;
    source_revision.revalidate(&file, path)?;
    decoded.source = Some(source_revision);
    Ok(decoded)
}

/// A checked request binds the FD actually decoded, and the current root/name.
pub fn load_relative(root: &Path, relative: &Path) -> io::Result<Preview> {
    load_relative_expected(root, relative, None)
}
pub fn load_relative_expected(
    root: &Path,
    relative: &Path,
    expected: Option<files_core::relative_source::Proof>,
) -> io::Result<Preview> {
    let mut opened = files_core::relative_source::Opened::open(root, relative)?;
    if let Some(expected) = expected {
        opened.expect(expected)?;
    }
    let data = files_core::relative_source::read_prefix(&mut opened.file, MAX_INPUT_BYTES + 1)?;
    let mut decoded = decode(&data)?;
    opened.revalidate()?;
    decoded.source = Some(opened.proof.file);
    decoded.relative_source = Some(opened.proof);
    Ok(decoded)
}

fn dimensions_allowed(width: u32, height: u32) -> io::Result<()> {
    if width == 0
        || height == 0
        || width > MAX_SOURCE_DIMENSION
        || height > MAX_SOURCE_DIMENSION
        || u64::from(width) * u64::from(height) > MAX_SOURCE_PIXELS
    {
        return Err(invalid(
            "Imagen fuera del límite: 8192 por eje y 4 Mipíxeles",
        ));
    }
    Ok(())
}

/// Minimal non-allocating dimension preflight, before decoder construction.
/// Full validation (including PNG CRC and actual compressed data) stays with the codec.
fn header(data: &[u8]) -> io::Result<(ImageFormat, &'static str, u32, u32)> {
    if data.starts_with(b"\x89PNG\r\n\x1a\n") && data.len() >= 24 && &data[12..16] == b"IHDR" {
        return Ok((
            ImageFormat::Png,
            "png",
            u32::from_be_bytes(data[16..20].try_into().unwrap()),
            u32::from_be_bytes(data[20..24].try_into().unwrap()),
        ));
    }
    if data.starts_with(b"\xff\xd8") {
        let mut i = 2;
        while i < data.len() {
            if data[i] != 0xff {
                return Err(invalid("Cabecera JPEG inválida"));
            }
            while data.get(i) == Some(&0xff) {
                i += 1;
            }
            let marker = *data
                .get(i)
                .ok_or_else(|| invalid("Cabecera JPEG incompleta"))?;
            i += 1;
            if matches!(marker, 0xd9 | 0xda) {
                break;
            }
            if marker == 0x01 || (0xd0..=0xd7).contains(&marker) {
                continue;
            }
            let length_bytes = data
                .get(i..i + 2)
                .ok_or_else(|| invalid("Cabecera JPEG incompleta"))?;
            let length = usize::from(u16::from_be_bytes(length_bytes.try_into().unwrap()));
            let segment = data
                .get(i..i.saturating_add(length))
                .filter(|_| length >= 2)
                .ok_or_else(|| invalid("Segmento JPEG inválido"))?;
            if matches!(marker, 0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf) {
                if segment.len() < 8 {
                    return Err(invalid("Dimensiones JPEG incompletas"));
                }
                return Ok((
                    ImageFormat::Jpeg,
                    "jpeg",
                    u32::from(u16::from_be_bytes([segment[5], segment[6]])),
                    u32::from(u16::from_be_bytes([segment[3], segment[4]])),
                ));
            }
            i += length;
        }
        return Err(invalid("JPEG sin dimensiones"));
    }
    if data.len() >= 25 && &data[..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        let (width, height) = match &data[12..16] {
            b"VP8X" if data.len() >= 30 => {
                if data[20] & 2 != 0 {
                    return Err(invalid("WebP animado aún no admitido"));
                }
                (
                    1 + u32::from_le_bytes([data[24], data[25], data[26], 0]),
                    1 + u32::from_le_bytes([data[27], data[28], data[29], 0]),
                )
            }
            b"VP8 " if data.len() >= 30 && &data[23..26] == b"\x9d\x01\x2a" => (
                u32::from(u16::from_le_bytes([data[26], data[27]]) & 0x3fff),
                u32::from(u16::from_le_bytes([data[28], data[29]]) & 0x3fff),
            ),
            b"VP8L" if data[20] == 0x2f => {
                let bits = u32::from_le_bytes(data[21..25].try_into().unwrap());
                ((bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1)
            }
            _ => return Err(invalid("Cabecera WebP no admitida")),
        };
        return Ok((ImageFormat::WebP, "webp", width, height));
    }
    Err(invalid("Formato no admitido: PNG, JPEG o WebP estático"))
}

// The VP8X canvas can disagree with an inner VP8 bitstream. Reject that before
// its decoder allocates frame planes (its own equality check occurs after decode).
fn validate_webp_frames(data: &[u8], width: u32, height: u32) -> io::Result<()> {
    let riff_size = u32::from_le_bytes(data[4..8].try_into().unwrap()) as usize;
    if riff_size.checked_add(8) != Some(data.len()) {
        return Err(invalid("Tamaño del contenedor WebP inválido"));
    }
    let mut index = 12usize;
    let mut frames = 0usize;
    while index < data.len() {
        let chunk = data
            .get(index..index + 8)
            .ok_or_else(|| invalid("Chunk WebP incompleto"))?;
        let size = u32::from_le_bytes(chunk[4..8].try_into().unwrap()) as usize;
        let start = index + 8;
        let end = start
            .checked_add(size)
            .ok_or_else(|| invalid("Chunk WebP demasiado grande"))?;
        let payload = data
            .get(start..end)
            .ok_or_else(|| invalid("Chunk WebP incompleto"))?;
        let dimensions = match &chunk[..4] {
            b"VP8 "
                if payload.len() >= 10
                    && &payload[3..6] == b"\x9d\x01\x2a"
                    && payload[0] & 1 == 0 =>
            {
                Some((
                    u32::from(u16::from_le_bytes([payload[6], payload[7]]) & 0x3fff),
                    u32::from(u16::from_le_bytes([payload[8], payload[9]]) & 0x3fff),
                ))
            }
            b"VP8L" if payload.len() >= 5 && payload[0] == 0x2f => {
                let bits = u32::from_le_bytes(payload[1..5].try_into().unwrap());
                Some(((bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1))
            }
            b"VP8 " | b"VP8L" => return Err(invalid("Bitstream WebP inválido")),
            b"ANIM" | b"ANMF" => return Err(invalid("WebP animado aún no admitido")),
            _ => None,
        };
        if let Some(dimensions) = dimensions {
            frames += 1;
            if dimensions != (width, height) || frames > 1 {
                return Err(invalid("Dimensiones internas WebP contradictorias"));
            }
        }
        index = end
            .checked_add(size & 1)
            .ok_or_else(|| invalid("Chunk WebP demasiado grande"))?;
    }
    if index != data.len() || frames != 1 {
        return Err(invalid("WebP sin un único frame completo"));
    }
    Ok(())
}

fn decode(data: &[u8]) -> io::Result<Preview> {
    decode_to(data, MAX_OUTPUT_DIMENSION)
}
fn decode_to(data: &[u8], max_dimension: u32) -> io::Result<Preview> {
    if data.len() > MAX_INPUT_BYTES {
        return Err(invalid("Imagen comprimida mayor de 8 MiB"));
    }
    let (format, label, source_width, source_height) = header(data)?;
    if format == ImageFormat::Jpeg {
        let dct_limit = max_dimension.max(source_width.max(source_height).div_ceil(8));
        let scaled = crate::jpeg_scaled::decode(data, dct_limit)?;
        let orientation =
            Orientation::from_exif(scaled.orientation).unwrap_or(Orientation::NoTransforms);
        if orientation == Orientation::NoTransforms
            && scaled.width <= max_dimension
            && scaled.height <= max_dimension
        {
            return Ok(Preview {
                source: None,
                relative_source: None,
                width: scaled.width,
                height: scaled.height,
                source_width: scaled.source_width,
                source_height: scaled.source_height,
                format: "jpeg",
                rgb: scaled.rgb,
            });
        }
        let rgb = image::RgbImage::from_raw(scaled.width, scaled.height, scaled.rgb)
            .ok_or_else(|| invalid("Longitud JPEG inválida"))?;
        return rasterize_to(
            image::DynamicImage::ImageRgb8(rgb),
            orientation,
            scaled.source_width,
            scaled.source_height,
            "jpeg",
            max_dimension,
        );
    }
    if format == ImageFormat::Png {
        return crate::png_rows::decode(data, max_dimension);
    }
    dimensions_allowed(source_width, source_height)?;
    if format == ImageFormat::WebP {
        validate_webp_frames(data, source_width, source_height)?;
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SOURCE_DIMENSION);
    limits.max_image_height = Some(MAX_SOURCE_DIMENSION);
    limits.max_alloc = Some(DECODER_ALLOC_BYTES);
    let mut reader = ImageReader::with_format(Cursor::new(data), format);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().map_err(decoder_error)?;
    if decoder.dimensions() != (source_width, source_height)
        || decoder.total_bytes() > MAX_DECODED_BYTES
    {
        return Err(invalid(
            "La imagen decodificada supera 16 MiB o difiere de la cabecera",
        ));
    }
    let orientation = decoder.orientation().map_err(decoder_error)?;
    let image = image::DynamicImage::from_decoder(decoder).map_err(decoder_error)?;
    rasterize_to(
        image,
        orientation,
        source_width,
        source_height,
        label,
        max_dimension,
    )
}
pub(crate) fn rasterize_to(
    image: image::DynamicImage,
    orientation: Orientation,
    source_width: u32,
    source_height: u32,
    label: &'static str,
    max_dimension: u32,
) -> io::Result<Preview> {
    let (decoded_width, decoded_height) = image.dimensions();
    let (oriented_width, oriented_height) =
        oriented_dimensions(decoded_width, decoded_height, orientation);
    let (width, height) = fit_to(oriented_width, oriented_height, max_dimension);
    let bytes = (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(3))
        .filter(|n| *n <= MAX_OUTPUT_BYTES)
        .ok_or_else(|| invalid("Vista de imagen demasiado grande"))?;
    let mut rgb = Vec::new();
    rgb.try_reserve_exact(bytes)
        .map_err(|_| invalid("No hay memoria para la vista de imagen"))?;
    for y in 0..height {
        for x in 0..width {
            let ox = x * oriented_width / width;
            let oy = y * oriented_height / height;
            let (sx, sy) = source_coordinate(ox, oy, decoded_width, decoded_height, orientation);
            rgb.extend_from_slice(&composite(image.get_pixel(sx, sy).0, x, y));
        }
    }
    Ok(Preview {
        source: None,
        relative_source: None,
        width,
        height,
        source_width,
        source_height,
        format: label,
        rgb,
    })
}

#[cfg(test)]
fn fit(width: u32, height: u32) -> (u32, u32) {
    fit_to(width, height, MAX_OUTPUT_DIMENSION)
}
pub(crate) fn composite([r, g, b, a]: [u8; 4], x: u32, y: u32) -> [u8; 3] {
    let background = if (x / 16 + y / 16).is_multiple_of(2) {
        40u16
    } else {
        64u16
    };
    let alpha = u16::from(a);
    [r, g, b].map(|v| ((u16::from(v) * alpha + background * (255 - alpha) + 127) / 255) as u8)
}
pub(crate) fn fit_to(width: u32, height: u32, max_dimension: u32) -> (u32, u32) {
    let longest = width.max(height);
    if longest <= max_dimension {
        return (width, height);
    }
    (
        ((u64::from(width) * u64::from(max_dimension) / u64::from(longest)) as u32).max(1),
        ((u64::from(height) * u64::from(max_dimension) / u64::from(longest)) as u32).max(1),
    )
}
pub(crate) fn oriented_dimensions(w: u32, h: u32, o: Orientation) -> (u32, u32) {
    match o {
        Orientation::Rotate90
        | Orientation::Rotate270
        | Orientation::Rotate90FlipH
        | Orientation::Rotate270FlipH => (h, w),
        _ => (w, h),
    }
}
pub(crate) fn source_coordinate(x: u32, y: u32, w: u32, h: u32, o: Orientation) -> (u32, u32) {
    match o {
        Orientation::NoTransforms => (x, y),
        Orientation::FlipHorizontal => (w - 1 - x, y),
        Orientation::Rotate180 => (w - 1 - x, h - 1 - y),
        Orientation::FlipVertical => (x, h - 1 - y),
        Orientation::Rotate90 => (y, h - 1 - x),
        Orientation::Rotate270 => (w - 1 - y, x),
        Orientation::Rotate90FlipH => (y, x),
        Orientation::Rotate270FlipH => (w - 1 - y, h - 1 - x),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageEncoder, RgbImage, RgbaImage};

    fn png(rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
        let mut data = Vec::new();
        image::codecs::png::PngEncoder::new(&mut data)
            .write_image(rgba, width, height, image::ExtendedColorType::Rgba8)
            .unwrap();
        data
    }
    #[test]
    fn thumbnail_reuses_reduced_idct_then_bounds_twelve_and_twenty_four_mp_output() {
        for (w, h) in [(4000, 3000), (6000, 4000)] {
            let data = crate::jpeg_scaled::tests::fixture(w, h, false);
            let p = decode_to(&data, 160).unwrap();
            assert_eq!(
                (p.source_width, p.source_height),
                (u32::from(w), u32::from(h))
            );
            assert!(p.width <= 160 && p.height <= 160);
            assert_eq!(p.rgb.len(), p.width as usize * p.height as usize * 3);
        }
    }
    #[cfg(unix)]
    #[test]
    fn checked_decoder_rejects_verified_a_replaced_before_open_and_returns_b() {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = files_core::theme::home()
            .join(".cache/files-test")
            .join(format!(
                "image-checked-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("source.png");
        std::fs::write(&path, png(&[255, 0, 0, 255], 1, 1)).unwrap();
        let (_, a) = files_core::media_source::open(&path).unwrap();
        let replacement = root.join("next");
        std::fs::write(&replacement, png(&[0, 0, 255, 255], 1, 1)).unwrap();
        std::fs::rename(replacement, &path).unwrap();
        assert!(load_checked(&path, a).is_err());
        let (_, b) = files_core::media_source::open(&path).unwrap();
        let ready = load_checked(&path, b).unwrap();
        assert_eq!(ready.source, Some(b));
        assert_eq!(ready.rgb, [0, 0, 255]);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn png_rgb_alpha_and_no_upscaling() {
        let data = png(
            &[
                255, 0, 0, 255, 0, 255, 0, 0, 0, 0, 255, 128, 255, 255, 255, 255,
            ],
            2,
            2,
        );
        let preview = decode(&data).unwrap();
        assert_eq!(
            (
                preview.width,
                preview.height,
                preview.source_width,
                preview.source_height,
                preview.format
            ),
            (2, 2, 2, 2, "png")
        );
        assert_eq!(
            preview.rgb,
            &[255, 0, 0, 40, 40, 40, 20, 20, 148, 255, 255, 255]
        );
        assert_eq!(preview.rgb.len(), 12);
    }
    #[test]
    fn jpeg_webp_content_detection_and_exact_payload() {
        let raw = vec![90u8; 9 * 5 * 3];
        let jpeg = crate::jpeg_scaled::tests::fixture(9, 5, false);
        let mut webp = Vec::new();
        image::codecs::webp::WebPEncoder::new_lossless(&mut webp)
            .write_image(&raw, 9, 5, image::ExtendedColorType::Rgb8)
            .unwrap();
        for (data, label) in [(&jpeg, "jpeg"), (&webp, "webp")] {
            let preview = decode(data).unwrap();
            assert_eq!(
                (preview.width, preview.height, preview.format),
                (9, 5, label)
            );
            assert_eq!(preview.rgb.len(), 9 * 5 * 3);
        }
        let tiny = {
            let mut result = Vec::new();
            image::codecs::webp::WebPEncoder::new_lossless(&mut result)
                .write_image(&[1, 2, 3], 1, 1, image::ExtendedColorType::Rgb8)
                .unwrap();
            result
        };
        assert_eq!(decode(&tiny).unwrap().rgb, [1, 2, 3]);
    }
    #[test]
    fn jpeg_exif_orientation_is_applied_without_full_frame_rotation() {
        let jpeg = crate::jpeg_scaled::tests::fixture(2, 3, false);
        // APP1, Exif preamble, little-endian TIFF with one orientation=6 entry.
        let exif = b"Exif\0\0II\x2a\0\x08\0\0\0\x01\0\x12\x01\x03\0\x01\0\0\0\x06\0\0\0\0\0\0\0";
        let mut oriented = Vec::new();
        oriented.extend_from_slice(&jpeg[..2]);
        oriented.extend_from_slice(b"\xff\xe1");
        oriented.extend_from_slice(&((exif.len() + 2) as u16).to_be_bytes());
        oriented.extend_from_slice(exif);
        oriented.extend_from_slice(&jpeg[2..]);
        let preview = decode(&oriented).unwrap();
        assert_eq!((preview.source_width, preview.source_height), (2, 3));
        assert_eq!((preview.width, preview.height), (3, 2));
    }
    #[test]
    fn all_orientation_maps_match_reference_and_keep_bounds() {
        let input = RgbaImage::from_fn(2, 3, |x, y| image::Rgba([(x + y * 2) as u8, 0, 0, 255]));
        for exif in 1..=8 {
            let orientation = Orientation::from_exif(exif).unwrap();
            let mut reference = image::DynamicImage::ImageRgba8(input.clone());
            reference.apply_orientation(orientation);
            let dimensions = oriented_dimensions(2, 3, orientation);
            assert_eq!(reference.dimensions(), dimensions);
            for y in 0..dimensions.1 {
                for x in 0..dimensions.0 {
                    let (sx, sy) = source_coordinate(x, y, 2, 3, orientation);
                    assert_eq!(
                        reference.get_pixel(x, y),
                        *input.get_pixel(sx, sy),
                        "orientation={exif}"
                    );
                }
            }
        }
    }
    #[test]
    fn source_bombs_corruption_and_unsupported_formats_are_rejected() {
        assert!(dimensions_allowed(2048, 2048).is_ok());
        assert!(dimensions_allowed(2049, 2048).is_err());
        assert!(dimensions_allowed(8193, 1).is_err());
        assert!(dimensions_allowed(0, 1).is_err());
        let mut bomb = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        bomb.extend_from_slice(&8192u32.to_be_bytes());
        bomb.extend_from_slice(&8192u32.to_be_bytes());
        bomb.extend_from_slice(&[8, 6, 0, 0, 0]);
        bomb.extend_from_slice(&[0; 4]); // complete IHDR, source cap rejects before CRC/allocation
        assert!(
            decode(&bomb)
                .unwrap_err()
                .to_string()
                .contains("40Mipíxeles")
        );
        assert!(decode(b"GIF89a\x01\0\x01\0").is_err());
        assert!(decode(b"\xff\xd8\xff\xe1\xff\xff").is_err());
        let good = png(&[1, 2, 3, 255], 1, 1);
        assert!(decode(&good[..good.len() / 2]).is_err());
        let mut animated = b"RIFF\x16\0\0\0WEBPVP8X\x0a\0\0\0\x02\0\0\0\0\0\0\0\0\0".to_vec();
        assert!(
            decode(&animated)
                .unwrap_err()
                .to_string()
                .contains("animado")
        );
        animated[20] = 0;
        assert!(decode(&animated).is_err());
    }

    #[test]
    fn sixteen_bit_png_converts_only_sampled_pixels_to_rgb8() {
        let mut data = Vec::new();
        let sample = [0xffffu16, 0x8080u16, 0u16, 0xffffu16];
        let bytes: Vec<u8> = sample
            .iter()
            .flat_map(|sample| sample.to_ne_bytes())
            .collect();
        image::codecs::png::PngEncoder::new(&mut data)
            .write_image(&bytes, 1, 1, image::ExtendedColorType::Rgba16)
            .unwrap();
        assert_eq!(decode(&data).unwrap().rgb, [255, 128, 0]);
    }
    #[test]
    fn extended_webp_inner_frame_bomb_is_rejected_before_codec() {
        // Tiny canvas, inconsistent huge VP8 frame, no valid compressed payload.
        let mut data=b"RIFF\x26\0\0\0WEBPVP8X\x0a\0\0\0\0\0\0\0\0\0\0\0\0\0VP8 \x0a\0\0\0\0\0\0\x9d\x01\x2a\0\x20\0\x20".to_vec();
        let size = (data.len() - 8) as u32;
        data[4..8].copy_from_slice(&size.to_le_bytes());
        let error = decode(&data).unwrap_err();
        assert!(error.to_string().contains("internas WebP"));
    }
    #[test]
    fn bounded_downsampling_does_not_upscale_or_exceed_wire_cap() {
        let input = RgbImage::from_pixel(8192, 1, image::Rgb([4, 5, 6]));
        let mut data = Vec::new();
        image::codecs::png::PngEncoder::new(&mut data)
            .write_image(input.as_raw(), 8192, 1, image::ExtendedColorType::Rgb8)
            .unwrap();
        let preview = decode(&data).unwrap();
        assert_eq!((preview.width, preview.height), (1024, 1));
        assert_eq!(preview.rgb.len(), 3072);
        for (w, h) in [(8192, 1), (1, 8192), (2048, 2048), (2000, 1000), (7, 3)] {
            let (x, y) = fit(w, h);
            assert!(x > 0 && y > 0 && x <= 1024 && y <= 1024 && x <= w && y <= h);
            assert!((x as usize) * (y as usize) * 3 <= MAX_OUTPUT_BYTES);
        }
    }
    #[cfg(unix)]
    #[test]
    fn opened_file_rejects_symlinks_fifo_directory_and_large_sparse_input() {
        use std::os::unix::{ffi::OsStrExt, fs::symlink};
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let root = std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .unwrap()
            .join(".cache/files-test")
            .join(format!(
                "image-helper-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("image.png");
        std::fs::write(&file, png(&[1, 2, 3, 255], 1, 1)).unwrap();
        assert_eq!(load(&file).unwrap().rgb, [1, 2, 3]);
        let link = root.join("link.png");
        symlink(&file, &link).unwrap();
        assert!(load(&link).is_err());
        let fifo = root.join("pipe.png");
        let c = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
        assert!(load(&fifo).is_err());
        assert!(load(&root).is_err());
        let huge = root.join("huge.png");
        std::fs::File::create(&huge)
            .unwrap()
            .set_len(MAX_INPUT_BYTES as u64 + 1)
            .unwrap();
        assert!(load(&huge).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(all(test, target_os = "linux"))]
mod relative_tests {
    use super::*;
    use std::os::unix::fs::symlink;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "files-relative-image-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn write(&self, name: &str, data: &[u8]) {
            std::fs::write(self.0.join(name), data).unwrap();
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn png() -> Vec<u8> {
        use image::ImageEncoder;
        let mut bytes = Vec::new();
        image::codecs::png::PngEncoder::new(&mut bytes)
            .write_image(
                &[255, 0, 0, 255, 0, 0, 255, 128],
                2,
                1,
                image::ExtendedColorType::Rgba8,
            )
            .unwrap();
        bytes
    }
    #[test]
    fn checked_relative_decode_binds_actual_fd_and_document_root() {
        use files_core::relative_source::Opened;
        let f = Fixture::new();
        f.write("image", &png());
        let expected = Opened::open(&f.0, Path::new("image")).unwrap().proof;
        let decoded = load_relative_expected(&f.0, Path::new("image"), Some(expected)).unwrap();
        assert_eq!(decoded.source, Some(expected.file));
        assert_eq!(decoded.relative_source, Some(expected));
        // Deterministic gap: verified A, then actor replaces name before helper B opens.
        f.write("next", &png());
        std::fs::rename(f.0.join("next"), f.0.join("image")).unwrap();
        let err = load_relative_expected(&f.0, Path::new("image"), Some(expected)).unwrap_err();
        assert!(err.to_string().contains("antes de leer"));
        // Even a valid image with identical bytes inside a different root is not A.
        let other = Fixture::new();
        other.write("image", &png());
        assert!(load_relative_expected(&other.0, Path::new("image"), Some(expected)).is_err());
    }
    #[test]
    fn relative_image_reuses_png_jpeg_webp_decoder_without_path_decoding() {
        use image::ImageEncoder;
        let f = Fixture::new();
        std::fs::create_dir(f.0.join("imágenes")).unwrap();
        let mut webp = Vec::new();
        image::codecs::webp::WebPEncoder::new_lossless(&mut webp)
            .write_image(&[1, 2, 3], 1, 1, image::ExtendedColorType::Rgb8)
            .unwrap();
        for data in [png(), crate::jpeg_scaled::tests::fixture(9, 5, false), webp] {
            f.write("imágenes/%2e%2e", &data);
            let relative = load_relative(&f.0, Path::new("imágenes/%2e%2e")).unwrap();
            let direct = load(&f.0.join("imágenes/%2e%2e")).unwrap();
            assert_eq!(
                (
                    relative.width,
                    relative.height,
                    relative.source_width,
                    relative.source_height,
                    relative.format
                ),
                (
                    direct.width,
                    direct.height,
                    direct.source_width,
                    direct.source_height,
                    direct.format
                )
            );
            assert_eq!(relative.rgb, direct.rgb);
        }
    }
    #[test]
    fn relative_image_rejects_symlinks_escape_and_invalid_roots() {
        let f = Fixture::new();
        let outside = Fixture::new();
        outside.write("image", &png());
        symlink(outside.0.join("image"), f.0.join("link")).unwrap();
        symlink(&outside.0, f.0.join("dir")).unwrap();
        for path in [
            "link",
            "dir/image",
            "../image",
            "/image",
            "",
            "dir/../image",
        ] {
            assert!(load_relative(&f.0, Path::new(path)).is_err(), "{path}");
        }
        f.write("file", &png());
        for root in [f.0.join("dir"), f.0.join("file"), f.0.join("missing")] {
            assert!(load_relative(&root, Path::new("image")).is_err());
        }
    }
    #[test]
    fn relative_image_checks_actual_sentinel_before_decoder() {
        let f = Fixture::new();
        let mut data = png();
        data.resize(MAX_INPUT_BYTES, 0);
        f.write("image", &data);
        // An image occupying the full compressed budget is not rejected merely
        // because its read reached the cap. PNG decoding tolerates this padding.
        assert_eq!(
            load_relative(&f.0, Path::new("image")).unwrap().rgb,
            load(&f.0.join("image")).unwrap().rgb
        );
        data.push(0);
        f.write("image", &data);
        assert_eq!(
            load_relative(&f.0, Path::new("image"))
                .unwrap_err()
                .to_string(),
            "Imagen comprimida mayor de 8 MiB"
        );
        f.write("image", b"not an image");
        assert!(load_relative(&f.0, Path::new("image")).is_err());
    }
}
