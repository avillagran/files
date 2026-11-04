//! Bounded JPEG DCT preview; uses the reviewed local
//! jpeg-decoder patch (immediate workers + pre-allocation coefficient limit).
//! Wired only in files-helper; no native libjpeg/IJG dependency.
use jpeg_decoder::{Decoder, PixelFormat};
use std::io::{self, Cursor};

pub const MAX_INPUT_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_SOURCE_DIMENSION: u32 = 8192;
pub const MAX_SOURCE_PIXELS: u64 = 40 * 1024 * 1024;
pub const MAX_OUTPUT_DIMENSION: u32 = 1024;
pub const MAX_OUTPUT_BYTES: usize = 1024 * 1024 * 3;
pub const MAX_COEFFICIENT_BYTES: usize = 32 * 1024 * 1024;
const MAX_METADATA_BYTES: usize = 1024 * 1024;
const MAX_SCANS: usize = 64;

#[derive(Debug)]
pub struct ScaledJpeg {
    /// Decoded unrotated dimensions; EXIF orientation is returned separately.
    pub width: u32,
    pub height: u32,
    pub source_width: u32,
    pub source_height: u32,
    pub rgb: Vec<u8>,
    /// EXIF 1..8, defaults to 1; caller must apply it once, not twice.
    pub orientation: u8,
}
#[derive(Debug, Clone, Copy)]
struct Header {
    width: u32,
    height: u32,
    components: usize,
    progressive: bool,
    coefficient_bytes: usize,
}
fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn codec_error(_: jpeg_decoder::Error) -> io::Error {
    invalid("JPEG dañado o no admitido por el decodificador acotado")
}

/// Decode directly to a supported 1/1, 1/2, 1/4 or 1/8 IDCT size.
/// No full-source RGB allocation, no duplicated compressed input, no worker
/// threads or unbounded MCU queues. Progressive images whose full coefficients
/// exceed32MiB use opt-in1/8 DC storage, with the SAME32MiB coefficient cap.
/// That path skips AC entropy and is not a full integrity validation.
///
/// This is not a total allocator/PSS cap. Existing helper RLIMIT_AS, parent
/// process timeout/cancel+kill/reap, and measured app-tree gates remain required.
pub fn decode(data: &[u8], max_dimension: u32) -> io::Result<ScaledJpeg> {
    if max_dimension == 0 || max_dimension > MAX_OUTPUT_DIMENSION {
        return Err(invalid("Límite de salida JPEG inválido (1–1024)"));
    }
    let header = preflight(data)?;
    let dc_preview = header.progressive && header.coefficient_bytes > MAX_COEFFICIENT_BYTES;
    let expected = if dc_preview {
        (header.width.div_ceil(8), header.height.div_ceil(8))
    } else {
        dct_dimensions(header.width, header.height, max_dimension)?
    };
    if expected.0 > max_dimension || expected.1 > max_dimension {
        return Err(invalid(
            "Reducción JPEG mínima1/8 supera el límite de salida",
        ));
    }
    let mut decoder = Decoder::new(Cursor::new(data));
    decoder.set_parallel_decoding(false);
    decoder.set_progressive_dc_preview(dc_preview);
    decoder.set_max_coefficient_buffer_size(MAX_COEFFICIENT_BYTES);
    decoder.set_max_decoding_buffer_size(MAX_OUTPUT_BYTES);
    decoder.read_info().map_err(codec_error)?;
    let info = decoder
        .info()
        .ok_or_else(|| invalid("JPEG sin dimensiones"))?;
    if (u32::from(info.width), u32::from(info.height)) != (header.width, header.height)
        || !matches!(info.pixel_format, PixelFormat::RGB24 | PixelFormat::L8)
    {
        return Err(invalid("Dimensiones o espacio de color JPEG no admitidos"));
    }
    // scale() chooses the first factor reaching EITHER requested axis. Keep
    // the non-dominant axis unreachable to avoid tiny-height panoramas selecting
    // an unexpectedly small factor (for example, every factor reaches height 1).
    let request = if header.width >= header.height {
        (expected.0 as u16, u16::MAX)
    } else {
        (u16::MAX, expected.1 as u16)
    };
    let (width, height) = decoder.scale(request.0, request.1).map_err(codec_error)?;
    let (width, height) = (u32::from(width), u32::from(height));
    if (width, height) != expected || width > max_dimension || height > max_dimension {
        return Err(invalid(
            "Factor IDCT JPEG no coincide con la salida acotada",
        ));
    }
    let pixels = (width as usize)
        .checked_mul(height as usize)
        .ok_or_else(|| invalid("Tamaño JPEG desbordado"))?;
    let expected_bytes = pixels
        .checked_mul(info.pixel_format.pixel_bytes())
        .filter(|n| *n <= MAX_OUTPUT_BYTES)
        .ok_or_else(|| invalid("Vista JPEG mayor de 3 MiB"))?;
    let decoded = decoder.decode().map_err(codec_error)?;
    if decoded.len() != expected_bytes {
        return Err(invalid("Longitud JPEG decodificada contradictoria"));
    }
    let orientation = decoder
        .exif_data()
        .and_then(image::metadata::Orientation::from_exif_chunk)
        .map_or(1, image::metadata::Orientation::to_exif);
    // Release metadata and coefficient buffers before expanding grayscale.
    drop(decoder);
    let rgb = if info.pixel_format == PixelFormat::L8 {
        let mut rgb = Vec::new();
        rgb.try_reserve_exact(pixels * 3)
            .map_err(|_| invalid("No hay memoria para la vista JPEG"))?;
        for value in decoded {
            rgb.extend_from_slice(&[value; 3]);
        }
        rgb
    } else {
        decoded
    };
    Ok(ScaledJpeg {
        width,
        height,
        source_width: header.width,
        source_height: header.height,
        rgb,
        orientation,
    })
}

