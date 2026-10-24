//! «Автодействия» (как Auto Actions в Clip Studio Paint и «Операции» в Photoshop): запись команд
//! меню с их настройками и повтор одной кнопкой. Пишутся фильтры, команды слоя, холста и
//! выделения; рисование, смена инструмента и отмена не пишутся. Наборы — текстовые файлы
//! `<набор>.gsact` в папке `actions` настроек программы. Одно действие — одна отмена.

use crate::i18n::{tr, trf};
use crate::{filter_title, FilterDialog, FilterKind, LayerAction, PaintApp};
use eframe::egui;
use paint_core::filters::DistortKind;

/// Шаг действия.
#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    Filter(FilterKind, Vec<f64>),
    Layer(LayerAction),
    /// Повернуть слой (по часовой — `Some(true)`) или отразить (по горизонтали — `Some(true)`).
    LayerTurn(Option<bool>, Option<bool>),
    /// Холст: 0 — отразить по горизонтали, 1 — по вертикали, 2 — повернуть против часовой, 3 — по часовой.
    Canvas(u8),
    Trim,
    Flatten,
    SelectAll,
    Deselect,
    InvertSelection,
    /// Расширить (`true`) или сузить выделение на n точек.
    Grow(u32, bool),
    /// Новый слой: 0 — обычный, 1 — папка, 2 — контура, 3 — фигур.
    NewLayer(u8),
    /// Новый слой коррекции (её текст, `Adjust::to_text`).
    Adjust(String),
    /// Залить выделенное основным цветом.
    Fill,
}

pub(crate) const FILTERS: [FilterKind; 33] = [
    FilterKind::HueSaturation,
    FilterKind::BrightnessContrast,
    FilterKind::GaussianBlur,
    FilterKind::Sharpen,
    FilterKind::SharpenMore,
    FilterKind::Unsharp,
    FilterKind::LineWidth,
    FilterKind::Dust,
    FilterKind::Aberration,
    FilterKind::Noise,
    FilterKind::Mosaic,
    FilterKind::Crystallize,
    FilterKind::RetroFilm,
    FilterKind::Perlin,
    FilterKind::Pencil,
    FilterKind::Artistic,
    FilterKind::Average,
    FilterKind::Blur,
    FilterKind::BlurStrong,
    FilterKind::Motion,
    FilterKind::Radial,
    FilterKind::Spin,
    FilterKind::Smoothing,
    FilterKind::Lens,
    FilterKind::Distort(DistortKind::Pinch),
    FilterKind::Distort(DistortKind::Ripple),
    FilterKind::Distort(DistortKind::Fisheye),
    FilterKind::Distort(DistortKind::Polar),
    FilterKind::Distort(DistortKind::Wave),
    FilterKind::Distort(DistortKind::Twirl),
    FilterKind::Levels,
    FilterKind::Curves,
    FilterKind::Median,
];

const LAYER_ACTIONS: [LayerAction; 7] = [LayerAction::Duplicate, LayerAction::Delete, LayerAction::Clear, LayerAction::Merge, LayerAction::Transfer, LayerAction::Up, LayerAction::Down];

