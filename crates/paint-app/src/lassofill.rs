//! Заливка лассо (как в SAI 2026, `docs/41-sai2-after-2017.md`): обвести — залить; с «замкнутыми
//! областями» — только то, что внутри обводки замкнуто линиями, с закрытием зазоров.

use crate::i18n::tr;
use crate::{sai_ui, PaintApp, SelDrag};
use eframe::egui::{self, Vec2};
use paint_core::selection::{Combine, FillRegion, LassoFill};

/// Настройки инструмента.
#[derive(Clone, Copy, PartialEq)]
pub struct Settings {
    /// 0 — от руки, 1 — многоугольник, 2 — многоугольник и от руки.
    pub shape: u8,
    pub aa: bool,
    pub opts: LassoFill,
    /// Где искать области: 0 — текущий слой, 1 — указанные слои («Источник выбора»), 2 — всё изображение.
    pub source: u8,
    /// Не ограничиваться выделением.
    pub pass_through: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { shape: 0, aa: true, opts: LassoFill::default(), source: 0, pass_through: false }
    }
}

const REGIONS: [FillRegion; 4] = [FillRegion::Transparent, FillRegion::TransparentFuzzy, FillRegion::Similar, FillRegion::Opaque];

impl Settings {
    pub fn save(&self, s: &mut crate::settings::Settings) {
        let o = &self.opts;
        s.set("lassofill.shape", self.shape);
        s.set("lassofill.aa", self.aa as u8);
        s.set("lassofill.enclosed", o.enclosed as u8);
        s.set("lassofill.region", REGIONS.iter().position(|r| *r == o.region).unwrap_or(0));
        s.set("lassofill.range", o.range);
        s.set("lassofill.gap", o.gap);
        s.set("lassofill.dilate", o.dilate);
        s.set("lassofill.ignore_painted", o.ignore_painted as u8);
        s.set("lassofill.source", self.source);
        s.set("lassofill.pass_through", self.pass_through as u8);
    }

    pub fn load(&mut self, s: &crate::settings::Settings) {
        let f = |k: &str| s.f32(k);
        if let Some(v) = f("lassofill.shape") {
            self.shape = (v as u8).min(2);
        }
        if let Some(v) = f("lassofill.aa") {
            self.aa = v != 0.0;
        }
        if let Some(v) = f("lassofill.enclosed") {
            self.opts.enclosed = v != 0.0;
        }
        if let Some(v) = f("lassofill.region") {
            self.opts.region = REGIONS[(v as usize).min(3)];
        }
        if let Some(v) = f("lassofill.range") {
            self.opts.range = v.clamp(0.0, 255.0) as u8;
        }
        if let Some(v) = f("lassofill.gap") {
            self.opts.gap = v.clamp(0.0, 100.0) as u32;
        }
        if let Some(v) = f("lassofill.dilate") {
            self.opts.dilate = v.clamp(0.0, 20.0) as u32;
        }
        if let Some(v) = f("lassofill.ignore_painted") {
            self.opts.ignore_painted = v != 0.0;
        }
        if let Some(v) = f("lassofill.source") {
            self.source = (v as u8).min(2);
        }
        if let Some(v) = f("lassofill.pass_through") {
            self.pass_through = v != 0.0;
        }
    }
}

impl PaintApp {
    /// Нажатие, движение, отпускание мыши с заливкой лассо.
    pub(crate) fn lasso_fill_input(&mut self, at: Option<Vec2>, moves: &[Vec2], pressed: bool, released: bool, double: bool, down: bool) {
        let poly = self.lassofill.shape != 0;
        if poly && double {
            self.close_polygon();
            return;
        }
        if pressed {
            if let Some(c) = at {
                if poly {
                    match self.sel_poly.as_mut() {
                        Some(d) => {
                            if d.points.last() != Some(&c) {
                                d.points.push(c);
                            }
                        }
                        None => self.sel_poly = Some(SelDrag { points: vec![c], mode: Combine::Replace, fill: true }),
                    }
                } else {
                    self.sel_drag = Some(SelDrag { points: vec![c, c], mode: Combine::Replace, fill: true });
                }
            }
            return;
        }
        if let Some(d) = self.sel_drag.as_mut() {
            d.points.extend_from_slice(moves);
        } else if self.lassofill.shape == 2 && down {
            // многоугольник и от руки: с нажатой кнопкой — от руки
            if let Some(d) = self.sel_poly.as_mut() {
                d.points.extend_from_slice(moves);
            }
        }
        if released {
            if let Some(d) = self.sel_drag.take() {
                self.lasso_fill_points(&d.points);
            }
        }
    }

