//! GrazySAI Studio — мастерская кистей (а позже и плагинов). Отдельная программа, чтобы
//! рисовалка не росла: редактор кисти с полосой пробы, генератор отпечатка, сохранение в
//! `.gsbrush` и «Экспорт в GrazySAI» — кисть сразу появляется в открытой рисовалке
//! (папка настроек, `brushes/dev/`, отметка `.changed`; docs/34-brushes-and-plugins.md).

#![cfg_attr(windows, windows_subsystem = "windows")]

#[allow(dead_code)]
#[path = "../../paint-app/src/i18n.rs"]
mod i18n;
#[allow(dead_code)]
#[path = "../../paint-app/src/sai_ui.rs"]
mod sai_ui;
#[allow(dead_code)]
#[path = "../../paint-app/src/files.rs"]
mod files;
#[allow(dead_code)]
#[path = "../../paint-app/src/settings.rs"]
mod settings;
mod plugin_tab;

use eframe::egui::{self, Color32, Pos2, Rect, Sense, Vec2};
use i18n::{tr, trf};
use paint_brush::{Brush, TexChoice, ToolKind};
use paint_core::dynamics::{DynInput, DynOutput, Dynamic};
use paint_core::{InputPoint, Layer, Stroke};
use std::path::PathBuf;

/// Полоса пробы: размер холста.
const PAD_W: u32 = 960;
const PAD_H: u32 = 300;

struct Studio {
    brush: Brush,
    /// Файл кисти, если открыт или сохранён.
    path: Option<PathBuf>,
    /// Рисунок на полосе пробы (без текущего мазка) и холст для показа.
    base: Layer,
    pad: paint_core::Document,
    tex: Option<egui::TextureHandle>,
    redraw: bool,
    /// Мазок, который сейчас рисуют мышью или пером.
    live: Option<(Stroke, Vec<InputPoint>)>,
    /// Нажим мыши (у мыши его нет; перо Windows Ink даёт свой).
    mouse_pressure: f32,
    /// Образец мазка пересчитывается при каждой правке.
    sample: bool,
    color: [u8; 3],
    status: String,
    dialog: Option<(files::Pending, Want)>,
    tip_names: Option<Vec<String>>,
    tex_names: Option<Vec<String>>,
    /// Генератор отпечатка: вид и число брызг.
    gen_dots: u32,
    /// Вкладка: кисть или плагин.
    tab: Tab,
    plugin: plugin_tab::PluginTab,
    /// Список своей динамики из плагинов (читается с диска не чаще раза в 2 с).
    dyn_cache: Option<(std::time::Instant, Vec<paint_plugin::Plugin>)>,
}

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Brush,
    Plugin,
}

/// Что ждём от окна выбора файла.
#[derive(Clone, Copy, PartialEq)]
pub enum Want {
    Open,
    Save,
    Tip,
    OpenPlugin,
    SavePlugin,
    AddWasm,
}

impl Studio {
    fn new() -> Self {
        let mut brush = ToolKind::Pencil.new_brush();
        brush.params.size = 16.0;
        brush.custom = Some(tr("Новая кисть").to_string());
        let pad = blank_pad();
        Self {
            brush,
            path: None,
            base: pad.layers[0].clone(),
            pad,
            tex: None,
            redraw: true,
            live: None,
            mouse_pressure: 0.7,
            sample: true,
            color: [25, 25, 35],
            status: tr("Правьте кисть слева и рисуйте на полосе пробы; «Экспорт в GrazySAI» — кисть сразу появится в рисовалке").to_string(),
            dialog: None,
            tip_names: None,
            tex_names: None,
            gen_dots: 24,
            tab: Tab::Brush,
            plugin: plugin_tab::PluginTab::new(),
            dyn_cache: None,
        }
    }

