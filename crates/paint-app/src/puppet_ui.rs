//! Окно «Марионетка» (Puppet как в After Effects) и булавки на холсте: щелчок ставит булавку
//! выбранного вида, перетаскивание двигает её (метка в текущем кадре таймлайна), Ctrl +
//! перетаскивание записывает движение по кадрам в реальном времени (как запись в AE).

use crate::i18n::tr;
use crate::{PaintApp, ViewXf};
use eframe::egui::{self, Color32, Vec2};
use paint_core::puppet::{Pin, PinKind, PinPose, Puppet};
use std::time::Instant;

/// Перетаскивание булавки: номер, марионетка до него, запись (начало, первый кадр).
type PinDrag = (usize, Puppet, Option<(Instant, u32)>);

pub struct PuppetUi {
    pub show: bool,
    pub kind: PinKind,
    pub sel: Option<usize>,
    pub show_mesh: bool,
    /// Перетаскивание: булавка, как было до него, запись (начало, первый кадр).
    pub drag: Option<PinDrag>,
}

impl Default for PuppetUi {
    fn default() -> Self {
        Self { show: false, kind: PinKind::Position, sel: None, show_mesh: true, drag: None }
    }
}

fn kind_name(k: PinKind) -> &'static str {
    match k {
        PinKind::Position => "Position",
        PinKind::Starch => "Starch",
        PinKind::Bend => "Bend",
        PinKind::Advanced => "Advanced",
        PinKind::Overlap => "Overlap",
    }
}

fn kind_color(k: PinKind) -> Color32 {
    match k {
        PinKind::Position => Color32::from_rgb(250, 210, 40),
        PinKind::Starch => Color32::from_rgb(200, 200, 200),
        PinKind::Bend => Color32::from_rgb(200, 120, 50),
        PinKind::Advanced => Color32::from_rgb(60, 190, 180),
        PinKind::Overlap => Color32::from_rgb(120, 90, 220),
    }
}

impl PaintApp {
    fn puppet_layer(&self) -> Option<&Puppet> {
        self.doc.layer(&self.selected).and_then(|l| l.puppet.as_deref())
    }

    /// Пересчитать марионетки и показать.
    pub(crate) fn refresh_puppets(&mut self) {
        let keys = self.doc.update_puppets();
        self.invalidate(keys);
        self.thumbs.clear();
    }

    /// Мышь у марионетки: окно открыто и выбран слой-марионетка.
    pub(crate) fn puppet_takes_mouse(&self, start: bool) -> bool {
        self.puppet.show && self.puppet_layer().is_some() && (self.puppet.drag.is_some() || start)
    }