    /// Залить по обводке.
    pub(crate) fn lasso_fill_points(&mut self, points: &[Vec2]) {
        if points.len() < 3 {
            return;
        }
        if !self.can_paint() {
            self.status = String::from(tr("Заливать можно видимый обычный слой без запрета рисования"));
            return;
        }
        let pts: Vec<[f64; 2]> = points.iter().map(|p| [p.x as f64, p.y as f64]).collect();
        let lasso = paint_core::selection::polygon_aa(&pts, self.lassofill.aa);
        let st = self.lassofill;
        let area = if st.opts.enclosed {
            let src = match st.source {
                0 => self.doc.layer(&self.selected).map(|l| l.pixels.clone()).unwrap_or_default(),
                1 => self.doc.pick_source(&self.selected),
                _ => self.doc.merged_pixels(),
            };
            paint_core::selection::lasso_fill(&lasso, &src, self.doc.width, self.doc.height, (points[0].x.floor() as i32, points[0].y.floor() as i32), st.opts)
        } else {
            lasso
        };
        let c = self.color();
        let keys = self.doc.fill_with(&self.selected.clone(), &area, [c.r(), c.g(), c.b()], !st.pass_through);
        if keys.is_empty() && st.opts.enclosed {
            self.status = String::from(tr("Внутри обводки нет замкнутых областей (увеличьте «Допуск зазоров»)"));
        }
        self.invalidate(keys);
        self.thumbs.clear();
        self.dirty = true;
    }

    /// Панель настроек заливки лассо.
    pub(crate) fn lasso_fill_panel(&mut self, ui: &mut egui::Ui, lw: f32, w: f32) {
        let before = self.lassofill;
        let st = &mut self.lassofill;
        ui.label(tr("Обводка:"));
        for (i, name) in [tr("от руки"), tr("многоугольник"), tr("многоугольник и от руки")].into_iter().enumerate() {
            ui.radio_value(&mut st.shape, i as u8, name);
        }
        ui.checkbox(&mut st.aa, tr("Сглаживание края"));
        ui.separator();
        ui.checkbox(&mut st.opts.enclosed, tr("Только замкнутые области")).on_hover_text(tr("Заливается только то, что внутри обводки замкнуто линиями; зазоры в линиях закрываются"));
        ui.add_enabled_ui(st.opts.enclosed, |ui| {
            let names = [tr("прозрачное"), tr("прозрачное с допуском"), tr("похожий цвет"), tr("непрозрачное")];
            for (r, n) in REGIONS.iter().zip(names) {
                ui.radio_value(&mut st.opts.region, *r, n);
            }
            let rows: [(&str, &mut u32, u32, &str); 3] = [(tr("Допуск"), &mut 0, 255, ""), (tr("Допуск зазоров"), &mut st.opts.gap, 100, " px"), (tr("Расширить"), &mut st.opts.dilate, 20, " px")];
            let mut range = st.opts.range as u32;
            for (k, (label, v, max, unit)) in rows.into_iter().enumerate() {
                let v: &mut u32 = if k == 0 { &mut range } else { v };
                sai_ui::row(ui, label, lw, true, |ui| {
                    let mut f = *v as f32;
                    if sai_ui::bar(ui, &mut f, 0.0, max as f32, format!("{}{unit}", *v), w, true).changed() {
                        *v = f.round().clamp(0.0, max as f32) as u32;
                    }
                });
            }
            st.opts.range = range as u8;
            ui.label(tr("Где искать:"));
            for (i, name) in [tr("текущий слой"), tr("указанные слои"), tr("всё изображение")].into_iter().enumerate() {
                ui.radio_value(&mut st.source, i as u8, name);
            }
            ui.checkbox(&mut st.opts.ignore_painted, tr("Не трогать закрашенное"));
        });
        ui.checkbox(&mut st.pass_through, tr("Не ограничиваться выделением"));
        if self.lassofill != before {
            self.save_settings();
        }
    }
}
