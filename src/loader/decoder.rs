use std::fs;
use std::io::Cursor;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use image::{AnimationDecoder, DynamicImage, ImageFormat};

const MAX_FILE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_IMAGE_PIXELS: u64 = 200_000_000;
const MAX_DECODED_BYTES: u64 = 800 * 1024 * 1024;
const MAX_ANIMATION_FRAMES: usize = 500;

#[derive(Clone)]
pub struct AnimationFrame {
    pub color_image: Arc<egui::ColorImage>,
    pub delay: Duration,
}

#[derive(Clone)]
pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
    pub bytes_size: usize,
    pub color_image: Arc<egui::ColorImage>,
    pub animation_frames: Arc<[AnimationFrame]>,
}

fn read_exif_orientation(file_bytes: &[u8]) -> u32 {
    let mut cursor = Cursor::new(file_bytes);
    let exifreader = exif::Reader::new();
    if let Ok(exif_data) = exifreader.read_from_container(&mut cursor) {
        if let Some(field) = exif_data.get_field(exif::Tag::Orientation, exif::In::PRIMARY) {
            if let Some(val) = field.value.get_uint(0) {
                return val;
            }
        }
    }
    1
}

fn validate_dimensions(width: u32, height: u32) -> Result<(), String> {
    let pixels = u64::from(width)
        .checked_mul(u64::from(height))
        .ok_or_else(|| "Image dimensions overflow".to_string())?;
    let bytes = pixels
        .checked_mul(4)
        .ok_or_else(|| "Decoded image size overflow".to_string())?;
    if pixels > MAX_IMAGE_PIXELS || bytes > MAX_DECODED_BYTES {
        return Err(format!(
            "Image is too large to open safely ({width}×{height}); limit is {MAX_IMAGE_PIXELS} pixels"
        ));
    }
    Ok(())
}

fn frame_to_color(frame: image::Frame) -> AnimationFrame {
    let (numerator, denominator) = frame.delay().numer_denom_ms();
    let delay_ms = if denominator == 0 {
        100
    } else {
        (u64::from(numerator) / u64::from(denominator)).max(10)
    };
    let buffer = frame.into_buffer();
    let size = [buffer.width() as usize, buffer.height() as usize];
    AnimationFrame {
        color_image: Arc::new(egui::ColorImage::from_rgba_unmultiplied(
            size,
            buffer.as_raw(),
        )),
        delay: Duration::from_millis(delay_ms),
    }
}

fn decode_animation(bytes: &[u8], format: ImageFormat) -> Result<Vec<AnimationFrame>, String> {
    let frames = match format {
        ImageFormat::Gif => image::codecs::gif::GifDecoder::new(Cursor::new(bytes))
            .map_err(|error| format!("Failed to decode GIF: {error}"))?
            .into_frames()
            .collect_frames(),
        ImageFormat::WebP => image::codecs::webp::WebPDecoder::new(Cursor::new(bytes))
            .map_err(|error| format!("Failed to decode WebP: {error}"))?
            .into_frames()
            .collect_frames(),
        _ => return Ok(Vec::new()),
    }
    .map_err(|error| format!("Failed to decode animation: {error}"))?;

    if frames.len() <= 1 {
        return Ok(Vec::new());
    }
    if frames.len() > MAX_ANIMATION_FRAMES {
        return Err(format!(
            "Animation has too many frames ({}); limit is {MAX_ANIMATION_FRAMES}",
            frames.len()
        ));
    }

    let mut decoded_bytes = 0_u64;
    let mut decoded = Vec::with_capacity(frames.len());
    for frame in frames {
        validate_dimensions(frame.buffer().width(), frame.buffer().height())?;
        decoded_bytes = decoded_bytes.saturating_add(frame.buffer().len() as u64);
        if decoded_bytes > MAX_DECODED_BYTES {
            return Err("Animation requires too much decoded memory".to_string());
        }
        decoded.push(frame_to_color(frame));
    }
    Ok(decoded)
}

