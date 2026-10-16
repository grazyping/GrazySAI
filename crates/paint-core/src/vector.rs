//! Векторные фигуры: кривые Безье из узлов и их заливка в пиксели.
//! Устройство как у образца — docs/11-linework-shapes.md, разделы 4 и 5.

use crate::pixel::{Px, ONE};
use crate::tile::ColorMap;

pub type Pt = [f64; 2];

/// Узел пути в координатах холста.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Node {
    pub point: Pt,
    pub handle_in: Pt,
    pub handle_out: Pt,
}

/// Отрезок пути — кубическая кривая: точка, её исходящая ручка, входящая ручка следующей, следующая.
pub fn segments(nodes: &[Node], closed: bool) -> Vec<[Pt; 4]> {
    let n = nodes.len();
    let count = if closed { n } else { n.saturating_sub(1) };
    (0..count)
        .map(|i| {
            let (a, b) = (nodes[i], nodes[(i + 1) % n]);
            [a.point, a.handle_out, b.handle_in, b.point]
        })
        .collect()
}

pub fn cubic_at(s: &[Pt; 4], t: f64) -> Pt {
    let u = 1.0 - t;
    let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
    [
        a * s[0][0] + b * s[1][0] + c * s[2][0] + d * s[3][0],
        a * s[0][1] + b * s[1][1] + c * s[2][1] + d * s[3][1],
    ]
}

/// Ломаная по пути: каждый отрезок делится на `parts` равных по параметру частей
/// (образец при заливке делит на 16).
pub fn flatten(nodes: &[Node], closed: bool, parts: usize) -> Vec<Pt> {
    let mut out = Vec::new();
    for s in segments(nodes, closed) {
        if out.is_empty() {
            out.push(s[0]);
        }
        for k in 1..=parts {
            out.push(cubic_at(&s, k as f64 / parts as f64));
        }
    }
    out
}

/// Покрытие пикселей многоугольниками (правило «ненулевой обмотки»): доля площади пикселя
/// 0..=1. По вертикали — `SUB` подстрок на пиксель, по горизонтали — точные доли.
/// Возвращает левый верхний угол, ширину, высоту и покрытие по строкам.
pub fn coverage(polys: &[Vec<Pt>]) -> (i32, i32, usize, usize, Vec<f32>) {
    const SUB: usize = 16;
    let pts = polys.iter().flatten();
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for p in pts {
        x0 = x0.min(p[0]);
        y0 = y0.min(p[1]);
        x1 = x1.max(p[0]);
        y1 = y1.max(p[1]);
    }
    if x0 > x1 || !(x0.is_finite() && y0.is_finite() && x1.is_finite() && y1.is_finite()) {
        return (0, 0, 0, 0, Vec::new());
    }
    // считается только там, где может быть холст (стороны до 100 000 px, с запасом): испорченная
    // фигура в миллионы пикселей иначе требует сотни гигабайт
    const LO: f64 = -4096.0;
    const HI: f64 = 104_096.0;
    let (left, top) = (x0.clamp(LO, HI).floor() as i32, y0.clamp(LO, HI).floor() as i32);
    let (right, bottom) = (x1.clamp(LO, HI).ceil() as i32, y1.clamp(LO, HI).ceil() as i32);
    let (w, h) = ((right - left).max(1) as usize, (bottom - top).max(1) as usize);
    if w * h > 1 << 29 {
        return (0, 0, 0, 0, Vec::new());
    }
    let mut cov = vec![0f32; w * h];
    let mut cross: Vec<(f64, i32)> = Vec::new();
    for row in 0..h {
        for sub in 0..SUB {
            let y = top as f64 + row as f64 + (sub as f64 + 0.5) / SUB as f64;
            cross.clear();
            for poly in polys {
                for i in 0..poly.len() {
                    let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
                    if (a[1] <= y) != (b[1] <= y) {
                        let t = (y - a[1]) / (b[1] - a[1]);
                        cross.push((a[0] + t * (b[0] - a[0]), if b[1] > a[1] { 1 } else { -1 }));
                    }
                }
            }
            cross.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut wind = 0;
            for i in 0..cross.len() {
                let was = wind;
                wind += cross[i].1;
                if was == 0 && wind != 0 {
                    // начало отрезка заливки; конец — где обмотка снова станет нулём
                    let start = cross[i].0;
                    let mut j = i + 1;
                    let mut wj = wind;
                    while j < cross.len() {
                        wj += cross[j].1;
                        if wj == 0 {
                            break;
                        }
                        j += 1;
                    }
                    let end = cross.get(j).map_or(start, |c| c.0);
                    add_span(&mut cov[row * w..(row + 1) * w], start - left as f64, end - left as f64, 1.0 / SUB as f32);
                }
            }
        }
    }
    (left, top, w, h, cov)
}

