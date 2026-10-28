//! Фильтры, как в SAI 2 (`docs/20-filters.md`): тон и насыщенность, яркость и контраст,
//! размытие по Гауссу. Цветовые фильтры работают с прямым (не предумноженным) цветом каждой
//! точки, альфа не меняется. Правила прочитаны в коде образца и сверены с живой SAI 2017.

use crate::pixel::{Px, CLEAR, ONE};
use crate::tile::{ColorMap, Tile, TILE};

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

/// «Уровни»: входные точки чёрного и белого (0–255), гамма середины (0,1–10; 1 — без изменений),
/// выходные точки чёрного и белого. Каналы RGB одинаково, альфа не меняется.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Levels {
    pub in_black: u8,
    pub in_white: u8,
    pub gamma: f64,
    pub out_black: u8,
    pub out_white: u8,
}

impl Default for Levels {
    fn default() -> Self {
        Self { in_black: 0, in_white: 255, gamma: 1.0, out_black: 0, out_white: 255 }
    }
}

impl Levels {
    pub fn map(&self, v: f64) -> f64 {
        let (ib, iw) = (self.in_black as f64 / 255.0, (self.in_white.max(self.in_black.saturating_add(1))) as f64 / 255.0);
        let t = ((v - ib) / (iw - ib)).clamp(0.0, 1.0).powf(1.0 / self.gamma.clamp(0.1, 10.0));
        let (ob, ow) = (self.out_black as f64 / 255.0, self.out_white as f64 / 255.0);
        ob + t * (ow - ob)
    }

    pub fn apply(&self, p: Px) -> Px {
        let Some((c, a)) = straight(p) else { return p };
        pack(c.map(|v| self.map(v)), a)
    }
}

