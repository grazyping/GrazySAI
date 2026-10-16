//! Фильтры, как в SAI 2 (`docs/20-filters.md`): тон и насыщенность, яркость и контраст,
//! размытие по Гауссу. Цветовые фильтры работают с прямым (не предумноженным) цветом каждой
//! точки, альфа не меняется. Правила прочитаны в коде образца и сверены с живой SAI 2017.

use crate::pixel::{Px, ONE};
use crate::tile::{ColorMap, TILE};

/// Яркость, контраст и глубина цвета, каждый −100…100.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BrightnessContrast {
    pub brightness: i32,
    pub contrast: i32,
    pub depth: i32,
}

/// Тон −180…180°, насыщенность и яркость −100…100, «Colorize» — окрасить в один тон.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HueSaturation {
    pub hue: i32,
    pub saturation: i32,
    pub luminance: i32,
    pub colorize: bool,
}

/// Прямой цвет 0..1 точки или `None`, если точка прозрачна.
fn straight(p: Px) -> Option<([f64; 3], f64)> {
    if p[3] == 0 {
        return None;
    }
    let a = p[3] as f64;
    Some(([p[0] as f64 / a, p[1] as f64 / a, p[2] as f64 / a], a))
}

fn pack(c: [f64; 3], a: f64) -> Px {
    let q = |v: f64| (v.clamp(0.0, 1.0) * a + 0.5) as u16;
    [q(c[0]), q(c[1]), q(c[2]), a as u16]
}

impl BrightnessContrast {
    pub fn apply(&self, p: Px) -> Px {
        let Some((mut c, a)) = straight(p) else { return p };
        let b = self.brightness as f64 * 0.01;
        let k = self.contrast as f64 * 0.01;
        let d = self.depth as f64 * 0.01;
        let bb = b * b;
        // яркость: к белому или к чёрному на долю b²
        for v in c.iter_mut() {
            *v = if b >= 0.0 { (*v + *v * bb + bb).min(1.0) } else { (*v - (1.0 - *v) * bb - bb).max(0.0) };
        }
        // глубина: вверх — к квадрату цвета относительно максимума (ярче краски), вниз — к серому
        if d >= 0.0 {
            let m = c[0].max(c[1]).max(c[2]);
            if m > 0.0 {
                let w = d * d * 4.0;
                for v in c.iter_mut() {
                    *v += (*v * *v / m - *v) * w;
                }
            }
        } else {
            let luma = c[0] * 0.299 + c[1] * 0.587 + c[2] * 0.114;
            for v in c.iter_mut() {
                *v += (luma - *v) * -d;
            }
        }
        // контраст вокруг середины: вверх — в 1 + 4k² раз, вниз — в 1 + k
        let kk = if k >= 0.0 { k * k * 4.0 + 1.0 } else { k + 1.0 };
        for v in c.iter_mut() {
            *v = (*v - 0.5) * kk + 0.5;
        }
        pack(c, a)
    }
}

/// Углы сектора тона: (множитель размаха, множитель доли) для R, G, B в секторах 0…5.
const SECTORS: [[(f64, f64); 3]; 6] = [
    [(0.0, 0.0), (-1.0, 1.0), (-1.0, 0.0)],
    [(0.0, -1.0), (0.0, 0.0), (-1.0, 0.0)],
    [(-1.0, 0.0), (0.0, 0.0), (-1.0, 1.0)],
    [(-1.0, 0.0), (0.0, -1.0), (0.0, 0.0)],
    [(-1.0, 1.0), (-1.0, 0.0), (0.0, 0.0)],
    [(0.0, 0.0), (-1.0, 0.0), (0.0, -1.0)],
];

/// Цвет по тону `h` (0..6, секторы по 60°), размаху `chroma` и наибольшему каналу `max`.
fn from_hue(h: f64, chroma: f64, max: f64) -> [f64; 3] {
    let s = (h as usize).min(5);
    let f = (h - s as f64) * chroma;
    SECTORS[s].map(|(k, j)| (k * chroma + max + j * f).clamp(0.0, 1.0))
}

