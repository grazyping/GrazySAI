//! Слои коррекции (Clip Studio Paint, Photoshop): параметры вместо пикселей; меняют цвет всего,
//! что ниже в своей папке (с обтравкой — только слой под собой). Цвет — прямой (не
//! предумноженный), альфа не меняется; маска, непрозрачность и режим — как у обычного слоя.

use crate::filters::{BrightnessContrast, HueSaturation, Levels};
use crate::pixel::Px;

/// Узел карты градиента: место 0..=1 (от тёмного к светлому) и цвет.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stop {
    pub at: f32,
    pub color: [u8; 3],
}

#[derive(Clone, Debug, PartialEq)]
pub enum Adjust {
    /// Карта градиента: яркость → цвет по узлам.
    GradientMap(Vec<Stop>),
    Levels(Levels),
    HueSaturation(HueSaturation),
    BrightnessContrast(BrightnessContrast),
    /// Инверсия.
    Invert,
    /// Постеризация: уровней на канал (2–64).
    Posterize(u8),
    /// Порог (двоичный): яркость выше — белое, ниже — чёрное (1–255).
    Threshold(u8),
}

impl Adjust {
    /// Карта градиента по умолчанию — от чёрного к белому.
    pub fn gradient_default() -> Adjust {
        Adjust::GradientMap(vec![Stop { at: 0.0, color: [0, 0, 0] }, Stop { at: 1.0, color: [255, 255, 255] }])
    }

    /// Цвет точки градиента на месте `t` 0..=1.
    pub fn gradient_at(stops: &[Stop], t: f32) -> [f32; 3] {
        let f = |c: [u8; 3]| c.map(|v| v as f32 / 255.0);
        match stops {
            [] => [t; 3],
            [s] => f(s.color),
            _ => {
                let k = stops.iter().position(|s| s.at >= t).unwrap_or(stops.len());
                if k == 0 {
                    return f(stops[0].color);
                }
                if k == stops.len() {
                    return f(stops[k - 1].color);
                }
                let (a, b) = (stops[k - 1], stops[k]);
                let u = if b.at > a.at { (t - a.at) / (b.at - a.at) } else { 1.0 };
                let (ca, cb) = (f(a.color), f(b.color));
                [ca[0] + (cb[0] - ca[0]) * u, ca[1] + (cb[1] - ca[1]) * u, ca[2] + (cb[2] - ca[2]) * u]
            }
        }
    }

    /// Точка (предумноженная, единица `ONE`) после коррекции.
    pub fn apply(&self, p: Px) -> Px {
        if p[3] == 0 {
            return p;
        }
        let straight = |p: Px| {
            let a = p[3] as f32;
            ([p[0] as f32 / a, p[1] as f32 / a, p[2] as f32 / a], a)
        };
        let pack = |c: [f32; 3], a: f32| -> Px {
            let q = |v: f32| (v.clamp(0.0, 1.0) * a + 0.5) as u16;
            [q(c[0]), q(c[1]), q(c[2]), a as u16]
        };
        match self {
            Adjust::Levels(l) => l.apply(p),
            Adjust::HueSaturation(h) => h.apply(p),
            Adjust::BrightnessContrast(b) => b.apply(p),
            Adjust::GradientMap(stops) => {
                let (c, a) = straight(p);
                let luma = c[0] * 0.299 + c[1] * 0.587 + c[2] * 0.114;
                pack(Self::gradient_at(stops, luma.clamp(0.0, 1.0)), a)
            }
            Adjust::Invert => {
                let (c, a) = straight(p);
                pack(c.map(|v| 1.0 - v), a)
            }
            Adjust::Posterize(n) => {
                let n = (*n).clamp(2, 64) as f32 - 1.0;
                let (c, a) = straight(p);
                pack(c.map(|v| (v * n).round() / n), a)
            }
            Adjust::Threshold(t) => {
                let (c, a) = straight(p);
                let luma = c[0] * 0.299 + c[1] * 0.587 + c[2] * 0.114;
                let v = if luma * 255.0 >= *t as f32 { 1.0 } else { 0.0 };
                pack([v; 3], a)
            }
        }
    }

