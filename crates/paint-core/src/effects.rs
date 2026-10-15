//! Эффекты слоя SAI 2: «Кайма» (Water Fringe — краска густеет у края, как у акварели) и
//! «Двухцветный» (Binary Color — каждый канал либо полный, либо пустой). Устройство — по коду
//! SAI (`docs/25-layer-effects.md`). Накладываются на сам слой до бумаги.

use crate::layer::Layer;
use crate::paper::{dense_q14, toward_q14};
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

/// Эффект слоя на его плитку (предумноженные пиксели в долях единицы).
/// `size` — размер холста: за его краем кайма берёт крайние точки (края холста каймы не дают).
pub fn apply(layer: &Layer, key: Key, buf: &mut [[f32; 4]], size: (i32, i32)) {
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
                *px = toward_q14(p, dense_q14(p), w).map(|v| v as f32 / 16384.0);
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
        apply(&l, (0, 0), &mut buf, (64, 64));
        assert_eq!(buf[0], [0.0, 1.0, 0.0, 1.0]);
        assert_eq!(buf[1], [0.0; 4]);
    }
}