    /// Своя динамика из плагинов: плагин из вкладки «Плагин» (если в нём есть динамика) и
    /// установленные в GrazySAI. Ключ — «id/имя».
    fn plugin_dynamics(&mut self) -> Vec<(String, String, paint_plugin::Plugin)> {
        // установленные — из кеша (не чаще раза в 2 с), плагин из редактора — всегда свежий
        let fresh = self.dyn_cache.as_ref().is_some_and(|(t, _)| t.elapsed() < std::time::Duration::from_secs(2));
        if !fresh {
            self.dyn_cache = Some((std::time::Instant::now(), installed_plugins()));
        }
        let mut list: Vec<paint_plugin::Plugin> = self.plugin.current().filter(|p| !p.dynamics.is_empty()).cloned().into_iter().collect();
        for p in self.dyn_cache.as_ref().map(|(_, v)| v.clone()).unwrap_or_default() {
            if !list.iter().any(|q| q.id == p.id) {
                list.push(p);
            }
        }
        list.into_iter().flat_map(|p| p.dynamics.clone().into_iter().map(move |(n, _)| (format!("{}/{n}", p.id), format!("{} — {n}", p.name), p.clone()))).collect()
    }

    /// Мазок кистью с её динамикой из плагина.
    fn stroke(&mut self) -> Stroke {
        let mut st = self.brush.stroke(None);
        if let Some(spec) = self.brush.dyn_plugin.clone() {
            if let Some((_, _, p)) = self.plugin_dynamics().into_iter().find(|(k, _, _)| *k == spec) {
                match paint_plugin::dynamics_hook(&p, spec.split_once('/').map_or("", |s| s.1)) {
                    Ok((h, _)) => st.dyn_hook = Some(h),
                    Err(e) => self.status = e,
                }
            }
        }
        st
    }

    fn name(&self) -> String {
        self.brush.custom.clone().unwrap_or_else(|| tr(self.brush.kind.label()).to_string())
    }

    /// Полоса пробы: образец-волна сверху (по желанию) и свои мазки.
    fn render(&mut self, ctx: &egui::Context) {
        self.pad.layers[0] = self.base.clone();
        if self.sample {
            let mut st = self.stroke();
            st.params.erase = false;
            let (w, h) = (PAD_W as f32, 110.0);
            for k in 0..=90 {
                let t = k as f32 / 90.0;
                let pressure = (t * std::f32::consts::PI).sin().max(0.05);
                st.add_point(InputPoint { x: w * 0.05 + t * w * 0.9, y: h / 2.0 + 8.0 + (t * std::f32::consts::TAU * 1.5).sin() * h * 0.28, pressure });
            }
            self.pad.commit_stroke(0, &st, self.color);
        }
        if let Some((st, _)) = &self.live {
            self.pad.commit_stroke(0, st, self.color);
        }
        let rgba = self.pad.flatten();
        let img = egui::ColorImage::from_rgba_unmultiplied([PAD_W as usize, PAD_H as usize], &rgba);
        match &mut self.tex {
            Some(t) => t.set(img, egui::TextureOptions::NEAREST),
            None => self.tex = Some(ctx.load_texture("pad", img, egui::TextureOptions::NEAREST)),
        }
        self.redraw = false;
    }

    fn save_to(&mut self, path: PathBuf) {
        let path = files::with_ext(path, "gsbrush");
        let name = self.name();
        self.status = match paint_brush::save(&path, &[(&self.brush, name)]) {
            Ok(()) => {
                self.path = Some(path.clone());
                trf("Сохранено: {0}", &[&path.display()])
            }
            Err(e) => trf("Не удалось сохранить {0}: {1}", &[&path.display(), &e]),
        };
    }

    /// В папку рисовалки `brushes/dev/` и отметка `.changed`: открытая рисовалка подхватит кисть
    /// в течение секунды.
    fn export(&mut self) {
        let Some(dir) = settings::dir().map(|d| d.join("brushes").join("dev")) else {
            self.status = tr("Папка настроек GrazySAI не найдена").to_string();
            return;
        };
        let name = self.name();
        let path = dir.join(format!("{}.gsbrush", paint_brush::safe_name(&name)));
        let r = std::fs::create_dir_all(&dir).map_err(Into::into).and_then(|_| paint_brush::save(&path, &[(&self.brush, name.clone())]));
        self.status = match r.and_then(|_| Ok(std::fs::write(dir.join(".changed"), format!("{:?}", std::time::SystemTime::now()))?)) {
            Ok(()) => trf("Кисть «{0}» отправлена в GrazySAI", &[&name]),
            Err(e) => trf("Не удалось сохранить {0}: {1}", &[&path.display(), &e]),
        };
    }

