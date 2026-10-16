//! Свой обозреватель файлов, как FileViewer в SAI («Файл → Открыть/Сохранить через
//! обозреватель», «Всегда использовать обозреватель файлов»): путь цепочкой папок, слева
//! «Проводник» (дерево папок) и «Закладки», справа миниатюры рисунков; кнопки «вверх»,
//! «обновить», «новая папка», «переименовать», «удалить», «в закладки»; вид, порядок, тип файлов.
//!
//! Миниатюры считаются в фоне (`paint_formats::preview`) и кэшируются на диске в папке
//! настроек (`thumbnails/`), как у SAI. Удаление в Linux — в корзину рабочего стола
//! (freedesktop.org Trash), в Windows — навсегда, после вопроса.

use eframe::egui::{self, Color32, Pos2, Rect, Vec2};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::SystemTime;

use crate::i18n::{tr, trf};
use crate::sai_ui;

#[derive(Clone, Copy, PartialEq)]
pub enum Mode {
    Open,
    Save,
}

pub enum Outcome {
    Open(PathBuf),
    Save(PathBuf),
    Cancel,
}

struct Item {
    path: PathBuf,
    name: String,
    dir: bool,
    size: u64,
    modified: SystemTime,
}

enum Thumb {
    Loading,
    Ready(egui::TextureHandle),
    Failed,
}

/// Сторона миниатюры, точек.
const THUMB: u32 = 112;
const CELL: Vec2 = Vec2::new(132.0, 150.0);

/// Форматы сохранения: расширение и подпись.
const SAVE_FORMATS: [(&str, &str); 3] = [("sai2", "SAI 2 (.sai2)"), ("ora", "OpenRaster (.ora)"), ("psd", "Photoshop (.psd)")];

pub struct FileViewer {
    pub mode: Mode,
    pub dir: PathBuf,
    pub bookmarks: Vec<PathBuf>,
    pub view: u8,
    pub sort: u8,
    items: Vec<Item>,
    selected: Option<usize>,
    left_tab: u8,
    expanded: HashSet<PathBuf>,
    subdirs: HashMap<PathBuf, Vec<PathBuf>>,
    scroll_tree: bool,
    all_files: bool,
    thumbs: HashMap<PathBuf, Thumb>,
    queue: Arc<Mutex<VecDeque<(PathBuf, u64, u64)>>>,
    rx: mpsc::Receiver<(PathBuf, Option<egui::ColorImage>)>,
    alive: Arc<AtomicBool>,
    name: String,
    save_fmt: usize,
    status: String,
    rename: Option<(PathBuf, String)>,
    new_folder: Option<String>,
    delete_ask: Option<PathBuf>,
    overwrite_ask: Option<PathBuf>,
}

impl Drop for FileViewer {
    fn drop(&mut self) {
        self.alive.store(false, Ordering::Relaxed);
    }
}

/// Папка кэша миниатюр (`None` — настройки выключены: без кэша на диске).
fn cache_dir() -> Option<PathBuf> {
    crate::settings::dir().map(|d| d.join("thumbnails"))
}

fn cache_file(path: &Path, mtime: u64, size: u64) -> Option<PathBuf> {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (path, mtime, size).hash(&mut h);
    cache_dir().map(|d| d.join(format!("{:016x}.png", h.finish())))
}

fn load_thumb(path: &Path, mtime: u64, size: u64) -> Option<egui::ColorImage> {
    let cached = cache_file(path, mtime, size);
    let img = cached
        .as_ref()
        .and_then(|c| paint_core::io::load_png(c).ok())
        .or_else(|| {
            let t = paint_formats::preview::thumbnail(path, THUMB)?;
            if let Some(c) = &cached {
                let _ = std::fs::create_dir_all(c.parent()?);
                let _ = paint_core::io::save_png(c, t.0, t.1, &t.2);
            }
            Some(t)
        })?;
    Some(egui::ColorImage::from_rgba_unmultiplied([img.0 as usize, img.1 as usize], &img.2))
}

