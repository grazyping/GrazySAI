//! Слой контура в GrazySAI: новый пустой слой, добавление пути от пера, пересчёт пикселей по кривым.
//! Кривые хранятся в слое в виде блока `.sai2` (`Layer::vector`), как у образца.

use crate::sai2_vector::{self, ControlPoint, CurveType, Path, PressurePoint, Shape, Style};
use paint_core::layer::VectorData;
use paint_core::vectorize::PathNode;
use paint_core::Layer;
use std::io;

/// Кривые пустого слоя контура.
pub fn empty() -> VectorData {
    VectorData { kind: *b"liwk", raw: sai2_vector::write(&[], true) }
}

/// Стиль нового штриха: цвет, толщина, плотность, ступень жёсткости 0…4 (4 — самая жёсткая).
#[derive(Clone, Copy, Debug)]
pub struct LineStyle {
    pub rgb: [u8; 3],
    pub size: f32,
    pub density: f32,
    pub hardness: u32,
}

/// Цвет в виде `scol`: B, G, R, A по 16 бит, 0x4000 — единица.
fn scol(rgb: [u8; 3]) -> [u8; 8] {
    let v = |c: u8| ((c as u32 * 0x4000 + 127) / 255) as u16;
    let mut out = [0u8; 8];
    for (i, x) in [v(rgb[2]), v(rgb[1]), v(rgb[0]), 0x4000].into_iter().enumerate() {
        out[i * 2..i * 2 + 2].copy_from_slice(&x.to_le_bytes());
    }
    out
}

/// Добавить путь штриха новым объектом поверх остальных. Возвращает новый блок.
pub fn append(raw: &[u8], nodes: &[PathNode], closed: bool, st: &LineStyle) -> io::Result<Vec<u8>> {
    append_many(raw, &[(nodes.to_vec(), closed)], st)
}

/// Добавить много путей (каждый — своим объектом) за один разбор блока.
pub fn append_many(raw: &[u8], paths: &[(Vec<PathNode>, bool)], st: &LineStyle) -> io::Result<Vec<u8>> {
    let mut shapes = sai2_vector::parse(raw, true)?;
    let mut next_shape = shapes.iter().map(|s| s.id).max().unwrap_or(0) + 1;
    let mut next_path = shapes.iter().flat_map(|s| &s.paths).map(|p| p.id).max().unwrap_or(0) + 1;
    let mut next_cp = shapes.iter().flat_map(|s| &s.paths).flat_map(|p| &p.points).map(|c| c.id).max().unwrap_or(0) + 1;
    let mut next_pp = shapes.iter().flat_map(|s| &s.paths).flat_map(|p| &p.points).flat_map(|c| &c.pressure).map(|q| q.id).max().unwrap_or(0) + 1;
    for (nodes, closed) in paths {
        let closed = *closed;
        let Some(first) = nodes.first() else { continue };
        // новый объект — последний в порядке наложения
        for s in shapes.iter_mut() {
            if s.next == 0 {
                s.next = next_shape;
            }
            for p in s.paths.iter_mut() {
                if p.next_in_layer == 0 {
                    p.next_in_layer = next_path;
                }
            }
        }
        let off = first.node.point;
        let rel = |q: [f64; 2]| [q[0] - off[0], q[1] - off[1]];
        let points = nodes
            .iter()
            .map(|n| {
                let id = next_cp;
                next_cp += 1;
                ControlPoint {
                    id,
                    point: rel(n.node.point),
                    handle_in: rel(n.node.handle_in),
                    handle_out: rel(n.node.handle_out),
                    weight: n.weight as f32,
                    second: 1.0,
                    pointed: n.pointed,
                    rest: [0; 3],
                    pressure: n
                        .pressure
                        .iter()
                        .map(|&(pos, w)| {
                            let id = next_pp;
                            next_pp += 1;
                            PressurePoint { id, pos, weight: w as f32, second: 1.0, rest: [0; 4] }
                        })
                        .collect(),
                }
            })
            .collect();
        shapes.push(Shape {
            id: next_shape,
            next: 0,
            style: Style::Linework {
                color: scol(st.rgb),
                size: st.size,
                density: st.density,
                params: [0.0, 1.0, 0.0],
                hardness: st.hardness.min(4),
                amplify: 0.0,
                flags: [true, false],
            },
            paths: vec![Path { id: next_path, next_in_layer: 0, offset: off, curve: CurveType::Normal, closed, rest: [0; 2], points }],
        });
        next_shape += 1;
        next_path += 1;
    }
    Ok(sai2_vector::write(&shapes, true))
}

