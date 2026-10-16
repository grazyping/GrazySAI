//! Свои клавиши: действие → клавиша с модификаторами. По умолчанию — как в SAI 2017
//! (`docs/07-sai2-reference.md`). Назначения хранятся в настройках строками `key.<id>=Ctrl+S`.

use eframe::egui::{InputState, Key};
use std::collections::HashMap;

static HINTS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// Показывать ли быстрые клавиши в меню и подсказках («Окно → Показывать быстрые клавиши»).
pub fn set_hints(on: bool) {
    HINTS.store(on, std::sync::atomic::Ordering::Relaxed);
}

pub fn hints() -> bool {
    HINTS.load(std::sync::atomic::Ordering::Relaxed)
}

/// Подпись с клавишей в скобках в конце («Отменить (Ctrl+Z)», «Файл(F)»): при выключенных
/// подсказках — без скобок. Переводы держат клавишу так же в конце (проверяет тест).
pub fn hint(s: &str) -> String {
    if hints() {
        return s.to_string();
    }
    let t = s.trim_end();
    match (t.ends_with(')'), t.rfind('(')) {
        (true, Some(i)) => t[..i].trim_end().to_string(),
        _ => s.to_string(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Action {
    Undo,
    Redo,
    Open,
    Save,
    SaveAs,
    SizeDown,
    SizeUp,
    ZoomIn,
    ZoomOut,
    Fill,
    Clear,
    Merge,
    Transfer,
    SelectAll,
    Deselect,
    HideSelection,
    Marquee,
    Lasso,
    Wand,
    Pencil,
    AirBrush,
    Brush,
    Water,
    Eraser,
    LineEdit,
    LinePressure,
    SwapColors,
    FlipView,
    RotateCcw,
    RotateCw,
    ResetRotation,
    HueSaturation,
    Transform,
    Ruler,
    New,
    Close,
    FromClipboard,
    Copy,
    Cut,
    Paste,
    ResetView,
    Zoom100,
    NewView,
    Fullscreen,
    TogglePanels,
    InvertSelection,
    Marker,
    SelPen,
    SelEraser,
    Gradient,
    Bucket,
    MoveTool,
    TextTool,
    HandTool,
    RotateTool,
    IgnoreRuler,
    StraightLine,
    TransparentColor,
    PickMerged,
    PickLayer,
    PrevView,
    NextView,
    CloseAllViews,
    ScrollUp,
    ScrollDown,
    ScrollLeft,
    ScrollRight,
    ZoomInCursor,
    ZoomOutCursor,
    LayerAbove,
    LayerBelow,
    LayerFront,
    LayerBack,
    ShowAlone,
    ToggleVisible,
    GrowSel1,
    ShrinkSel1,
    Density10,
    Density20,
    Density30,
    Density40,
    Density50,
    Density60,
    Density70,
    Density80,
    Density90,
    Density100,
    Picker,
    ZoomTool,
}

/// Действие, имя для настроек, подпись в окне, клавиша по умолчанию.
/// Пустая клавиша — по умолчанию не назначена (в SAI её нет).
pub const ALL: [(Action, &str, &str, &str); 89] = [
    (Action::Undo, "undo", "Отменить", "Ctrl+Z"),
    (Action::Redo, "redo", "Повторить", "Ctrl+Y"),
    (Action::Open, "open", "Открыть", "Ctrl+O"),
    (Action::Save, "save", "Сохранить", "Ctrl+S"),
    (Action::SaveAs, "save_as", "Сохранить как", "Ctrl+Shift+S"),
    (Action::SizeDown, "size_down", "Размер кисти меньше", "OpenBracket"),
    (Action::SizeUp, "size_up", "Размер кисти больше", "CloseBracket"),
    (Action::ZoomIn, "zoom_in", "Приблизить", "Ctrl+Equals"),
    (Action::ZoomOut, "zoom_out", "Отдалить", "Ctrl+Minus"),
    (Action::Fill, "fill", "Залить", "Alt+Delete"),
    (Action::Clear, "clear", "Очистить слой", "Delete"),
    (Action::Merge, "merge", "Слить с нижним", "Ctrl+E"),
    (Action::Transfer, "transfer", "Перенести на нижний", "F"),
    (Action::SelectAll, "select_all", "Выделить всё", "Ctrl+A"),
    (Action::Deselect, "deselect", "Снять выделение", "Ctrl+D"),
    (Action::HideSelection, "hide_selection", "Показать/скрыть выделение", "Ctrl+H"),
    (Action::Marquee, "marquee", "Прямоугольник выделения", "M"),
    (Action::Lasso, "lasso", "Лассо", "L"),
    (Action::Wand, "wand", "Волшебная палочка", "W"),
    (Action::Pencil, "pencil", "Карандаш (на слое контура — перо)", "N"),
    (Action::AirBrush, "airbrush", "Аэрограф", "B"),
    (Action::Brush, "brush", "Кисть", "V"),
    (Action::Water, "water", "Акварель", "C"),
    (Action::Eraser, "eraser", "Ластик (на слое контура — ластик контура)", "E"),
    (Action::LineEdit, "line_edit", "Контур: узлы", "I"),
    (Action::LinePressure, "line_pressure", "Контур: нажим", "P"),
    (Action::SwapColors, "swap_colors", "Поменять цвета", "X"),
    (Action::FlipView, "flip_view", "Зеркало вида", "H"),
    (Action::RotateCcw, "rotate_ccw", "Повернуть вид против часовой", "Shift+PageUp"),
    (Action::RotateCw, "rotate_cw", "Повернуть вид по часовой", "Shift+PageDown"),
    (Action::ResetRotation, "reset_rotation", "Сбросить поворот вида", "Shift+Home"),
    (Action::HueSaturation, "hue_saturation", "Тон и насыщенность", "Ctrl+U"),
    (Action::Transform, "transform", "Трансформация", "Ctrl+T"),
    (Action::Ruler, "ruler", "Показать/скрыть линейку", "Ctrl+R"),
    (Action::New, "new", "Создать холст", "Ctrl+N"),
    (Action::Close, "close", "Закрыть холст", "Ctrl+W"),
    (Action::FromClipboard, "from_clipboard", "Холст из буфера обмена", "Ctrl+B"),
    (Action::Copy, "copy", "Копировать", "Ctrl+C"),
    (Action::Cut, "cut", "Вырезать", "Ctrl+X"),
    (Action::Paste, "paste", "Вставить", "Ctrl+V"),
    (Action::ResetView, "reset_view", "Сбросить положение вида", "Home"),
    (Action::Zoom100, "zoom_100", "Масштаб 100%", "Ctrl+Alt+0"),
    (Action::NewView, "new_view", "Новый вид", ""),
    (Action::Fullscreen, "fullscreen", "Полный экран", "F11"),
    (Action::TogglePanels, "panels", "Показывать все панели", "Tab"),
    (Action::InvertSelection, "invert_selection", "Инвертировать", ""),
    (Action::Marker, "marker", "Маркер", ""),
    (Action::SelPen, "sel_pen", "Перо выделения", ""),
    (Action::SelEraser, "sel_eraser", "Ластик выделения", ""),
    (Action::Gradient, "gradient", "Градиент", ""),
    (Action::Bucket, "bucket", "Заливка (ведро)", ""),
    (Action::MoveTool, "move_tool", "Перемещение", ""),
    (Action::TextTool, "text_tool", "Текст", ""),
    (Action::HandTool, "hand_tool", "Рука", ""),
    (Action::RotateTool, "rotate_tool", "Поворот вида", ""),
    (Action::IgnoreRuler, "ignore_ruler", "Не замечать линейку", ""),
    (Action::StraightLine, "straight_line", "Прямая линия", ""),
    (Action::TransparentColor, "transparent", "Прозрачный цвет", ""),
    (Action::PickMerged, "pick_merged", "Пипетка по сведённому", ""),
    (Action::PickLayer, "pick_layer", "Пипетка по текущему слою", ""),
    (Action::PrevView, "prev_view", "Предыдущий вид", ""),
    (Action::NextView, "next_view", "Следующий вид", ""),
    (Action::CloseAllViews, "close_all_views", "Закрыть все виды", ""),
    (Action::ScrollUp, "scroll_up", "Прокрутить вид вверх", ""),
    (Action::ScrollDown, "scroll_down", "Прокрутить вид вниз", ""),
    (Action::ScrollLeft, "scroll_left", "Прокрутить вид влево", ""),
    (Action::ScrollRight, "scroll_right", "Прокрутить вид вправо", ""),
    (Action::ZoomInCursor, "zoom_in_cursor", "Приблизить у курсора", ""),
    (Action::ZoomOutCursor, "zoom_out_cursor", "Отдалить у курсора", ""),
    (Action::LayerAbove, "layer_above", "Выбрать слой выше", ""),
    (Action::LayerBelow, "layer_below", "Выбрать слой ниже", ""),
    (Action::LayerFront, "layer_front", "Слой на передний план", ""),
    (Action::LayerBack, "layer_back", "Слой на задний план", ""),
    (Action::ShowAlone, "show_alone", "Показать только этот слой", ""),
    (Action::ToggleVisible, "toggle_visible", "Показать/скрыть слой", ""),
    (Action::GrowSel1, "grow_sel_1", "Расширить выделение на 1 px", ""),
    (Action::ShrinkSel1, "shrink_sel_1", "Сузить выделение на 1 px", ""),
    (Action::Density10, "density_10", "Плотность кисти 10", ""),
    (Action::Density20, "density_20", "Плотность кисти 20", ""),
    (Action::Density30, "density_30", "Плотность кисти 30", ""),
    (Action::Density40, "density_40", "Плотность кисти 40", ""),
    (Action::Density50, "density_50", "Плотность кисти 50", ""),
    (Action::Density60, "density_60", "Плотность кисти 60", ""),
    (Action::Density70, "density_70", "Плотность кисти 70", ""),
    (Action::Density80, "density_80", "Плотность кисти 80", ""),
    (Action::Density90, "density_90", "Плотность кисти 90", ""),
    (Action::Density100, "density_100", "Плотность кисти 100", ""),
    (Action::Picker, "picker", "Пипетка", ""),
    (Action::ZoomTool, "zoom_tool", "Масштаб (инструмент)", ""),
];

/// Клавиша с модификаторами (Ctrl — это Cmd на macOS).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Binding {
    pub key: Key,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

impl Binding {
    /// Разобрать строку вида `Ctrl+Shift+S`.
    pub fn parse(s: &str) -> Option<Binding> {
        let mut b = Binding { key: Key::A, ctrl: false, shift: false, alt: false };
        let mut key = None;
        for part in s.split('+').map(str::trim) {
            match part {
                "Ctrl" => b.ctrl = true,
                "Shift" => b.shift = true,
                "Alt" => b.alt = true,
                k => key = Key::from_name(k),
            }
        }
        b.key = key?;
        Some(b)
    }

    pub fn text(&self) -> String {
        let mut s = String::new();
        for (on, name) in [(self.ctrl, "Ctrl+"), (self.shift, "Shift+"), (self.alt, "Alt+")] {
            if on {
                s.push_str(name);
            }
        }
        s.push_str(self.key.name());
        s
    }

    /// Для показа: знаки вместо имён клавиш (`[` вместо `OpenBracket`), как подписано на
    /// клавиатуре. В настройках хранится `text()`.
    pub fn label(&self) -> String {
        let t = self.text();
        let (mods, key) = t.rsplit_once('+').map_or(("", t.as_str()), |(m, k)| (m, k));
        let key = match key {
            "OpenBracket" => "[",
            "CloseBracket" => "]",
            "Equals" => "=",
            "Minus" => "−",
            "Plus" => "+",
            "Comma" => ",",
            "Period" => ".",
            "Slash" => "/",
            "Backslash" => "\\",
            "Semicolon" => ";",
            "Quote" => "'",
            "Backtick" => "`",
            k => k,
        };
        if mods.is_empty() { key.to_string() } else { format!("{mods}+{key}") }
    }

    pub fn pressed(&self, i: &InputState) -> bool {
        let m = i.modifiers;
        i.key_pressed(self.key) && m.command == self.ctrl && m.shift == self.shift && m.alt == self.alt
    }
}

pub type Keymap = HashMap<Action, Option<Binding>>;

pub fn defaults() -> Keymap {
    ALL.iter().map(|(a, _, _, k)| (*a, Binding::parse(k))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_parse_and_round_trip() {
        for (_, id, _, k) in ALL.into_iter().filter(|e| !e.3.is_empty()) {
            let b = Binding::parse(k).unwrap_or_else(|| panic!("{id}: {k}"));
            assert_eq!(Binding::parse(&b.text()), Some(b), "{id}");
        }
        let d = defaults();
        assert_eq!(d.len(), ALL.len());
        assert_eq!(Binding::parse("Ctrl+Equals").unwrap().label(), "Ctrl+=");
        assert_eq!(Binding::parse("OpenBracket").unwrap().label(), "[");
        // нет двух действий на одной клавише
        let mut seen = std::collections::HashSet::new();
        for b in d.values().flatten() {
            assert!(seen.insert(b.text()), "повтор {}", b.text());
        }
    }
}

#[cfg(test)]
mod hint_tests {
    use super::*;

    /// Подписи с клавишей в скобках: на всех языках клавиша остаётся в конце, и без неё
    /// остаётся непустая подпись.
    #[test]
    fn hints_strip_on_every_language() {
        let src = include_str!("main.rs");
        let keys: Vec<&str> = src.split("keys::hint(tr(\"").skip(1).filter_map(|r| r.split("\")").next()).collect();
        assert!(keys.len() >= 25, "мало подписей: {}", keys.len());
        set_hints(false);
        for (code, _, _) in crate::i18n::LANGS {
            crate::i18n::set(code);
            for k in &keys {
                let full = crate::i18n::tr(k);
                let short = hint(full);
                assert!(full.trim_end().ends_with(')'), "{code}: клавиша не в конце: «{full}»");
                assert!(!short.is_empty() && !short.contains('('), "{code}: «{full}» → «{short}»");
            }
        }
        crate::i18n::set("en");
        set_hints(true);
    }
}
