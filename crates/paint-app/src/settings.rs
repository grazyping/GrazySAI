//! Настройки между запусками: простой текст `ключ=значение`, по строке на настройку.
//!
//! Где лежит: `PAINT_SETTINGS` (путь к файлу; `off` — не читать и не писать), иначе в Windows
//! `%APPDATA%\GrazySAI\settings.txt`, в Linux `$XDG_CONFIG_HOME/GrazySAI/settings.txt` или
//! `~/.config/GrazySAI/settings.txt` (папка прежнего имени `SOsai` переезжает сюда при первом
//! запуске). При проверках виртуальным пером (`PAINT_VIRTUAL_PEN`) без
//! `PAINT_SETTINGS` настройки не используются, чтобы прогоны не зависели от прошлых запусков.

use std::path::PathBuf;

/// Настройки — тот же текст `ключ=значение`, что у файла кисти (`paint_brush::Kv`).
pub type Settings = paint_brush::Kv;

/// Чтение и запись файла настроек.
pub trait SettingsFile: Sized {
    fn load() -> Self;
    fn save(&self);
}

impl SettingsFile for Settings {
    fn load() -> Self {
        file().and_then(|p| std::fs::read_to_string(p).ok()).map_or_else(Self::default, |t| Self::from_text(&t))
    }

    fn save(&self) {
        let Some(path) = file() else { return };
        if let Some(d) = path.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = paint_core::io::write_atomic(&path, self.to_text().as_bytes());
    }
}

/// Папка настроек и восстановления (`None` — настройки выключены).
pub fn dir() -> Option<PathBuf> {
    match std::env::var("PAINT_SETTINGS") {
        Ok(v) if v == "off" => return None,
        Ok(v) => return PathBuf::from(v).parent().map(|p| p.to_path_buf()),
        Err(_) => {}
    }
    if std::env::var_os("PAINT_VIRTUAL_PEN").is_some() {
        return None;
    }
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
    };
    let base = base?;
    let dir = base.join("GrazySAI");
    // до 0.8.42 программа звалась SOsai: её папка (настройки, недавние файлы, копии для
    // восстановления) переезжает под новое имя один раз; не вышло переименовать — берётся старая
    let old = base.join(concat!("SO", "sai"));
    if !dir.exists() && old.is_dir() && std::fs::rename(&old, &dir).is_err() {
        return Some(old);
    }
    Some(dir)
}

fn file() -> Option<PathBuf> {
    match std::env::var("PAINT_SETTINGS") {
        Ok(v) if v == "off" => None,
        Ok(v) => Some(PathBuf::from(v)),
        Err(_) => dir().map(|d| d.join("settings.txt")),
    }
}

