//! Плоские форматы для проверки чужими программами (GRAZYSAI_FORMATS_OUT — папка). Без неё пропускается.
#[test]
fn formats_to_dir() {
    let Some(dir) = std::env::var_os("GRAZYSAI_FORMATS_OUT") else { return };
    let dir = std::path::PathBuf::from(dir);
    let (w, h) = (40u32, 24u32);
    let mut v = Vec::new();
    for y in 0..h {
        for x in 0..w {
            v.extend([(x * 6) as u8, (y * 10) as u8, 120, if x < 20 { 255 } else { (y * 10) as u8 }]);
        }
    }
    for ext in ["png", "bmp", "tga", "tiff", "webp", "qoi", "jpg"] {
        paint_core::io::save_image(&dir.join(format!("t.{ext}")), w, h, &v).unwrap();
    }
}