/// «Кривые»: тон всех каналов RGB через кривую (две точки, монотонная, как кривая нажима).
pub fn curves(c: &crate::curve::PressureCurve, p: Px) -> Px {
    let Some((col, a)) = straight(p) else { return p };
    pack(col.map(|v| c.apply(v as f32) as f64), a)
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

/// Перевернуть буфер `w`×`h` (строки — в столбцы) блоками, параллельно.
fn transpose<T: Copy + Default + Send + Sync>(src: &[T], w: usize, h: usize) -> Vec<T> {
    let mut out = vec![T::default(); src.len()];
    let threads = crate::threads();
    // каждый поток пишет свои строки результата (столбцы исходника)
    let rows = w.div_ceil(threads).max(1);
    std::thread::scope(|scope| {
        for (n, chunk) in out.chunks_mut(rows * h).enumerate() {
            scope.spawn(move || {
                let x0 = n * rows;
                let nx = chunk.len() / h;
                for yb in (0..h).step_by(64) {
                    for xb in (0..nx).step_by(64) {
                        for x in xb..(xb + 64).min(nx) {
                            for y in yb..(yb + 64).min(h) {
                                chunk[x * h + y] = src[y * w + x0 + x];
                            }
                        }
                    }
                }
            });
        }
    });
    out
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
    let buf: Vec<[f64; 4]> = src.read_rect(x0, y0, bw, bh).into_iter().map(|p| p.map(|c| c as f64 * kf)).collect();
    let threads = crate::threads();
    // Порядок сложения у каждой точки — как в прямой свёртке (отводы ядра по возрастанию, за краем
    // пропускаются), поэтому результат тот же до бита. Отводы за краем отсекаются диапазоном, а не
    // проверкой на каждом; вертикальный проход — тот же горизонтальный по перевёрнутому буферу
    // (столбцы подряд в памяти, а не через строку).
    let pass = |buf: &[[f64; 4]], bw: usize, bh: usize| -> Vec<[f64; 4]> {
        let mut out = vec![[0f64; 4]; buf.len()];
        let rows = bh.div_ceil(threads).max(1);
        std::thread::scope(|scope| {
            for (n, chunk) in out.chunks_mut(rows * bw).enumerate() {
                let kernel = &kernel;
                scope.spawn(move || {
                    for (ry, orow) in chunk.chunks_mut(bw).enumerate() {
                        let srow = &buf[(n * rows + ry) * bw..][..bw];
                        for (x, o) in orow.iter_mut().enumerate() {
                            let lo = (r - x as i32).max(0) as usize;
                            let hi = ((bw - 1 - x) as i32 + r).min(2 * r) as usize;
                            let mut acc = [0f64; 4];
                            for (j, &kv) in kernel.iter().enumerate().take(hi + 1).skip(lo) {
                                let p = srow[x + j - r as usize];
                                for c in 0..4 {
                                    acc[c] += p[c] * kv;
                                }
                            }
                            *o = acc;
                        }
                    }
                });
            }
        });
        out
    };
    let blurred = transpose(&pass(&transpose(&pass(&buf, bw, bh), bw, bh), bh, bw), bh, bw);
    write_back(src, x0, y0, bw, &blurred, sel)
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
    let threads = crate::threads();
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
    let buf = src.read_rect(x0, y0, bw, bh).into_iter().map(|p| p.map(|c| c as f64 * k)).collect();
    Some((x0, y0, bw, bh, buf))
}

/// Плотный результат `w`×`h`: точка `i` (строками) — `f(i)`, строки по потокам.
fn par_rows<T: Copy + Default + Send>(w: usize, h: usize, f: impl Fn(usize) -> T + Sync) -> Vec<T> {
    let mut out = vec![T::default(); w * h];
    let rows = h.div_ceil(crate::threads()).max(1);
    std::thread::scope(|scope| {
        for (n, chunk) in out.chunks_mut((rows * w).max(1)).enumerate() {
            let f = &f;
            scope.spawn(move || {
                for (k, o) in chunk.iter_mut().enumerate() {
                    *o = f(n * rows * w + k);
                }
            });
        }
    });
    out
}

/// Записать плотный результат обратно с долей выделения — по плиткам, параллельно.
fn write_back(src: &ColorMap, x0: i32, y0: i32, bw: usize, res: &[[f64; 4]], sel: &(dyn Fn(i32, i32) -> u32 + Sync)) -> ColorMap {
    let mut out = src.clone();
    if bw == 0 || res.is_empty() {
        return out;
    }
    let (x1, y1) = (x0 + bw as i32, y0 + (res.len() / bw) as i32);
    let (k0, k1) = (crate::tile::key_of(x0, y0), crate::tile::key_of(x1 - 1, y1 - 1));
    let keys: Vec<(i32, i32)> = (k0.1..=k1.1).flat_map(|ky| (k0.0..=k1.0).map(move |kx| (kx, ky))).collect();
    let threads = crate::threads();
    let done: Vec<((i32, i32), Tile<Px>)> = std::thread::scope(|scope| {
        let parts: Vec<_> = keys
            .chunks(keys.len().div_ceil(threads).max(1))
            .map(|part| {
                scope.spawn(move || {
                    let mut v = Vec::with_capacity(part.len());
                    for &(kx, ky) in part {
                        let mut t = src.tiles.get(&(kx, ky)).cloned().unwrap_or_else(|| Tile::filled(CLEAR));
                        let (tx, ty) = (kx * TILE, ky * TILE);
                        for gy in y0.max(ty)..y1.min(ty + TILE) {
                            for gx in x0.max(tx)..x1.min(tx + TILE) {
                                let s = sel(gx, gy);
                                if s == 0 {
                                    continue;
                                }
                                let i = ((gy - ty) * TILE + gx - tx) as usize;
                                let v = res[(gy - y0) as usize * bw + (gx - x0) as usize];
                                let a = (v[3].clamp(0.0, 1.0) * ONE as f64 + 0.5) as u16;
                                let q = |c: f64| ((c.clamp(0.0, 1.0) * ONE as f64 + 0.5) as u16).min(a);
                                let n = [q(v[0]), q(v[1]), q(v[2]), a];
                                t.0[i] = if s >= ONE { n } else { mix(t.0[i], n, s) };
                            }
                        }
                        v.push(((kx, ky), t));
                    }
                    v
                })
            })
            .collect();
        parts.into_iter().flat_map(|p| p.join().unwrap()).collect()
    });
    out.tiles.extend(done);
    out.tiles.retain(|_, t| t.0.iter().any(|p| p[3] != 0));
    out
}

/// «Резкость» (как Sharpen / Sharpen More в Clip Studio Paint): свёртка крестом 3×3 — центр
/// `1 + 4s`, соседи `−s` (s = 0,5 обычная, 1 — сильная), по предумноженным каналам.
pub fn sharpen(src: &ColorMap, w: u32, h: u32, s: f64, sel: &(dyn Fn(i32, i32) -> u32 + Sync)) -> ColorMap {
    let Some((x0, y0, bw, bh, buf)) = dense(src, w, h, 1) else { return src.clone() };
    let at = |x: isize, y: isize| buf[y.clamp(0, bh as isize - 1) as usize * bw + x.clamp(0, bw as isize - 1) as usize];
    let res: Vec<[f64; 4]> = par_rows(bw, bh, |i| {
            let (x, y) = ((i % bw) as isize, (i / bw) as isize);
            let (c, l, r, u, d) = (at(x, y), at(x - 1, y), at(x + 1, y), at(x, y - 1), at(x, y + 1));
            [0, 1, 2, 3].map(|k| c[k] * (1.0 + 4.0 * s) - s * (l[k] + r[k] + u[k] + d[k]))
        });
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
    let res: Vec<[f64; 4]> = par_rows(bw, bh, |i| {
            let (x, y) = (x0 + (i % bw) as i32, y0 + (i / bw) as i32);
            let o = buf[i];
            let b = blurred.get(x, y).map(|c| c as f64 * k);
            let diff = [0, 1, 2, 3].map(|c| o[c] - b[c]);
            if diff.iter().fold(0.0f64, |m, d| m.max(d.abs())) <= t {
                o
            } else {
                [0, 1, 2, 3].map(|c| o[c] + amount * diff[c])
            }
        });
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

// ---------- «Эффекты» и «Отрисовка» как в Clip Studio Paint (0.8.33) ----------

/// Случайное 0..1 от целых координат (без состояния — одинаково при каждом предпросмотре).
fn hash01(x: i32, y: i32, seed: u32) -> f64 {
    let mut h = (x as u32).wrapping_mul(0x8da6_b343) ^ (y as u32).wrapping_mul(0xd816_3841) ^ seed.wrapping_mul(0xcb1a_b31f);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    (h & 0xff_ffff) as f64 / 16_777_215.0
}

/// Прямоугольник содержимого слоя на холсте (или пусто).
fn canvas_bounds(src: &ColorMap, w: u32, h: u32, pad: i32) -> Option<(i32, i32, i32, i32)> {
    let (x0, y0, x1, y1) = crate::selection::content_bounds(src)?;
    let b = ((x0 - pad).max(0), (y0 - pad).max(0), (x1 + pad).min(w as i32), (y1 + pad).min(h as i32));
    (b.0 < b.2 && b.1 < b.3).then_some(b)
}

/// Точка слоя с двулинейной выборкой (предумноженные 0..=1).
fn sample(src: &ColorMap, x: f64, y: f64) -> [f64; 4] {
    let (fx, fy) = (x - 0.5, y - 0.5);
    let (ix, iy) = (fx.floor() as i32, fy.floor() as i32);
    let (tx, ty) = (fx - ix as f64, fy - iy as f64);
    let k = 1.0 / ONE as f64;
    let g = |x: i32, y: i32| src.get(x, y).map(|c| c as f64 * k);
    let (a, b, c, d) = (g(ix, iy), g(ix + 1, iy), g(ix, iy + 1), g(ix + 1, iy + 1));
    [0, 1, 2, 3].map(|i| (a[i] * (1.0 - tx) + b[i] * tx) * (1.0 - ty) + (c[i] * (1.0 - tx) + d[i] * tx) * ty)
}

/// Общий проход: новая точка по координатам `f(x, y)` в пределах прямоугольника.
fn per_pixel(src: &ColorMap, b: (i32, i32, i32, i32), sel: &(dyn Fn(i32, i32) -> u32 + Sync), f: &(dyn Fn(i32, i32) -> [f64; 4] + Sync)) -> ColorMap {
    let (x0, y0, x1, y1) = b;
    let bw = (x1 - x0) as usize;
    let mut res = vec![[0f64; 4]; bw * (y1 - y0) as usize];
    let parts = crate::threads();
    let chunk = res.len().div_ceil(parts).max(1);
    std::thread::scope(|scope| {
        for (k, part) in res.chunks_mut(chunk).enumerate() {
            scope.spawn(move || {
                for (j, o) in part.iter_mut().enumerate() {
                    let i = k * chunk + j;
                    *o = f(x0 + (i % bw) as i32, y0 + (i / bw) as i32);
                }
            });
        }
    });
    write_back(src, x0, y0, bw, &res, sel)
}

/// «Хроматическая аберрация»: красный и синий расходятся на `intensity` px — по кругу от
/// середины холста (`radial`) или вдоль угла `angle` (градусы).
pub fn chromatic_aberration(src: &ColorMap, w: u32, h: u32, radial: bool, intensity: f64, angle: f64, sel: &(dyn Fn(i32, i32) -> u32 + Sync)) -> ColorMap {
    let pad = intensity.ceil() as i32 + 1;
    let Some(b) = canvas_bounds(src, w, h, pad) else { return src.clone() };
    let (cx, cy) = (w as f64 / 2.0, h as f64 / 2.0);
    let far = cx.hypot(cy).max(1.0);
    let (dx, dy) = (angle.to_radians().cos() * intensity, angle.to_radians().sin() * intensity);
    per_pixel(src, b, sel, &|x, y| {
        let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
        let (sx, sy) = if radial { ((px - cx) / far * intensity, (py - cy) / far * intensity) } else { (dx, dy) };
        // за краем холста — край (иначе у края цветная рамка)
        let cl = |x: f64, y: f64| (x.clamp(0.5, w as f64 - 0.5), y.clamp(0.5, h as f64 - 0.5));
        let ((rx, ry), (bx, by)) = (cl(px - sx, py - sy), cl(px + sx, py + sy));
        let r = sample(src, rx, ry);
        let g = sample(src, px, py);
        let bl = sample(src, bx, by);
        let a = r[3].max(g[3]).max(bl[3]);
        [r[0], g[1], bl[2], a]
    })
}

/// «Шум»: случайная добавка ±сила/2 к каждой точке (цветной — к каждому каналу своя).
pub fn noise(src: &ColorMap, w: u32, h: u32, color: bool, strength: f64, sel: &(dyn Fn(i32, i32) -> u32 + Sync)) -> ColorMap {
    let Some(b) = canvas_bounds(src, w, h, 0) else { return src.clone() };
    let k = 1.0 / ONE as f64;
    per_pixel(src, b, sel, &|x, y| {
        let p = src.get(x, y).map(|c| c as f64 * k);
        let n = |s: u32| (hash01(x, y, s) - 0.5) * strength * p[3];
        let g = n(1);
        let d = if color { [n(1), n(2), n(3)] } else { [g, g, g] };
        [p[0] + d[0], p[1] + d[1], p[2] + d[2], p[3]]
    })
}

/// «Мозаика»: квадраты `tile` px (от угла холста) — среднее по квадрату.
pub fn mosaic(src: &ColorMap, w: u32, h: u32, tile: u32, sel: &(dyn Fn(i32, i32) -> u32 + Sync)) -> ColorMap {
    let t = tile.max(1) as i32;
    let Some((x0, y0, x1, y1)) = canvas_bounds(src, w, h, 0) else { return src.clone() };
    let (x0, y0) = (x0.div_euclid(t) * t, y0.div_euclid(t) * t);
    let (x1, y1) = ((x1 + t - 1).div_euclid(t) * t, (y1 + t - 1).div_euclid(t) * t);
    let k = 1.0 / ONE as f64;
    let mut avg = std::collections::HashMap::new();
    for ty in (y0..y1).step_by(t as usize) {
        for tx in (x0..x1).step_by(t as usize) {
            let mut s = [0f64; 4];
            let mut n = 0.0f64;
            for y in ty..(ty + t).min(h as i32) {
                for x in tx..(tx + t).min(w as i32) {
                    let p = src.get(x, y);
                    for c in 0..4 {
                        s[c] += p[c] as f64 * k;
                    }
                    n += 1.0;
                }
            }
            avg.insert((tx, ty), s.map(|v| v / n.max(1.0)));
        }
    }
    per_pixel(src, (x0, y0, x1.min(w as i32), y1.min(h as i32)), sel, &|x, y| avg[&(x.div_euclid(t) * t, y.div_euclid(t) * t)])
}

/// «Медиана»: каждый канал (с предумноженной альфой) — медиана окна (2r+1)×(2r+1); убирает
/// одиночные точки, сохраняя края. Радиус 1–5.
pub fn median(src: &ColorMap, w: u32, h: u32, radius: u32, sel: &(dyn Fn(i32, i32) -> u32 + Sync)) -> ColorMap {
    let r = radius.clamp(1, 5) as i32;
    let Some(b) = canvas_bounds(src, w, h, r) else { return src.clone() };
    let k = 1.0 / ONE as f64;
    // соседи всех точек рамки — в рамке, расширенной ещё на r (за содержимым всё равно пусто):
    // плотный буфер вместо поиска плитки на каждого соседа
    let (rx0, ry0) = ((b.0 - r).max(0), (b.1 - r).max(0));
    let (rx1, ry1) = ((b.2 + r).min(w as i32), (b.3 + r).min(h as i32));
    let rw = (rx1 - rx0) as usize;
    let px = src.read_rect(rx0, ry0, rw, (ry1 - ry0) as usize);
    per_pixel(src, b, sel, &|x, y| {
        // окно до 11×11 — на стеке; медиана по `select_nth` та же, что по сортировке
        let mut ch = [[0u16; 121]; 4];
        let mut n = 0;
        for yy in (y - r).max(0)..=(y + r).min(h as i32 - 1) {
            let row = (yy - ry0) as usize * rw;
            for xx in (x - r).max(0)..=(x + r).min(w as i32 - 1) {
                let p = px[row + (xx - rx0) as usize];
                for c in 0..4 {
                    ch[c][n] = p[c];
                }
                n += 1;
            }
        }
        let mut out = [0f64; 4];
        for c in 0..4 {
            let v = &mut ch[c][..n];
            let mid = n / 2;
            out[c] = *v.select_nth_unstable(mid).1 as f64 * k;
        }
        // предумножение: цвет не ярче альфы
        for c in 0..3 {
            out[c] = out[c].min(out[3]);
        }
        out
    })
}

/// «Кристаллизация»: ячейки Вороного вокруг точек сетки `cell` px со сдвигом `random` (0..1);
/// точка получает средний цвет своей ячейки. `tiling` — ячейки повторяются по краям холста.
pub fn crystallize(src: &ColorMap, w: u32, h: u32, cell: u32, random: f64, tiling: bool, sel: &(dyn Fn(i32, i32) -> u32 + Sync)) -> ColorMap {
    let c = cell.max(2) as i32;
    let (gw, gh) = ((w as i32 + c - 1) / c, (h as i32 + c - 1) / c);
    let site = |gx: i32, gy: i32| -> (f64, f64) {
        let (wx, wy) = if tiling { (gx.rem_euclid(gw), gy.rem_euclid(gh)) } else { (gx, gy) };
        let jx = (hash01(wx, wy, 7) - 0.5) * random;
        let jy = (hash01(wx, wy, 11) - 0.5) * random;
        ((gx as f64 + 0.5 + jx) * c as f64, (gy as f64 + 0.5 + jy) * c as f64)
    };
    let owner = |x: i32, y: i32| -> (i32, i32) {
        let (gx, gy) = (x.div_euclid(c), y.div_euclid(c));
        let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
        let mut best = (f64::MAX, (gx, gy));
        for dy in -1..=1 {
            for dx in -1..=1 {
                let (sx, sy) = site(gx + dx, gy + dy);
                let d = (sx - px).powi(2) + (sy - py).powi(2);
                if d < best.0 {
                    best = (d, (gx + dx, gy + dy));
                }
            }
        }
        let (ox, oy) = best.1;
        if tiling { (ox.rem_euclid(gw), oy.rem_euclid(gh)) } else { (ox, oy) }
    };
    let Some(b) = canvas_bounds(src, w, h, c) else { return src.clone() };
    let k = 1.0 / ONE as f64;
    let mut sums: std::collections::HashMap<(i32, i32), ([f64; 4], f64)> = std::collections::HashMap::new();
    for y in b.1..b.3 {
        for x in b.0..b.2 {
            let p = src.get(x, y);
            let e = sums.entry(owner(x, y)).or_insert(([0.0; 4], 0.0));
            for (acc, &v) in e.0.iter_mut().zip(&p) {
                *acc += v as f64 * k;
            }
            e.1 += 1.0;
        }
    }
    per_pixel(src, b, sel, &|x, y| sums.get(&owner(x, y)).map_or([0.0; 4], |(s, n)| s.map(|v| v / n)))
}

/// Пресет и эффект «Ретро-плёнки».
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RetroFilm {
    /// 0 — винтаж (выцветание), 1 — современный (холоднее), 2 — тёплый.
    pub preset: u8,
    /// 0 — нет, 1 — сепия, 2 — засветка, 3 — сепия и засветка.
    pub effect: u8,
    pub intensity: f64,
    pub noise: f64,
}

/// «Ретро-плёнка»: тон пресета, сепия, засветка от угла, шум и лёгкая аберрация.
pub fn retro_film(src: &ColorMap, w: u32, h: u32, r: RetroFilm, sel: &(dyn Fn(i32, i32) -> u32 + Sync)) -> ColorMap {
    let ca = chromatic_aberration(src, w, h, true, 2.0 * r.intensity, 0.0, &|_, _| ONE);
    let Some(b) = canvas_bounds(&ca, w, h, 0) else { return src.clone() };
    let k = 1.0 / ONE as f64;
    let diag = (w as f64).hypot(h as f64).max(1.0);
    per_pixel(src, b, sel, &|x, y| {
        let p = ca.get(x, y).map(|c| c as f64 * k);
        if p[3] <= 0.0 {
            return [0.0; 4];
        }
        let mut c = [p[0] / p[3], p[1] / p[3], p[2] / p[3]];
        let t = r.intensity;
        // тон пресета
        let tone = match r.preset {
            1 => [0.96, 1.0, 1.06],
            2 => [1.08, 1.0, 0.88],
            _ => [1.04, 1.0, 0.92],
        };
        let fade = if r.preset == 0 { 0.12 * t } else { 0.05 * t };
        for i in 0..3 {
            c[i] = (c[i] * (1.0 + (tone[i] - 1.0) * t)) * (1.0 - fade) + fade * 0.5;
        }
        if r.effect & 1 != 0 {
            let l = 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2];
            let sepia = [l * 1.07 + 0.05, l * 0.95 + 0.02, l * 0.75];
            for i in 0..3 {
                c[i] += (sepia[i] - c[i]) * t;
            }
        }
        if r.effect & 2 != 0 {
            // засветка: оранжевое пятно от левого верхнего угла
            let d = ((x as f64).hypot(y as f64) / diag).min(1.0);
            let leak = (1.0 - d * 1.6).max(0.0).powi(2) * t;
            let col = [1.0, 0.55, 0.2];
            for i in 0..3 {
                c[i] = c[i] + (1.0 - c[i]) * col[i] * leak;
            }
        }
        let n = (hash01(x, y, 3) - 0.5) * r.noise;
        let q = |i: usize| (c[i] + n).clamp(0.0, 1.0) * p[3];
        [q(0), q(1), q(2), p[3]]
    })
}

/// Настройки «Шума Перлина».
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Perlin {
    /// Размер узора, px.
    pub scale: f64,
    pub amplitude: f64,
    /// Ослабление каждой следующей октавы (0..1).
    pub attenuation: f64,
    /// Число октав.
    pub repetition: u32,
    pub offset: [f64; 2],
}

fn perlin2(x: f64, y: f64) -> f64 {
    let (xi, yi) = (x.floor() as i32, y.floor() as i32);
    let (xf, yf) = (x - xi as f64, y - yi as f64);
    let grad = |ix: i32, iy: i32, dx: f64, dy: f64| {
        let a = hash01(ix, iy, 99) * std::f64::consts::TAU;
        a.cos() * dx + a.sin() * dy
    };
    let fade = |t: f64| t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
    let (u, v) = (fade(xf), fade(yf));
    let n00 = grad(xi, yi, xf, yf);
    let n10 = grad(xi + 1, yi, xf - 1.0, yf);
    let n01 = grad(xi, yi + 1, xf, yf - 1.0);
    let n11 = grad(xi + 1, yi + 1, xf - 1.0, yf - 1.0);
    let a = n00 + (n10 - n00) * u;
    let b = n01 + (n11 - n01) * u;
    a + (b - a) * v
}

/// «Шум Перлина»: облачный серый узор по всему холсту (содержимое слоя заменяется, как у CSP).
pub fn perlin_noise(src: &ColorMap, w: u32, h: u32, p: Perlin, sel: &(dyn Fn(i32, i32) -> u32 + Sync)) -> ColorMap {
    per_pixel(src, (0, 0, w as i32, h as i32), sel, &|x, y| {
        let (mut f, mut a, mut sum) = (1.0 / p.scale.max(1.0), 1.0, 0.0);
        for _ in 0..p.repetition.clamp(1, 10) {
            sum += perlin2((x as f64 + p.offset[0]) * f, (y as f64 + p.offset[1]) * f) * a;
            f *= 2.0;
            a *= p.attenuation;
        }
        let v = (0.5 + sum * p.amplitude).clamp(0.0, 1.0);
        [v, v, v, 1.0]
    })
}

#[cfg(test)]
mod effect_tests {
    use super::*;
    use crate::pixel::{from_rgba8, to_rgba8};

    fn spot() -> ColorMap {
        let mut m = ColorMap::new();
        for y in 0..40 {
            for x in 0..40 {
                let v = if (10..30).contains(&x) { 200 } else { 50 };
                m.set(x, y, from_rgba8([v, v, v, 255]));
            }
        }
        m
    }

    #[test]
    fn effects_do_what_they_say() {
        let all = |_: i32, _: i32| ONE;
        // мозаика 10 px: квадрат 0..10 по x — ровный цвет
        let m = mosaic(&spot(), 40, 40, 10, &all);
        assert_eq!(to_rgba8(m.get(0, 0)), to_rgba8(m.get(9, 9)));
        // аберрация сдвигом вправо: у левого края светлой полосы красный уже тёмный, синий светлый
        let ca = chromatic_aberration(&spot(), 40, 40, false, 3.0, 0.0, &all);
        let c = to_rgba8(ca.get(11, 20));
        assert!(c[0] < c[2], "{c:?}");
        // шум серый меняет, но не трогает альфу
        let n = noise(&spot(), 40, 40, false, 0.5, &all);
        assert_ne!(to_rgba8(n.get(5, 5)), to_rgba8(spot().get(5, 5)));
        assert_eq!(n.get(5, 5)[3], ONE as u16);
        // кристаллизация — значения из исходных (средние по ячейкам), Перлин — непрозрачный серый
        let cr = crystallize(&spot(), 40, 40, 8, 0.5, false, &all);
        assert!(to_rgba8(cr.get(20, 20))[0] >= 50);
        let pn = perlin_noise(&ColorMap::new(), 32, 32, Perlin { scale: 8.0, amplitude: 1.0, attenuation: 0.5, repetition: 3, offset: [0.0, 0.0] }, &all);
        let g = to_rgba8(pn.get(5, 7));
        assert!(g[0] == g[1] && g[3] == 255);
    }
}

// ---------- «Размытие» как в Clip Studio Paint (0.8.34) ----------

/// Середина рисунка слоя (для радиального размытия, вращения и искажений), иначе холста.
pub fn content_center(src: &ColorMap, w: u32, h: u32) -> (f64, f64) {
    crate::selection::content_bounds(src).map_or((w as f64 / 2.0, h as f64 / 2.0), |(x0, y0, x1, y1)| ((x0 + x1) as f64 / 2.0, (y0 + y1) as f64 / 2.0))
}

/// «Среднее»: выделенное (или весь рисунок слоя) — одним средним цветом.
pub fn average(src: &ColorMap, w: u32, h: u32, sel: &(dyn Fn(i32, i32) -> u32 + Sync)) -> ColorMap {
    let Some(b) = canvas_bounds(src, w, h, 0) else { return src.clone() };
    let k = 1.0 / ONE as f64;
    let (mut sum, mut n) = ([0f64; 4], 0f64);
    for y in b.1..b.3 {
        for x in b.0..b.2 {
            let s = sel(x, y) as f64 * k;
            if s > 0.0 {
                let p = src.get(x, y);
                for c in 0..4 {
                    sum[c] += p[c] as f64 * k * s;
                }
                n += s;
            }
        }
    }
    if n <= 0.0 {
        return src.clone();
    }
    let avg = sum.map(|v| v / n);
    per_pixel(src, b, sel, &|_, _| avg)
}

/// Размытие по линиям выборки: точка — среднее (или с весом «плавно») по `n` точкам `pts(x, y, t)`,
/// t от 0 до 1.
fn line_blur(src: &ColorMap, b: (i32, i32, i32, i32), samples: usize, smooth: bool, sel: &(dyn Fn(i32, i32) -> u32 + Sync), at: &(dyn Fn(f64, f64, f64) -> (f64, f64) + Sync)) -> ColorMap {
    let n = samples.max(1);
    per_pixel(src, b, sel, &|x, y| {
        let (mut acc, mut wsum) = ([0f64; 4], 0f64);
        for i in 0..n {
            let t = if n == 1 { 0.5 } else { i as f64 / (n - 1) as f64 };
            // «плавно» — треугольный вес к середине выборки
            let wt = if smooth { 1.0 - (2.0 * t - 1.0).abs() * 0.9 } else { 1.0 };
            let (sx, sy) = at(x as f64 + 0.5, y as f64 + 0.5, t);
            let p = sample(src, sx, sy);
            for c in 0..4 {
                acc[c] += p[c] * wt;
            }
            wsum += wt;
        }
        acc.map(|v| v / wsum)
    })
}

/// Направленное размытие: сила (px для движения, доля для радиального), угол (градусы, только
/// движение), направление (0 — в обе стороны, 1 — вперёд/наружу, 2 — назад/внутрь), «плавно».
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Streak {
    pub strength: f64,
    pub angle: f64,
    pub dir: u8,
    pub smooth: bool,
}

/// «Размытие в движении»: длина `strength` px под углом.
pub fn motion_blur(src: &ColorMap, w: u32, h: u32, st: Streak, sel: &(dyn Fn(i32, i32) -> u32 + Sync)) -> ColorMap {
    let Streak { strength: len, angle, dir, smooth } = st;
    let Some(b) = canvas_bounds(src, w, h, len.ceil() as i32 + 1) else { return src.clone() };
    let (ux, uy) = (angle.to_radians().cos(), angle.to_radians().sin());
    let (a, z) = match dir {
        1 => (0.0, len),
        2 => (-len, 0.0),
        _ => (-len / 2.0, len / 2.0),
    };
    line_blur(src, b, (len.ceil() as usize).clamp(1, 400), smooth, sel, &|x, y, t| {
        let d = a + (z - a) * t;
        (x - ux * d, y - uy * d)
    })
}

/// «Радиальное размытие»: вдоль луча от центра на долю `strength` расстояния.
pub fn radial_blur(src: &ColorMap, w: u32, h: u32, center: (f64, f64), st: Streak, sel: &(dyn Fn(i32, i32) -> u32 + Sync)) -> ColorMap {
    let Streak { strength, dir, smooth, .. } = st;
    let Some(b) = canvas_bounds(src, w, h, 0).map(|_| (0, 0, w as i32, h as i32)) else { return src.clone() };
    let (a, z) = match dir {
        1 => (0.0, strength),
        2 => (-strength, 0.0),
        _ => (-strength / 2.0, strength / 2.0),
    };
    let far = (w as f64).hypot(h as f64);
    line_blur(src, b, ((strength * far).ceil() as usize).clamp(1, 200), smooth, sel, &|x, y, t| {
        let k = 1.0 - (a + (z - a) * t);
        (center.0 + (x - center.0) * k, center.1 + (y - center.1) * k)
    })
}

/// «Размытие вращением»: по дуге вокруг центра на `angle` градусов; `dir` 0 — в обе стороны,
/// 1 — по часовой, 2 — против.
pub fn spin_blur(src: &ColorMap, w: u32, h: u32, center: (f64, f64), angle: f64, dir: u8, sel: &(dyn Fn(i32, i32) -> u32 + Sync)) -> ColorMap {
    let b = (0, 0, w as i32, h as i32);
    if crate::selection::content_bounds(src).is_none() {
        return src.clone();
    }
    let ang = angle.to_radians();
    let (a, z) = match dir {
        1 => (0.0, ang),
        2 => (-ang, 0.0),
        _ => (-ang / 2.0, ang / 2.0),
    };
    let far = (w as f64).hypot(h as f64) / 2.0;
    line_blur(src, b, ((ang * far).ceil() as usize).clamp(1, 360), true, sel, &|x, y, t| {
        let r = -(a + (z - a) * t);
        let (dx, dy) = (x - center.0, y - center.1);
        let (s, c) = r.sin_cos();
        (center.0 + dx * c - dy * s, center.1 + dx * s + dy * c)
    })
}

/// «Размытие» и «Сильное размытие»: квадрат `2r + 1` (r = 1 и 2), дважды — мягче, как у CSP.
pub fn box_blur(src: &ColorMap, w: u32, h: u32, r: i32, sel: &(dyn Fn(i32, i32) -> u32 + Sync)) -> ColorMap {
    let Some((x0, y0, bw, bh, mut buf)) = dense(src, w, h, 2 * r + 1) else { return src.clone() };
    for _ in 0..2 {
        for horizontal in [true, false] {
            let prev = buf.clone();
            let at = |x: isize, y: isize| prev[y.clamp(0, bh as isize - 1) as usize * bw + x.clamp(0, bw as isize - 1) as usize];
            for (i, o) in buf.iter_mut().enumerate() {
                let (x, y) = ((i % bw) as isize, (i / bw) as isize);
                let mut acc = [0f64; 4];
                for d in -(r as isize)..=r as isize {
                    let p = if horizontal { at(x + d, y) } else { at(x, y + d) };
                    for c in 0..4 {
                        acc[c] += p[c];
                    }
                }
                *o = acc.map(|v| v / (2 * r + 1) as f64);
            }
        }
    }
    write_back(src, x0, y0, bw, &buf, sel)
}

/// «Сглаживание»: размытие 3×3 только там, где край — разница с соседями больше порога (ступеньки
/// линий), ровные места не трогаются.
pub fn smoothing(src: &ColorMap, w: u32, h: u32, sel: &(dyn Fn(i32, i32) -> u32 + Sync)) -> ColorMap {
    let Some((x0, y0, bw, bh, buf)) = dense(src, w, h, 1) else { return src.clone() };
    let at = |x: isize, y: isize| buf[y.clamp(0, bh as isize - 1) as usize * bw + x.clamp(0, bw as isize - 1) as usize];
    let res: Vec<[f64; 4]> = par_rows(bw, bh, |i| {
            let (x, y) = ((i % bw) as isize, (i / bw) as isize);
            let c = at(x, y);
            let edge = [(-1, 0), (1, 0), (0, -1), (0, 1)].iter().any(|&(dx, dy)| {
                let n = at(x + dx, y + dy);
                (0..4).any(|k| (n[k] - c[k]).abs() > 0.25)
            });
            if !edge {
                return c;
            }
            let mut acc = [0f64; 4];
            for (dy, wy) in [(-1isize, 1.0), (0, 2.0), (1, 1.0)] {
                for (dx, wx) in [(-1isize, 1.0), (0, 2.0), (1, 1.0)] {
                    let p = at(x + dx, y + dy);
                    for k in 0..4 {
                        acc[k] += p[k] * wx * wy;
                    }
                }
            }
            acc.map(|v| v / 16.0)
        });
    write_back(src, x0, y0, bw, &res, sel)
}

/// «Размытие линзы»: усреднение по кругу (или многоугольнику с `sides` ≥ 3 углами) радиуса
/// `radius`; светлые места усиливаются (блики) в доле `highlight`. Круг считается полосами строк
/// с накопленными суммами — быстро и для больших радиусов.
pub fn lens_blur(src: &ColorMap, w: u32, h: u32, radius: f64, sides: u32, highlight: f64, sel: &(dyn Fn(i32, i32) -> u32 + Sync)) -> ColorMap {
    let r = radius.max(0.5).ceil() as i32;
    let Some((x0, y0, bw, bh, buf)) = dense(src, w, h, r) else { return src.clone() };
    // блики: значения поднимаются в степень перед усреднением и возвращаются после
    let gamma = 1.0 + 2.0 * highlight.clamp(0.0, 1.0);
    let lin: Vec<[f64; 4]> = par_rows(bw, bh, |i| {
        let p = buf[i];
        if p[3] <= 0.0 {
            return [0.0; 4];
        }
        let f = |c: f64| (c / p[3]).clamp(0.0, 1.0).powf(gamma) * p[3];
        [f(p[0]), f(p[1]), f(p[2]), p[3]]
    });
    // ширина фигуры в строке dy: круг или правильный многоугольник
    let half = |dy: i32| -> f64 {
        let y = dy as f64;
        if sides < 3 {
            return (radius * radius - y * y).max(0.0).sqrt();
        }
        // наибольшее |x| точки многоугольника на высоте y (вершина сверху)
        let n = sides as f64;
        let mut best = -1.0f64;
        for i in 0..sides {
            let a0 = std::f64::consts::FRAC_PI_2 + i as f64 * std::f64::consts::TAU / n;
            let a1 = a0 + std::f64::consts::TAU / n;
            let (p0, p1) = ((radius * a0.cos(), radius * a0.sin()), (radius * a1.cos(), radius * a1.sin()));
            if (p0.1 - y) * (p1.1 - y) <= 0.0 && (p1.1 - p0.1).abs() > 1e-9 {
                let t = (y - p0.1) / (p1.1 - p0.1);
                best = best.max((p0.0 + (p1.0 - p0.0) * t).abs());
            }
        }
        best.max(0.0)
    };
    let spans: Vec<(i32, i32)> = (-r..=r).map(|dy| (dy, half(dy).floor() as i32)).filter(|s| s.1 >= 0 || s.0 == 0).collect();
    // накопленные суммы по строкам (строки — по потокам)
    let mut pre = vec![[0f64; 4]; (bw + 1) * bh];
    let rows = bh.div_ceil(crate::threads()).max(1);
    std::thread::scope(|scope| {
        for (n, chunk) in pre.chunks_mut(rows * (bw + 1)).enumerate() {
            let lin = &lin;
            scope.spawn(move || {
                for (ry, row) in chunk.chunks_mut(bw + 1).enumerate() {
                    let y = n * rows + ry;
                    for x in 0..bw {
                        let (a, p) = (row[x], lin[y * bw + x]);
                        row[x + 1] = [a[0] + p[0], a[1] + p[1], a[2] + p[2], a[3] + p[3]];
                    }
                }
            });
        }
    });
    let res: Vec<[f64; 4]> = par_rows(bw, bh, |i| {
        {
            let (x, y) = ((i % bw) as i32, (i / bw) as i32);
            let (mut acc, mut n) = ([0f64; 4], 0f64);
            for &(dy, hw) in &spans {
                let yy = y + dy;
                if yy < 0 || yy >= bh as i32 {
                    continue;
                }
                let (a, z) = ((x - hw).max(0) as usize, ((x + hw + 1) as usize).min(bw));
                if a >= z {
                    continue;
                }
                let row = yy as usize * (bw + 1);
                for c in 0..4 {
                    acc[c] += pre[row + z][c] - pre[row + a][c];
                }
                n += (z - a) as f64;
            }
            let p = acc.map(|v| v / n.max(1.0));
            if p[3] <= 0.0 {
                return [0.0; 4];
            }
            let f = |c: f64| (c / p[3]).clamp(0.0, 1.0).powf(1.0 / gamma) * p[3];
            [f(p[0]), f(p[1]), f(p[2]), p[3]]
        }
    });
    write_back(src, x0, y0, bw, &res, sel)
}

/// Вид искажения (подменю «Искажение» как в Clip Studio Paint).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DistortKind {
    Pinch,
    Ripple,
    Fisheye,
    Polar,
    Wave,
    Twirl,
}