    fn open(&mut self, path: PathBuf) {
        let r = paint_formats::pack::read(&path).map(|f| paint_brush::from_files(&f, tr("Кисть")));
        match r {
            Ok(mut list) if !list.is_empty() => {
                self.brush = list.remove(0);
                self.path = Some(path);
                self.tip_names = None;
                self.redraw = true;
                self.status = trf("Открыта кисть «{0}»", &[&self.name()]);
            }
            Ok(_) => self.status = tr("В файле нет кистей GrazySAI").to_string(),
            Err(e) => self.status = trf("Кисть не открылась: {0}", &[&e]),
        }
    }

    /// Картинка или кисть GIMP — отпечаток этой кисти.
    fn add_tip(&mut self, path: PathBuf) {
        match paint_formats::brush_tip::load_all(&path).and_then(|(name, cells, random)| Ok((paint_core::paper::save_tips(&name, &cells)?, random))) {
            Ok((name, random)) => {
                self.brush.grain.tip = Some(name);
                self.brush.grain.tip_random = random;
                self.tip_names = None;
                self.redraw = true;
            }
            Err(e) => self.status = trf("Картинка не открылась: {0}", &[&e]),
        }
    }

    /// Отпечаток из генератора: сохраняется в папку отпечатков и выбирается.
    fn generate(&mut self, kind: &str) {
        let stamp = match kind {
            "square" => paint_core::Stamp { width: 64, height: 64, alpha: vec![255; 64 * 64] },
            "spray" => spray(self.gen_dots),
            "chalk" => chalk(),
            _ => paint_core::Stamp::disc(self.brush.params.hardness),
        };
        match paint_core::paper::save_tip(&format!("Studio {kind}"), &stamp) {
            Ok(name) => {
                self.brush.grain.tip = Some(name);
                self.tip_names = None;
                self.redraw = true;
            }
            Err(e) => self.status = e.to_string(),
        }
    }

    fn ask(&mut self, ctx: &egui::Context, want: Want) {
        let suggested = match want {
            Want::OpenPlugin | Want::SavePlugin | Want::AddWasm => self.plugin.path.clone().unwrap_or_else(|| std::env::current_dir().unwrap_or_default().join("plugin.gsplugin")),
            _ => self.path.clone().unwrap_or_else(|| std::env::current_dir().unwrap_or_default().join(format!("{}.gsbrush", paint_brush::safe_name(&self.name())))),
        };
        if !files::native() {
            self.status = tr("Окно выбора файла недоступно; перетащите файл в окно").to_string();
            return;
        }
        let ask = match want {
            Want::Open | Want::OpenPlugin => files::Ask::Open,
            Want::Save => files::Ask::SaveBrush(0),
            Want::Tip => files::Ask::Tip,
            Want::SavePlugin => files::Ask::SavePlugin,
            Want::AddWasm => files::Ask::OpenWasm,
        };
        self.dialog = Some((files::ask(ask, &suggested, ctx), want));
    }

    fn poll_dialog(&mut self) {
        let Some(answer) = self.dialog.as_ref().and_then(|(d, _)| d.poll()) else { return };
        let Some((_, want)) = self.dialog.take() else { return };
        if let files::Answer::Path(p) = answer {
            match want {
                Want::Open => self.open(p),
                Want::Save => self.save_to(p),
                Want::Tip => self.add_tip(p),
                Want::OpenPlugin => self.plugin.open(p),
                Want::SavePlugin => self.plugin.save_to(p),
                Want::AddWasm => self.plugin.add_wasm(p),
            }
        }
    }

