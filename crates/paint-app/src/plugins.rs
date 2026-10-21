//! Плагины в рисовалке: установленные пакеты (`plugins/` в папке настроек, `plugins/dev/` — из
//! мастерской), установка и холст для плагина поверх документа (`paint_plugin::Canvas`).

use paint_core::pixel::{from_rgba8, to_rgba8};
use paint_core::tile::ColorMap;
use paint_core::{Document, Layer};
use std::path::{Path, PathBuf};

pub struct Installed {
    pub plugin: paint_plugin::Plugin,
    pub path: PathBuf,
    /// Из мастерской (`plugins/dev/`).
    pub dev: bool,
}

/// Папка установленных плагинов.
pub fn dir() -> Option<PathBuf> {
    crate::settings::dir().map(|d| d.join("plugins"))
}

/// Все пакеты из `plugins/` и `plugins/dev/`; ошибки — текстом с именем файла.
pub fn load_all() -> (Vec<Installed>, Vec<String>) {
    let (mut out, mut errors) = (Vec::new(), Vec::new());
    let Some(d) = dir() else { return (out, errors) };
    for (folder, dev) in [(d.clone(), false), (d.join("dev"), true)] {
        let Ok(rd) = std::fs::read_dir(&folder) else { continue };
        let mut paths: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "gsplugin")).collect();
        paths.sort();
        for path in paths {
            match paint_plugin::load(&path) {
                // из мастерской — поверх установленного с тем же id
                Ok(plugin) => {
                    out.retain(|i: &Installed| i.plugin.id != plugin.id);
                    out.push(Installed { plugin, path, dev });
                }
                Err(e) => errors.push(format!("{}: {e}", path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())),
            }
        }
    }
    (out, errors)
}

/// Установить пакет: проверить и положить в `plugins/<id>.gsplugin`.
pub fn install(src: &Path) -> Result<paint_plugin::Plugin, String> {
    let plugin = paint_plugin::load(src).map_err(|e| e.to_string())?;
    let d = dir().ok_or("нет папки настроек")?; // не перевод
    std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    let dest = d.join(format!("{}.gsplugin", plugin.id));
    if dest != src {
        std::fs::copy(src, &dest).map_err(|e| e.to_string())?;
    }
    Ok(plugin)
}

/// Холст для плагина: текущий слой документа (и новые слои, которые плагин создаст). Прежние
/// пиксели тронутых слоёв запоминаются — после запуска они уходят в историю одной записью на слой.
pub struct DocCanvas<'a> {
    pub doc: &'a mut Document,
    pub path: Vec<usize>,
    pub color: [u8; 3],
    /// Тронутые слои и их пиксели до плагина.
    pub touched: Vec<(Vec<usize>, ColorMap)>,
    /// Новые слои — для перерисовки и выбора после запуска.
    pub created: usize,
}

impl DocCanvas<'_> {
    fn layer(&self) -> Option<&Layer> {
        self.doc.layer(&self.path).filter(|l| l.children.is_none())
    }

    fn touch(&mut self) {
        if !self.touched.iter().any(|(p, _)| *p == self.path) {
            let orig = self.layer().map(|l| l.pixels.clone()).unwrap_or_default();
            self.touched.push((self.path.clone(), orig));
        }
    }
}

impl paint_plugin::Canvas for DocCanvas<'_> {
    fn size(&self) -> (u32, u32) {
        (self.doc.width, self.doc.height)
    }

    fn get(&self, x: i32, y: i32) -> [u8; 4] {
        if x < 0 || y < 0 || x >= self.doc.width as i32 || y >= self.doc.height as i32 {
            return [0; 4];
        }
        self.layer().map_or([0; 4], |l| to_rgba8(l.pixels.get(x, y)))
    }

    fn set(&mut self, x: i32, y: i32, c: [u8; 4]) {
        if x < 0 || y < 0 || x >= self.doc.width as i32 || y >= self.doc.height as i32 || self.layer().is_none() {
            return;
        }
        self.touch();
        let path = self.path.clone();
        if let Some(l) = self.doc.layer_mut(&path) {
            l.pixels.set(x, y, from_rgba8(c));
        }
    }

    fn selection(&self) -> Option<[i32; 4]> {
        self.doc.selection.bounds().map(|(a, b, c, d)| [a, b, c, d])
    }

    fn selected(&self, x: i32, y: i32) -> f32 {
        self.doc.selection.amount(x, y) as f32 / paint_core::pixel::ONE as f32
    }

    fn bounds(&self) -> Option<[i32; 4]> {
        self.layer().and_then(|l| paint_core::selection::content_bounds(&l.pixels)).map(|(a, b, c, d)| [a, b, c, d])
    }

    fn color(&self) -> [u8; 3] {
        self.color
    }

    fn set_color(&mut self, c: [u8; 3]) {
        self.color = c;
    }

    fn new_layer(&mut self, name: &str) -> bool {
        if self.created >= 64 {
            return false;
        }
        let mut path = self.path.clone();
        let Some(last) = path.last_mut() else { return false };
        *last += 1;
        let name = if name.trim().is_empty() { crate::i18n::tr("Слой плагина") } else { name };
        self.doc.insert_layer(&path, Layer::new(name));
        self.path = path;
        self.touched.push((self.path.clone(), ColorMap::new()));
        self.created += 1;
        true
    }
}
