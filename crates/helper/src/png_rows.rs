//! Bounded row PNG adapter using the SAME png crate as image. No full-source
//! raster/deinterlace allocation. One reduced RGB bitmap + source-row map.
use crate::image_preview::{Preview, composite, fit_to, oriented_dimensions, source_coordinate};
use image::metadata::Orientation;
use std::io::{self, Cursor};
const MAX_INPUT: usize = 8 * 1024 * 1024;
const MAX_METADATA: usize = 1024 * 1024;
const MAX_PIXELS: u64 = 40 * 1024 * 1024;
const MAX_ROW: usize = 8192 * 4 * 2;
const MAX_RGB: usize = 1024 * 1024 * 3;
const MAX_CHUNKS: usize = 65536;
fn invalid(s: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, s)
}
fn codec_error(_: png::DecodingError) -> io::Error {
    invalid("PNG dañado o fuera de límites del decoder por filas")
}
#[derive(Debug)]
struct Header {
    w: u32,
    h: u32,
    interlaced: bool,
    orientation: Orientation,
}
fn preflight(data: &[u8]) -> io::Result<Header> {
    if data.len() > MAX_INPUT || !data.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(invalid("PNG inválido o mayor de8MiB"));
    }
    let mut pos = 8usize;
    let mut header = None;
    let mut metadata = 0usize;
    let mut exif = false;
    let mut image_data = false;
    for count in 0..MAX_CHUNKS {
        let chunk = data
            .get(pos..pos + 8)
            .ok_or_else(|| invalid("Chunk PNG incompleto"))?;
        let len = u32::from_be_bytes(chunk[..4].try_into().unwrap()) as usize;
        let end = pos
            .checked_add(12)
            .and_then(|n| n.checked_add(len))
            .filter(|n| *n <= data.len())
            .ok_or_else(|| invalid("Chunk PNG fuera del archivo"))?;
        let kind = &chunk[4..8];
        let payload = &data[pos + 8..end - 4];
        if kind != b"IDAT" {
            metadata = metadata
                .checked_add(len + 12)
                .filter(|n| *n <= MAX_METADATA)
                .ok_or_else(|| invalid("Metadata PNG supera1MiB"))?;
        }
        match kind {
            b"IHDR" => {
                if count != 0 || len != 13 || header.is_some() {
                    return Err(invalid("IHDR PNG no válido"));
                }
                let w = u32::from_be_bytes(payload[..4].try_into().unwrap());
                let h = u32::from_be_bytes(payload[4..8].try_into().unwrap());
                if w == 0
                    || h == 0
                    || w > 8192
                    || h > 8192
                    || u64::from(w) * u64::from(h) > MAX_PIXELS
                    || payload[12] > 1
                {
                    return Err(invalid("PNG fuera de límites8192/40Mipíxeles"));
                }
                header = Some(Header {
                    w,
                    h,
                    interlaced: payload[12] == 1,
                    orientation: Orientation::NoTransforms,
                });
            }
            b"eXIf" => {
                if exif {
                    return Err(invalid("Más de un bloque EXIF PNG"));
                }
                exif = true;
                let h = header
                    .as_mut()
                    .ok_or_else(|| invalid("Metadata antes de IHDR"))?;
                h.orientation =
                    Orientation::from_exif_chunk(payload).unwrap_or(Orientation::NoTransforms);
            }
            b"IDAT" => {
                image_data = true;
            }
            b"acTL" | b"fcTL" | b"fdAT" => return Err(invalid("Preview PNG animado pendiente")),
            b"IEND" => {
                if len != 0 || !image_data {
                    return Err(invalid("PNG sin frame completo"));
                }
                // Preserve earlier PNG padding compatibility; total input still
                // includes and bounds any bytes after IEND. Reader validates CRC.
                return header.ok_or_else(|| invalid("PNG sin IHDR"));
            }
            _ => {}
        }
        pos = end;
    }
    Err(invalid("PNG supera65536chunks"))
}
pub fn decode(data: &[u8], max: u32) -> io::Result<Preview> {
    if max == 0 || max > 1024 {
        return Err(invalid("Límite PNG fuera de1..1024"));
    }
    let h = preflight(data)?;
    let mut options = png::DecodeOptions::default();
    options.set_ignore_crc(false);
    options.set_ignore_adler32(false);
    options.set_skip_ancillary_crc_failures(false);
    options.set_ignore_text_chunk(true);
    options.set_ignore_iccp_chunk(true);
    let mut decoder = png::Decoder::new_with_options(Cursor::new(data), options);
    decoder.set_limits(png::Limits {
        bytes: 32 * 1024 * 1024,
    });
    // EXPAND palettes/low-bit gray/tRNS; keep16bits for rounded conversion
    // matching image::DynamicImage, rather than truncating low16-bit precision.
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().map_err(codec_error)?;
    if (
        reader.info().width,
        reader.info().height,
        reader.info().interlaced,
    ) != (h.w, h.h, h.interlaced)
    {
        return Err(invalid("Cabecera PNG contradictoria"));
    }
    let (color, depth) = reader.output_color_type();
    let channels = match color {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        _ => return Err(invalid("Color PNG no expandido")),
    };
    let sample_bytes = match depth {
        png::BitDepth::Eight => 1,
        png::BitDepth::Sixteen => 2,
        _ => return Err(invalid("Precisión PNG no expandida")),
    };
    let stride = channels * sample_bytes;
    if reader.output_line_size(h.w).is_none_or(|n| n > MAX_ROW) {
        return Err(invalid("Fila PNG supera64KiB"));
    }
    let (ow, oh) = oriented_dimensions(h.w, h.h, h.orientation);
    let (w, hh) = fit_to(ow, oh, max);
    let len = (w as usize)
        .checked_mul(hh as usize)
        .and_then(|n| n.checked_mul(3))
        .filter(|n| *n <= MAX_RGB)
        .ok_or_else(|| invalid("RGB PNG supera3MiB"))?;
    let mut rgb = Vec::new();
    rgb.try_reserve_exact(len)
        .map_err(|_| invalid("Sin memoria para PNG reducido"))?;
    rgb.resize(len, 0);
    let transpose = matches!(
        h.orientation,
        Orientation::Rotate90
            | Orientation::Rotate270
            | Orientation::Rotate90FlipH
            | Orientation::Rotate270FlipH
    );
    let mut selected_rows = vec![u16::MAX; h.h as usize];
    let outer = if transpose { w } else { hh };
    let inner = if transpose { hh } else { w };
    for axis in 0..outer {
        let (x, y) = if transpose { (axis, 0) } else { (0, axis) };
        let (_, sy) = source_coordinate(x * ow / w, y * oh / hh, h.w, h.h, h.orientation);
        selected_rows[sy as usize] = axis as u16;
    }
    let passes: [(u32, u32, u32, u32); 7] = [
        (0, 0, 8, 8),
        (4, 0, 8, 8),
        (0, 4, 4, 8),
        (2, 0, 4, 4),
        (0, 2, 2, 4),
        (1, 0, 2, 2),
        (0, 1, 1, 2),
    ];
    let mut writes = 0usize;
    for (pass, pattern) in passes
        .iter()
        .enumerate()
        .take(if h.interlaced { 7 } else { 1 })
    {
        let (x0, y0, dx, dy) = if h.interlaced { *pattern } else { (0, 0, 1, 1) };
        if x0 >= h.w || y0 >= h.h {
            continue;
        }
        let samples = (h.w - x0).div_ceil(dx) as usize;
        for (line, sy) in (y0..h.h).step_by(dy as usize).enumerate() {
            let row = reader
                .next_interlaced_row()
                .map_err(codec_error)?
                .ok_or_else(|| invalid("Faltan filas PNG"))?;
            let correct = match row.interlace() {
                png::InterlaceInfo::Null(_) => !h.interlaced,
                png::InterlaceInfo::Adam7(info) => {
                    h.interlaced && *info == png::Adam7Info::new((pass + 1) as u8, line as u32, h.w)
                }
            };
            if !correct || row.data().len() != samples * stride {
                return Err(invalid("Fila PNG no coincide con Adam7/cabecera"));
            }
            let axis = selected_rows[sy as usize];
            if axis == u16::MAX {
                continue;
            }
            for other in 0..inner {
                let (x, y) = if transpose {
                    (u32::from(axis), other)
                } else {
                    (other, u32::from(axis))
                };
                let (sx, expected_y) =
                    source_coordinate(x * ow / w, y * oh / hh, h.w, h.h, h.orientation);
                if expected_y != sy {
                    return Err(invalid("Fila PNG no coincide con orientación"));
                }
                if sx < x0 || !(sx - x0).is_multiple_of(dx) {
                    continue;
                }
                let at = ((sx - x0) / dx) as usize * stride;
                let pixel = pixel(&row.data()[at..at + stride], channels, sample_bytes);
                let out = (y as usize * w as usize + x as usize) * 3;
                rgb[out..out + 3].copy_from_slice(&composite(pixel, x, y));
                writes += 1;
            }
        }
    }
    if reader.next_interlaced_row().map_err(codec_error)?.is_some()
        || writes != w as usize * hh as usize
    {
        return Err(invalid("PNG reducido incompleto o con filas extra"));
    }
    reader.finish().map_err(codec_error)?;
    Ok(Preview {
        width: w,
        height: hh,
        source_width: h.w,
        source_height: h.h,
        format: "png",
        rgb,
        source: None,
        relative_source: None,
    })
}
fn pixel(data: &[u8], channels: usize, bytes: usize) -> [u8; 4] {
    let channel = |i| {
        if bytes == 1 {
            data[i]
        } else {
            ((u32::from(u16::from_be_bytes([data[i * 2], data[i * 2 + 1]])) + 128) / 257) as u8
        }
    };
    match channels {
        1 => [channel(0), channel(0), channel(0), 255],
        2 => [channel(0), channel(0), channel(0), channel(1)],
        3 => [channel(0), channel(1), channel(2), 255],
        4 => [channel(0), channel(1), channel(2), channel(3)],
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn encode(
        color: png::ColorType,
        depth: png::BitDepth,
        bytes: &[u8],
        orientation: Option<u8>,
    ) -> Vec<u8> {
        let mut result = Vec::new();
        let mut info = png::Info::with_size(5, 3);
        info.color_type = color;
        info.bit_depth = depth;
        if let Some(o) = orientation {
            let mut exif = b"II\x2a\0\x08\0\0\0\x01\0\x12\x01\x03\0\x01\0\0\0".to_vec();
            exif.extend_from_slice(&[o, 0, 0, 0, 0, 0, 0, 0]);
            info.exif_metadata = Some(exif.into());
        }
        let mut encoder = png::Encoder::with_info(&mut result, info).unwrap();
        if color == png::ColorType::Indexed {
            encoder.set_palette(vec![240, 30, 40, 40, 220, 90, 20, 40, 230]);
            encoder.set_trns(vec![255, 128, 0]);
        }
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(bytes).unwrap();
        writer.finish().unwrap();
        result
    }
    fn reference(data: &[u8], max: u32, o: Orientation) -> Preview {
        let image = image::load_from_memory_with_format(data, image::ImageFormat::Png).unwrap();
        let (w, h) = (image.width(), image.height());
        crate::image_preview::rasterize_to(image, o, w, h, "png", max).unwrap()
    }
    #[test]
    fn rows_match_full_reference_all_color_types_and_eight_sixteen_bits() {
        for (color, channels) in [
            (png::ColorType::Grayscale, 1),
            (png::ColorType::GrayscaleAlpha, 2),
            (png::ColorType::Rgb, 3),
            (png::ColorType::Rgba, 4),
        ] {
            for depth in [png::BitDepth::Eight, png::BitDepth::Sixteen] {
                let mut bytes = Vec::new();
                for i in 0..15 * channels {
                    if depth == png::BitDepth::Eight {
                        bytes.push(((i * 53 + 13) % 256) as u8);
                    } else {
                        let values = [0u16, 1, 128, 255, 256, 511, 32768, 65534, 65535];
                        bytes.extend_from_slice(&values[i % values.len()].to_be_bytes());
                    }
                }
                let data = encode(color, depth, &bytes, None);
                for max in [3, 1024] {
                    let row = decode(&data, max).unwrap();
                    let full = reference(&data, max, Orientation::NoTransforms);
                    assert_eq!(
                        (row.width, row.height, row.rgb),
                        (full.width, full.height, full.rgb),
                        "{color:?}/{depth:?}"
                    );
                }
            }
        }
    }
    #[test]
    fn low_bit_gray_and_palette_transparency_expand_before_sampling() {
        for depth in [png::BitDepth::One, png::BitDepth::Two, png::BitDepth::Four] {
            let bits = depth as usize;
            let bytes_per_row = (5 * bits).div_ceil(8);
            let data = encode(
                png::ColorType::Grayscale,
                depth,
                &vec![0x69; bytes_per_row * 3],
                None,
            );
            assert_eq!(
                decode(&data, 3).unwrap().rgb,
                reference(&data, 3, Orientation::NoTransforms).rgb
            );
        }
        let data = encode(
            png::ColorType::Indexed,
            png::BitDepth::Two,
            &[0x18, 0, 0x61, 0x80, 0x86, 0x40],
            None,
        );
        assert_eq!(
            decode(&data, 1024).unwrap().rgb,
            reference(&data, 1024, Orientation::NoTransforms).rgb
        );
    }
    #[test]
    fn all_eight_orientations_scatter_without_second_rgb_bitmap() {
        let pixels = (0..60).map(|i| (i * 31 % 256) as u8).collect::<Vec<_>>();
        for value in 1..=8 {
            let data = encode(
                png::ColorType::Rgba,
                png::BitDepth::Eight,
                &pixels,
                Some(value),
            );
            let o = Orientation::from_exif(value).unwrap();
            for max in [3, 1024] {
                let row = decode(&data, max).unwrap();
                let full = reference(&data, max, o);
                assert_eq!(
                    (row.width, row.height, row.rgb),
                    (full.width, full.height, full.rgb),
                    "orientation{value}"
                );
            }
        }
    }
    #[test]
    fn adam7_odd_dimensions_match_full_frame_without_deinterlace_raster() {
        let data = include_bytes!("../tests/fixtures/adam7-rgba-31x23.png");
        assert!(preflight(data).unwrap().interlaced);
        for max in [1, 7, 1024] {
            let row = decode(data, max).unwrap();
            let full = reference(data, max, Orientation::NoTransforms);
            assert_eq!(
                (row.width, row.height, row.rgb),
                (full.width, full.height, full.rgb)
            );
        }
    }
    fn insert_exif(data: &[u8], value: u8, late: bool) -> Vec<u8> {
        let mut payload = b"II\x2a\0\x08\0\0\0\x01\0\x12\x01\x03\0\x01\0\0\0".to_vec();
        payload.extend_from_slice(&[value, 0, 0, 0, 0, 0, 0, 0]);
        let mut crc = u32::MAX;
        for byte in b"eXIf".iter().chain(&payload) {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                crc = (crc >> 1) ^ if crc & 1 != 0 { 0xedb88320 } else { 0 };
            }
        }
        let mut chunk = Vec::new();
        chunk.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        chunk.extend_from_slice(b"eXIf");
        chunk.extend_from_slice(&payload);
        chunk.extend_from_slice(&(!crc).to_be_bytes());
        let at = if late { data.len() - 12 } else { 33 };
        let mut result = data[..at].to_vec();
        result.extend_from_slice(&chunk);
        result.extend_from_slice(&data[at..]);
        result
    }
    #[test]
    fn adam7_empty_passes_transpose_and_late_exif_match_reference() {
        for data in [
            include_bytes!("../tests/fixtures/adam7-rgba-31x23.png").as_slice(),
            include_bytes!("../tests/fixtures/adam7-rgba-1x19.png").as_slice(),
            include_bytes!("../tests/fixtures/adam7-rgba-19x1.png").as_slice(),
        ] {
            for value in 1..=8 {
                let o = Orientation::from_exif(value).unwrap();
                let oriented = insert_exif(data, value, true);
                for max in [1, 7, 1024] {
                    let row = decode(&oriented, max).unwrap();
                    let full = reference(data, max, o);
                    assert_eq!(
                        (row.width, row.height, row.rgb),
                        (full.width, full.height, full.rgb)
                    );
                }
            }
        }
    }
    #[test]
    fn late_metadata_crc_and_over_budget_chunks_do_not_publish_pixels() {
        let data = encode(png::ColorType::Rgb, png::BitDepth::Eight, &[120; 45], None);
        let mut late = insert_exif(&data, 6, true);
        let end = late.len() - 12;
        late[end - 1] ^= 1;
        assert!(decode(&late, 1024).is_err());
        let mut oversized = data[..33].to_vec();
        oversized.extend_from_slice(&(MAX_METADATA as u32).to_be_bytes());
        oversized.extend_from_slice(b"tEXt");
        oversized.resize(oversized.len() + MAX_METADATA + 4, 0);
        oversized.extend_from_slice(&data[33..]);
        assert!(
            preflight(&oversized)
                .unwrap_err()
                .to_string()
                .contains("Metadata")
        );
        let mut truncated = data;
        truncated[33..37].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(preflight(&truncated).is_err());
    }
    #[test]
    fn large_png_is_encoded_and_decoded_by_rows_not_full_source_rgb() {
        use std::io::Write;
        let (w, h) = (4000, 3000);
        let mut data = Vec::new();
        let mut encoder = png::Encoder::new(&mut data, w, h);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        {
            let mut stream = writer.stream_writer().unwrap();
            let red = [230, 20, 40].repeat(w as usize);
            let blue = [20, 40, 230].repeat(w as usize);
            for y in 0..h {
                stream
                    .write_all(if y < h / 2 { &red } else { &blue })
                    .unwrap();
            }
            stream.finish().unwrap();
        }
        writer.finish().unwrap();
        let preview = decode(&data, 1024).unwrap();
        assert_eq!(
            (
                preview.width,
                preview.height,
                preview.source_width,
                preview.source_height
            ),
            (1024, 768, w, h)
        );
        assert_eq!(&preview.rgb[..3], &[230, 20, 40]);
        assert_eq!(&preview.rgb[preview.rgb.len() - 3..], &[20, 40, 230]);
        assert!(preview.rgb.capacity() <= MAX_RGB);
        let thumb = decode(&data, 160).unwrap();
        assert_eq!((thumb.width, thumb.height), (160, 120));
    }
    #[test]
    fn malformed_crc_truncation_metadata_and_animation_fail_closed() {
        let data = encode(png::ColorType::Rgb, png::BitDepth::Eight, &[120; 45], None);
        for n in [0, 8, 20, data.len() - 1] {
            assert!(decode(&data[..n], 1024).is_err());
        }
        let mut bad = data.clone();
        bad[29] ^= 1;
        assert!(decode(&bad, 1024).is_err());
        assert!(decode(&data, 0).is_err());
        assert!(decode(&data, 1025).is_err());
        let mut header = data.clone();
        header[16..20].copy_from_slice(&8193u32.to_be_bytes());
        assert!(preflight(&header).is_err());
        let mut animation = Vec::new();
        let mut encoder = png::Encoder::new(&mut animation, 1, 1);
        encoder.set_animated(1, 0).unwrap();
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&[0]).unwrap();
        writer.finish().unwrap();
        assert!(
            decode(&animation, 1024)
                .unwrap_err()
                .to_string()
                .contains("animado")
        );
    }
}
