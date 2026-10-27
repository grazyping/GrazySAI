//! GrazySAI-Convert — перевод рисунков и моделей 3D из формата в формат, пачкой.
//! Окно: перетащить файлы, выбрать форматы, «Преобразовать». Без окна:
//! `GrazySAI-Convert --to psd [--out ПАПКА] ФАЙЛЫ…` (и `--to glb` / `--to obj` для моделей).

#![cfg_attr(windows, windows_subsystem = "windows")]

#[allow(dead_code, reason = "модуль общий с GrazySAI, конвертер берёт из него часть")]
#[path = "../../paint-app/src/i18n.rs"]
mod i18n;

use eframe::egui;
use i18n::{tr, trf};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};

/// Во что переводятся рисунки (со слоями — первые четыре) и модели.
const PICTURE_OUT: [&str; 11] = ["sai2", "psd", "psb", "ora", "png", "jpg", "webp", "tiff", "bmp", "tga", "qoi"];
const MODEL_OUT: [&str; 2] = ["glb", "obj"];

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    Picture,
    Model,
}

fn ext(p: &Path) -> String {
    p.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase()
}

fn kind(p: &Path) -> Option<Kind> {
    if paint_formats::convert::can_read(p) {
        Some(Kind::Picture)
    } else if paint_3d::MODEL_EXTS.contains(&ext(p).as_str()) {
        Some(Kind::Model)
    } else {
        None
    }
}

/// Куда писать: в папку `out` или рядом с исходным, с новым расширением; исходный файл не
/// затирается никогда — при совпадении имени к нему добавляется «-converted».
fn out_path(src: &Path, out: Option<&Path>, to: &str) -> PathBuf {
    let stem = src.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "file".into());
    let dir = out.map(Path::to_path_buf).or_else(|| src.parent().map(Path::to_path_buf)).unwrap_or_default();
    let p = dir.join(format!("{stem}.{to}"));
    if same_file(&p, src) {
        dir.join(format!("{stem}-converted.{to}"))
    } else {
        p
    }
}

/// В одной пачке `figure.fbx` и `figure.obj` дали бы один `figure.glb`: второму — «-2», «-3»…
fn unique(dst: PathBuf, used: &mut Vec<PathBuf>) -> PathBuf {
    let mut p = dst.clone();
    let mut n = 2;
    while used.contains(&p) {
        let stem = dst.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        p = dst.with_file_name(format!("{stem}-{n}.{}", ext(&dst)));
        n += 1;
    }
    used.push(p.clone());
    p
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}

/// Записать через временный файл рядом (то же расширение — запись выбирается по нему), потом
/// подменить: сбой не оставляет половину файла.
fn via_temp(dst: &Path, write: impl FnOnce(&Path) -> Result<(), String>) -> Result<(), String> {
    let name = dst.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = dst.with_file_name(format!(".part-{name}"));
    let r = write(&tmp).and_then(|_| paint_core::io::replace_with(&tmp, dst).map_err(|e| e.to_string()));
    if r.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    r
}

/// Один файл: заметки о приближённом или ошибка. Сбой внутри чтения чужого файла не роняет
/// всю пачку.
fn convert(src: &Path, dst: &Path) -> Result<Vec<String>, String> {
    if same_file(src, dst) {
        return Err(tr("файл не может заменить сам себя").into());
    }
    let to = ext(dst);
    let job = || -> Result<Vec<String>, String> {
        match kind(src) {
            Some(Kind::Picture) => {
                if !PICTURE_OUT.contains(&to.as_str()) {
                    return Err(trf("рисунок нельзя записать в .{0}", &[&to]));
                }
                let (doc, notes) = paint_formats::convert::load(src).map_err(|e| e.to_string())?;
                via_temp(dst, |t| paint_formats::convert::save(&doc, t, paint_formats::psd::SaveOptions { exact: true, ..Default::default() }).map_err(|e| e.to_string()))?;
                Ok(notes)
            }
            Some(Kind::Model) => {
                let files = paint_3d::gather(src).map_err(|e| e.to_string())?;
                let model = paint_3d::model::load(&files).map_err(|e| e.to_string())?;
                let mut notes = vec![tr("модель — в позе покоя: анимации не переносятся").to_string()];
                match to.as_str() {
                    "glb" => {
                        let bytes = paint_3d::export::glb(&model).map_err(|e| e.to_string())?;
                        paint_core::io::write_atomic(dst, &bytes).map_err(|e| e.to_string())?;
                    }
                    "obj" => {
                        let stem = dst.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                        let (obj, mtl, tex) = paint_3d::export::obj(&model, &stem).map_err(|e| e.to_string())?;
                        for (name, bytes) in &tex {
                            paint_core::io::write_atomic(&dst.with_file_name(name), bytes).map_err(|e| e.to_string())?;
                        }
                        paint_core::io::write_atomic(&dst.with_extension("mtl"), mtl.as_bytes()).map_err(|e| e.to_string())?;
                        paint_core::io::write_atomic(dst, obj.as_bytes()).map_err(|e| e.to_string())?;
                        if !model.skins.is_empty() || model.meshes.iter().any(|m| !m.morph_names.is_empty()) {
                            notes.push(tr("в OBJ нет скелета и морфов — записана только сетка").into());
                        }
                    }
                    _ => return Err(trf("модель нельзя записать в .{0}", &[&to])),
                }
                Ok(notes)
            }
            None => Err(trf("формат .{0} не читается", &[&ext(src)])),
        }
    };
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(job)).unwrap_or_else(|_| Err(tr("сбой при разборе файла").into()))
}

