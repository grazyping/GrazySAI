//! Окно редактора: холст, панели по образцу, рисование на выбранном слое, открытие `.sai2`.

#![cfg_attr(windows, windows_subsystem = "windows")]

mod pen;
mod sai_ui;
mod virtual_pen;

use eframe::egui::{self, Color32, Pos2, Rect, Vec2};
use paint_core::tile::{Key, TILE};
use paint_core::mix::MixParams;
use paint_core::{
    BlendMode, BrushParams, Document, InputPoint, Layer, LiveStroke, Stabilizer, Stroke,
};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Плитка показа: 256×256 точек, то есть 4×4 плитки ядра. Одна текстура на плитку.
const DT: i32 = 256;

/// Режимы в том же порядке, что в списке образца (docs/ref/sai2-2017-blend-modes.png).
const MODES: [(BlendMode, &str); 28] = [
    (BlendMode::Normal, "Обычный"),
    (BlendMode::Multiply, "Умножение"),
    (BlendMode::Screen, "Экран"),
    (BlendMode::Overlay, "Перекрытие"),
    (BlendMode::Shade, "Тень"),
    (BlendMode::Shine, "Сияние"),
    (BlendMode::ShadeShine, "Тень/Сияние"),
    (BlendMode::Burn, "Затемнение"),
    (BlendMode::Dodge, "Осветление"),
    (BlendMode::BurnDodge, "Затемнение/Осветление"),
    (BlendMode::Darken, "Темнее"),
    (BlendMode::ColorBurn, "Затемнение основы"),
    (BlendMode::LinearBurn, "Линейный затемнитель"),
    (BlendMode::DarkerColor, "Темнее цвет"),
    (BlendMode::Lighten, "Светлее"),
    (BlendMode::ColorDodge, "Осветление основы"),
    (BlendMode::LinearDodge, "Линейный осветлитель"),
    (BlendMode::LighterColor, "Светлее цвет"),
    (BlendMode::SoftLight, "Мягкий свет"),
    (BlendMode::HardLight, "Жёсткий свет"),
    (BlendMode::VividLight, "Яркий свет"),
    (BlendMode::LinearLight, "Линейный свет"),
    (BlendMode::PinLight, "Точечный свет"),
    (BlendMode::HardMix, "Жёсткое смешение"),
    (BlendMode::Difference, "Разница"),
    (BlendMode::Exclusion, "Исключение"),
    (BlendMode::Subtract, "Вычитание"),
    (BlendMode::Divide, "Деление"),
];

const SIZES: [f32; 35] = [
    0.6, 0.8, 1.0, 1.3, 1.6, 2.0, 2.5, 3.0, 3.5, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 12.0, 14.0,
    16.0, 20.0, 25.0, 30.0, 35.0, 40.0, 50.0, 60.0, 70.0, 80.0, 100.0, 120.0, 160.0, 200.0, 250.0,
    300.0, 350.0,
];

#[derive(Clone, Copy, PartialEq)]
enum Tool {
    /// Кисть из сетки инструментов слоя (`PaintApp::brush`).
    Brush,
    Hand,
    Picker,
    Zoom,
}

/// Инструмент рисования в сетке «кистей слоя», как у образца: имя, значок, свои параметры.
struct BrushTool {
    name: &'static str,
    icon: &'static str,
    params: BrushParams,
    /// Пока не сделан — показан неактивным.
    enabled: bool,
}

fn brush_tools() -> Vec<BrushTool> {
    let b = |size: f32, hardness: f32, density: f32| BrushParams {
        size,
        hardness,
        density,
        ..Default::default()
    };
    vec![
        BrushTool {
            name: "Каранд.",
            icon: "pen",
            params: b(3.0, 1.0, 1.0),
            enabled: true,
        },
        BrushTool {
            name: "Аэрогр.",
            icon: "airbrush",
            params: BrushParams {
                size_by_pressure: false,
                density_by_pressure: true,
                ..b(50.0, 0.0, 0.3)
            },
            enabled: true,
        },
        BrushTool {
            name: "Кисть",
            icon: "brush",
            params: BrushParams { mix: Some(MixParams::new(0.0, 0.0, 0.5)), ..b(15.0, 0.75, 1.0) },
            enabled: true,
        },
        BrushTool {
            name: "Акварель",
            icon: "water",
            params: BrushParams { mix: Some(MixParams::new(0.3, 0.3, 0.5)), ..b(30.0, 0.5, 0.8) },
            enabled: true,
        },
        BrushTool {
            name: "Маркер",
            icon: "marker",
            // у Маркера в SAI нет порога размытия
            params: BrushParams {
                mix: Some(MixParams { blur_pressure: 0.0, ..MixParams::new(0.0, 0.0, 0.5) }),
                ..b(6.0, 0.9, 1.0)
            },
            enabled: true,
        },
        BrushTool {
            name: "Ластик",
            icon: "eraser",
            params: BrushParams {
                erase: true,
                size_by_pressure: false,
                ..b(30.0, 0.75, 1.0)
            },
            enabled: true,
        },
        BrushTool {
            name: "Выд.К.",
            icon: "selpen",
            params: b(10.0, 1.0, 1.0),
            enabled: false,
        },
        BrushTool {
            name: "Выд.Л.",
            icon: "selers",
            params: b(10.0, 1.0, 1.0),
            enabled: false,
        },
    ]
}

#[derive(Clone, Copy, PartialEq)]
enum LayerAction {
    Duplicate,
    Delete,
    Clear,
    Merge,
    Transfer,
    Up,
    Down,
}

/// Номер ластика в сетке — им рисует обратный конец пера.
const ERASER: usize = 5;

/// Масштабы в списке быстрой панели, %.
const ZOOMS: [f32; 14] = [
    6.25, 12.5, 25.0, 33.3, 50.0, 66.7, 100.0, 150.0, 200.0, 300.0, 400.0, 800.0, 1600.0, 3200.0,
];

struct DisplayTile {
    pixels: Vec<u8>,
    texture: Option<egui::TextureHandle>,
    stale: bool,
}

struct PaintApp {
    doc: Document,
    file: Option<PathBuf>,
    open_path: String,
    status: String,
    notes: Vec<String>,
    zoom: f32,
    /// Точка холста, которая показана в левом верхнем углу области просмотра.
    offset: Vec2,
    fit_pending: bool,
    tool: Tool,
    brushes: Vec<BrushTool>,
    brush: usize,
    /// Основной цвет в HSV (тон хранится и у серых), фоновый — RGB.
    hsv: [f32; 3],
    back: Color32,
    stabilizer: paint_core::stabilizer::Level,
    selected: Vec<usize>,
    live: Option<LiveStroke>,
    smoother: Stabilizer,
    display: HashMap<(i32, i32), DisplayTile>,
    queue: VecDeque<Key>,
    queued: HashSet<Key>,
    pen: pen::PenFeed,
    /// Сила последнего касания пера (Windows Ink); `None` — в этом сеансе пера ещё не было.
    touch_force: Option<f32>,
    touch_down: bool,
    /// Текущий мазок начат пером (касанием): до его конца нажим берётся от пера, даже когда
    /// касание уже кончилось — иначе последняя точка при отрыве шла бы с нажимом мыши 1,0.
    stroke_by_touch: bool,
    /// Последний использованный нажим — для строки состояния.
    last_pressure: f32,
    last_source: &'static str,
    /// Область холста на экране в прошлом кадре — для навигатора.
    view: Rect,
    show_open: bool,
    show_new: bool,
    new_size: [u32; 2],
    show_about: bool,
    /// Есть несохранённые изменения.
    dirty: bool,
    /// В открытом файле были слои контура или фигур (их кривые при сохранении станут растром).
    had_vectors: bool,
    show_save_as: bool,
    save_path: String,
    /// Миниатюры слоёв по пути в дереве; сбрасываются после правок.
    thumbs: HashMap<Vec<usize>, egui::TextureHandle>,
    /// Виртуальное перо для проверок без планшета (`PAINT_VIRTUAL_PEN`).
    virtual_pen: Option<virtual_pen::VirtualPen>,
    /// Кадров после конца сценария до сохранения результата (`PAINT_VIRTUAL_PEN_OUT`).
    virtual_done: u32,
    /// Сводка по нажиму от пера за сеанс: (источник, мин., макс., число замеров) — для проверок.
    pen_stats: HashMap<&'static str, (f32, f32, u32)>,
    started: Instant,
}

impl PaintApp {
    fn new(path: Option<PathBuf>, pen: pen::PenFeed) -> Self {
        let mut app = Self {
            doc: Document::new(1200, 900),
            file: None,
            open_path: path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            status: String::from("Новый холст 1200×900"),
            notes: Vec::new(),
            zoom: 1.0,
            offset: Vec2::ZERO,
            fit_pending: true,
            tool: Tool::Brush,
            brushes: brush_tools(),
            // PAINT_BRUSH — номер кисти в сетке при запуске (для проверок виртуальным пером)
            brush: std::env::var("PAINT_BRUSH").ok().and_then(|v| v.parse().ok()).filter(|&i: &usize| i < ERASER).unwrap_or(0),
            hsv: [0.0, 0.0, 0.08],
            back: Color32::WHITE,
            stabilizer: paint_core::stabilizer::Level::Plain(3),
            selected: vec![0],
            live: None,
            smoother: Stabilizer::new(3),
            display: HashMap::new(),
            queue: VecDeque::new(),
            queued: HashSet::new(),
            pen,
            touch_force: None,
            touch_down: false,
            stroke_by_touch: false,
            last_pressure: 1.0,
            last_source: "мышь",
            view: Rect::NOTHING,
            show_open: false,
            show_new: false,
            new_size: [1200, 900],
            show_about: false,
            dirty: false,
            had_vectors: false,
            show_save_as: false,
            save_path: String::new(),
            thumbs: HashMap::new(),
            virtual_pen: virtual_pen::VirtualPen::from_env(),
            virtual_done: 0,
            pen_stats: HashMap::new(),
            started: Instant::now(),
        };
        match path {
            Some(p) => app.open(p),
            None => app.invalidate_all(),
        }
        app
    }

