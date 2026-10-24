//! Библиотека кистей: сторонние кисти (Clip Studio, Krita, Photoshop, GIMP, файлы GrazySAI)
//! хранятся в своей папке программы, `library/`: каждая кисть — файл `<id>.gsbrush` и образец
//! мазка `<id>.png`; сведения (имя, откуда, набор, метки, избранное, «новая») — в `library.txt`.
//! Исходные файлы копируются в `library/originals/` — библиотека не зависит от того, где они
//! лежали. Окно библиотеки — в `main.rs`.

use crate::i18n::tr;
use paint_brush::{Brush, Kv};
use std::path::{Path, PathBuf};

/// Одна кисть библиотеки.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub id: String,
    pub name: String,
    /// Откуда: «Clip Studio», «Krita», «Photoshop», «GIMP», «GrazySAI».
    pub source: String,
    /// Набор — имя исходного файла без расширения.
    pub set: String,
    pub tags: Vec<String>,
    pub fav: bool,
    /// Добавлена и ещё не просмотрена.
    pub new: bool,
    /// Когда добавлена (секунды от 1970).
    pub added: u64,
}

pub struct Library {
    pub dir: PathBuf,
    pub entries: Vec<Entry>,
}

/// Расширения файлов, которые принимает библиотека.
pub const EXTS: [&str; 11] = ["saitdat", "saitgrp", "sut", "bfps", "kpp", "bundle", "abr", "gbr", "gih", "gsbrush", "gsbrushes"];

pub fn is_brush_file(p: &Path) -> bool {
    p.extension().and_then(|e| e.to_str()).is_some_and(|e| EXTS.contains(&e.to_ascii_lowercase().as_str()))
}

impl Library {
    /// Библиотека из своей папки программы (`None` — настройки выключены).
    pub fn open() -> Option<Self> {
        Some(Self::at(crate::settings::dir()?.join("library")))
    }

    pub fn at(dir: PathBuf) -> Self {
        let s = std::fs::read_to_string(dir.join("library.txt")).map(|t| Kv::from_text(&t)).unwrap_or_default();
        let n = s.f32("count").map_or(0, |v| v as usize).min(100_000);
        let entries = (0..n)
            .filter_map(|i| {
                let k = |f: &str| format!("{i}.{f}");
                let id = s.str(&k("id"))?.to_string();
                // id — только буквы и цифры: из него строится имя файла
                if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric()) {
                    return None;
                }
                Some(Entry {
                    name: s.str(&k("name")).unwrap_or("").to_string(),
                    source: s.str(&k("source")).unwrap_or("").to_string(),
                    set: s.str(&k("set")).unwrap_or("").to_string(),
                    tags: s.str(&k("tags")).unwrap_or("").split(',').map(str::trim).filter(|t| !t.is_empty()).map(str::to_string).collect(),
                    fav: s.bool(&k("fav")).unwrap_or(false),
                    new: s.bool(&k("new")).unwrap_or(false),
                    added: s.f32(&k("added")).map_or(0, |v| v as u64),
                    id,
                })
            })
            .filter(|e| dir.join(format!("{}.gsbrush", e.id)).exists())
            .collect();
        Self { dir, entries }
    }

    pub fn save(&self) {
        let mut s = Kv::default();
        s.set("count", self.entries.len());
        for (i, e) in self.entries.iter().enumerate() {
            let k = |f: &str| format!("{i}.{f}");
            s.set(&k("id"), &e.id);
            s.set(&k("name"), &e.name);
            s.set(&k("source"), &e.source);
            s.set(&k("set"), &e.set);
            s.set(&k("tags"), e.tags.join(", "));
            s.set(&k("fav"), e.fav as u8);
            s.set(&k("new"), e.new as u8);
            s.set(&k("added"), e.added);
        }
        let _ = std::fs::create_dir_all(&self.dir);
        let _ = paint_core::io::write_atomic(&self.dir.join("library.txt"), s.to_text().as_bytes());
    }

    fn fresh_id(&self) -> String {
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_micros());
        let mut n = 0u32;
        loop {
            let id = format!("b{t:x}{n}");
            if !self.entries.iter().any(|e| e.id == id) && !self.dir.join(format!("{id}.gsbrush")).exists() {
                return id;
            }
            n += 1;
        }
    }

    /// Добавить кисть: файл кисти и образец мазка; возвращает её id.
    pub fn add(&mut self, brush: &Brush, name: &str, source: &str, set: &str) -> Result<String, String> {
        std::fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        let id = self.fresh_id();
        paint_brush::save(&self.dir.join(format!("{id}.gsbrush")), &[(brush, name.to_string())]).map_err(|e| e.to_string())?;
        let (w, h) = (240, 60);
        let rgba = brush.preview_layer_rgba(w, h);
        let _ = paint_core::io::save_image(&self.dir.join(format!("{id}.png")), w, h, &rgba);
        let added = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        self.entries.push(Entry { id: id.clone(), name: name.to_string(), source: source.to_string(), set: set.to_string(), tags: Vec::new(), fav: false, new: true, added });
        Ok(id)
    }

    /// Кисть из библиотеки (картинки отпечатков и текстуры — в свою папку программы).
    pub fn load(&self, id: &str) -> Option<Brush> {
        let files = paint_formats::pack::read(&self.dir.join(format!("{id}.gsbrush"))).ok()?;
        paint_brush::from_files(&files, tr("Кисть")).into_iter().next()
    }

    /// Образец мазка: RGBA и размер.
    pub fn preview(&self, id: &str) -> Option<(u32, u32, Vec<u8>)> {
        paint_core::io::load_image(&self.dir.join(format!("{id}.png"))).ok()
    }

    pub fn remove(&mut self, id: &str) {
        self.entries.retain(|e| e.id != id);
        let _ = std::fs::remove_file(self.dir.join(format!("{id}.gsbrush")));
        let _ = std::fs::remove_file(self.dir.join(format!("{id}.png")));
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut Entry> {
        self.entries.iter_mut().find(|e| e.id == id)
    }

    /// Копия исходного файла в `originals/` (если такого ещё нет).
    pub fn keep_original(&self, path: &Path) {
        let dir = self.dir.join("originals");
        let Some(name) = path.file_name() else { return };
        let to = dir.join(name);
        if !to.exists() && std::fs::create_dir_all(&dir).is_ok() {
            let _ = std::fs::copy(path, to);
        }
    }

    /// Все «откуда», наборы и метки — для навигации (по алфавиту, без повторов).
    pub fn groups(&self) -> (Vec<String>, Vec<String>, Vec<String>) {
        let mut sources: Vec<String> = self.entries.iter().map(|e| e.source.clone()).collect();
        let mut sets: Vec<String> = self.entries.iter().map(|e| e.set.clone()).filter(|s| !s.is_empty()).collect();
        let mut tags: Vec<String> = self.entries.iter().flat_map(|e| e.tags.clone()).collect();
        for v in [&mut sources, &mut sets, &mut tags] {
            v.sort_by_key(|s| s.to_lowercase());
            v.dedup();
        }
        (sources, sets, tags)
    }
}

