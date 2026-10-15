//! Смешивание краски с холстом — Кисть, Акварель, Маркер (`docs/16-mixing.md`).
//!
//! Как в образце (правила прочитаны в коде SAI), своими словами:
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
    /// Акварель: слой под мазком размывается (`water_blur`).
    pub blur: bool,
    /// Маркер: где пиксель уже непрозрачен не меньше, чем станет от краски, он перекрашивается
    /// чистым цветом кисти с прежней непрозрачностью; подхваченный цвет ложится только на
    /// прозрачное (`stroke::lay`, docs/16-mixing.md).
    pub marker: bool,
}

impl MixParams {
    /// Значения нового инструмента в образце: Кисть и Маркер — 0/0/50, Акварель — 30/30/50;
    /// порог размытия 50, нажим на смешивание включён.
    pub fn new(blending: f32, dilution: f32, persistence: f32) -> Self {
        Self { blending, dilution, persistence, blend_by_pressure: true, blur_pressure: 0.5, blur: false, marker: false }
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

/// Радиусы двух проходов размытия Акварели в пикселях слоя для кисти диаметра `size`.
///
/// В образце размытие идёт по уменьшенной копии слоя уровня
/// `⌊log₂ D⌋ − 4` (0–3); число шагов `n = (уровень + D) >> (уровень + 1)`, не меньше 1; два прохода
/// скользящего среднего по столбцам и строкам с радиусами `n/2` и `n − n/2`. У нас — на самом слое, радиусы умножены на масштаб уровня.
pub fn water_blur_radii(size: f32) -> (usize, usize) {
    let d = size.max(1.0) as i64;
    let level = ((63 - (d.max(1) as u64).leading_zeros() as i64) - 4).clamp(0, 3);
    let n = ((level + d) >> (level + 1)).max(1) as usize;
    let k = 1usize << level;
    ((n / 2) * k, (n - n / 2) * k)
}

/// Скользящее среднее радиуса `r` по строкам (`horizontal`) или столбцам; края повторяются.
pub fn box_blur(buf: &mut [[f64; 4]], w: usize, h: usize, r: usize, horizontal: bool) {
    if r == 0 {
        return;
    }
    if horizontal {
        blur_rows(buf, w, r);
    } else {
        blur_columns(buf, w, h, r);
    }
}

fn workers(cells: usize) -> usize {
    if cells < 40_000 {
        1
    } else {
        std::thread::available_parallelism().map_or(4, |n| n.get())
    }
}

/// Скользящее среднее одной линии `src` в `out` (края повторяются).
fn blur_line(src: &[[f64; 4]], out: &mut [[f64; 4]], r: usize) {
    let len = src.len();
    let n = (2 * r + 1) as f64;
    let get = |i: isize| src[i.clamp(0, len as isize - 1) as usize];
    let mut sum = [0.0f64; 4];
    for i in -(r as isize)..=(r as isize) {
        let v = get(i);
        for c in 0..4 {
            sum[c] += v[c];
        }
    }
    for (i, o) in out.iter_mut().enumerate() {
        *o = sum.map(|s| s / n);
        let (add, sub) = (get(i as isize + r as isize + 1), get(i as isize - r as isize));
        for c in 0..4 {
            sum[c] += add[c] - sub[c];
        }
    }
}

/// По строкам; строки независимы — считаются параллельно (арифметика каждой строки прежняя).
fn blur_rows(buf: &mut [[f64; 4]], w: usize, r: usize) {
    let one = |line: &mut [[f64; 4]]| {
        let src = line.to_vec();
        blur_line(&src, line, r);
    };
    let threads = workers(buf.len());
    if threads < 2 {
        buf.chunks_mut(w).for_each(one);
        return;
    }
    let per = (buf.len() / w).div_ceil(threads) * w;
    std::thread::scope(|scope| {
        for part in buf.chunks_mut(per) {
            scope.spawn(move || part.chunks_mut(w).for_each(one));
        }
    });
}

/// Указатель на буфер для потоков, пишущих в непересекающиеся столбцы.
#[derive(Clone, Copy)]
struct Cols(*mut [f64; 4]);
unsafe impl Send for Cols {}
unsafe impl Sync for Cols {}

/// По столбцам: полосы по 16 столбцов копируются в непрерывный кусок, считаются, пишутся назад.
fn blur_columns(buf: &mut [[f64; 4]], w: usize, h: usize, r: usize) {
    const B: usize = 16;
    let bands = w.div_ceil(B);
    let ptr = Cols(buf.as_mut_ptr());
    let band = move |k: usize| {
        let p = ptr;
        let (x0, x1) = (k * B, ((k + 1) * B).min(w));
        let bw = x1 - x0;
        let mut local = vec![[0.0f64; 4]; bw * h];
        // SAFETY: полоса k — только столбцы x0..x1, другие потоки их не трогают; буфер жив,
        // пока идут потоки (scope).
        unsafe {
            for y in 0..h {
                for j in 0..bw {
                    local[j * h + y] = *p.0.add(y * w + x0 + j);
                }
            }
        }
        let mut out = vec![[0.0f64; 4]; h];
        for j in 0..bw {
            blur_line(&local[j * h..(j + 1) * h], &mut out, r);
            unsafe {
                for (y, v) in out.iter().enumerate() {
                    *p.0.add(y * w + x0 + j) = *v;
                }
            }
        }
    };
    let threads = workers(buf.len()).min(bands);
    if threads < 2 {
        (0..bands).for_each(band);
        return;
    }
    let next = std::sync::atomic::AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| loop {
                let k = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if k >= bands {
                    break;
                }
                band(k);
            });
        }
    });
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
        // Маркер: на прозрачном краска ложится тем прозрачнее, чем слабее нажим, — по живой SAI
        // 2017 итог на пустом слое равен p² при любой скорости мазка, а перекраска непрозрачного
        // набирает 1 − exp(−4,2·p^1,2). Отсюда непрозрачность краски p² / (1 − exp(−4,2·p^1,2)).
        // Подобрано по измерениям, не прочитано.
        if self.params.marker {
            let p = pressure.clamp(0.0, 1.0) as f64;
            let q = 1.0 - (-4.2 * p.powf(1.2)).exp();
            let target = if q > 1e-9 { (p * p / q).min(1.0) } else { 0.0 };
            // непрозрачность холста не подмешивается: где холст непрозрачнее, побеждает перекраска
            let want = target;
            if out[3] > 1e-9 {
                let k = (want / out[3]).min(1.0);
                for v in out.iter_mut() {
                    *v *= k;
                }
            }
        }
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

