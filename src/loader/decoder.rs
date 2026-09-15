use std::fs;
use std::io::Cursor;
use std::path::Path;
use std::sync::Arc;

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

pub fn decode_full_image(path: &Path) -> Result<Arc<DecodedImage>, String> {
    let file_bytes = fs::read(path).map_err(|e| format!("Failed to read file: {}", e))?;
    let orientation = read_exif_orientation(&file_bytes);

    // Fast image decoding for PNG, JPEG, WebP, GIF, BMP, TIFF, QOI
    let dyn_img = image::load_from_memory(&file_bytes)
        .map_err(|e| format!("Failed to decode image: {}", e))?;

    let oriented_img = match orientation {
        2 => dyn_img.fliph(),
        3 => dyn_img.rotate180(),
        4 => dyn_img.flipv(),
        5 => dyn_img.rotate90().fliph(),
        6 => dyn_img.rotate90(),
        7 => dyn_img.rotate270().fliph(),
        8 => dyn_img.rotate270(),
        _ => dyn_img,
    };

    let rgba = oriented_img.to_rgba8();
    let width = rgba.width();
    let height = rgba.height();
    let raw_bytes = rgba.into_raw();
    let bytes_size = raw_bytes.len();

    let color_image =
        egui::ColorImage::from_rgba_unmultiplied([width as usize, height as usize], &raw_bytes);

    Ok(Arc::new(DecodedImage {
        width,
        height,
        bytes_size,
        color_image: Arc::new(color_image),
    }))
}

pub fn decode_thumbnail(path: &Path, max_dim: u32) -> Result<egui::ColorImage, String> {
    let file_bytes = fs::read(path).map_err(|e| format!("Failed to read file: {e}"))?;
    let orientation = read_exif_orientation(&file_bytes);
    let image =
        image::load_from_memory(&file_bytes).map_err(|e| format!("Failed to decode image: {e}"))?;
    let oriented = match orientation {
        2 => image.fliph(),
        3 => image.rotate180(),
        4 => image.flipv(),
        5 => image.rotate90().fliph(),
        6 => image.rotate90(),
        7 => image.rotate270().fliph(),
        8 => image.rotate270(),
        _ => image,
    };
    let thumbnail = oriented.thumbnail(max_dim, max_dim).to_rgba8();
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

    use fast_image_resize::images::Image;
    use fast_image_resize::{PixelType, ResizeOptions, Resizer};

    let src_bytes = full_image.color_image.as_raw();
    let src_image = Image::from_vec_u8(src_w, src_h, src_bytes.to_vec(), PixelType::U8x4)
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