/// Настройки фильтра числами — в одном порядке для записи и чтения.
pub(crate) fn filter_params(f: &FilterDialog) -> Vec<f64> {
    let b = |v: bool| v as u8 as f64;
    vec![
        f.hs.hue as f64, f.hs.saturation as f64, f.hs.luminance as f64, b(f.hs.colorize),
        f.bc.brightness as f64, f.bc.contrast as f64, f.bc.depth as f64,
        f.levels.in_black as f64, f.levels.in_white as f64, f.levels.gamma, f.levels.out_black as f64, f.levels.out_white as f64,
        f.curve.points[0][0] as f64, f.curve.points[0][1] as f64, f.curve.points[1][0] as f64, f.curve.points[1][1] as f64,
        f.median_r as f64, f.radius, f.amount, f.threshold as f64, f.width_px as f64, b(f.thicken), f.dust_size as f64, f.dust_mode as f64,
        b(f.ca_radial), f.ca_px, f.ca_angle, b(f.noise_color), f.noise_amount, f.tile as f64, f.cell as f64, f.randomness, b(f.tiling),
        f.retro.preset as f64, f.retro.effect as f64, f.retro.intensity, f.retro.noise,
        f.perlin.scale, f.perlin.amplitude, f.perlin.attenuation, f.perlin.repetition as f64, f.perlin.offset[0], f.perlin.offset[1],
        f.motion.strength, f.motion.angle, f.motion.dir as f64, b(f.motion.smooth),
        f.radial.strength, f.radial.angle, f.radial.dir as f64, b(f.radial.smooth),
        f.spin_angle, f.spin_dir as f64, f.lens_radius, f.lens_sides as f64, f.lens_highlight,
        f.distort.center.0, f.distort.center.1, f.distort.radius, f.distort.amount, f.distort.amplitude, f.distort.wavelength, f.distort.angle, f.distort.shape as f64, b(f.distort.to_polar),
        f.pencil.thickness, f.pencil.darkness, f.pencil.hatching, b(f.pencil.color),
        f.artistic.radius as f64, f.artistic.levels as f64, f.artistic.edges,
    ]
}

pub(crate) fn set_filter_params(f: &mut FilterDialog, v: &[f64]) {
    let mut it = v.iter().copied().filter(|x| x.is_finite());
    macro_rules! put {
        ($e:expr, bool) => {
            if let Some(x) = it.next() {
                $e = x != 0.0;
            }
        };
        ($e:expr, $t:ty) => {
            if let Some(x) = it.next() {
                $e = x as $t;
            }
        };
    }
    put!(f.hs.hue, i32); put!(f.hs.saturation, i32); put!(f.hs.luminance, i32); put!(f.hs.colorize, bool);
    put!(f.bc.brightness, i32); put!(f.bc.contrast, i32); put!(f.bc.depth, i32);
    put!(f.levels.in_black, u8); put!(f.levels.in_white, u8); put!(f.levels.gamma, f64); put!(f.levels.out_black, u8); put!(f.levels.out_white, u8);
    put!(f.curve.points[0][0], f32); put!(f.curve.points[0][1], f32); put!(f.curve.points[1][0], f32); put!(f.curve.points[1][1], f32);
    put!(f.median_r, u32); put!(f.radius, f64); put!(f.amount, f64); put!(f.threshold, u8); put!(f.width_px, u32); put!(f.thicken, bool); put!(f.dust_size, u32); put!(f.dust_mode, u8);
    put!(f.ca_radial, bool); put!(f.ca_px, f64); put!(f.ca_angle, f64); put!(f.noise_color, bool); put!(f.noise_amount, f64); put!(f.tile, u32); put!(f.cell, u32); put!(f.randomness, f64); put!(f.tiling, bool);
    put!(f.retro.preset, u8); put!(f.retro.effect, u8); put!(f.retro.intensity, f64); put!(f.retro.noise, f64);
    put!(f.perlin.scale, f64); put!(f.perlin.amplitude, f64); put!(f.perlin.attenuation, f64); put!(f.perlin.repetition, u32); put!(f.perlin.offset[0], f64); put!(f.perlin.offset[1], f64);
    put!(f.motion.strength, f64); put!(f.motion.angle, f64); put!(f.motion.dir, u8); put!(f.motion.smooth, bool);
    put!(f.radial.strength, f64); put!(f.radial.angle, f64); put!(f.radial.dir, u8); put!(f.radial.smooth, bool);
    put!(f.spin_angle, f64); put!(f.spin_dir, u8); put!(f.lens_radius, f64); put!(f.lens_sides, u32); put!(f.lens_highlight, f64);
    put!(f.distort.center.0, f64); put!(f.distort.center.1, f64); put!(f.distort.radius, f64); put!(f.distort.amount, f64); put!(f.distort.amplitude, f64); put!(f.distort.wavelength, f64); put!(f.distort.angle, f64); put!(f.distort.shape, u8); put!(f.distort.to_polar, bool);
    put!(f.pencil.thickness, f64); put!(f.pencil.darkness, f64); put!(f.pencil.hatching, f64); put!(f.pencil.color, bool);
    put!(f.artistic.radius, u32); put!(f.artistic.levels, u32); put!(f.artistic.edges, f64);
    if let FilterKind::Distort(k) = f.kind {
        f.distort.kind = k;
    }
}