/// Настройки искажения.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Distort {
    pub kind: DistortKind,
    /// Середина (точки холста) и радиус действия (px): щипок, рябь, «рыбий глаз», скручивание;
    /// у полярных координат — только середина.
    pub center: (f64, f64),
    pub radius: f64,
    /// Щипок −1…1 (плюс — к середине), «рыбий глаз» 0…1, скручивание — угол в градусах.
    pub amount: f64,
    /// Рябь и волна: размах и длина волны (px); волна: угол сдвига (градусы) и форма
    /// (0 — синус, 1 — треугольник, 2 — прямоугольник).
    pub amplitude: f64,
    pub wavelength: f64,
    pub angle: f64,
    pub shape: u8,
    /// Полярные координаты: из прямоугольных в полярные (`true`) или обратно.
    pub to_polar: bool,
}

/// Откуда взять точку `(px, py)` после искажения; `None` — точка не сдвигается.
fn distort_source(d: &Distort, w: u32, h: u32, px: f64, py: f64) -> Option<(f64, f64)> {
    use std::f64::consts::{FRAC_PI_2, TAU};
    let (cx, cy) = d.center;
    let (dx, dy) = (px - cx, py - cy);
    let r = dx.hypot(dy);
    let t = r / d.radius.max(1.0);
    let scaled = |k: f64| Some((cx + dx * k, cy + dy * k));
    let local = t < 1.0 && r > 0.0;
    match d.kind {
        // щипок: множитель sin(π/2·t)^(−a) — на краю круга 1, к середине растёт (сжатие) или падает
        DistortKind::Pinch if local => scaled((t * FRAC_PI_2).sin().powf(-0.75 * d.amount.clamp(-1.0, 1.0))),
        // «рыбий глаз»: r' = r·(1 − s + s·t²) — середина увеличена в 1/(1 − s) раз
        DistortKind::Fisheye if local => {
            let s = d.amount.clamp(0.0, 0.95);
            scaled(1.0 - s + s * t * t)
        }
        // скручивание: поворот на угол·(1 − t)², к краю круга сходит на нет
        DistortKind::Twirl if local => {
            let (sn, cs) = (-d.amount.to_radians() * (1.0 - t) * (1.0 - t)).sin_cos();
            Some((cx + dx * cs - dy * sn, cy + dx * sn + dy * cs))
        }
        // рябь: сдвиг вдоль луча на размах·sin(2πr/λ); затухает к краю круга и в первые
        // полволны у середины (иначе середина выворачивается)
        DistortKind::Ripple if local => {
            let lam = d.wavelength.max(1.0);
            scaled((r + d.amplitude * (TAU * r / lam).sin() * (1.0 - t) * (2.0 * r / lam).min(1.0)) / r)
        }
        DistortKind::Wave => {
            let (sn, cs) = d.angle.to_radians().sin_cos();
            let q = (-px * sn + py * cs) / d.wavelength.max(1.0);
            let f = match d.shape {
                1 => 4.0 * ((q - 0.25).rem_euclid(1.0) - 0.5).abs() - 1.0,
                2 => {
                    if q.rem_euclid(1.0) < 0.5 {
                        1.0
                    } else {
                        -1.0
                    }
                }
                _ => (TAU * q).sin(),
            };
            Some((px - cs * d.amplitude * f, py - sn * d.amplitude * f))
        }
        // полярные: верх прямоугольника — середина круга, ширина — оборот по часовой от 12 часов;
        // радиус круга — половина меньшей стороны холста
        DistortKind::Polar => {
            let rm = w.min(h) as f64 / 2.0;
            if d.to_polar {
                let turn = (dx.atan2(-dy) / TAU).rem_euclid(1.0);
                Some((turn * w as f64, r / rm * h as f64))
            } else {
                let (sn, cs) = (px / w as f64 * TAU).sin_cos();
                let rr = py / h as f64 * rm;
                Some((cx + rr * sn, cy - rr * cs))
            }
        }
        _ => None,
    }
}