impl FileViewer {
    pub fn new(mode: Mode, dir: PathBuf, name: String, bookmarks: Vec<PathBuf>) -> Self {
        let queue: Arc<Mutex<VecDeque<(PathBuf, u64, u64)>>> = Arc::default();
        let alive = Arc::new(AtomicBool::new(true));
        let (tx, rx) = mpsc::channel();
        // миниатюры считаются в фоне, в несколько потоков
        let workers = paint_core::threads().clamp(2, 6);
        for _ in 0..workers {
            let (q, a, tx) = (queue.clone(), alive.clone(), tx.clone());
            std::thread::spawn(move || {
                while a.load(Ordering::Relaxed) {
                    let job = q.lock().unwrap().pop_front();
                    match job {
                        Some((p, mtime, size)) => {
                            // битый файл не должен останавливать поток: у него просто нет миниатюры
                            let img = std::panic::catch_unwind(|| load_thumb(&p, mtime, size)).ok().flatten();
                            if tx.send((p, img)).is_err() {
                                break;
                            }
                        }
                        None => std::thread::sleep(std::time::Duration::from_millis(30)),
                    }
                }
            });
        }
        let dir = if dir.is_dir() { dir } else { home() };
        let save_fmt = SAVE_FORMATS.iter().position(|(e, _)| name.to_lowercase().ends_with(&format!(".{e}"))).unwrap_or(0);
        let mut v = Self {
            mode,
            dir: dir.clone(),
            bookmarks,
            view: 0,
            sort: 0,
            items: Vec::new(),
            selected: None,
            left_tab: 0,
            expanded: HashSet::new(),
            subdirs: HashMap::new(),
            scroll_tree: true,
            all_files: false,
            thumbs: HashMap::new(),
            queue,
            rx,
            alive,
            name,
            save_fmt,
            status: String::new(),
            rename: None,
            new_folder: None,
            delete_ask: None,
            overwrite_ask: None,
        };
        v.go(dir);
        v
    }

    /// Перейти в папку: перечитать, раскрыть дерево до неё.
    fn go(&mut self, dir: PathBuf) {
        self.dir = dir.clone();
        self.selected = None;
        self.status.clear();
        let mut p = Some(dir.as_path());
        while let Some(d) = p {
            self.expanded.insert(d.to_path_buf());
            p = d.parent();
        }
        self.scroll_tree = true;
        self.reload();
    }