impl HueSaturation {
    pub fn apply(&self, p: Px) -> Px {
        let Some((c, a)) = straight(p) else { return p };
        let h_shift = self.hue as f64 / 60.0;
        let s = self.saturation as f64 * 0.01;
        let l = self.luminance as f64 * 0.01;
        let (r, g, b) = (c[0], c[1], c[2]);
        // наибольший канал; при равенстве — R, затем G, затем B
        let imax = if r >= g && r >= b { 0 } else if g >= b { 1 } else { 2 };
        let max = c[imax];
        let min = r.min(g).min(b);
        let mut chroma = max - min;
        let mut sum = max + min;
        let mut h = if chroma > 0.0 {
            let n = [r, g, b, r, g];
            (n[imax + 1] - n[imax + 2]) / chroma + imax as f64 * 2.0
        } else {
            0.0
        };
        h += h_shift;
        if h < 0.0 {
            h += 6.0;
        }
        if h >= 6.0 {
            h -= 6.0;
        }
        if self.colorize {
            // окрасить: тон задан, насыщенность (s+1)/2, яркость двигает середину
            let mut sat = (s + 1.0) * 0.5;
            let mut mid = sum * 0.5;
            mid += if l >= 0.0 { (1.0 - mid) * l } else { mid * l };
            let (hi, lo);
            if mid >= 0.5 {
                hi = (1.0 - sat) * mid + sat;
                lo = mid * 2.0 - hi;
            } else {
                lo = (1.0 - sat) * mid;
                hi = mid * 2.0 - lo;
            }
            sat = hi - lo;
            let h = (self.hue as f64 / 60.0).rem_euclid(6.0);
            return pack(from_hue(h, sat, hi), a);
        }
        // яркость: к белому (сумма → 2, размах → 0) или к чёрному
        if l >= 0.0 {
            sum += (2.0 - sum) * l;
            chroma -= chroma * l;
        } else {
            sum += sum * l;
            chroma += chroma * l;
        }
        // насыщенность: вверх — к наибольшему размаху при этой яркости, бережнее к уже ярким
        if s >= 0.0 {
            let lim = if sum >= 1.0 { 2.0 - sum } else { sum };
            let w = ((1.0 - chroma) * (1.0 - chroma)).powi(2);
            let t = ((w * 1.2 + 1.0) * s * (1.0 - w)).min(1.0);
            let g = ((s - 0.8).max(0.0) * 5.0).powi(3);
            let k = (chroma * 128.0).min(1.0);
            chroma += ((t - s) * (1.0 - g) + s) * k * (lim - chroma);
        } else {
            chroma += chroma * s;
        }
        let max = (chroma + sum) * 0.5;
        pack(from_hue(h, chroma, max), a)
    }
}

/// Применить к каждой точке слоя (в пределах выделения: доля `sel(x, y)` 0..=ONE).
pub fn map_pixels(src: &ColorMap, sel: &dyn Fn(i32, i32) -> u32, f: &dyn Fn(Px) -> Px) -> ColorMap {
    let mut out = src.clone();
    for (&(tx, ty), t) in out.tiles.iter_mut() {
        for (i, p) in t.0.iter_mut().enumerate() {
            let s = sel(tx * TILE + i as i32 % TILE, ty * TILE + i as i32 / TILE);
            if s == 0 || p[3] == 0 {
                continue;
            }
            let n = f(*p);
            *p = if s >= ONE { n } else { mix(*p, n, s) };
        }
    }
    out
}

fn mix(a: Px, b: Px, s: u32) -> Px {
    let inv = ONE - s;
    [0, 1, 2, 3].map(|c| ((a[c] as u32 * inv + b[c] as u32 * s + ONE / 2) / ONE) as u16)
}

