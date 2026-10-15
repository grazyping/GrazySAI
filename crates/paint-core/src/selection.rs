//! Выделение: маска 0..=ONE поверх холста. Пустая маска — выделения нет, рисовать можно везде.
//!
//! Как в SAI 2: прямоугольник, лассо, волшебная палочка; новая область заменяет выделение,
//! с Shift добавляется, с Alt вычитается, с Shift+Alt остаётся пересечение.

use crate::pixel::{to_rgba8, Px, ONE};
use crate::tile::{index_of, key_of, ColorMap, Key, MaskMap, Tile, TILE};
use crate::vector::{coverage, Pt};
use std::collections::HashMap;

/// Как новая область соединяется с прежним выделением.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Combine {
    Replace,
    Add,
    Subtract,
    Intersect,
}

impl Combine {
    /// По клавишам-модификаторам, как в SAI.
    pub fn from_modifiers(shift: bool, alt: bool) -> Combine {
        match (shift, alt) {
            (true, true) => Combine::Intersect,
            (true, false) => Combine::Add,
            (false, true) => Combine::Subtract,
            (false, false) => Combine::Replace,
        }
    }
}

#[derive(Clone, Default, Debug, PartialEq)]
pub struct Selection {
    pub mask: MaskMap,
}

impl Selection {
    /// Выделения нет (вся маска пуста).
    pub fn is_empty(&self) -> bool {
        self.mask.tiles.values().all(|t| t.0.iter().all(|&v| v == 0))
    }

    /// Доля выделения в точке 0..=ONE; без выделения — ONE везде.
    pub fn amount(&self, x: i32, y: i32) -> u32 {
        if self.mask.tiles.is_empty() {
            return ONE;
        }
        self.mask.get(x, y) as u32
    }

    /// Плитка выделения для ограничения правки; `None` — ограничения нет (выделения нет).
    /// Пустая плитка при непустом выделении — правка в ней запрещена.
    pub fn tile(&self, key: Key) -> Option<Tile<u16>> {
        if self.mask.tiles.is_empty() {
            return None;
        }
        Some(self.mask.tiles.get(&key).cloned().unwrap_or_else(|| Tile::filled(0)))
    }

    /// Соединяет область `area` с выделением.
    pub fn combine(&mut self, area: &MaskMap, mode: Combine) {
        match mode {
            Combine::Replace => self.mask = area.clone(),
            Combine::Add => {
                for (k, t) in &area.tiles {
                    let dst = self.mask.tile_mut(*k);
                    for (d, &s) in dst.0.iter_mut().zip(t.0.iter()) {
                        *d = (*d).max(s);
                    }
                }
            }
            Combine::Subtract => {
                for (k, t) in &area.tiles {
                    if let Some(dst) = self.mask.tiles.get_mut(k) {
                        for (d, &s) in dst.0.iter_mut().zip(t.0.iter()) {
                            *d = ((*d as u32 * (ONE - s as u32)) / ONE) as u16;
                        }
                    }
                }
            }
            Combine::Intersect => {
                let keys: Vec<Key> = self.mask.tiles.keys().copied().collect();
                for k in keys {
                    match area.tiles.get(&k) {
                        Some(t) => {
                            let dst = self.mask.tile_mut(k);
                            for (d, &s) in dst.0.iter_mut().zip(t.0.iter()) {
                                *d = (*d).min(s);
                            }
                        }
                        None => {
                            self.mask.tiles.remove(&k);
                        }
                    }
                }
            }
        }
        self.prune();
    }

    /// Убирает пустые плитки, чтобы `is_empty` и показ работали быстро.
    fn prune(&mut self) {
        self.mask.tiles.retain(|_, t| t.0.iter().any(|&v| v != 0));
    }

    /// Выделить весь холст.
    pub fn all(width: u32, height: u32) -> MaskMap {
        rect(0.0, 0.0, width as f64, height as f64)
    }

    /// Обратить выделение в пределах холста.
    pub fn invert(&mut self, width: u32, height: u32) {
        let mut out = MaskMap::new();
        for y in 0..height as i32 {
            for x in 0..width as i32 {
                let v = ONE - self.mask.get(x, y) as u32;
                if v > 0 {
                    out.set(x, y, v as u16);
                }
            }
        }
        self.mask = out;
        self.prune();
    }