/// Пиксели слоя заново по его кривым (слой контура или фигур).
pub fn render(layer: &Layer) -> io::Result<paint_core::tile::ColorMap> {
    let mut l = Layer::new("");
    let Some(v) = &layer.vector else { return Ok(layer.pixels.clone()) };
    let linework = &v.kind == b"liwk";
    let shapes = sai2_vector::parse(&v.raw, linework)?;
    if linework {
        crate::sai2_doc::render_linework(&mut l, &shapes);
    } else {
        crate::sai2_doc::render_shapes(&mut l.pixels, &shapes);
    }
    Ok(l.pixels)
}

/// Узлы пути в координатах холста.
fn nodes_of(p: &Path) -> Vec<paint_core::vector::Node> {
    let at = |q: [f64; 2]| [q[0] + p.offset[0], q[1] + p.offset[1]];
    p.points.iter().map(|c| paint_core::vector::Node { point: at(c.point), handle_in: at(c.handle_in), handle_out: at(c.handle_out) }).collect()
}

/// Какой путь под точкой (x, y): (объект, путь). Ищется верхний (последний в порядке наложения),
/// расстояние до кривой не больше `reach` плюс половина толщины.
pub fn hit(shapes: &[Shape], x: f64, y: f64, reach: f64) -> Option<(usize, usize)> {
    let mut best: Option<(usize, usize)> = None;
    for (si, sh) in shapes.iter().enumerate() {
        let half = match &sh.style {
            Style::Linework { size, .. } => *size as f64 * 0.5,
            _ => 0.0,
        };
        for (pi, p) in sh.paths.iter().enumerate() {
            let pts = paint_core::vector::flatten(&nodes_of(p), p.closed, 16);
            let near = pts.windows(2).any(|w| {
                let (a, b) = (w[0], w[1]);
                let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
                let l2 = dx * dx + dy * dy;
                let t = if l2 > 0.0 { (((x - a[0]) * dx + (y - a[1]) * dy) / l2).clamp(0.0, 1.0) } else { 0.0 };
                let (cx, cy) = (a[0] + dx * t, a[1] + dy * t);
                (x - cx).hypot(y - cy) <= reach + half
            }) || (pts.len() == 1 && (x - pts[0][0]).hypot(y - pts[0][1]) <= reach + half);
            if near {
                best = Some((si, pi));
            }
        }
    }
    best
}

/// Ближайший узел к (x, y) не дальше `reach`: (объект, путь, узел).
pub fn hit_node(shapes: &[Shape], x: f64, y: f64, reach: f64) -> Option<(usize, usize, usize)> {
    let mut best: Option<((usize, usize, usize), f64)> = None;
    for (si, sh) in shapes.iter().enumerate() {
        for (pi, p) in sh.paths.iter().enumerate() {
            for (ni, c) in p.points.iter().enumerate() {
                let d = (c.point[0] + p.offset[0] - x).hypot(c.point[1] + p.offset[1] - y);
                if d <= reach && best.is_none_or(|b| d < b.1) {
                    best = Some(((si, pi, ni), d));
                }
            }
        }
    }
    best.map(|b| b.0)
}