    fn settings_ui(&mut self, ui: &mut egui::Ui) {
        let plugin_dyns: Vec<(String, String)> = self.plugin_dynamics().into_iter().map(|(k, l, _)| (k, l)).collect();
        let b = &mut self.brush;
        let mut changed = false;
        egui::CollapsingHeader::new(tr("Кисть")).default_open(true).show(ui, |ui| {
            let mut name = b.custom.clone().unwrap_or_default();
            ui.horizontal(|ui| {
                ui.label(tr("Имя"));
                if ui.text_edit_singleline(&mut name).changed() {
                    b.custom = (!name.trim().is_empty()).then(|| name.clone());
                }
            });
            ui.horizontal(|ui| {
                ui.label(tr("Вторая строка"));
                ui.text_edit_singleline(&mut b.extra);
            });
            ui.horizontal(|ui| {
                ui.label(tr("Вид"));
                egui::ComboBox::from_id_salt("kind").selected_text(tr(b.kind.label())).show_ui(ui, |ui| {
                    for k in [ToolKind::Pencil, ToolKind::AirBrush, ToolKind::Brush, ToolKind::Water, ToolKind::Marker, ToolKind::Eraser, ToolKind::Binary] {
                        if ui.selectable_label(b.kind == k, tr(k.label())).clicked() && b.kind != k {
                            // вид задаёт поведение (смешивание и т. п.): параметры — как у нового инструмента
                            let keep = (b.custom.clone(), b.extra.clone(), b.grain.clone(), b.dynamics.clone(), b.params.size);
                            *b = k.new_brush();
                            (b.custom, b.extra, b.grain, b.dynamics) = (keep.0, keep.1, keep.2, keep.3);
                            b.params.size = keep.4;
                            changed = true;
                        }
                    }
                });
            });
        });
        egui::CollapsingHeader::new(tr("Размер и плотность")).default_open(true).show(ui, |ui| {
            let p = &mut b.params;
            changed |= ui.add(egui::Slider::new(&mut p.size, 0.5..=500.0).logarithmic(true).text(tr("Размер"))).changed();
            changed |= ui.checkbox(&mut p.size_by_pressure, tr("Нажим на размер")).changed();
            changed |= ui.add_enabled(p.size_by_pressure, egui::Slider::new(&mut p.min_size, 0.0..=1.0).text(tr("Минимальный"))).changed();
            changed |= ui.add(egui::Slider::new(&mut p.density, 0.0..=1.0).text(tr("Плотность"))).changed();
            changed |= ui.checkbox(&mut p.density_by_pressure, tr("Нажим на плотность")).changed();
            changed |= ui.add_enabled(p.density_by_pressure, egui::Slider::new(&mut p.min_density, 0.0..=1.0).text(tr("Мин. плотн."))).changed();
            ui.horizontal(|ui| {
                ui.label(tr("Жесткость"));
                for (i, step) in paint_core::brush::HARDNESS_STEPS.iter().enumerate() {
                    if ui.selectable_label((p.hardness - step).abs() < 0.01, format!("{}", i + 1)).clicked() {
                        p.hardness = *step;
                        changed = true;
                    }
                }
            });
            changed |= ui.add(egui::Slider::new(&mut p.spacing, 0.0..=3.0).text(tr("Шаг (0 — как в SAI)"))).changed();
            if let Some(m) = p.mix.as_mut() {
                ui.separator();
                changed |= ui.add(egui::Slider::new(&mut m.blending, 0.0..=1.0).text(tr("Смешивание"))).changed();
                changed |= ui.add(egui::Slider::new(&mut m.dilution, 0.0..=1.0).text(tr("Разбавление"))).changed();
                changed |= ui.add(egui::Slider::new(&mut m.persistence, 0.0..=1.0).text(tr("Стойкость"))).changed();
            }
        });
        let tip_names = self.tip_names.get_or_insert_with(paint_core::paper::tip_names).clone();
        let mut want_tip = false;
        let mut gen: Option<&str> = None;
        egui::CollapsingHeader::new(tr("Отпечаток")).default_open(true).show(ui, |ui| {
            let g = &mut b.grain;
            let round = tr("[ Круг ]");
            egui::ComboBox::from_id_salt("tip").selected_text(g.tip.clone().unwrap_or_else(|| round.to_string())).show_ui(ui, |ui| {
                if ui.selectable_label(g.tip.is_none(), round).clicked() {
                    g.tip = None;
                    changed = true;
                }
                for n in &tip_names {
                    if ui.selectable_label(g.tip.as_deref() == Some(n), n).clicked() {
                        g.tip = Some(n.clone());
                        changed = true;
                    }
                }
            });
            ui.horizontal(|ui| {
                want_tip = ui.button(tr("Добавить картинку…")).clicked();
            });
            ui.horizontal_wrapped(|ui| {
                ui.label(tr("Создать:"));
                for (k, label) in [("disc", tr("круг")), ("square", tr("квадрат")), ("spray", tr("брызги")), ("chalk", tr("мел"))] {
                    if ui.button(label).clicked() {
                        gen = Some(k);
                    }
                }
            });
            ui.add(egui::Slider::new(&mut self.gen_dots, 4..=200).text(tr("брызг")));
            changed |= ui.add(egui::Slider::new(&mut g.tip_angle, -180.0..=180.0).suffix("°").text(tr("Поворот"))).changed();
            changed |= ui.add(egui::Slider::new(&mut g.tip_ratio, 0.01..=1.0).text(tr("Толщина"))).changed();
            changed |= ui.checkbox(&mut g.tip_follow, tr("по мазку")).changed();
            changed |= ui.checkbox(&mut g.tip_random, tr("вразнобой")).changed();
        });
        let tex_names = self.tex_names.get_or_insert_with(|| paint_core::paper::names_of("brushtex")).clone();
        egui::CollapsingHeader::new(tr("Текстура")).show(ui, |ui| {
            let g = &mut b.grain;
            let none = tr("[ Нет текстуры ]");
            let shown = match &g.tex {
                TexChoice::File(n) => n.clone(),
                TexChoice::Paper => tr("[ Бумага слоя ]").to_string(),
                TexChoice::None => none.to_string(),
            };
            egui::ComboBox::from_id_salt("tex").selected_text(shown).show_ui(ui, |ui| {
                if ui.selectable_label(g.tex == TexChoice::None, none).clicked() {
                    g.tex = TexChoice::None;
                    changed = true;
                }
                for n in &tex_names {
                    if ui.selectable_label(g.tex == TexChoice::File(n.clone()), n).clicked() {
                        g.tex = TexChoice::File(n.clone());
                        changed = true;
                    }
                }
            });
            changed |= ui.add(egui::Slider::new(&mut g.tex_strength, 0.0..=1.0).text(tr("Сила"))).changed();
            let mut sc = g.tex_scale as f32;
            if ui.add(egui::Slider::new(&mut sc, 10.0..=500.0).suffix(" %").text(tr("Масштаб"))).changed() {
                g.tex_scale = sc as u16;
                changed = true;
            }
        });
        egui::CollapsingHeader::new(tr("Динамика")).default_open(true).show(ui, |ui| {
            let input_name = |i: DynInput| match i {
                DynInput::Pressure => tr("Нажим"),
                DynInput::Speed => tr("Скорость"),
                DynInput::Direction => tr("Направление"),
                DynInput::Distance => tr("Путь"),
                DynInput::Random => tr("Случайно"),
            };
            let output_name = |o: DynOutput| match o {
                DynOutput::Size => tr("Размер"),
                DynOutput::Density => tr("Плотность"),
                DynOutput::Angle => tr("Поворот"),
                DynOutput::Ratio => tr("Толщина"),
                DynOutput::Scatter => tr("Разброс"),
            };
            let mut remove = None;
            for (n, d) in b.dynamics.iter_mut().enumerate() {
                ui.push_id(("dyn", n), |ui| {
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt("in").selected_text(input_name(d.input)).show_ui(ui, |ui| {
                            for i in DynInput::ALL {
                                if ui.selectable_label(d.input == i, input_name(i)).clicked() && d.input != i {
                                    d.input = i;
                                    d.scale = i.default_scale();
                                    changed = true;
                                }
                            }
                        });
                        ui.label("→");
                        egui::ComboBox::from_id_salt("out").selected_text(output_name(d.output)).show_ui(ui, |ui| {
                            for o in DynOutput::ALL {
                                changed |= ui.selectable_value(&mut d.output, o, output_name(o)).changed();
                            }
                        });
                        if ui.small_button("×").on_hover_text(tr("Удалить связь")).clicked() {
                            remove = Some(n);
                        }
                    });
                    if matches!(d.input, DynInput::Speed | DynInput::Distance) {
                        changed |= ui.add(egui::Slider::new(&mut d.scale, 1.0..=5000.0).logarithmic(true).suffix(" px").text(tr("Масштаб"))).changed();
                    }
                    changed |= sai_ui::points_editor(ui, &mut d.curve, ui.available_width() - 12.0, 70.0);
                    ui.add_space(6.0);
                });
            }
            if let Some(n) = remove {
                b.dynamics.remove(n);
                changed = true;
            }
            if b.dynamics.len() < 32 && ui.button(tr("+ Связь «вход → кривая → параметр»")).clicked() {
                b.dynamics.push(Dynamic::new(DynInput::Pressure, DynOutput::Size));
                changed = true;
            }
            ui.horizontal(|ui| {
                ui.label(tr("Из плагина"));
                let none = tr("[ Нет ]");
                let shown = b.dyn_plugin.as_ref().map(|s| plugin_dyns.iter().find(|(k, _)| k == s).map_or_else(|| s.clone(), |(_, l)| l.clone())).unwrap_or_else(|| none.to_string());
                egui::ComboBox::from_id_salt("dynplug").selected_text(shown).show_ui(ui, |ui| {
                    if ui.selectable_label(b.dyn_plugin.is_none(), none).clicked() {
                        b.dyn_plugin = None;
                        changed = true;
                    }
                    for (k, l) in &plugin_dyns {
                        if ui.selectable_label(b.dyn_plugin.as_deref() == Some(k), l).clicked() {
                            b.dyn_plugin = Some(k.clone());
                            changed = true;
                        }
                    }
                });
            });
        });
        if want_tip {
            if let Some(ctx) = Some(ui.ctx().clone()) {
                self.ask(&ctx, Want::Tip);
            }
        }
        if let Some(k) = gen {
            self.generate(k);
        }
        if changed {
            self.redraw = true;
        }
    }

    /// Полоса пробы: рисование мышью или пером.
    fn pad_ui(&mut self, ui: &mut egui::Ui) {
        let avail = ui.available_size();
        let k = (avail.x / PAD_W as f32).min((avail.y - 8.0) / PAD_H as f32).clamp(0.2, 2.0);
        let size = Vec2::new(PAD_W as f32 * k, PAD_H as f32 * k);
        let (rect, resp) = ui.allocate_exact_size(size, Sense::drag());
        if let Some(t) = &self.tex {
            ui.painter().image(t.id(), rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        }
        ui.painter().rect_stroke(rect, 0.0, egui::Stroke::new(1.0_f32, Color32::from_gray(150)));
        // нажим: у пера (касание с силой) — его, у мыши — из ползунка
        let force = ui.input(|i| i.events.iter().rev().find_map(|e| if let egui::Event::Touch { force, .. } = e { *force } else { None }));
        let pressure = force.unwrap_or(self.mouse_pressure).clamp(0.0, 1.0);
        let to_pad = |p: Pos2| InputPoint { x: (p.x - rect.left()) / k, y: (p.y - rect.top()) / k, pressure };
        if resp.drag_started() {
            if let Some(p) = resp.interact_pointer_pos() {
                let mut st = self.stroke();
                let q = to_pad(p);
                st.add_point(q);
                self.live = Some((st, vec![q]));
                self.redraw = true;
            }
        } else if resp.dragged() {
            if let (Some(p), Some((st, pts))) = (resp.interact_pointer_pos(), self.live.as_mut()) {
                let q = to_pad(p);
                st.add_point(q);
                pts.push(q);
                self.redraw = true;
            }
        }
        if resp.drag_stopped() {
            if let Some((st, _)) = self.live.take() {
                // мазок остаётся на полосе
                self.pad.layers[0] = self.base.clone();
                self.pad.commit_stroke(0, &st, self.color);
                self.base = self.pad.layers[0].clone();
                self.redraw = true;
            }
        }
    }
}