    fn reload(&mut self) {
        self.items.clear();
        self.queue.lock().unwrap().clear();
        let Ok(rd) = std::fs::read_dir(&self.dir) else {
            self.status = trf("Папка недоступна: {0}", &[&self.dir.display()]);
            return;
        };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let Ok(meta) = e.metadata() else { continue };
            let path = e.path();
            let dir = meta.is_dir();
            if !dir && !self.all_files && !paint_formats::preview::supported(&path) {
                continue;
            }
            self.items.push(Item { path, name, dir, size: meta.len(), modified: meta.modified().unwrap_or(SystemTime::UNIX_EPOCH) });
        }
        self.sort_items();
        self.subdirs.remove(&self.dir);
    }

    fn sort_items(&mut self) {
        let sort = self.sort;
        self.items.sort_by(|a, b| {
            b.dir.cmp(&a.dir).then_with(|| match sort {
                1 => b.modified.cmp(&a.modified),
                2 => b.size.cmp(&a.size),
                _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            })
        });
    }

    fn subdirs_of(&mut self, p: &Path) -> Vec<PathBuf> {
        if let Some(v) = self.subdirs.get(p) {
            return v.clone();
        }
        let mut v: Vec<PathBuf> = std::fs::read_dir(p)
            .map(|rd| rd.flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_dir()) && !e.file_name().to_string_lossy().starts_with('.')).map(|e| e.path()).collect())
            .unwrap_or_default();
        v.sort_by_key(|a| a.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default());
        self.subdirs.insert(p.to_path_buf(), v.clone());
        v
    }

    fn thumb(&mut self, ctx: &egui::Context, item: usize) -> Option<egui::TextureId> {
        let it = &self.items[item];
        match self.thumbs.get(&it.path) {
            Some(Thumb::Ready(t)) => Some(t.id()),
            Some(_) => None,
            None => {
                let mtime = it.modified.duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_secs());
                self.queue.lock().unwrap().push_back((it.path.clone(), mtime, it.size));
                self.thumbs.insert(it.path.clone(), Thumb::Loading);
                let _ = ctx;
                None
            }
        }
    }

    fn take_ready(&mut self, ctx: &egui::Context) {
        while let Ok((p, img)) = self.rx.try_recv() {
            let t = match img {
                Some(img) => Thumb::Ready(ctx.load_texture(format!("fv-{}", p.display()), img, egui::TextureOptions::LINEAR)),
                None => Thumb::Failed,
            };
            self.thumbs.insert(p, t);
        }
    }

    /// Путь для сохранения из поля имени и выбранного формата.
    fn save_path(&self) -> Option<PathBuf> {
        let name = self.name.trim();
        if name.is_empty() {
            return None;
        }
        let ext = SAVE_FORMATS[self.save_fmt].0;
        let has = SAVE_FORMATS.iter().any(|(e, _)| name.to_lowercase().ends_with(&format!(".{e}")));
        Some(self.dir.join(if has { name.to_string() } else { format!("{name}.{ext}") }))
    }

    /// Показ окна; `Some` — выбор сделан или окно закрыто.
    pub fn show(&mut self, ctx: &egui::Context) -> Option<Outcome> {
        self.take_ready(ctx);
        if self.thumbs.values().any(|t| matches!(t, Thumb::Loading)) {
            ctx.request_repaint_after(std::time::Duration::from_millis(60));
        }
        let mut out = None;
        let mut open = true;
        let title = if self.mode == Mode::Open { tr("Открыть") } else { tr("Сохранить") };
        let screen = ctx.screen_rect();
        egui::Window::new(title)
            .id(egui::Id::new("file_viewer"))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size(screen.size() * 0.8)
            .default_pos(screen.center() - screen.size() * 0.4)
            .show(ctx, |ui| out = self.contents(ui));
        if !open {
            out = Some(Outcome::Cancel);
        }
        self.dialogs(ctx);
        out
    }

    fn contents(&mut self, ui: &mut egui::Ui) -> Option<Outcome> {
        let mut out = None;
        // путь цепочкой папок
        ui.horizontal_wrapped(|ui| {
            ui.label(tr("Путь:"));
            let mut acc = PathBuf::new();
            let mut go = None;
            for c in self.dir.components() {
                acc.push(c.as_os_str());
                let label = match c {
                    std::path::Component::RootDir => "/".to_string(),
                    other => other.as_os_str().to_string_lossy().into_owned(),
                };
                if label.is_empty() {
                    continue;
                }
                if ui.add(egui::Button::new(label).small().frame(false)).clicked() {
                    go = Some(acc.clone());
                }
                if !matches!(c, std::path::Component::RootDir | std::path::Component::Prefix(_)) {
                    ui.label("/");
                }
            }
            if let Some(d) = go {
                self.go(d);
            }
        });
        ui.separator();
        let bottom_h = 64.0;
        let h = (ui.available_height() - bottom_h).max(120.0);
        ui.horizontal(|ui| {
            // слева: проводник и закладки
            ui.allocate_ui(Vec2::new(250.0, h), |ui| {
                ui.set_min_size(Vec2::new(250.0, h));
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut self.left_tab, 0, tr("Проводник"));
                        ui.selectable_value(&mut self.left_tab, 1, tr("Закладки"));
                    });
                    egui::Frame::none().fill(Color32::WHITE).stroke(egui::Stroke::new(1.0_f32, sai_ui::BORDER)).show(ui, |ui| {
                        egui::ScrollArea::both().id_salt("fv_tree").auto_shrink([false, false]).max_height(h - 30.0).show(ui, |ui| {
                            ui.set_min_width(240.0);
                            if self.left_tab == 0 {
                                for r in roots() {
                                    self.tree_node(ui, &r, 0);
                                }
                                self.scroll_tree = false;
                            } else {
                                self.bookmark_list(ui);
                            }
                        });
                    });
                });
            });
            ui.separator();
            ui.vertical(|ui| {
                out = self.toolbar_and_grid(ui, h);
            });
        });
        ui.separator();
        // низ: тип файлов, имя и формат (сохранение), состояние, OK / Отмена
        ui.horizontal(|ui| {
            ui.label(tr("Тип:"));
            let label = |all: bool| if all { tr("Все файлы").to_string() } else { tr("Рисунки (*.sai2, *.psd, *.ora, *.png, *.jpg, *.bmp, *.tga, *.tif, *.webp, *.qoi)").to_string() };
            let before = self.all_files;
            egui::ComboBox::from_id_salt("fv_type").selected_text(label(self.all_files)).width(420.0).show_ui(ui, |ui| {
                ui.selectable_value(&mut self.all_files, false, label(false));
                ui.selectable_value(&mut self.all_files, true, label(true));
            });
            if before != self.all_files {
                self.reload();
            }
            if self.mode == Mode::Save {
                ui.label(tr("Имя:"));
                ui.add(egui::TextEdit::singleline(&mut self.name).desired_width(220.0));
                egui::ComboBox::from_id_salt("fv_fmt").selected_text(SAVE_FORMATS[self.save_fmt].1).show_ui(ui, |ui| {
                    for (i, (_, l)) in SAVE_FORMATS.iter().enumerate() {
                        ui.selectable_value(&mut self.save_fmt, i, *l);
                    }
                });
            }
        });
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(&self.status).color(Color32::from_rgb(170, 40, 30)));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(tr("Отмена")).clicked() {
                    out = Some(Outcome::Cancel);
                }
                if ui.button("OK").clicked() {
                    out = self.ok();
                }
            });
        });
        let typing = ui.ctx().wants_keyboard_input();
        let (enter, esc, back) = ui.input(|i| (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape), i.key_pressed(egui::Key::Backspace)));
        if esc && self.rename.is_none() && self.new_folder.is_none() {
            out = Some(Outcome::Cancel);
        }
        if enter && (self.mode == Mode::Save || !typing) && self.rename.is_none() && self.new_folder.is_none() {
            out = out.or_else(|| self.ok());
        }
        if back && !typing {
            if let Some(p) = self.dir.parent().map(Path::to_path_buf) {
                self.go(p);
            }
        }
        out
    }

    /// OK: открыть выбранный файл (папку — войти) или сохранить под именем из поля.
    fn ok(&mut self) -> Option<Outcome> {
        match self.mode {
            Mode::Open => {
                let it = self.items.get(self.selected?)?;
                if it.dir {
                    let p = it.path.clone();
                    self.go(p);
                    None
                } else {
                    Some(Outcome::Open(it.path.clone()))
                }
            }
            Mode::Save => {
                let p = self.save_path()?;
                if p.exists() {
                    self.overwrite_ask = Some(p);
                    None
                } else {
                    Some(Outcome::Save(p))
                }
            }
        }
    }

    fn tree_node(&mut self, ui: &mut egui::Ui, p: &Path, depth: usize) {
        let open = self.expanded.contains(p);
        let current = self.dir == p;
        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.display().to_string());
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width().max(200.0), 20.0), egui::Sense::click());
        let painter = ui.painter();
        if current {
            painter.rect_filled(rect, 0.0, sai_ui::SELECT_BG);
            painter.rect_stroke(rect, 0.0, egui::Stroke::new(1.0_f32, sai_ui::SELECT_EDGE));
            if self.scroll_tree {
                ui.scroll_to_rect(rect, Some(egui::Align::Center));
            }
        }
        let x0 = rect.left() + 4.0 + depth as f32 * 12.0;
        let arrow = Rect::from_center_size(Pos2::new(x0 + 5.0, rect.center().y), Vec2::splat(12.0));
        // треугольник раскрытия (символов ▸▾ нет во всех шрифтах — рисуется сам)
        let (c, k) = (arrow.center(), 3.5);
        let tri = if open {
            vec![c + Vec2::new(-k, -k * 0.5), c + Vec2::new(k, -k * 0.5), c + Vec2::new(0.0, k * 0.7)]
        } else {
            vec![c + Vec2::new(-k * 0.5, -k), c + Vec2::new(k * 0.7, 0.0), c + Vec2::new(-k * 0.5, k)]
        };
        painter.add(egui::Shape::convex_polygon(tri, if open { sai_ui::INK } else { sai_ui::DIM }, egui::Stroke::NONE));
        let icon = Rect::from_min_size(Pos2::new(x0 + 13.0, rect.top() + 3.0), Vec2::new(16.0, 14.0));
        sai_ui::glyph(painter, icon, Color32::from_rgb(70, 110, 180), "new_folder");
        painter.text(Pos2::new(icon.right() + 5.0, rect.center().y), egui::Align2::LEFT_CENTER, &name, egui::FontId::proportional(13.0), sai_ui::INK);
        if resp.clicked() {
            let on_arrow = resp.interact_pointer_pos().is_some_and(|q| q.x < arrow.right() + 2.0);
            if on_arrow {
                if open {
                    self.expanded.remove(p);
                } else {
                    self.expanded.insert(p.to_path_buf());
                }
            } else {
                self.expanded.insert(p.to_path_buf());
                self.go(p.to_path_buf());
            }
        }
        resp.context_menu(|ui| {
            if ui.button(tr("В закладки")).clicked() {
                if !self.bookmarks.iter().any(|b| b == p) {
                    self.bookmarks.push(p.to_path_buf());
                }
                ui.close_menu();
            }
        });
        if self.expanded.contains(p) {
            for c in self.subdirs_of(p) {
                self.tree_node(ui, &c, depth + 1);
            }
        }
    }

    fn bookmark_list(&mut self, ui: &mut egui::Ui) {
        let mut go = None;
        let mut remove = None;
        let mut swap = None;
        if self.bookmarks.is_empty() {
            ui.weak(tr("Закладок нет: звёздочка над миниатюрами добавляет текущую папку"));
        }
        for (i, b) in self.bookmarks.iter().enumerate() {
            let name = b.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| b.display().to_string());
            let r = ui.selectable_label(self.dir == *b, format!("★ {name}")).on_hover_text(b.display().to_string());
            if r.clicked() {
                go = Some(b.clone());
            }
            r.context_menu(|ui| {
                if i > 0 && ui.button(tr("Выше")).clicked() {
                    swap = Some((i, i - 1));
                    ui.close_menu();
                }
                if i + 1 < self.bookmarks.len() && ui.button(tr("Ниже")).clicked() {
                    swap = Some((i, i + 1));
                    ui.close_menu();
                }
                if ui.button(tr("Убрать из закладок")).clicked() {
                    remove = Some(i);
                    ui.close_menu();
                }
            });
        }
        if let Some((a, b)) = swap {
            self.bookmarks.swap(a, b);
        }
        if let Some(i) = remove {
            self.bookmarks.remove(i);
        }
        if let Some(d) = go {
            self.go(d);
        }
    }

    fn toolbar_and_grid(&mut self, ui: &mut egui::Ui, h: f32) -> Option<Outcome> {
        let mut out = None;
        let b = Vec2::splat(22.0);
        ui.horizontal(|ui| {
            if sai_ui::icon_button(ui, b, false, self.dir.parent().is_some(), tr("Вверх (Backspace)"), |p, r, c| sai_ui::glyph(p, r, c, "up")).clicked() {
                if let Some(p) = self.dir.parent().map(Path::to_path_buf) {
                    self.go(p);
                }
            }
            if sai_ui::icon_button(ui, b, false, true, tr("Обновить"), |p, r, c| sai_ui::glyph(p, r, c, "cw")).clicked() {
                self.thumbs.clear();
                self.reload();
            }
            if sai_ui::icon_button(ui, b, false, true, tr("Новая папка"), |p, r, c| sai_ui::glyph(p, r, c, "new_folder")).clicked() {
                self.new_folder = Some(tr("Новая папка").to_string());
            }
            let sel = self.selected.and_then(|i| self.items.get(i)).map(|it| it.path.clone());
            if sai_ui::icon_button(ui, b, false, sel.is_some(), tr("Переименовать"), |p, r, c| sai_ui::glyph(p, r, c, "pen")).clicked() {
                if let Some(p) = &sel {
                    self.rename = Some((p.clone(), p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()));
                }
            }
            if sai_ui::icon_button(ui, b, false, sel.is_some(), tr("Удалить"), |p, r, c| sai_ui::glyph(p, r, c, "delete")).clicked() {
                self.delete_ask = sel.clone();
            }
            let marked = self.bookmarks.contains(&self.dir);
            if sai_ui::icon_button(ui, b, marked, true, tr("Текущую папку — в закладки"), |p, r, c| {
                p.text(r.center(), egui::Align2::CENTER_CENTER, "★", egui::FontId::proportional(15.0), c);
            })
            .clicked()
            {
                if marked {
                    self.bookmarks.retain(|x| *x != self.dir);
                } else {
                    self.bookmarks.push(self.dir.clone());
                }
            }
            ui.add_space(8.0);
            ui.label(tr("Вид"));
            egui::ComboBox::from_id_salt("fv_view").selected_text(if self.view == 0 { tr("Миниатюры") } else { tr("Список") }).show_ui(ui, |ui| {
                ui.selectable_value(&mut self.view, 0, tr("Миниатюры"));
                ui.selectable_value(&mut self.view, 1, tr("Список"));
            });
            ui.label(tr("Порядок"));
            let names = [tr("Имя"), tr("Дата"), tr("Размер")];
            let before = self.sort;
            egui::ComboBox::from_id_salt("fv_sort").selected_text(names[self.sort as usize % 3]).show_ui(ui, |ui| {
                for (i, n) in names.iter().enumerate() {
                    ui.selectable_value(&mut self.sort, i as u8, *n);
                }
            });
            if before != self.sort {
                self.selected = None;
                self.sort_items();
            }
        });
        egui::Frame::none().fill(Color32::WHITE).stroke(egui::Stroke::new(1.0_f32, sai_ui::BORDER)).show(ui, |ui| {
            egui::ScrollArea::vertical().id_salt("fv_grid").auto_shrink([false, false]).max_height(h - 34.0).show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                if self.view == 0 {
                    let per_row = ((ui.available_width() / (CELL.x + 4.0)).floor() as usize).max(1);
                    let n = self.items.len();
                    for row in 0..n.div_ceil(per_row) {
                        ui.horizontal(|ui| {
                            for i in row * per_row..((row + 1) * per_row).min(n) {
                                if let Some(o) = self.cell(ui, i) {
                                    out = Some(o);
                                }
                            }
                        });
                    }
                } else {
                    for i in 0..self.items.len() {
                        if let Some(o) = self.list_row(ui, i) {
                            out = Some(o);
                        }
                    }
                }
                if self.items.is_empty() {
                    ui.weak(tr("Здесь нет рисунков"));
                }
            });
        });
        out
    }

    /// Нажатия по элементу: выбор, двойной щелчок, меню.
    fn item_input(&mut self, resp: &egui::Response, i: usize) -> Option<Outcome> {
        let mut out = None;
        if resp.clicked() || resp.secondary_clicked() {
            self.selected = Some(i);
            if self.mode == Mode::Save && !self.items[i].dir {
                self.name = self.items[i].name.clone();
                if let Some(f) = SAVE_FORMATS.iter().position(|(e, _)| self.name.to_lowercase().ends_with(&format!(".{e}"))) {
                    self.save_fmt = f;
                }
            }
        }
        if resp.double_clicked() {
            self.selected = Some(i);
            out = self.ok();
        }
        let path = self.items[i].path.clone();
        let dir = self.items[i].dir;
        resp.context_menu(|ui| {
            if ui.button(if dir || self.mode == Mode::Open { tr("Открыть") } else { tr("Сохранить сюда") }).clicked() {
                self.selected = Some(i);
                out = self.ok();
                ui.close_menu();
            }
            if ui.button(tr("Переименовать…")).clicked() {
                self.rename = Some((path.clone(), path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()));
                ui.close_menu();
            }
            if ui.button(tr("Удалить…")).clicked() {
                self.delete_ask = Some(path.clone());
                ui.close_menu();
            }
            if dir && ui.button(tr("В закладки")).clicked() {
                if !self.bookmarks.contains(&path) {
                    self.bookmarks.push(path.clone());
                }
                ui.close_menu();
            }
        });
        out
    }

    fn cell(&mut self, ui: &mut egui::Ui, i: usize) -> Option<Outcome> {
        let (rect, resp) = ui.allocate_exact_size(CELL, egui::Sense::click());
        let sel = self.selected == Some(i);
        let tex = if self.items[i].dir { None } else { self.thumb(ui.ctx(), i) };
        let it = &self.items[i];
        let p = ui.painter();
        if sel {
            p.rect_filled(rect, 0.0, sai_ui::SELECT_BG);
            p.rect_stroke(rect, 0.0, egui::Stroke::new(1.0_f32, sai_ui::SELECT_EDGE));
        } else if resp.hovered() {
            p.rect_filled(rect, 0.0, Color32::from_gray(242));
        }
        let area = Rect::from_center_size(Pos2::new(rect.center().x, rect.top() + 8.0 + THUMB as f32 / 2.0), Vec2::splat(THUMB as f32));
        if it.dir {
            sai_ui::glyph(p, Rect::from_center_size(area.center(), Vec2::new(46.0, 38.0)), Color32::from_rgb(70, 110, 180), "new_folder");
        } else {
            // значок вида файла в углу, как у SAI
            // листок с загнутым углом
            let o = rect.min + Vec2::new(4.0, 4.0);
            let sheet = vec![o, o + Vec2::new(6.0, 0.0), o + Vec2::new(9.0, 3.0), o + Vec2::new(9.0, 12.0), o + Vec2::new(0.0, 12.0)];
            p.add(egui::Shape::closed_line(sheet, egui::Stroke::new(1.0_f32, sai_ui::DIM)));
            p.line_segment([o + Vec2::new(6.0, 0.0), o + Vec2::new(6.0, 3.0)], egui::Stroke::new(1.0_f32, sai_ui::DIM));
            p.line_segment([o + Vec2::new(6.0, 3.0), o + Vec2::new(9.0, 3.0)], egui::Stroke::new(1.0_f32, sai_ui::DIM));
            match (&tex, self.thumbs.get(&it.path)) {
                (Some(t), _) => {
                    let size = ui.ctx().tex_manager().read().meta(*t).map(|m| m.size).unwrap_or([THUMB as usize; 2]);
                    let k = (THUMB as f32 / size[0].max(size[1]) as f32).min(1.0);
                    let r = Rect::from_center_size(area.center(), Vec2::new(size[0] as f32 * k, size[1] as f32 * k));
                    sai_ui::checker(p, r, 6.0);
                    p.image(*t, r, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                    p.rect_stroke(r, 0.0, egui::Stroke::new(1.0_f32, Color32::from_gray(200)));
                }
                (None, Some(Thumb::Failed)) => {
                    p.text(area.center(), egui::Align2::CENTER_CENTER, "ERROR", egui::FontId::proportional(12.0), Color32::from_rgb(220, 30, 60));
                }
                _ => {
                    p.text(area.center(), egui::Align2::CENTER_CENTER, "…", egui::FontId::proportional(16.0), sai_ui::DIM);
                }
            }
        }
        let galley = ui.painter().layout(it.name.clone(), egui::FontId::proportional(12.0), sai_ui::INK, CELL.x - 8.0);
        let text_pos = Pos2::new(rect.center().x - galley.size().x / 2.0, area.bottom() + 6.0);
        ui.painter().with_clip_rect(rect).galley(text_pos, galley, sai_ui::INK);
        let resp = resp.on_hover_text(it.name.clone());
        self.item_input(&resp, i)
    }

    fn list_row(&mut self, ui: &mut egui::Ui, i: usize) -> Option<Outcome> {
        let it = &self.items[i];
        let when = it.modified.duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let text = if it.dir { format!("📁 {}", it.name) } else { format!("{}    {}    {}", it.name, human_size(it.size), date(when)) };
        let resp = ui.selectable_label(self.selected == Some(i), text);
        self.item_input(&resp, i)
    }

    /// Окна вопросов: новая папка, переименование, удаление, замена файла.
    fn dialogs(&mut self, ctx: &egui::Context) {
        if let Some(mut name) = self.new_folder.take() {
            let mut keep = true;
            egui::Window::new(tr("Новая папка")).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
                let r = ui.text_edit_singleline(&mut name);
                r.request_focus();
                ui.horizontal(|ui| {
                    if ui.button("OK").clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        match std::fs::create_dir(self.dir.join(name.trim())) {
                            Ok(()) => self.reload(),
                            Err(e) => self.status = e.to_string(),
                        }
                        keep = false;
                    }
                    if ui.button(tr("Отмена")).clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        keep = false;
                    }
                });
            });
            if keep {
                self.new_folder = Some(name);
            }
        }
        if let Some((path, mut name)) = self.rename.take() {
            let mut keep = true;
            egui::Window::new(tr("Переименовать")).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
                ui.text_edit_singleline(&mut name).request_focus();
                ui.horizontal(|ui| {
                    if ui.button("OK").clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        let to = path.with_file_name(name.trim());
                        if to.exists() {
                            self.status = trf("Уже есть: {0}", &[&to.display()]);
                        } else if let Err(e) = std::fs::rename(&path, &to) {
                            self.status = e.to_string();
                        } else {
                            self.thumbs.remove(&path);
                            self.reload();
                        }
                        keep = false;
                    }
                    if ui.button(tr("Отмена")).clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        keep = false;
                    }
                });
            });
            if keep {
                self.rename = Some((path, name));
            }
        }
        if let Some(path) = self.delete_ask.clone() {
            egui::Window::new(tr("Удалить")).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
                let msg = if cfg!(target_os = "linux") { tr("Переместить в корзину:") } else { tr("Удалить навсегда:") };
                ui.label(msg);
                ui.label(path.display().to_string());
                ui.horizontal(|ui| {
                    if ui.button(tr("Удалить")).clicked() {
                        match delete(&path) {
                            Ok(()) => {
                                self.thumbs.remove(&path);
                                self.reload();
                            }
                            Err(e) => self.status = e,
                        }
                        self.delete_ask = None;
                    }
                    if ui.button(tr("Отмена")).clicked() {
                        self.delete_ask = None;
                    }
                });
            });
        }
        if self.overwrite_ask.is_some() {
            // ответ «да» отдаётся через `confirmed_overwrite`
        }
    }

    /// Окно «Заменить файл?» при сохранении; `Some` — ответ «да».
    pub fn overwrite_window(&mut self, ctx: &egui::Context) -> Option<Outcome> {
        let path = self.overwrite_ask.clone()?;
        let mut out = None;
        egui::Window::new(tr("Сохранить")).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
            ui.label(trf("Файл уже есть: {0}. Заменить?", &[&path.display()]));
            ui.horizontal(|ui| {
                if ui.button(tr("Заменить")).clicked() {
                    out = Some(Outcome::Save(path.clone()));
                    self.overwrite_ask = None;
                }
                if ui.button(tr("Отмена")).clicked() {
                    self.overwrite_ask = None;
                }
            });
        });
        out
    }
}