    fn color(&self) -> Color32 {
        sai_ui::hsv_to_rgb(self.hsv[0], self.hsv[1], self.hsv[2])
    }

    fn file_name(&self) -> String {
        self.file.as_ref().and_then(|f| f.file_name()).map_or_else(
            || "НовыйХолст1.sai2".to_string(),
            |n| n.to_string_lossy().into_owned(),
        )
    }

    fn open(&mut self, path: PathBuf) {
        let started = Instant::now();
        match paint_formats::sai2_doc::load(&path) {
            Ok(loaded) => {
                self.had_vectors = !loaded.vector_layers.is_empty();
                self.doc = loaded.document;
                self.notes = loaded.warnings;
                self.status = format!(
                    "Открыт {} — {}×{}, слоёв {}, за {:.1} с",
                    path.display(),
                    self.doc.width,
                    self.doc.height,
                    loaded.info.layers.len(),
                    started.elapsed().as_secs_f32()
                );
                self.file = Some(path);
                self.selected = first_raster(&self.doc.layers).unwrap_or_default();
                self.live = None;
                self.fit_pending = true;
                self.invalidate_all();
            }
            Err(e) => self.status = format!("Не удалось открыть {}: {e}", path.display()),
        }
    }

    fn new_canvas(&mut self, w: u32, h: u32) {
        self.doc = Document::new(w, h);
        self.had_vectors = false;
        self.file = None;
        self.notes.clear();
        self.selected = vec![0];
        self.live = None;
        self.fit_pending = true;
        self.status = format!("Новый холст {w}×{h}");
        self.invalidate_all();
    }

/// Сохранить в `.sai2`: в тот же файл или, если его нет или в нём были кривые, — спросить путь.
    fn save(&mut self) {
        match self.file.clone() {
            Some(f) if !self.had_vectors && f.extension().is_some_and(|e| e.eq_ignore_ascii_case("sai2")) => self.save_to(f),
            _ => self.ask_save_as(),
        }
    }

    fn ask_save_as(&mut self) {
        let base = self.file.clone().unwrap_or_else(|| std::env::current_dir().unwrap_or_default().join("НовыйХолст1.sai2"));
        let suggested = if self.had_vectors {
            let stem = base.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            base.with_file_name(format!("{stem}-paint-rs.sai2"))
        } else {
            base.with_extension("sai2")
        };
        self.save_path = suggested.display().to_string();
        self.show_save_as = true;
    }

    fn save_to(&mut self, path: PathBuf) {
        let started = Instant::now();
        match paint_formats::sai2_write::save(&self.doc, &path) {
            Ok(()) => {
                self.status = format!("Сохранено: {} (за {:.1} с)", path.display(), started.elapsed().as_secs_f32());
                self.file = Some(path);
                self.dirty = false;
                self.had_vectors = false;
            }
            Err(e) => self.status = format!("Не удалось сохранить {}: {e}", path.display()),
        }
    }

    fn export_png(&mut self) {
        let path = match &self.file {
            Some(f) => f.with_extension("export.png"),
            None => std::env::current_dir()
                .unwrap_or_default()
                .join("paint-rs-export.png"),
        };
        let result =
            paint_core::io::save_png(&path, self.doc.width, self.doc.height, &self.doc.flatten());
        self.status = match result {
            Ok(()) => format!("Сохранено: {}", path.display()),
            Err(e) => format!("Не удалось сохранить {}: {e}", path.display()),
        };
    }

    fn invalidate_all(&mut self) {
        self.thumbs.clear();
        self.display.clear();
        self.queue.clear();
        self.queued.clear();
        let (tw, th) = (
            (self.doc.width as i32 + TILE - 1) / TILE,
            (self.doc.height as i32 + TILE - 1) / TILE,
        );
        for ty in 0..th {
            for tx in 0..tw {
                self.queue.push_back((tx, ty));
                self.queued.insert((tx, ty));
            }
        }
    }

    fn invalidate(&mut self, keys: impl IntoIterator<Item = Key>) {
        for key in keys {
            if self.queued.insert(key) {
                self.queue.push_front(key); // свежие правки показываем первыми
            }
        }
    }

    /// Пересчитывает часть ожидающих плиток, не дольше `budget`.
    fn render_pending(&mut self, budget: Duration) {
        let started = Instant::now();
        let (w, h) = (self.doc.width as i32, self.doc.height as i32);
        while let Some(key) = self.queue.pop_front() {
            self.queued.remove(&key);
            if key.0 < 0 || key.1 < 0 || key.0 * TILE >= w || key.1 * TILE >= h {
                continue;
            }
            let rgba = self.doc.render_tile(key);
            let dkey = (key.0 * TILE / DT, key.1 * TILE / DT);
            let tile = self.display.entry(dkey).or_insert_with(|| DisplayTile {
                pixels: vec![0u8; (DT * DT * 4) as usize],
                texture: None,
                stale: true,
            });
            let (ox, oy) = (key.0 * TILE - dkey.0 * DT, key.1 * TILE - dkey.1 * DT);
            for y in 0..TILE {
                let dst = (((oy + y) * DT + ox) * 4) as usize;
                let src = (y * TILE * 4) as usize;
                tile.pixels[dst..dst + (TILE * 4) as usize]
                    .copy_from_slice(&rgba[src..src + (TILE * 4) as usize]);
            }
            tile.stale = true;
            if started.elapsed() > budget {
                break;
            }
        }
    }

    fn can_paint(&self) -> bool {
        self.doc
            .layer(&self.selected)
            .is_some_and(|l| l.children.is_none() && l.visible)
    }

    /// Текущий нажим и признак «ластик на обратном конце пера».
    fn pressure(&mut self) -> (f32, bool) {
        let fresh = self.pen.lock().ok().and_then(|s| {
            s.at.filter(|t| t.elapsed() < Duration::from_millis(250))
                .map(|_| (s.pressure, s.eraser))
        });
        let (pressure, eraser, source) = match (fresh, self.touch_force) {
            (Some((p, e)), _) => (p, e, "перо (XInput 2)"),
            (None, Some(f)) if self.touch_down || (self.stroke_by_touch && self.live.is_some()) => (f, false, "перо (Windows Ink)"),
            _ => (1.0, false, "мышь"),
        };
        self.last_pressure = pressure;
        self.last_source = source;
        let e = self.pen_stats.entry(source).or_insert((f32::MAX, f32::MIN, 0));
        *e = (e.0.min(pressure), e.1.max(pressure), e.2 + 1);
        (pressure, eraser)
    }

    fn begin_stroke(&mut self, p: InputPoint, eraser_end: bool) {
        if !self.can_paint() {
            self.status = String::from(
                "Рисовать можно на видимом обычном слое — выберите его в списке справа",
            );
            return;
        }
        let params = if eraser_end {
            self.brushes[ERASER].params
        } else {
            self.brushes[self.brush].params
        };
        self.stroke_by_touch = self.touch_down;
        self.smoother = Stabilizer::with_level(self.stabilizer);
        let c = self.color();
        let mut live = LiveStroke::new(
            self.selected.clone(),
            Stroke::new(params),
            [c.r(), c.g(), c.b()],
        );
        live.stroke.add_point(self.smoother.push(p));
        let keys = self.doc.live_update(&mut live);
        self.invalidate(keys);
        self.live = Some(live);
    }

    fn continue_stroke(&mut self, p: InputPoint) {
        let Some(mut live) = self.live.take() else {
            return;
        };
        live.stroke.add_point(self.smoother.push(p));
        let keys = self.doc.live_update(&mut live);
        self.invalidate(keys);
        self.live = Some(live);
    }

    fn end_stroke(&mut self) {
        self.thumbs.clear();
        self.dirty = true;
        let Some(mut live) = self.live.take() else {
            return;
        };
        for p in self.smoother.finish() {
            live.stroke.add_point(p);
        }
        let keys = self.doc.live_finish(live);
        self.invalidate(keys);
    }

    fn undo(&mut self) {
        if let Some(keys) = self.doc.undo() {
            self.after_structure(keys);
        }
    }

    fn redo(&mut self) {
        if let Some(keys) = self.doc.redo() {
            self.after_structure(keys);
        }
    }

    fn pick_color(&mut self, x: f32, y: f32) {
        let key = ((x as i32).div_euclid(TILE), (y as i32).div_euclid(TILE));
        let rgba = self.doc.render_tile(key);
        let i = (((y as i32).rem_euclid(TILE) * TILE + (x as i32).rem_euclid(TILE)) * 4) as usize;
        self.hsv = sai_ui::rgb_to_hsv(Color32::from_rgb(rgba[i], rgba[i + 1], rgba[i + 2]));
    }

    fn add_layer(&mut self, folder: bool) {
        let mut path = self.selected.clone();
        let index = path.pop().map_or(0, |i| i + 1);
        let n = count_layers(&self.doc.layers) + 1;
        let new = if folder { Layer::folder(&format!("Папка{n}"), Vec::new()) } else { Layer::new(&format!("Слой{n}")) };
        path.push(index);
        let keys = self.doc.insert_layer(&path, new);
        self.selected = path;
        self.after_structure(keys);
    }