    /// Расширить (`grow`) или сузить выделение на 1 px: наибольшее или наименьшее значение
    /// среди точки и её восьми соседей.
    pub fn grow(&mut self, grow: bool) {
        self.grow_by(1, grow);
    }

    /// Расширить или сузить на `n` px, как «Dilate/Erode Selection» SAI 2017 (1–25 px): шаги по
    /// очереди квадратом 3×3 и крестом (первый — квадрат), поэтому углы срезаются под 45°
    /// (сверено с живой SAI: прямоугольник +25 px — срез по 12 px).
    pub fn grow_by(&mut self, n: u32, grow: bool) {
        let Some((x0, y0, x1, y1)) = self.bounds() else { return };
        let pad = if grow { n as i32 } else { 0 };
        let (ox, oy) = (x0 - pad, y0 - pad);
        let (bw, bh) = ((x1 - x0 + 2 * pad) as usize, (y1 - y0 + 2 * pad) as usize);
        let mut buf = vec![0u16; bw * bh];
        for (i, v) in buf.iter_mut().enumerate() {
            *v = self.mask.get(ox + (i % bw) as i32, oy + (i / bw) as i32);
        }
        let buf = morph(buf, bw, bh, n, grow, 0);
        let mut out = MaskMap::new();
        for (i, &v) in buf.iter().enumerate() {
            if v > 0 {
                out.set(ox + (i % bw) as i32, oy + (i / bw) as i32, v);
            }
        }
        self.mask = out;
        self.prune();
    }

    /// Границы выделения: (x0, y0, x1, y1), x1 и y1 не включительно.
    pub fn bounds(&self) -> Option<(i32, i32, i32, i32)> {
        let mut b: Option<(i32, i32, i32, i32)> = None;
        for (&(tx, ty), t) in &self.mask.tiles {
            for (i, &v) in t.0.iter().enumerate() {
                if v == 0 {
                    continue;
                }
                let (x, y) = (tx * TILE + i as i32 % TILE, ty * TILE + i as i32 / TILE);
                b = Some(match b {
                    None => (x, y, x + 1, y + 1),
                    Some((a, c, d, e)) => (a.min(x), c.min(y), d.max(x + 1), e.max(y + 1)),
                });
            }
        }
        b
    }

    /// Отрезки границы выделения (по порогу половины) в координатах холста — для показа
    /// «бегущих муравьёв». Соседние отрезки одной линии склеены.
    pub fn outline(&self) -> Vec<[(i32, i32); 2]> {
        let half = (ONE / 2) as u16;
        let inside = |x: i32, y: i32| self.mask.get(x, y) >= half;
        let Some((x0, y0, x1, y1)) = self.bounds() else { return Vec::new() };
        let mut out = Vec::new();
        // горизонтальные границы: между строками y-1 и y
        for y in y0..=y1 {
            let mut run: Option<i32> = None;
            for x in x0..=x1 {
                let edge = x < x1 && inside(x, y - 1) != inside(x, y);
                match (edge, run) {
                    (true, None) => run = Some(x),
                    (false, Some(s)) => {
                        out.push([(s, y), (x, y)]);
                        run = None;
                    }
                    _ => {}
                }
            }
        }
        // вертикальные: между столбцами x-1 и x
        for x in x0..=x1 {
            let mut run: Option<i32> = None;
            for y in y0..=y1 {
                let edge = y < y1 && inside(x - 1, y) != inside(x, y);
                match (edge, run) {
                    (true, None) => run = Some(y),
                    (false, Some(s)) => {
                        out.push([(x, s), (x, y)]);
                        run = None;
                    }
                    _ => {}
                }
            }
        }
        out
    }
}