impl eframe::App for Studio {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_dialog();
        if let Some(p) = ctx.input(|i| i.raw.dropped_files.iter().find_map(|f| f.path.clone())) {
            let ext = p.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).unwrap_or_default();
            if ext == "gsbrush" || ext == "gsbrushes" {
                self.open(p);
            } else if ext == "gsplugin" {
                self.tab = Tab::Plugin;
                self.plugin.open(p);
            } else if ext == "wasm" {
                self.tab = Tab::Plugin;
                self.plugin.add_wasm(p);
            } else {
                self.add_tip(p);
            }
        }
        egui::TopBottomPanel::top("tabs").show(ctx, |ui| {
            ui.horizontal(|ui| {
                // плагин из редактора мог измениться — образец мазка пересчитать
                if ui.selectable_value(&mut self.tab, Tab::Brush, egui::RichText::new(tr("Кисть")).heading()).changed() {
                    self.redraw = true;
                }
                ui.selectable_value(&mut self.tab, Tab::Plugin, egui::RichText::new(tr("Плагин")).heading());
            });
        });
        if self.tab == Tab::Plugin {
            egui::TopBottomPanel::top("pbar").show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if let Some(w) = self.plugin.bar(ui) {
                        self.ask(ctx, w);
                    }
                });
            });
            egui::TopBottomPanel::bottom("pstatus").show(ctx, |ui| {
                ui.label(if self.plugin.status.is_empty() { &self.status } else { &self.plugin.status });
            });
            self.plugin.ui(ctx);
            if std::mem::take(&mut self.plugin.want_wasm) {
                self.ask(ctx, Want::AddWasm);
            }
            return;
        }
        egui::TopBottomPanel::top("bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.menu_button(tr("Новая"), |ui| {
                    for k in [ToolKind::Pencil, ToolKind::AirBrush, ToolKind::Brush, ToolKind::Water, ToolKind::Marker, ToolKind::Eraser] {
                        if ui.button(tr(k.label())).clicked() {
                            self.brush = k.new_brush();
                            self.brush.custom = Some(tr("Новая кисть").to_string());
                            self.path = None;
                            self.redraw = true;
                            ui.close_menu();
                        }
                    }
                });
                if ui.button(tr("Открыть…")).clicked() {
                    self.ask(ctx, Want::Open);
                }
                if ui.button(tr("Сохранить")).clicked() {
                    match self.path.clone() {
                        Some(p) => self.save_to(p),
                        None => self.ask(ctx, Want::Save),
                    }
                }
                if ui.button(tr("Сохранить как…")).clicked() {
                    self.ask(ctx, Want::Save);
                }
                ui.separator();
                if ui.button(egui::RichText::new(tr("Экспорт в GrazySAI")).strong()).on_hover_text(tr("Кисть сразу появится в открытой рисовалке (папка brushes/dev)")).clicked() {
                    self.export();
                }
                ui.separator();
                egui::ComboBox::from_id_salt("lang").selected_text(i18n::LANGS[i18n::current()].1).show_ui(ui, |ui| {
                    for (code, name, _) in i18n::LANGS.iter() {
                        if ui.selectable_label(i18n::code() == *code, *name).clicked() {
                            i18n::set(code);
                        }
                    }
                });
            });
        });
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.label(&self.status);
        });
        egui::SidePanel::left("settings").default_width(340.0).show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| self.settings_ui(ui));
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.checkbox(&mut self.sample, tr("Образец мазка")).changed() {
                    self.redraw = true;
                }
                ui.add(egui::Slider::new(&mut self.mouse_pressure, 0.05..=1.0).text(tr("Нажим мыши")));
                let mut c = Color32::from_rgb(self.color[0], self.color[1], self.color[2]);
                if ui.color_edit_button_srgba(&mut c).changed() {
                    self.color = [c.r(), c.g(), c.b()];
                    self.redraw = true;
                }
                if ui.button(tr("Очистить")).clicked() {
                    self.base = blank_pad().layers[0].clone();
                    self.redraw = true;
                }
            });
            self.pad_ui(ui);
        });
        if self.redraw {
            self.render(ctx);
        }
    }
}