/// Удалить путь; объект без путей удаляется. Цепочки «следующий» связываются заново.
pub fn delete_path(shapes: &mut Vec<Shape>, si: usize, pi: usize) {
    let Some(sh) = shapes.get_mut(si) else { return };
    if pi >= sh.paths.len() {
        return;
    }
    let gone = sh.paths.remove(pi);
    for p in shapes.iter_mut().flat_map(|s| s.paths.iter_mut()) {
        if p.next_in_layer == gone.id {
            p.next_in_layer = gone.next_in_layer;
        }
    }
    if shapes[si].paths.is_empty() {
        let gone = shapes.remove(si);
        for s in shapes.iter_mut() {
            if s.next == gone.id {
                s.next = gone.next;
            }
        }
    }
}

/// Пересчитать ручки пути вида «Normal» по его точкам.
pub fn update_handles(p: &mut Path) {
    if p.curve != CurveType::Normal {
        return;
    }
    let pts: Vec<[f64; 2]> = p.points.iter().map(|c| c.point).collect();
    let pointed: Vec<bool> = p.points.iter().map(|c| c.pointed).collect();
    for (c, (hi, ho)) in p.points.iter_mut().zip(paint_core::vectorize::normal_handles(&pts, &pointed, p.closed)) {
        c.handle_in = hi;
        c.handle_out = ho;
    }
}

/// Сдвинуть узел в точку холста (x, y); ручки — заново.
pub fn move_node(shapes: &mut [Shape], (si, pi, ni): (usize, usize, usize), x: f64, y: f64) {
    let Some(p) = shapes.get_mut(si).and_then(|s| s.paths.get_mut(pi)) else { return };
    let Some(c) = p.points.get_mut(ni) else { return };
    let new = [x - p.offset[0], y - p.offset[1]];
    let (dx, dy) = (new[0] - c.point[0], new[1] - c.point[1]);
    c.point = new;
    // у вида «Cubic Bezier» ручки едут вместе с узлом
    c.handle_in = [c.handle_in[0] + dx, c.handle_in[1] + dy];
    c.handle_out = [c.handle_out[0] + dx, c.handle_out[1] + dy];
    update_handles(p);
}

/// Задать толщину объекта (инструмент «Weight»).
pub fn set_size(shapes: &mut [Shape], si: usize, value: f32) {
    if let Some(Style::Linework { size, .. }) = shapes.get_mut(si).map(|s| &mut s.style) {
        *size = value;
    }
}

/// Задать цвет объекта (инструмент «Color»).
pub fn set_color(shapes: &mut [Shape], si: usize, rgb: [u8; 3]) {
    if let Some(Style::Linework { color, .. }) = shapes.get_mut(si).map(|s| &mut s.style) {
        *color = scol(rgb);
    }
}

/// Задать нажим узла 0..1 (инструмент «Pressure»).
pub fn set_node_pressure(shapes: &mut [Shape], (si, pi, ni): (usize, usize, usize), value: f32) {
    if let Some(c) = shapes.get_mut(si).and_then(|s| s.paths.get_mut(pi)).and_then(|p| p.points.get_mut(ni)) {
        c.weight = value.clamp(0.0, 1.0);
    }
}

/// Пиксели по разобранным кривым слоя контура.
pub fn render_shapes(shapes: &[Shape]) -> paint_core::tile::ColorMap {
    let mut l = Layer::new("");
    crate::sai2_doc::render_linework(&mut l, shapes);
    l.pixels
}

/// Вид фигуры инструмента фигур.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShapeKind {
    Rectangle,
    Ellipse,
    Triangle,
}

/// Кривые пустого слоя фигур.
pub fn empty_shapes() -> VectorData {
    VectorData { kind: *b"shap", raw: sai2_vector::write(&[], false) }
}

