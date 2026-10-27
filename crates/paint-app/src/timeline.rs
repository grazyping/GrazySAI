//! Таймлайн и анимация как в Clip Studio Paint: меню «Анимация», окно «Таймлайн» (дорожки папок
//! анимации, кадры, проигрывание, Onion skin), вывод в GIF, APNG, PNG-кадры и видео (ffmpeg).

use crate::i18n::{tr, trf};
use crate::{files, menu_icons, PaintApp};
use eframe::egui::{self, Color32, Pos2, Rect, Vec2};
use paint_core::anim::AnimTrack;
use paint_core::layer::Layer;
use std::path::PathBuf;
use std::time::Instant;

/// Состояние проигрывания и окна.
#[derive(Default)]
pub struct TimelineUi {
    pub show: bool,
    /// Идёт проигрывание: с какого мгновения и с какого кадра.
    pub playing: Option<(Instant, u32)>,
    pub onion_settings: bool,
    pub ffmpeg: Option<bool>,
}

/// Пути папок анимации (сверху вниз, как в списке слоёв).
fn anim_folders(layers: &[Layer], prefix: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
    for (i, l) in layers.iter().enumerate().rev() {
        prefix.push(i);
        if l.anim.is_some() {
            out.push(prefix.clone());
        } else if let Some(c) = &l.children {
            anim_folders(c, prefix, out);
        }
        prefix.pop();
    }
}

const EXPORTS: [(&str, &str); 4] = [("GIF…", "gif"), ("APNG…", "png"), ("PNG-кадры…", "png"), ("Видео MP4 (ffmpeg)…", "mp4")];

impl PaintApp {
    /// Папка анимации, в которой выбранный слой (или она сама).
    fn current_anim_folder(&self) -> Option<Vec<usize>> {
        let p = &self.selected;
        if self.doc.layer(p).is_some_and(|l| l.anim.is_some()) {
            return Some(p.clone());
        }
        let parent = &p[..p.len().checked_sub(1)?];
        self.doc.layer(parent).filter(|l| l.anim.is_some()).map(|_| parent.to_vec())
    }

    /// Перейти к кадру: показать его, выбрать его cel в папке выбранного слоя.
    pub(crate) fn set_frame(&mut self, f: u32) {
        let t = &mut self.doc.timeline;
        t.frame = f.min(t.frames.saturating_sub(1));
        let f = t.frame;
        if let Some(folder) = self.current_anim_folder() {
            if let Some(l) = self.doc.layer(&folder) {
                let cel = l.anim.as_ref().and_then(|a| a.cel_at(f)).map(str::to_string);
                if let Some(i) = cel.and_then(|c| l.children.as_ref()?.iter().position(|x| x.name == c)) {
                    let mut p = folder.clone();
                    p.push(i);
                    self.selected = p;
                }
            }
        }
        // пересчитать только рисунок папок анимации, а не весь холст
        match paint_core::document::anim_tiles(&self.doc.layers) {
            Some(keys) => self.invalidate(keys),
            None => self.refresh_all(),
        }
        self.refresh_puppets();
    }

    /// Новая папка анимации с первым cel «1» в кадре 0; таймлайн включается.
    pub(crate) fn new_anim_folder(&mut self) {
        let mut path = self.selected.clone();
        let index = path.pop().map_or(0, |i| i + 1);
        let mut folder = Layer::folder(tr("Анимация"), vec![Layer::new("1")]);
        let mut track = AnimTrack::default();
        track.keys.insert(self.doc.timeline.frame, "1".into());
        folder.anim = Some(Box::new(track));
        path.push(index);
        let keys = self.doc.insert_layer(&path, folder);
        path.push(0);
        self.selected = path;
        self.doc.timeline.enabled = true;
        self.after_structure(keys);
        self.refresh_all();
        self.timeline.show = true;
    }

