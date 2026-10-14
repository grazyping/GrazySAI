//! Смешивание краски с холстом — Кисть, Акварель, Маркер (`docs/16-mixing.md`).
//!
//! Как в образце (`sai2.bak`: коэффициенты; смешивание;
//! размер пачки), своими словами:
//!
//! - отпечатки идут пачками по N штук; на пачку один раз берётся средний цвет холста под ней и
//!   из него и цвета кисти получается цвет краски, которым рисуется вся пачка;
//! - проценты из панели переводятся в веса `(1 − %)^1,5` (та же приближённая степень, что в
//!   отпечатке): смешивание 0 — чистый цвет кисти, разбавление 0 — прозрачность холста не
//!   подхватывается, стойкость 0 — краска пачки не тянется из прошлой;
//! - веса усиливаются по числу отпечатков в пачке: `w + (1 − w)·w·(N/20)·0,8`;
//! - при включённом «нажиме на смешивание» вес цвета кисти растёт от нуля при нажиме ниже
//!   порога размытия до полного при сильном — лёгкий нажим только размазывает.

use crate::dab::approx_pow;

/// Настройки смешивания из панели инструмента, доли 0..=1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MixParams {
    /// «Смешивание» (Blending): доля цвета холста в краске.
    pub blending: f32,
    /// «Разбавление» (Dilution): сколько прозрачности холста подхватывает краска.
    pub dilution: f32,
    /// «Стойкость» (Persistence): насколько краска пачки повторяет прошлую.
    pub persistence: f32,
    /// Нажим на смешивание: при слабом нажиме кисть только размазывает.
    pub blend_by_pressure: bool,
    /// Порог размытия: ниже этого нажима цвет кисти не добавляется (при `blend_by_pressure`).
    pub blur_pressure: f32,
}

impl MixParams {
    /// Значения нового инструмента в образце: Кисть и Маркер — 0/0/50, Акварель — 30/30/50;
    /// порог размытия 50, нажим на смешивание включён.
    pub fn new(blending: f32, dilution: f32, persistence: f32) -> Self {
        Self { blending, dilution, persistence, blend_by_pressure: true, blur_pressure: 0.5 }
    }
}

/// Предел пачки в «узлах» штриха образца: у самой жёсткой кисти 20, у самой мягкой 4.
pub fn batch_limit(softness: f32) -> usize {
    let h = 1.0 - softness.clamp(0.0, 1.0) as f64;
    (h * h * 16.0 + 4.0) as usize
}

/// Во сколько раз пачка в наших отпечатках меньше предела в узлах образца. Подобрано по живой
/// SAI (`tests/mixing_live.rs`): с пачкой по коду (20) след за полосой затухал вдвое медленнее,
/// с пачкой 20/8 совпал до 1–3 уровней. Похоже, узел штриха в образце — не один наш отпечаток.
const NODES_PER_BATCH_DIV: usize = 8;

/// Предел пачки в наших отпечатках.
pub fn batch_dabs(softness: f32) -> usize {
    (batch_limit(softness) / NODES_PER_BATCH_DIV).max(1)
}

/// Средний цвет холста под пачкой: сумма предумноженных каналов 0..=1 и число пикселей.
#[derive(Clone, Copy, Debug, Default)]
pub struct Sample {
    pub sum: [f64; 4],
    pub count: u32,
}

impl Sample {
    pub fn add(&mut self, px: [f64; 4]) {
        for (s, v) in self.sum.iter_mut().zip(px) {
            *s += v;
        }
        self.count += 1;
    }
}

/// Состояние краски на кисти в течение штриха.
pub struct Mixer {
    blend: f64,
    dilution: f64,
    persistence: f64,
    /// Разбавление выключено (0 %): вес нажима на него не действует.
    no_dilution: bool,
    params: MixParams,
    /// Цвет кисти, прямой (не предумноженный), непрозрачный.
    brush: [f64; 3],
    /// Краска прошлой пачки, предумноженная.
    last: Option<[f64; 4]>,
    /// Размер прошлой пачки — новая не больше чем на 2 отпечатка крупнее.
    pub batch: usize,
}

impl Mixer {
    pub fn new(params: MixParams, rgb: [u8; 3]) -> Self {
        let w = |v: f32| approx_pow(1.0 - v.clamp(0.0, 1.0) as f64, 1.5);
        Self {
            blend: w(params.blending),
            dilution: if params.dilution > 0.0 { w(params.dilution) } else { 1.0 },
            persistence: w(params.persistence),
            no_dilution: params.dilution <= 0.0,
            params,
            brush: rgb.map(|c| c as f64 / 255.0),
            last: None,
            batch: 1,
        }
    }

