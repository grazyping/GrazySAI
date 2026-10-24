//! Стили слоя (как «Свойство слоя» Clip Studio Paint и «Стили слоя» Photoshop): обводка снаружи,
//! тень, внешнее свечение, наложение цвета. Пиксели слоя не меняются — стиль считается при
//! сведении. Только у обычных слоёв (не у папок).

use crate::layer::Layer;
use crate::tile::{Key, TILE};

/// Дальше стольких точек от рисунка стиль не достаёт.
pub const MAX_REACH: i32 = 128;

/// Цвет и непрозрачность (0–100).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Paint {
    pub color: [u8; 3],
    pub opacity: u8,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Styles {
    /// Обводка снаружи: толщина 1–64.
    pub stroke: Option<(u16, Paint)>,
    /// Тень: сдвиг, размытие 0–64.
    pub shadow: Option<(i16, i16, u16, Paint)>,
    /// Внешнее свечение: размер 1–64.
    pub glow: Option<(u16, Paint)>,
    /// Наложение цвета на рисунок.
    pub overlay: Option<Paint>,
}

impl Styles {
    pub fn is_empty(&self) -> bool {
        self.stroke.is_none() && self.shadow.is_none() && self.glow.is_none() && self.overlay.is_none()
    }

    /// На сколько точек стиль выходит за рисунок.
    pub fn reach(&self) -> i32 {
        let mut r = 0;
        if let Some((w, _)) = self.stroke {
            r = r.max(w as i32 + 1);
        }
        if let Some((dx, dy, b, _)) = self.shadow {
            r = r.max((dx as i32).abs().max((dy as i32).abs()) + b as i32 + 1);
        }
        if let Some((s, _)) = self.glow {
            r = r.max(s as i32 + 1);
        }
        r.min(MAX_REACH)
    }

    /// Строки для блока файла.
    pub fn to_text(&self) -> String {
        let p = |p: &Paint| format!("{} {} {} {}", p.color[0], p.color[1], p.color[2], p.opacity);
        let mut s = String::new();
        if let Some((w, c)) = &self.stroke {
            s += &format!("stroke {w} {}\n", p(c));
        }
        if let Some((dx, dy, b, c)) = &self.shadow {
            s += &format!("shadow {dx} {dy} {b} {}\n", p(c));
        }
        if let Some((w, c)) = &self.glow {
            s += &format!("glow {w} {}\n", p(c));
        }
        if let Some(c) = &self.overlay {
            s += &format!("overlay {}\n", p(c));
        }
        s
    }

    pub fn from_text(s: &str) -> Styles {
        let mut out = Styles::default();
        for line in s.lines() {
            let mut it = line.split_ascii_whitespace();
            let Some(kind) = it.next() else { continue };
            let n: Vec<i32> = it.filter_map(|v| v.parse().ok()).collect();
            let paint = |k: usize| -> Option<Paint> { Some(Paint { color: [n.get(k)?, n.get(k + 1)?, n.get(k + 2)?].map(|v| (*v).clamp(0, 255) as u8), opacity: (*n.get(k + 3)?).clamp(0, 100) as u8 }) };
            let size = |k: usize| n.get(k).map(|v| (*v).clamp(1, 64) as u16);
            match kind {
                "stroke" => out.stroke = size(0).zip(paint(1)),
                "shadow" => {
                    out.shadow = (|| Some(((*n.first()?).clamp(-64, 64) as i16, (*n.get(1)?).clamp(-64, 64) as i16, (*n.get(2)?).clamp(0, 64) as u16, paint(3)?)))();
                }
                "glow" => out.glow = size(0).zip(paint(1)),
                "overlay" => out.overlay = paint(0),
                _ => {}
            }
        }
        out
    }
}

