//! Внешний вид в духе PaintTool SAI 2 (docs/04-ui-layout.md):
//! светло-серые панели, мелкий шрифт, сиреневое выделение, числовые поля-полоски, цветовой круг,
//! значки инструментов, нарисованные линиями.

use eframe::egui::{self, Color32, Painter, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, Vec2};
use std::f32::consts::{PI, TAU};

pub const PANEL: Color32 = Color32::from_gray(240);
pub const FIELD: Color32 = Color32::from_gray(176);
pub const BORDER: Color32 = Color32::from_gray(160);
pub const SELECT_BG: Color32 = Color32::from_rgb(220, 220, 250);
pub const SELECT_EDGE: Color32 = Color32::from_rgb(140, 140, 220);
pub const HEADER: Color32 = Color32::from_rgb(160, 70, 30);
pub const INK: Color32 = Color32::from_gray(20);
pub const DIM: Color32 = Color32::from_gray(160);

/// Оформление: светлая тема, мелкий шрифт, плотные отступы.
pub fn apply_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::light();
    let v = &mut style.visuals;
    v.panel_fill = PANEL;
    v.window_fill = PANEL;
    v.extreme_bg_color = Color32::WHITE;
    v.faint_bg_color = Color32::from_gray(232);
    v.selection.bg_fill = SELECT_BG;
    v.selection.stroke = Stroke::new(1.0_f32, SELECT_EDGE);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, Color32::from_gray(200));
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, INK);
    v.widgets.inactive.bg_fill = Color32::from_gray(232);
    v.widgets.inactive.weak_bg_fill = Color32::from_gray(232);
    v.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, INK);
    v.widgets.hovered.bg_fill = Color32::from_gray(222);
    v.widgets.hovered.weak_bg_fill = Color32::from_gray(222);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, SELECT_EDGE);
    v.widgets.active.bg_fill = SELECT_BG;
    v.widgets.active.weak_bg_fill = SELECT_BG;
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.rounding = egui::Rounding::ZERO;
    }
    v.window_rounding = egui::Rounding::ZERO;
    v.menu_rounding = egui::Rounding::ZERO;
    style.spacing.item_spacing = Vec2::new(4.0, 3.0);
    style.spacing.button_padding = Vec2::new(4.0, 1.0);
    style.spacing.interact_size = Vec2::new(18.0, 18.0);
    style.spacing.combo_height = 300.0;
    use egui::{FontId, TextStyle};
    style.text_styles = [
        (TextStyle::Heading, FontId::proportional(14.0)),
        (TextStyle::Body, FontId::proportional(12.0)),
        (TextStyle::Button, FontId::proportional(12.0)),
        (TextStyle::Monospace, FontId::monospace(11.0)),
        (TextStyle::Small, FontId::proportional(10.0)),
    ]
    .into();
    ctx.set_style(style);
}

/// Числовое поле-полоска, как «Плотн.» и «Непроз.» у образца: рамка, заливка по значению, число
/// справа. Тянуть мышью, крутить колесом.
pub fn bar(
    ui: &mut Ui,
    value: &mut f32,
    min: f32,
    max: f32,
    text: String,
    width: f32,
    enabled: bool,
) -> Response {
    let (rect, mut response) = ui.allocate_exact_size(
        Vec2::new(width, 16.0),
        if enabled {
            Sense::click_and_drag()
        } else {
            Sense::hover()
        },
    );
    if enabled {
        if let Some(pos) = response.interact_pointer_pos() {
            let t = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
            let new = min + t * (max - min);
            if (new - *value).abs() > f32::EPSILON {
                *value = new;
                response.mark_changed();
            }
        }
        if response.hovered() {
            let scroll = ui.input(|i| i.raw_scroll_delta.y);
            if scroll != 0.0 {
                *value = (*value + scroll.signum() * (max - min) / 100.0).clamp(min, max);
                response.mark_changed();
            }
        }
    }
    let p = ui.painter();
    p.rect_filled(rect, 0.0, Color32::WHITE);
    let t = ((*value - min) / (max - min)).clamp(0.0, 1.0);
    let fill = Rect::from_min_size(rect.min, Vec2::new(rect.width() * t, rect.height()));
    p.rect_filled(
        fill,
        0.0,
        if enabled {
            Color32::from_gray(205)
        } else {
            Color32::from_gray(235)
        },
    );
    p.rect_stroke(rect, 0.0, Stroke::new(1.0_f32, BORDER));
    p.text(
        rect.right_center() - Vec2::new(4.0, 0.0),
        egui::Align2::RIGHT_CENTER,
        text,
        egui::FontId::proportional(12.0),
        if enabled { INK } else { DIM },
    );
    response
}

