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
