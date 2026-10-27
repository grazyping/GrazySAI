//! Своя настройка интерфейса (как в Clip Studio, Krita, Photoshop): поиск по всем командам
//! (Ctrl+Shift+P), панель «Быстрый доступ» с любыми командами (несколько наборов) и рабочие
//! пространства — сохранённые раскладки панелей.

use crate::actions::FILTERS;
use crate::i18n::{tr, trf};
use crate::{filter_title, keys, menu_icons, FilterKind, PaintApp};
use eframe::egui;

/// Команда для поиска и быстрого доступа.
#[derive(Clone, Debug, PartialEq)]
pub enum Cmd {
    Key(keys::Action),
    Filter(FilterKind),
    /// Новый слой коррекции (номер вида в `layer_style::kinds`).
    Adjust(usize),
    /// Окно: 0 — стиль и коррекция слоя, 1 — автодействия, 2 — менеджер ресурсов, 3 — быстрые клавиши.
    Window(u8),
    /// Автодействие: набор и имя.
    Auto(String, String),
}

const WINDOWS: [&str; 4] = ["Стиль и коррекция слоя…", "Автодействия…", "Менеджер ресурсов…", "Быстрые клавиши…"];

impl Cmd {
    pub fn id(&self) -> String {
        match self {
            Cmd::Key(a) => format!("key:{}", keys::ALL.iter().find(|x| x.0 == *a).map_or("", |x| x.1)),
            Cmd::Filter(k) => format!("filter:{k:?}"),
            Cmd::Adjust(i) => format!("adjust:{i}"),
            Cmd::Window(w) => format!("win:{w}"),
            Cmd::Auto(s, n) => format!("auto:{s}\u{1e}{n}"),
        }
    }

    pub fn from_id(id: &str) -> Option<Cmd> {
        let (kind, rest) = id.split_once(':')?;
        Some(match kind {
            "key" => Cmd::Key(keys::ALL.iter().find(|x| x.1 == rest)?.0),
            "filter" => Cmd::Filter(*FILTERS.iter().find(|k| format!("{k:?}") == rest)?),
            "adjust" => Cmd::Adjust(rest.parse().ok().filter(|i| *i < crate::layer_style::kinds().len())?),
            "win" => Cmd::Window(rest.parse().ok().filter(|i| (*i as usize) < WINDOWS.len())?),
            "auto" => {
                let (s, n) = rest.split_once('\u{1e}')?;
                Cmd::Auto(s.to_string(), n.to_string())
            }
            _ => return None,
        })
    }

    /// Подпись (со знаком из меню, если есть).
    pub fn label(&self) -> (Option<&'static str>, String) {
        match self {
            Cmd::Key(a) => {
                let l = keys::ALL.iter().find(|x| x.0 == *a).map_or("", |x| x.2);
                (menu_icons::icon(l).or_else(|| menu_icons::icon(&format!("{l}…"))), tr(l).to_string())
            }
            Cmd::Filter(k) => (None, trf("Фильтр: {0}", &[&filter_title(*k)])),
            Cmd::Adjust(i) => (menu_icons::icon("Новый слой коррекции"), trf("Слой коррекции: {0}", &[&crate::layer_style::adjust_name(&crate::layer_style::kinds()[*i])])),
            Cmd::Window(w) => (menu_icons::icon(WINDOWS[*w as usize]), tr(WINDOWS[*w as usize]).to_string()),
            Cmd::Auto(_, n) => (menu_icons::icon("Автодействия…"), trf("Действие: {0}", &[n])),
        }
    }
}

/// Набор быстрого доступа.
#[derive(Clone, Debug, PartialEq)]
pub struct QuickSet {
    pub name: String,
    pub cmds: Vec<String>,
}

