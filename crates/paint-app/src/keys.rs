//! Свои клавиши: действие → клавиша с модификаторами. По умолчанию — как в SAI 2017
//! (`docs/07-sai2-reference.md`). Назначения хранятся в настройках строками `key.<id>=Ctrl+S`.

use eframe::egui::{InputState, Key};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Action {
    Undo,
    Redo,
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
}

/// Действие, имя для настроек, подпись в окне, клавиша по умолчанию.
pub const ALL: [(Action, &str, &str, &str); 32] = [
    (Action::Undo, "undo", "Отменить", "Ctrl+Z"),
    (Action::Redo, "redo", "Повторить", "Ctrl+Y"),
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
        for (_, id, _, k) in ALL {
            let b = Binding::parse(k).unwrap_or_else(|| panic!("{id}: {k}"));
            assert_eq!(Binding::parse(&b.text()), Some(b), "{id}");
        }
        let d = defaults();
        assert_eq!(d.len(), ALL.len());
        // нет двух действий на одной клавише
        let mut seen = std::collections::HashSet::new();
        for b in d.values().flatten() {
            assert!(seen.insert(b.text()), "повтор {}", b.text());
        }
    }
}