/// «Искажение»: каждая точка холста берётся из места, которое даёт `distort_source`
/// (двулинейно; за краем холста — край).
pub fn distort(src: &ColorMap, w: u32, h: u32, d: &Distort, sel: &(dyn Fn(i32, i32) -> u32 + Sync)) -> ColorMap {
    if crate::selection::content_bounds(src).is_none() {
        return src.clone();
    }
    let k = 1.0 / ONE as f64;
    per_pixel(src, (0, 0, w as i32, h as i32), sel, &|x, y| {
        let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
        match distort_source(d, w, h, px, py) {
            Some((sx, sy)) => sample(src, sx.clamp(0.5, w as f64 - 0.5), sy.clamp(0.5, h as f64 - 0.5)),
            None => src.get(x, y).map(|c| c as f64 * k),
        }
    })
}

/// Размытие серого буфера: три прохода квадратом радиуса `r` (почти гаусс); за краем — край.
fn blur_gray(buf: &mut [f64], bw: usize, bh: usize, r: usize) {
    if r == 0 || buf.is_empty() {
        return;
    }
    let mut tmp = vec![0f64; buf.len()];
    for _ in 0..3 {
        for horizontal in [true, false] {
            let (lines, len) = if horizontal { (bh, bw) } else { (bw, bh) };
            for line in 0..lines {
                let at = |i: isize| {
                    let i = i.clamp(0, len as isize - 1) as usize;
                    if horizontal {
                        line * bw + i
                    } else {
                        i * bw + line
                    }
                };
                let r = r as isize;
                let mut acc: f64 = (-r..=r).map(|k| buf[at(k)]).sum();
                for i in 0..len as isize {
                    tmp[at(i)] = acc / (2 * r + 1) as f64;
                    acc += buf[at(i + r + 1)] - buf[at(i - r)];
                }
            }
            buf.copy_from_slice(&tmp);
        }
    }
}