    /// Действия со слоем из кнопок под списком, меню «Слой» и клавиш образца.
    fn layer_action(&mut self, action: LayerAction) {
        let path = self.selected.clone();
        let result = match action {
            LayerAction::Duplicate => {
                let keys = self.doc.duplicate_layer(&path);
                let mut p = path.clone();
                *p.last_mut().unwrap_or(&mut 0) += 1;
                Some((p, keys))
            }
            LayerAction::Delete => {
                let keys = self.doc.delete_layer(&path);
                Some((path.clone(), keys))
            }
            LayerAction::Clear => Some((path.clone(), self.doc.clear_layer(&path))),
            LayerAction::Merge => self.doc.merge_down(&path),
            LayerAction::Transfer => self.doc.transfer_down(&path).map(|(_, k)| (path.clone(), k)),
            LayerAction::Up => self.doc.move_layer(&path, 1),
            LayerAction::Down => self.doc.move_layer(&path, -1),
        };
        match result {
            Some((p, keys)) => {
                self.selected = p;
                self.after_structure(keys);
            }
            None => self.status = String::from("С этим слоем так нельзя (нужен обычный слой и обычный слой под ним)"),
        }
    }

    /// После изменения стопки: проверить выбор, перерисовать, обновить миниатюры.
    fn after_structure(&mut self, keys: Vec<Key>) {
        self.dirty = true;
        if self.doc.layer(&self.selected).is_none() {
            let mut p = self.selected.clone();
            while let Some(last) = p.last_mut() {
                if *last > 0 {
                    *last -= 1;
                    if self.doc.layer(&p).is_some() {
                        break;
                    }
                } else {
                    p.pop();
                    if self.doc.layer(&p).is_some() {
                        break;
                    }
                }
            }
            self.selected = if self.doc.layer(&p).is_some() { p } else { first_raster(&self.doc.layers).unwrap_or_default() };
        }
        self.thumbs.clear();
        self.invalidate(keys);
    }

