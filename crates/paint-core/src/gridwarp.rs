//! Деформация сеткой (трансформация SAI 2 Alpha 2026: «Решётка» и «Сетка», деление 2×2…5×5 или
//! своё). Узлы сетки двигаются; точки между ними — по сдвигам узлов (сверено с живой SAI,
//! docs/41):
//! - «Решётка» (Lattice) — узлы тянут картинку мягко, сама картинка до узла не доходит: сдвиг —
//!   сумма сдвигов узлов с весами квадратичного B-сплайна (при 2×2 середина уходит на ¼ сдвига
//!   среднего узла — как в SAI);
//! - «Сетка» (Mesh) — картинка проходит через узлы: сдвиги узлов сглажены сплайном Катмулла — Рома.
//!
//! Обратное отображение строится растеризацией мелких треугольников; где картинка складывается
//! сама на себя, «Перезапись» оставляет верхний слой, «Смешивание» кладёт его поверх нижнего.

use crate::selection::Selection;
use crate::tile::{ColorMap, MaskMap};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum GridKind {
    #[default]
    Lattice,
    Mesh,
}

/// Сетка над прямоугольником `src` (x0, y0, x1, y1): `cols`×`rows` клеток, узлы построчно
/// ((rows + 1)·(cols + 1) штук) — их места на холсте.
#[derive(Clone, Debug, PartialEq)]
pub struct GridWarp {
    pub src: [f64; 4],
    pub cols: usize,
    pub rows: usize,
    pub points: Vec<[f64; 2]>,
    pub kind: GridKind,
    /// «Смешивание» складок (иначе «Перезапись»).
    pub blend: bool,
}

impl GridWarp {
    /// Нетронутая сетка: узлы на равных расстояниях.
    pub fn new(src: [f64; 4], cols: usize, rows: usize, kind: GridKind) -> Self {
        let (cols, rows) = (cols.clamp(1, 32), rows.clamp(1, 32));
        let points = (0..=rows).flat_map(|j| (0..=cols).map(move |i| (i, j))).map(|(i, j)| home(src, cols, rows, i, j)).collect();
        Self { src, cols, rows, points, kind, blend: false }
    }

    pub fn point(&self, i: usize, j: usize) -> [f64; 2] {
        self.points[j * (self.cols + 1) + i]
    }

    /// Сдвиг узла от его места в нетронутой сетке.
    fn shift(&self, i: usize, j: usize) -> [f64; 2] {
        let p = self.point(i, j);
        let h = home(self.src, self.cols, self.rows, i, j);
        [p[0] - h[0], p[1] - h[1]]
    }

    /// Точка источника `(x, y)` → точка результата.
    pub fn apply(&self, x: f64, y: f64) -> (f64, f64) {
        let [x0, y0, x1, y1] = self.src;
        let u = ((x - x0) / (x1 - x0).max(1e-9)).clamp(0.0, 1.0);
        let v = ((y - y0) / (y1 - y0).max(1e-9)).clamp(0.0, 1.0);
        let wu = weights(self.kind, self.cols, u);
        let wv = weights(self.kind, self.rows, v);
        let (mut dx, mut dy) = (0.0, 0.0);
        for (j, &b) in wv.iter().enumerate() {
            if b == 0.0 {
                continue;
            }
            for (i, &a) in wu.iter().enumerate() {
                if a == 0.0 {
                    continue;
                }
                let d = self.shift(i, j);
                dx += a * b * d[0];
                dy += a * b * d[1];
            }
        }
        (x + dx, y + dy)
    }

