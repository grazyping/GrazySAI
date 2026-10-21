//! Вкладка «Плагин» мастерской: `plugin.txt` и `main.lua` в редакторе, проверка на пробной
//! картинке (до и после, журнал, ошибка с номером строки), пакет `.gsplugin` и «Экспорт в
//! GrazySAI» (папка `plugins/dev/` и отметка `.changed`: открытая рисовалка подхватит плагин).

use crate::i18n::{tr, trf};
use eframe::egui::{self, Color32, Pos2, Rect, Vec2};
use std::path::PathBuf;

/// Заготовки — примеры из репозитория.
const TEMPLATES: [(&str, &str, &str); 4] = [
    ("invert", include_str!("../../../plugins/examples/invert/plugin.txt"), include_str!("../../../plugins/examples/invert/main.lua")),
    ("grid", include_str!("../../../plugins/examples/grid/plugin.txt"), include_str!("../../../plugins/examples/grid/main.lua")),
    ("noise", include_str!("../../../plugins/examples/noise/plugin.txt"), include_str!("../../../plugins/examples/noise/main.lua")),
    ("jitter", include_str!("../../../plugins/examples/jitter/plugin.txt"), include_str!("../../../plugins/examples/jitter/main.lua")),
];

/// Пробная картинка.
const TEST_W: u32 = 320;
const TEST_H: u32 = 200;

pub struct PluginTab {
    pub manifest: String,
    pub source: String,
    pub path: Option<PathBuf>,
    /// Разобранный плагин или ошибка разбора.
    parsed: Result<paint_plugin::Plugin, String>,
    /// Выбранная команда и значения её полей.
    command: usize,
    values: Vec<(String, f64)>,
    before: Option<egui::TextureHandle>,
    after: Option<egui::TextureHandle>,
    pub log: Vec<String>,
    pub error: Option<String>,
    pub status: String,
}

impl PluginTab {
    pub fn new() -> Self {
        let mut t = Self {
            manifest: TEMPLATES[0].1.to_string(),
            source: TEMPLATES[0].2.to_string(),
            path: None,
            parsed: Err(String::new()),
            command: 0,
            values: Vec::new(),
            before: None,
            after: None,
            log: Vec::new(),
            error: None,
            status: String::new(),
        };
        t.reparse();
        t
    }

    /// Плагин из редактора, если он разобрался.
    pub fn current(&self) -> Option<&paint_plugin::Plugin> {
        self.parsed.as_ref().ok()
    }

    fn reparse(&mut self) {
        self.parsed = paint_plugin::parse(&self.manifest, &self.source).map_err(|e| e.to_string());
        if let Some(c) = self.parsed.as_ref().ok().filter(|p| !p.commands.is_empty()).map(|p| {
            self.command = self.command.min(p.commands.len() - 1);
            &p.commands[self.command]
        }) {
            // значения полей сохраняются, пока поле с тем же id есть
            let old = std::mem::take(&mut self.values);
            self.values = c.params.iter().map(|q| (q.id.clone(), old.iter().find(|(k, _)| *k == q.id).map_or(q.default, |(_, v)| *v))).collect();
        }
    }

    /// Пробная картинка: цветной переход, круги и прозрачный угол.
    fn test_canvas() -> paint_plugin::MemoryCanvas {
        let mut c = paint_plugin::MemoryCanvas::new(TEST_W, TEST_H, [0, 0, 0, 0]);
        for y in 0..TEST_H as i32 {
            for x in 0..TEST_W as i32 {
                let (fx, fy) = (x as f32 / TEST_W as f32, y as f32 / TEST_H as f32);
                let mut px = [(255.0 * fx) as u8, (200.0 * fy + 30.0) as u8, (255.0 * (1.0 - fx)) as u8, 255];
                for (cx, cy, r, col) in [(90.0, 100.0, 55.0, [250, 220, 40]), (220.0, 80.0, 40.0, [30, 30, 40]), (240.0, 150.0, 30.0, [250, 250, 250])] {
                    if ((x as f32 - cx).powi(2) + (y as f32 - cy).powi(2)).sqrt() < r {
                        px = [col[0], col[1], col[2], 255];
                    }
                }
                if x > 280 && y > 160 {
                    px = [0, 0, 0, 0];
                }
                paint_plugin::Canvas::set(&mut c, x, y, px);
            }
        }
        c.color = [200, 40, 60];
        c
    }

    fn texture(ctx: &egui::Context, name: &str, rgba: &[u8]) -> egui::TextureHandle {
        ctx.load_texture(name, egui::ColorImage::from_rgba_unmultiplied([TEST_W as usize, TEST_H as usize], rgba), egui::TextureOptions::NEAREST)
    }