    /// Новый cel в текущей папке анимации — в текущий кадр.
    pub(crate) fn new_cel(&mut self) {
        let Some(folder) = self.current_anim_folder() else {
            self.status = tr("Сначала создайте папку анимации («Анимация → Новая папка анимации»)").to_string();
            return;
        };
        let Some(l) = self.doc.layer(&folder) else { return };
        let names: Vec<String> = l.children.as_deref().unwrap_or(&[]).iter().map(|c| c.name.clone()).collect();
        let mut n = names.len() + 1;
        while names.contains(&n.to_string()) {
            n += 1;
        }
        let name = n.to_string();
        let count = names.len();
        let mark = self.doc.history_mark();
        let mut at = folder.clone();
        at.push(count);
        let mut keys = self.doc.insert_layer(&at, Layer::new(&name));
        let f = self.doc.timeline.frame;
        let mut track = self.doc.layer(&folder).and_then(|l| l.anim.as_deref().cloned()).unwrap_or_default();
        track.keys.insert(f, name);
        keys.extend(self.doc.set_anim(&folder, Some(track)));
        // новый слой и метка — одна отмена
        self.doc.squash_since(mark);
        self.selected = at;
        self.after_structure(keys);
        self.refresh_all();
    }

    /// Поставить метку в кадр `f` дорожки `folder`: cel (`Some("")` — пустой кадр) или убрать (`None`).
    fn set_key(&mut self, folder: &[usize], f: u32, cel: Option<String>) {
        let mut track = self.doc.layer(folder).and_then(|l| l.anim.as_deref().cloned()).unwrap_or_default();
        match cel {
            Some(c) => {
                track.keys.insert(f, c);
            }
            None => {
                track.keys.remove(&f);
            }
        }
        let keys = self.doc.set_anim(folder, Some(track));
        self.invalidate(keys);
        self.dirty = true;
        self.set_frame(f);
    }