/// Расширить (`grow`, наибольшее) или сузить (наименьшее) значения плотной карты `bw`×`bh` на
/// `n` px, как SAI 2017 (выделение и «Обводка»): шаги по очереди квадратом 3×3 и крестом, первый —
/// квадрат, поэтому углы срезаются под 45°. За краем карты — `outside`. Строки — в потоках.
pub fn morph(mut buf: Vec<u16>, bw: usize, bh: usize, n: u32, grow: bool, outside: u16) -> Vec<u16> {
    let pick = |a: u16, b: u16| if grow { a.max(b) } else { a.min(b) };
    let at = |buf: &[u16], x: isize, y: isize| -> u16 {
        if x < 0 || y < 0 || x >= bw as isize || y >= bh as isize {
            outside
        } else {
            buf[y as usize * bw + x as usize]
        }
    };
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let rows = bh.div_ceil(threads).max(1);
    // один проход: каждая строка результата по `f(x, y)`
    let pass = |f: &(dyn Fn(isize, isize) -> u16 + Sync)| -> Vec<u16> {
        let mut out = vec![0u16; bw * bh];
        std::thread::scope(|scope| {
            for (k, chunk) in out.chunks_mut(rows * bw).enumerate() {
                scope.spawn(move || {
                    for (i, o) in chunk.iter_mut().enumerate() {
                        let idx = k * rows * bw + i;
                        *o = f((idx % bw) as isize, (idx / bw) as isize);
                    }
                });
            }
        });
        out
    };
    for step in 0..n {
        buf = if step % 2 == 0 {
            // квадрат: по строке, затем по столбцу
            let row = pass(&|x, y| pick(pick(at(&buf, x - 1, y), at(&buf, x, y)), at(&buf, x + 1, y)));
            pass(&|x, y| pick(pick(at(&row, x, y - 1), at(&row, x, y)), at(&row, x, y + 1)))
        } else {
            pass(&|x, y| [(-1, 0), (1, 0), (0, -1), (0, 1)].iter().fold(at(&buf, x, y), |v, &(dx, dy)| pick(v, at(&buf, x + dx, y + dy))))
        };
    }
    buf
}

/// Прямоугольник (координаты холста, края могут быть дробными — частичное покрытие).
pub fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> MaskMap {
    polygon(&[[x0, y0], [x1, y0], [x1, y1], [x0, y1]])
}

/// Многоугольник (лассо) со сглаженным краем.
pub fn polygon(points: &[Pt]) -> MaskMap {
    let mut out = MaskMap::new();
    if points.len() < 3 {
        return out;
    }
    let (left, top, w, _h, cov) = coverage(&[points.to_vec()]);
    for (i, &c) in cov.iter().enumerate() {
        let v = (c.clamp(0.0, 1.0) * ONE as f32 + 0.5) as u32;
        if v > 0 {
            out.set(left + (i % w) as i32, top + (i / w) as i32, v.min(ONE) as u16);
        }
    }
    out
}

/// Многоугольник лассо: со сглаженным краем или без (тогда точка выделена, если покрыта больше
/// чем наполовину) — галочка «Сглаживание» инструмента, как в SAI.
pub fn polygon_aa(points: &[Pt], aa: bool) -> MaskMap {
    let mut m = polygon(points);
    if !aa {
        for t in m.tiles.values_mut() {
            for v in t.0.iter_mut() {
                *v = if *v as u32 * 2 > ONE { ONE as u16 } else { 0 };
            }
        }
    }
    m
}

/// Волшебная палочка: связная (по 4 соседям) область, где цвет отличается от цвета в точке
/// щелчка не больше чем на `tolerance` (0..=255, по наибольшей разнице каналов RGBA).
pub fn magic_wand(pixels: &ColorMap, width: u32, height: u32, x: i32, y: i32, tolerance: u8) -> MaskMap {
    let mut out = MaskMap::new();
    let (w, h) = (width as i32, height as i32);
    if x < 0 || y < 0 || x >= w || y >= h {
        return out;
    }
    let rgba = |p: Px| to_rgba8(p);
    let seed = rgba(pixels.get(x, y));
    let near = |c: [u8; 4]| (0..4).all(|i| (c[i] as i32 - seed[i] as i32).abs() <= tolerance as i32);
    let mut seen: HashMap<Key, Vec<bool>> = HashMap::new();
    let mut stack = vec![(x, y)];
    while let Some((px, py)) = stack.pop() {
        let key = key_of(px, py);
        let flags = seen.entry(key).or_insert_with(|| vec![false; (TILE * TILE) as usize]);
        let i = index_of(px, py);
        if flags[i] {
            continue;
        }
        flags[i] = true;
        if !near(rgba(pixels.get(px, py))) {
            continue;
        }
        out.set(px, py, ONE as u16);
        for (nx, ny) in [(px + 1, py), (px - 1, py), (px, py + 1), (px, py - 1)] {
            if nx >= 0 && ny >= 0 && nx < w && ny < h {
                stack.push((nx, ny));
            }
        }
    }
    out
}

