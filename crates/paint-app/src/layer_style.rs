//! Окно «Стиль и коррекция слоя»: стили обычного слоя (обводка, тень, свечение, наложение цвета)
//! и настройки слоя коррекции (карта градиента, уровни, тон и насыщенность, яркость и контраст,
//! постеризация, порог). Правка — сразу на холсте, как у эффекта и бумаги слоя.

use crate::i18n::tr;
use crate::PaintApp;
use eframe::egui::{self, Color32};
use paint_core::adjust::{Adjust, Stop};
use paint_core::filters::{BrightnessContrast, HueSaturation, Levels};
use paint_core::style::Paint;

/// Имя вида коррекции на языке интерфейса.
pub(crate) fn adjust_name(a: &Adjust) -> &'static str {
    match a {
        Adjust::GradientMap(_) => tr("Карта градиента"),
        Adjust::Levels(_) => tr("Уровни"),
        Adjust::HueSaturation(_) => tr("Тон и насыщенность"),
        Adjust::BrightnessContrast(_) => tr("Яркость и контраст"),
        Adjust::Invert => tr("Инверсия"),
        Adjust::Posterize(_) => tr("Постеризация"),
        Adjust::Threshold(_) => tr("Порог"),
    }
}

/// Готовые карты градиента: имя и узлы.
fn presets() -> Vec<(&'static str, Vec<Stop>)> {
    let s = |at: f32, c: [u8; 3]| Stop { at, color: c };
    vec![
        ("Чёрно-белая", vec![s(0.0, [0, 0, 0]), s(1.0, [255, 255, 255])]),
        ("Сепия", vec![s(0.0, [30, 18, 8]), s(0.5, [150, 105, 60]), s(1.0, [255, 240, 210])]),
        ("Закат", vec![s(0.0, [40, 10, 70]), s(0.45, [220, 60, 60]), s(0.8, [255, 170, 60]), s(1.0, [255, 245, 200])]),
        ("Ночь", vec![s(0.0, [5, 5, 25]), s(0.6, [40, 70, 140]), s(1.0, [200, 230, 255])]),
        ("Мята", vec![s(0.0, [10, 40, 50]), s(0.6, [70, 180, 150]), s(1.0, [235, 255, 240])]),
    ]
}

/// Полоса градиента для показа.
fn gradient_bar(ui: &mut egui::Ui, stops: &[Stop], w: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, 16.0), egui::Sense::hover());
    let p = ui.painter();
    let n = 64;
    for i in 0..n {
        let t = i as f32 / (n - 1) as f32;
        let c = Adjust::gradient_at(stops, t).map(|v| (v * 255.0).round() as u8);
        let x0 = rect.left() + rect.width() * i as f32 / n as f32;
        let x1 = rect.left() + rect.width() * (i + 1) as f32 / n as f32 + 0.5;
        p.rect_filled(egui::Rect::from_min_max(egui::pos2(x0, rect.top()), egui::pos2(x1, rect.bottom())), 0.0, Color32::from_rgb(c[0], c[1], c[2]));
    }
    p.rect_stroke(rect, 0.0, egui::Stroke::new(1.0_f32, Color32::GRAY));
}

fn paint_row(ui: &mut egui::Ui, p: &mut Paint) -> bool {
    let mut changed = ui.color_edit_button_srgb(&mut p.color).changed();
    changed |= ui.add(egui::Slider::new(&mut p.opacity, 0..=100).text(tr("непрозрачность"))).changed();
    changed
}

impl PaintApp {
    /// Новый слой коррекции над выбранным; окно настроек открывается сразу.
    pub(crate) fn add_adjust_layer(&mut self, a: Adjust) {
        let mut path = self.selected.clone();
        let index = path.pop().map_or(0, |i| i + 1);
        let mut layer = paint_core::layer::Layer::new(adjust_name(&a));
        layer.adjust = Some(a);
        path.push(index);
        let keys = self.doc.insert_layer(&path, layer);
        self.selected = path;
        self.after_structure(keys);
        self.refresh_all();
        self.show_style = true;
    }