    /// Точки линий сетки для показа: по каждой линии узлов — ломаная из `steps` отрезков на клетку
    /// (у «Решётки» — прямые между узлами, как в SAI; у «Сетки» — кривые через узлы).
    pub fn guide_lines(&self, steps: usize) -> Vec<Vec<[f64; 2]>> {
        let mut out = Vec::new();
        if self.kind == GridKind::Lattice {
            for j in 0..=self.rows {
                out.push((0..=self.cols).map(|i| self.point(i, j)).collect());
            }
            for i in 0..=self.cols {
                out.push((0..=self.rows).map(|j| self.point(i, j)).collect());
            }
            return out;
        }
        let [x0, y0, x1, y1] = self.src;
        for j in 0..=self.rows {
            let y = y0 + (y1 - y0) * j as f64 / self.rows as f64;
            out.push((0..=self.cols * steps).map(|k| {
                let x = x0 + (x1 - x0) * k as f64 / (self.cols * steps) as f64;
                let (a, b) = self.apply(x, y);
                [a, b]
            }).collect());
        }
        for i in 0..=self.cols {
            let x = x0 + (x1 - x0) * i as f64 / self.cols as f64;
            out.push((0..=self.rows * steps).map(|k| {
                let y = y0 + (y1 - y0) * k as f64 / (self.rows * steps) as f64;
                let (a, b) = self.apply(x, y);
                [a, b]
            }).collect());
        }
        out
    }
}

fn home(src: [f64; 4], cols: usize, rows: usize, i: usize, j: usize) -> [f64; 2] {
    [src[0] + (src[2] - src[0]) * i as f64 / cols as f64, src[1] + (src[3] - src[1]) * j as f64 / rows as f64]
}

/// Веса узлов `0..=n` в точке `t` (0..1).
fn weights(kind: GridKind, n: usize, t: f64) -> Vec<f64> {
    let mut w = vec![0.0; n + 1];
    match kind {
        GridKind::Lattice => {
            // зажатый B-сплайн степени min(2, n) с равными промежутками узлов
            let p = n.min(2);
            let inner = n + 1 - p; // число промежутков
            let mut knots = vec![0.0; p + 1];
            knots.extend((1..inner).map(|k| k as f64 / inner as f64));
            knots.extend(std::iter::repeat_n(1.0, p + 1));
            for (i, wi) in w.iter_mut().enumerate() {
                *wi = basis(i, p, t, &knots);
            }
        }
        GridKind::Mesh => {
            // Катмулл — Ром по узлам; за краем — продолжение по прямой
            let x = t * n as f64;
            let k = (x.floor() as usize).min(n - 1);
            let s = x - k as f64;
            let c = [
                -0.5 * s * s * s + s * s - 0.5 * s,
                1.5 * s * s * s - 2.5 * s * s + 1.0,
                -1.5 * s * s * s + 2.0 * s * s + 0.5 * s,
                0.5 * s * s * s - 0.5 * s * s,
            ];
            for (m, &cm) in c.iter().enumerate() {
                let idx = k as isize - 1 + m as isize;
                if idx < 0 {
                    // P₋₁ = 2·P₀ − P₁
                    w[0] += 2.0 * cm;
                    w[1] -= cm;
                } else if idx > n as isize {
                    w[n] += 2.0 * cm;
                    w[n - 1] -= cm;
                } else {
                    w[idx as usize] += cm;
                }
            }
        }
    }
    w
}

/// Базисная функция B-сплайна (Кокс — де Бур).
fn basis(i: usize, p: usize, t: f64, k: &[f64]) -> f64 {
    if p == 0 {
        let last = t >= 1.0 && k[i + 1] >= 1.0 && k[i] < 1.0;
        return if (k[i] <= t && t < k[i + 1]) || last { 1.0 } else { 0.0 };
    }
    let a = if k[i + p] > k[i] { (t - k[i]) / (k[i + p] - k[i]) * basis(i, p - 1, t, k) } else { 0.0 };
    let b = if k[i + p + 1] > k[i + 1] { (k[i + p + 1] - t) / (k[i + p + 1] - k[i + 1]) * basis(i + 1, p - 1, t, k) } else { 0.0 };
    a + b
}

