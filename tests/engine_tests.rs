use std::path::PathBuf;

fn make_test_image(w: u32, h: u32) -> image::RgbaImage {
    image::RgbaImage::from_fn(w, h, |x, y| {
        image::Rgba([(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8, 255])
    })
}

#[test]
fn test_image_decode_and_resize() {
    let tmp = std::env::temp_dir().join("viewlume_test_decode.png");
    let img = make_test_image(1920, 1080);
    img.save(&tmp).expect("save test image");

    let file_bytes = std::fs::read(&tmp).expect("read test image");
    let dyn_img = image::load_from_memory(&file_bytes).expect("decode image");
    assert_eq!(dyn_img.width(), 1920);
    assert_eq!(dyn_img.height(), 1080);

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

    let _ = std::fs::remove_file(&tmp);
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

#[test]
fn test_navigation_with_generated_files() {
    let dir = std::env::temp_dir().join("viewlume_test_nav");
    let _ = std::fs::create_dir_all(&dir);

    let names = ["a.png", "b.png", "c.png"];
    for name in &names {
        let img = make_test_image(64, 64);
        img.save(dir.join(name)).expect("save");
    }

    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
        })
        .collect();
    files.sort_by(|a, b| {
        natord::compare(
            a.file_name().unwrap().to_str().unwrap(),
            b.file_name().unwrap().to_str().unwrap(),
        )
    });

    assert_eq!(files.len(), 3);
    assert!(files[0].ends_with("a.png"));
    assert!(files[2].ends_with("c.png"));

    for name in &names {
        let _ = std::fs::remove_file(dir.join(name));
    }
    let _ = std::fs::remove_dir(&dir);
}