    /// Размер следующей пачки: растёт от 1 не больше чем на 2 за пачку до предела по жёсткости.
    pub fn next_batch(&mut self, softness: f32) -> usize {
        self.batch = batch_dabs(softness).min(self.batch + 2).max(1);
        self.batch
    }

    /// Краска для пачки из `n` отпечатков при нажиме `pressure`: предумноженная, 0..=1.
    pub fn paint(&mut self, canvas: Sample, n: usize, pressure: f32) -> [f64; 4] {
        let (mut blend, mut dilution) = (self.blend, self.dilution);
        if self.params.blend_by_pressure {
            let t = self.params.blur_pressure.clamp(0.0, 1.0) as f64;
            let p = pressure.clamp(0.0, 1.0) as f64;
            let mut f = if t < 1.0 && p >= t { (p - t) / (1.0 - t) } else { 0.0 };
            if self.persistence < 0.05 {
                f *= self.persistence * 20.0;
            }
            blend *= f;
            if !self.no_dilution {
                dilution *= f;
            }
        }
        let boost = |w: f64| w + (1.0 - w) * w * (n as f64 / 20.0) * 0.8;
        let (blend, dilution, keep) = (boost(blend), boost(dilution), boost(self.persistence));

        // Холст: предумноженный средний цвет и тот же цвет без прозрачности.
        let count = canvas.count.max(1) as f64;
        let premul = canvas.sum.map(|s| s / count);
        let alpha = premul[3];
        let (straight, straight_alpha) = if canvas.count == 0 || alpha < 1.0 / 128.0 {
            ([self.brush[0], self.brush[1], self.brush[2]], 1.0)
        } else {
            ([premul[0] / alpha, premul[1] / alpha, premul[2] / alpha], 1.0)
        };
        // Разбавление: от холста как есть (с прозрачностью) к холсту без прозрачности.
        let a = alpha + (straight_alpha - alpha) * dilution;
        let mut out = [0.0; 4];
        for i in 0..3 {
            let c = premul[i] + (straight[i] - premul[i]) * dilution;
            // Смешивание: к цвету кисти с той же непрозрачностью.
            out[i] = c + (a * self.brush[i] - c) * blend;
        }
        out[3] = a;
        // Стойкость: краска тянется из прошлой пачки.
        if let Some(last) = self.last {
            for i in 0..4 {
                out[i] = last[i] + (out[i] - last[i]) * keep;
            }
        }
        self.last = Some(out);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn white() -> Sample {
        Sample { sum: [1.0; 4], count: 1 }
    }

    #[test]
    fn zero_settings_paint_the_brush_colour() {
        let mut m = Mixer::new(MixParams { blend_by_pressure: false, ..MixParams::new(0.0, 0.0, 0.0) }, [255, 0, 0]);
        let p = m.paint(white(), 1, 1.0);
        assert!((p[0] - 1.0).abs() < 1e-6 && p[1].abs() < 1e-6 && (p[3] - 1.0).abs() < 1e-6, "{p:?}");
    }

    #[test]
    fn full_blending_takes_the_canvas_colour() {
        let mut m = Mixer::new(MixParams { blend_by_pressure: false, ..MixParams::new(1.0, 0.0, 0.0) }, [255, 0, 0]);
        let p = m.paint(white(), 1, 1.0);
        assert!(p.iter().all(|&v| (v - 1.0).abs() < 1e-6), "{p:?}");
    }

    #[test]
    fn light_pressure_only_smudges() {
        let mut m = Mixer::new(MixParams::new(0.0, 0.0, 0.0), [255, 0, 0]);
        let p = m.paint(white(), 1, 0.3);
        assert!(p.iter().all(|&v| (v - 1.0).abs() < 1e-6), "{p:?}");
    }

    #[test]
    fn dilution_picks_up_transparency() {
        let mut m = Mixer::new(MixParams { blend_by_pressure: false, ..MixParams::new(0.0, 1.0, 0.0) }, [0, 0, 255]);
        let p = m.paint(Sample::default(), 1, 1.0);
        assert!(p[3] < 1e-6, "краска на пустом слое при полном разбавлении прозрачна: {p:?}");
    }

    #[test]
    fn batches_grow_to_the_hardness_limit() {
        let mut m = Mixer::new(MixParams::new(0.0, 0.0, 0.0), [0, 0, 0]);
        let sizes: Vec<usize> = (0..4).map(|_| m.next_batch(0.0)).collect();
        assert_eq!(sizes, [2, 2, 2, 2]);
        assert_eq!((batch_limit(0.0), batch_limit(1.0)), (20, 4));
        assert_eq!(batch_dabs(1.0), 1);
    }
}
