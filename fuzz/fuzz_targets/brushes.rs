#![no_main]
//! Чужие кисти из произвольных байтов: Photoshop `.abr`, GIMP `.gbr`/`.gih`, пресет Krita `.kpp`,
//! набор `.bundle`, кисть CSP `.sut` (SQLite). Ошибка — можно, падение, зависание, гигабайты — нет.
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = paint_formats::brush_tip::abr(data);
    let _ = paint_formats::brush_tip::gbr(data);
    let _ = paint_formats::brush_tip::gih_all(data);
    let base = format!("/dev/shm/grazysai-fuzz-{}", std::process::id());
    for ext in ["kpp", "bundle", "sut"] {
        let path = std::path::PathBuf::from(format!("{base}.{ext}"));
        if std::fs::write(&path, data).is_ok() {
            match ext {
                "kpp" => drop(paint_formats::krita::load_kpp(&path)),
                "bundle" => drop(paint_formats::krita::load_bundle(&path)),
                _ => drop(paint_formats::csp::load(&path)),
            }
        }
    }
});