/// Подпись слева и элемент справа в одну строку фиксированной ширины.
pub fn row(ui: &mut Ui, label: &str, label_w: f32, enabled: bool, add: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(Vec2::new(label_w, 16.0), Sense::hover());
        ui.painter().text(
            r.left_center(),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(12.0),
            if enabled { INK } else { DIM },
        );
        ui.add_enabled_ui(enabled, add);
    });
}

/// Сворачиваемый раздел «▸ Текстура»: возвращает, открыт ли он.
pub fn section(ui: &mut Ui, id: &str, title: &str) -> bool {
    let id = ui.make_persistent_id(id);
    let mut open = ui.data_mut(|d| *d.get_persisted_mut_or(id, false));
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 18.0), Sense::click());
    if response.clicked() {
        open = !open;
        ui.data_mut(|d| d.insert_persisted(id, open));
    }
    let p = ui.painter();
    let c = rect.left_center() + Vec2::new(6.0, 0.0);
    let tri = if open {
        vec![
            c + Vec2::new(-4.0, -2.5),
            c + Vec2::new(4.0, -2.5),
            c + Vec2::new(0.0, 3.0),
        ]
    } else {
        vec![
            c + Vec2::new(-2.5, -4.0),
            c + Vec2::new(3.0, 0.0),
            c + Vec2::new(-2.5, 4.0),
        ]
    };
    p.add(Shape::convex_polygon(tri, HEADER, Stroke::NONE));
    p.text(
        rect.left_center() + Vec2::new(14.0, 0.0),
        egui::Align2::LEFT_CENTER,
        title,
        egui::FontId::proportional(12.0),
        HEADER,
    );
    p.line_segment(
        [rect.left_bottom(), rect.right_bottom()],
        Stroke::new(1.0_f32, Color32::from_gray(215)),
    );
    open
}