    /// Масштаб вокруг центра области просмотра.
    fn set_zoom(&mut self, zoom: f32) {
        let zoom = zoom.clamp(0.02, 32.0);
        if self.view.is_positive() {
            let c = self.view.size() / 2.0;
            let anchor = c / self.zoom + self.offset;
            self.offset = anchor - c / zoom;
        }
        self.zoom = zoom;
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        egui::menu::bar(ui, |ui| {
            ui.menu_button("Файл(F)", |ui| {
                if ui.button("Создать холст…").clicked() {
                    self.show_new = true;
                    ui.close_menu();
                }
                if ui.button("Открыть…").clicked() {
                    self.show_open = true;
                    ui.close_menu();
                }
                if ui.button("Сохранить                Ctrl+S").clicked() {
                    self.save();
                    ui.close_menu();
                }
                if ui.button("Сохранить как…      Shift+Ctrl+S").clicked() {
                    self.ask_save_as();
                    ui.close_menu();
                }
                if ui.button("Экспорт в PNG").clicked() {
                    self.export_png();
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Выход").clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button("Правка(E)", |ui| {
                if ui
                    .add_enabled(
                        self.doc.can_undo(),
                        egui::Button::new("Отменить        Ctrl+Z"),
                    )
                    .clicked()
                {
                    self.undo();
                    ui.close_menu();
                }
                if ui
                    .add_enabled(
                        self.doc.can_redo(),
                        egui::Button::new("Повторить       Ctrl+Y"),
                    )
                    .clicked()
                {
                    self.redo();
                    ui.close_menu();
                }
            });
            ui.menu_button("Холст(C)", |ui| {
                for item in [
                    "Изменить разрешение…",
                    "Изменить размер…",
                    "Отразить по горизонтали",
                    "Отразить по вертикали",
                    "Повернуть на 90° против часовой",
                    "Повернуть на 90° по часовой",
                ] {
                    ui.add_enabled(false, egui::Button::new(item));
                }
            });
            ui.menu_button("Слой(L)", |ui| {
                if ui.button("Новый слой").clicked() {
                    self.add_layer(false);
                    ui.close_menu();
                }
                ui.add_enabled(false, egui::Button::new("Новый слой контура"));
                if ui.button("Новая папка").clicked() {
                    self.add_layer(true);
                    ui.close_menu();
                }
                ui.separator();
                for (item, action) in [
                    ("Копировать слой", LayerAction::Duplicate),
                    ("Удалить слой", LayerAction::Delete),
                    ("Очистить слой           Delete", LayerAction::Clear),
                    ("Перенести на нижний     F", LayerAction::Transfer),
                    ("Слить с нижним          Ctrl+E", LayerAction::Merge),
                    ("Поднять слой", LayerAction::Up),
                    ("Опустить слой", LayerAction::Down),
                ] {
                    if ui.button(item).clicked() {
                        self.layer_action(action);
                        ui.close_menu();
                    }
                }
                ui.add_enabled(false, egui::Button::new("Свести всё"));
            });
            ui.menu_button("Выделение(S)", |ui| {
                for item in ["Выделить всё", "Снять выделение", "Инвертировать"]
                {
                    ui.add_enabled(false, egui::Button::new(item));
                }
            });
            ui.menu_button("Линейка(R)", |ui| {
                ui.add_enabled(false, egui::Button::new("Показать линейку"));
            });
            ui.menu_button("Фильтр(T)", |ui| {
                for item in [
                    "Тон и насыщенность…",
                    "Яркость и контраст…",
                    "Размытие по Гауссу…",
                ] {
                    ui.add_enabled(false, egui::Button::new(item));
                }
            });
            ui.menu_button("Вид(V)", |ui| {
                if ui.button("Увеличить        Ctrl++").clicked() {
                    self.set_zoom(self.zoom * 1.25);
                    ui.close_menu();
                }
                if ui.button("Уменьшить       Ctrl+−").clicked() {
                    self.set_zoom(self.zoom / 1.25);
                    ui.close_menu();
                }
                if ui.button("Масштаб 100%").clicked() {
                    self.set_zoom(1.0);
                    ui.close_menu();
                }
                if ui.button("Вписать в окно").clicked() {
                    self.fit_pending = true;
                    ui.close_menu();
                }
            });
            ui.menu_button("Окно(W)", |ui| {
                ui.add_enabled(false, egui::Button::new("Панели — позже"));
            });
            ui.menu_button("Помощь(H)", |ui| {
                if ui.button("О программе").clicked() {
                    self.show_about = true;
                    ui.close_menu();
                }
            });
        });
    }

    /// Быстрая панель над холстом: отмена, выделение, масштаб, угол, сглаживание.
    fn quick_bar(&mut self, ui: &mut egui::Ui) {
        let b = Vec2::splat(20.0);
        ui.horizontal(|ui| {
            if sai_ui::icon_button(
                ui,
                b,
                false,
                self.doc.can_undo(),
                "Отменить (Ctrl+Z)",
                |p, r, c| sai_ui::glyph(p, r, c, "undo"),
            )
            .clicked()
            {
                self.undo();
            }
            if sai_ui::icon_button(
                ui,
                b,
                false,
                self.doc.can_redo(),
                "Повторить (Ctrl+Y)",
                |p, r, c| sai_ui::glyph(p, r, c, "redo"),
            )
            .clicked()
            {
                self.redo();
            }
            ui.separator();
            sai_ui::icon_button(
                ui,
                b,
                false,
                false,
                "Снять выделение",
                |p, r, c| sai_ui::glyph(p, r, c, "deselect"),
            );
            sai_ui::icon_button(
                ui,
                b,
                false,
                false,
                "Инвертировать выделение",
                |p, r, c| sai_ui::glyph(p, r, c, "invert"),
            );
            sai_ui::icon_button(
                ui,
                b,
                false,
                false,
                "Показать выделение",
                |p, r, c| sai_ui::glyph(p, r, c, "eye"),
            );
            ui.add_enabled(false, egui::Label::new("Выделение"));
            ui.separator();
            let current = format!("{:.0}%", self.zoom * 100.0);
            let mut pick = None;
            egui::ComboBox::from_id_salt("zoom")
                .selected_text(current)
                .width(64.0)
                .show_ui(ui, |ui| {
                    for z in ZOOMS {
                        if ui.selectable_label(false, format!("{z}%")).clicked() {
                            pick = Some(z / 100.0);
                        }
                    }
                });
            if let Some(z) = pick {
                self.set_zoom(z);
            }
            if sai_ui::icon_button(ui, b, false, true, "Уменьшить", |p, r, c| {
                sai_ui::glyph(p, r, c, "minus")
            })
            .clicked()
            {
                self.set_zoom(self.zoom / 1.25);
            }
            if sai_ui::icon_button(ui, b, false, true, "Увеличить", |p, r, c| {
                sai_ui::glyph(p, r, c, "plus")
            })
            .clicked()
            {
                self.set_zoom(self.zoom * 1.25);
            }
            if sai_ui::icon_button(ui, b, false, true, "Масштаб 100%", |p, r, c| {
                sai_ui::glyph(p, r, c, "reset")
            })
            .clicked()
            {
                self.set_zoom(1.0);
            }
            ui.separator();
            ui.add_enabled_ui(false, |ui| {
                egui::ComboBox::from_id_salt("angle")
                    .selected_text("+0.0°")
                    .width(56.0)
                    .show_ui(ui, |_| {});
            });
            for (g, tip) in [
                ("ccw", "Повернуть вид против часовой"),
                ("cw", "Повернуть вид по часовой"),
                ("reset", "Сбросить поворот"),
            ] {
                sai_ui::icon_button(ui, b, false, false, tip, |p, r, c| {
                    sai_ui::glyph(p, r, c, g)
                });
            }
            sai_ui::icon_button(ui, b, false, false, "Отразить вид", |p, r, c| {
                sai_ui::glyph(p, r, c, "flip")
            });
            ui.separator();
            ui.label("Сглаживание");
            egui::ComboBox::from_id_salt("stab")
                .selected_text(self.stabilizer.label())
                .width(44.0)
                .height(420.0)
                .show_ui(ui, |ui| {
                    for v in paint_core::stabilizer::Level::all() {
                        ui.selectable_value(&mut self.stabilizer, v, v.label());
                    }
                });
            sai_ui::icon_button(ui, b, false, false, "Прямая линия", |p, r, c| {
                sai_ui::glyph(p, r, c, "line")
            });
        });
    }

    fn left_panel(&mut self, ui: &mut egui::Ui) {
        // Переключатели вида панели цвета (рабочий пока только круг).
        ui.horizontal(|ui| {
            sai_ui::icon_button(
                ui,
                Vec2::splat(18.0),
                true,
                true,
                "Цветовой круг",
                |p, r, _| {
                    p.circle_stroke(
                        r.center(),
                        r.width() / 2.0,
                        egui::Stroke::new(2.0_f32, Color32::from_rgb(200, 80, 160)),
                    );
                },
            );
            for tip in ["Ползунки RGB", "Ползунки HSV", "Смеситель", "Образцы цвета"]
            {
                sai_ui::icon_button(ui, Vec2::splat(18.0), false, false, tip, |p, r, c| {
                    for i in 0..3 {
                        let y = r.top() + 2.0 + i as f32 * 4.0;
                        p.line_segment(
                            [Pos2::new(r.left(), y), Pos2::new(r.right(), y)],
                            egui::Stroke::new(1.5_f32, c),
                        );
                    }
                });
            }
        });
        ui.vertical_centered(|ui| {
            sai_ui::color_wheel(ui, &mut self.hsv, 178.0);
        });
        ui.add_space(4.0);
        // Общие инструменты и два цвета.
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                let b = Vec2::splat(24.0);
                ui.horizontal(|ui| {
                    for (g, tip) in [
                        ("select", "Выделение"),
                        ("lasso", "Лассо"),
                        ("wand", "Волшебная палочка"),
                        ("bucket", "Заливка"),
                        ("text", "Текст"),
                    ] {
                        sai_ui::icon_button(ui, b, false, false, tip, |p, r, c| {
                            sai_ui::glyph(p, r, c, g)
                        });
                    }
                });
                ui.horizontal(|ui| {
                    sai_ui::icon_button(
                        ui,
                        b,
                        false,
                        false,
                        "Перемещение",
                        |p, r, c| sai_ui::glyph(p, r, c, "move"),
                    );
                    if sai_ui::icon_button(
                        ui,
                        b,
                        self.tool == Tool::Zoom,
                        true,
                        "Масштаб: щелчок — крупнее, Alt+щелчок — мельче",
                        |p, r, c| sai_ui::glyph(p, r, c, "zoom"),
                    )
                    .clicked()
                    {
                        self.tool = Tool::Zoom;
                    }
                    sai_ui::icon_button(
                        ui,
                        b,
                        false,
                        false,
                        "Поворот вида",
                        |p, r, c| sai_ui::glyph(p, r, c, "rotate"),
                    );
                    if sai_ui::icon_button(
                        ui,
                        b,
                        self.tool == Tool::Hand,
                        true,
                        "Рука (пробел)",
                        |p, r, c| sai_ui::glyph(p, r, c, "hand"),
                    )
                    .clicked()
                    {
                        self.tool = Tool::Hand;
                    }
                    if sai_ui::icon_button(
                        ui,
                        b,
                        self.tool == Tool::Picker,
                        true,
                        "Пипетка (Alt)",
                        |p, r, c| sai_ui::glyph(p, r, c, "picker"),
                    )
                    .clicked()
                    {
                        self.tool = Tool::Picker;
                    }
                });
            });
            // основной и фоновый цвет, обмен
            let (rect, response) =
                ui.allocate_exact_size(Vec2::new(52.0, 50.0), egui::Sense::click());
            let p = ui.painter();
            let back = Rect::from_min_size(rect.min + Vec2::new(18.0, 20.0), Vec2::splat(26.0));
            p.rect_filled(back, 0.0, self.back);
            p.rect_stroke(back, 0.0, egui::Stroke::new(1.0_f32, sai_ui::BORDER));
            let fore = Rect::from_min_size(rect.min + Vec2::new(2.0, 2.0), Vec2::splat(26.0));
            p.rect_filled(fore, 0.0, self.color());
            p.rect_stroke(fore, 0.0, egui::Stroke::new(1.0_f32, sai_ui::BORDER));
            let swap = Rect::from_min_size(rect.min + Vec2::new(34.0, 2.0), Vec2::splat(14.0));
            sai_ui::glyph(p, swap, sai_ui::INK, "cw");
            sai_ui::checker(
                p,
                Rect::from_min_size(rect.min + Vec2::new(2.0, 34.0), Vec2::splat(12.0)),
                3.0,
            );
            if response.clicked()
                && response
                    .interact_pointer_pos()
                    .is_some_and(|q| swap.expand(2.0).contains(q))
            {
                let fore = self.color();
                self.hsv = sai_ui::rgb_to_hsv(self.back);
                self.back = fore;
            }
        });
        ui.add_space(2.0);
        // Кисти слоя: 4 в ряд, имя сверху, значок снизу справа.
        egui::Frame::none()
            .stroke(egui::Stroke::new(1.0_f32, sai_ui::BORDER))
            .fill(Color32::WHITE)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                let w = (ui.available_width() / 4.0).floor();
                for row in 0..self.brushes.len().div_ceil(4) {
                    ui.horizontal(|ui| {
                        for i in row * 4..(row * 4 + 4).min(self.brushes.len()) {
                            let (name, icon, enabled) = (
                                self.brushes[i].name,
                                self.brushes[i].icon,
                                self.brushes[i].enabled,
                            );
                            let selected = self.tool == Tool::Brush && self.brush == i;
                            let tip = if enabled {
                                name.to_string()
                            } else {
                                format!("{name} — позже")
                            };
                            let r = sai_ui::icon_button(
                                ui,
                                Vec2::new(w, 36.0),
                                selected,
                                enabled,
                                &tip,
                                |p, r, c| {
                                    p.text(
                                        r.left_top(),
                                        egui::Align2::LEFT_TOP,
                                        name,
                                        egui::FontId::proportional(10.0),
                                        c,
                                    );
                                    sai_ui::glyph(
                                        p,
                                        Rect::from_min_size(
                                            r.right_bottom() - Vec2::splat(16.0),
                                            Vec2::splat(16.0),
                                        ),
                                        c,
                                        icon,
                                    );
                                },
                            );
                            if r.clicked() {
                                self.tool = Tool::Brush;
                                self.brush = i;
                            }
                        }
                    });
                }
            });
        ui.add_space(4.0);
        let lw = 92.0;
        let w = ui.available_width() - lw - 8.0;
        let brush = &mut self.brushes[self.brush].params;
        sai_ui::row(ui, "Режим", lw, false, |ui| {
            egui::ComboBox::from_id_salt("paintmode")
                .selected_text("Обычный")
                .width(w - 8.0)
                .show_ui(ui, |_| {});
        });
        sai_ui::row(ui, "Жесткость", lw, true, |ui| {
            ui.spacing_mut().item_spacing.x = 1.0;
            for (i, step) in paint_core::brush::HARDNESS_STEPS.iter().enumerate() {
                let sel = (brush.hardness - step).abs() < 0.01;
                if sai_ui::icon_button(
                    ui,
                    Vec2::new(w / 5.0 - 1.0, 16.0),
                    sel,
                    true,
                    &format!("Ступень {}", i + 1),
                    |p, r, c| sai_ui::hardness_icon(p, r, i, c),
                )
                .clicked()
                {
                    brush.hardness = *step;
                }
            }
        });
        sai_ui::row(ui, "Размер", lw, true, |ui| {
            let text = if brush.size < 10.0 {
                format!("{:.1}", brush.size)
            } else {
                format!("{:.0}", brush.size)
            };
            let mut t = brush.size.ln();
            if sai_ui::bar(ui, &mut t, 0.5f32.ln(), 500f32.ln(), text, w, true).changed() {
                brush.size = (t.exp() * 10.0).round() / 10.0;
            }
        });
        sai_ui::row(
            ui,
            "Минимальный",
            lw,
            brush.size_by_pressure,
            |ui| {
                let mut v = brush.min_size * 100.0;
                let text = format!("{:.0}%", v);
                if sai_ui::bar(ui, &mut v, 0.0, 100.0, text, w, brush.size_by_pressure).changed() {
                    brush.min_size = (v.round() / 100.0).clamp(0.0, 1.0);
                }
            },
        );
        sai_ui::row(ui, "Плотн.", lw, true, |ui| {
            let mut v = brush.density * 100.0;
            let text = format!("{:.0}", v);
            if sai_ui::bar(ui, &mut v, 0.0, 100.0, text, w, true).changed() {
                brush.density = (v.round() / 100.0).clamp(0.0, 1.0);
            }
        });
        sai_ui::row(
            ui,
            "Мин. Плотн.",
            lw,
            brush.density_by_pressure,
            |ui| {
                let mut v = brush.min_density * 100.0;
                let text = format!("{:.0}%", v);
                if sai_ui::bar(ui, &mut v, 0.0, 100.0, text, w, brush.density_by_pressure).changed()
                {
                    brush.min_density = (v.round() / 100.0).clamp(0.0, 1.0);
                }
            },
        );
        if let Some(mix) = brush.mix.as_mut() {
            // Смешивание с холстом (как у Кисти, Акварели и Маркера в SAI).
            let pct = |ui: &mut egui::Ui, label: &str, v: &mut f32| {
                sai_ui::row(ui, label, lw, true, |ui| {
                    let mut t = *v * 100.0;
                    let text = format!("{:.0}", t);
                    if sai_ui::bar(ui, &mut t, 0.0, 100.0, text, w, true).changed() {
                        *v = (t.round() / 100.0).clamp(0.0, 1.0);
                    }
                });
            };
            pct(ui, "Смешивание", &mut mix.blending);
            pct(ui, "Разбавление", &mut mix.dilution);
            pct(ui, "Стойкость", &mut mix.persistence);
            if mix.blend_by_pressure {
                pct(ui, "Порог размыт.", &mut mix.blur_pressure);
            }
        }
        if sai_ui::section(ui, "sec_form", "Форма") {
            ui.weak("   Формы кисти — позже");
        }
        if sai_ui::section(ui, "sec_tex", "Текстура") {
            ui.weak("   Текстуры кисти — позже");
        }
        if sai_ui::section(ui, "sec_misc", "Разное") {
            sai_ui::row(ui, "   Sharpness", lw, true, |ui| {
                let mut v = brush.sharpness * 100.0;
                let text = format!("{:.0}", v);
                if sai_ui::bar(ui, &mut v, 0.0, 100.0, text, w, true).changed() {
                    brush.sharpness = (v.round() / 100.0).clamp(0.0, 1.0);
                }
            });
        }
        ui.horizontal(|ui| {
            ui.checkbox(&mut brush.size_by_pressure, "Размер");
            ui.checkbox(&mut brush.density_by_pressure, "Плотн.");
            if let Some(mix) = brush.mix.as_mut() {
                ui.checkbox(&mut mix.blend_by_pressure, "Смеш.");
            }
        });
        // Сетка размеров: кружок и число, 5 в ряд.
        let mut pick = None;
        let current = brush.size;
        egui::Frame::none()
            .stroke(egui::Stroke::new(1.0_f32, sai_ui::BORDER))
            .fill(Color32::WHITE)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing = Vec2::ZERO;
                        let cw = (ui.available_width() / 5.0).floor();
                        for row in SIZES.chunks(5) {
                            ui.horizontal(|ui| {
                                for &size in row {
                                    let sel = (current - size).abs() < 0.01;
                                    let r = sai_ui::icon_button(
                                        ui,
                                        Vec2::new(cw, 40.0),
                                        sel,
                                        true,
                                        "",
                                        |p, r, c| {
                                            let d = (size.sqrt() * 2.6).clamp(1.0, 22.0);
                                            p.circle_filled(
                                                r.center_top() + Vec2::new(0.0, 13.0),
                                                d / 2.0,
                                                c,
                                            );
                                            let text = if size < 10.0 && size.fract() != 0.0 {
                                                format!("{size:.1}")
                                            } else {
                                                format!("{size:.0}")
                                            };
                                            p.text(
                                                r.center_bottom(),
                                                egui::Align2::CENTER_BOTTOM,
                                                text,
                                                egui::FontId::proportional(10.0),
                                                c,
                                            );
                                        },
                                    );
                                    if r.clicked() {
                                        pick = Some(size);
                                    }
                                }
                            });
                        }
                    });
            });
        if let Some(size) = pick {
            self.brushes[self.brush].params.size = size;
        }
    }

    fn layer_rows(
        &mut self,
        ui: &mut egui::Ui,
        path: &mut Vec<usize>,
        depth: usize,
        changed: &mut bool,
    ) {
        let count = match path.as_slice() {
            [] => self.doc.layers.len(),
            p => self
                .doc
                .layer(p)
                .and_then(|l| l.children.as_ref())
                .map_or(0, Vec::len),
        };
        // В списке верхний слой — первым.
        for i in (0..count).rev() {
            path.push(i);
            let Some(layer) = self.doc.layer(path) else {
                path.pop();
                continue;
            };
            let is_folder = layer.children.is_some();
            let (name, visible, clipping) = (layer.name.clone(), layer.visible, layer.clipping);
            let mode = MODES
                .iter()
                .find(|(m, _)| *m == layer.mode)
                .map_or("Обычный", |(_, n)| n);
            let mode = if is_folder && layer.pass_through {
                "Насквозь"
            } else {
                mode
            };
            let opacity = format!("{}%", (layer.opacity * 100.0).round());
            let selected = self.selected == *path;
            let fid = ui.make_persistent_id(("folder", path.clone()));
            let mut open = ui.data_mut(|d| *d.get_persisted_mut_or(fid, false));
            let (rect, response) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), 42.0), egui::Sense::click());
            let p = ui.painter();
            p.rect_filled(
                rect,
                0.0,
                if selected {
                    sai_ui::SELECT_BG
                } else {
                    Color32::WHITE
                },
            );
            p.rect_stroke(
                rect.shrink(0.5),
                0.0,
                egui::Stroke::new(
                    1.0_f32,
                    if selected {
                        sai_ui::SELECT_EDGE
                    } else {
                        Color32::from_gray(200)
                    },
                ),
            );
            let x0 = rect.left() + 3.0 + depth as f32 * 10.0;
            let eye = Rect::from_min_size(Pos2::new(x0, rect.top() + 3.0), Vec2::new(18.0, 17.0));
            p.rect_stroke(eye, 0.0, egui::Stroke::new(1.0_f32, sai_ui::BORDER));
            if visible {
                sai_ui::glyph(p, eye.shrink(2.0), sai_ui::INK, "eye");
            }
            let mark = Rect::from_min_size(Pos2::new(x0, rect.top() + 22.0), Vec2::new(18.0, 17.0));
            p.rect_stroke(mark, 0.0, egui::Stroke::new(1.0_f32, sai_ui::BORDER));
            if selected && !is_folder {
                sai_ui::glyph(p, mark.shrink(3.0), sai_ui::INK, "pen");
            }
            let thumb =
                Rect::from_min_size(Pos2::new(x0 + 22.0, rect.top() + 3.0), Vec2::splat(36.0));
            if is_folder {
                sai_ui::glyph(p, thumb.shrink(6.0), Color32::from_gray(90), "new_folder");
                let arrow = if open { "▾" } else { "▸" };
                p.text(
                    thumb.left_bottom() + Vec2::new(2.0, -2.0),
                    egui::Align2::LEFT_BOTTOM,
                    arrow,
                    egui::FontId::proportional(12.0),
                    sai_ui::INK,
                );
            } else {
                p.rect_filled(thumb, 0.0, Color32::WHITE);
                if !self.thumbs.contains_key(path.as_slice()) {
                    if let Some(img) = layer_thumbnail(&self.doc, path, 36) {
                        let t = ui.ctx().load_texture(format!("thumb-{path:?}"), img, egui::TextureOptions::LINEAR);
                        self.thumbs.insert(path.clone(), t);
                    }
                }
                if let Some(t) = self.thumbs.get(path.as_slice()) {
                    let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
                    ui.painter().image(t.id(), thumb, uv, Color32::WHITE);
                }
                let p = ui.painter();
                p.rect_stroke(
                    thumb,
                    0.0,
                    egui::Stroke::new(1.0_f32, Color32::from_gray(190)),
                );
            }
            if clipping {
                p.rect_filled(
                    Rect::from_min_size(
                        Pos2::new(thumb.left() - 3.0, rect.top() + 3.0),
                        Vec2::new(2.0, 36.0),
                    ),
                    0.0,
                    Color32::from_rgb(200, 60, 60),
                );
            }
            let tx = thumb.right() + 4.0;
            p.text(
                Pos2::new(tx, rect.top() + 3.0),
                egui::Align2::LEFT_TOP,
                &name,
                egui::FontId::proportional(12.0),
                sai_ui::INK,
            );
            p.text(
                Pos2::new(tx, rect.top() + 18.0),
                egui::Align2::LEFT_TOP,
                mode,
                egui::FontId::proportional(10.0),
                sai_ui::INK,
            );
            p.text(
                Pos2::new(tx, rect.top() + 29.0),
                egui::Align2::LEFT_TOP,
                &opacity,
                egui::FontId::proportional(10.0),
                sai_ui::INK,
            );
            if response.clicked() {
                let at = response.interact_pointer_pos().unwrap_or_default();
                if eye.contains(at) {
                    if let Some(l) = self.doc.layer_mut(path) {
                        l.visible = !l.visible;
                        *changed = true;
                    }
                } else if is_folder && thumb.contains(at) {
                    open = !open;
                    ui.data_mut(|d| d.insert_persisted(fid, open));
                } else {
                    self.selected = path.clone();
                }
            }
            if response.double_clicked() && is_folder {
                open = !open;
                ui.data_mut(|d| d.insert_persisted(fid, open));
            }
            if is_folder && open {
                self.layer_rows(ui, path, depth + 1, changed);
            }
            path.pop();
        }
    }

    /// Навигатор: уменьшенный холст из тех же текстур, что и на экране, и рамка видимой части.
    fn navigator(&mut self, ui: &mut egui::Ui) {
        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), 150.0),
            egui::Sense::click_and_drag(),
        );
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 0.0, Color32::WHITE);
        let (w, h) = (self.doc.width as f32, self.doc.height as f32);
        let k = ((rect.width() - 6.0) / w).min((rect.height() - 6.0) / h);
        let origin = rect.center() - Vec2::new(w, h) * k / 2.0;
        for (&(dx, dy), tile) in &self.display {
            if let Some(t) = &tile.texture {
                let min = origin + Vec2::new((dx * DT) as f32, (dy * DT) as f32) * k;
                let part = Vec2::new(
                    (w - (dx * DT) as f32).min(DT as f32),
                    (h - (dy * DT) as f32).min(DT as f32),
                );
                let uv = Rect::from_min_max(
                    Pos2::ZERO,
                    Pos2::new(part.x / DT as f32, part.y / DT as f32),
                );
                p.image(
                    t.id(),
                    Rect::from_min_size(min, part * k),
                    uv,
                    Color32::WHITE,
                );
            }
        }
        p.rect_stroke(
            Rect::from_min_size(origin, Vec2::new(w, h) * k),
            0.0,
            egui::Stroke::new(1.0_f32, Color32::from_gray(150)),
        );
        if self.view.is_positive() {
            let seen =
                Rect::from_min_size(origin + self.offset * k, self.view.size() / self.zoom * k);
            p.rect_stroke(
                seen,
                0.0,
                egui::Stroke::new(1.0_f32, Color32::from_rgb(220, 40, 40)),
            );
        }
        if let Some(pos) = response.interact_pointer_pos() {
            if response.is_pointer_button_down_on() {
                let c = (pos - origin) / k;
                self.offset = c - self.view.size() / self.zoom / 2.0;
            }
        }
        p.rect_stroke(rect, 0.0, egui::Stroke::new(1.0_f32, sai_ui::BORDER));
    }

    fn right_panel(&mut self, ui: &mut egui::Ui) {
        let mut changed = false;
        self.navigator(ui);
        let lw = 50.0;
        let b = Vec2::splat(18.0);
        let w = ui.available_width() - lw - 3.0 * 20.0 - 10.0;
        sai_ui::row(ui, "Масштаб", lw, true, |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let mut z = self.zoom.ln();
            let text = format!("{:.0}%", self.zoom * 100.0);
            if sai_ui::bar(ui, &mut z, 0.02f32.ln(), 32f32.ln(), text, w, true).changed() {
                self.set_zoom(z.exp());
            }
            if sai_ui::icon_button(ui, b, false, true, "Уменьшить", |p, r, c| {
                sai_ui::glyph(p, r, c, "minus")
            })
            .clicked()
            {
                self.set_zoom(self.zoom / 1.25);
            }
            if sai_ui::icon_button(ui, b, false, true, "Увеличить", |p, r, c| {
                sai_ui::glyph(p, r, c, "plus")
            })
            .clicked()
            {
                self.set_zoom(self.zoom * 1.25);
            }
            if sai_ui::icon_button(ui, b, false, true, "100%", |p, r, c| {
                sai_ui::glyph(p, r, c, "reset")
            })
            .clicked()
            {
                self.set_zoom(1.0);
            }
        });
        sai_ui::row(ui, "Угол", lw, false, |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let mut a = 0.0;
            sai_ui::bar(ui, &mut a, -180.0, 180.0, "+0°".into(), w, false);
            for g in ["ccw", "cw", "reset"] {
                sai_ui::icon_button(
                    ui,
                    b,
                    false,
                    false,
                    "Поворот вида — позже",
                    |p, r, c| sai_ui::glyph(p, r, c, g),
                );
            }
        });
        ui.add_space(3.0);
        if sai_ui::section(ui, "sec_effect", "Эффекты слоя") {
            sai_ui::row(ui, "   Эффект", lw + 10.0, false, |ui| {
                egui::ComboBox::from_id_salt("effect")
                    .selected_text("[ Нет ]")
                    .width(100.0)
                    .show_ui(ui, |_| {});
            });
        }
        if sai_ui::section(ui, "sec_paper", "Текстура") {
            sai_ui::row(ui, "   Выбрать", lw + 10.0, false, |ui| {
                egui::ComboBox::from_id_salt("paper")
                    .selected_text("[ Нет ]")
                    .width(100.0)
                    .show_ui(ui, |_| {});
            });
        }
        let lw2 = 52.0;
        let w2 = ui.available_width() - lw2 - 8.0;
        let path = self.selected.clone();
        if let Some(layer) = self.doc.layer_mut(&path) {
            let current = MODES
                .iter()
                .find(|(m, _)| *m == layer.mode)
                .map_or("Обычный", |(_, n)| n);
            sai_ui::row(ui, "Режим", lw2, true, |ui| {
                egui::ComboBox::from_id_salt("mode")
                    .selected_text(current)
                    .width(w2 - 4.0)
                    .height(420.0)
                    .show_ui(ui, |ui| {
                        for (mode, name) in MODES {
                            changed |= ui.selectable_value(&mut layer.mode, mode, name).changed();
                        }
                    });
            });
            sai_ui::row(ui, "Непроз.", lw2, true, |ui| {
                let mut v = layer.opacity * 100.0;
                let text = format!("{:.0}%", v);
                if sai_ui::bar(ui, &mut v, 0.0, 100.0, text, w2, true).changed() {
                    layer.opacity = v.round() / 100.0;
                    changed = true;
                }
            });
            sai_ui::row(ui, "Замок", lw2, true, |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                if sai_ui::icon_button(
                    ui,
                    b,
                    layer.preserve_opacity,
                    true,
                    "Сохранять непрозрачность",
                    |p, r, c| sai_ui::glyph(p, r, c, "lock_alpha"),
                )
                .clicked()
                {
                    layer.preserve_opacity = !layer.preserve_opacity;
                }
                for (g, tip) in [
                    ("lock_paint", "Запрет рисования — позже"),
                    ("move", "Запрет перемещения — позже"),
                    ("lock_all", "Запрет всего — позже"),
                ] {
                    sai_ui::icon_button(ui, b, false, false, tip, |p, r, c| {
                        sai_ui::glyph(p, r, c, g)
                    });
                }
            });
            changed |= ui.checkbox(&mut layer.clipping, "Прикрепление").changed();
            if layer.children.is_some() {
                changed |= ui
                    .checkbox(&mut layer.pass_through, "Насквозь (папка)")
                    .changed();
            }
            ui.add_enabled(false, egui::RadioButton::new(false, "Источник выбора"));
        }
        ui.add_space(3.0);
        let b2 = Vec2::splat(22.0);
        ui.horizontal(|ui| {
            if sai_ui::icon_button(ui, b2, false, true, "Новый слой", |p, r, c| {
                sai_ui::glyph(p, r, c, "new_layer")
            })
            .clicked()
            {
                self.add_layer(false);
            }
            sai_ui::icon_button(
                ui,
                b2,
                false,
                false,
                "Новый слой контура — позже",
                |p, r, c| sai_ui::glyph(p, r, c, "new_linework"),
            );
            if sai_ui::icon_button(ui, b2, false, true, "Новая папка", |p, r, c| {
                sai_ui::glyph(p, r, c, "new_folder")
            })
            .clicked()
            {
                self.add_layer(true);
            }
            sai_ui::icon_button(
                ui,
                b2,
                false,
                false,
                "Линейка перспективы — позже",
                |p, r, c| sai_ui::glyph(p, r, c, "ruler"),
            );
            sai_ui::icon_button(
                ui,
                b2,
                false,
                false,
                "Маска слоя — позже",
                |p, r, c| sai_ui::glyph(p, r, c, "mask"),
            );
        });
        ui.horizontal(|ui| {
            for (g, tip, action) in [
                ("down", "Перенести на нижний (F)", LayerAction::Transfer),
                ("merge", "Слить с нижним (Ctrl+E)", LayerAction::Merge),
                ("clear", "Очистить слой (Delete)", LayerAction::Clear),
                ("delete", "Удалить слой", LayerAction::Delete),
            ] {
                if sai_ui::icon_button(ui, b2, false, true, tip, |p, r, c| sai_ui::glyph(p, r, c, g)).clicked() {
                    self.layer_action(action);
                }
            }
            if sai_ui::icon_button(ui, b2, false, true, "Слой выше", |p, r, c| sai_ui::glyph(p, r, c, "up")).clicked() {
                self.layer_action(LayerAction::Up);
            }
            if sai_ui::icon_button(ui, b2, false, true, "Слой ниже", |p, r, c| sai_ui::glyph(p, r, c, "lower")).clicked() {
                self.layer_action(LayerAction::Down);
            }
        });
        egui::Frame::none()
            .fill(sai_ui::FIELD)
            .stroke(egui::Stroke::new(1.0_f32, sai_ui::BORDER))
            .inner_margin(2.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 1.0;
                        let mut path = Vec::new();
                        self.layer_rows(ui, &mut path, 0, &mut changed);
                    });
            });
        if changed {
            self.dirty = true;
            self.invalidate_all();
        }
    }

    fn canvas(&mut self, ui: &mut egui::Ui) {
        let (rect, response) =
            ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
        self.view = rect;
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, sai_ui::FIELD);
        let (w, h) = (self.doc.width as f32, self.doc.height as f32);
        if self.fit_pending && rect.width() > 10.0 {
            self.zoom = (rect.width() / w).min(rect.height() / h).min(1.0) * 0.95;
            self.offset = Vec2::new(
                (w - rect.width() / self.zoom) / 2.0,
                (h - rect.height() / self.zoom) / 2.0,
            );
            self.fit_pending = false;
        }
        let to_canvas =
            |p: Pos2, zoom: f32, offset: Vec2| -> Vec2 { (p - rect.min) / zoom + offset };

        // Масштаб колесом — вокруг указателя; перетаскивание средней кнопкой или «Рукой» — сдвиг.
        let (scroll, zoom_delta, middle, space, pointer, moves, pressed, released, touches) = ui
            .input(|i| {
                let moves: Vec<Pos2> = i
                    .events
                    .iter()
                    .filter_map(|e| {
                        if let egui::Event::PointerMoved(p) = e {
                            Some(*p)
                        } else {
                            None
                        }
                    })
                    .collect();
                let touches: Vec<(egui::TouchPhase, Option<f32>)> = i
                    .events
                    .iter()
                    .filter_map(|e| {
                        if let egui::Event::Touch { phase, force, .. } = e {
                            Some((*phase, *force))
                        } else {
                            None
                        }
                    })
                    .collect();
                (
                    i.smooth_scroll_delta.y,
                    i.zoom_delta(),
                    i.pointer.middle_down(),
                    i.key_down(egui::Key::Space),
                    i.pointer.latest_pos(),
                    moves,
                    i.pointer.primary_pressed(),
                    i.pointer.primary_released(),
                    touches,
                )
            });
        // Перо через Windows Ink приходит касанием с силой нажима.
        for (phase, force) in touches {
            match phase {
                egui::TouchPhase::Start | egui::TouchPhase::Move => {
                    self.touch_down = true;
                    match force {
                        Some(f) => self.touch_force = Some(f.clamp(0.0, 1.0)),
                        // Перо уже известно, а силы нет — это касание легче первого уровня нажима.
                        None if self.touch_force.is_some() => self.touch_force = Some(0.0),
                        None => {}
                    }
                }
                egui::TouchPhase::End | egui::TouchPhase::Cancel => self.touch_down = false,
            }
        }
        if let Some(pos) = pointer.filter(|p| rect.contains(*p)) {
            let factor = if zoom_delta != 1.0 {
                zoom_delta
            } else if scroll != 0.0 {
                (scroll / 240.0).exp()
            } else {
                1.0
            };
            if factor != 1.0 {
                let anchor = to_canvas(pos, self.zoom, self.offset);
                self.zoom = (self.zoom * factor).clamp(0.02, 32.0);
                self.offset = anchor - (pos - rect.min) / self.zoom;
            }
        }
        let panning = middle || space || self.tool == Tool::Hand;
        if panning && (response.dragged() || middle) {
            self.offset -= ui.input(|i| i.pointer.delta()) / self.zoom;
        } else if self.tool == Tool::Zoom {
            if response.clicked() {
                if let Some(pos) = response.interact_pointer_pos() {
                    let anchor = to_canvas(pos, self.zoom, self.offset);
                    let out = ui.input(|i| i.modifiers.alt);
                    self.zoom = (if out {
                        self.zoom / 1.5
                    } else {
                        self.zoom * 1.5
                    })
                    .clamp(0.02, 32.0);
                    self.offset = anchor - (pos - rect.min) / self.zoom;
                }
            }
        } else if self.tool == Tool::Picker
            || (ui.input(|i| i.modifiers.alt) && self.live.is_none())
        {
            if let Some(pos) = response.interact_pointer_pos() {
                let c = to_canvas(pos, self.zoom, self.offset);
                self.pick_color(c.x, c.y);
            }
        } else if !panning {
            let (pressure, eraser_end) = self.pressure();
            let point = |p: Pos2, zoom: f32, offset: Vec2| {
                let c = to_canvas(p, zoom, offset);
                InputPoint {
                    x: c.x,
                    y: c.y,
                    pressure,
                }
            };
            if pressed && response.hovered() {
                if let Some(pos) = pointer {
                    self.begin_stroke(point(pos, self.zoom, self.offset), eraser_end);
                }
            } else if self.live.is_some() {
                for pos in moves {
                    self.continue_stroke(point(pos, self.zoom, self.offset));
                }
            }
            if released {
                self.end_stroke();
            }
        }

        // Показ: только плитки, попавшие в окно.
        let origin = rect.min - self.offset * self.zoom;
        let options = if self.zoom >= 2.0 {
            egui::TextureOptions::NEAREST
        } else {
            egui::TextureOptions::LINEAR
        };
        let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
        painter.rect_filled(
            Rect::from_min_size(origin, Vec2::new(w, h) * self.zoom).intersect(rect),
            0.0,
            Color32::WHITE,
        );
        for (&(dx, dy), tile) in self.display.iter_mut() {
            let min = origin + Vec2::new((dx * DT) as f32, (dy * DT) as f32) * self.zoom;
            let screen = Rect::from_min_size(min, Vec2::splat(DT as f32 * self.zoom));
            if !screen.intersects(rect) {
                continue;
            }
            if tile.stale || tile.texture.is_none() {
                let image = egui::ColorImage::from_rgba_unmultiplied(
                    [DT as usize, DT as usize],
                    &tile.pixels,
                );
                match &mut tile.texture {
                    Some(t) => t.set(image, options),
                    None => {
                        tile.texture = Some(ui.ctx().load_texture(
                            format!("tile-{dx}-{dy}"),
                            image,
                            options,
                        ))
                    }
                }
                tile.stale = false;
            }
            if let Some(t) = &tile.texture {
                // Край холста внутри плитки: показываем только часть до границы.
                let part = Vec2::new(
                    (w - (dx * DT) as f32).min(DT as f32),
                    (h - (dy * DT) as f32).min(DT as f32),
                );
                let shown = Rect::from_min_size(min, part * self.zoom);
                let shown_uv =
                    Rect::from_min_max(uv.min, Pos2::new(part.x / DT as f32, part.y / DT as f32));
                painter.image(t.id(), shown, shown_uv, Color32::WHITE);
            }
        }
        // Кружок размера кисти.
        if let Some(pos) = pointer.filter(|p| rect.contains(*p)) {
            if self.tool == Tool::Brush {
                let size = self.brushes[self.brush].params.size;
                painter.circle_stroke(
                    pos,
                    (size * self.zoom / 2.0).max(1.0),
                    egui::Stroke::new(1.0_f32, Color32::from_gray(90)),
                );
            }
        }
    }
}