fn luma(c: [f64; 3]) -> f64 {
    0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2]
}

/// «Карандашный рисунок» (своя версия; у CSP способ закрыт).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pencil {
    /// Толщина линий, px (радиус размытия для осветления основы).
    pub thickness: f64,
    /// Темнота линий 0…1.
    pub darkness: f64,
    /// Штриховка тёмных мест 0…1.
    pub hatching: f64,
    /// Оставить цвет (иначе серый карандаш).
    pub color: bool,
}

/// Карандашный рисунок: серое поверх белого делится на своё размытие (осветление основы) —
/// ровные места белеют, края остаются линиями; темнота — степень, штриховка — шум, вытянутый по
/// диагонали, в тёмных местах. Альфа слоя сохраняется.
pub fn pencil_drawing(src: &ColorMap, w: u32, h: u32, p: Pencil, sel: &(dyn Fn(i32, i32) -> u32 + Sync)) -> ColorMap {
    let r = p.thickness.round().max(1.0) as usize;
    let Some((x0, y0, bw, bh, buf)) = dense(src, w, h, 3 * r as i32) else { return src.clone() };
    let white: Vec<[f64; 3]> = buf.iter().map(|q| [0, 1, 2].map(|i| q[i] + 1.0 - q[3])).collect();
    let gray: Vec<f64> = white.iter().map(|c| luma(*c)).collect();
    let mut soft = gray.clone();
    blur_gray(&mut soft, bw, bh, r);
    let gamma = 1.0 + 3.0 * p.darkness;
    let res: Vec<[f64; 4]> = (0..buf.len())
        .map(|i| {
            let mut s = (gray[i] / soft[i].max(1e-4)).min(1.0).powf(gamma);
            if p.hatching > 0.0 {
                let (x, y) = (x0 + (i % bw) as i32, y0 + (i / bw) as i32);
                let n = (0..12).map(|k| hash01(x - k, y + k, 7)).sum::<f64>() / 12.0;
                let streak = ((n - 0.5) * 5.0 + 0.5).clamp(0.0, 1.0);
                s *= 1.0 - p.hatching * (1.0 - soft[i]) * streak;
            }
            let a = buf[i][3];
            let c = if p.color { white[i].map(|v| v * s) } else { [s; 3] };
            [c[0] * a, c[1] * a, c[2] * a, a]
        })
        .collect();
    write_back(src, x0, y0, bw, &res, sel)
}