/// Кнопка-значок: рамка, сиреневый фон у выбранной, рисунок — замыканием.
pub fn icon_button(
    ui: &mut Ui,
    size: Vec2,
    selected: bool,
    enabled: bool,
    tip: &str,
    draw: impl FnOnce(&Painter, Rect, Color32),
) -> Response {
    let (rect, response) = ui.allocate_exact_size(
        size,
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let p = ui.painter();
    if selected {
        p.rect_filled(rect, 0.0, SELECT_BG);
        p.rect_stroke(rect.shrink(0.5), 0.0, Stroke::new(1.0_f32, SELECT_EDGE));
    } else if enabled && response.hovered() {
        p.rect_filled(rect, 0.0, Color32::from_gray(225));
    }
    draw(p, rect.shrink(3.0), if enabled { INK } else { DIM });
    response.on_hover_text(tip)
}

/// Значки общих инструментов и кнопок, нарисованные линиями.
pub fn glyph(p: &Painter, r: Rect, c: Color32, name: &str) {
    let s = Stroke::new(1.3_f32, c);
    let ctr = r.center();
    let k = r.width().min(r.height()) / 2.0;
    match name {
        "select" => {
            let q = r.shrink(1.0);
            for (a, b) in [
                (q.left_top(), q.right_top()),
                (q.right_top(), q.right_bottom()),
                (q.right_bottom(), q.left_bottom()),
                (q.left_bottom(), q.left_top()),
            ] {
                let n = 4;
                for i in 0..n {
                    if i % 2 == 0 {
                        p.line_segment(
                            [
                                a + (b - a) * (i as f32 / n as f32),
                                a + (b - a) * ((i + 1) as f32 / n as f32),
                            ],
                            s,
                        );
                    }
                }
            }
        }
        "lasso" => {
            let pts: Vec<Pos2> = (0..=24)
                .map(|i| {
                    let t = i as f32 / 24.0 * TAU;
                    ctr + Vec2::new(t.cos() * k * 0.9, t.sin() * k * 0.55 - k * 0.2)
                })
                .collect();
            p.add(Shape::line(pts, s));
            p.line_segment(
                [
                    ctr + Vec2::new(-k * 0.2, k * 0.3),
                    ctr + Vec2::new(-k * 0.5, k * 0.9),
                ],
                s,
            );
        }
        "wand" => {
            p.line_segment(
                [
                    ctr + Vec2::new(-k * 0.8, k * 0.8),
                    ctr + Vec2::new(k * 0.2, -k * 0.2),
                ],
                Stroke::new(2.0_f32, c),
            );
            for a in 0..4 {
                let t = a as f32 * PI / 2.0 + PI / 4.0;
                let o = ctr + Vec2::new(k * 0.45, -k * 0.45);
                p.line_segment([o, o + Vec2::new(t.cos(), t.sin()) * k * 0.4], s);
            }
        }
        "bucket" => {
            p.add(Shape::convex_polygon(
                vec![
                    ctr + Vec2::new(-k * 0.6, -k * 0.1),
                    ctr + Vec2::new(0.0, -k * 0.7),
                    ctr + Vec2::new(k * 0.6, -k * 0.1),
                    ctr + Vec2::new(0.0, k * 0.5),
                ],
                Color32::TRANSPARENT,
                s,
            ));
            p.circle_filled(ctr + Vec2::new(k * 0.7, k * 0.5), k * 0.2, c);
        }
        "text" => {
            p.text(
                ctr,
                egui::Align2::CENTER_CENTER,
                "T",
                egui::FontId::proportional(r.height() + 2.0),
                c,
            );
        }
        "move" => {
            p.line_segment([ctr - Vec2::X * k, ctr + Vec2::X * k], s);
            p.line_segment([ctr - Vec2::Y * k, ctr + Vec2::Y * k], s);
            for d in [Vec2::X, -Vec2::X, Vec2::Y, -Vec2::Y] {
                let tip = ctr + d * k;
                let side = Vec2::new(-d.y, d.x) * k * 0.3;
                p.add(Shape::convex_polygon(
                    vec![tip, tip - d * k * 0.35 + side, tip - d * k * 0.35 - side],
                    c,
                    Stroke::NONE,
                ));
            }
        }
        "zoom" => {
            p.circle_stroke(ctr + Vec2::new(-k * 0.2, -k * 0.2), k * 0.55, s);
            p.line_segment(
                [
                    ctr + Vec2::new(k * 0.2, k * 0.2),
                    ctr + Vec2::new(k * 0.85, k * 0.85),
                ],
                Stroke::new(2.0_f32, c),
            );
        }
        "rotate" | "undo" | "redo" | "ccw" | "cw" => {
            let flip = matches!(name, "redo" | "cw");
            let pts: Vec<Pos2> = (0..=18)
                .map(|i| {
                    let t = PI * 0.25 + i as f32 / 18.0 * PI * 1.5;
                    let t = if flip { PI - t } else { t };
                    ctr + Vec2::new(t.cos(), -t.sin()) * k * 0.75
                })
                .collect();
            let end = *pts.last().unwrap();
            p.add(Shape::line(pts, s));
            let d = if flip {
                Vec2::new(-1.0, 0.0)
            } else {
                Vec2::new(1.0, 0.0)
            };
            p.add(Shape::convex_polygon(
                vec![
                    end + Vec2::new(0.0, -k * 0.35),
                    end + Vec2::new(0.0, k * 0.35),
                    end + d * k * 0.4,
                ],
                c,
                Stroke::NONE,
            ));
        }
        "hand" => {
            for i in 0..4 {
                let x = ctr.x - k * 0.5 + i as f32 * k * 0.33;
                p.line_segment(
                    [
                        Pos2::new(x, ctr.y),
                        Pos2::new(x, ctr.y - k * 0.8 + (i as f32 - 1.5).abs() * k * 0.15),
                    ],
                    s,
                );
            }
            p.add(Shape::line(
                vec![
                    ctr + Vec2::new(-k * 0.55, 0.0),
                    ctr + Vec2::new(-k * 0.5, k * 0.6),
                    ctr + Vec2::new(k * 0.5, k * 0.6),
                    ctr + Vec2::new(k * 0.55, -k * 0.1),
                ],
                s,
            ));
        }
        "picker" => {
            p.line_segment(
                [
                    ctr + Vec2::new(-k * 0.8, k * 0.8),
                    ctr + Vec2::new(k * 0.3, -k * 0.3),
                ],
                Stroke::new(1.5_f32, c),
            );
            p.line_segment(
                [
                    ctr + Vec2::new(k * 0.1, -k * 0.6),
                    ctr + Vec2::new(k * 0.6, -k * 0.1),
                ],
                Stroke::new(3.0_f32, c),
            );
        }
        "minus" => {
            p.line_segment(
                [ctr - Vec2::X * k * 0.6, ctr + Vec2::X * k * 0.6],
                Stroke::new(2.0_f32, c),
            );
        }
        "plus" => {
            p.line_segment(
                [ctr - Vec2::X * k * 0.6, ctr + Vec2::X * k * 0.6],
                Stroke::new(2.0_f32, c),
            );
            p.line_segment(
                [ctr - Vec2::Y * k * 0.6, ctr + Vec2::Y * k * 0.6],
                Stroke::new(2.0_f32, c),
            );
        }
        "reset" => {
            p.rect_filled(Rect::from_center_size(ctr, Vec2::splat(k * 1.1)), 0.0, c);
        }
        "flip" => {
            p.add(Shape::convex_polygon(
                vec![
                    ctr + Vec2::new(-k, 0.0),
                    ctr + Vec2::new(-k * 0.3, -k * 0.5),
                    ctr + Vec2::new(-k * 0.3, k * 0.5),
                ],
                c,
                Stroke::NONE,
            ));
            p.add(Shape::convex_polygon(
                vec![
                    ctr + Vec2::new(k, 0.0),
                    ctr + Vec2::new(k * 0.3, -k * 0.5),
                    ctr + Vec2::new(k * 0.3, k * 0.5),
                ],
                c,
                Stroke::NONE,
            ));
            p.line_segment(
                [ctr - Vec2::X * k * 0.3, ctr + Vec2::X * k * 0.3],
                Stroke::new(2.0_f32, c),
            );
        }
        "line" => {
            p.line_segment(
                [
                    ctr + Vec2::new(-k * 0.8, k * 0.8),
                    ctr + Vec2::new(k * 0.8, -k * 0.8),
                ],
                s,
            );
        }
        "eye" => {
            let pts: Vec<Pos2> = (0..=16)
                .map(|i| {
                    let t = i as f32 / 16.0 * TAU;
                    ctr + Vec2::new(t.cos() * k * 0.9, t.sin() * k * 0.45)
                })
                .collect();
            p.add(Shape::line(pts, Stroke::new(1.0_f32, c)));
            p.circle_filled(ctr, k * 0.3, c);
        }
        "deselect" | "invert" => {
            glyph(p, r, c, "select");
            if name == "invert" {
                p.rect_filled(r.shrink(k * 0.6), 0.0, c);
            }
        }
        "new_layer" => {
            p.add(Shape::line(
                vec![
                    r.left_top(),
                    r.right_top() - Vec2::X * k * 0.5,
                    r.right_top() + Vec2::new(0.0, k * 0.5),
                    r.right_bottom(),
                    r.left_bottom(),
                    r.left_top(),
                ],
                s,
            ));
        }
        "new_linework" => {
            glyph(p, r, c, "new_layer");
            p.line_segment(
                [
                    ctr + Vec2::new(-k * 0.4, k * 0.5),
                    ctr + Vec2::new(k * 0.4, -k * 0.4),
                ],
                Stroke::new(1.8_f32, c),
            );
        }
        "new_folder" => {
            p.add(Shape::line(
                vec![
                    r.left_bottom(),
                    r.left_top() + Vec2::Y * k * 0.3,
                    r.left_top() + Vec2::new(k * 0.6, k * 0.3),
                    r.left_top() + Vec2::new(k * 0.8, k * 0.5),
                    r.right_top() + Vec2::Y * k * 0.5,
                    r.right_bottom(),
                    r.left_bottom(),
                ],
                s,
            ));
        }
        "ruler" => {
            p.line_segment([r.left_bottom(), r.right_top()], s);
            p.line_segment([r.left_bottom(), r.right_bottom() - Vec2::X * k * 0.2], s);
            p.line_segment([r.center_bottom(), r.center_top()], s);
        }
        "mask" => {
            p.rect_stroke(r, 0.0, s);
            p.circle_filled(ctr, k * 0.5, c);
        }
        "lower" => {
            p.add(Shape::convex_polygon(vec![ctr + Vec2::new(-k * 0.6, -k * 0.3), ctr + Vec2::new(k * 0.6, -k * 0.3), ctr + Vec2::new(0.0, k * 0.6)], c, Stroke::NONE));
        }
        "up" => {
            p.add(Shape::convex_polygon(vec![ctr + Vec2::new(-k * 0.6, k * 0.3), ctr + Vec2::new(k * 0.6, k * 0.3), ctr + Vec2::new(0.0, -k * 0.6)], c, Stroke::NONE));
        }
        "down" => {
            p.add(Shape::convex_polygon(
                vec![
                    ctr + Vec2::new(-k * 0.6, -k * 0.1),
                    ctr + Vec2::new(k * 0.6, -k * 0.1),
                    ctr + Vec2::new(0.0, k * 0.8),
                ],
                c,
                Stroke::NONE,
            ));
            p.rect_stroke(
                Rect::from_min_max(r.left_top(), Pos2::new(r.right(), ctr.y - k * 0.3)),
                0.0,
                s,
            );
        }
        "merge" => {
            p.rect_stroke(
                Rect::from_min_size(r.left_top(), Vec2::splat(k * 1.3)),
                0.0,
                s,
            );
            p.rect_stroke(
                Rect::from_min_size(r.left_top() + Vec2::splat(k * 0.7), Vec2::splat(k * 1.3)),
                0.0,
                s,
            );
        }
        "clear" => {
            p.add(Shape::convex_polygon(
                vec![
                    ctr + Vec2::new(-k * 0.9, k * 0.3),
                    ctr + Vec2::new(0.0, -k * 0.6),
                    ctr + Vec2::new(k * 0.9, k * 0.3),
                    ctr + Vec2::new(0.0, k * 0.9),
                ],
                Color32::TRANSPARENT,
                s,
            ));
        }
        "delete" => {
            p.rect_stroke(
                Rect::from_min_max(
                    r.left_top() + Vec2::new(k * 0.3, k * 0.4),
                    r.right_bottom() - Vec2::X * k * 0.3,
                ),
                0.0,
                s,
            );
            p.line_segment(
                [
                    r.left_top() + Vec2::Y * k * 0.2,
                    r.right_top() + Vec2::Y * k * 0.2,
                ],
                s,
            );
            for i in 0..3 {
                let x = r.left() + k * (0.65 + i as f32 * 0.35);
                p.line_segment(
                    [
                        Pos2::new(x, r.top() + k * 0.7),
                        Pos2::new(x, r.bottom() - k * 0.25),
                    ],
                    Stroke::new(1.0_f32, c),
                );
            }
        }
        "lock_alpha" => {
            p.rect_stroke(r.shrink(k * 0.2), 0.0, Stroke::new(1.0_f32, c));
            for i in 0..2 {
                for j in 0..2 {
                    if (i + j) % 2 == 0 {
                        let q = r.shrink(k * 0.2);
                        let h = q.width() / 2.0;
                        p.rect_filled(
                            Rect::from_min_size(
                                q.min + Vec2::new(i as f32 * h, j as f32 * h),
                                Vec2::splat(h),
                            ),
                            0.0,
                            Color32::from_gray(190),
                        );
                    }
                }
            }
        }
        "lock_paint" => {
            p.line_segment(
                [
                    ctr + Vec2::new(-k * 0.7, k * 0.7),
                    ctr + Vec2::new(k * 0.6, -k * 0.6),
                ],
                Stroke::new(2.0_f32, c),
            );
        }
        "lock_all" => {
            p.rect_filled(
                Rect::from_min_max(
                    ctr + Vec2::new(-k * 0.6, -k * 0.1),
                    ctr + Vec2::new(k * 0.6, k * 0.8),
                ),
                1.0,
                c,
            );
            p.circle_stroke(
                ctr + Vec2::new(0.0, -k * 0.25),
                k * 0.4,
                Stroke::new(1.5_f32, c),
            );
        }
        "pen" => {
            p.line_segment(
                [
                    ctr + Vec2::new(-k * 0.8, k * 0.8),
                    ctr + Vec2::new(k * 0.7, -k * 0.7),
                ],
                Stroke::new(2.0_f32, c),
            );
            p.line_segment(
                [
                    ctr + Vec2::new(-k * 0.8, k * 0.8),
                    ctr + Vec2::new(-k * 0.45, k * 0.75),
                ],
                Stroke::new(1.0_f32, c),
            );
        }
        "airbrush" => {
            p.circle_stroke(ctr + Vec2::new(k * 0.2, -k * 0.2), k * 0.45, s);
            for (dx, dy) in [(-0.6f32, 0.4f32), (-0.8, 0.7), (-0.4, 0.8), (-0.9, 0.2)] {
                p.circle_filled(ctr + Vec2::new(dx, dy) * k, 0.8, c);
            }
        }
        "brush" => {
            p.line_segment(
                [
                    ctr + Vec2::new(-k * 0.2, k * 0.2),
                    ctr + Vec2::new(k * 0.8, -k * 0.8),
                ],
                Stroke::new(1.5_f32, c),
            );
            p.add(Shape::convex_polygon(
                vec![
                    ctr + Vec2::new(-k * 0.9, k * 0.9),
                    ctr + Vec2::new(-k * 0.45, k * 0.05),
                    ctr + Vec2::new(0.05, k * 0.45),
                ],
                c,
                Stroke::NONE,
            ));
        }
        "water" => {
            glyph(p, r, c, "brush");
            p.circle_filled(
                ctr + Vec2::new(k * 0.6, k * 0.5),
                k * 0.25,
                Color32::from_rgb(90, 140, 220),
            );
        }
        "marker" => {
            p.line_segment(
                [
                    ctr + Vec2::new(-k * 0.6, k * 0.6),
                    ctr + Vec2::new(k * 0.6, -k * 0.6),
                ],
                Stroke::new(3.0_f32, c),
            );
        }
        "eraser" => {
            p.add(Shape::convex_polygon(
                vec![
                    ctr + Vec2::new(-k * 0.9, k * 0.3),
                    ctr + Vec2::new(-k * 0.1, -k * 0.6),
                    ctr + Vec2::new(k * 0.6, k * 0.0),
                    ctr + Vec2::new(-k * 0.2, k * 0.9),
                ],
                Color32::from_gray(150),
                Stroke::new(1.0_f32, c),
            ));
        }
        "selpen" | "selers" => {
            glyph(p, Rect::from_min_size(r.min, r.size() * 0.6), c, "select");
            glyph(
                p,
                Rect::from_min_size(r.min + r.size() * 0.35, r.size() * 0.65),
                c,
                if name == "selpen" { "pen" } else { "eraser" },
            );
        }
        _ => {}
    }
}

/// Значок ступени жёсткости: профиль края от «колокола» до «ступеньки».
pub fn hardness_icon(p: &Painter, r: Rect, step: usize, c: Color32) {
    let n = 20;
    let mut pts = vec![r.left_bottom()];
    for i in 0..=n {
        let x = i as f32 / n as f32 * 2.0 - 1.0;
        let h = match step {
            0 => (1.0 - x * x).max(0.0).powf(1.5),
            1 => (1.0 - x.abs().powf(2.5)).max(0.0),
            2 => (1.0 - x.abs().powf(5.0)).max(0.0),
            3 => (1.0 - x.abs().powf(12.0)).max(0.0),
            _ => 1.0,
        };
        let w = if step == 4 { 0.85 } else { 1.0 };
        pts.push(Pos2::new(
            r.center().x + x * r.width() / 2.0 * w,
            r.bottom() - h * r.height(),
        ));
    }
    pts.push(r.right_bottom());
    p.add(Shape::convex_polygon(pts, c, Stroke::NONE));
}

pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> Color32 {
    let h6 = (h.rem_euclid(1.0)) * 6.0;
    let i = h6.floor();
    let f = h6 - i;
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    let (r, g, b) = match i as i32 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    Color32::from_rgb(
        (r * 255.0).round() as u8,
        (g * 255.0).round() as u8,
        (b * 255.0).round() as u8,
    )
}

pub fn rgb_to_hsv(c: Color32) -> [f32; 3] {
    let (r, g, b) = (
        c.r() as f32 / 255.0,
        c.g() as f32 / 255.0,
        c.b() as f32 / 255.0,
    );
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        ((g - b) / d).rem_euclid(6.0) / 6.0
    } else if max == g {
        ((b - r) / d + 2.0) / 6.0
    } else {
        ((r - g) / d + 4.0) / 6.0
    };
    [h, if max == 0.0 { 0.0 } else { d / max }, max]
}

