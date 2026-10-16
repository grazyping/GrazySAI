#![no_main]
//! Картинки (PNG, JPEG, BMP, TGA, TIFF, WebP, QOI) и миниатюра jssf из произвольных байтов:
//! первый байт выбирает формат.
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Some((&k, rest)) = data.split_first() else { return };
    let ext = ["png", "jpg", "bmp", "tga", "tiff", "webp", "qoi"][k as usize % 7];
    let path = std::path::PathBuf::from(format!("/dev/shm/grazysai-fuzz-{}.{ext}", std::process::id()));
    if std::fs::write(&path, rest).is_ok() {
        let _ = paint_core::io::load_image(&path);
    }
    let _ = paint_formats::jssf::decode(rest);
});