fn add_span(row: &mut [f32], a: f64, b: f64, k: f32) {
    let (a, b) = (a.max(0.0), b.min(row.len() as f64));
    if b <= a {
        return;
    }
    let (ia, ib) = (a.floor() as usize, b.floor() as usize);
    if ia == ib {
        row[ia] += (b - a) as f32 * k;
        return;
    }
    row[ia] += (ia as f64 + 1.0 - a) as f32 * k;
    let n = row.len();
    for v in &mut row[ia + 1..ib.min(n)] {
        *v += k;
    }
    if ib < row.len() {
        row[ib] += (b - ib as f64) as f32 * k;
    }
}

/// Непрозрачность края фигуры по доле площади пикселя. Подобрано по краям фигур в файле,
/// сохранённом образцом (`пинг4.sai2`, 589 краевых пикселей, отклонение 0,6/255): образец
/// делает края светлее точной площади — до c ≈ 0,85 примерно 0,84·c, дальше круто к 1.
pub fn shape_opacity(c: f32) -> f32 {
    c * (1.0 - 0.16 * (1.0 - c.powi(10)))
}

/// Залить пути одной фигуры цветом `color` (предумноженный, наш формат) поверх `pixels`.
pub fn fill_shape(pixels: &mut ColorMap, paths: &[(Vec<Node>, bool)], color: Px) {
    let polys: Vec<Vec<Pt>> = paths.iter().map(|(n, closed)| flatten(n, *closed, 16)).filter(|p| p.len() >= 3).collect();
    let (left, top, w, h, cov) = coverage(&polys);
    for y in 0..h {
        for x in 0..w {
            let c = shape_opacity(cov[y * w + x].min(1.0));
            if c <= 0.0 {
                continue;
            }
            let (px, py) = (left + x as i32, top + y as i32);
            let dst = pixels.get(px, py);
            let src = color.map(|v| (v as f32 * c) as u32);
            let inv = ONE - src[3].min(ONE);
            let out: Px = std::array::from_fn(|i| (src[i] + (dst[i] as u32 * inv + ONE / 2) / ONE).min(ONE) as u16);
            pixels.set(px, py, out);
        }
    }
}

/// Опорное значение нажима на пути: место (длина дуги от начала пути) и значение.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Knot {
    pub at: f64,
    pub value: f64,
}

/// Наклоны нажима по правилу образца (`vec_pressure_handles`): если наклоны слева и справа
/// разного знака — ровно; иначе меньший, подтянутый к среднему с весом (меньший/больший)².
fn knot_slope(prev: Knot, cur: Knot, next: Knot) -> f64 {
    let dl = cur.at - prev.at;
    let dr = next.at - cur.at;
    let l = if dl > 0.0 { (cur.value - prev.value) / dl } else { 0.0 };
    let r = if dr > 0.0 { (next.value - cur.value) / dr } else { 0.0 };
    if l * r < 0.0 {
        return 0.0;
    }
    let (small, big) = if l.abs() < r.abs() { (l, r) } else { (r, l) };
    if big == 0.0 {
        return 0.0;
    }
    let w = (small.abs() / big.abs()).powi(2);
    small + ((small + big) * 0.5 - small) * w
}