/// Без окна: `--to ФОРМАТ [--out ПАПКА] ФАЙЛЫ…`. Код выхода 1, если что-то не вышло.
fn cli(args: &[String]) -> i32 {
    let (mut to, mut out, mut files) = (None, None, Vec::new());
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--to" => to = it.next().map(|s| s.trim_start_matches('.').to_ascii_lowercase()),
            "--out" => out = it.next().map(PathBuf::from),
            _ => files.push(PathBuf::from(a)),
        }
    }
    let Some(to) = to else {
        eprintln!("{}", tr("нужно --to ФОРМАТ"));
        return 2;
    };
    let mut failed = false;
    let mut used = Vec::new();
    for src in &files {
        let dst = unique(out_path(src, out.as_deref(), &to), &mut used);
        match convert(src, &dst) {
            Ok(notes) => {
                println!("{} → {}", src.display(), dst.display());
                for n in notes {
                    println!("  {n}");
                }
            }
            Err(e) => {
                failed = true;
                eprintln!("{}: {e}", src.display());
            }
        }
    }
    i32::from(failed)
}

struct Item {
    path: PathBuf,
    kind: Kind,
    /// Итог: `None` — ещё не делали.
    done: Option<Result<String, String>>,
}

struct App {
    items: Vec<Item>,
    picture_to: usize,
    model_to: usize,
    out: Option<PathBuf>,
    work: Option<Receiver<(usize, Result<String, String>)>>,
    picked: Option<Receiver<Vec<PathBuf>>>,
    skipped: Vec<PathBuf>,
}

impl App {
    fn add(&mut self, paths: impl IntoIterator<Item = PathBuf>) {
        for p in paths {
            if self.items.iter().any(|i| i.path == p) {
                continue;
            }
            match kind(&p) {
                Some(k) => self.items.push(Item { path: p, kind: k, done: None }),
                None => self.skipped.push(p),
            }
        }
    }