impl PaintApp {
    fn dialogs(&mut self, ctx: &egui::Context) {
        if self.show_open {
            let mut open = true;
            egui::Window::new("Открыть")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label("Путь к файлу .sai2 (или перетащите файл в окно):");
                    ui.add(egui::TextEdit::singleline(&mut self.open_path).desired_width(420.0));
                    ui.horizontal(|ui| {
                        if ui.button("Открыть").clicked() && !self.open_path.trim().is_empty()
                        {
                            self.open(PathBuf::from(self.open_path.trim()));
                            self.show_open = false;
                        }
                        if ui.button("Отмена").clicked() {
                            self.show_open = false;
                        }
                    });
                });
            self.show_open &= open;
        }
        if self.show_new {
            let mut open = true;
            egui::Window::new("Новый холст")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    egui::Grid::new("newcanvas").num_columns(2).show(ui, |ui| {
                        ui.label("Ширина :");
                        ui.add(
                            egui::DragValue::new(&mut self.new_size[0])
                                .range(1..=16000)
                                .suffix(" px"),
                        );
                        ui.end_row();
                        ui.label("Высота :");
                        ui.add(
                            egui::DragValue::new(&mut self.new_size[1])
                                .range(1..=16000)
                                .suffix(" px"),
                        );
                        ui.end_row();
                    });
                    ui.horizontal(|ui| {
                        for (w, h) in [(512, 512), (1200, 900), (2480, 3508), (4096, 4096)] {
                            if ui.small_button(format!("{w}×{h}")).clicked() {
                                self.new_size = [w, h];
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui.button("OK").clicked() {
                            let [w, h] = self.new_size;
                            self.new_canvas(w, h);
                            self.show_new = false;
                        }
                        if ui.button("Отмена").clicked() {
                            self.show_new = false;
                        }
                    });
                });
            self.show_new &= open;
        }
        if self.show_save_as {
            let mut open = true;
            egui::Window::new("Сохранить как").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
                ui.label("Путь к файлу .sai2:");
                ui.add(egui::TextEdit::singleline(&mut self.save_path).desired_width(460.0));
                if self.had_vectors {
                    ui.colored_label(egui::Color32::from_rgb(170, 60, 20), "В открытом файле есть слои контура или фигур: их кривые пока сохраняются\nрастром. Поэтому предложен новый файл — исходный останется нетронутым.");
                }
                let path = PathBuf::from(self.save_path.trim());
                let exists = path.exists();
                if exists {
                    ui.colored_label(egui::Color32::from_rgb(170, 60, 20), "Такой файл уже есть — он будет заменён.");
                }
                ui.horizontal(|ui| {
                    if ui.button(if exists { "Заменить" } else { "Сохранить" }).clicked() && !self.save_path.trim().is_empty() {
                        self.save_to(path.with_extension("sai2"));
                        self.show_save_as = false;
                    }
                    if ui.button("Отмена").clicked() {
                        self.show_save_as = false;
                    }
                });
            });
            self.show_save_as &= open;
        }
        if self.show_about {
            let mut open = true;
            egui::Window::new("О программе").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
                ui.label(format!("SOsai {} — редактор по образцу PaintTool SAI 2, личный проект.", env!("CARGO_PKG_VERSION")));
                ui.label("Исходный код: github.com/grazyping/SOsai");
                ui.label("Открывает .sai2 (растровые слои, папки, контур, фигуры), рисует кистью образца.");
            });
            self.show_about &= open;
        }
    }
}