/// Состояние окон настройки.
pub struct CustomUi {
    pub quick: Vec<QuickSet>,
    pub quick_set: usize,
    pub show_quick: bool,
    /// Поиск команд: строка, выбранная строка, добавлять ли в быстрый доступ.
    pub palette: Option<(String, usize, bool)>,
    /// Рабочие пространства: имя и раскладка.
    pub workspaces: Vec<(String, String)>,
    /// Имя для «Сохранить рабочее пространство…».
    pub ws_name: Option<String>,
    /// Команда клавиш, выбранная в поиске: сработает в следующем кадре, как нажатие.
    pub forced: Option<keys::Action>,
}

impl Default for CustomUi {
    fn default() -> Self {
        let ids = ["key:undo", "key:redo", "key:select_all", "key:deselect", "key:fill", "filter:GaussianBlur", "adjust:0", "win:0"];
        Self {
            quick: vec![QuickSet { name: tr("Основной").to_string(), cmds: ids.iter().map(|s| s.to_string()).collect() }],
            quick_set: 0,
            show_quick: false,
            palette: None,
            workspaces: Vec::new(),
            ws_name: None,
            forced: None,
        }
    }
}

impl CustomUi {
    pub fn save(&self, s: &mut crate::settings::Settings) {
        s.set("quick.count", self.quick.len());
        for (i, q) in self.quick.iter().enumerate() {
            s.set(&format!("quick.{i}.name"), &q.name);
            s.set(&format!("quick.{i}.cmds"), q.cmds.join("\u{1f}"));
        }
        s.set("quick.set", self.quick_set);
        s.set("quick.show", self.show_quick as u8);
        s.set("ws.count", self.workspaces.len());
        for (i, (n, d)) in self.workspaces.iter().enumerate() {
            s.set(&format!("ws.{i}.name"), n);
            s.set(&format!("ws.{i}.data"), d);
        }
    }

    pub fn load(&mut self, s: &crate::settings::Settings) {
        if let Some(n) = s.f32("quick.count").map(|v| v as usize).filter(|&n| n > 0 && n < 100) {
            self.quick = (0..n)
                .map(|i| QuickSet {
                    name: s.str(&format!("quick.{i}.name")).unwrap_or("").to_string(),
                    cmds: s.str(&format!("quick.{i}.cmds")).unwrap_or("").split('\u{1f}').filter(|x| !x.is_empty()).map(str::to_string).collect(),
                })
                .collect();
        }
        if let Some(v) = s.f32("quick.set") {
            self.quick_set = (v as usize).min(self.quick.len().saturating_sub(1));
        }
        s.read_bool("quick.show", &mut self.show_quick);
        if let Some(n) = s.f32("ws.count").map(|v| v as usize).filter(|&n| n < 100) {
            self.workspaces = (0..n).filter_map(|i| Some((s.str(&format!("ws.{i}.name"))?.to_string(), s.str(&format!("ws.{i}.data"))?.to_string()))).collect();
        }
    }
}

impl PaintApp {
    /// Все команды для поиска.
    fn all_commands(&self) -> Vec<Cmd> {
        let mut v: Vec<Cmd> = keys::ALL.iter().map(|x| Cmd::Key(x.0)).collect();
        v.extend(FILTERS.iter().map(|k| Cmd::Filter(*k)));
        v.extend((0..crate::layer_style::kinds().len()).map(Cmd::Adjust));
        v.extend((0..WINDOWS.len() as u8).map(Cmd::Window));
        for s in &self.actions.sets {
            v.extend(s.actions.iter().map(|a| Cmd::Auto(s.name.clone(), a.name.clone())));
        }
        v
    }

    pub(crate) fn run_cmd(&mut self, c: &Cmd) {
        match c {
            Cmd::Key(a) => self.custom.forced = Some(*a),
            Cmd::Filter(k) => self.open_filter(*k),
            Cmd::Adjust(i) => {
                if let Some(a) = crate::layer_style::kinds().into_iter().nth(*i) {
                    self.add_adjust_layer(a);
                }
            }
            Cmd::Window(w) => match w {
                0 => self.show_style = true,
                1 => self.show_actions = true,
                2 => self.assets = Some(Default::default()),
                _ => self.show_keys = true,
            },
            Cmd::Auto(s, n) => {
                if !self.actions.loaded {
                    self.actions.sets = crate::actions::load();
                    self.actions.loaded = true;
                }
                let found = self.actions.sets.iter().enumerate().find_map(|(si, set)| (set.name == *s).then(|| set.actions.iter().position(|a| a.name == *n).map(|ai| (si, ai))).flatten());
                match found {
                    Some((si, ai)) => self.play_action(si, ai, 0),
                    None => self.status = trf("Действие «{0}» не найдено", &[n]),
                }
            }
        }
    }

