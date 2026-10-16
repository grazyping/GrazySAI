#![no_main]
//! Чтение .ora из произвольных байтов: ошибка — можно, падение, зависание, гигабайты памяти — нет.
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let path = std::path::PathBuf::from(format!("/dev/shm/grazysai-fuzz-{}.ora", std::process::id()));
    if std::fs::write(&path, data).is_ok() {
        let _ = paint_formats::preview::thumbnail(&path, 128);
        let _ = paint_formats::ora::load(&path);
    }
});