/// Миниатюра слоя поверх белого: ближайшие точки на сетке `size`×`size` с полями по пропорции.
fn layer_thumbnail(doc: &Document, path: &[usize], size: usize) -> Option<egui::ColorImage> {
    let layer = doc.layer(path)?;
    let (w, h) = (doc.width as f32, doc.height as f32);
    let k = (w / size as f32).max(h / size as f32);
    let (ox, oy) = ((size as f32 - w / k) / 2.0, (size as f32 - h / k) / 2.0);
    let mut img = egui::ColorImage::new([size, size], Color32::WHITE);
    for y in 0..size {
        for x in 0..size {
            let (cx, cy) = (((x as f32 + 0.5 - ox) * k) as i32, ((y as f32 + 0.5 - oy) * k) as i32);
            if cx < 0 || cy < 0 || cx >= doc.width as i32 || cy >= doc.height as i32 {
                img[(x, y)] = Color32::from_gray(225);
                continue;
            }
            let [r, g, b, a] = paint_core::pixel::to_rgba8(layer.pixels.get(cx, cy));
            let a = a as u32;
            let over = |c: u8| ((c as u32 * a + 255 * (255 - a)) / 255) as u8;
            img[(x, y)] = Color32::from_rgb(over(r), over(g), over(b));
        }
    }
    Some(img)
}

