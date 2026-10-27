//! Стыкуемые панели, как в Clip Studio: холст и панели — вкладки egui_dock; вкладку можно
//! оторвать в плавающее окно и пристыковать к любому краю или к другой вкладке.
//! Что показано, по-прежнему решает «Окно → Показать» (`win.show`): дерево вкладок подгоняется
//! под него каждый кадр.

use crate::i18n::tr;
use crate::PaintApp;
use eframe::egui;
use egui_dock::{DockArea, DockState, NodeIndex, TabViewer};

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) enum Tab {
    Canvas,
    Navigator,
    Layers,
    Color,
    Tools,
    Timeline,
}

const PANELS: [Tab; 4] = [Tab::Navigator, Tab::Layers, Tab::Color, Tab::Tools];

impl Tab {
    /// Номер в `win.show` / `win.separate`.
    fn panel(self) -> Option<usize> {
        PANELS.iter().position(|&t| t == self)
    }

    /// Соседка по колонке SAI: навигатор над слоями, цвет над инструментами.
    fn partner(self) -> Tab {
        match self {
            Tab::Navigator => Tab::Layers,
            Tab::Layers => Tab::Navigator,
            Tab::Color => Tab::Tools,
            Tab::Tools => Tab::Color,
            Tab::Canvas | Tab::Timeline => self,
        }
    }
}

/// Раскладка как у SAI: навигатор и слои — с краю окна, цвет и инструменты — ближе к холсту.
pub(crate) fn default_layout(tools_right: bool, layers_right: bool) -> DockState<Tab> {
    let mut d = DockState::new(vec![Tab::Canvas]);
    let t = d.main_surface_mut();
    // доли — от ширины окна 1400 px: 236 и 186 px; доля — у левой (верхней) половины
    let [_, tools] = if tools_right { t.split_right(NodeIndex::root(), 0.8, vec![Tab::Color]) } else { t.split_left(NodeIndex::root(), 0.2, vec![Tab::Color]) };
    t.split_below(tools, 0.4, vec![Tab::Tools]);
    let [_, nav] = if layers_right { t.split_right(NodeIndex::root(), 0.86, vec![Tab::Navigator]) } else { t.split_left(NodeIndex::root(), 0.14, vec![Tab::Navigator]) };
    t.split_below(nav, 0.3, vec![Tab::Layers]);
    d
}

/// Что пишется в настройки: дерево вкладок и места плавающих окон (egui_dock 0.14 их не
/// записывает).
#[derive(serde::Serialize, serde::Deserialize)]
struct Saved {
    dock: DockState<Tab>,
    windows: Vec<(usize, [f32; 4])>,
}

/// Так egui_dock 0.14 называет окно поверхности `i`.
fn window_id(i: usize) -> egui::Id {
    egui::Id::new(format!("window {:?}", egui_dock::SurfaceIndex(i)))
}

/// Раскладка из настроек; испорченная или без холста — `None`.
pub(crate) fn parse(text: &str) -> Option<DockState<Tab>> {
    let Saved { mut dock, windows } = serde_json::from_str(text).ok()?;
    // номер окна — из файла: за пределами egui_dock падает, а не отвечает None
    let n = dock.surfaces_count();
    for (i, [x, y, w, h]) in windows.into_iter().filter(|&(i, _)| i < n) {
        if let Some(st) = dock.get_window_state_mut(egui_dock::SurfaceIndex(i)) {
            st.set_position(egui::pos2(x, y)).set_size(egui::vec2(w, h));
        }
    }
    dock.find_tab(&Tab::Canvas).is_some().then_some(dock)
}

/// Одной строкой без пробелов: рабочее пространство делит свою строку по пробелам. Пробелы
/// бывают только внутри строк JSON (подписи меню), там `\u0020` — та же буква.
/// Ещё не показанное дерево хранит места вкладок бесконечными, а JSON пишет их как `null` и
/// потом не читает: такие места — нулями (их всё равно пересчитает первый же показ).
fn encode(saved: &Saved) -> String {
    serde_json::to_string(saved).unwrap_or_default().replace(' ', "\\u0020").replace("\"x\":null", "\"x\":0.0").replace("\"y\":null", "\"y\":0.0")
}

impl PaintApp {
    pub(crate) fn dock_text(&self) -> String {
        encode(&Saved { dock: self.dock.clone(), windows: self.dock_windows.clone() })
    }
}

struct Viewer<'a> {
    app: &'a mut PaintApp,
}