fn decode_oriented_image(path: &Path) -> Result<(DynamicImage, Vec<AnimationFrame>), String> {
    let metadata = fs::metadata(path).map_err(|e| format!("Failed to inspect file: {e}"))?;
    if metadata.len() > MAX_FILE_BYTES {
        return Err(format!(
            "File is too large to open safely ({} MiB); limit is {} MiB",
            metadata.len() / 1024 / 1024,
            MAX_FILE_BYTES / 1024 / 1024
        ));
    }
    let file_bytes = fs::read(path).map_err(|e| format!("Failed to read file: {}", e))?;
    let orientation = read_exif_orientation(&file_bytes);
    let format = ImageFormat::from_path(path)
        .or_else(|_| image::guess_format(&file_bytes))
        .map_err(|error| format!("Unknown or unsupported image format: {error}"))?;
    let (width, height) = image::ImageReader::with_format(Cursor::new(&file_bytes), format)
        .into_dimensions()
        .map_err(|error| format!("Failed to read image dimensions: {error}"))?;
    validate_dimensions(width, height)?;
    let animation_frames = decode_animation(&file_bytes, format)?;

    // Decode static formats from memory so EXIF orientation can be read from
    // the same byte buffer without a second filesystem read.
    let dyn_img = image::load_from_memory_with_format(&file_bytes, format)
        .map_err(|e| format!("Failed to decode image: {}", e))?;
    validate_dimensions(dyn_img.width(), dyn_img.height())?;

    let image = match orientation {
        2 => dyn_img.fliph(),
        3 => dyn_img.rotate180(),
        4 => dyn_img.flipv(),
        5 => dyn_img.rotate90().fliph(),
        6 => dyn_img.rotate90(),
        7 => dyn_img.rotate270().fliph(),
        8 => dyn_img.rotate270(),
        _ => dyn_img,
    };
    Ok((image, animation_frames))
}

fn into_decoded(image: DynamicImage, animation_frames: Vec<AnimationFrame>) -> Arc<DecodedImage> {
    let rgba = image.to_rgba8();
    let width = rgba.width();
    let height = rgba.height();
    let raw_bytes = rgba.into_raw();
    let bytes_size = raw_bytes.len()
        + animation_frames
            .iter()
            .map(|frame| frame.color_image.as_raw().len() * std::mem::size_of::<egui::Color32>())
            .sum::<usize>();

    let color_image =
        egui::ColorImage::from_rgba_unmultiplied([width as usize, height as usize], &raw_bytes);

    Arc::new(DecodedImage {
        width,
        height,
        bytes_size,
        color_image: Arc::new(color_image),
        animation_frames: animation_frames.into(),
    })
}

pub fn decode_full_image(path: &Path) -> Result<Arc<DecodedImage>, String> {
    decode_oriented_image(path).map(|(image, frames)| into_decoded(image, frames))
}

pub fn decode_full_image_with_preview<F>(
    path: &Path,
    preview_max_dim: u32,
    on_preview: F,
) -> Result<Arc<DecodedImage>, String>
where
    F: FnOnce(u32, u32, Arc<egui::ColorImage>),
{
    let (image, frames) = decode_oriented_image(path)?;
    let (width, height) = (image.width(), image.height());
    if width.max(height) > preview_max_dim.saturating_mul(2) {
        let preview = image.thumbnail(preview_max_dim, preview_max_dim).to_rgba8();
        let preview_size = [preview.width() as usize, preview.height() as usize];
        let preview = Arc::new(egui::ColorImage::from_rgba_unmultiplied(
            preview_size,
            preview.as_raw(),
        ));
        on_preview(width, height, preview);
    }
    Ok(into_decoded(image, frames))
}

pub fn decode_thumbnail(path: &Path, max_dim: u32) -> Result<egui::ColorImage, String> {
    let thumbnail = decode_oriented_image(path)?
        .0
        .thumbnail(max_dim, max_dim)
        .to_rgba8();
    let size = [thumbnail.width() as usize, thumbnail.height() as usize];
    Ok(egui::ColorImage::from_rgba_unmultiplied(
        size,
        thumbnail.as_raw(),
    ))
}