#[cfg(test)]
mod blur_tests {
    use super::*;

    /// Прежний последовательный счёт — образец для параллельного.
    fn reference(buf: &mut [[f64; 4]], w: usize, h: usize, r: usize, horizontal: bool) {
        let (len, lines) = if horizontal { (w, h) } else { (h, w) };
        let at = |line: usize, i: usize| if horizontal { line * w + i } else { i * w + line };
        let mut src = vec![[0.0f64; 4]; len];
        let n = (2 * r + 1) as f64;
        for line in 0..lines {
            for (i, v) in src.iter_mut().enumerate() {
                *v = buf[at(line, i)];
            }
            let get = |i: isize| src[i.clamp(0, len as isize - 1) as usize];
            let mut sum = [0.0f64; 4];
            for i in -(r as isize)..=(r as isize) {
                let v = get(i);
                for c in 0..4 {
                    sum[c] += v[c];
                }
            }
            for i in 0..len {
                buf[at(line, i)] = sum.map(|s| s / n);
                let (add, sub) = (get(i as isize + r as isize + 1), get(i as isize - r as isize));
                for c in 0..4 {
                    sum[c] += add[c] - sub[c];
                }
            }
        }
    }

    #[test]
    fn parallel_blur_is_bit_exact() {
        let (w, h) = (333usize, 271usize);
        let start: Vec<[f64; 4]> = (0..w * h).map(|i| [((i * 37) % 101) as f64 / 101.0, ((i * 11) % 13) as f64 / 13.0, 0.25, 1.0]).collect();
        for (r, horizontal) in [(1, true), (40, false), (88, true), (88, false), (300, false)] {
            let (mut a, mut b) = (start.clone(), start.clone());
            box_blur(&mut a, w, h, r, horizontal);
            reference(&mut b, w, h, r, horizontal);
            assert!(a == b, "r {r}, по строкам {horizontal}");
        }
    }
}
