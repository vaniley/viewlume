use std::fs;
use std::io::Cursor;
use std::path::Path;
use std::sync::Arc;

use image::DynamicImage;

#[derive(Clone)]
pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
    pub bytes_size: usize,
    pub color_image: Arc<egui::ColorImage>,
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

fn decode_oriented_image(path: &Path) -> Result<DynamicImage, String> {
    let file_bytes = fs::read(path).map_err(|e| format!("Failed to read file: {}", e))?;
    let orientation = read_exif_orientation(&file_bytes);

    // Decode static formats from memory so EXIF orientation can be read from
    // the same byte buffer without a second filesystem read.
    let dyn_img = image::load_from_memory(&file_bytes)
        .map_err(|e| format!("Failed to decode image: {}", e))?;

    Ok(match orientation {
        2 => dyn_img.fliph(),
        3 => dyn_img.rotate180(),
        4 => dyn_img.flipv(),
        5 => dyn_img.rotate90().fliph(),
        6 => dyn_img.rotate90(),
        7 => dyn_img.rotate270().fliph(),
        8 => dyn_img.rotate270(),
        _ => dyn_img,
    })
}

fn into_decoded(image: DynamicImage) -> Arc<DecodedImage> {
    let rgba = image.to_rgba8();
    let width = rgba.width();
    let height = rgba.height();
    let raw_bytes = rgba.into_raw();
    let bytes_size = raw_bytes.len();

    let color_image =
        egui::ColorImage::from_rgba_unmultiplied([width as usize, height as usize], &raw_bytes);

    Arc::new(DecodedImage {
        width,
        height,
        bytes_size,
        color_image: Arc::new(color_image),
    })
}

pub fn decode_full_image(path: &Path) -> Result<Arc<DecodedImage>, String> {
    decode_oriented_image(path).map(into_decoded)
}

pub fn decode_full_image_with_preview<F>(
    path: &Path,
    preview_max_dim: u32,
    on_preview: F,
) -> Result<Arc<DecodedImage>, String>
where
    F: FnOnce(u32, u32, Arc<egui::ColorImage>),
{
    let image = decode_oriented_image(path)?;
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
    Ok(into_decoded(image))
}

pub fn decode_thumbnail(path: &Path, max_dim: u32) -> Result<egui::ColorImage, String> {
    let thumbnail = decode_oriented_image(path)?
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
}