fn count_layers(layers: &[Layer]) -> usize {
    layers
        .iter()
        .map(|l| 1 + l.children.as_deref().map_or(0, count_layers))
        .sum()
}

/// Занятая память процесса и вся память, байт (Linux; на других системах — нули).
fn memory_use() -> (u64, u64) {
    #[cfg(target_os = "linux")]
    {
        let rss = std::fs::read_to_string("/proc/self/statm")
            .ok()
            .and_then(|s| {
                s.split_whitespace()
                    .nth(1)
                    .and_then(|v| v.parse::<u64>().ok())
            })
            .unwrap_or(0)
            * 4096;
        let total = std::fs::read_to_string("/proc/meminfo")
            .ok()
            .and_then(|s| {
                s.lines().find(|l| l.starts_with("MemTotal")).and_then(|l| {
                    l.split_whitespace()
                        .nth(1)
                        .and_then(|v| v.parse::<u64>().ok())
                })
            })
            .unwrap_or(0)
            * 1024;
        (rss, total)
    }
    #[cfg(not(target_os = "linux"))]
    {
        (0, 0)
    }
}

fn first_raster(layers: &[Layer]) -> Option<Vec<usize>> {
    for (i, layer) in layers.iter().enumerate().rev() {
        match &layer.children {
            None => return Some(vec![i]),
            Some(children) => {
                if let Some(mut rest) = first_raster(children) {
                    rest.insert(0, i);
                    return Some(rest);
                }
            }
        }
    }
    None
}