impl Step {
    pub fn to_text(&self) -> String {
        let yn = |b: bool| if b { "1" } else { "0" };
        match self {
            Step::Filter(k, v) => format!("filter {k:?} {}", v.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(" ")),
            Step::Layer(a) => format!("layer {a:?}"),
            Step::LayerTurn(cw, h) => match (cw, h) {
                (Some(true), _) => "turn cw".into(),
                (Some(false), _) => "turn ccw".into(),
                (_, Some(true)) => "turn fliph".into(),
                _ => "turn flipv".into(),
            },
            Step::Canvas(op) => format!("canvas {op}"),
            Step::Trim => "trim".into(),
            Step::Flatten => "flatten".into(),
            Step::SelectAll => "selall".into(),
            Step::Deselect => "deselect".into(),
            Step::InvertSelection => "invert".into(),
            Step::Grow(n, g) => format!("grow {n} {}", yn(*g)),
            Step::NewLayer(k) => format!("new {k}"),
            Step::Adjust(t) => format!("adjust {t}"),
            Step::Fill => "fill".into(),
        }
    }

    pub fn from_text(s: &str) -> Option<Step> {
        let (head, rest) = s.trim().split_once(' ').unwrap_or((s.trim(), ""));
        let mut w = rest.split_ascii_whitespace();
        Some(match head {
            "filter" => {
                let name = w.next()?;
                let k = *FILTERS.iter().find(|k| format!("{k:?}") == name)?;
                Step::Filter(k, w.filter_map(|x| x.parse().ok()).collect())
            }
            "layer" => Step::Layer(*LAYER_ACTIONS.iter().find(|a| format!("{a:?}") == rest.trim())?),
            "turn" => match rest.trim() {
                "cw" => Step::LayerTurn(Some(true), None),
                "ccw" => Step::LayerTurn(Some(false), None),
                "fliph" => Step::LayerTurn(None, Some(true)),
                "flipv" => Step::LayerTurn(None, Some(false)),
                _ => return None,
            },
            "canvas" => Step::Canvas(rest.trim().parse::<u8>().ok().filter(|v| *v < 4)?),
            "trim" => Step::Trim,
            "flatten" => Step::Flatten,
            "selall" => Step::SelectAll,
            "deselect" => Step::Deselect,
            "invert" => Step::InvertSelection,
            "grow" => Step::Grow(w.next()?.parse::<u32>().ok()?.clamp(1, 1000), w.next()? == "1"),
            "new" => Step::NewLayer(rest.trim().parse::<u8>().ok().filter(|v| *v < 4)?),
            "adjust" => {
                paint_core::adjust::Adjust::from_text(rest)?;
                Step::Adjust(rest.trim().to_string())
            }
            "fill" => Step::Fill,
            _ => return None,
        })
    }