/// Непрозрачность слоя в квадрате `x0..x0+w`, `y0..y0+w` (0..1) — по плиткам, без поиска на
/// каждую точку.
fn alpha_region(layer: &Layer, x0: i32, y0: i32, w: i32) -> Vec<f32> {
    let mut a = vec![0f32; (w * w) as usize];
    let k = 1.0 / crate::pixel::ONE as f32;
    for ty in y0.div_euclid(TILE)..=(y0 + w - 1).div_euclid(TILE) {
        for tx in x0.div_euclid(TILE)..=(x0 + w - 1).div_euclid(TILE) {
            let Some(t) = layer.pixels.tiles.get(&(tx, ty)) else { continue };
            for yy in 0..TILE {
                let y = ty * TILE + yy - y0;
                if y < 0 || y >= w {
                    continue;
                }
                for xx in 0..TILE {
                    let x = tx * TILE + xx - x0;
                    if x < 0 || x >= w {
                        continue;
                    }
                    a[(y * w + x) as usize] = t.0[(yy * TILE + xx) as usize][3] as f32 * k;
                }
            }
        }
    }
    a
}

/// Размытие квадрата `w`×`w` тремя проходами среднего по `r` (почти по Гауссу).
fn blur(src: &[f32], w: usize, r: usize) -> Vec<f32> {
    if r == 0 {
        return src.to_vec();
    }
    let mut a = src.to_vec();
    let mut tmp = vec![0f32; a.len()];
    let k = 1.0 / (2 * r + 1) as f32;
    let pass = |from: &[f32], to: &mut [f32], horizontal: bool| {
        for line in 0..w {
            let at = |i: usize| if horizontal { line * w + i } else { i * w + line };
            let mut sum = 0f32;
            for i in 0..=r.min(w - 1) {
                sum += from[at(i)];
            }
            for i in 0..w {
                to[at(i)] = sum * k;
                if i + r + 1 < w {
                    sum += from[at(i + r + 1)];
                }
                if i >= r {
                    sum -= from[at(i - r)];
                }
            }
        }
    };
    for _ in 0..3 {
        pass(&a, &mut tmp, true);
        pass(&tmp, &mut a, false);
    }
    a
}

/// Квадрат расстояния до ближайшей точки рисунка (`inside`) — точный (Felzenszwalb).
fn distance2(inside: &[bool], w: usize) -> Vec<f32> {
    const INF: f32 = 1e12;
    let mut d: Vec<f32> = inside.iter().map(|&b| if b { 0.0 } else { INF }).collect();
    let mut f = vec![0f32; w];
    let mut out = vec![0f32; w];
    let mut v = vec![0usize; w];
    let mut z = vec![0f32; w + 1];
    let mut line = |vals: &mut dyn FnMut(usize, Option<f32>) -> f32| {
        for (i, x) in f.iter_mut().enumerate() {
            *x = vals(i, None);
        }
        let mut k = 0;
        v[0] = 0;
        z[0] = -INF;
        z[1] = INF;
        for q in 1..w {
            loop {
                let p = v[k];
                let s = ((f[q] + (q * q) as f32) - (f[p] + (p * p) as f32)) / (2.0 * q as f32 - 2.0 * p as f32);
                if s <= z[k] && k > 0 {
                    k -= 1;
                    continue;
                }
                if s <= z[k] {
                    // k == 0: заменить первую параболу
                    v[0] = q;
                    z[0] = -INF;
                    z[1] = INF;
                    break;
                }
                k += 1;
                v[k] = q;
                z[k] = s;
                z[k + 1] = INF;
                break;
            }
        }
        k = 0;
        for (q, o) in out.iter_mut().enumerate() {
            while z[k + 1] < q as f32 {
                k += 1;
            }
            let p = v[k];
            *o = (q as f32 - p as f32).powi(2) + f[p];
        }
        for (i, o) in out.iter().enumerate() {
            vals(i, Some(*o));
        }
    };
    for x in 0..w {
        line(&mut |y, set| {
            if let Some(v) = set {
                d[y * w + x] = v;
            }
            d[y * w + x]
        });
    }
    for y in 0..w {
        line(&mut |x, set| {
            if let Some(v) = set {
                d[y * w + x] = v;
            }
            d[y * w + x]
        });
    }
    d
}