/// Размытие по Гауссу с σ = `radius` (как в SAI; сверено по сечению линии при радиусах 1, 3 и
/// 9,9). Считается по предумноженным значениям, за краем — прозрачность; результат смешивается
/// с исходным по доле выделения. `w`, `h` — размер холста.
pub fn gaussian_blur(src: &ColorMap, w: u32, h: u32, radius: f64, sel: &(dyn Fn(i32, i32) -> u32 + Sync)) -> ColorMap {
    let sigma = radius.max(0.1);
    let r = (sigma * 3.0).ceil() as i32;
    let kernel: Vec<f64> = {
        let k: Vec<f64> = (-r..=r).map(|x| (-(x * x) as f64 / (2.0 * sigma * sigma)).exp()).collect();
        let s: f64 = k.iter().sum();
        k.into_iter().map(|v| v / s).collect()
    };
    // считаем только рамку содержимого с запасом r (за ней результат пуст)
    let Some((cx0, cy0, cx1, cy1)) = crate::selection::content_bounds(src) else { return src.clone() };
    let (x0, y0) = ((cx0 - r).max(0), (cy0 - r).max(0));
    let (x1, y1) = ((cx1 + r).min(w as i32), (cy1 + r).min(h as i32));
    if x1 <= x0 || y1 <= y0 {
        return src.clone();
    }
    let (bw, bh) = ((x1 - x0) as usize, (y1 - y0) as usize);
    let kf = 1.0 / ONE as f64;
    let mut buf = vec![[0f64; 4]; bw * bh];
    for (i, v) in buf.iter_mut().enumerate() {
        let p = src.get(x0 + (i % bw) as i32, y0 + (i / bw) as i32);
        *v = p.map(|c| c as f64 * kf);
    }
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let pass = |buf: &[[f64; 4]], horizontal: bool| -> Vec<[f64; 4]> {
        let mut out = vec![[0f64; 4]; buf.len()];
        let rows = bh.div_ceil(threads).max(1);
        std::thread::scope(|scope| {
            for (n, chunk) in out.chunks_mut(rows * bw).enumerate() {
                let kernel = &kernel;
                scope.spawn(move || {
                    for (k, o) in chunk.iter_mut().enumerate() {
                        let idx = n * rows * bw + k;
                        let (x, y) = ((idx % bw) as i32, (idx / bw) as i32);
                        let mut acc = [0f64; 4];
                        for (j, &kv) in kernel.iter().enumerate() {
                            let d = j as i32 - r;
                            let (sx, sy) = if horizontal { (x + d, y) } else { (x, y + d) };
                            if sx < 0 || sy < 0 || sx >= bw as i32 || sy >= bh as i32 {
                                continue;
                            }
                            let p = buf[sy as usize * bw + sx as usize];
                            for c in 0..4 {
                                acc[c] += p[c] * kv;
                            }
                        }
                        *o = acc;
                    }
                });
            }
        });
        out
    };
    let blurred = pass(&pass(&buf, true), false);
    let mut out = src.clone();
    for y in 0..bh {
        for x in 0..bw {
            let (gx, gy) = (x0 + x as i32, y0 + y as i32);
            let s = sel(gx, gy);
            if s == 0 {
                continue;
            }
            let old = src.get(gx, gy);
            let v = blurred[y * bw + x];
            let a = (v[3].clamp(0.0, 1.0) * ONE as f64 + 0.5) as u16;
            let q = |c: f64| ((c.clamp(0.0, 1.0) * ONE as f64 + 0.5) as u16).min(a);
            let n = [q(v[0]), q(v[1]), q(v[2]), a];
            let p = if s >= ONE { n } else { mix(old, n, s) };
            if p != old {
                out.set(gx, gy, p);
            }
        }
    }
    out.tiles.retain(|_, t| t.0.iter().any(|p| p[3] != 0));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pixel::{from_rgba8, to_rgba8};

    #[test]
    fn hue_shift_turns_red_into_green_and_blue() {
        let red = from_rgba8([255, 0, 0, 255]);
        let f = |h| HueSaturation { hue: h, ..Default::default() }.apply(red);
        assert_eq!(to_rgba8(f(120)), [0, 255, 0, 255]);
        assert_eq!(to_rgba8(f(-120)), [0, 0, 255, 255]);
        assert_eq!(to_rgba8(f(0)), [255, 0, 0, 255]);
    }

    #[test]
    fn saturation_down_makes_grey_and_luminance_up_makes_white() {
        let c = from_rgba8([200, 100, 50, 255]);
        let g = to_rgba8(HueSaturation { saturation: -100, ..Default::default() }.apply(c));
        assert!(g[0] == g[1] && g[1] == g[2], "{g:?}");
        let w = to_rgba8(HueSaturation { luminance: 100, ..Default::default() }.apply(c));
        assert_eq!(w, [255, 255, 255, 255]);
    }

    #[test]
    fn brightness_51_matches_the_reference_on_black() {
        // в SAI: чёрная линия с альфой 0,565 поверх белого 111 → 149 при яркости +51
        let p = from_rgba8([0, 0, 0, 144]);
        let q = to_rgba8(BrightnessContrast { brightness: 51, ..Default::default() }.apply(p));
        let shown = 255.0 * (1.0 - q[3] as f64 / 255.0 * (1.0 - q[0] as f64 / 255.0));
        assert!((shown - 149.0).abs() < 1.0, "{shown}");
    }

    #[test]
    fn contrast_and_depth_keep_alpha() {
        let p = from_rgba8([120, 80, 40, 200]);
        let q = BrightnessContrast { contrast: 60, depth: -40, ..Default::default() }.apply(p);
        assert_eq!(q[3], p[3]);
    }
}