/// Нажим в точке `at` (длина дуги): кубика между опорными значениями, не ниже нуля.
pub fn pressure_at(knots: &[Knot], at: f64) -> f64 {
    let n = knots.len();
    if n == 0 {
        return 1.0;
    }
    if n == 1 || at <= knots[0].at {
        return knots[0].value;
    }
    if at >= knots[n - 1].at {
        return knots[n - 1].value;
    }
    let i = knots.windows(2).position(|w| at <= w[1].at).unwrap_or(n - 2);
    let get = |k: isize| -> Knot {
        if k < 0 {
            // за началом — зеркально, как у образца
            Knot { at: 2.0 * knots[0].at - knots[1].at, value: 2.0 * knots[0].value - knots[1].value }
        } else if k as usize >= n {
            Knot { at: 2.0 * knots[n - 1].at - knots[n - 2].at, value: 2.0 * knots[n - 1].value - knots[n - 2].value }
        } else {
            knots[k as usize]
        }
    };
    let (a, b) = (knots[i], knots[i + 1]);
    let d = b.at - a.at;
    if d <= 0.0 {
        return b.value;
    }
    let sa = knot_slope(get(i as isize - 1), a, b);
    let sb = knot_slope(a, b, get(i as isize + 2));
    let (p1, p2) = ((a.value + sa * d / 3.0).max(0.0), (b.value - sb * d / 3.0).max(0.0));
    let t = (at - a.at) / d;
    let u = 1.0 - t;
    (u * u * u * a.value + 3.0 * u * u * t * p1 + 3.0 * u * t * t * p2 + t * t * t * b.value).max(0.0)
}

/// Больше точек на путь не бывает у настоящих рисунков (1 млн px длины при шаге 0,25).
const MAX_SAMPLES: f64 = 4_000_000.0;

/// Точки вдоль пути через `step` px длины дуги с нажимом. `pressure_points[i]` — точки нажима
/// отрезка i: (доля длины дуги отрезка, значение); `weights[i]` — нажим узла i.
pub fn sample_path(nodes: &[Node], closed: bool, weights: &[f64], pressure_points: &[Vec<(f64, f64)>], step: f64) -> Vec<(Pt, f64)> {
    const FINE: usize = 256;
    let segs = segments(nodes, closed);
    // мелкая ломаная и длины дуги
    let mut poly: Vec<(Pt, f64)> = Vec::new();
    let mut seg_start = Vec::new();
    let mut s = 0.0;
    for seg in &segs {
        seg_start.push(s);
        if poly.is_empty() {
            poly.push((seg[0], 0.0));
        }
        for k in 1..=FINE {
            let p = cubic_at(seg, k as f64 / FINE as f64);
            let q = poly.last().unwrap().0;
            s += ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2)).sqrt();
            poly.push((p, s));
        }
    }
    seg_start.push(s);
    // испорченный путь (координаты не числа или кривая в миллионы пикселей) — не рисуется:
    // иначе миллиарды точек и вся память
    if !s.is_finite() || s / step.max(1e-6) > MAX_SAMPLES {
        return Vec::new();
    }
    let mut knots = Vec::new();
    for (i, w) in weights.iter().enumerate().take(seg_start.len()) {
        knots.push(Knot { at: seg_start[i], value: *w });
        if i + 1 < seg_start.len() {
            let len = seg_start[i + 1] - seg_start[i];
            for &(pos, v) in pressure_points.get(i).map(|v| v.as_slice()).unwrap_or(&[]) {
                knots.push(Knot { at: seg_start[i] + pos * len, value: v });
            }
        }
    }
    if closed && !weights.is_empty() {
        knots.push(Knot { at: s, value: weights[0] });
    }
    let mut out = Vec::new();
    let mut j = 0;
    let mut at = 0.0;
    while at <= s && !poly.is_empty() {
        while j + 1 < poly.len() && poly[j + 1].1 < at {
            j += 1;
        }
        let p = if j + 1 < poly.len() {
            let (a, b) = (poly[j], poly[j + 1]);
            let t = if b.1 > a.1 { (at - a.1) / (b.1 - a.1) } else { 0.0 };
            [a.0[0] + (b.0[0] - a.0[0]) * t, a.0[1] + (b.0[1] - a.0[1]) * t]
        } else {
            poly[j].0
        };
        out.push((p, pressure_at(&knots, at)));
        at += step;
    }
    out
}
