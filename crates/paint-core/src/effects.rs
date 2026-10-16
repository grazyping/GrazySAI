//! Эффекты слоя SAI 2: «Кайма» (Water Fringe — краска густеет у края, как у акварели) и
//! «Двухцветный» (Binary Color — каждый канал либо полный, либо пустой). Устройство — по коду
//! SAI (`docs/25-layer-effects.md`). Накладываются на сам слой до бумаги.

use crate::layer::Layer;
use crate::paper::{dense_q14_fx, mult_q14, toward_q14, ver1_q14};
use crate::tile::{Key, TILE};

#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    /// Кайма: метка вида из файла (`watr`), ширина 1–15 точек, сила 0–100.
    Fringe { kind: [u8; 4], width: u16, strength: u16 },
    /// Двухцветный: порог 0–100; `rest` — второе число блока, как в файле.
    Binary { threshold: u16, rest: u32 },
}

impl Effect {
    /// Блок описания слоя в `.sai2`: метка и содержимое.
    pub fn to_block(&self) -> ([u8; 4], Vec<u8>) {
        match self {
            Effect::Fringe { kind, width, strength } => {
                let mut b = kind.to_vec();
                b.extend(width.to_le_bytes());
                b.extend(strength.to_le_bytes());
                (*b"frng", b)
            }
            Effect::Binary { threshold, rest } => {
                let mut b = b"cbin".to_vec();
                b.extend((*threshold as u32).to_le_bytes());
                b.extend(rest.to_le_bytes());
                (*b"ceff", b)
            }
        }
    }

    /// Из блока `.sai2` (`frng` или `ceff`); неизвестное — `None`.
    pub fn from_block(tag: &[u8; 4], b: &[u8]) -> Option<Effect> {
        let u16_at = |o: usize| Some(u16::from_le_bytes(b.get(o..o + 2)?.try_into().ok()?));
        let u32_at = |o: usize| Some(u32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?));
        match tag {
            b"frng" => {
                let kind: [u8; 4] = b.get(0..4)?.try_into().ok()?;
                if &kind != b"watr" && &kind != b"vivd" {
                    return None;
                }
                Some(Effect::Fringe { kind, width: u16_at(4)?.clamp(1, 15), strength: u16_at(6)?.min(100) })
            }
            b"ceff" if b.get(0..4)? == b"cbin" => {
                Some(Effect::Binary { threshold: u32_at(4)?.min(100) as u16, rest: u32_at(8).unwrap_or(0) })
            }
            _ => None,
        }
    }
}

/// Карта каймы для области `x0..x0+w`, `y0..y0+h`: насколько точка непрозрачнее окрестности
/// `(2r+1)²`. `alpha` — непрозрачность точки слоя, единица 16384.
pub fn fringe_map(alpha: impl Fn(i32, i32) -> i32, x0: i32, y0: i32, w: usize, h: usize, r: i32) -> Vec<i32> {
    let (gw, gh) = (w + 2 * r as usize, h + 2 * r as usize);
    // непрозрачность с прибавкой 1/8 у всех непустых точек
    let mut a = vec![0i32; gw * gh];
    for y in 0..gh {
        for x in 0..gw {
            let v = alpha(x0 - r + x as i32, y0 - r + y as i32);
            a[y * gw + x] = if v > 0 { v + 0x800 } else { 0 };
        }
    }
    let n = (2 * r + 1) as f32;
    let k = 1.0 / n;
    // сначала по вертикали, потом по горизонтали; каждое среднее с отбрасыванием дробной части
    let mut v = vec![0i32; gw * h];
    for x in 0..gw {
        let mut sum: i32 = (0..2 * r as usize).map(|j| a[j * gw + x]).sum();
        for y in 0..h {
            sum += a[(y + 2 * r as usize) * gw + x];
            v[y * gw + x] = (sum as f32 * k) as i32;
            sum -= a[y * gw + x];
        }
    }
    let mut out = vec![0i32; w * h];
    for y in 0..h {
        let row = &v[y * gw..(y + 1) * gw];
        let mut sum: i32 = row[..2 * r as usize].iter().sum();
        for x in 0..w {
            sum += row[x + 2 * r as usize];
            let blur = (sum as f32 * k) as i32;
            sum -= row[x];
            let center = a[(y + r as usize) * gw + x + r as usize];
            out[y * w + x] = (8 * (center - blur - 32)).clamp(0, 0x7fff);
        }
    }
    out
}