pub fn generate_thumbnail(
    full_image: &DecodedImage,
    max_dim: u32,
) -> Result<egui::ColorImage, String> {
    let (src_w, src_h) = (full_image.width, full_image.height);
    if src_w == 0 || src_h == 0 {
        return Err("Zero dimension image".to_string());
    }

    let scale = (max_dim as f32 / src_w.max(src_h) as f32).min(1.0);
    let dst_w = ((src_w as f32 * scale).round() as u32).max(1);
    let dst_h = ((src_h as f32 * scale).round() as u32).max(1);

    if dst_w == src_w && dst_h == src_h {
        return Ok((*full_image.color_image).clone());
    }

    use fast_image_resize::images::{Image, ImageRef};
    use fast_image_resize::{PixelType, ResizeOptions, Resizer};

    let src_bytes = full_image.color_image.as_raw();
    let src_image = ImageRef::new(src_w, src_h, src_bytes, PixelType::U8x4)
        .map_err(|e| format!("fast_image_resize src error: {:?}", e))?;

    let mut dst_image = Image::new(dst_w, dst_h, PixelType::U8x4);
    let mut resizer = Resizer::new();
    let options = ResizeOptions::new();

    resizer
        .resize(&src_image, &mut dst_image, &options)
        .map_err(|e| format!("Resize error: {:?}", e))?;

    Ok(egui::ColorImage::from_rgba_unmultiplied(
        [dst_w as usize, dst_h as usize],
        dst_image.buffer(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(extension: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "viewlume-decoder-test-{}-{extension}.{extension}",
            std::process::id()
        ))
    }

    #[test]
    fn preview_keeps_full_dimensions_with_a_small_pixel_buffer() {
        let path =
            std::env::temp_dir().join(format!("viewlume-preview-test-{}.png", std::process::id()));
        image::RgbaImage::from_pixel(800, 400, image::Rgba([20, 40, 60, 255]))
            .save(&path)
            .expect("save preview fixture");

        let mut preview_size = None;
        let decoded = decode_full_image_with_preview(&path, 128, |width, height, preview| {
            assert_eq!((width, height), (800, 400));
            preview_size = Some(preview.size);
        })
        .expect("decode image with preview");

        assert_eq!((decoded.width, decoded.height), (800, 400));
        assert_eq!(preview_size, Some([128, 64]));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn rejects_dimensions_that_exceed_the_memory_limit() {
        let error = validate_dimensions(50_000, 50_000).expect_err("must reject huge image");
        assert!(error.contains("too large"));
    }

    #[test]
    fn decodes_multiple_gif_frames_with_delays() {
        use image::codecs::gif::GifEncoder;
        use image::{Delay, Frame};

        let path = temp_path("gif");
        let file = std::fs::File::create(&path).expect("create GIF fixture");
        let mut encoder = GifEncoder::new(file);
        let frames = [egui::Color32::RED, egui::Color32::BLUE].map(|color| {
            let [r, g, b, a] = color.to_array();
            Frame::from_parts(
                image::RgbaImage::from_pixel(2, 2, image::Rgba([r, g, b, a])),
                0,
                0,
                Delay::from_numer_denom_ms(80, 1),
            )
        });
        encoder.encode_frames(frames).expect("encode GIF fixture");
        drop(encoder);

        let decoded = decode_full_image(&path).expect("decode animated GIF");
        assert_eq!(decoded.animation_frames.len(), 2);
        assert_eq!(decoded.animation_frames[0].delay, Duration::from_millis(80));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn decodes_built_in_static_formats() {
        let rgba = DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            3,
            2,
            image::Rgba([20, 80, 160, 255]),
        ));
        for (extension, format) in [
            ("webp", ImageFormat::WebP),
            ("ico", ImageFormat::Ico),
            ("tga", ImageFormat::Tga),
            ("pnm", ImageFormat::Pnm),
        ] {
            let path = temp_path(extension);
            rgba.save_with_format(&path, format)
                .unwrap_or_else(|error| panic!("encode {extension} fixture: {error}"));
            let decoded = decode_full_image(&path)
                .unwrap_or_else(|error| panic!("decode {extension} fixture: {error}"));
            assert_eq!((decoded.width, decoded.height), (3, 2));
            let _ = std::fs::remove_file(path);
        }

        let ff_path = temp_path("ff");
        DynamicImage::ImageRgba16(image::ImageBuffer::from_pixel(
            3,
            2,
            image::Rgba([1000, 2000, 3000, u16::MAX]),
        ))
        .save_with_format(&ff_path, ImageFormat::Farbfeld)
        .expect("encode Farbfeld fixture");
        let decoded = decode_full_image(&ff_path).expect("decode Farbfeld fixture");
        assert_eq!((decoded.width, decoded.height), (3, 2));
        let _ = std::fs::remove_file(ff_path);

        // Minimal 4×4 DXT1 DDS fixture (DDS has no encoder in `image`).
        let dds_path = temp_path("dds");
        let mut dds = Vec::from(*b"DDS ");
        for value in [124_u32, 0x0008_1007, 4, 4, 8, 0, 0] {
            dds.extend_from_slice(&value.to_le_bytes());
        }
        dds.extend_from_slice(&[0; 44]);
        for value in [32_u32, 0x4, 0x3154_5844, 0, 0, 0, 0, 0, 0x1000, 0, 0, 0, 0] {
            dds.extend_from_slice(&value.to_le_bytes());
        }
        dds.extend_from_slice(&[0x00, 0xf8, 0x00, 0x00, 0, 0, 0, 0]);
        std::fs::write(&dds_path, dds).expect("write DDS fixture");
        let decoded = decode_full_image(&dds_path).expect("decode DDS fixture");
        assert_eq!((decoded.width, decoded.height), (4, 4));
        let _ = std::fs::remove_file(dds_path);

        let float_image = DynamicImage::ImageRgb32F(image::Rgb32FImage::from_pixel(
            3,
            2,
            image::Rgb([0.1, 0.4, 0.8]),
        ));
        for (extension, format) in [("hdr", ImageFormat::Hdr), ("exr", ImageFormat::OpenExr)] {
            let path = temp_path(extension);
            float_image
                .save_with_format(&path, format)
                .unwrap_or_else(|error| panic!("encode {extension} fixture: {error}"));
            let decoded = decode_full_image(&path)
                .unwrap_or_else(|error| panic!("decode {extension} fixture: {error}"));
            assert_eq!((decoded.width, decoded.height), (3, 2));
            let _ = std::fs::remove_file(path);
        }
    }
}