    /// Проигрывание: следующий кадр по времени.
    pub(crate) fn timeline_tick(&mut self, ctx: &egui::Context) {
        let Some((t0, f0)) = self.timeline.playing else { return };
        let t = &self.doc.timeline;
        let (start, end, fps, looped) = (t.start, t.end.max(t.start), t.fps.max(1), t.looped);
        let len = end - start + 1;
        let passed = (t0.elapsed().as_secs_f64() * fps as f64) as u32;
        let mut f = f0.saturating_sub(start) + passed;
        if f >= len {
            if looped {
                f %= len;
            } else {
                self.timeline.playing = None;
                f = len - 1;
            }
        }
        let f = start + f;
        if f != self.doc.timeline.frame {
            self.doc.timeline.frame = f;
            self.refresh_all();
            self.refresh_puppets();
        }
        if self.timeline.playing.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(0.5 / fps as f64));
        }
    }

    /// Меню «Анимация».
    pub(crate) fn anim_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button(tr("Анимация"), |ui| {
            if ui.add(menu_icons::button("Новая папка анимации")).clicked() {
                self.new_anim_folder();
                ui.close_menu();
            }
            if ui.add(menu_icons::button("Новый кадр (cel)")).clicked() {
                self.new_cel();
                ui.close_menu();
            }
            ui.separator();
            let mut on = self.doc.timeline.enabled;
            if ui.checkbox(&mut on, tr("Включить таймлайн")).changed() {
                self.doc.timeline.enabled = on;
                self.refresh_all();
                self.dirty = true;
            }
            if ui.checkbox(&mut self.timeline.show, tr("Окно «Таймлайн»")).changed() {
                ui.close_menu();
            }
            let mut onion = self.doc.timeline.onion_on;
            if ui.checkbox(&mut onion, "Onion skin").changed() {
                self.doc.timeline.onion_on = onion;
                self.refresh_all();
            }
            if ui.button(tr("Настройки Onion skin…")).clicked() {
                self.timeline.onion_settings = true;
                ui.close_menu();
            }
            ui.separator();
            ui.menu_button(menu_icons::label("Вывод анимации"), |ui| {
                let ffmpeg = *self.timeline.ffmpeg.get_or_insert_with(paint_formats::anim_export::has_ffmpeg);
                for (k, (label, _)) in EXPORTS.iter().enumerate() {
                    let enabled = k != 3 || ffmpeg;
                    let r = ui.add_enabled(enabled, egui::Button::new(tr(label)));
                    let r = if enabled { r } else { r.on_disabled_hover_text(tr("Нужен ffmpeg: установите его и перезапустите программу")) };
                    if r.clicked() {
                        let path = self.anim_export_default(k as u8);
                        if !self.native_dialog(files::Ask::AnimExport(k as u8), &path) {
                            self.run_anim_export(k as u8, path);
                        }
                        ui.close_menu();
                    }
                }
            });
        });
    }

    pub(crate) fn anim_export_default(&self, kind: u8) -> PathBuf {
        let ext = EXPORTS[kind.min(3) as usize].1;
        let base = self.file.clone().unwrap_or_else(|| std::env::current_dir().unwrap_or_default().join("animation"));
        let suffix = if kind == 1 { "apng.png".to_string() } else { ext.to_string() };
        base.with_extension(suffix)
    }

    pub(crate) fn run_anim_export(&mut self, kind: u8, path: PathBuf) {
        use paint_formats::anim_export as ex;
        let t = self.doc.timeline.clone();
        let (w, h) = (self.doc.width, self.doc.height);
        let range = t.start..=t.end.max(t.start);
        let n = range.clone().count();
        let keep = self.doc.timeline.frame;
        let doc = &mut self.doc;
        // у марионеток пиксели — кадр таймлайна: пересчитать для каждого кадра
        let frames = range.map(|f| {
            doc.timeline.frame = f;
            doc.update_puppets();
            doc.flatten_frame(f)
        });
        let res = match kind {
            0 => ex::gif(&path, w, h, t.fps, t.looped, frames).map(|_| path.display().to_string()),
            1 => ex::apng(&path, w, h, t.fps, t.looped, frames.collect()).map(|_| path.display().to_string()),
            2 => ex::png_sequence(&path, w, h, frames).map(|v| v.first().map_or(String::new(), |p| p.display().to_string())),
            _ => ex::video(&path, w, h, t.fps, frames).map(|_| path.display().to_string()),
        };
        self.doc.timeline.frame = keep;
        let keys = self.doc.update_puppets();
        self.invalidate(keys);
        self.status = match res {
            Ok(p) => trf("Анимация выведена ({0} кадров): {1}", &[&n, &p]),
            Err(e) => trf("Не удалось вывести анимацию: {0}", &[&e]),
        };
    }

    /// Окно «Таймлайн» и настройки Onion skin.
    pub(crate) fn timeline_window(&mut self, ctx: &egui::Context) {
        self.timeline_tick(ctx);
        if self.timeline.onion_settings {
            let mut open = true;
            let mut changed = false;
            egui::Window::new(tr("Настройки Onion skin")).open(&mut open).resizable(false).show(ctx, |ui| {
                let o = &mut self.doc.timeline.onion;
                changed |= ui.add(egui::Slider::new(&mut o.before, 0..=10).text(tr("кадров до"))).changed();
                changed |= ui.add(egui::Slider::new(&mut o.after, 0..=10).text(tr("кадров после"))).changed();
                changed |= ui.add(egui::Slider::new(&mut o.opacity, 0.0..=1.0).text(tr("непрозрачность"))).changed();
                changed |= ui.add(egui::Slider::new(&mut o.step, 0.0..=0.5).text(tr("ослабление на кадр"))).changed();
                changed |= ui.checkbox(&mut o.tint, tr("Окрашивать")).changed();
                ui.horizontal(|ui| {
                    ui.label(tr("до"));
                    changed |= ui.color_edit_button_srgb(&mut o.color_before).changed();
                    ui.label(tr("после"));
                    changed |= ui.color_edit_button_srgb(&mut o.color_after).changed();
                });
            });
            if !open {
                self.timeline.onion_settings = false;
            }
            if changed {
                self.refresh_all();
                self.dirty = true;
            }
        }
        if !self.timeline.show {
            return;
        }
        let mut folders = Vec::new();
        anim_folders(&self.doc.layers, &mut Vec::new(), &mut folders);
        let mut open = true;
        let mut go: Option<u32> = None;
        let mut key_cmd: Option<(Vec<usize>, u32, Option<String>)> = None;
        let mut select: Option<Vec<usize>> = None;
        let mut settings_changed = false;
        let mut play_toggle = false;
        egui::Window::new(tr("Таймлайн")).id(egui::Id::new("timeline")).open(&mut open).default_width(640.0).default_pos([440.0, 600.0]).resizable(true).show(ctx, |ui| {
            let t = &mut self.doc.timeline;
            let playing = self.timeline.playing.is_some();
            ui.horizontal(|ui| {
                if ui.button("⏮").on_hover_text(tr("В начало")).clicked() {
                    go = Some(t.start);
                }
                if ui.button("◀").on_hover_text(tr("Предыдущий кадр")).clicked() {
                    go = Some(t.frame.saturating_sub(1));
                }
                if ui.button(if playing { "⏸" } else { "▶" }).on_hover_text(tr("Проиграть / стоп")).clicked() {
                    play_toggle = true;
                }
                if ui.button("▶|").on_hover_text(tr("Следующий кадр")).clicked() {
                    go = Some((t.frame + 1).min(t.frames - 1));
                }
                if ui.button("⏭").on_hover_text(tr("В конец")).clicked() {
                    go = Some(t.end);
                }
                settings_changed |= ui.checkbox(&mut t.looped, tr("По кругу")).changed();
                settings_changed |= ui.checkbox(&mut t.onion_on, "Onion skin").changed();
                settings_changed |= ui.checkbox(&mut t.enabled, tr("Таймлайн")).changed();
            });
            ui.horizontal(|ui| {
                ui.label(trf("Кадр {0} из {1}", &[&(t.frame + 1), &t.frames]));
                settings_changed |= ui.add(egui::DragValue::new(&mut t.fps).range(1..=120).suffix(" fps")).changed();
                ui.label(tr("кадров:"));
                settings_changed |= ui.add(egui::DragValue::new(&mut t.frames).range(1..=100_000)).changed();
                ui.label(tr("проигрывать:"));
                let (mut s, mut e) = (t.start + 1, t.end + 1);
                settings_changed |= ui.add(egui::DragValue::new(&mut s).range(1..=t.frames)).changed();
                ui.label("–");
                settings_changed |= ui.add(egui::DragValue::new(&mut e).range(1..=t.frames)).changed();
                t.start = s.saturating_sub(1);
                t.end = e.saturating_sub(1);
                t.clamp();
            });
            ui.separator();
            if folders.is_empty() {
                ui.label(tr("Нет папок анимации: «Анимация → Новая папка анимации»"));
                return;
            }
            let cell = 22.0f32;
            let name_w = 110.0f32;
            let frames = t.frames;
            let cur = t.frame;
            let (r0, r1) = (t.start, t.end);
            egui::ScrollArea::horizontal().show(ui, |ui| {
                // шкала с номерами
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(name_w + cell * frames as f32, 18.0), egui::Sense::click_and_drag());
                let p = ui.painter();
                for f in 0..frames {
                    let x = rect.left() + name_w + cell * f as f32;
                    let r = Rect::from_min_size(Pos2::new(x, rect.top()), Vec2::new(cell, 18.0));
                    if f == cur {
                        p.rect_filled(r, 0.0, Color32::from_rgb(120, 150, 230));
                    } else if f < r0 || f > r1 {
                        p.rect_filled(r, 0.0, Color32::from_gray(200));
                    }
                    if (f + 1) % 5 == 0 || f == 0 {
                        p.text(r.center(), egui::Align2::CENTER_CENTER, (f + 1).to_string(), egui::FontId::proportional(10.0), Color32::from_gray(40));
                    }
                }
                if let Some(pos) = resp.interact_pointer_pos().filter(|_| resp.clicked() || resp.dragged()) {
                    let f = ((pos.x - rect.left() - name_w) / cell).floor();
                    if f >= 0.0 {
                        go = Some((f as u32).min(frames - 1));
                    }
                }
                for path in &folders {
                    let Some(layer) = self.doc.layer(path) else { continue };
                    let Some(track) = layer.anim.as_deref() else { continue };
                    let cels: Vec<String> = layer.children.as_deref().unwrap_or(&[]).iter().map(|c| c.name.clone()).collect();
                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(name_w + cell * frames as f32, cell), egui::Sense::click());
                    let p = ui.painter();
                    let sel = self.selected.starts_with(path);
                    p.rect_filled(Rect::from_min_size(rect.min, Vec2::new(name_w - 2.0, cell)), 0.0, if sel { Color32::from_rgb(200, 210, 245) } else { Color32::from_gray(230) });
                    p.text(rect.min + Vec2::new(4.0, cell / 2.0), egui::Align2::LEFT_CENTER, &layer.name, egui::FontId::proportional(12.0), Color32::from_gray(20));
                    for f in 0..frames {
                        let x = rect.left() + name_w + cell * f as f32;
                        let r = Rect::from_min_size(Pos2::new(x, rect.top()), Vec2::new(cell, cell)).shrink(1.0);
                        let key = track.keys.get(&f);
                        let shown = track.cel_at(f);
                        let fill = match (key, shown) {
                            (Some(_), Some(_)) => Color32::from_rgb(250, 220, 150),
                            (None, Some(_)) => Color32::from_rgb(252, 238, 205),
                            _ => Color32::from_gray(245),
                        };
                        p.rect_filled(r, 2.0, fill);
                        if f == cur {
                            p.rect_stroke(r, 2.0, egui::Stroke::new(1.5_f32, Color32::from_rgb(60, 90, 200)));
                        }
                        match key {
                            Some(c) if !c.is_empty() => {
                                p.text(r.center(), egui::Align2::CENTER_CENTER, c, egui::FontId::proportional(10.0), Color32::from_gray(20));
                            }
                            Some(_) => {
                                p.text(r.center(), egui::Align2::CENTER_CENTER, "×", egui::FontId::proportional(11.0), Color32::from_gray(120));
                            }
                            None => {}
                        }
                    }
                    let hit = resp.interact_pointer_pos().map(|pos| ((pos.x - rect.left() - name_w) / cell).floor());
                    if resp.clicked() {
                        match hit {
                            Some(f) if f >= 0.0 => go = Some((f as u32).min(frames - 1)),
                            _ => select = Some(path.clone()),
                        }
                    }
                    // кадр, по которому открыли меню: запоминается при щелчке, иначе меню закроется,
                    // как только кнопку отпустят (позиции указателя уже нет)
                    let menu_id = resp.id.with("menu_frame");
                    if resp.secondary_clicked() {
                        let f = hit.filter(|f| *f >= 0.0).map(|f| (f as u32).min(frames - 1));
                        ui.ctx().data_mut(|d| d.insert_temp(menu_id, f));
                    }
                    if let Some(f) = ui.ctx().data(|d| d.get_temp::<Option<u32>>(menu_id)).flatten() {
                        resp.context_menu(|ui| {
                            ui.label(trf("Кадр {0}", &[&(f + 1)]));
                            for c in &cels {
                                if ui.button(trf("Показать «{0}»", &[c])).clicked() {
                                    key_cmd = Some((path.clone(), f, Some(c.clone())));
                                    ui.close_menu();
                                }
                            }
                            ui.separator();
                            if ui.button(tr("Пустой кадр")).clicked() {
                                key_cmd = Some((path.clone(), f, Some(String::new())));
                                ui.close_menu();
                            }
                            if ui.add_enabled(track.keys.contains_key(&f), egui::Button::new(tr("Убрать метку"))).clicked() {
                                key_cmd = Some((path.clone(), f, None));
                                ui.close_menu();
                            }
                        });
                    }
                }
            });
            ui.horizontal(|ui| {
                if ui.button(tr("Новый кадр (cel)")).clicked() {
                    key_cmd = Some((Vec::new(), u32::MAX, None));
                }
                ui.label(egui::RichText::new(tr("Правый щелчок по клетке — какой cel показывать с этого кадра")).small());
            });
        });
        if !open {
            self.timeline.show = false;
        }
        if play_toggle {
            self.timeline.playing = match self.timeline.playing {
                Some(_) => None,
                None => {
                    let t = &self.doc.timeline;
                    let f = if t.frame < t.start || t.frame >= t.end { t.start } else { t.frame };
                    self.doc.timeline.enabled = true;
                    Some((Instant::now(), f))
                }
            };
        }
        if settings_changed {
            self.refresh_all();
            self.dirty = true;
        }
        if let Some(p) = select {
            self.selected = p;
        }
        match key_cmd {
            Some((_, u32::MAX, None)) => self.new_cel(),
            Some((path, f, cel)) => self.set_key(&path, f, cel),
            None => {}
        }
        if let Some(f) = go {
            self.timeline.playing = None;
            self.set_frame(f);
        }
    }
}