/// Маска, сдвинутая на целые (`dx`, `dy`).
pub fn shift_mask(src: &MaskMap, dx: i32, dy: i32) -> MaskMap {
    let mut out = MaskMap::new();
    for (&(tx, ty), t) in &src.tiles {
        for (i, &v) in t.0.iter().enumerate() {
            if v != 0 {
                out.set(tx * TILE + i as i32 % TILE + dx, ty * TILE + i as i32 / TILE + dy, v);
            }
        }
    }
    out
}

/// Пиксели слоя после сдвига на (`dx`, `dy`): без выделения сдвигается всё; с выделением
/// выделенная часть вырезается и кладётся поверх остального на новом месте.
pub fn shift_pixels(src: &ColorMap, sel: &Selection, dx: i32, dy: i32) -> ColorMap {
    let mut out = ColorMap::new();
    let whole = sel.mask.tiles.is_empty();
    if !whole {
        // то, что остаётся на месте
        for (&(tx, ty), t) in &src.tiles {
            for (i, &p) in t.0.iter().enumerate() {
                let (x, y) = (tx * TILE + i as i32 % TILE, ty * TILE + i as i32 / TILE);
                let keep = ONE - sel.amount(x, y);
                if keep > 0 && p[3] > 0 {
                    out.set(x, y, p.map(|c| ((c as u32 * keep + ONE / 2) / ONE) as u16));
                }
            }
        }
    }
    for (&(tx, ty), t) in &src.tiles {
        for (i, &p) in t.0.iter().enumerate() {
            if p[3] == 0 {
                continue;
            }
            let (x, y) = (tx * TILE + i as i32 % TILE, ty * TILE + i as i32 / TILE);
            let k = if whole { ONE } else { sel.amount(x, y) };
            if k == 0 {
                continue;
            }
            let m = p.map(|c| ((c as u32 * k + ONE / 2) / ONE) as u16);
            let (nx, ny) = (x + dx, y + dy);
            let under = out.get(nx, ny);
            // «поверх» в предумноженных значениях
            let inv = ONE - m[3] as u32;
            let v = [0, 1, 2, 3].map(|c| (m[c] as u32 + (under[c] as u32 * inv + ONE / 2) / ONE).min(ONE) as u16);
            out.set(nx, ny, v);
        }
    }
    out.tiles.retain(|_, t| t.0.iter().any(|p| p[3] != 0));
    out
}

/// Пиксели слоя после трансформации матрицей `m` (точка источника → точка результата):
/// без выделения — весь слой, с выделением — выделенная часть поверх остального. Выделение
/// трансформируется так же. Возвращает (пиксели, маска выделения).
pub fn transform_pixels(src: &ColorMap, sel: &Selection, m: [f64; 6], mode: crate::resample::Mode) -> (ColorMap, MaskMap) {
    warp_pixels(src, sel, Warp::Affine(m), mode)
}

/// Отображение точки источника в точку результата.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Warp {
    /// `x' = m0·x + m1·y + m2`, `y' = m3·x + m4·y + m5`.
    Affine([f64; 6]),
    /// Матрица 3×3 по строкам (перспектива).
    Projective([f64; 9]),
    /// Свободная деформация, как в SAI 2017: прямоугольник `src` (x0, y0, x1, y1) в
    /// четырёхугольник `quad` (по часовой от левого верхнего). Точка — смесь билинейного и
    /// перспективного отображения в доле `perspective` (0..=1, ползунок «Perspective» 0–100;
    /// сверено с живой SAI, docs/17-selection-move.md).
    Quad { src: [f64; 4], quad: [[f64; 2]; 4], perspective: f64 },
}