#[cfg(test)]
mod hue_tests {
    use super::*;
    use crate::pixel::{from_rgba8, to_rgba8};

    #[test]
    fn hue_shift_is_exact_for_every_primary_and_secondary() {
        let cases = [([255, 0, 0], [255, 255, 0]), ([255, 255, 0], [0, 255, 0]), ([0, 255, 0], [0, 255, 255]), ([0, 255, 255], [0, 0, 255]), ([0, 0, 255], [255, 0, 255]), ([255, 0, 255], [255, 0, 0])];
        for (from, to) in cases {
            let p = from_rgba8([from[0], from[1], from[2], 255]);
            let q = to_rgba8(HueSaturation { hue: 60, ..Default::default() }.apply(p));
            assert_eq!(&q[..3], &to, "{from:?}");
        }
    }
}

/// Градиент, как инструмент «Градиент» SAI 2017 (сверено с живой SAI, docs/31-gradient-selection.md):
/// доля `t` растёт линейно от начальной точки к конечной (по проекции на отрезок или, для круга,
/// по расстоянию от начальной), за концами — 0 и 1; «S-кривая» — `3t² − 2t³`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gradient {
    pub from: [f64; 2],
    pub to: [f64; 2],
    pub circle: bool,
    pub fg: [u8; 3],
    /// Конечный цвет; `None` — к прозрачности.
    pub bg: Option<[u8; 3]>,
    pub reverse: bool,
    pub s_curve: bool,
}

impl Gradient {
    /// Доля конечного цвета в центре пикселя (x, y).
    pub fn t(&self, x: i32, y: i32) -> f64 {
        let (px, py) = (x as f64 + 0.5 - self.from[0], y as f64 + 0.5 - self.from[1]);
        let (dx, dy) = (self.to[0] - self.from[0], self.to[1] - self.from[1]);
        let len2 = dx * dx + dy * dy;
        let mut t = if len2 < 1e-9 {
            1.0
        } else if self.circle {
            ((px * px + py * py) / len2).sqrt()
        } else {
            (px * dx + py * dy) / len2
        }
        .clamp(0.0, 1.0);
        if self.reverse {
            t = 1.0 - t;
        }
        if self.s_curve {
            t = t * t * (3.0 - 2.0 * t);
        }
        t
    }

    /// Цвет и непрозрачность (0..=ONE) в точке.
    fn paint(&self, x: i32, y: i32) -> ([u8; 3], u32) {
        let t = self.t(x, y);
        match self.bg {
            Some(bg) => {
                let c = [0, 1, 2].map(|i| (self.fg[i] as f64 + (bg[i] as f64 - self.fg[i] as f64) * t).round() as u8);
                (c, ONE)
            }
            None => (self.fg, ((1.0 - t) * ONE as f64).round() as u32),
        }
    }
}