/// «Художественный» (своя версия): сглаживание Кувахары, плакатные уровни и тёмные контуры.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Artistic {
    /// Радиус сглаживания (мазка), px.
    pub radius: u32,
    /// Уровней на канал (2–16), 0 — без плаката.
    pub levels: u32,
    /// Контуры 0…1 (0 — без контуров).
    pub edges: f64,
}

/// Художественный: у каждой точки из четырёх квадратов `(r+1)²` вокруг берётся средний цвет
/// самого ровного (наименьший разброс яркости) — мазки с чёткими краями; затем уровни яркости и
/// контур по Собелю на яркости.
pub fn artistic(src: &ColorMap, w: u32, h: u32, a: Artistic, sel: &(dyn Fn(i32, i32) -> u32 + Sync)) -> ColorMap {
    let r = a.radius.max(1) as isize;
    let Some((x0, y0, bw, bh, buf)) = dense(src, w, h, r as i32 + 2) else { return src.clone() };
    let sw = bw + 1;
    let mut sat = vec![[0f64; 6]; sw * (bh + 1)];
    for y in 0..bh {
        for x in 0..bw {
            let q = buf[y * bw + x];
            let l = luma([q[0], q[1], q[2]]);
            let v = [q[0], q[1], q[2], q[3], l, l * l];
            for c in 0..6 {
                sat[(y + 1) * sw + x + 1][c] = v[c] + sat[y * sw + x + 1][c] + sat[(y + 1) * sw + x][c] - sat[y * sw + x][c];
            }
        }
    }
    let smooth: Vec<[f64; 4]> = (0..bw * bh)
        .map(|i| {
            let (x, y) = ((i % bw) as isize, (i / bw) as isize);
            let mut best = (f64::MAX, buf[i]);
            for (dx, dy) in [(-r, -r), (0, -r), (-r, 0), (0, 0)] {
                let (xa, ya) = ((x + dx).max(0) as usize, (y + dy).max(0) as usize);
                let (xb, yb) = ((x + dx + r + 1).min(bw as isize) as usize, (y + dy + r + 1).min(bh as isize) as usize);
                let n = ((xb - xa) * (yb - ya)) as f64;
                let s: [f64; 6] = std::array::from_fn(|c| sat[yb * sw + xb][c] - sat[ya * sw + xb][c] - sat[yb * sw + xa][c] + sat[ya * sw + xa][c]);
                let mean = s[4] / n;
                let var = s[5] / n - mean * mean;
                if var < best.0 {
                    best = (var, [s[0] / n, s[1] / n, s[2] / n, s[3] / n]);
                }
            }
            best.1
        })
        .collect();
    let lum: Vec<f64> = smooth.iter().map(|q| luma([0, 1, 2].map(|i| q[i] + 1.0 - q[3]))).collect();
    let at = |x: isize, y: isize| lum[y.clamp(0, bh as isize - 1) as usize * bw + x.clamp(0, bw as isize - 1) as usize];
    let res: Vec<[f64; 4]> = par_rows(bw, bh, |i| {
            let q = smooth[i];
            if q[3] <= 0.0 {
                return q;
            }
            let mut c = [0, 1, 2].map(|k| (q[k] / q[3]).clamp(0.0, 1.0));
            // уровни — по яркости, оттенок остаётся (поканально цвета уходят в розовое и зелёное)
            if a.levels >= 2 {
                let n = (a.levels - 1) as f64;
                let l = luma(c);
                let lq = (l * n).round() / n;
                c = if l > 1e-6 { c.map(|v| (v * lq / l).clamp(0.0, 1.0)) } else { [lq; 3] };
            }
            if a.edges > 0.0 {
                let (x, y) = ((i % bw) as isize, (i / bw) as isize);
                let gx = at(x + 1, y - 1) + 2.0 * at(x + 1, y) + at(x + 1, y + 1) - at(x - 1, y - 1) - 2.0 * at(x - 1, y) - at(x - 1, y + 1);
                let gy = at(x - 1, y + 1) + 2.0 * at(x, y + 1) + at(x + 1, y + 1) - at(x - 1, y - 1) - 2.0 * at(x, y - 1) - at(x + 1, y - 1);
                let mag = gx.hypot(gy) / 4.0;
                let line = ((mag * (1.0 + 4.0 * a.edges) - 0.15) * 3.0).clamp(0.0, 1.0);
                c = c.map(|v| v * (1.0 - 0.85 * line));
            }
            [c[0] * q[3], c[1] * q[3], c[2] * q[3], q[3]]
        });
    write_back(src, x0, y0, bw, &res, sel)
}

