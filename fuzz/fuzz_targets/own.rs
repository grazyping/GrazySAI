#![no_main]
//! Свои файлы из произвольных байтов: архив (`.gsbrush`, `.gsplugin`, `.gsact`), плагин,
//! запись процесса `.gsrec`. Ошибка — можно, падение, зависание, гигабайты — нет.
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let base = format!("/dev/shm/grazysai-fuzz-own-{}", std::process::id());
    let pack = std::path::PathBuf::from(format!("{base}.gsplugin"));
    if std::fs::write(&pack, data).is_ok() {
        let _ = paint_formats::pack::read(&pack);
        let _ = paint_plugin::load(&pack);
    }
    let rec = std::path::PathBuf::from(format!("{base}.gsrec"));
    if std::fs::write(&rec, data).is_ok() {
        let _ = paint_formats::record::info(&rec);
    }
});