/// Слой `src` с градиентом поверх (по всему холсту `w`×`h`, с долей выделения `sel`);
/// `preserve` — сохранять непрозрачность слоя.
pub fn gradient(src: &ColorMap, w: u32, h: u32, g: &Gradient, sel: &(dyn Fn(i32, i32) -> u32 + Sync), preserve: bool) -> ColorMap {
    use crate::tile::Tile;
    let keys: Vec<(i32, i32)> = (0..h.div_ceil(TILE as u32) as i32).flat_map(|ty| (0..w.div_ceil(TILE as u32) as i32).map(move |tx| (tx, ty))).collect();
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let per = keys.len().div_ceil(threads).max(1);
    let tiles: Vec<((i32, i32), Tile<Px>)> = std::thread::scope(|scope| {
        let jobs: Vec<_> = keys
            .chunks(per)
            .map(|chunk| {
                scope.spawn(move || {
                    let mut out = Vec::new();
                    for &(tx, ty) in chunk {
                        let mut t = src.tiles.get(&(tx, ty)).cloned().unwrap_or_else(|| Tile::filled([0; 4]));
                        let mut any = false;
                        for (i, px) in t.0.iter_mut().enumerate() {
                            let (x, y) = (tx * TILE + i as i32 % TILE, ty * TILE + i as i32 / TILE);
                            if x >= w as i32 || y >= h as i32 {
                                continue;
                            }
                            let s = sel(x, y);
                            if s == 0 {
                                continue;
                            }
                            let (c, a) = g.paint(x, y);
                            let m = (a * s.min(ONE) + ONE / 2) / ONE;
                            if m > 0 {
                                *px = crate::document::blend_paint(*px, c, m, preserve);
                                any = true;
                            }
                        }
                        if any {
                            out.push(((tx, ty), t));
                        }
                    }
                    out
                })
            })
            .collect();
        jobs.into_iter().flat_map(|j| j.join().unwrap()).collect()
    });
    let mut out = src.clone();
    out.tiles.extend(tiles);
    out
}

#[cfg(test)]
mod gradient_tests {
    use super::*;

    fn g() -> Gradient {
        Gradient { from: [0.0, 0.0], to: [800.0, 0.0], circle: false, fg: [180, 25, 177], bg: Some([255, 255, 255]), reverse: false, s_curve: false }
    }

    #[test]
    fn linear_and_s_curve_match_sai() {
        // живая SAI 2017, протяжка 800 px, основной 25 → фоновый 255 по зелёному каналу
        let green = |g: &Gradient, x: i32| g.paint(x, 0).0[1];
        assert_eq!(green(&g(), 400), 140);
        assert_eq!(green(&g(), 10), 28);
        let s = Gradient { s_curve: true, ..g() };
        assert_eq!((green(&s, 120), green(&s, 200), green(&s, 400)), (39, 61, 140));
        let c = Gradient { circle: true, ..g() };
        assert_eq!(green(&c, 60), 42);
        assert!((Gradient { bg: None, ..g() }.paint(400, 0).1 as i32 - ONE as i32 / 2).abs() < 30);
    }
}

/// Слой в плотный буфер (предумноженные 0..=1) по прямоугольнику содержимого, расширенному на
/// `pad` и обрезанному холстом `w`×`h`.
/// Плотный буфер: левый верх, ширина, высота, точки.
type Dense = (i32, i32, usize, usize, Vec<[f64; 4]>);

fn dense(src: &ColorMap, w: u32, h: u32, pad: i32) -> Option<Dense> {
    let (x0, y0, x1, y1) = crate::selection::content_bounds(src)?;
    let (x0, y0) = ((x0 - pad).max(0), (y0 - pad).max(0));
    let (x1, y1) = ((x1 + pad).min(w as i32), (y1 + pad).min(h as i32));
    if x0 >= x1 || y0 >= y1 {
        return None;
    }
    let (bw, bh) = ((x1 - x0) as usize, (y1 - y0) as usize);
    let k = 1.0 / ONE as f64;
    let buf = (0..bw * bh).map(|i| src.get(x0 + (i % bw) as i32, y0 + (i / bw) as i32).map(|c| c as f64 * k)).collect();
    Some((x0, y0, bw, bh, buf))
}

/// Записать плотный результат обратно с долей выделения.
fn write_back(src: &ColorMap, x0: i32, y0: i32, bw: usize, res: &[[f64; 4]], sel: &(dyn Fn(i32, i32) -> u32 + Sync)) -> ColorMap {
    let mut out = src.clone();
    for (i, v) in res.iter().enumerate() {
        let (x, y) = (x0 + (i % bw) as i32, y0 + (i / bw) as i32);
        let s = sel(x, y);
        if s == 0 {
            continue;
        }
        let a = (v[3].clamp(0.0, 1.0) * ONE as f64 + 0.5) as u16;
        let q = |c: f64| ((c.clamp(0.0, 1.0) * ONE as f64 + 0.5) as u16).min(a);
        let n = [q(v[0]), q(v[1]), q(v[2]), a];
        let old = src.get(x, y);
        let p = if s >= ONE { n } else { mix(old, n, s) };
        if p != old {
            out.set(x, y, p);
        }
    }
    out.tiles.retain(|_, t| t.0.iter().any(|p| p[3] != 0));
    out
}