    fn start(&mut self) {
        let (tx, rx) = channel();
        let mut used = Vec::new();
        let jobs: Vec<(usize, PathBuf, PathBuf)> = self
            .items
            .iter()
            .enumerate()
            .map(|(i, it)| {
                let to = if it.kind == Kind::Picture { PICTURE_OUT[self.picture_to] } else { MODEL_OUT[self.model_to] };
                (i, it.path.clone(), unique(out_path(&it.path, self.out.as_deref(), to), &mut used))
            })
            .collect();
        for it in &mut self.items {
            it.done = None;
        }
        std::thread::spawn(move || {
            for (i, src, dst) in jobs {
                let r = convert(&src, &dst).map(|notes| {
                    // в списке — имя готового файла; папка видна в строке «Куда»
                    let mut s = dst.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    for n in notes {
                        s.push_str("; ");
                        s.push_str(&n);
                    }
                    s
                });
                if tx.send((i, r)).is_err() {
                    break;
                }
            }
        });
        self.work = Some(rx);
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
        self.add(dropped);
        if let Some(rx) = &self.picked {
            if let Ok(v) = rx.try_recv() {
                self.picked = None;
                self.add(v);
            }
        }
        let mut busy = false;
        if let Some(rx) = &self.work {
            busy = true;
            while let Ok((i, r)) = rx.try_recv() {
                if let Some(it) = self.items.get_mut(i) {
                    it.done = Some(r);
                }
            }
            if self.items.iter().all(|i| i.done.is_some()) {
                self.work = None;
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        egui::TopBottomPanel::top("opts").show(ctx, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.add_enabled(self.picked.is_none(), egui::Button::new(tr("Добавить файлы…"))).clicked() {
                    let (tx, rx) = channel();
                    std::thread::spawn(move || {
                        let _ = tx.send(rfd::FileDialog::new().pick_files().unwrap_or_default());
                    });
                    self.picked = Some(rx);
                }
                if ui.add_enabled(!busy, egui::Button::new(tr("Очистить список"))).clicked() {
                    self.items.clear();
                    self.skipped.clear();
                }
            });
            ui.horizontal(|ui| {
                ui.label(tr("Рисунки в"));
                egui::ComboBox::from_id_salt("pic").selected_text(PICTURE_OUT[self.picture_to]).show_index(ui, &mut self.picture_to, PICTURE_OUT.len(), |i| PICTURE_OUT[i].to_string());
                ui.label(tr("Модели 3D в"));
                egui::ComboBox::from_id_salt("model").selected_text(MODEL_OUT[self.model_to]).show_index(ui, &mut self.model_to, MODEL_OUT.len(), |i| MODEL_OUT[i].to_string());
            });
            ui.horizontal(|ui| {
                ui.label(tr("Куда:"));
                match &self.out {
                    Some(d) => ui.label(d.display().to_string()),
                    None => ui.label(tr("рядом с исходными")),
                };
                if ui.button(tr("Выбрать папку…")).clicked() {
                    if let Some(d) = rfd::FileDialog::new().pick_folder() {
                        self.out = Some(d);
                    }
                }
                if self.out.is_some() && ui.button(tr("Рядом с исходными")).clicked() {
                    self.out = None;
                }
            });
            ui.horizontal(|ui| {
                let go = egui::Button::new(if busy { tr("Идёт…") } else { tr("Преобразовать") });
                if ui.add_enabled(!busy && !self.items.is_empty(), go).clicked() {
                    self.start();
                }
                ui.weak(tr("Со слоями: SAI 2, PSD, PSB, OpenRaster; остальные картинки — сведённым рисунком."));
            });
            ui.add_space(4.0);
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.items.is_empty() {
                ui.centered_and_justified(|ui| ui.label(tr("Перетащите сюда рисунки или модели 3D")));
                return;
            }
            egui::ScrollArea::vertical().show(ui, |ui| {
                for it in &self.items {
                    ui.horizontal(|ui| {
                        let (mark, color) = match &it.done {
                            None => ("·", ui.visuals().weak_text_color()),
                            Some(Ok(_)) => ("✔", egui::Color32::from_rgb(60, 170, 80)),
                            Some(Err(_)) => ("✖", egui::Color32::from_rgb(210, 60, 60)),
                        };
                        ui.colored_label(color, mark);
                        let name = it.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                        ui.label(name).on_hover_text(it.path.display().to_string());
                        match &it.done {
                            Some(Ok(s)) => {
                                ui.weak(format!("→ {s}"));
                            }
                            Some(Err(e)) => {
                                ui.colored_label(color, e);
                            }
                            None => {}
                        }
                    });
                }
                for p in &self.skipped {
                    ui.weak(trf("{0} — этот формат не читается", &[&p.display()]));
                }
            });
        });
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let lang = std::env::var("PAINT_LANG").ok().unwrap_or_else(|| i18n::system().to_string());
    i18n::set(&lang);
    if args.iter().any(|a| a == "--to") {
        std::process::exit(cli(&args));
    }
    let mut app = App { items: Vec::new(), picture_to: 1, model_to: 0, out: None, work: None, picked: None, skipped: Vec::new() };
    app.add(args.into_iter().map(PathBuf::from));
    let options = eframe::NativeOptions { viewport: egui::ViewportBuilder::default().with_title("GrazySAI-Convert").with_inner_size([760.0, 460.0]), ..Default::default() };
    let _ = eframe::run_native(
        "GrazySAI-Convert",
        options,
        Box::new(|cc| {
            i18n::install_fonts(&cc.egui_ctx);
            Ok(Box::new(app))
        }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_overwrites_the_source() {
        let dir = std::env::temp_dir().join(format!("gs-convert-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("a.png");
        paint_core::io::save_png(&src, 2, 2, &[255; 16]).unwrap();
        let dst = out_path(&src, None, "png");
        assert_eq!(dst, dir.join("a-converted.png"));
        let mut used = Vec::new();
        assert_eq!(unique(dir.join("b.glb"), &mut used), dir.join("b.glb"));
        assert_eq!(unique(dir.join("b.glb"), &mut used), dir.join("b-2.glb"));
        assert!(convert(&src, &src).is_err());
        convert(&src, &out_path(&src, None, "psd")).unwrap();
        let (doc, _) = paint_formats::convert::load(&dir.join("a.psd")).unwrap();
        assert_eq!((doc.width, doc.height), (2, 2));
        assert!(convert(&dir.join("a.psd"), &dir.join("a.glb")).is_err(), "рисунок — не модель");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