impl Warp {
    fn full(self) -> [f64; 9] {
        match self {
            Warp::Affine(m) => [m[0], m[1], m[2], m[3], m[4], m[5], 0.0, 0.0, 1.0],
            Warp::Projective(m) => m,
            Warp::Quad { src, quad, .. } => {
                let [x0, y0, x1, y1] = src;
                let to_unit = [1.0 / (x1 - x0).max(1e-9), 0.0, -x0 / (x1 - x0).max(1e-9), 0.0, 1.0 / (y1 - y0).max(1e-9), -y0 / (y1 - y0).max(1e-9), 0.0, 0.0, 1.0];
                crate::resample::mul3(crate::resample::square_to_quad(quad), to_unit)
            }
        }
    }

    pub fn apply(self, x: f64, y: f64) -> (f64, f64) {
        let m = self.full();
        let w = m[6] * x + m[7] * y + m[8];
        let proj = ((m[0] * x + m[1] * y + m[2]) / w, (m[3] * x + m[4] * y + m[5]) / w);
        match self {
            Warp::Quad { src, quad, perspective } if perspective < 1.0 => {
                let u = (x - src[0]) / (src[2] - src[0]).max(1e-9);
                let v = (y - src[1]) / (src[3] - src[1]).max(1e-9);
                let [a, b, c, d] = quad;
                let bil = |i: usize| (1.0 - u) * (1.0 - v) * a[i] + u * (1.0 - v) * b[i] + u * v * c[i] + (1.0 - u) * v * d[i];
                let t = perspective;
                ((1.0 - t) * bil(0) + t * proj.0, (1.0 - t) * bil(1) + t * proj.1)
            }
            _ => proj,
        }
    }

    /// Обратное отображение: точка результата → точка источника.
    fn inverse(self) -> Option<impl Fn(f64, f64) -> Option<(f64, f64)>> {
        let inv = crate::resample::invert3(self.full())?;
        let blend = matches!(self, Warp::Quad { perspective, .. } if perspective < 1.0);
        Some(move |x: f64, y: f64| {
            let w = inv[6] * x + inv[7] * y + inv[8];
            if w.abs() < 1e-12 {
                return None;
            }
            let mut p = ((inv[0] * x + inv[1] * y + inv[2]) / w, (inv[3] * x + inv[4] * y + inv[5]) / w);
            if !blend {
                return Some(p);
            }
            // смесь с билинейным: метод Ньютона от перспективного ответа
            for _ in 0..12 {
                let f = self.apply(p.0, p.1);
                let (ex, ey) = (f.0 - x, f.1 - y);
                if ex * ex + ey * ey < 1e-8 {
                    return Some(p);
                }
                let h = 1e-3;
                let fx = self.apply(p.0 + h, p.1);
                let fy = self.apply(p.0, p.1 + h);
                let (a, b, c, d) = ((fx.0 - f.0) / h, (fy.0 - f.0) / h, (fx.1 - f.1) / h, (fy.1 - f.1) / h);
                let det = a * d - b * c;
                if det.abs() < 1e-12 {
                    return None;
                }
                p = (p.0 - (d * ex - b * ey) / det, p.1 - (a * ey - c * ex) / det);
            }
            let f = self.apply(p.0, p.1);
            ((f.0 - x).hypot(f.1 - y) < 0.01).then_some(p)
        })
    }
}