/// Добавить фигуру в рамке (x0, y0)–(x1, y1) цветом `rgb` поверх остальных — как шаблоны SAI:
/// прямоугольник и треугольник — острые узлы от центра, эллипс — четыре узла «Cubic Bezier»
/// с ручками 0,5523 полуоси.
pub fn append_shape(raw: &[u8], kind: ShapeKind, x0: f64, y0: f64, x1: f64, y1: f64, rgb: [u8; 3]) -> io::Result<Vec<u8>> {
    let mut shapes = sai2_vector::parse(raw, false)?;
    let next_shape = shapes.iter().map(|s| s.id).max().unwrap_or(0) + 1;
    let next_path = shapes.iter().flat_map(|s| &s.paths).map(|p| p.id).max().unwrap_or(0) + 1;
    let mut next_cp = shapes.iter().flat_map(|s| &s.paths).flat_map(|p| &p.points).map(|c| c.id).max().unwrap_or(0) + 1;
    for s in shapes.iter_mut() {
        if s.next == 0 {
            s.next = next_shape;
        }
        for p in s.paths.iter_mut() {
            if p.next_in_layer == 0 {
                p.next_in_layer = next_path;
            }
        }
    }
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let (rx, ry) = ((x1 - x0).abs() / 2.0, (y1 - y0).abs() / 2.0);
    let mut cp = |point: [f64; 2], hin: [f64; 2], hout: [f64; 2], pointed: bool| {
        let id = next_cp;
        next_cp += 1;
        ControlPoint { id, point, handle_in: hin, handle_out: hout, weight: 0.0, second: 0.0, pointed, rest: [0; 3], pressure: Vec::new() }
    };
    let sharp = |p: [f64; 2]| (p, p, p);
    let (curve, points) = match kind {
        ShapeKind::Rectangle => (
            CurveType::Normal,
            [[-rx, -ry], [rx, -ry], [rx, ry], [-rx, ry]].map(sharp).into_iter().map(|(p, a, b)| cp(p, a, b, true)).collect(),
        ),
        ShapeKind::Triangle => (
            CurveType::Normal,
            [[0.0, -ry], [rx, ry], [-rx, ry]].map(sharp).into_iter().map(|(p, a, b)| cp(p, a, b, true)).collect(),
        ),
        ShapeKind::Ellipse => {
            const K: f64 = 0.552_284_749_830_793_6;
            let (kx, ky) = (rx * K, ry * K);
            (
                CurveType::Bezier,
                vec![
                    cp([0.0, -ry], [-kx, -ry], [kx, -ry], false),
                    cp([rx, 0.0], [rx, -ky], [rx, ky], false),
                    cp([0.0, ry], [kx, ry], [-kx, ry], false),
                    cp([-rx, 0.0], [-rx, ky], [-rx, -ky], false),
                ],
            )
        }
    };
    shapes.push(Shape {
        id: next_shape,
        next: 0,
        style: Style::Fill { color: scol(rgb) },
        paths: vec![Path { id: next_path, next_in_layer: 0, offset: [cx, cy], curve, closed: true, rest: [0; 2], points }],
    });
    Ok(sai2_vector::write(&shapes, false))
}

/// Пиксели слоя по любым кривым (контур или фигуры).
pub fn render_layer_shapes(shapes: &[Shape], linework: bool) -> paint_core::tile::ColorMap {
    let mut l = Layer::new("");
    if linework {
        crate::sai2_doc::render_linework(&mut l, shapes);
    } else {
        crate::sai2_doc::render_shapes(&mut l.pixels, shapes);
    }
    l.pixels
}

#[cfg(test)]
mod tests {
    use super::*;
    use paint_core::vectorize::{vectorize, StrokePoint};