    pub(crate) fn puppet_mouse(&mut self, at: Option<Vec2>, pressed: bool, released: bool, ctrl: bool) {
        let f = self.doc.timeline.frame;
        let path = self.selected.clone();
        let zoom = self.zoom;
        if pressed && self.puppet.drag.is_none() {
            let Some(p) = at else { return };
            let Some(pp) = self.puppet_layer().cloned() else { return };
            let pos = [p.x, p.y];
            // ближняя булавка (в 10 точках экрана) — по её положению в кадре
            let near = pp.pins.iter().enumerate().map(|(i, pin)| (i, pin.pose(f).pos)).map(|(i, q)| (i, ((q[0] - pos[0]).powi(2) + (q[1] - pos[1]).powi(2)).sqrt() * zoom)).filter(|x| x.1 < 10.0).min_by(|a, b| a.1.total_cmp(&b.1)).map(|x| x.0);
            let i = match near {
                Some(i) => i,
                None => {
                    // новая булавка: место на исходнике — по треугольнику деформированной сетки
                    let (mesh, cur) = match pp.last_mesh() {
                        Some(m) => m,
                        None => {
                            let m = pp.mesh();
                            let c = pp.deform(&m, f, None);
                            (std::sync::Arc::new(m), c)
                        }
                    };
                    let rest = mesh.tris.iter().find_map(|t| {
                        let (a, b, c) = (cur[t[0] as usize], cur[t[1] as usize], cur[t[2] as usize]);
                        let d = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
                        if d.abs() < 1e-9 {
                            return None;
                        }
                        let u = ((b[1] - c[1]) * (pos[0] - c[0]) + (c[0] - b[0]) * (pos[1] - c[1])) / d;
                        let v = ((c[1] - a[1]) * (pos[0] - c[0]) + (a[0] - c[0]) * (pos[1] - c[1])) / d;
                        let w = 1.0 - u - v;
                        (u >= 0.0 && v >= 0.0 && w >= 0.0).then(|| {
                            let r = |k: usize| mesh.rest[t[k] as usize];
                            [r(0)[0] * u + r(1)[0] * v + r(2)[0] * w, r(0)[1] * u + r(1)[1] * v + r(2)[1] * w]
                        })
                    });
                    let Some(rest) = rest else {
                        self.status = tr("Булавку ставят на рисунок слоя").to_string();
                        return;
                    };
                    let mut pin = Pin::new(self.puppet.kind, rest);
                    // уже сдвинутая сетка: новая булавка держит то место, где её поставили
                    if self.puppet.kind.holds_position() && (rest[0] - pos[0]).abs() + (rest[1] - pos[1]).abs() > 0.01 {
                        pin.keys.insert(f, PinPose { pos, rot: 0.0, scale: 1.0 });
                    }
                    let before = pp;
                    if let Some(l) = self.doc.layer_mut(&path).and_then(|l| l.puppet.as_mut()) {
                        l.pins.push(pin);
                        let n = l.pins.len() - 1;
                        self.doc.puppet_edited(&path, before);
                        self.puppet.sel = Some(n);
                        self.refresh_puppets();
                        self.dirty = true;
                    }
                    return;
                }
            };
            self.puppet.sel = Some(i);
            let record = (ctrl && pp.pins[i].kind.holds_position()).then(|| {
                self.doc.timeline.enabled = true;
                (Instant::now(), f)
            });
            self.puppet.drag = Some((i, pp, record));
        }
        let Some((i, _, record)) = self.puppet.drag.as_ref().map(|d| (d.0, (), d.2)) else { return };
        if let Some(p) = at {
            // запись: кадр идёт по времени, как при проигрывании
            let frame = match record {
                Some((t0, f0)) => {
                    let t = &self.doc.timeline;
                    let k = (t0.elapsed().as_secs_f64() * t.fps.max(1) as f64) as u32;
                    let fr = (f0 + k).min(t.frames - 1);
                    if fr != t.frame {
                        self.doc.timeline.frame = fr;
                        self.refresh_all();
                        self.refresh_puppets();
                    }
                    fr
                }
                None => f,
            };
            if let Some(pin) = self.doc.layer_mut(&path).and_then(|l| l.puppet.as_mut()).and_then(|pp| pp.pins.get_mut(i)) {
                let pose = pin.pose(frame);
                match pin.kind {
                    PinKind::Position | PinKind::Advanced => {
                        pin.keys.insert(frame, PinPose { pos: [p.x, p.y], ..pose });
                    }
                    PinKind::Bend => {
                        // Bend: тянуть — поворот вокруг булавки
                        let a = (p.y - pose.pos[1]).atan2(p.x - pose.pos[0]);
                        pin.keys.insert(frame, PinPose { rot: a, ..pose });
                    }
                    // Starch и Overlap не двигаются — переносится их место
                    _ => pin.at = [p.x, p.y],
                }
            }
            self.refresh_puppets();
        }
        if released {
            if let Some((_, before, _)) = self.puppet.drag.take() {
                self.doc.puppet_edited(&path, before);
                self.dirty = true;
            }
        }
    }

    /// Сетка и булавки поверх холста.
    pub(crate) fn puppet_overlay(&self, painter: &egui::Painter, xf: ViewXf) {
        if !self.puppet.show {
            return;
        }
        let Some(pp) = self.puppet_layer() else { return };
        let f = self.doc.timeline.frame;
        if self.puppet.show_mesh {
            if let Some((mesh, cur)) = pp.last_mesh().filter(|m| m.0.edges.len() < 60_000) {
                let s = egui::Stroke::new(0.6_f32, Color32::from_rgba_unmultiplied(255, 220, 60, 110));
                for &(a, b) in &mesh.edges {
                    let (pa, pb) = (cur[a as usize], cur[b as usize]);
                    painter.line_segment([xf.to_screen(Vec2::new(pa[0], pa[1])), xf.to_screen(Vec2::new(pb[0], pb[1]))], s);
                }
            }
        }
        for (i, pin) in pp.pins.iter().enumerate() {
            let pose = pin.pose(f);
            let c = xf.to_screen(Vec2::new(pose.pos[0], pose.pos[1]));
            let col = kind_color(pin.kind);
            if matches!(pin.kind, PinKind::Starch | PinKind::Overlap) || (pin.kind.turns() && self.puppet.sel == Some(i)) {
                painter.circle_stroke(c, pin.extent * self.zoom, egui::Stroke::new(1.0_f32, col.gamma_multiply(0.6)));
            }
            painter.circle_filled(c, if self.puppet.sel == Some(i) { 6.0 } else { 4.5 }, col);
            painter.circle_stroke(c, 6.5, egui::Stroke::new(1.0_f32, Color32::BLACK));
        }
    }

