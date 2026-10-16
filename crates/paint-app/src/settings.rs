//! Настройки между запусками: простой текст `ключ=значение`, по строке на настройку.
//!
//! Где лежит: `PAINT_SETTINGS` (путь к файлу; `off` — не читать и не писать), иначе в Windows
//! `%APPDATA%\GrazySAI\settings.txt`, в Linux `$XDG_CONFIG_HOME/GrazySAI/settings.txt` или
//! `~/.config/GrazySAI/settings.txt` (папка прежнего имени `SOsai` переезжает сюда при первом
//! запуске). При проверках виртуальным пером (`PAINT_VIRTUAL_PEN`) без
//! `PAINT_SETTINGS` настройки не используются, чтобы прогоны не зависели от прошлых запусков.

use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Default)]
pub struct Settings {
    map: BTreeMap<String, String>,
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

impl Settings {
    pub fn load() -> Self {
        let mut map = BTreeMap::new();
        if let Some(text) = file().and_then(|p| std::fs::read_to_string(p).ok()) {
            for line in text.lines() {
                if let Some((k, v)) = line.split_once('=') {
                    map.insert(k.trim().to_string(), v.trim().to_string());
                }
            }
        }
        Self { map }
    }

    pub fn save(&self) {
        let Some(path) = file() else { return };
        if let Some(d) = path.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let text: String = self.map.iter().map(|(k, v)| format!("{k}={v}\n")).collect();
        let tmp = path.with_extension("tmp");
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(tmp, path);
        }
    }

    pub fn set(&mut self, key: &str, value: impl ToString) {
        self.map.insert(key.to_string(), value.to_string());
    }

    pub fn f32(&self, key: &str) -> Option<f32> {
        self.map.get(key).and_then(|v| v.parse().ok()).filter(|v: &f32| v.is_finite())
    }

    pub fn bool(&self, key: &str) -> Option<bool> {
        self.map.get(key).map(|v| v == "1" || v == "true")
    }

    pub fn remove(&mut self, key: &str) {
        self.map.remove(key);
    }

    pub fn str(&self, key: &str) -> Option<&str> {
        self.map.get(key).map(|s| s.as_str())
    }

    /// Прочитать число в поле, если оно есть.
    pub fn read_f32(&self, key: &str, into: &mut f32) {
        if let Some(v) = self.f32(key) {
            *into = v;
        }
    }

    pub fn read_bool(&self, key: &str, into: &mut bool) {
        if let Some(v) = self.bool(key) {
            *into = v;
        }
    }
}