    /// Раскладка панелей одной строкой (для рабочего пространства).
    fn layout_text(&self) -> String {
        let w = &self.win;
        let b = |v: bool| if v { '1' } else { '0' };
        format!(
            "{} {} {} {} {} {} {} {} {} {} {} {} {} {} {}",
            w.ui_scale,
            b(w.tools_right),
            b(w.layers_right),
            b(w.dot_cursor),
            b(w.brush_circle),
            b(w.size_numbers),
            b(w.size_list_top),
            w.hsv_mode,
            w.swatch_size,
            w.show.iter().map(|&v| b(v)).collect::<String>(),
            w.separate.iter().map(|&v| b(v)).collect::<String>(),
            b(self.dark_theme),
            b(self.custom.show_quick),
            b(w.dock),
            // в тексте JSON нет пробелов: имена вкладок — латиница
            self.dock_text(),
        )
    }

    fn apply_layout(&mut self, t: &str) {
        let p: Vec<&str> = t.split_ascii_whitespace().collect();
        if p.len() < 13 {
            return;
        }
        let one = |s: &str| s == "1";
        let w = &mut self.win;
        w.ui_scale = p[0].parse::<f32>().unwrap_or(1.0).clamp(1.0, 2.0);
        w.tools_right = one(p[1]);
        w.layers_right = one(p[2]);
        w.dot_cursor = one(p[3]);
        w.brush_circle = one(p[4]);
        w.size_numbers = one(p[5]);
        w.size_list_top = one(p[6]);
        w.hsv_mode = p[7].parse::<u8>().unwrap_or(0).min(2);
        w.swatch_size = p[8].parse::<u8>().unwrap_or(2).min(2);
        for (i, c) in p[9].chars().take(w.show.len()).enumerate() {
            w.show[i] = c == '1';
        }
        for (i, c) in p[10].chars().take(w.separate.len()).enumerate() {
            w.separate[i] = c == '1';
        }
        self.dark_theme = one(p[11]);
        crate::sai_ui::set_dark(self.dark_theme);
        self.custom.show_quick = one(p[12]);
        // пространства, сохранённые до стыкуемых панелей, раскладку вкладок не меняют
        if let (Some(d), Some(t)) = (p.get(13), p.get(14).and_then(|t| crate::dock::parse(t))) {
            self.win.dock = one(d);
            self.dock = t;
        }
    }