/// Плагины, установленные в GrazySAI (`plugins/dev/` — поверх `plugins/`).
fn installed_plugins() -> Vec<paint_plugin::Plugin> {
    let mut list: Vec<paint_plugin::Plugin> = Vec::new();
    if let Some(d) = settings::dir().map(|d| d.join("plugins")) {
        for dir in [d.join("dev"), d] {
            for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                if let Ok(p) = paint_plugin::load(&e.path()) {
                    if !list.iter().any(|q| q.id == p.id) {
                        list.push(p);
                    }
                }
            }
        }
    }
    list
}

/// Пустая полоса пробы: белый слой.
fn blank_pad() -> paint_core::Document {
    let mut d = paint_core::Document::new(PAD_W, PAD_H);
    for y in 0..PAD_H as i32 {
        for x in 0..PAD_W as i32 {
            d.layers[0].pixels.set(x, y, paint_core::pixel::from_rgba8([255, 255, 255, 255]));
        }
    }
    d
}

/// Брызги: `n` кругов разного размера в круге.
fn spray(n: u32) -> paint_core::Stamp {
    let s = 128usize;
    let mut alpha = vec![0u8; s * s];
    for i in 0..n as u64 {
        let r = paint_core::dynamics::hash01(i * 3).sqrt() * 54.0;
        let a = paint_core::dynamics::hash01(i * 3 + 1) * std::f32::consts::TAU;
        let rad = 2.0 + paint_core::dynamics::hash01(i * 3 + 2) * 6.0;
        let (cx, cy) = (64.0 + r * a.cos(), 64.0 + r * a.sin());
        for y in 0..s {
            for x in 0..s {
                let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
                let v = ((rad - d + 0.5).clamp(0.0, 1.0) * 255.0) as u8;
                alpha[y * s + x] = alpha[y * s + x].max(v);
            }
        }
    }
    paint_core::Stamp { width: s, height: s, alpha }
}