fn dct_dimensions(width: u32, height: u32, max: u32) -> io::Result<(u32, u32)> {
    for denominator in [1u32, 2, 4, 8] {
        let candidate = (width.div_ceil(denominator), height.div_ceil(denominator));
        if candidate.0 <= max && candidate.1 <= max {
            return Ok(candidate);
        }
    }
    Err(invalid(
        "La reducción IDCT mínima 1/8 aún supera el límite de salida",
    ))
}

/// Non-allocating complete marker walk. Counts scans and retained metadata and
/// computes exact padded progressive coefficient payload from SOF sampling.
/// Normal mode validates full entropy. Sparse1/8 preview validates DC entropy
/// and marker structure only; ignored AC entropy is not an integrity check.
fn preflight(data: &[u8]) -> io::Result<Header> {
    if data.len() > MAX_INPUT_BYTES {
        return Err(invalid("JPEG comprimido mayor de 8 MiB"));
    }
    if !data.starts_with(b"\xff\xd8") {
        return Err(invalid("JPEG sin marcador inicial"));
    }
    let mut position = 2usize;
    let mut entropy = false;
    let mut header = None;
    let mut scans = 0usize;
    let mut metadata_bytes = 0usize;
    loop {
        if entropy {
            while data.get(position).is_some_and(|byte| *byte != 0xff) {
                position += 1;
            }
        }
        if data.get(position) != Some(&0xff) {
            return Err(invalid("JPEG incompleto o marcador inválido"));
        }
        while data.get(position) == Some(&0xff) {
            position += 1;
        }
        let marker = *data
            .get(position)
            .ok_or_else(|| invalid("Marcador JPEG incompleto"))?;
        position += 1;
        if entropy && (marker == 0 || (0xd0..=0xd7).contains(&marker)) {
            continue;
        }
        entropy = false;
        if marker == 0xd9 {
            if scans == 0 || position != data.len() {
                return Err(invalid("JPEG sin scans o con datos tras EOI"));
            }
            return header.ok_or_else(|| invalid("JPEG sin SOF"));
        }
        if marker == 0x01 {
            continue;
        }
        if marker == 0 || marker == 0xd8 || (0xd0..=0xd7).contains(&marker) {
            return Err(invalid("Marcador JPEG fuera de scan"));
        }
        let length = data
            .get(position..position + 2)
            .ok_or_else(|| invalid("Segmento JPEG incompleto"))?;
        let length = usize::from(u16::from_be_bytes(length.try_into().unwrap()));
        let end = position
            .checked_add(length)
            .filter(|_| length >= 2)
            .ok_or_else(|| invalid("Segmento JPEG inválido"))?;
        let segment = data
            .get(position + 2..end)
            .ok_or_else(|| invalid("Segmento JPEG incompleto"))?;
        position = end;
        if (0xe0..=0xef).contains(&marker) || marker == 0xfe {
            metadata_bytes = metadata_bytes
                .checked_add(segment.len())
                .filter(|n| *n <= MAX_METADATA_BYTES)
                .ok_or_else(|| invalid("Metadatos JPEG mayores de 1 MiB"))?;
        }
        if matches!(marker,0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf) {
            if header.is_some() || !matches!(marker, 0xc0..=0xc2) {
                return Err(invalid(
                    "Sólo JPEG DCT secuencial o progresivo Huffman de un frame",
                ));
            }
            let value = parse_sof(segment, marker == 0xc2)?;

            header = Some(value);
        }
        if marker == 0xda {
            let value = header.ok_or_else(|| invalid("Scan JPEG sin SOF"))?;
            scans += 1;
            if scans > MAX_SCANS {
                return Err(invalid("JPEG supera el límite de 64 scans"));
            }
            let count = usize::from(*segment.first().ok_or_else(|| invalid("Scan JPEG vacío"))?);
            if count == 0 || count > value.components || segment.len() != 4 + 2 * count {
                return Err(invalid("Cabecera de scan JPEG inválida"));
            }
            entropy = true;
        }
    }
}
fn parse_sof(segment: &[u8], progressive: bool) -> io::Result<Header> {
    if segment.len() < 6 || segment[0] != 8 {
        return Err(invalid("JPEG requiere precisión de 8 bits"));
    }
    let height = u32::from(u16::from_be_bytes([segment[1], segment[2]]));
    let width = u32::from(u16::from_be_bytes([segment[3], segment[4]]));
    let components = usize::from(segment[5]);
    if !matches!(components, 1 | 3) || segment.len() != 6 + 3 * components {
        return Err(invalid(
            "Sólo JPEG RGB/YCbCr o gris de 8 bits; CMYK pendiente",
        ));
    }
    if width == 0
        || height == 0
        || width > MAX_SOURCE_DIMENSION
        || height > MAX_SOURCE_DIMENSION
        || u64::from(width) * u64::from(height) > MAX_SOURCE_PIXELS
    {
        return Err(invalid(
            "JPEG fuera de límites: 8192 por eje y 40 Mipíxeles",
        ));
    }
    let mut sampling = [(0u32, 0u32); 3];
    let mut sampling_blocks = 0u32;
    for (i, component) in segment[6..].as_chunks::<3>().0.iter().enumerate() {
        let (h, v) = (u32::from(component[1] >> 4), u32::from(component[1] & 15));
        if !(1..=4).contains(&h) || !(1..=4).contains(&v) {
            return Err(invalid("Muestreo JPEG inválido"));
        }
        sampling[i] = (h, v);
        sampling_blocks += h * v;
        for previous in segment[6..6 + i * 3].as_chunks::<3>().0 {
            if previous[0] == component[0] {
                return Err(invalid("Componente JPEG duplicado"));
            }
        }
    }
    if sampling_blocks > 10 {
        return Err(invalid("Más de diez bloques por MCU JPEG"));
    }
    let hmax = sampling[..components]
        .iter()
        .map(|(h, _)| *h)
        .max()
        .unwrap();
    let vmax = sampling[..components]
        .iter()
        .map(|(_, v)| *v)
        .max()
        .unwrap();
    let mcu_count = width.div_ceil(hmax * 8) as usize * height.div_ceil(vmax * 8) as usize;
    let coefficient_bytes = mcu_count
        .checked_mul(sampling_blocks as usize)
        .and_then(|n| n.checked_mul(64 * std::mem::size_of::<i16>()))
        .ok_or_else(|| invalid("Presupuesto de coeficientes JPEG desbordado"))?;
    Ok(Header {
        width,
        height,
        components,
        progressive,
        coefficient_bytes,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    fn segment(output: &mut Vec<u8>, marker: u8, payload: &[u8]) {
        output.extend_from_slice(&[0xff, marker]);
        output.extend_from_slice(&((payload.len() + 2) as u16).to_be_bytes());
        output.extend_from_slice(payload);
    }
    // Synthetic neutral YCbCr image, constant DC=0 and AC=0. One-bit canonical
    // Huffman codes for category zero and EOB. Fixture encoding is O(MCU count)
    // bits, never allocates source RGB (72 MB for a 24 MP image).
    fn zeros(output: &mut Vec<u8>, bits: usize) {
        output.resize(output.len() + bits / 8, 0);
        if !bits.is_multiple_of(8) {
            output.push((1u8 << (8 - bits % 8)) - 1);
        }
    }
    pub(crate) fn fixture(width: u16, height: u16, progressive: bool) -> Vec<u8> {
        let mut result = vec![0xff, 0xd8];
        let mut quant = vec![0];
        quant.extend_from_slice(&[1; 64]);
        segment(&mut result, 0xdb, &quant);
        let mut frame = vec![8];
        frame.extend_from_slice(&height.to_be_bytes());
        frame.extend_from_slice(&width.to_be_bytes());
        frame.extend_from_slice(&[3, 1, 0x22, 0, 2, 0x11, 0, 3, 0x11, 0]);
        segment(&mut result, if progressive { 0xc2 } else { 0xc0 }, &frame);
        let mut table = Vec::new();
        for class in [0u8, 0x10] {
            table.extend_from_slice(&[class, 1]);
            table.extend_from_slice(&[0; 15]);
            table.push(0);
        }
        segment(&mut result, 0xc4, &table);
        segment(
            &mut result,
            0xda,
            &[3, 1, 0, 2, 0, 3, 0, 0, if progressive { 0 } else { 63 }, 0],
        );
        let mcus = usize::from(width.div_ceil(16)) * usize::from(height.div_ceil(16));
        zeros(&mut result, mcus * 6 * if progressive { 1 } else { 2 });
        if progressive {
            // Tests use multiples of 16; each separate AC scan covers one
            // component's actual blocks, zero coefficient EOB in each block.
            for (component, blocks) in [
                (
                    1,
                    usize::from(width.div_ceil(8)) * usize::from(height.div_ceil(8)),
                ),
                (2, mcus),
                (3, mcus),
            ] {
                segment(&mut result, 0xda, &[1, component, 0, 1, 63, 0]);
                zeros(&mut result, blocks);
            }
        }
        result.extend_from_slice(&[0xff, 0xd9]);
        result
    }
    #[test]
    fn twelve_and_twenty_four_mp_baseline_use_scaled_idct() {
        for (w, h, expected) in [(4000, 3000, (1000, 750)), (6000, 4000, (750, 500))] {
            let jpeg = fixture(w, h, false);
            assert!(jpeg.len() < 200_000);
            let image = decode(&jpeg, 1024).unwrap();
            assert_eq!(
                (image.source_width, image.source_height),
                (u32::from(w), u32::from(h))
            );
            assert_eq!((image.width, image.height), expected);
            assert_eq!(
                image.rgb.len(),
                expected.0 as usize * expected.1 as usize * 3
            );
            assert!(image.rgb.iter().all(|value| *value == 128));
        }
    }
    #[test]
    fn small_progressive_decodes_and_large_progressive_uses_bounded_dc_preview() {
        let jpeg = fixture(128, 96, true);
        let image = decode(&jpeg, 1024).unwrap();
        assert_eq!((image.width, image.height), (128, 96));
        assert!(image.rgb.iter().all(|value| *value == 128));
        for (w, h) in [(4000, 3000), (6000, 4000)] {
            let jpeg = fixture(w, h, true);
            let header = preflight(&jpeg).unwrap();
            assert!(header.coefficient_bytes > MAX_COEFFICIENT_BYTES);
            let image = decode(&jpeg, 1024).unwrap();
            assert_eq!(
                (image.width, image.height),
                (u32::from(w).div_ceil(8), u32::from(h).div_ceil(8))
            );
            assert!(image.rgb.iter().all(|value| *value == 128));
        }
    }
    #[test]
    fn sparse_dc_matches_full_one_eighth_with_real_color_and_dc_refinements() {
        let jpeg = include_bytes!("../tests/fixtures/progressive-gradient-64x48.jpg");
        let decode = |sparse| {
            let mut decoder = Decoder::new(Cursor::new(jpeg));
            decoder.set_parallel_decoding(false);
            decoder.set_progressive_dc_preview(sparse);
            decoder.set_max_coefficient_buffer_size(MAX_COEFFICIENT_BYTES);
            assert_eq!(decoder.scale(8, 6).unwrap(), (8, 6));
            decoder.decode().unwrap()
        };
        let sparse = decode(true);
        let full = decode(false);
        assert_eq!(
            sparse, full,
            "DC preview must equal full codec at1/8, including refinement signs"
        );
        // libjpeg-turbo uses a larger chroma IDCT at4:2:0 (C API probe in
        // evidence). Equal overall scale is not the same sampling criterion.
    }
    #[test]
    fn matching_four_four_four_component_scales_agree_with_external_reference() {
        let jpeg = include_bytes!("../tests/fixtures/progressive-gradient-444-64x48.jpg");
        let mut decoder = Decoder::new(Cursor::new(jpeg));
        decoder.set_parallel_decoding(false);
        decoder.set_progressive_dc_preview(true);
        decoder.scale(8, 6).unwrap();
        let sparse = decoder.decode().unwrap();
        let reference =
            include_bytes!("../tests/fixtures/progressive-gradient-444-8x6-reference.rgb");
        assert_eq!(sparse.len(), reference.len());
        assert!(
            sparse
                .iter()
                .zip(reference)
                .all(|(a, b)| a.abs_diff(*b) <= 3),
            "matched component-scale color reference disagrees"
        );
    }
    #[test]
    fn dc_preview_refuses_wrong_scale_and_duplicate_initial_dc_scans() {
        let original = fixture(128, 96, true);
        let mut decoder = Decoder::new(Cursor::new(&original));
        decoder.set_progressive_dc_preview(true);
        decoder.set_parallel_decoding(false);
        decoder.scale(128, 96).unwrap();
        assert!(decoder.decode().unwrap_err().to_string().contains("1/8"));
        let sos = original
            .windows(2)
            .enumerate()
            .filter_map(|(i, b)| (b == [0xff, 0xda]).then_some(i))
            .collect::<Vec<_>>();
        let mut duplicate = original[..sos[1]].to_vec();
        duplicate.extend_from_slice(&original[sos[0]..sos[1]]);
        duplicate.extend_from_slice(&original[sos[1]..]);
        let mut decoder = Decoder::new(Cursor::new(duplicate));
        decoder.set_progressive_dc_preview(true);
        decoder.set_parallel_decoding(false);
        decoder.scale(16, 12).unwrap();
        assert!(
            decoder
                .decode()
                .unwrap_err()
                .to_string()
                .contains("Duplicate DC")
        );
    }
    #[test]
    fn scale_retains_small_images_and_panorama_dominant_axis() {
        for (width, height, expected) in [
            (16, 16, (16, 16)),
            (1000, 16, (1000, 16)),
            (16, 1000, (16, 1000)),
            (8192, 16, (1024, 2)),
        ] {
            let image = decode(&fixture(width, height, false), 1024).unwrap();
            assert_eq!((image.width, image.height), expected);
        }
        assert_eq!(dct_dimensions(1000, 1, 1024).unwrap(), (1000, 1));
        assert!(dct_dimensions(8192, 6000, 512).is_err());
    }
    #[test]
    fn exif_orientation_is_returned_for_host_application() {
        let original = fixture(16, 32, false);
        let mut result = original[..2].to_vec();
        segment(
            &mut result,
            0xe1,
            b"Exif\0\0II\x2a\0\x08\0\0\0\x01\0\x12\x01\x03\0\x01\0\0\0\x06\0\0\0\0\0\0\0",
        );
        result.extend_from_slice(&original[2..]);
        let image = decode(&result, 1024).unwrap();
        assert_eq!((image.width, image.height, image.orientation), (16, 32, 6));
    }
    #[test]
    fn scan_metadata_and_source_limits_precede_decode() {
        let mut data = fixture(16, 16, false);
        data.truncate(data.len() - 2);
        for _ in 0..MAX_SCANS {
            segment(&mut data, 0xda, &[1, 1, 0, 1, 63, 0]);
            zeros(&mut data, 4);
        }
        data.extend_from_slice(&[0xff, 0xd9]);
        assert!(
            preflight(&data)
                .unwrap_err()
                .to_string()
                .contains("64 scans")
        );
        let mut data = vec![0xff, 0xd8];
        for _ in 0..17 {
            segment(&mut data, 0xe2, &[0; 65530]);
        }
        data.extend_from_slice(&fixture(16, 16, false)[2..]);
        assert!(preflight(&data).unwrap_err().to_string().contains("1 MiB"));
        assert!(parse_sof(&[8, 0, 16, 0x20, 1, 1, 1, 0x11, 0], false).is_err());
        assert!(decode(b"not a JPEG", 1024).is_err());
        assert!(decode(&fixture(16, 16, false), 0).is_err());
        let mut truncated = fixture(16, 16, false);
        truncated.pop();
        assert!(preflight(&truncated).is_err());
        let mut trailing = fixture(16, 16, false);
        trailing.push(0);
        assert!(preflight(&trailing).is_err());
    }
}