    /// Подменю «Окно → Рабочее пространство».
    pub(crate) fn workspace_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button(menu_icons::label("Рабочее пространство"), |ui| {
            let mut apply = None;
            for (i, (n, _)) in self.custom.workspaces.iter().enumerate() {
                if ui.button(n).clicked() {
                    apply = Some(i);
                    ui.close_menu();
                }
            }
            if let Some(i) = apply {
                let t = self.custom.workspaces[i].1.clone();
                self.apply_layout(&t);
                self.save_settings();
            }
            if !self.custom.workspaces.is_empty() {
                ui.separator();
            }
            if ui.button(tr("Сохранить текущее…")).clicked() {
                self.custom.ws_name = Some(trf("Пространство {0}", &[&(self.custom.workspaces.len() + 1)]));
                ui.close_menu();
            }
            if !self.custom.workspaces.is_empty() {
                ui.menu_button(tr("Удалить"), |ui| {
                    let mut del = None;
                    for (i, (n, _)) in self.custom.workspaces.iter().enumerate() {
                        if ui.button(n).clicked() {
                            del = Some(i);
                            ui.close_menu();
                        }
                    }
                    if let Some(i) = del {
                        self.custom.workspaces.remove(i);
                        self.save_settings();
                    }
                });
            }
        });
    }

    pub(crate) fn customize_windows(&mut self, ctx: &egui::Context) {
        // имя нового рабочего пространства
        if let Some(mut name) = self.custom.ws_name.take() {
            let mut keep = true;
            let mut ok = false;
            egui::Window::new(tr("Сохранить рабочее пространство")).collapsible(false).resizable(false).show(ctx, |ui| {
                let r = ui.text_edit_singleline(&mut name);
                r.request_focus();
                ui.horizontal(|ui| {
                    ok = ui.button("OK").clicked() || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                    if ui.button(tr("Отмена")).clicked() {
                        keep = false;
                    }
                });
            });
            if ok && !name.trim().is_empty() {
                let data = self.layout_text();
                let n = name.trim().to_string();
                match self.custom.workspaces.iter_mut().find(|w| w.0 == n) {
                    Some(w) => w.1 = data,
                    None => self.custom.workspaces.push((n, data)),
                }
                self.save_settings();
            } else if keep && !ok {
                self.custom.ws_name = Some(name);
            }
        }
        self.palette_window(ctx);
        self.quick_window(ctx);
    }

    fn palette_window(&mut self, ctx: &egui::Context) {
        let Some((mut q, mut sel, add)) = self.custom.palette.take() else { return };
        let all = self.all_commands();
        let ql = q.to_lowercase();
        let found: Vec<(Cmd, Option<&str>, String)> = all
            .into_iter()
            .map(|c| {
                let (i, l) = c.label();
                (c, i, l)
            })
            .filter(|(_, _, l)| ql.split_whitespace().all(|w| l.to_lowercase().contains(w)))
            .collect();
        sel = sel.min(found.len().saturating_sub(1));
        let mut run: Option<Cmd> = None;
        let mut close = false;
        let title = if add { tr("Добавить в быстрый доступ") } else { tr("Поиск команд") };
        egui::Window::new(title).id(egui::Id::new("cmd_palette")).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_TOP, [0.0, 60.0]).fixed_size([420.0, 360.0]).show(ctx, |ui| {
            let r = ui.add(egui::TextEdit::singleline(&mut q).hint_text(tr("Начните вводить название команды")).desired_width(400.0));
            r.request_focus();
            if r.changed() {
                sel = 0;
            }
            let (down, up, enter, esc) = ui.input(|i| (i.key_pressed(egui::Key::ArrowDown), i.key_pressed(egui::Key::ArrowUp), i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape)));
            if down {
                sel = (sel + 1).min(found.len().saturating_sub(1));
            }
            if up {
                sel = sel.saturating_sub(1);
            }
            if esc {
                close = true;
            }
            if enter {
                run = found.get(sel).map(|f| f.0.clone());
            }
            egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                for (k, (c, icon, label)) in found.iter().enumerate().take(200) {
                    let text = menu_icons::with_icon(*icon, label);
                    let r = ui.selectable_label(k == sel, text);
                    if k == sel && (down || up) {
                        r.scroll_to_me(None);
                    }
                    if r.clicked() {
                        run = Some(c.clone());
                    }
                }
                if found.is_empty() {
                    ui.label(tr("Ничего не найдено"));
                }
            });
        });
        match run {
            Some(c) if add => {
                let id = c.id();
                if let Some(set) = self.custom.quick.get_mut(self.custom.quick_set) {
                    if !set.cmds.contains(&id) {
                        set.cmds.push(id);
                    }
                }
                self.save_settings();
            }
            Some(c) => self.run_cmd(&c),
            None if !close => self.custom.palette = Some((q, sel, add)),
            None => {}
        }
    }

    fn quick_window(&mut self, ctx: &egui::Context) {
        if !self.custom.show_quick {
            return;
        }
        let mut open = true;
        let mut run: Option<Cmd> = None;
        let mut changed = false;
        let mut add = false;
        egui::Window::new(tr("Быстрый доступ")).id(egui::Id::new("quick_access")).open(&mut open).default_width(240.0).resizable(true).show(ctx, |ui| {
            let c = &mut self.custom;
            ui.horizontal(|ui| {
                let cur = c.quick.get(c.quick_set).map(|s| s.name.clone()).unwrap_or_default();
                egui::ComboBox::from_id_salt("quick_set").selected_text(cur).width(120.0).show_ui(ui, |ui| {
                    for (i, s) in c.quick.iter().enumerate() {
                        if ui.selectable_label(i == c.quick_set, &s.name).clicked() {
                            c.quick_set = i;
                            changed = true;
                        }
                    }
                });
                if ui.small_button("+").on_hover_text(tr("Добавить команду")).clicked() {
                    add = true;
                }
                ui.menu_button("…", |ui| {
                    if ui.button(tr("Новый набор")).clicked() {
                        c.quick.push(QuickSet { name: trf("Набор {0}", &[&(c.quick.len() + 1)]), cmds: Vec::new() });
                        c.quick_set = c.quick.len() - 1;
                        changed = true;
                        ui.close_menu();
                    }
                    if ui.add_enabled(c.quick.len() > 1, egui::Button::new(tr("Удалить набор"))).clicked() {
                        c.quick.remove(c.quick_set);
                        c.quick_set = 0;
                        changed = true;
                        ui.close_menu();
                    }
                    if let Some(s) = c.quick.get_mut(c.quick_set) {
                        ui.label(tr("Имя набора:"));
                        changed |= ui.text_edit_singleline(&mut s.name).changed();
                    }
                });
            });
            ui.separator();
            let Some(set) = c.quick.get_mut(c.quick_set) else { return };
            let mut remove = None;
            let mut swap = None;
            let n = set.cmds.len();
            ui.horizontal_wrapped(|ui| {
                for (k, id) in set.cmds.iter().enumerate() {
                    let Some(cmd) = Cmd::from_id(id) else { continue };
                    let (icon, label) = cmd.label();
                    let r = ui.button(menu_icons::with_icon(icon, &label));
                    if r.clicked() {
                        run = Some(cmd.clone());
                    }
                    r.context_menu(|ui| {
                        if ui.add_enabled(k > 0, egui::Button::new(tr("Левее"))).clicked() {
                            swap = Some((k, k - 1));
                            ui.close_menu();
                        }
                        if ui.add_enabled(k + 1 < n, egui::Button::new(tr("Правее"))).clicked() {
                            swap = Some((k, k + 1));
                            ui.close_menu();
                        }
                        if ui.button(tr("Убрать")).clicked() {
                            remove = Some(k);
                            ui.close_menu();
                        }
                    });
                }
            });
            if set.cmds.is_empty() {
                ui.label(egui::RichText::new(tr("Пусто: «+» добавляет любую команду")).small());
            }
            if let Some((a, b)) = swap {
                set.cmds.swap(a, b);
                changed = true;
            }
            if let Some(k) = remove {
                set.cmds.remove(k);
                changed = true;
            }
        });
        if !open {
            self.custom.show_quick = false;
            changed = true;
        }
        if add {
            self.custom.palette = Some((String::new(), 0, true));
        }
        if changed {
            self.save_settings();
        }
        if let Some(c) = run {
            self.run_cmd(&c);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_ids_round_trip() {
        let mut cmds: Vec<Cmd> = keys::ALL.iter().map(|x| Cmd::Key(x.0)).collect();
        cmds.extend(FILTERS.iter().map(|k| Cmd::Filter(*k)));
        cmds.extend((0..crate::layer_style::kinds().len()).map(Cmd::Adjust));
        cmds.extend((0..WINDOWS.len() as u8).map(Cmd::Window));
        cmds.push(Cmd::Auto("Мои действия".into(), "Действие 1".into()));
        for c in cmds {
            assert_eq!(Cmd::from_id(&c.id()), Some(c.clone()), "{}", c.id());
        }
        assert_eq!(Cmd::from_id("key:нет"), None);
        assert!(CustomUi::default().quick[0].cmds.iter().all(|id| Cmd::from_id(id).is_some()));
    }
}