/// «Резкость» (как Sharpen / Sharpen More в Clip Studio Paint): свёртка крестом 3×3 — центр
/// `1 + 4s`, соседи `−s` (s = 0,5 обычная, 1 — сильная), по предумноженным каналам.
pub fn sharpen(src: &ColorMap, w: u32, h: u32, s: f64, sel: &(dyn Fn(i32, i32) -> u32 + Sync)) -> ColorMap {
    let Some((x0, y0, bw, bh, buf)) = dense(src, w, h, 1) else { return src.clone() };
    let at = |x: isize, y: isize| buf[y.clamp(0, bh as isize - 1) as usize * bw + x.clamp(0, bw as isize - 1) as usize];
    let res: Vec<[f64; 4]> = (0..bw * bh)
        .map(|i| {
            let (x, y) = ((i % bw) as isize, (i / bw) as isize);
            let (c, l, r, u, d) = (at(x, y), at(x - 1, y), at(x + 1, y), at(x, y - 1), at(x, y + 1));
            [0, 1, 2, 3].map(|k| c[k] * (1.0 + 4.0 * s) - s * (l[k] + r[k] + u[k] + d[k]))
        })
        .collect();
    write_back(src, x0, y0, bw, &res, sel)
}

/// «Нерезкая маска» (Unsharp mask): к точке прибавляется `сила·(точка − размытая)` там, где
/// разница больше порога (0–255); размытие — Гаусс с σ = радиус.
pub fn unsharp_mask(src: &ColorMap, w: u32, h: u32, radius: f64, amount: f64, threshold: u8, sel: &(dyn Fn(i32, i32) -> u32 + Sync)) -> ColorMap {
    let pad = (radius.max(0.1) * 3.0).ceil() as i32;
    let blurred = gaussian_blur(src, w, h, radius, &|_, _| ONE);
    let Some((x0, y0, bw, bh, buf)) = dense(src, w, h, pad) else { return src.clone() };
    let k = 1.0 / ONE as f64;
    let t = threshold as f64 / 255.0;
    let res: Vec<[f64; 4]> = (0..bw * bh)
        .map(|i| {
            let (x, y) = (x0 + (i % bw) as i32, y0 + (i / bw) as i32);
            let o = buf[i];
            let b = blurred.get(x, y).map(|c| c as f64 * k);
            let diff = [0, 1, 2, 3].map(|c| o[c] - b[c]);
            if diff.iter().fold(0.0f64, |m, d| m.max(d.abs())) <= t {
                o
            } else {
                [0, 1, 2, 3].map(|c| o[c] + amount * diff[c])
            }
        })
        .collect();
    let _ = bh;
    write_back(src, x0, y0, bw, &res, sel)
}

#[cfg(test)]
mod sharpen_tests {
    use super::*;
    use crate::pixel::{from_rgba8, to_rgba8};

    fn step() -> ColorMap {
        let mut m = ColorMap::new();
        for y in 0..20 {
            for x in 0..20 {
                let g = if x < 10 { 77 } else { 178 };
                m.set(x, y, from_rgba8([g, g, g, 255]));
            }
        }
        m
    }

    #[test]
    fn sharpen_raises_contrast_at_the_edge_only() {
        let out = sharpen(&step(), 20, 20, 0.5, &|_, _| ONE);
        let g = |x| to_rgba8(out.get(x, 10))[0] as i32;
        // 178 + 0,5·(178 − 77) ≈ 228; 77 − 0,5·101 ≈ 26; вдали от края — как было
        assert!((g(10) - 228).abs() <= 1 && (g(9) - 26).abs() <= 1, "{} {}", g(9), g(10));
        assert_eq!((g(3), g(16)), (77, 178));
    }