/// Корни дерева: в Windows — диски, иначе `/` и домашняя папка.
fn roots() -> Vec<PathBuf> {
    if cfg!(windows) {
        (b'A'..=b'Z').map(|c| PathBuf::from(format!("{}:\\", c as char))).filter(|p| p.exists()).collect()
    } else {
        vec![PathBuf::from("/")]
    }
}

pub fn home() -> PathBuf {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

fn human_size(n: u64) -> String {
    match n {
        0..=1023 => format!("{n} B"),
        1024..=1_048_575 => format!("{:.1} KB", n as f64 / 1024.0),
        _ => format!("{:.1} MB", n as f64 / 1_048_576.0),
    }
}

/// Дата UTC: ГГГГ-ММ-ДД ЧЧ:ММ:СС (без сторонних библиотек).
fn date(secs: u64) -> String {
    let days = (secs / 86400) as i64;
    let (y, m, d) = civil(days);
    let s = secs % 86400;
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
}

/// Дни от 1970-01-01 → год, месяц, день (алгоритм Хиннанта).
fn civil(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + (m <= 2) as i64, m, d)
}

/// Удалить: в Linux — в корзину рабочего стола (freedesktop.org Trash), иначе — насовсем.
fn delete(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        let base = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".local/share")).join("Trash");
        let (files, info) = (base.join("files"), base.join("info"));
        std::fs::create_dir_all(&files).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(&info).map_err(|e| e.to_string())?;
        let name = path.file_name().ok_or("нет имени")?.to_string_lossy().into_owned();
        let mut target = name.clone();
        let mut n = 1;
        while files.join(&target).exists() || info.join(format!("{target}.trashinfo")).exists() {
            n += 1;
            target = format!("{name}.{n}");
        }
        let abs = std::fs::canonicalize(path).map_err(|e| e.to_string())?;
        let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let when = date(now).replace(' ', "T");
        let enc: String = abs.to_string_lossy().bytes().map(|b| if b.is_ascii_alphanumeric() || b"/-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect();
        std::fs::write(info.join(format!("{target}.trashinfo")), format!("[Trash Info]\nPath={enc}\nDeletionDate={when}\n")).map_err(|e| e.to_string())?;
        if let Err(e) = std::fs::rename(path, files.join(&target)) {
            let _ = std::fs::remove_file(info.join(format!("{target}.trashinfo")));
            return Err(trf("Не удалось переместить в корзину: {0}", &[&e]));
        }
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let r = if path.is_dir() { std::fs::remove_dir_all(path) } else { std::fs::remove_file(path) };
        r.map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// В Linux удаление — в корзину по freedesktop.org: файл в `Trash/files`, рядом `.trashinfo`.
    #[cfg(target_os = "linux")]
    #[test]
    fn deleting_moves_to_the_trash() {
        let d = std::env::temp_dir().join(format!("grazysai-trash-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::env::set_var("XDG_DATA_HOME", d.join("data"));
        let f = d.join("рисунок 1.png");
        std::fs::write(&f, b"x").unwrap();
        delete(&f).unwrap();
        assert!(!f.exists());
        let trash = d.join("data/Trash");
        assert!(trash.join("files/рисунок 1.png").exists());
        let info = std::fs::read_to_string(trash.join("info/рисунок 1.png.trashinfo")).unwrap();
        assert!(info.starts_with("[Trash Info]\nPath=/") && info.contains("%20"), "{info}");
        std::fs::remove_dir_all(d).ok();
    }

    #[test]
    fn dates_are_civil() {
        assert_eq!(date(0), "1970-01-01 00:00:00");
        assert_eq!(date(1_760_000_000), "2025-10-09 08:53:20");
    }
}