    /// Текст для блока файла: вид и числа через пробел.
    pub fn to_text(&self) -> String {
        match self {
            Adjust::GradientMap(stops) => {
                let mut s = String::from("gradient");
                for st in stops {
                    s += &format!(" {} {} {} {}", st.at, st.color[0], st.color[1], st.color[2]);
                }
                s
            }
            Adjust::Levels(l) => format!("levels {} {} {} {} {}", l.in_black, l.in_white, l.gamma, l.out_black, l.out_white),
            Adjust::HueSaturation(h) => format!("hue {} {} {} {}", h.hue, h.saturation, h.luminance, h.colorize as u8),
            Adjust::BrightnessContrast(b) => format!("bright {} {} {}", b.brightness, b.contrast, b.depth),
            Adjust::Invert => "invert".into(),
            Adjust::Posterize(n) => format!("posterize {n}"),
            Adjust::Threshold(t) => format!("threshold {t}"),
        }
    }

    pub fn from_text(s: &str) -> Option<Adjust> {
        let mut it = s.split_ascii_whitespace();
        let kind = it.next()?;
        let nums: Vec<f64> = it.map(|v| v.parse::<f64>().ok().filter(|x| x.is_finite())).collect::<Option<_>>()?;
        let u8_at = |i: usize| nums.get(i).map(|v| v.clamp(0.0, 255.0) as u8);
        let i32_at = |i: usize, lim: i32| nums.get(i).map(|v| (*v as i32).clamp(-lim, lim));
        Some(match kind {
            "gradient" => {
                let mut stops: Vec<Stop> = nums
                    .chunks_exact(4)
                    .take(64)
                    .map(|c| Stop { at: (c[0] as f32).clamp(0.0, 1.0), color: [c[1], c[2], c[3]].map(|v| v.clamp(0.0, 255.0) as u8) })
                    .collect();
                stops.sort_by(|a, b| a.at.total_cmp(&b.at));
                Adjust::GradientMap(stops)
            }
            "levels" => Adjust::Levels(Levels { in_black: u8_at(0)?, in_white: u8_at(1)?, gamma: nums.get(2)?.clamp(0.1, 10.0), out_black: u8_at(3)?, out_white: u8_at(4)? }),
            "hue" => Adjust::HueSaturation(HueSaturation { hue: i32_at(0, 180)?, saturation: i32_at(1, 100)?, luminance: i32_at(2, 100)?, colorize: nums.get(3).is_some_and(|v| *v != 0.0) }),
            "bright" => Adjust::BrightnessContrast(BrightnessContrast { brightness: i32_at(0, 100)?, contrast: i32_at(1, 100)?, depth: i32_at(2, 100)? }),
            "invert" => Adjust::Invert,
            "posterize" => Adjust::Posterize(u8_at(0)?.clamp(2, 64)),
            "threshold" => Adjust::Threshold(u8_at(0)?.max(1)),
            _ => return None,
        })
    }
}

/// Коррекция плитки (предумноженные точки в долях единицы).
pub fn apply_buf(a: &Adjust, buf: &mut [[f32; 4]]) {
    for px in buf.iter_mut() {
        if px[3] > 0.0 {
            *px = crate::pixel::to_f32(a.apply(crate::pixel::from_f32(*px)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pixel::ONE;

    #[test]
    fn gradient_map_and_text_round_trip() {
        let g = Adjust::GradientMap(vec![Stop { at: 0.0, color: [0, 0, 255] }, Stop { at: 1.0, color: [255, 255, 0] }]);
        // белое → жёлтое, чёрное → синее, половина прозрачности сохраняется
        let half = ONE / 2;
        let w = g.apply([half as u16; 4]);
        assert_eq!((w[0], w[1], w[2], w[3]), (half as u16, half as u16, 0, half as u16));
        let k = g.apply([0, 0, 0, ONE as u16]);
        assert_eq!((k[0], k[2]), (0, ONE as u16));
        for a in [g, Adjust::Levels(Levels { in_black: 10, in_white: 200, gamma: 1.5, out_black: 5, out_white: 250 }), Adjust::Invert, Adjust::Posterize(4), Adjust::Threshold(128), Adjust::HueSaturation(HueSaturation { hue: 30, saturation: -20, luminance: 5, colorize: true }), Adjust::BrightnessContrast(BrightnessContrast { brightness: 10, contrast: -5, depth: 0 })] {
            assert_eq!(Adjust::from_text(&a.to_text()), Some(a));
        }
        assert_eq!(Adjust::from_text("levels 1 2"), None);
        assert_eq!(Adjust::from_text("что-то"), None);
    }
}