#[cfg(test)]
mod stylize_tests {
    use super::*;
    use crate::pixel::{from_rgba8, to_rgba8};

    #[test]
    fn pencil_whitens_flat_areas_and_artistic_posterizes() {
        let all = |_: i32, _: i32| ONE;
        let mut m = ColorMap::new();
        for y in 0..60 {
            for x in 0..60 {
                let c = if x < 30 { [40, 60, 200, 255] } else { [(x * 3) as u8, 180, 90, 255] };
                m.set(x, y, from_rgba8(c));
            }
        }
        let px = |c: &ColorMap, x, y| to_rgba8(c.get(x, y));
        // карандаш: ровное место белое, край тёмный, цвет — серый
        let p = pencil_drawing(&m, 60, 60, Pencil { thickness: 3.0, darkness: 0.5, hatching: 0.0, color: false }, &all);
        assert!(px(&p, 10, 30)[0] > 245);
        assert!(px(&p, 29, 30)[0] < 200, "{:?}", px(&p, 29, 30));
        let e = px(&p, 29, 30);
        assert!(e[0] == e[1] && e[1] == e[2]);
        // художественный: уровней 2 — яркость 0 или 1 (синее уходит в чёрное); край — темнее
        let a = artistic(&m, 60, 60, Artistic { radius: 3, levels: 2, edges: 0.0 }, &all);
        assert_eq!(px(&a, 10, 10)[..3], [0, 0, 0]);
        let ae = artistic(&m, 60, 60, Artistic { radius: 3, levels: 0, edges: 1.0 }, &all);
        assert!(px(&ae, 30, 30)[2] < px(&m, 31, 30)[2].max(px(&m, 29, 30)[2]) / 2 + 20);
        assert_eq!(px(&ae, 10, 10), px(&m, 10, 10));
    }
}