/// Как `transform_pixels`, но с любым отображением `warp`.
pub fn warp_pixels(src: &ColorMap, sel: &Selection, warp: Warp, mode: crate::resample::Mode) -> (ColorMap, MaskMap) {
    let whole = sel.mask.tiles.is_empty();
    // область источника
    let bounds = if whole { content_bounds(src) } else { sel.bounds() };
    let Some((x0, y0, x1, y1)) = bounds else { return (src.clone(), sel.mask.clone()) };
    let Some(inverse) = warp.inverse() else { return (src.clone(), sel.mask.clone()) };
    let (sw, sh) = ((x1 - x0) as usize, (y1 - y0) as usize);
    let k = 1.0 / ONE as f32;
    let mut part = vec![[0f32; 4]; sw * sh];
    let mut mpart = vec![[0f32; 4]; sw * sh];
    let mut out = ColorMap::new();
    for y in y0..y1 {
        for x in x0..x1 {
            let p = src.get(x, y);
            let s = if whole { ONE } else { sel.amount(x, y) };
            let i = (y - y0) as usize * sw + (x - x0) as usize;
            let f = s as f32 / ONE as f32;
            part[i] = p.map(|c| c as f32 * k * f);
            mpart[i] = [f, f, f, f];
        }
    }
    if !whole {
        for (&(tx, ty), t) in &src.tiles {
            for (i, &p) in t.0.iter().enumerate() {
                let (x, y) = (tx * TILE + i as i32 % TILE, ty * TILE + i as i32 / TILE);
                let keep = ONE - sel.amount(x, y);
                if keep > 0 && p[3] > 0 {
                    out.set(x, y, p.map(|c| ((c as u32 * keep + ONE / 2) / ONE) as u16));
                }
            }
        }
    }
    // область результата — образ углов
    let corners = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)].map(|(x, y)| warp.apply(x as f64, y as f64));
    let dx0 = corners.iter().map(|c| c.0).fold(f64::MAX, f64::min).floor() as i32 - 1;
    let dy0 = corners.iter().map(|c| c.1).fold(f64::MAX, f64::min).floor() as i32 - 1;
    let dx1 = corners.iter().map(|c| c.0).fold(f64::MIN, f64::max).ceil() as i32 + 1;
    let dy1 = corners.iter().map(|c| c.1).fold(f64::MIN, f64::max).ceil() as i32 + 1;
    let (dw, dh) = ((dx1 - dx0) as usize, (dy1 - dy0) as usize);
    // обратное отображение в координаты буфера источника
    let back = |x: f64, y: f64| inverse(x, y).map(|(sx, sy)| (sx - x0 as f64, sy - y0 as f64));
    let moved = crate::resample::sample_map(&part, sw, sh, dx0, dy0, dw, dh, mode, back);
    let mmask = crate::resample::sample_map(&mpart, sw, sh, dx0, dy0, dw, dh, mode, back);
    let mut mask = MaskMap::new();
    for j in 0..dh {
        for i in 0..dw {
            let (x, y) = (dx0 + i as i32, dy0 + j as i32);
            let p = moved[j * dw + i];
            if p[3] > 0.0 {
                let mv = p.map(|c| (c.clamp(0.0, 1.0) * ONE as f32 + 0.5) as u32);
                let under = out.get(x, y);
                let inv_a = ONE - mv[3].min(ONE);
                let v = [0, 1, 2, 3].map(|c| (mv[c] + (under[c] as u32 * inv_a + ONE / 2) / ONE).min(ONE) as u16);
                out.set(x, y, v);
            }
            if !whole {
                let a = (mmask[j * dw + i][3].clamp(0.0, 1.0) * ONE as f32 + 0.5) as u16;
                if a > 0 {
                    mask.set(x, y, a);
                }
            }
        }
    }
    out.tiles.retain(|_, t| t.0.iter().any(|p| p[3] != 0));
    mask.tiles.retain(|_, t| t.0.iter().any(|&v| v != 0));
    (out, mask)
}

/// Выделение по непрозрачности пикселей: доля выделения = альфа.
pub fn from_opacity(src: &ColorMap) -> MaskMap {
    let mut out = MaskMap::new();
    for (k, t) in &src.tiles {
        if t.0.iter().all(|p| p[3] == 0) {
            continue;
        }
        let dst = out.tile_mut(*k);
        for (d, p) in dst.0.iter_mut().zip(t.0.iter()) {
            *d = p[3];
        }
    }
    out
}

/// Границы непустых пикселей слоя.
pub fn content_bounds(src: &ColorMap) -> Option<(i32, i32, i32, i32)> {
    let mut b: Option<(i32, i32, i32, i32)> = None;
    for (&(tx, ty), t) in &src.tiles {
        for (i, p) in t.0.iter().enumerate() {
            if p[3] == 0 {
                continue;
            }
            let (x, y) = (tx * TILE + i as i32 % TILE, ty * TILE + i as i32 / TILE);
            b = Some(match b {
                None => (x, y, x + 1, y + 1),
                Some((a, c, d, e)) => (a.min(x), c.min(y), d.max(x + 1), e.max(y + 1)),
            });
        }
    }
    b
}

