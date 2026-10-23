//! Кисть-разброс (SAI 2 2018+): на каждый шаг мазка — несколько форм со случайным сдвигом,
//! размером, углом, толщиной и цветом. Правила сверены с живой SAI 2026-07-02b (docs/41):
//! - «Разброс» S — сдвиг до ±S·размер кисти поперёк мазка, равномерно; «Во все стороны» — ещё и
//!   вдоль, независимо (квадрат, не круг); «По Гауссу» — нормально, σ ≈ 0,44·S, обрезка ±1,3·S;
//! - «Разброс размера» J — множитель размера 1 − J·u (u равномерно в 0..1);
//! - «Число» N и «разброс числа» J — от N − J·(N−1) до N форм (при N = 1 всегда одна);
//! - разброс оттенка — ±J·180°, насыщенности и яркости — ±J (с обрезкой), «основной → фоновый» —
//!   доля фонового цвета до J; «к каждой форме» — свой цвет у каждой формы, иначе один на мазок.

use crate::dynamics::hash01;

/// Как поворачивать формы.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AngleControl {
    /// Только угол из настроек.
    None,
    /// «Авто»: по направлению мазка (наклона пера у нас нет).
    #[default]
    Auto,
    /// По направлению пера.
    Direction,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScatterParams {
    pub angle_control: AngleControl,
    /// Угол, радианы.
    pub angle: f32,
    /// Разброс угла 0..1 (1 — ±180°).
    pub angle_jitter: f32,
    /// Размер формы — доля размера кисти (1 — 100 %).
    pub scaling: f32,
    pub size_jitter: f32,
    /// Разброс сдвига — доля размера кисти, 0..2.
    pub scattering: f32,
    /// Отношение ширины к высоте формы: меньше 1 — уже, больше 1 — ниже.
    pub ratio: f32,
    pub ratio_jitter: f32,
    /// Форм за шаг, 1..20.
    pub count: u8,
    pub count_jitter: f32,
    pub all_directions: bool,
    pub gaussian: bool,
    /// Размер форм не зависит от нажима.
    pub absolute_size: bool,
    /// Формы — в целых точках холста.
    pub int_position: bool,
    /// Цвет: доля фонового цвета до этой величины.
    pub fg_bg_jitter: f32,
    pub hue_jitter: f32,
    pub sat_jitter: f32,
    pub bri_jitter: f32,
    /// Цвет — свой у каждой формы (иначе один на весь мазок).
    pub each_shape: bool,
}

impl Default for ScatterParams {
    fn default() -> Self {
        Self {
            angle_control: AngleControl::Auto,
            angle: 0.0,
            angle_jitter: 0.0,
            scaling: 1.0,
            size_jitter: 0.0,
            scattering: 0.0,
            ratio: 1.0,
            ratio_jitter: 0.0,
            count: 1,
            count_jitter: 0.0,
            all_directions: false,
            gaussian: false,
            absolute_size: false,
            int_position: false,
            fg_bg_jitter: 0.0,
            hue_jitter: 0.0,
            sat_jitter: 0.0,
            bri_jitter: 0.0,
            each_shape: false,
        }
    }
}

/// Одна форма шага: сдвиг (px), множитель размера, угол (радианы), толщина (≤ 1) и случайное
/// число для цвета.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shape {
    pub dx: f32,
    pub dy: f32,
    pub size: f32,
    pub angle: f32,
    pub ratio: f32,
    pub color_seed: u64,
}

impl ScatterParams {
    pub fn has_color_jitter(&self) -> bool {
        self.fg_bg_jitter > 0.0 || self.hue_jitter > 0.0 || self.sat_jitter > 0.0 || self.bri_jitter > 0.0
    }

    /// Формы шага номер `n` мазка с зерном `seed`: размер кисти `diameter`, направление `heading`.
    pub fn shapes(&self, seed: u64, n: u64, diameter: f32, heading: f32) -> Vec<Shape> {
        let base = seed ^ n.wrapping_mul(0x2545_f491_4f6c_dd1d);
        let mut k = 0u64;
        let mut rnd = || {
            k += 1;
            hash01(base.wrapping_add(k.wrapping_mul(0x9e37_79b9)))
        };
        let count = self.count.clamp(1, 20) as f32;
        let drop = (rnd() * self.count_jitter.clamp(0.0, 1.0) * (count - 1.0)).round();
        let count = (count - drop).max(1.0) as usize;
        let reach = self.scattering.clamp(0.0, 2.0) * diameter;
        let (s, c) = heading.sin_cos();
        let mut out = Vec::with_capacity(count);
        for _ in 0..count {
            let off = |r: &mut dyn FnMut() -> f32| -> f32 {
                if reach <= 0.0 {
                    return 0.0;
                }
                if self.gaussian {
                    // Бокс — Мюллер; обрезка, чтобы форма не улетала далеко
                    let (u1, u2) = (r().max(1e-6), r());
                    let g = (-2.0 * u1.ln()).sqrt() * (std::f32::consts::TAU * u2).cos();
                    (g * 0.44).clamp(-1.3, 1.3) * reach
                } else {
                    (r() * 2.0 - 1.0) * reach
                }
            };
            let across = off(&mut rnd);
            let along = if self.all_directions { off(&mut rnd) } else { 0.0 };
            let size = 1.0 - self.size_jitter.clamp(0.0, 1.0) * rnd();
            let turn = match self.angle_control {
                AngleControl::None => 0.0,
                AngleControl::Auto | AngleControl::Direction => heading,
            };
            let angle = self.angle + turn + (rnd() * 2.0 - 1.0) * self.angle_jitter.clamp(0.0, 1.0) * std::f32::consts::PI;
            let r = self.ratio.clamp(0.01, 100.0) * (1.0 - self.ratio_jitter.clamp(0.0, 1.0) * rnd());
            // ширина : высота — сжатие по одной оси; у вытянутой поперёк — поворот на 90°
            let (ratio, angle) = if r <= 1.0 { (r, angle) } else { (1.0 / r, angle + std::f32::consts::FRAC_PI_2) };
            let color_seed = (rnd().to_bits() as u64) << 32 | rnd().to_bits() as u64;
            out.push(Shape { dx: c * along - s * across, dy: s * along + c * across, size, angle, ratio, color_seed });
        }
        out
    }