/// Угол кольца для тона 0 (красный — слева вверху, как у образца); тон растёт по часовой.
const HUE0: f32 = -0.75 * PI;

/// Цветовой круг: кольцо тона и квадрат насыщенности (вправо) и яркости (вверх).
pub fn color_wheel(ui: &mut Ui, hsv: &mut [f32; 3], size: f32) -> bool {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click_and_drag());
    let ctr = rect.center();
    let outer = size / 2.0 - 2.0;
    let inner = outer - 14.0;
    let half = inner * 0.70;
    let square = Rect::from_center_size(ctr, Vec2::splat(half * 2.0));
    let mut changed = false;
    let drag_id = response.id.with("part");
    if response.drag_started() || response.clicked() {
        if let Some(pos) = response.interact_pointer_pos() {
            let ring = (pos - ctr).length() > inner - 2.0;
            ui.data_mut(|d| d.insert_temp(drag_id, ring));
        }
    }
    if let Some(pos) = response.interact_pointer_pos() {
        if response.is_pointer_button_down_on() || response.clicked() {
            let ring = ui.data(|d| d.get_temp::<bool>(drag_id)).unwrap_or(false);
            if ring {
                let a = (pos - ctr).angle();
                hsv[0] = ((a - HUE0) / TAU).rem_euclid(1.0);
            } else {
                hsv[1] = ((pos.x - square.left()) / square.width()).clamp(0.0, 1.0);
                hsv[2] = ((square.bottom() - pos.y) / square.height()).clamp(0.0, 1.0);
            }
            changed = true;
        }
    }
    let p = ui.painter();
    // кольцо
    let n = 120;
    let mut mesh = egui::Mesh::default();
    for i in 0..=n {
        let t = i as f32 / n as f32;
        let a = HUE0 + t * TAU;
        let dir = Vec2::new(a.cos(), a.sin());
        let c = hsv_to_rgb(t, 1.0, 1.0);
        mesh.colored_vertex(ctr + dir * outer, c);
        mesh.colored_vertex(ctr + dir * inner, c);
        if i > 0 {
            let b = (i * 2) as u32;
            mesh.add_triangle(b - 2, b - 1, b);
            mesh.add_triangle(b - 1, b + 1, b);
        }
    }
    p.add(Shape::mesh(mesh));
    p.circle_stroke(ctr, outer, Stroke::new(1.0_f32, Color32::from_gray(150)));
    p.circle_stroke(ctr, inner, Stroke::new(1.0_f32, Color32::from_gray(150)));
    // квадрат: белый — цвет тона — чёрный
    let mut sq = egui::Mesh::default();
    let hue = hsv_to_rgb(hsv[0], 1.0, 1.0);
    sq.colored_vertex(square.left_top(), Color32::WHITE);
    sq.colored_vertex(square.right_top(), hue);
    sq.colored_vertex(square.right_bottom(), Color32::BLACK);
    sq.colored_vertex(square.left_bottom(), Color32::BLACK);
    sq.add_triangle(0, 1, 2);
    sq.add_triangle(0, 2, 3);
    p.add(Shape::mesh(sq));
    p.rect_stroke(square, 0.0, Stroke::new(1.0_f32, Color32::from_gray(120)));
    // метки
    let a = HUE0 + hsv[0] * TAU;
    let m = ctr + Vec2::new(a.cos(), a.sin()) * (outer + inner) / 2.0;
    p.circle_stroke(m, 5.0, Stroke::new(2.0_f32, Color32::from_gray(60)));
    p.circle_stroke(m, 5.0, Stroke::new(1.0_f32, Color32::WHITE));
    let sv = Pos2::new(
        square.left() + hsv[1] * square.width(),
        square.bottom() - hsv[2] * square.height(),
    );
    p.circle_stroke(sv, 4.0, Stroke::new(2.0_f32, Color32::from_gray(40)));
    p.circle_stroke(sv, 4.0, Stroke::new(1.0_f32, Color32::WHITE));
    changed
}

/// Шахматка «прозрачного» фона.
pub fn checker(p: &Painter, r: Rect, cell: f32) {
    p.rect_filled(r, 0.0, Color32::WHITE);
    let (nx, ny) = (
        (r.width() / cell).ceil() as i32,
        (r.height() / cell).ceil() as i32,
    );
    for j in 0..ny {
        for i in 0..nx {
            if (i + j) % 2 == 1 {
                let c = Rect::from_min_size(
                    r.min + Vec2::new(i as f32 * cell, j as f32 * cell),
                    Vec2::splat(cell),
                )
                .intersect(r);
                p.rect_filled(c, 0.0, Color32::from_gray(200));
            }
        }
    }
}