    /// Выполнить выбранную команду на пробной картинке.
    fn test(&mut self, ctx: &egui::Context) {
        self.reparse();
        let p = match &self.parsed {
            Ok(p) => p.clone(),
            Err(e) => {
                self.error = Some(e.clone());
                return;
            }
        };
        let mut c = Self::test_canvas();
        self.before = Some(Self::texture(ctx, "before", &c.flatten()));
        let r = paint_plugin::run(&p, &p.commands[self.command], &self.values, &mut c, paint_plugin::Limits { time: std::time::Duration::from_secs(20), ..Default::default() });
        self.after = Some(Self::texture(ctx, "after", &c.flatten()));
        match r {
            Ok(rep) => {
                self.error = None;
                self.log = rep.log;
                self.status = trf("Проверка прошла за {0} мс; слоёв: {1}", &[&rep.elapsed.as_millis(), &c.layers.len()]);
            }
            Err(e) => {
                self.error = Some(e);
                self.status = tr("Ошибка — см. ниже").to_string();
            }
        }
    }

    /// Собрать пакет (с проверкой разбора).
    pub fn save_to(&mut self, path: PathBuf) {
        self.reparse();
        if let Err(e) = &self.parsed {
            self.status = e.clone();
            return;
        }
        let path = crate::files::with_ext(path, "gsplugin");
        self.status = match paint_plugin::pack(&path, &self.manifest, &self.source) {
            Ok(()) => {
                self.path = Some(path.clone());
                trf("Сохранено: {0}", &[&path.display()])
            }
            Err(e) => trf("Не удалось сохранить {0}: {1}", &[&path.display(), &e]),
        };
    }

    pub fn open(&mut self, path: PathBuf) {
        let files = match paint_formats::pack::read(&path) {
            Ok(f) => f,
            Err(e) => {
                self.status = trf("Плагин не открылся: {0}", &[&e]);
                return;
            }
        };
        let text = |name: &str| files.iter().find(|(n, _)| n == name).map(|(_, d)| String::from_utf8_lossy(d).into_owned());
        match (text("plugin.txt"), text("main.lua")) {
            (Some(m), Some(s)) => {
                self.manifest = m;
                self.source = s;
                self.path = Some(path);
                self.reparse();
                self.status = tr("Плагин открыт").to_string();
            }
            _ => self.status = trf("Плагин не открылся: {0}", &[&"plugin.txt / main.lua"]),
        }
    }

    /// В `plugins/dev/` рисовалки и отметка — плагин появится в её меню «Плагины».
    fn export(&mut self) {
        self.reparse();
        let p = match &self.parsed {
            Ok(p) => p.clone(),
            Err(e) => {
                self.status = e.clone();
                return;
            }
        };
        let Some(dir) = crate::settings::dir().map(|d| d.join("plugins").join("dev")) else {
            self.status = tr("Папка настроек GrazySAI не найдена").to_string();
            return;
        };
        let path = dir.join(format!("{}.gsplugin", p.id));
        let r = std::fs::create_dir_all(&dir)
            .map_err(|e| e.to_string())
            .and_then(|_| paint_plugin::pack(&path, &self.manifest, &self.source).map_err(|e| e.to_string()))
            .and_then(|_| std::fs::write(dir.join(".changed"), format!("{:?}", std::time::SystemTime::now())).map_err(|e| e.to_string()));
        self.status = match r {
            Ok(()) => trf("Плагин «{0}» отправлен в GrazySAI: меню «Плагины»", &[&p.name]),
            Err(e) => trf("Не удалось сохранить {0}: {1}", &[&path.display(), &e]),
        };
    }

    /// Кнопки вкладки (в верхней строке мастерской). Возвращает, что спросить у окна файлов.
    pub fn bar(&mut self, ui: &mut egui::Ui) -> Option<crate::Want> {
        let mut want = None;
        ui.menu_button(tr("Новый из примера"), |ui| {
            for (name, m, s) in TEMPLATES {
                if ui.button(name).clicked() {
                    self.manifest = m.to_string();
                    self.source = s.to_string();
                    self.path = None;
                    self.reparse();
                    ui.close_menu();
                }
            }
        });
        if ui.button(tr("Открыть…")).clicked() {
            want = Some(crate::Want::OpenPlugin);
        }
        if ui.button(tr("Сохранить")).clicked() {
            match self.path.clone() {
                Some(p) => self.save_to(p),
                None => want = Some(crate::Want::SavePlugin),
            }
        }
        if ui.button(tr("Сохранить как…")).clicked() {
            want = Some(crate::Want::SavePlugin);
        }
        ui.separator();
        if ui.button(egui::RichText::new(tr("Экспорт в GrazySAI")).strong()).on_hover_text(tr("Плагин сразу появится в меню «Плагины» открытой рисовалки (папка plugins/dev)")).clicked() {
            self.export();
        }
        want
    }