/// «Мел»: круг с рваным краем и пропусками внутри.
fn chalk() -> paint_core::Stamp {
    let s = 128usize;
    let alpha = (0..s * s)
        .map(|i| {
            let (x, y) = ((i % s) as f32 + 0.5 - 64.0, (i / s) as f32 + 0.5 - 64.0);
            let d = (x * x + y * y).sqrt() / 60.0;
            let n = paint_core::dynamics::hash01(i as u64);
            if d < 1.0 && n > 0.25 + d * 0.5 { (255.0 * (1.0 - d * 0.3)) as u8 } else { 0 }
        })
        .collect();
    paint_core::Stamp { width: s, height: s, alpha }
}

fn main() -> eframe::Result {
    #[cfg(target_os = "linux")]
    if std::env::var_os("DISPLAY").is_some() && std::env::var_os("PAINT_RS_WAYLAND").is_none() {
        std::env::remove_var("WAYLAND_DISPLAY");
        std::env::remove_var("WAYLAND_SOCKET");
    }
    let lang = std::env::var("PAINT_LANG").ok().unwrap_or_else(|| i18n::system().to_string());
    i18n::set(&lang);
    // отпечатки и текстуры — общие с рисовалкой (её папка настроек)
    paint_core::paper::set_user_dir(settings::dir());
    let path = std::env::args().nth(1).map(PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_title("GrazySAI Studio").with_inner_size([1360.0, 820.0]),
        ..Default::default()
    };
    eframe::run_native(
        "GrazySAI Studio",
        options,
        Box::new(move |cc| {
            i18n::install_fonts(&cc.egui_ctx);
            let mut s = Studio::new();
            if let Some(p) = path {
                s.open(p);
            }
            Ok(Box::new(s))
        }),
    )
}