    #[test]
    fn stroke_goes_into_a_new_linework_layer_and_back() {
        let raw: Vec<StrokePoint> = (0..=100).map(|i| StrokePoint { at: [10.0 + i as f64, 20.0 + (i as f64 / 10.0).sin() * 5.0], pressure: 0.3 + 0.007 * i as f64 }).collect();
        let (nodes, closed) = vectorize(&raw, 1.0).unwrap();
        let st = LineStyle { rgb: [200, 30, 40], size: 6.0, density: 1.0, hardness: 4 };
        let mut layer = Layer::new("контур");
        layer.vector = Some(empty());
        let block = append(&layer.vector.as_ref().unwrap().raw, &nodes, closed, &st).unwrap();
        let block = append(&block, &nodes, closed, &st).unwrap();
        let shapes = sai2_vector::parse(&block, true).unwrap();
        assert_eq!(shapes.len(), 2);
        assert_eq!((shapes[0].next, shapes[1].next), (shapes[1].id, 0));
        assert_eq!(sai2_vector::write(&shapes, true), block, "запись устойчива");
        layer.vector = Some(VectorData { kind: *b"liwk", raw: block });
        let px = render(&layer).unwrap();
        let p = paint_core::pixel::to_rgba8(px.get(60, (20.0 + 5.0f64.sin() * 5.0).round() as i32));
        assert!(p[3] > 200 && p[0] > 150 && p[1] < 80, "линия нарисована цветом стиля: {p:?}");
    }

    #[test]
    fn shapes_follow_the_sai_templates() {
        let mut raw = empty_shapes().raw;
        raw = append_shape(&raw, ShapeKind::Rectangle, 10.0, 10.0, 50.0, 30.0, [255, 0, 0]).unwrap();
        raw = append_shape(&raw, ShapeKind::Ellipse, 60.0, 10.0, 157.0, 104.0, [0, 0, 255]).unwrap();
        raw = append_shape(&raw, ShapeKind::Triangle, 10.0, 40.0, 49.0, 75.5, [0, 255, 0]).unwrap();
        let shapes = sai2_vector::parse(&raw, false).unwrap();
        assert_eq!(shapes.len(), 3);
        let e = &shapes[1].paths[0];
        assert_eq!(e.curve, CurveType::Bezier);
        assert!((e.points[0].handle_out[0] - 26.786).abs() < 0.01, "ручка эллипса как у SAI");
        assert_eq!(shapes[2].paths[0].points[0].point, [0.0, -17.75]);
        let px = render_layer_shapes(&shapes, false);
        let at = |x, y| paint_core::pixel::to_rgba8(px.get(x, y));
        assert_eq!(at(30, 20), [255, 0, 0, 255]);
        assert_eq!(at(108, 57), [0, 0, 255, 255]);
        assert_eq!(at(29, 65), [0, 255, 0, 255]);
        assert_eq!(at(55, 20)[3], 0);
    }

    #[test]
    fn edit_tools_change_paths() {
        let raw: Vec<StrokePoint> = (0..=60).map(|i| StrokePoint { at: [10.0 + i as f64, 30.0], pressure: 1.0 }).collect();
        let (nodes, closed) = vectorize(&raw, 1.0).unwrap();
        let st = LineStyle { rgb: [0, 0, 0], size: 4.0, density: 1.0, hardness: 4 };
        let mut block = empty().raw;
        for _ in 0..3 {
            block = append(&block, &nodes, closed, &st).unwrap();
        }
        let mut shapes = sai2_vector::parse(&block, true).unwrap();
        assert_eq!(hit(&shapes, 40.0, 31.0, 1.0), Some((2, 0)), "верхний путь под точкой");
        assert_eq!(hit(&shapes, 40.0, 60.0, 1.0), None);
        set_size(&mut shapes, 2, 12.0);
        set_color(&mut shapes, 2, [255, 0, 0]);
        let n = hit_node(&shapes, 70.0, 30.0, 3.0).unwrap();
        move_node(&mut shapes, n, 70.0, 50.0);
        set_node_pressure(&mut shapes, n, 0.25);
        delete_path(&mut shapes, 1, 0);
        assert_eq!(shapes.len(), 2);
        assert_eq!(shapes[0].next, shapes[1].id, "цепочка связана заново");
        let again = sai2_vector::parse(&sai2_vector::write(&shapes, true), true).unwrap();
        assert_eq!(again, shapes);
        let px = render_shapes(&shapes);
        assert!(paint_core::pixel::to_rgba8(px.get(69, 49))[3] > 0, "сдвинутый узел нарисован");
    }
}