    pub fn ui(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("plugin_test").default_width(420.0).show(ctx, |ui| {
            ui.heading(tr("Проверка"));
            if let Ok(p) = &self.parsed {
                if !p.dynamics.is_empty() {
                    ui.label(tr("Своя динамика кисти: выберите её на вкладке «Кисть» → «Динамика» → «Из плагина» — образец мазка покажет её сразу."));
                }
            }
            match &self.parsed {
                Ok(p) if !p.commands.is_empty() => {
                    let p = p.clone();
                    ui.label(format!("{} · {} {}", p.name, p.id, p.version));
                    egui::ComboBox::from_label(tr("Команда")).selected_text(p.commands.get(self.command).map(|c| c.menu.clone()).unwrap_or_default()).show_ui(ui, |ui| {
                        for (i, c) in p.commands.iter().enumerate() {
                            if ui.selectable_label(i == self.command, &c.menu).clicked() {
                                self.command = i;
                            }
                        }
                    });
                    if let Some(c) = p.commands.get(self.command) {
                        if self.values.len() != c.params.len() {
                            self.reparse();
                        }
                        ui.spacing_mut().slider_width = 180.0;
                        for (q, (_, v)) in c.params.iter().zip(self.values.iter_mut()) {
                            match &q.kind {
                                paint_plugin::ParamKind::Slider { min, max, step } => {
                                    let mut s = egui::Slider::new(v, *min..=*max).text(&q.label);
                                    if *step > 0.0 {
                                        s = s.step_by(*step);
                                    }
                                    ui.add(s);
                                }
                                paint_plugin::ParamKind::Check => {
                                    let mut b = *v >= 0.5;
                                    if ui.checkbox(&mut b, &q.label).changed() {
                                        *v = b as u8 as f64;
                                    }
                                }
                                paint_plugin::ParamKind::Choice(opts) => {
                                    let cur = (*v as usize).min(opts.len().saturating_sub(1));
                                    egui::ComboBox::from_label(&q.label).selected_text(opts.get(cur).cloned().unwrap_or_default()).show_ui(ui, |ui| {
                                        for (i, o) in opts.iter().enumerate() {
                                            if ui.selectable_label(i == cur, o).clicked() {
                                                *v = i as f64;
                                            }
                                        }
                                    });
                                }
                            }
                        }
                    }
                }
                Ok(_) => {}
                Err(e) if !e.is_empty() => {
                    ui.colored_label(Color32::from_rgb(200, 50, 50), e);
                }
                Err(_) => {}
            }
            if ui.button(egui::RichText::new(tr("▶ Проверить на пробной картинке")).strong()).clicked() {
                self.test(ctx);
            }
            let w = ui.available_width().min(TEST_W as f32);
            let size = Vec2::new(w, w * TEST_H as f32 / TEST_W as f32);
            for (label, t) in [(tr("До"), &self.before), (tr("После"), &self.after)] {
                if let Some(t) = t {
                    ui.label(label);
                    let (r, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                    ui.painter().image(t.id(), r, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                }
            }
            if let Some(e) = &self.error {
                ui.colored_label(Color32::from_rgb(200, 50, 50), e);
            }
            if !self.log.is_empty() {
                ui.label(tr("Журнал:"));
                egui::ScrollArea::vertical().max_height(140.0).show(ui, |ui| {
                    for l in &self.log {
                        ui.monospace(l);
                    }
                });
            }
            ui.separator();
            ui.hyperlink_to(tr("Как писать плагины (docs/36-plugins.md)"), "https://github.com/grazyping/GrazySAI/blob/main/docs/36-plugins.md");
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            let mut changed = false;
            ui.label("plugin.txt");
            changed |= ui.add(egui::TextEdit::multiline(&mut self.manifest).code_editor().desired_rows(9).desired_width(f32::INFINITY)).changed();
            ui.label("main.lua");
            egui::ScrollArea::vertical().id_salt("src").show(ui, |ui| {
                changed |= ui.add(egui::TextEdit::multiline(&mut self.source).code_editor().desired_rows(26).desired_width(f32::INFINITY)).changed();
            });
            if changed {
                self.reparse();
            }
        });
    }
}