    pub(crate) fn style_window(&mut self, ctx: &egui::Context) {
        if !self.show_style {
            return;
        }
        let mut open = true;
        let mut changed = false;
        let mut bake = false;
        let path = self.selected.clone();
        egui::Window::new(tr("Стиль и коррекция слоя")).id(egui::Id::new("layer_style")).open(&mut open).default_width(320.0).resizable(true).show(ctx, |ui| {
            let Some(layer) = self.doc.layer_mut(&path) else { return };
            ui.label(egui::RichText::new(&layer.name).strong());
            if let Some(a) = layer.adjust.as_mut() {
                ui.label(adjust_name(a));
                match a {
                    Adjust::GradientMap(stops) => {
                        let w = ui.available_width().min(300.0);
                        gradient_bar(ui, stops, w);
                        let mut remove = None;
                        for (i, s) in stops.iter_mut().enumerate() {
                            ui.horizontal(|ui| {
                                changed |= ui.color_edit_button_srgb(&mut s.color).changed();
                                changed |= ui.add(egui::Slider::new(&mut s.at, 0.0..=1.0).fixed_decimals(2)).changed();
                                if ui.small_button(egui_phosphor::regular::X).on_hover_text(tr("Убрать узел")).clicked() {
                                    remove = Some(i);
                                }
                            });
                        }
                        if let Some(i) = remove.filter(|_| stops.len() > 2) {
                            stops.remove(i);
                            changed = true;
                        }
                        ui.horizontal(|ui| {
                            if ui.button(tr("Добавить узел")).clicked() {
                                let c = Adjust::gradient_at(stops, 0.5).map(|v| (v * 255.0).round() as u8);
                                stops.push(Stop { at: 0.5, color: c });
                                changed = true;
                            }
                            if ui.button(tr("Обратить")).clicked() {
                                for s in stops.iter_mut() {
                                    s.at = 1.0 - s.at;
                                }
                                changed = true;
                            }
                        });
                        ui.horizontal_wrapped(|ui| {
                            for (name, p) in presets() {
                                if ui.button(tr(name)).clicked() {
                                    *stops = p;
                                    changed = true;
                                }
                            }
                        });
                        if changed {
                            stops.sort_by(|a, b| a.at.total_cmp(&b.at));
                        }
                    }
                    Adjust::Levels(l) => {
                        changed |= ui.add(egui::Slider::new(&mut l.in_black, 0..=254).text(tr("вход: чёрное"))).changed();
                        changed |= ui.add(egui::Slider::new(&mut l.in_white, 1..=255).text(tr("вход: белое"))).changed();
                        changed |= ui.add(egui::Slider::new(&mut l.gamma, 0.1..=10.0).logarithmic(true).text(tr("гамма"))).changed();
                        changed |= ui.add(egui::Slider::new(&mut l.out_black, 0..=255).text(tr("выход: чёрное"))).changed();
                        changed |= ui.add(egui::Slider::new(&mut l.out_white, 0..=255).text(tr("выход: белое"))).changed();
                    }
                    Adjust::HueSaturation(h) => {
                        changed |= ui.add(egui::Slider::new(&mut h.hue, -180..=180).text(tr("тон"))).changed();
                        changed |= ui.add(egui::Slider::new(&mut h.saturation, -100..=100).text(tr("насыщенность"))).changed();
                        changed |= ui.add(egui::Slider::new(&mut h.luminance, -100..=100).text(tr("яркость"))).changed();
                        changed |= ui.checkbox(&mut h.colorize, tr("окрасить")).changed();
                    }
                    Adjust::BrightnessContrast(b) => {
                        changed |= ui.add(egui::Slider::new(&mut b.brightness, -100..=100).text(tr("яркость"))).changed();
                        changed |= ui.add(egui::Slider::new(&mut b.contrast, -100..=100).text(tr("контраст"))).changed();
                        changed |= ui.add(egui::Slider::new(&mut b.depth, -100..=100).text(tr("глубина цвета"))).changed();
                    }
                    Adjust::Posterize(n) => changed |= ui.add(egui::Slider::new(n, 2..=64).text(tr("уровней"))).changed(),
                    Adjust::Threshold(t) => changed |= ui.add(egui::Slider::new(t, 1..=255).text(tr("порог"))).changed(),
                    Adjust::Invert => {}
                }
                ui.label(egui::RichText::new(tr("Действует на всё ниже в своей папке; с обтравкой — только на слой под собой. Непрозрачность, режим и маска — как у слоя.")).small());
                return;
            }
            if layer.children.is_some() || layer.vector.is_some() || layer.scene3d.is_some() {
                ui.label(tr("Стили — только у обычных слоёв"));
                return;
            }
            let st = layer.styles.get_or_insert_with(Default::default);
            let section = |ui: &mut egui::Ui, on: bool, name: &'static str| -> (bool, bool) {
                let mut v = on;
                let c = ui.checkbox(&mut v, tr(name)).changed();
                (v, c)
            };
            // обводка
            let (on, c) = section(ui, st.stroke.is_some(), "Обводка");
            if c {
                st.stroke = on.then_some((3, Paint { color: [0, 0, 0], opacity: 100 }));
                changed = true;
            }
            if let Some((w, p)) = st.stroke.as_mut() {
                ui.indent("stroke", |ui| {
                    changed |= ui.add(egui::Slider::new(w, 1..=64).text(tr("толщина"))).changed();
                    ui.horizontal(|ui| changed |= paint_row(ui, p));
                });
            }
            let (on, c) = section(ui, st.shadow.is_some(), "Тень");
            if c {
                st.shadow = on.then_some((6, 6, 6, Paint { color: [0, 0, 0], opacity: 50 }));
                changed = true;
            }
            if let Some((dx, dy, b, p)) = st.shadow.as_mut() {
                ui.indent("shadow", |ui| {
                    changed |= ui.add(egui::Slider::new(dx, -64..=64).text(tr("сдвиг X"))).changed();
                    changed |= ui.add(egui::Slider::new(dy, -64..=64).text(tr("сдвиг Y"))).changed();
                    changed |= ui.add(egui::Slider::new(b, 0..=64).text(tr("размытие"))).changed();
                    ui.horizontal(|ui| changed |= paint_row(ui, p));
                });
            }
            let (on, c) = section(ui, st.glow.is_some(), "Свечение");
            if c {
                st.glow = on.then_some((10, Paint { color: [255, 240, 150], opacity: 75 }));
                changed = true;
            }
            if let Some((w, p)) = st.glow.as_mut() {
                ui.indent("glow", |ui| {
                    changed |= ui.add(egui::Slider::new(w, 1..=64).text(tr("размер"))).changed();
                    ui.horizontal(|ui| changed |= paint_row(ui, p));
                });
            }
            let (on, c) = section(ui, st.overlay.is_some(), "Наложение цвета");
            if c {
                st.overlay = on.then_some(Paint { color: [255, 0, 0], opacity: 100 });
                changed = true;
            }
            if let Some(p) = st.overlay.as_mut() {
                ui.indent("overlay", |ui| {
                    ui.horizontal(|ui| changed |= paint_row(ui, p));
                });
            }
            if st.is_empty() {
                layer.styles = None;
            } else {
                ui.separator();
                bake = ui.button(tr("Применить стиль к пикселям")).on_hover_text(tr("Для SAI и PSD: стиль становится рисунком слоя")).clicked();
            }
        });
        if !open {
            self.show_style = false;
        }
        if bake {
            let keys = self.doc.bake_styles(&path);
            self.invalidate(keys);
            self.thumbs.clear();
            self.dirty = true;
        } else if changed {
            self.refresh_all();
            self.thumbs.clear();
            self.dirty = true;
        }
    }
}

/// Виды слоёв коррекции для меню: имя и слой по умолчанию.
pub(crate) fn kinds() -> Vec<Adjust> {
    vec![
        Adjust::gradient_default(),
        Adjust::Levels(Levels::default()),
        Adjust::HueSaturation(HueSaturation::default()),
        Adjust::BrightnessContrast(BrightnessContrast::default()),
        Adjust::Invert,
        Adjust::Posterize(4),
        Adjust::Threshold(128),
    ]
}