impl TabViewer for Viewer<'_> {
    type Tab = Tab;

    fn title(&mut self, tab: &mut Tab) -> egui::WidgetText {
        if *tab == Tab::Timeline {
            return tr("Таймлайн").into();
        }
        match tab.panel() {
            Some(i) => tr(crate::SHOW_NAMES[i]).into(),
            None => self.app.file_name().into(),
        }
    }

    fn id(&mut self, tab: &mut Tab) -> egui::Id {
        egui::Id::new(("dock_tab", *tab as u8))
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Tab) {
        let Some(i) = tab.panel() else {
            if *tab == Tab::Timeline {
                self.app.timeline_panel(ui);
            } else {
                self.app.canvas(ui);
            }
            return;
        };
        // во вкладке — только эта панель (как у отделённой панели)
        let full = self.app.win.show;
        for (k, v) in self.app.win.show.iter_mut().take(4).enumerate() {
            *v = k == i;
        }
        // виджеты SAI — постоянной ширины: в узкой вкладке прокрутка, а не наезд
        ui.set_min_width(if i >= 2 { 220.0 } else { 170.0 });
        if i >= 2 {
            self.app.left_panel(ui);
        } else {
            self.app.right_panel(ui);
        }
        self.app.win.show = full;
    }

    fn closeable(&mut self, tab: &mut Tab) -> bool {
        *tab != Tab::Canvas
    }

    fn on_close(&mut self, tab: &mut Tab) -> bool {
        if let Some(i) = tab.panel() {
            self.app.win.show[i] = false;
        }
        if *tab == Tab::Timeline {
            self.app.timeline.show = false;
        }
        true
    }

    fn allowed_in_windows(&self, tab: &mut Tab) -> bool {
        *tab != Tab::Canvas
    }

    fn clear_background(&self, tab: &Tab) -> bool {
        *tab != Tab::Canvas
    }

    fn scroll_bars(&self, tab: &Tab) -> [bool; 2] {
        if *tab == Tab::Canvas { [false, false] } else { [true, true] }
    }

    fn tab_style_override(&self, tab: &Tab, global: &egui_dock::TabStyle) -> Option<egui_dock::TabStyle> {
        // холст — во всю вкладку, без полей
        (*tab == Tab::Canvas).then(|| {
            let mut s = global.clone();
            s.tab_body.inner_margin = egui::Margin::ZERO;
            s
        })
    }
}

impl PaintApp {
    /// Подогнать дерево под «Окно → Показать» и «Отдельные панели».
    fn sync_dock(&mut self) {
        for (i, &tab) in PANELS.iter().enumerate() {
            let want = self.win.show[i] && !self.win.separate[i];
            let found = self.dock.find_tab(&tab);
            match (want, found) {
                (false, Some(at)) => {
                    self.dock.remove_tab(at);
                }
                (true, None) => {
                    // рядом с соседкой по колонке, иначе — новой колонкой со своей стороны
                    if let Some((s, n, _)) = self.dock.find_tab(&tab.partner()) {
                        // доля — у верхней половины
                        let (split, f) = if i % 2 == 0 { (egui_dock::Split::Above, 0.4) } else { (egui_dock::Split::Below, 0.6) };
                        self.dock.split((s, n), split, f, egui_dock::Node::leaf(tab));
                    } else {
                        let right = if i >= 2 { self.win.tools_right } else { self.win.layers_right };
                        let t = self.dock.main_surface_mut();
                        if right {
                            t.split_right(NodeIndex::root(), 0.84, vec![tab]);
                        } else {
                            t.split_left(NodeIndex::root(), 0.16, vec![tab]);
                        }
                    }
                }
                _ => {}
            }
        }
        // таймлайн — под холстом
        match (self.timeline.show, self.dock.find_tab(&Tab::Timeline)) {
            (false, Some(at)) => {
                self.dock.remove_tab(at);
            }
            (true, None) => {
                if let Some((s, n, _)) = self.dock.find_tab(&Tab::Canvas) {
                    self.dock.split((s, n), egui_dock::Split::Below, 0.72, egui_dock::Node::leaf(Tab::Timeline));
                }
            }
            _ => {}
        }
    }