/// Что показывать в окне библиотеки.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum Filter {
    #[default]
    All,
    Fav,
    New,
    Source(String),
    Set(String),
    Tag(String),
}

impl Filter {
    pub fn matches(&self, e: &Entry, search: &str) -> bool {
        let ok = match self {
            Filter::All => true,
            Filter::Fav => e.fav,
            Filter::New => e.new,
            Filter::Source(s) => &e.source == s,
            Filter::Set(s) => &e.set == s,
            Filter::Tag(t) => e.tags.contains(t),
        };
        let q = search.trim().to_lowercase();
        ok && (q.is_empty() || e.name.to_lowercase().contains(&q) || e.set.to_lowercase().contains(&q) || e.tags.iter().any(|t| t.to_lowercase().contains(&q)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Кисть добавляется, сведения переживают перезапуск, удаление убирает файлы.
    #[test]
    fn add_save_reopen_remove() {
        let dir = std::env::temp_dir().join(format!("gs-library-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut lib = Library::at(dir.clone());
        let b = paint_brush::ToolKind::Pencil.new_brush();
        let id = lib.add(&b, "Проба", "Krita", "набор").unwrap();
        lib.get_mut(&id).unwrap().tags = vec!["линии".into()];
        lib.save();
        let back = Library::at(dir.clone());
        assert_eq!(back.entries.len(), 1);
        assert_eq!(back.entries[0].tags, vec!["линии".to_string()]);
        assert!(back.entries[0].new);
        assert!(back.load(&id).is_some());
        assert!(back.preview(&id).is_some());
        assert!(Filter::Tag("линии".into()).matches(&back.entries[0], ""));
        assert!(Filter::All.matches(&back.entries[0], "проб"));
        assert!(!Filter::Fav.matches(&back.entries[0], ""));
        let mut back = back;
        back.remove(&id);
        assert!(!dir.join(format!("{id}.gsbrush")).exists());
        std::fs::remove_dir_all(dir).ok();
    }
}