    /// Подпись шага в окне.
    pub fn label(&self) -> String {
        match self {
            Step::Filter(k, _) => trf("Фильтр: {0}", &[&filter_title(*k)]),
            Step::Layer(a) => tr(match a {
                LayerAction::Duplicate => "Копировать слой",
                LayerAction::Delete => "Удалить слой",
                LayerAction::Clear => "Очистить слой",
                LayerAction::Merge => "Слить с нижним",
                LayerAction::Transfer => "Перенести на нижний",
                LayerAction::Up => "Поднять слой",
                LayerAction::Down => "Опустить слой",
            })
            .to_string(),
            Step::LayerTurn(cw, h) => tr(match (cw, h) {
                (Some(false), _) => "Повернуть слой на 90° против часовой",
                (Some(true), _) => "Повернуть слой на 90° по часовой",
                (_, Some(true)) => "Отразить слой по горизонтали",
                _ => "Отразить слой по вертикали",
            })
            .to_string(),
            Step::Canvas(op) => tr(["Отразить по горизонтали", "Отразить по вертикали", "Повернуть на 90° против часовой", "Повернуть на 90° по часовой"][(*op as usize).min(3)]).to_string(),
            Step::Trim => tr("Обрезать по выделению").to_string(),
            Step::Flatten => tr("Свести всё").to_string(),
            Step::SelectAll => tr("Выделить всё").to_string(),
            Step::Deselect => tr("Снять выделение").to_string(),
            Step::InvertSelection => tr("Инвертировать выделение").to_string(),
            Step::Grow(n, g) => trf(if *g { "Расширить выделение на {0} px" } else { "Сузить выделение на {0} px" }, &[n]),
            Step::NewLayer(k) => tr(["Новый слой", "Новая папка", "Новый слой контура", "Новый слой фигур"][(*k as usize).min(3)]).to_string(),
            Step::Adjust(t) => match paint_core::adjust::Adjust::from_text(t) {
                Some(a) => trf("Слой коррекции: {0}", &[&crate::layer_style::adjust_name(&a)]),
                None => t.clone(),
            },
            Step::Fill => tr("Залить").to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Action {
    pub name: String,
    /// Шаги с галочкой «выполнять».
    pub steps: Vec<(bool, Step)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ActionSet {
    pub name: String,
    pub actions: Vec<Action>,
}

impl ActionSet {
    pub fn to_text(&self) -> String {
        let mut s = String::from("gsact 1\n");
        for a in &self.actions {
            s += &format!("action {}\n", a.name.replace('\n', " "));
            for (on, st) in &a.steps {
                s += &format!("step {} {}\n", *on as u8, st.to_text());
            }
        }
        s
    }

    pub fn from_text(name: &str, text: &str) -> ActionSet {
        let mut set = ActionSet { name: name.to_string(), actions: Vec::new() };
        for line in text.lines() {
            if let Some(n) = line.strip_prefix("action ") {
                set.actions.push(Action { name: n.trim().to_string(), steps: Vec::new() });
            } else if let Some(r) = line.strip_prefix("step ") {
                let (on, st) = r.split_once(' ').unwrap_or(("1", r));
                if let (Some(a), Some(st)) = (set.actions.last_mut(), Step::from_text(st)) {
                    a.steps.push((on != "0", st));
                }
            }
        }
        set
    }
}

fn dir() -> Option<std::path::PathBuf> {
    Some(crate::settings::dir()?.join("actions"))
}

/// Наборы из папки настроек; без них — один пустой «Мои действия».
pub fn load() -> Vec<ActionSet> {
    let mut out: Vec<ActionSet> = dir()
        .and_then(|d| std::fs::read_dir(d).ok())
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "gsact"))
        .filter_map(|e| {
            let p = e.path();
            let name = p.file_stem()?.to_string_lossy().into_owned();
            Some(ActionSet::from_text(&name, &std::fs::read_to_string(&p).ok()?))
        })
        .collect();
    out.sort_by_key(|s| s.name.to_lowercase());
    if out.is_empty() {
        out.push(ActionSet { name: tr("Мои действия").to_string(), actions: Vec::new() });
    }
    out
}

pub fn save(set: &ActionSet) -> Result<(), String> {
    let d = dir().ok_or(tr("нет папки настроек"))?;
    std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    paint_core::io::write_atomic(&d.join(format!("{}.gsact", crate::assets::clean_name(&set.name))), set.to_text().as_bytes()).map_err(|e| e.to_string())
}

fn remove(set: &ActionSet) {
    if let Some(d) = dir() {
        std::fs::remove_file(d.join(format!("{}.gsact", crate::assets::clean_name(&set.name)))).ok();
    }
}

/// Состояние окна «Автодействия».
#[derive(Default)]
pub struct ActionsUi {
    pub sets: Vec<ActionSet>,
    /// Выбрано: набор, действие, шаг.
    pub sel: (usize, Option<usize>, Option<usize>),
    /// Идёт запись в выбранное действие.
    pub recording: bool,
    /// Идёт повтор: шаги не пишутся.
    pub playing: bool,
    pub rename: Option<String>,
    pub loaded: bool,
}

impl PaintApp {
    /// Записать шаг, если идёт запись.
    pub(crate) fn rec(&mut self, step: Step) {
        let a = &mut self.actions;
        if !a.recording || a.playing {
            return;
        }
        let (si, Some(ai), _) = a.sel else { return };
        let Some(act) = a.sets.get_mut(si).and_then(|s| s.actions.get_mut(ai)) else { return };
        act.steps.push((true, step));
        let n = act.steps.len() - 1;
        a.sel.2 = Some(n);
        let _ = save(&a.sets[si]);
    }

    /// Выполнить один шаг.
    fn run_step(&mut self, step: &Step) {
        match step {
            Step::Filter(k, v) => {
                self.open_filter(*k);
                if let Some(f) = self.filter.as_mut() {
                    set_filter_params(f, v);
                    f.dirty = true;
                }
                self.filter_job = None;
                self.finish_filter(true);
            }
            Step::Layer(a) => self.layer_action(*a),
            Step::LayerTurn(cw, h) => self.layer_turn(*cw, *h),
            Step::Canvas(op) => {
                let op = *op;
                self.canvas_op(|d| match op {
                    0 => d.flip_canvas(true),
                    1 => d.flip_canvas(false),
                    2 => d.rotate_canvas(false),
                    _ => d.rotate_canvas(true),
                });
            }
            Step::Trim => {
                if !self.doc.selection.is_empty() {
                    self.canvas_op(|d| d.trim_to_selection());
                    self.sel_dirty = true;
                }
            }
            Step::Flatten => self.canvas_op(|d| d.flatten_layers()),
            Step::SelectAll => self.select_all(),
            Step::Deselect => self.deselect(),
            Step::InvertSelection => self.invert_selection(),
            Step::Grow(n, g) => {
                self.doc.grow_selection_by(*n, *g);
                self.sel_dirty = true;
            }
            Step::NewLayer(k) => match k {
                0 => self.add_layer(false),
                1 => self.add_layer(true),
                2 => self.add_linework_layer(),
                _ => self.add_shape_layer(),
            },
            Step::Adjust(t) => {
                if let Some(a) = paint_core::adjust::Adjust::from_text(t) {
                    self.add_adjust_layer(a);
                }
            }
            Step::Fill => self.fill_selection(),
        }
    }

    /// Повторить действие с шага `from`: одна запись истории на всё действие.
    pub(crate) fn play_action(&mut self, si: usize, ai: usize, from: usize) {
        let Some(act) = self.actions.sets.get(si).and_then(|s| s.actions.get(ai)).cloned() else { return };
        self.actions.playing = true;
        let mark = self.doc.history_mark();
        for (on, st) in act.steps.iter().skip(from) {
            if *on {
                self.run_step(st);
            }
        }
        self.doc.squash_since(mark);
        self.actions.playing = false;
        self.refresh_all();
        self.thumbs.clear();
        self.dirty = true;
        self.status = trf("Выполнено действие «{0}»", &[&act.name]);
    }

    pub(crate) fn actions_window(&mut self, ctx: &egui::Context) {
        if !self.show_actions {
            return;
        }
        if !self.actions.loaded {
            self.actions.sets = load();
            self.actions.loaded = true;
        }
        let mut open = true;
        let mut play: Option<(usize, usize, usize)> = None;
        egui::Window::new(tr("Автодействия")).id(egui::Id::new("auto_actions")).open(&mut open).default_width(300.0).resizable(true).show(ctx, |ui| {
            let a = &mut self.actions;
            // набор
            ui.horizontal(|ui| {
                let cur = a.sets.get(a.sel.0).map(|s| s.name.clone()).unwrap_or_default();
                egui::ComboBox::from_id_salt("action_set").selected_text(cur).width(150.0).show_ui(ui, |ui| {
                    for (i, s) in a.sets.iter().enumerate() {
                        if ui.selectable_label(i == a.sel.0, &s.name).clicked() {
                            a.sel = (i, None, None);
                            a.recording = false;
                        }
                    }
                });
                if ui.button(tr("Новый набор")).clicked() {
                    let mut n = a.sets.len() + 1;
                    while a.sets.iter().any(|s| s.name == trf("Набор {0}", &[&n])) {
                        n += 1;
                    }
                    a.sets.push(ActionSet { name: trf("Набор {0}", &[&n]), actions: Vec::new() });
                    a.sel = (a.sets.len() - 1, None, None);
                    let _ = save(&a.sets[a.sel.0]);
                }
            });
            let si = a.sel.0.min(a.sets.len().saturating_sub(1));
            ui.separator();
            egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                let Some(set) = a.sets.get_mut(si) else { return };
                let mut changed = false;
                for (ai, act) in set.actions.iter_mut().enumerate() {
                    let selected = a.sel.1 == Some(ai);
                    let rec = selected && a.recording;
                    let title = if rec { format!("● {}", act.name) } else { act.name.clone() };
                    let r = ui.selectable_label(selected && a.sel.2.is_none(), egui::RichText::new(title).strong());
                    if r.clicked() {
                        a.sel = (si, Some(ai), None);
                    }
                    if r.double_clicked() {
                        play = Some((si, ai, 0));
                    }
                    if selected {
                        ui.indent(("steps", ai), |ui| {
                            for (k, (on, st)) in act.steps.iter_mut().enumerate() {
                                ui.horizontal(|ui| {
                                    changed |= ui.checkbox(on, "").changed();
                                    if ui.selectable_label(a.sel.2 == Some(k), st.label()).clicked() {
                                        a.sel.2 = Some(k);
                                    }
                                });
                            }
                            if act.steps.is_empty() {
                                ui.label(egui::RichText::new(tr("Пусто: нажмите «Запись» и выполните команды меню")).small());
                            }
                        });
                    }
                }
                if changed {
                    let _ = save(set);
                }
            });
            ui.separator();
            let has_act = a.sel.1.is_some();
            let mut delete_set = false;
            let n_sets = a.sets.len();
            ui.horizontal_wrapped(|ui| {
                if ui.button(tr("Новое действие")).clicked() {
                    if let Some(set) = a.sets.get_mut(si) {
                        let n = set.actions.len() + 1;
                        set.actions.push(Action { name: trf("Действие {0}", &[&n]), steps: Vec::new() });
                        a.sel = (si, Some(set.actions.len() - 1), None);
                        a.recording = true;
                        let _ = save(set);
                    }
                }
                let rec_label = if a.recording { tr("Стоп") } else { tr("Запись") };
                if ui.add_enabled(has_act, egui::Button::new(rec_label)).clicked() {
                    a.recording = !a.recording;
                }
                if ui.add_enabled(has_act && !a.recording, egui::Button::new(tr("Пуск"))).on_hover_text(tr("С выбранного шага или с начала; отменяется одним шагом")).clicked() {
                    if let (Some(ai), from) = (a.sel.1, a.sel.2.unwrap_or(0)) {
                        play = Some((si, ai, from));
                    }
                }
            });
            ui.horizontal_wrapped(|ui| {
                let Some(set) = a.sets.get_mut(si) else { return };
                let mut changed = false;
                if let Some(ai) = a.sel.1.filter(|&i| i < set.actions.len()) {
                    match a.sel.2.filter(|&k| k < set.actions[ai].steps.len()) {
                        Some(k) => {
                            let steps = &mut set.actions[ai].steps;
                            if ui.add_enabled(k > 0, egui::Button::new("▲")).clicked() {
                                steps.swap(k, k - 1);
                                a.sel.2 = Some(k - 1);
                                changed = true;
                            }
                            if ui.add_enabled(k + 1 < steps.len(), egui::Button::new("▼")).clicked() {
                                steps.swap(k, k + 1);
                                a.sel.2 = Some(k + 1);
                                changed = true;
                            }
                            if ui.button(tr("Копировать шаг")).clicked() {
                                let s = steps[k].clone();
                                steps.insert(k + 1, s);
                                a.sel.2 = Some(k + 1);
                                changed = true;
                            }
                            if ui.button(tr("Удалить шаг")).clicked() {
                                steps.remove(k);
                                a.sel.2 = None;
                                changed = true;
                            }
                        }
                        None => {
                            if ui.button(tr("Копировать действие")).clicked() {
                                let mut c = set.actions[ai].clone();
                                c.name = trf("{0} (копия)", &[&c.name]);
                                set.actions.insert(ai + 1, c);
                                a.sel.1 = Some(ai + 1);
                                changed = true;
                            }
                            if ui.button(tr("Переименовать")).clicked() {
                                a.rename = Some(set.actions[ai].name.clone());
                            }
                            if ui.button(tr("Удалить действие")).clicked() {
                                set.actions.remove(ai);
                                a.sel = (si, None, None);
                                a.recording = false;
                                changed = true;
                            }
                        }
                    }
                } else if ui.add_enabled(n_sets > 1, egui::Button::new(tr("Удалить набор"))).clicked() {
                    delete_set = true;
                }
                if changed {
                    let _ = save(set);
                }
            });
            if delete_set {
                remove(&a.sets[si]);
                a.sets.remove(si);
                a.sel = (0, None, None);
            }
            if let Some(name) = a.rename.as_mut() {
                let mut done = false;
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(name);
                    done = ui.button("OK").clicked();
                });
                if done {
                    let n = name.trim().to_string();
                    a.rename = None;
                    if let (Some(set), Some(ai)) = (a.sets.get_mut(si), a.sel.1) {
                        if let Some(act) = set.actions.get_mut(ai).filter(|_| !n.is_empty()) {
                            act.name = n;
                            let _ = save(set);
                        }
                    }
                }
            }
        });
        if !open {
            self.show_actions = false;
            self.actions.recording = false;
        }
        if let Some((si, ai, from)) = play {
            self.play_action(si, ai, from);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_round_trip() {
        let steps = vec![
            Step::Filter(FilterKind::Distort(DistortKind::Wave), vec![1.0, 2.5, -3.0]),
            Step::Filter(FilterKind::GaussianBlur, vec![]),
            Step::Layer(LayerAction::Duplicate),
            Step::LayerTurn(Some(true), None),
            Step::LayerTurn(None, Some(false)),
            Step::Canvas(3),
            Step::Trim,
            Step::Flatten,
            Step::SelectAll,
            Step::Deselect,
            Step::InvertSelection,
            Step::Grow(5, false),
            Step::NewLayer(2),
            Step::Adjust("gradient 0 0 0 0 1 255 255 255".into()),
            Step::Fill,
        ];
        for s in &steps {
            assert_eq!(Step::from_text(&s.to_text()).as_ref(), Some(s), "{}", s.to_text());
        }
        let set = ActionSet { name: "н".into(), actions: vec![Action { name: "А б".into(), steps: steps.into_iter().map(|s| (true, s)).collect() }] };
        assert_eq!(ActionSet::from_text("н", &set.to_text()), set);
        assert_eq!(Step::from_text("canvas 9"), None);
        assert_eq!(Step::from_text("что-то"), None);
    }
}