/// Наложение цвета на рисунок слоя (до прикреплённых слоёв).
pub fn overlay(layer: &Layer, buf: &mut [[f32; 4]]) {
    let Some(o) = layer.styles.as_deref().and_then(|s| s.overlay) else { return };
    if layer.children.is_some() {
        return;
    }
    let k = o.opacity.min(100) as f32 / 100.0;
    let c = o.color.map(|v| v as f32 / 255.0);
    for px in buf.iter_mut() {
        for i in 0..3 {
            px[i] += (c[i] * px[3] - px[i]) * k;
        }
    }
}

/// Обводка, тень, свечение на плитку `buf` (предумноженные точки в долях единицы) — под
/// рисунком.
pub fn outer(layer: &Layer, key: Key, buf: &mut [[f32; 4]]) {
    let Some(st) = layer.styles.as_deref() else { return };
    if layer.children.is_some() {
        return;
    }
    let r = st.reach();
    if r == 0 {
        return;
    }
    let w = (TILE + 2 * r) as usize;
    let (x0, y0) = (key.0 * TILE - r, key.1 * TILE - r);
    let alpha = alpha_region(layer, x0, y0, w as i32);
    if alpha.iter().all(|&a| a <= 0.0) {
        return;
    }
    // слои под рисунком: (покрытие в квадрате, цвет)
    let mut under: Vec<(Vec<f32>, Paint)> = Vec::new();
    if let Some((dx, dy, b, p)) = st.shadow {
        let mut shifted = vec![0f32; w * w];
        for y in 0..w as i32 {
            for x in 0..w as i32 {
                let (sx, sy) = (x - dx as i32, y - dy as i32);
                if sx >= 0 && sy >= 0 && sx < w as i32 && sy < w as i32 {
                    shifted[(y * w as i32 + x) as usize] = alpha[(sy * w as i32 + sx) as usize];
                }
            }
        }
        under.push((blur(&shifted, w, (b as usize).div_ceil(3)), p));
    }
    if let Some((s, p)) = st.glow {
        let g = blur(&alpha, w, (s as usize).div_ceil(3));
        under.push((g.iter().map(|v| (v * 2.0).min(1.0)).collect(), p));
    }
    if let Some((s, p)) = st.stroke {
        let inside: Vec<bool> = alpha.iter().map(|&a| a >= 0.5).collect();
        let d2 = distance2(&inside, w);
        under.push((d2.iter().map(|&d| (s as f32 + 0.5 - d.sqrt()).clamp(0.0, 1.0)).collect(), p));
    }
    let off = r as usize;
    for y in 0..TILE as usize {
        for x in 0..TILE as usize {
            let i = (y + off) * w + x + off;
            let mut base = [0f32; 4];
            for (cov, p) in &under {
                let a = cov[i] * p.opacity.min(100) as f32 / 100.0;
                if a <= 0.0 {
                    continue;
                }
                let c = p.color.map(|v| v as f32 / 255.0);
                for ch in 0..3 {
                    base[ch] = c[ch] * a + base[ch] * (1.0 - a);
                }
                base[3] = a + base[3] * (1.0 - a);
            }
            let px = &mut buf[y * TILE as usize + x];
            let ka = 1.0 - px[3];
            for ch in 0..4 {
                px[ch] += base[ch] * ka;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_round_trip() {
        let s = Styles {
            stroke: Some((3, Paint { color: [255, 0, 0], opacity: 100 })),
            shadow: Some((4, -5, 6, Paint { color: [0, 0, 0], opacity: 50 })),
            glow: Some((8, Paint { color: [255, 255, 0], opacity: 70 })),
            overlay: Some(Paint { color: [0, 0, 255], opacity: 30 }),
        };
        assert_eq!(Styles::from_text(&s.to_text()), s);
        assert_eq!(s.reach(), 12);
    }

    #[test]
    fn distance_is_exact() {
        let w = 9;
        let mut inside = vec![false; w * w];
        inside[4 * w + 4] = true;
        let d = distance2(&inside, w);
        assert_eq!(d[4 * w + 4], 0.0);
        assert_eq!(d[4 * w + 7], 9.0);
        assert_eq!(d[7 * w + 8], 25.0);
    }
}
