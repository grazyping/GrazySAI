#![no_main]
//! Чтение .sai2 из произвольных байтов: ошибка — можно, падение, зависание, гигабайты памяти — нет.
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let path = std::path::PathBuf::from(format!("/dev/shm/grazysai-fuzz-{}.sai2", std::process::id()));
    if std::fs::write(&path, data).is_ok() {
        let _ = paint_formats::preview::thumbnail(&path, 128);
        let _ = paint_formats::sai2_doc::load(&path);
    }
});
