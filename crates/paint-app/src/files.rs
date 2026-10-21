//! Системные окна «Открыть» и «Сохранить»: в Windows — проводник, в Linux — окно рабочего стола
//! через xdg-desktop-portal (KDE, GNOME и другие). Окно ждёт ответа в своём потоке, программа
//! тем временем рисует кадры; ответ забирается в `poll`.
//!
//! Если системного окна нет (в Linux не запущен portal) — ответ приходит сразу пустым, тогда
//! показывается встроенное окно с путём. `PAINT_NATIVE_DIALOGS=off` — всегда встроенное (так
//! устроены проверки на скрытом экране).

use crate::i18n::tr;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

/// Что спрашиваем.
#[derive(Clone, Debug, PartialEq)]
pub enum Ask {
    Open,
    SaveAs,
    /// Экспорт: расширение `png`, `jpg` или `psd`.
    Export(&'static str),
    /// Опорная картинка поверх холста.
    Reference,
    /// Картинка — отпечаток кисти.
    Tip,
    /// Сохранить инструмент сетки `n` в файл кисти `.gsbrush`.
    SaveBrush(usize),
    /// Сохранить все инструменты сетки набором `.gsbrushes`.
    SaveBrushSet,
}

pub struct Pending {
    pub ask: Ask,
    rx: Receiver<Option<PathBuf>>,
    started: Instant,
}

/// Ответ системного окна.
pub enum Answer {
    Path(PathBuf),
    Cancelled,
    /// Системного окна нет — нужно встроенное.
    Unavailable,
}

pub fn native() -> bool {
    std::env::var("PAINT_NATIVE_DIALOGS").map_or(true, |v| v != "off")
}

/// Открыть системное окно; `suggested` — папка и имя по умолчанию.
pub fn ask(ask: Ask, suggested: &Path, ctx: &eframe::egui::Context) -> Pending {
    let (tx, rx) = channel();
    let dir = suggested.parent().map(Path::to_path_buf).filter(|d| d.is_dir());
    let name = suggested.file_name().map(|n| n.to_string_lossy().into_owned());
    let (title, filters): (&str, Vec<(String, Vec<&'static str>)>) = match &ask {
        Ask::Open => (
            tr("Открыть"),
            vec![
                (tr("Рисунки").to_string(), vec!["sai2", "psd", "psb", "ora", "png", "jpg", "jpeg", "bmp", "tga", "tif", "tiff", "webp", "qoi"]),
                (tr("Палитры (GIMP, Krita)").to_string(), vec!["gpl", "kpl"]),
                (tr("Кисти GrazySAI").to_string(), vec!["gsbrush", "gsbrushes"]),
                (tr("Плагины GrazySAI").to_string(), vec!["gsplugin"]),
                (tr("Кисти Clip Studio Paint").to_string(), vec!["sut", "bfps"]),
                (tr("Кисти Krita и GIMP").to_string(), vec!["kpp", "bundle", "gbr", "gih", "abr"]),
                (tr("Все файлы").to_string(), vec!["*"]),
            ],
        ),
        Ask::Tip => (
            tr("Отпечаток кисти"),
            vec![(tr("Картинки и кисти GIMP").to_string(), vec!["png", "jpg", "jpeg", "bmp", "tga", "tif", "tiff", "webp", "qoi", "gbr", "gih", "abr"])],
        ),
        Ask::Reference => (
            tr("Опорная картинка"),
            vec![(tr("Картинки").to_string(), vec!["png", "jpg", "jpeg", "bmp", "tga", "tif", "tiff", "webp", "qoi"])],
        ),
        Ask::SaveBrush(_) => (tr("Сохранить кисть"), vec![(tr("Кисть GrazySAI").to_string(), vec!["gsbrush"])]),
        Ask::SaveBrushSet => (tr("Сохранить набор кистей"), vec![(tr("Набор кистей GrazySAI").to_string(), vec!["gsbrushes"])]),
        Ask::SaveAs => (
            tr("Сохранить как"),
            vec![("SAI 2 (.sai2)".to_string(), vec!["sai2"]), ("OpenRaster (.ora)".to_string(), vec!["ora"]), ("Photoshop (.psd)".to_string(), vec!["psd"]), ("Photoshop PSB (.psb)".to_string(), vec!["psb"])],
        ),
        Ask::Export(ext) => (
            tr("Экспорт"),
            vec![(
                match *ext {
                    "jpg" => "JPEG (.jpg)".to_string(),
                    "psd" => "Photoshop (.psd)".to_string(),
                    "psb" => "Photoshop (.psb)".to_string(),
                    "ora" => "OpenRaster (.ora)".to_string(),
                    e => format!("{} (.{e})", e.to_ascii_uppercase()),
                },
                vec![*ext],
            )],
        ),
    };
    let kind = ask.clone();
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let mut d = rfd::FileDialog::new().set_title(title);
        for (label, exts) in &filters {
            d = d.add_filter(label, exts);
        }
        if let Some(dir) = dir {
            d = d.set_directory(dir);
        }
        let picked = match kind {
            Ask::Open | Ask::Reference | Ask::Tip => d.pick_file(),
            _ => {
                if let Some(n) = name {
                    d = d.set_file_name(n);
                }
                d.save_file()
            }
        };
        let _ = tx.send(picked);
        ctx.request_repaint();
    });
    Pending { ask, rx, started: Instant::now() }
}

impl Pending {
    /// Ответ, если он уже есть.
    pub fn poll(&self) -> Option<Answer> {
        let picked = self.rx.try_recv().ok()?;
        Some(match picked {
            Some(p) => Answer::Path(p),
            // мгновенный пустой ответ — окна не было (portal не отвечает)
            None if cfg!(target_os = "linux") && self.started.elapsed() < Duration::from_millis(250) => Answer::Unavailable,
            None => Answer::Cancelled,
        })
    }
}

/// Путь с нужным расширением (системные окна Linux его не всегда добавляют).
pub fn with_ext(path: PathBuf, ext: &str) -> PathBuf {
    let has = path.extension().is_some_and(|e| {
        let e = e.to_string_lossy().to_lowercase();
        e == ext || (ext == "jpg" && e == "jpeg")
    });
    if has {
        path
    } else {
        let mut s = path.into_os_string();
        s.push(format!(".{ext}"));
        PathBuf::from(s)
    }
}
