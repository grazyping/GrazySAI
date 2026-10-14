//! Слой контура в SOsai: новый пустой слой, добавление пути от пера, пересчёт пикселей по кривым.
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
    let mut shapes = sai2_vector::parse(raw, true)?;
    let next_shape = shapes.iter().map(|s| s.id).max().unwrap_or(0) + 1;
    let next_path = shapes.iter().flat_map(|s| &s.paths).map(|p| p.id).max().unwrap_or(0) + 1;
    let mut next_cp = shapes.iter().flat_map(|s| &s.paths).flat_map(|p| &p.points).map(|c| c.id).max().unwrap_or(0) + 1;
    let mut next_pp = shapes.iter().flat_map(|s| &s.paths).flat_map(|p| &p.points).flat_map(|c| &c.pressure).map(|q| q.id).max().unwrap_or(0) + 1;
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
    let Some(first) = nodes.first() else { return Ok(raw.to_vec()) };
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
}