    #[test]
    fn unsharp_threshold_keeps_small_differences() {
        let src = step();
        let same = unsharp_mask(&src, 20, 20, 2.0, 1.0, 255, &|_, _| ONE);
        assert_eq!(to_rgba8(same.get(10, 10)), to_rgba8(src.get(10, 10)));
        let sharp = unsharp_mask(&src, 20, 20, 2.0, 1.0, 0, &|_, _| ONE);
        assert!(to_rgba8(sharp.get(10, 10))[0] > 178);
    }
}

/// «Изменение толщины линий» (как Adjust line width в Clip Studio Paint): `n` шагов, каждый берёт
/// у точки и соседей ту, что непрозрачнее (`thicken`) или прозрачнее; шаги по очереди квадратом
/// 3×3 и крестом, как расширение выделения (`selection::morph`).
pub fn adjust_line_width(src: &ColorMap, w: u32, h: u32, n: u32, thicken: bool, sel: &(dyn Fn(i32, i32) -> u32 + Sync)) -> ColorMap {
    let Some((x0, y0, x1, y1)) = crate::selection::content_bounds(src) else { return src.clone() };
    let pad = if thicken { n as i32 + 1 } else { 1 };
    let (x0, y0) = ((x0 - pad).max(0), (y0 - pad).max(0));
    let (x1, y1) = ((x1 + pad).min(w as i32), (y1 + pad).min(h as i32));
    if x0 >= x1 || y0 >= y1 {
        return src.clone();
    }
    let (bw, bh) = ((x1 - x0) as usize, (y1 - y0) as usize);
    let mut buf: Vec<Px> = (0..bw * bh).map(|i| src.get(x0 + (i % bw) as i32, y0 + (i / bw) as i32)).collect();
    let better = |a: Px, b: Px| if (b[3] > a[3]) == thicken && b[3] != a[3] { b } else { a };
    for step in 0..n {
        let prev = buf.clone();
        let at = |x: isize, y: isize| -> Px {
            if x < 0 || y < 0 || x >= bw as isize || y >= bh as isize {
                [0; 4]
            } else {
                prev[y as usize * bw + x as usize]
            }
        };
        let square = step % 2 == 0;
        for (i, out) in buf.iter_mut().enumerate() {
            let (x, y) = ((i % bw) as isize, (i / bw) as isize);
            let mut v = at(x, y);
            for dy in -1..=1isize {
                for dx in -1..=1isize {
                    if (dx, dy) != (0, 0) && (square || dx == 0 || dy == 0) {
                        v = better(v, at(x + dx, y + dy));
                    }
                }
            }
            *out = v;
        }
    }
    let mut out = src.clone();
    for (i, &p) in buf.iter().enumerate() {
        let (x, y) = (x0 + (i % bw) as i32, y0 + (i / bw) as i32);
        let s = sel(x, y);
        if s == 0 {
            continue;
        }
        let old = src.get(x, y);
        let n = if s >= ONE { p } else { mix(old, p, s) };
        if n != old {
            out.set(x, y, n);
        }
    }
    out.tiles.retain(|_, t| t.0.iter().any(|p| p[3] != 0));
    out
}

/// Что убирает «Удаление пыли».
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DustMode {
    /// Мелкие пятна на прозрачном — стереть.
    FromTransparency,
    /// Мелкие небелые пятна на белом — сделать белыми.
    FromWhite,
    /// Мелкие прозрачные дырки — залить цветом вокруг.
    GapsSurrounding,
    /// Мелкие прозрачные дырки — залить основным цветом.
    GapsColor([u8; 3]),
}