/// Пиксели слоя после деформации сеткой (как `selection::warp_pixels`).
pub fn warp_pixels(src: &ColorMap, sel: &Selection, g: &GridWarp, mode: crate::resample::Mode) -> (ColorMap, MaskMap) {
    let whole = sel.mask.tiles.is_empty();
    let bounds = if whole { crate::selection::content_bounds(src) } else { sel.bounds() };
    if bounds.is_none() {
        return (src.clone(), sel.mask.clone());
    }
    let [x0, y0, x1, y1] = g.src;
    // мелкая сетка: клетки не крупнее ~6 px источника, не больше 400 по стороне
    let nu = (((x1 - x0) / 6.0).ceil() as usize).clamp(g.cols * 4, 400);
    let nv = (((y1 - y0) / 6.0).ceil() as usize).clamp(g.rows * 4, 400);
    let fwd: Vec<[f64; 4]> = (0..=nv)
        .flat_map(|j| (0..=nu).map(move |i| (i, j)))
        .map(|(i, j)| {
            let sx = x0 + (x1 - x0) * i as f64 / nu as f64;
            let sy = y0 + (y1 - y0) * j as f64 / nv as f64;
            let (dx, dy) = g.apply(sx, sy);
            [sx, sy, dx, dy]
        })
        .collect();
    let (mut bx0, mut by0, mut bx1, mut by1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for p in &fwd {
        bx0 = bx0.min(p[2]);
        by0 = by0.min(p[3]);
        bx1 = bx1.max(p[2]);
        by1 = by1.max(p[3]);
    }
    let (dx0, dy0) = (bx0.floor() as i32 - 1, by0.floor() as i32 - 1);
    let (dx1, dy1) = (bx1.ceil() as i32 + 1, by1.ceil() as i32 + 1);
    let (dw, dh) = ((dx1 - dx0) as usize, (dy1 - dy0) as usize);
    if dw * dh > 1 << 28 {
        return (src.clone(), sel.mask.clone());
    }
    // два слоя обратного отображения: первое попадание и (при складке) следующее
    let mut first: Vec<Option<(f64, f64)>> = vec![None; dw * dh];
    let mut second: Vec<Option<(f64, f64)>> = if g.blend { vec![None; dw * dh] } else { Vec::new() };
    let at = |i: usize, j: usize| fwd[j * (nu + 1) + i];
    for j in 0..nv {
        for i in 0..nu {
            let q = [at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1)];
            for tri in [[q[0], q[1], q[2]], [q[0], q[2], q[3]]] {
                raster(&tri, dx0, dy0, dw, dh, |idx, s| {
                    if g.blend && first[idx].is_some() {
                        second[idx] = Some(s);
                    } else {
                        first[idx] = Some(s);
                    }
                });
            }
        }
    }
    let lookup = |table: &Vec<Option<(f64, f64)>>, x: f64, y: f64| -> Option<(f64, f64)> {
        let (i, j) = ((x.floor() as i32 - dx0) as usize, (y.floor() as i32 - dy0) as usize);
        if i >= dw || j >= dh {
            return None;
        }
        table[j * dw + i]
    };
    let a = |x: f64, y: f64| lookup(&first, x, y);
    let b = |x: f64, y: f64| lookup(&second, x, y);
    let sb = (x0.floor() as i32, y0.floor() as i32, x1.ceil() as i32, y1.ceil() as i32);
    let layers: Vec<&dyn Fn(f64, f64) -> Option<(f64, f64)>> = if g.blend { vec![&a, &b] } else { vec![&a] };
    crate::selection::warp_core(src, sel, sb, (dx0, dy0, dx1, dy1), mode, &layers)
}