    /// Цвет формы (или мазка) с разбросом: основной `fg`, фоновый `bg`, зерно `seed`.
    pub fn color(&self, fg: [u8; 3], bg: [u8; 3], seed: u64) -> [u8; 3] {
        if !self.has_color_jitter() {
            return fg;
        }
        let r = |i: u64| hash01(seed.wrapping_add(i.wrapping_mul(0x9e37_79b9_7f4a_7c15)));
        let t = r(1) * self.fg_bg_jitter.clamp(0.0, 1.0);
        let mixed: [f32; 3] = std::array::from_fn(|i| (fg[i] as f32 * (1.0 - t) + bg[i] as f32 * t) / 255.0);
        let (mut h, mut s, mut v) = rgb_to_hsv(mixed);
        h = (h + (r(2) * 2.0 - 1.0) * self.hue_jitter.clamp(0.0, 1.0) * 0.5).rem_euclid(1.0);
        s = (s + (r(3) * 2.0 - 1.0) * self.sat_jitter.clamp(0.0, 1.0)).clamp(0.0, 1.0);
        v = (v + (r(4) * 2.0 - 1.0) * self.bri_jitter.clamp(0.0, 1.0)).clamp(0.0, 1.0);
        hsv_to_rgb(h, s, v).map(|c| (c * 255.0).round() as u8)
    }
}

fn rgb_to_hsv([r, g, b]: [f32; 3]) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d <= 0.0 {
        0.0
    } else if max == r {
        ((g - b) / d).rem_euclid(6.0) / 6.0
    } else if max == g {
        ((b - r) / d + 2.0) / 6.0
    } else {
        ((r - g) / d + 4.0) / 6.0
    };
    (h, if max > 0.0 { d / max } else { 0.0 }, max)
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [f32; 3] {
    let h6 = h.rem_euclid(1.0) * 6.0;
    let i = h6.floor();
    let f = h6 - i;
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    match i as i32 % 6 {
        0 => [v, t, p],
        1 => [q, v, p],
        2 => [p, v, t],
        3 => [p, q, v],
        4 => [t, p, v],
        _ => [v, p, q],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn many(p: &ScatterParams) -> Vec<Shape> {
        (0..4000).flat_map(|n| p.shapes(7, n, 10.0, 0.0)).collect()
    }

    /// Сдвиг: равномерно до ±S·размер поперёк мазка (по горизонтали — вдоль y), без «во все
    /// стороны» вдоль не сдвигается; с ним — квадрат; по Гауссу — σ ≈ 0,44·S.
    #[test]
    fn offsets_follow_sai() {
        let p = ScatterParams { scattering: 1.0, ..Default::default() };
        let s = many(&p);
        assert!(s.iter().all(|f| f.dx.abs() < 1e-4 && f.dy.abs() <= 10.0));
        let sd = (s.iter().map(|f| f.dy * f.dy).sum::<f32>() / s.len() as f32).sqrt();
        assert!((sd - 10.0 / 3f32.sqrt()).abs() < 0.3, "{sd}");
        let s = many(&ScatterParams { all_directions: true, ..p });
        assert!(s.iter().any(|f| f.dx.abs() > 8.0 && f.dy.abs() > 8.0));
        let s = many(&ScatterParams { gaussian: true, ..p });
        let sd = (s.iter().map(|f| f.dy * f.dy).sum::<f32>() / s.len() as f32).sqrt();
        assert!((sd - 4.4).abs() < 0.3, "{sd}");
    }

    /// Разброс числа: при N = 2 и 100 % — в среднем 1,5 формы; при N = 1 — всегда одна.
    #[test]
    fn count_jitter() {
        let p = ScatterParams { count: 2, count_jitter: 1.0, ..Default::default() };
        let mean = (0..4000).map(|n| p.shapes(3, n, 10.0, 0.0).len()).sum::<usize>() as f32 / 4000.0;
        assert!((mean - 1.5).abs() < 0.05, "{mean}");
        let p = ScatterParams { count: 1, count_jitter: 1.0, ..Default::default() };
        assert!((0..100).all(|n| p.shapes(3, n, 10.0, 0.0).len() == 1));
    }

    /// Разброс оттенка 100 % — оттенки по всему кругу; яркость ±100 % у белого — половина белых.
    #[test]
    fn color_jitter() {
        let p = ScatterParams { hue_jitter: 1.0, ..Default::default() };
        let hues: Vec<f32> = (0..2000).map(|n| rgb_to_hsv(p.color([255, 0, 0], [0, 0, 0], n * 77).map(|c| c as f32 / 255.0)).0).collect();
        assert!(hues.iter().any(|h| (0.4..0.6).contains(h)));
        let p = ScatterParams { bri_jitter: 1.0, ..Default::default() };
        let full = (0..2000).filter(|n| p.color([255, 0, 0], [0, 0, 0], n * 77)[0] == 255).count();
        assert!((900..1100).contains(&full), "{full}");
        assert_eq!(ScatterParams::default().color([1, 2, 3], [9, 9, 9], 5), [1, 2, 3]);
    }
}