#[cfg(test)]
mod distort_tests {
    use super::*;
    use crate::pixel::{from_rgba8, to_rgba8};

    #[test]
    fn distortions_move_points_and_keep_identity_at_zero() {
        let all = |_: i32, _: i32| ONE;
        let mut m = ColorMap::new();
        for y in 0..100 {
            for x in 0..100 {
                m.set(x, y, from_rgba8([(x * 2) as u8, (y * 2) as u8, 100, 255]));
            }
        }
        let base = Distort { kind: DistortKind::Pinch, center: (50.0, 50.0), radius: 40.0, amount: 0.0, amplitude: 0.0, wavelength: 20.0, angle: 0.0, shape: 0, to_polar: true };
        let px = |c: &ColorMap, x, y| to_rgba8(c.get(x, y));
        let close = |a: [u8; 4], b: [u8; 4], tol: i32| a.iter().zip(b).all(|(p, q)| (*p as i32 - q as i32).abs() <= tol);
        // нулевая сила — ничего не меняется
        for kind in [DistortKind::Pinch, DistortKind::Fisheye, DistortKind::Twirl, DistortKind::Ripple, DistortKind::Wave] {
            let out = distort(&m, 100, 100, &Distort { kind, ..base }, &all);
            assert!(close(px(&out, 37, 61), px(&m, 37, 61), 1), "{kind:?}");
        }
        // щипок к середине: точка в круге берёт цвет дальше от середины; вне круга — без изменений
        let p = distort(&m, 100, 100, &Distort { amount: 1.0, ..base }, &all);
        assert!(px(&p, 60, 50)[0] > px(&m, 60, 50)[0] + 4);
        assert_eq!(px(&p, 95, 5), px(&m, 95, 5));
        // «рыбий глаз» увеличивает середину: цвет ближе к середине
        let f = distort(&m, 100, 100, &Distort { kind: DistortKind::Fisheye, amount: 0.5, ..base }, &all);
        assert!(px(&f, 70, 50)[0] < px(&m, 70, 50)[0] - 4);
        // волна вдоль x: сдвиг по x меняется с y
        let wv = distort(&m, 100, 100, &Distort { kind: DistortKind::Wave, amplitude: 5.0, ..base }, &all);
        assert_ne!(px(&wv, 50, 5)[0], px(&m, 50, 5)[0]);
        assert_eq!(px(&wv, 50, 5)[1], px(&m, 50, 5)[1]);
        // полярные туда и обратно — почти исходное (вдали от середины круга)
        let pol = distort(&m, 100, 100, &Distort { kind: DistortKind::Polar, ..base }, &all);
        let back = distort(&pol, 100, 100, &Distort { kind: DistortKind::Polar, to_polar: false, ..base }, &all);
        assert!(close(px(&back, 50, 30), px(&m, 50, 30), 8), "{:?} {:?}", px(&back, 50, 30), px(&m, 50, 30));
    }
}

#[cfg(test)]
mod blur_tests {
    use super::*;
    use crate::pixel::{from_rgba8, to_rgba8};

    #[test]
    fn blurs_spread_and_keep_flat_areas() {
        let all = |_: i32, _: i32| ONE;
        let mut m = ColorMap::new();
        for y in 0..60 {
            for x in 0..60 {
                let v = if x >= 30 { 220 } else { 30 };
                m.set(x, y, from_rgba8([v, v, v, 255]));
            }
        }
        let g = |c: &ColorMap, x, y| to_rgba8(c.get(x, y))[0] as i32;
        // движение по горизонтали размывает край, по вертикали — нет
        let st = |angle| Streak { strength: 10.0, angle, dir: 0, smooth: false };
        let mh = motion_blur(&m, 60, 60, st(0.0), &all);
        let mv = motion_blur(&m, 60, 60, st(90.0), &all);
        assert!(g(&mh, 30, 30) < 200 && g(&mh, 30, 30) > 60);
        assert_eq!(g(&mv, 30, 30), 220);
        // среднее — один цвет
        let av = average(&m, 60, 60, &all);
        assert_eq!(g(&av, 2, 2), g(&av, 58, 58));
        // линза и квадрат: край размыт, середина половин прежняя
        for out in [lens_blur(&m, 60, 60, 5.0, 0, 0.0, &all), lens_blur(&m, 60, 60, 5.0, 6, 0.5, &all), box_blur(&m, 60, 60, 2, &all)] {
            assert!(g(&out, 29, 30) > 30 && g(&out, 30, 30) < 220);
            assert_eq!((g(&out, 5, 30), g(&out, 55, 30)), (30, 220));
        }
        // сглаживание трогает только край
        let sm = smoothing(&m, 60, 60, &all);
        assert_eq!(g(&sm, 10, 10), 30);
        assert!(g(&sm, 30, 10) < 220);
        let _ = (radial_blur(&m, 60, 60, (30.0, 30.0), Streak { strength: 0.2, angle: 0.0, dir: 0, smooth: true }, &all), spin_blur(&m, 60, 60, (30.0, 30.0), 20.0, 0, &all));
    }
}

#[cfg(test)]
mod levels_median_tests {
    use super::*;
    use crate::pixel::{from_rgba8, to_rgba8};

    #[test]
    fn levels_default_is_identity_and_stretches() {
        let l = Levels::default();
        for v in [0, 37, 128, 255] {
            let p = from_rgba8([v, v, v, 255]);
            assert_eq!(to_rgba8(l.apply(p)), to_rgba8(p));
        }
        let l = Levels { in_black: 50, in_white: 200, ..Default::default() };
        assert_eq!(to_rgba8(l.apply(from_rgba8([50, 125, 200, 255])))[..3], [0, 128, 255]);
        let l = Levels { out_black: 100, out_white: 150, ..Default::default() };
        assert_eq!(to_rgba8(l.apply(from_rgba8([0, 255, 0, 255])))[..2], [100, 150]);
    }

    #[test]
    fn median_removes_a_speck_and_keeps_an_edge() {
        let (w, h) = (20u32, 20u32);
        let mut m = ColorMap::new();
        for y in 0..20 {
            for x in 0..20 {
                let v = if x < 10 { 0 } else { 255 };
                m.set(x, y, from_rgba8([v, v, v, 255]));
            }
        }
        m.set(4, 4, from_rgba8([255, 255, 255, 255]));
        let out = median(&m, w, h, 1, &|_, _| ONE);
        assert_eq!(to_rgba8(out.get(4, 4))[0], 0, "точка убрана");
        assert_eq!((to_rgba8(out.get(9, 10))[0], to_rgba8(out.get(10, 10))[0]), (0, 255), "край на месте");
    }
}