    /// Панель, брошенная на середину холста, встала бы вкладкой рядом с ним и закрыла бы его
    /// (холст тогда не рисуется, вид и навигатор отстают) — такие вкладки уходят вправо от холста.
    fn keep_canvas_alone(&mut self) {
        let Some((s, n, _)) = self.dock.find_tab(&Tab::Canvas) else { return };
        let Some(tree) = self.dock.get_surface_mut(s).and_then(|x| x.node_tree_mut()) else { return };
        let others: Vec<Tab> = tree[n].tabs().map(|t| t.iter().copied().filter(|&t| t != Tab::Canvas).collect()).unwrap_or_default();
        if others.is_empty() {
            return;
        }
        tree[n].retain_tabs(|t| *t == Tab::Canvas);
        if let egui_dock::Node::Leaf { active, .. } = &mut tree[n] {
            *active = egui_dock::TabIndex(0);
        }
        tree.split_right(n, 0.8, others);
    }

    /// Главная область окна в режиме стыкуемых панелей.
    pub(crate) fn dock_area(&mut self, ctx: &egui::Context) {
        self.sync_dock();
        let mut style = egui_dock::Style::from_egui(&ctx.style());
        style.dock_area_padding = None;
        style.main_surface_border_stroke = egui::Stroke::NONE;
        let mut dock = std::mem::replace(&mut self.dock, DockState::new(Vec::new()));
        // оторванная вкладка уносит ширину своей колонки — окно не уже панели SAI
        for i in 1..dock.surfaces_count() {
            let at = egui_dock::SurfaceIndex(i);
            let Some(tree) = dock.get_surface(at).and_then(|s| s.node_tree()) else { continue };
            let need = tree.tabs().map(|t| if t.panel().is_some_and(|i| i >= 2) { 236.0 } else { 186.0 }).fold(0.0, f32::max);
            let Some(r) = tree.root_node().and_then(|n| n.rect()) else { continue };
            if r.is_finite() && r.width() + 1.0 < need {
                if let Some(w) = dock.get_window_state_mut(at) {
                    w.set_size(egui::vec2(need, r.height().max(300.0)));
                }
            }
        }
        let t = &mut dock.translations;
        t.tab_context_menu.close_button = tr("Закрыть").to_string();
        t.tab_context_menu.eject_button = tr("В отдельное окно").to_string();
        t.window.close_button_tooltip = tr("Закрыть").to_string();
        self.panels_busy.set(true);
        egui::CentralPanel::default().frame(egui::Frame::none()).show(ctx, |ui| {
            DockArea::new(&mut dock)
                .id(egui::Id::new("dock"))
                .style(style)
                .show_add_buttons(false)
                .show_window_collapse_buttons(false)
                .show_inside(ui, &mut Viewer { app: self });
        });
        self.dock = dock;
        self.keep_canvas_alone();
        // места окон — из памяти egui; размер — без полей рамки, как его задаёт set_size
        self.dock_windows = (1..self.dock.surfaces_count())
            .filter(|&i| self.dock.get_window_state_mut(egui_dock::SurfaceIndex(i)).is_some())
            .filter_map(|i| {
                let r = ctx.memory(|m| m.area_rect(window_id(i)))?;
                let m = ctx.style().spacing.window_margin.sum();
                Some((i, [r.min.x, r.min.y, r.width() - m.x, r.height() - m.y]))
            })
            .collect();
        self.finish_panels();
        // раскладку сохранить, когда отпущена мышь и что-то поменялось
        if ctx.input(|i| i.pointer.any_released()) {
            let text = self.dock_text();
            if text != self.dock_saved {
                self.dock_saved = text;
                self.save_settings();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_text_round_trip() {
        for (t, l) in [(false, false), (true, false), (false, true), (true, true)] {
            let d = default_layout(t, l);
            for tab in [Tab::Canvas].iter().chain(&PANELS) {
                assert!(d.find_tab(tab).is_some(), "{tab:?}");
            }
            let mut d = d;
            d.translations.tab_context_menu.eject_button = "В отдельное окно".into();
            let text = encode(&Saved { dock: d.clone(), windows: vec![(1, [10.0, 20.0, 200.0, 300.0])] });
            assert!(!text.contains(char::is_whitespace), "рабочее пространство делит строку по пробелам");
            let back = parse(&text).unwrap();
            assert_eq!(back.iter_all_tabs().count(), 5);
            assert_eq!(back.translations.tab_context_menu.eject_button, "В отдельное окно");
        }
        assert!(parse("мусор").is_none());
        let no_canvas = encode(&Saved { dock: DockState::new(vec![Tab::Layers]), windows: Vec::new() });
        assert!(parse(&no_canvas).is_none());
    }
}