impl eframe::App for PaintApp {
    /// Для проверок: `PAINT_EXPORT_ON_EXIT=файл.png` — сохранить холст и вывести сводку по перу.
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if let Ok(out) = std::env::var("PAINT_EXPORT_ON_EXIT") {
            let r = paint_core::io::save_png(std::path::Path::new(&out), self.doc.width, self.doc.height, &self.doc.flatten());
            eprintln!("export: {} -> {out}", if r.is_ok() { "saved" } else { "failed" });
            let info = self.pen.lock().map(|s| format!("{} [{}]", s.info, s.device)).unwrap_or_default();
            eprintln!("pen: {info}");
            for (src, (lo, hi, n)) in &self.pen_stats {
                eprintln!("pressure {src}: {n} замеров, от {lo:.3} до {hi:.3}");
            }
        }
    }

    /// Виртуальное перо: события касания с силой нажима и указателя, как от Windows Ink.
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        let Some(vp) = self.virtual_pen.as_mut() else { return };
        ctx.request_repaint();
        if !self.view.is_positive() {
            return;
        }
        let samples = vp.due();
        let finished = vp.finished();
        for smp in samples {
            let pos = self.view.min + (Vec2::new(smp.x, smp.y) - self.offset) * self.zoom;
            if let Ok(mut st) = self.pen.lock() {
                // обратный конец пера сообщает источник пера, как драйвер на Linux
                if smp.eraser {
                    st.eraser = true;
                    st.pressure = smp.pressure;
                    st.at = Some(Instant::now());
                } else {
                    st.eraser = false;
                    st.at = None;
                }
            }
            let (phase, button) = match smp.phase {
                virtual_pen::Phase::Down => (egui::TouchPhase::Start, Some(true)),
                virtual_pen::Phase::Move => (egui::TouchPhase::Move, None),
                virtual_pen::Phase::Up => (egui::TouchPhase::End, Some(false)),
            };
            raw.events.push(egui::Event::PointerMoved(pos));
            raw.events.push(egui::Event::Touch { device_id: egui::TouchDeviceId(7), id: egui::TouchId(0), phase, pos, force: Some(smp.pressure) });
            if let Some(pressed) = button {
                raw.events.push(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE });
            }
        }
        if finished {
            self.virtual_done += 1;
            if self.virtual_done == 20 {
                self.status = String::from("Виртуальное перо: сценарий закончен");
                if let Ok(out) = std::env::var("PAINT_VIRTUAL_PEN_OUT") {
                    let r = paint_core::io::save_png(std::path::Path::new(&out), self.doc.width, self.doc.height, &self.doc.flatten());
                    eprintln!("virtual pen: {} -> {out}", if r.is_ok() { "saved" } else { "save failed" });
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let dropped = ctx.input(|i| i.raw.dropped_files.iter().find_map(|f| f.path.clone()));
        if let Some(path) = dropped {
            self.open_path = path.display().to_string();
            self.open(path);
        }
        let (undo, redo) = ctx.input(|i| {
            let z = i.key_pressed(egui::Key::Z) && i.modifiers.command;
            (
                z && !i.modifiers.shift,
                (z && i.modifiers.shift) || (i.key_pressed(egui::Key::Y) && i.modifiers.command),
            )
        });
        if undo {
            self.undo();
        }
        if redo {
            self.redo();
        }

        // клавиши образца: [ и ] — размер кисти, Ctrl+Plus/Minus — масштаб
        let (smaller, bigger, zin, zout) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::OpenBracket),
                i.key_pressed(egui::Key::CloseBracket),
                i.modifiers.command
                    && (i.key_pressed(egui::Key::Plus) || i.key_pressed(egui::Key::Equals)),
                i.modifiers.command && i.key_pressed(egui::Key::Minus),
            )
        });
        let (key_save, key_save_as) = ctx.input(|i| {
            let s = i.modifiers.command && i.key_pressed(egui::Key::S);
            (s && !i.modifiers.shift, s && i.modifiers.shift)
        });
        if key_save {
            self.save();
        }
        if key_save_as {
            self.ask_save_as();
        }
        let (key_clear, key_merge, key_transfer) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::Delete),
                i.modifiers.command && i.key_pressed(egui::Key::E),
                !i.modifiers.any() && i.key_pressed(egui::Key::F),
            )
        });
        if !ctx.wants_keyboard_input() && self.live.is_none() {
            if key_clear {
                self.layer_action(LayerAction::Clear);
            }
            if key_merge {
                self.layer_action(LayerAction::Merge);
            }
            if key_transfer {
                self.layer_action(LayerAction::Transfer);
            }
        }
        if !ctx.wants_keyboard_input() {
            let size = &mut self.brushes[self.brush].params.size;
            if bigger {
                *size = SIZES
                    .iter()
                    .copied()
                    .find(|s| *s > *size + 0.01)
                    .unwrap_or(*size);
            }
            if smaller {
                *size = SIZES
                    .iter()
                    .rev()
                    .copied()
                    .find(|s| *s < *size - 0.01)
                    .unwrap_or(*size);
            }
            if zin {
                self.set_zoom(self.zoom * 1.25);
            }
            if zout {
                self.set_zoom(self.zoom / 1.25);
            }
        }

        if let Some(secs) = std::env::var("PAINT_EXIT_AFTER").ok().and_then(|v| v.parse::<f32>().ok()) {
            if self.started.elapsed().as_secs_f32() > secs {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            ctx.request_repaint_after(Duration::from_millis(200));
        }
        let title = format!(
            "SOsai {} — {}{}",
            env!("CARGO_PKG_VERSION"),
            self.file_name(),
            if self.dirty { " (*)" } else { "" }
        );
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        egui::TopBottomPanel::top("menu").show(ctx, |ui| self.menu_bar(ui));
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(&self.status);
                for note in &self.notes {
                    ui.separator();
                    ui.weak(note);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (used, total) = memory_use();
                    let mut v = if total > 0 {
                        used as f32 / total as f32 * 100.0
                    } else {
                        0.0
                    };
                    let text = format!("{:.1}%", v);
                    sai_ui::bar(ui, &mut v, 0.0, 100.0, text, 110.0, false);
                    ui.label("Памяти занято");
                    ui.separator();
                    let info = self.pen.lock().map(|s| s.info.clone()).unwrap_or_default();
                    ui.weak(format!(
                        "{info} | {}: нажим {:.2}",
                        self.last_source, self.last_pressure
                    ));
                    if !self.queue.is_empty() {
                        ui.separator();
                        ui.weak(format!("пересчёт: {} плиток", self.queue.len()));
                    }
                });
            });
        });
        egui::SidePanel::left("tools")
            .exact_width(236.0)
            .resizable(false)
            .show(ctx, |ui| self.left_panel(ui));
        egui::SidePanel::right("layers")
            .exact_width(186.0)
            .resizable(false)
            .show(ctx, |ui| self.right_panel(ui));
        egui::TopBottomPanel::top("quick").show(ctx, |ui| self.quick_bar(ui));
        egui::TopBottomPanel::bottom("tabs").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let (rect, _) =
                    ui.allocate_exact_size(Vec2::new(180.0, 18.0), egui::Sense::click());
                let p = ui.painter();
                p.rect_filled(rect, 0.0, sai_ui::SELECT_BG);
                p.rect_stroke(rect, 0.0, egui::Stroke::new(1.0_f32, sai_ui::SELECT_EDGE));
                p.text(
                    rect.left_center() + Vec2::new(6.0, 0.0),
                    egui::Align2::LEFT_CENTER,
                    self.file_name(),
                    egui::FontId::proportional(12.0),
                    sai_ui::INK,
                );
                p.text(
                    rect.right_center() - Vec2::new(4.0, 0.0),
                    egui::Align2::RIGHT_CENTER,
                    format!("{:.0}%", self.zoom * 100.0),
                    egui::FontId::proportional(10.0),
                    sai_ui::INK,
                );
            });
        });
        egui::CentralPanel::default()
            .frame(egui::Frame::none())
            .show(ctx, |ui| self.canvas(ui));
        self.dialogs(ctx);

        self.render_pending(Duration::from_millis(12));
        if !self.queue.is_empty() {
            ctx.request_repaint();
        }
    }
}

fn main() -> eframe::Result {
    // На Wayland перо доходит до окна только через XWayland (см. pen.rs), поэтому открываемся там.
    // PAINT_RS_WAYLAND=1 оставляет родной Wayland — без нажима пера.
    #[cfg(target_os = "linux")]
    if std::env::var_os("DISPLAY").is_some() && std::env::var_os("PAINT_RS_WAYLAND").is_none() {
        std::env::remove_var("WAYLAND_DISPLAY");
        std::env::remove_var("WAYLAND_SOCKET");
    }
    let path = std::env::args().nth(1).map(PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1360.0, 900.0])
            .with_fullscreen(std::env::var_os("PAINT_FULLSCREEN").is_some())
            .with_title("SOsai"),
        ..Default::default()
    };
    eframe::run_native(
        "SOsai",
        options,
        Box::new(move |cc| {
            sai_ui::apply_style(&cc.egui_ctx);
            let feed = pen::new_feed();
            pen::spawn(feed.clone(), cc.egui_ctx.clone());
            Ok(Box::new(PaintApp::new(path, feed)))
        }),
    )
}