    pub(crate) fn puppet_window(&mut self, ctx: &egui::Context) {
        if !self.puppet.show {
            return;
        }
        let mut open = true;
        let path = self.selected.clone();
        let f = self.doc.timeline.frame;
        let mut make: Option<bool> = None;
        let mut changed = false;
        // как было до правки в окне (исходник окно не меняет — его не копируем каждый кадр)
        let mut before: Option<(f32, u8, Vec<Pin>)> = None;
        egui::Window::new(tr("Марионетка (Puppet)")).id(egui::Id::new("puppet")).open(&mut open).default_width(300.0).resizable(true).show(ctx, |ui| {
            let Some(layer) = self.doc.layer_mut(&path) else { return };
            let Some(pp) = layer.puppet.as_mut() else {
                let ok = layer.children.is_none() && layer.vector.is_none() && layer.scene3d.is_none() && layer.adjust.is_none() && !layer.pixels.tiles.is_empty();
                ui.label(tr("Рисунок слоя становится марионеткой: сетка по силуэту, булавки тянут картинку."));
                if ui.add_enabled(ok, egui::Button::new(tr("Сделать марионеткой"))).clicked() {
                    make = Some(true);
                }
                return;
            };
            before = Some((pp.expansion, pp.density, pp.pins.clone()));
            changed |= ui.add(egui::Slider::new(&mut pp.expansion, 0.0..=32.0).text("Expansion")).changed();
            changed |= ui.add(egui::Slider::new(&mut pp.density, 1..=10).text("Density")).changed();
            ui.checkbox(&mut self.puppet.show_mesh, tr("Показывать сетку"));
            ui.separator();
            ui.label(tr("Новая булавка:"));
            ui.horizontal_wrapped(|ui| {
                for k in PinKind::ALL {
                    ui.radio_value(&mut self.puppet.kind, k, egui::RichText::new(kind_name(k)).color(kind_color(k).gamma_multiply(0.8)));
                }
            });
            let mut del = false;
            if let Some(pin) = self.puppet.sel.and_then(|i| pp.pins.get_mut(i)) {
                ui.separator();
                ui.label(egui::RichText::new(format!("{} — {}", kind_name(pin.kind), tr("выбранная булавка"))).strong());
                match pin.kind {
                    PinKind::Starch => {
                        changed |= ui.add(egui::Slider::new(&mut pin.amount, -2.0..=5.0).text("Amount")).changed();
                        changed |= ui.add(egui::Slider::new(&mut pin.extent, 1.0..=300.0).text("Extent")).changed();
                    }
                    PinKind::Overlap => {
                        changed |= ui.add(egui::Slider::new(&mut pin.in_front, -100.0..=100.0).text("In Front")).changed();
                        changed |= ui.add(egui::Slider::new(&mut pin.extent, 1.0..=300.0).text("Extent")).changed();
                    }
                    PinKind::Bend | PinKind::Advanced => {
                        let pose = pin.pose(f);
                        let (mut deg, mut sc) = (pose.rot.to_degrees(), pose.scale * 100.0);
                        let a = ui.add(egui::Slider::new(&mut deg, -180.0..=180.0).text(tr("поворот, °"))).changed();
                        let b = ui.add(egui::Slider::new(&mut sc, 10.0..=400.0).text(tr("размер, %"))).changed();
                        if a || b {
                            pin.keys.insert(f, PinPose { rot: deg.to_radians(), scale: sc / 100.0, ..pose });
                            changed = true;
                        }
                        changed |= ui.add(egui::Slider::new(&mut pin.extent, 1.0..=300.0).text("Extent")).changed();
                    }
                    PinKind::Position => {}
                }
                ui.horizontal(|ui| {
                    if ui.add_enabled(pin.keys.contains_key(&f), egui::Button::new(tr("Убрать метку кадра"))).clicked() {
                        pin.keys.remove(&f);
                        changed = true;
                    }
                    del = ui.button(tr("Удалить булавку")).clicked();
                });
            }
            if del {
                if let Some(i) = self.puppet.sel.take() {
                    pp.pins.remove(i);
                    changed = true;
                }
            }
            ui.separator();
            ui.label(egui::RichText::new(tr("Щелчок по рисунку — булавка; тянуть — двигать (метка в текущем кадре); Ctrl + тянуть — записать движение по кадрам, как при проигрывании.")).small());
            if ui.button(tr("Снять марионетку")).on_hover_text(tr("Слой снова — исходный рисунок")).clicked() {
                make = Some(false);
            }
        });
        if !open {
            self.puppet.show = false;
        }
        if let Some(on) = make {
            let keys = self.doc.set_puppet(&path, on);
            self.invalidate(keys);
            self.puppet.sel = None;
            self.refresh_puppets();
            self.dirty = true;
        } else if changed {
            if let Some((e, d, pins)) = before {
                if let Some(mut b) = self.doc.layer(&path).and_then(|l| l.puppet.as_deref()).cloned() {
                    b.expansion = e;
                    b.density = d;
                    b.pins = pins;
                    self.doc.puppet_edited(&path, b);
                }
            }
            self.refresh_puppets();
            self.dirty = true;
        }
    }
}