/// Ограничивает покрытие `m` (0..=ONE) выделением: доля `sel` (0..=ONE).
#[inline]
pub fn limit(m: u32, sel: u16) -> u32 {
    m * sel as u32 / ONE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangle_and_modes() {
        let mut s = Selection::default();
        assert!(s.is_empty());
        assert_eq!(s.amount(5, 5), ONE);
        s.combine(&rect(0.0, 0.0, 10.0, 10.0), Combine::Replace);
        assert_eq!(s.amount(5, 5), ONE);
        assert_eq!(s.amount(15, 5), 0);
        s.combine(&rect(5.0, 0.0, 20.0, 10.0), Combine::Add);
        assert_eq!(s.amount(15, 5), ONE);
        s.combine(&rect(0.0, 0.0, 3.0, 10.0), Combine::Subtract);
        assert_eq!(s.amount(1, 5), 0);
        s.combine(&rect(10.0, 0.0, 30.0, 10.0), Combine::Intersect);
        assert_eq!((s.amount(8, 5), s.amount(12, 5)), (0, ONE));
        assert_eq!(s.bounds(), Some((10, 0, 20, 10)));
    }

    #[test]
    fn selection_from_layer_opacity() {
        let mut px = ColorMap::new();
        px.set(3, 4, crate::pixel::from_rgba8([10, 20, 30, 128]));
        let m = from_opacity(&px);
        assert_eq!(m.get(3, 4), px.get(3, 4)[3]);
        assert_eq!(m.get(4, 4), 0);
    }

    #[test]
    fn grow_and_shrink_by_one_pixel() {
        let mut s = Selection::default();
        s.combine(&rect(5.0, 5.0, 10.0, 10.0), Combine::Replace);
        s.grow(true);
        assert_eq!(s.bounds(), Some((4, 4, 11, 11)));
        s.grow(false);
        s.grow(false);
        assert_eq!(s.bounds(), Some((6, 6, 9, 9)));
    }

    #[test]
    fn grow_by_cuts_corners_like_sai() {
        // SAI 2017: прямоугольник +25 px — стороны сдвигаются на 25, углы срезаны по 12 px
        let mut s = Selection::default();
        s.combine(&rect(100.0, 100.0, 200.0, 180.0), Combine::Replace);
        s.grow_by(25, true);
        assert_eq!(s.bounds(), Some((75, 75, 225, 205)));
        let top: Vec<i32> = (0..300).filter(|&x| s.amount(x, 75) > 0).collect();
        assert_eq!((top[0], *top.last().unwrap()), (75 + 12, 224 - 12));
        s.grow_by(25, false);
        assert_eq!(s.bounds(), Some((100, 100, 200, 180)));
    }

    #[test]
    fn half_pixel_edge_is_half_selected() {
        let m = rect(0.0, 0.0, 4.5, 4.0);
        assert_eq!(m.get(4, 1) as u32, ONE / 2);
    }

    #[test]
    fn invert_and_outline() {
        let mut s = Selection::default();
        s.combine(&rect(2.0, 2.0, 5.0, 4.0), Combine::Replace);
        assert_eq!(s.outline().len(), 4);
        s.invert(8, 8);
        assert_eq!((s.amount(3, 3), s.amount(0, 0)), (0, ONE));
    }

    #[test]
    fn wand_takes_connected_similar_colour() {
        let mut px = ColorMap::new();
        let red = crate::pixel::from_rgba8([255, 0, 0, 255]);
        for x in 0..10 {
            px.set(x, 0, red);
            px.set(x, 2, red);
        }
        let m = magic_wand(&px, 10, 3, 3, 0, 0);
        assert_eq!(m.get(9, 0) as u32, ONE);
        assert_eq!(m.get(3, 2), 0, "несвязная полоса не выделена");
        assert_eq!(m.get(3, 1), 0);
    }
}