/// Треугольник `tri` (точки: источник x, y; результат x, y) — по точкам результата (центры
/// пикселей); для каждой — точка источника (линейно внутри треугольника).
fn raster(tri: &[[f64; 4]; 3], ox: i32, oy: i32, w: usize, h: usize, mut put: impl FnMut(usize, (f64, f64))) {
    let [a, b, c] = *tri;
    let det = (b[2] - a[2]) * (c[3] - a[3]) - (c[2] - a[2]) * (b[3] - a[3]);
    if det.abs() < 1e-12 {
        return;
    }
    let xmin = (a[2].min(b[2]).min(c[2]) - 0.5).ceil() as i32;
    let xmax = (a[2].max(b[2]).max(c[2]) - 0.5).floor() as i32;
    let ymin = (a[3].min(b[3]).min(c[3]) - 0.5).ceil() as i32;
    let ymax = (a[3].max(b[3]).max(c[3]) - 0.5).floor() as i32;
    for y in ymin..=ymax {
        for x in xmin..=xmax {
            let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
            let l1 = ((px - a[2]) * (c[3] - a[3]) - (c[2] - a[2]) * (py - a[3])) / det;
            let l2 = ((b[2] - a[2]) * (py - a[3]) - (px - a[2]) * (b[3] - a[3])) / det;
            let l0 = 1.0 - l1 - l2;
            let eps = -1e-9;
            if l0 < eps || l1 < eps || l2 < eps {
                continue;
            }
            let (i, j) = ((x - ox) as usize, (y - oy) as usize);
            if i >= w || j >= h {
                continue;
            }
            put(j * w + i, (l0 * a[0] + l1 * b[0] + l2 * c[0], l0 * a[1] + l1 * b[1] + l2 * c[1]));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Нетронутая сетка ничего не меняет; «Решётка» 2×2: середина уходит на ¼ сдвига узла (SAI).
    #[test]
    fn lattice_matches_sai() {
        let mut g = GridWarp::new([0.0, 0.0, 400.0, 400.0], 2, 2, GridKind::Lattice);
        assert_eq!(g.apply(123.0, 77.0), (123.0, 77.0));
        g.points[4] = [260.0, 240.0];
        let (x, y) = g.apply(200.0, 200.0);
        assert!((x - 215.0).abs() < 1e-9 && (y - 210.0).abs() < 1e-9, "{x} {y}");
        assert_eq!(g.apply(0.0, 0.0), (0.0, 0.0));
        for n in 1..=5 {
            let g = GridWarp::new([10.0, 20.0, 330.0, 220.0], n, n, GridKind::Lattice);
            let (x, y) = g.apply(101.0, 99.0);
            assert!((x - 101.0).abs() < 1e-9 && (y - 99.0).abs() < 1e-9);
        }
    }

    /// «Сетка» проходит через узлы.
    #[test]
    fn mesh_passes_through_points() {
        let mut g = GridWarp::new([0.0, 0.0, 300.0, 300.0], 3, 3, GridKind::Mesh);
        assert_eq!(g.apply(55.0, 210.0), (55.0, 210.0));
        g.points[5] = [140.0, 130.0];
        let (x, y) = g.apply(100.0, 100.0);
        assert!((x - 140.0).abs() < 1e-9 && (y - 130.0).abs() < 1e-9);
        let (x, _) = g.apply(300.0, 150.0);
        assert!((x - 300.0).abs() < 1e-9);
    }

    /// Деформация двигает краску: точка в середине уходит за узлом.
    #[test]
    fn pixels_follow_the_grid() {
        let mut src = ColorMap::new();
        for y in 95..105 {
            for x in 95..105 {
                src.set(x, y, [0, 0, 0, crate::pixel::ONE as u16]);
            }
        }
        for y in [0, 199] {
            src.set(0, y, [0, 0, 0, crate::pixel::ONE as u16]);
            src.set(199, y, [0, 0, 0, crate::pixel::ONE as u16]);
        }
        let mut g = GridWarp::new([0.0, 0.0, 200.0, 200.0], 2, 2, GridKind::Mesh);
        g.points[4] = [140.0, 100.0];
        let (out, _) = warp_pixels(&src, &Selection::default(), &g, crate::resample::Mode::Bilinear);
        assert!(out.get(140, 100)[3] > 0 && out.get(100, 100)[3] == 0);
    }
}