/// Кайма на пиксель `p` (единица 16384): `e` — значение карты, `w` — доля по карте и силе, `s` —
/// сила в единицах 16384; у каждого цвета эффектов своя функция (по коду SAI).
fn fringe_px(p: [i32; 4], e: i32, w: i32, s: i32, fx: u8) -> [i32; 4] {
    let one = 0x4000;
    // плавная доля «глубокого» и «умножения»: 1 − (1 − w/2)², в единицах 16384
    let soft = || {
        let x = 2 * (one - (w >> 1));
        one - ((x * x) >> 16)
    };
    match fx {
        // глубокий: цвет уходит от самого яркого канала, непрозрачность растёт, избыток снимается
        2 => {
            let kk = (2 * soft()).min(0x7fff);
            let mx = p[0].max(p[1]).max(p[2]);
            let m3 = kk.min(p[3]);
            let alpha = p[3] + m3;
            let ex = (alpha - one).max(0);
            let col = |c: i32| (c - (((mx - c) * kk) >> 14) + (m3 - (m3 >> 1)) - ex).clamp(0, one);
            [col(p[0]), col(p[1]), col(p[2]), alpha.clamp(0, one)]
        }
        // умножение: к цели «цвет на белом в квадрате» с долей 4·(1 − (1 − w/2)²)
        3 => {
            let k = (4 * soft()).min(0x7fff);
            let t = mult_q14(p);
            let mut out = p;
            for c in 0..4 {
                out[c] = (p[c] + (((t[c].clamp(0, one) - p[c]) * k) >> 14)).clamp(0, one);
            }
            out
        }
        // как в версии 1: сила без удвоения, шаг с отбрасыванием младших разрядов
        4 => {
            let w1 = (e * s) >> 16;
            let t = ver1_q14(p);
            let mut out = p;
            for c in 0..4 {
                let d = t[c].clamp(0, one) - p[c];
                out[c] = (p[c] + (((d * w1) >> 16) << 2)).max(0);
            }
            out
        }
        _ => toward_q14(p, dense_q14_fx(p, fx == 0), w),
    }
}

/// Эффект слоя на его плитку (предумноженные пиксели в долях единицы).
/// `size` — размер холста: за его краем кайма берёт крайние точки (края холста каймы не дают).
/// `fx` — цвет эффектов слоя холста (`paper::fx_of`): у каждого своя функция каймы, по коду SAI.
pub fn apply(layer: &Layer, key: Key, buf: &mut [[f32; 4]], size: (i32, i32), fx: u8) {
    let Some(effect) = &layer.effect else { return };
    if layer.children.is_some() {
        return;
    }
    let q = |v: f32| (v * 16384.0).round() as i32;
    match effect {
        Effect::Fringe { width, strength, .. } => {
            let s2 = 2 * (*strength.min(&100) as i32 * 0x4000 / 100);
            if s2 == 0 {
                return;
            }
            let r = (*width).clamp(1, 15) as i32;
            let clamp = |v: i32, n: i32| v.clamp(0, (n - 1).max(0));
            let alpha = |x: i32, y: i32| (layer.pixels.get(clamp(x, size.0), clamp(y, size.1))[3] as i32 + 1) >> 1;
            let map = fringe_map(alpha, key.0 * TILE, key.1 * TILE, TILE as usize, TILE as usize, r);
            for (px, &e) in buf.iter_mut().zip(&map) {
                if e == 0 || px[3] <= 0.0 {
                    continue;
                }
                let w = (e * s2) >> 16;
                let p = px.map(q);
                *px = fringe_px(p, e, w, s2 / 2, fx).map(|v| v as f32 / 16384.0);
            }
        }
        Effect::Binary { threshold, .. } => {
            let t = *threshold.min(&100) as i32 * 0x4000 / 100;
            for px in buf.iter_mut() {
                *px = px.map(|v| if q(v) > t { 1.0 } else { 0.0 });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_round_trip() {
        for e in [
            Effect::Fringe { kind: *b"watr", width: 7, strength: 40 },
            Effect::Binary { threshold: 55, rest: 3 },
        ] {
            let (tag, body) = e.to_block();
            assert_eq!(Effect::from_block(&tag, &body), Some(e));
        }
    }

    #[test]
    fn fringe_is_a_band_inside_the_edge() {
        // непрозрачный квадрат 40×40 в точках 20..60
        let alpha = |x: i32, y: i32| if (20..60).contains(&x) && (20..60).contains(&y) { 0x4000 } else { 0 };
        let m = fringe_map(alpha, 0, 0, 80, 80, 5);
        let at = |x: usize, y: usize| m[y * 80 + x];
        assert_eq!(at(10, 40), 0, "снаружи");
        assert_eq!(at(40, 40), 0, "в глубине");
        assert!(at(20, 40) > at(23, 40) && at(23, 40) > 0, "у края сильнее: {} {}", at(20, 40), at(23, 40));
        assert!(at(24, 24) > at(24, 40), "в углу сильнее: {} {}", at(24, 24), at(24, 40));
    }

    #[test]
    fn binary_thresholds_each_channel() {
        let mut l = Layer::new("x");
        l.effect = Some(Effect::Binary { threshold: 50, rest: 0 });
        let mut buf = vec![[0.3, 0.6, 0.0, 0.8], [0.0, 0.0, 0.0, 0.0]];
        apply(&l, (0, 0), &mut buf, (64, 64), 0);
        assert_eq!(buf[0], [0.0, 1.0, 0.0, 1.0]);
        assert_eq!(buf[1], [0.0; 4]);
    }
}
