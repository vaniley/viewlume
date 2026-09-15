use std::path::PathBuf;

// Note: To test modules, we can create tests that test decoder and cache
#[test]
fn test_image_generation_and_loading() {
    let test_path = PathBuf::from("test_images/01_photo_gradient.jpg");
    assert!(test_path.exists(), "Test image must exist");

    // Test decoding directly
    let file_bytes = std::fs::read(&test_path).expect("Read test image");
    let dyn_img = image::load_from_memory(&file_bytes).expect("Decode image");
    assert_eq!(dyn_img.width(), 1920);
    assert_eq!(dyn_img.height(), 1080);

    // Test thumbnail downscaling with fast_image_resize
    use fast_image_resize::images::Image;
    use fast_image_resize::{PixelType, ResizeOptions, Resizer};

    let rgba = dyn_img.to_rgba8();
    let src_image = Image::from_vec_u8(1920, 1080, rgba.into_raw(), PixelType::U8x4).unwrap();
    let mut dst_image = Image::new(128, 72, PixelType::U8x4);
    let mut resizer = Resizer::new();
    resizer
        .resize(&src_image, &mut dst_image, &ResizeOptions::new())
        .unwrap();
    assert_eq!(dst_image.width(), 128);
    assert_eq!(dst_image.height(), 72);
}

#[test]
fn test_natural_sorting() {
    let mut files = vec!["img10.png", "img1.png", "img2.png", "img20.png"];
    files.sort_by(|a, b| natord::compare(a, b));
    assert_eq!(
        files,
        vec!["img1.png", "img2.png", "img10.png", "img20.png"]
    );
}