/// «Удаление пыли» (как Remove dust в Clip Studio Paint): связные (по 8 соседям) пятна площадью
/// не больше `size` точек убираются или заливаются по `mode`.
pub fn remove_dust(src: &ColorMap, w: u32, h: u32, size: u32, mode: DustMode, sel: &(dyn Fn(i32, i32) -> u32 + Sync)) -> ColorMap {
    let (w, h) = (w as usize, h as usize);
    let px: Vec<Px> = (0..w * h).map(|i| src.get((i % w) as i32, (i / w) as i32)).collect();
    // что считается «пятном»
    let white = |p: Px| crate::pixel::to_rgba8(p).iter().take(3).all(|&c| c >= 240) || p[3] == 0;
    let is_speck = |p: Px| match mode {
        DustMode::FromTransparency => p[3] > 0,
        DustMode::FromWhite => !white(p),
        DustMode::GapsSurrounding | DustMode::GapsColor(_) => p[3] == 0,
    };
    let mut seen = vec![false; w * h];
    let mut out = src.clone();
    let mut stack = Vec::new();
    let mut comp = Vec::new();
    for start in 0..w * h {
        if seen[start] || !is_speck(px[start]) {
            continue;
        }
        comp.clear();
        stack.push(start);
        seen[start] = true;
        let mut touches_edge = false;
        while let Some(i) = stack.pop() {
            comp.push(i);
            let (x, y) = ((i % w) as isize, (i / w) as isize);
            for dy in -1..=1isize {
                for dx in -1..=1isize {
                    let (nx, ny) = (x + dx, y + dy);
                    if nx < 0 || ny < 0 || nx >= w as isize || ny >= h as isize {
                        touches_edge = true;
                        continue;
                    }
                    let j = ny as usize * w + nx as usize;
                    if !seen[j] && is_speck(px[j]) {
                        seen[j] = true;
                        stack.push(j);
                    }
                }
            }
        }
        // дырка у края холста — не дырка
        let gap = matches!(mode, DustMode::GapsSurrounding | DustMode::GapsColor(_));
        if comp.len() > size as usize || (gap && touches_edge) {
            continue;
        }
        let fill: Option<Px> = match mode {
            DustMode::FromTransparency => Some([0; 4]),
            DustMode::FromWhite => Some(crate::pixel::from_rgba8([255, 255, 255, 255])),
            DustMode::GapsColor(c) => Some(crate::pixel::from_rgba8([c[0], c[1], c[2], 255])),
            DustMode::GapsSurrounding => {
                // средний цвет соседей дырки
                let mut sum = [0u64; 4];
                let mut n = 0u64;
                for &i in &comp {
                    let (x, y) = ((i % w) as isize, (i / w) as isize);
                    for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                        let (nx, ny) = (x + dx, y + dy);
                        if nx >= 0 && ny >= 0 && nx < w as isize && ny < h as isize {
                            let p = px[ny as usize * w + nx as usize];
                            if p[3] > 0 {
                                for c in 0..4 {
                                    sum[c] += p[c] as u64;
                                }
                                n += 1;
                            }
                        }
                    }
                }
                (n > 0).then(|| sum.map(|v| (v / n) as u16))
            }
        };
        let Some(fill) = fill else { continue };
        for &i in &comp {
            let (x, y) = ((i % w) as i32, (i / w) as i32);
            let s = sel(x, y);
            if s == 0 {
                continue;
            }
            let old = px[i];
            out.set(x, y, if s >= ONE { fill } else { mix(old, fill, s) });
        }
    }
    out.tiles.retain(|_, t| t.0.iter().any(|p| p[3] != 0));
    out
}

#[cfg(test)]
mod correction_tests {
    use super::*;
    use crate::pixel::from_rgba8;

    #[test]
    fn line_width_and_dust() {
        let black = from_rgba8([0, 0, 0, 255]);
        let mut m = ColorMap::new();
        for x in 5..35 {
            m.set(x, 20, black);
        }
        m.set(30, 5, black); // пылинка
        let thick = adjust_line_width(&m, 40, 40, 2, true, &|_, _| ONE);
        assert_eq!((thick.get(10, 18)[3], thick.get(10, 23)[3]), (ONE as u16, 0));
        let thin = adjust_line_width(&thick, 40, 40, 2, false, &|_, _| ONE);
        assert_eq!((thin.get(10, 20)[3], thin.get(10, 19)[3]), (ONE as u16, 0));
        let clean = remove_dust(&m, 40, 40, 4, DustMode::FromTransparency, &|_, _| ONE);
        assert_eq!((clean.get(30, 5)[3], clean.get(10, 20)[3]), (0, ONE as u16));
        // дырка в заливке закрывается цветом вокруг
        let mut f = ColorMap::new();
        for y in 0..10 {
            for x in 0..10 {
                if (x, y) != (5, 5) {
                    f.set(x, y, from_rgba8([200, 0, 0, 255]));
                }
            }
        }
        let filled = remove_dust(&f, 10, 10, 3, DustMode::GapsSurrounding, &|_, _| ONE);
        assert_eq!(crate::pixel::to_rgba8(filled.get(5, 5)), [200, 0, 0, 255]);
    }
}
