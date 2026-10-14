//! Штрих: цепочка отпечатков кисти вдоль траектории пера.
//!
//! Пока перо не поднято, штрих копит «покрытие» в отдельной маске. Отпечатки накладываются
//! друг на друга как полупрозрачные слои; непрозрачность каждого подобрана так, чтобы вдоль оси
//! мазка получалась заданная плотность (так устроено в образце). Сверх этой плотности маска не
//! растёт, поэтому повторный проход тем же штрихом краску не темнит. На слой штрих переносится
//! целиком — это же даёт одну запись отмены.

use crate::brush::{dab_opacity, step_fraction, BrushParams, DabShape};
use crate::dab::{step_factor, Dab, DabInput};

/// Отпечаток: по модели образца или подобранный (крупная самая жёсткая кисть).
#[derive(Clone)]
enum Shape {
    Sai(Dab, f32),
    Fitted(DabShape, f32),
}

impl Shape {
    fn opacity(&self) -> f32 {
        match self {
            Shape::Sai(_, o) | Shape::Fitted(_, o) => *o,
        }
    }
    fn outer(&self) -> f32 {
        match self {
            Shape::Sai(d, _) => d.outer(),
            Shape::Fitted(s, _) => s.outer(),
        }
    }
    fn coverage(&self, dist: f32) -> f32 {
        match self {
            Shape::Sai(d, _) => d.coverage(dist),
            Shape::Fitted(s, _) => s.coverage(dist),
        }
    }
}
use crate::mix::{Mixer, Sample};
use crate::pixel::{Px, ONE};
use crate::tile::{key_of, ColorMap, Key, MaskMap, Tile, TILE};
use std::collections::{HashMap, HashSet};

/// Отпечаток смешивающей кисти, ждущий своей пачки.
struct Pending {
    x: f32,
    y: f32,
    pressure: f32,
    shape: Shape,
}

/// Наибольший шаг между отпечатками большой жёсткой кисти, px (см. `placement_step_at`).
const FLAT_LARGE_STEP_PX: f32 = 0.85;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InputPoint {
    pub x: f32,
    pub y: f32,
    /// Давление пера 0..=1.
    pub pressure: f32,
}

pub struct Stroke {
    pub params: BrushParams,
    pub mask: MaskMap,
    last: Option<InputPoint>,
    /// Путь, пройденный после последнего отпечатка.
    carry: f32,
    pub dab_count: usize,
    /// Плитки, изменённые с прошлого вызова [`Stroke::take_touched`].
    touched: HashSet<Key>,
    /// Смешивающая кисть: отпечатки, ещё не нарисованные, и краска на кисти.
    pending: Vec<Pending>,
    mixer: Option<Mixer>,
}

impl Stroke {
    pub fn new(params: BrushParams) -> Self {
        Self {
            params,
            mask: MaskMap::new(),
            last: None,
            carry: 0.0,
            dab_count: 0,
            touched: HashSet::new(),
            pending: Vec::new(),
            mixer: None,
        }
    }

    /// Краска смешивается с холстом: штрих рисуется прямо в слой ([`Stroke::paint_mixed`]).
    pub fn is_mixing(&self) -> bool {
        self.params.mix.is_some() && !self.params.erase
    }

    /// Последняя принятая точка траектории.
    pub fn last_point(&self) -> Option<InputPoint> {
        self.last
    }

    /// Отдаёт и забывает список плиток, затронутых новыми отпечатками.
    pub fn take_touched(&mut self) -> HashSet<Key> {
        std::mem::take(&mut self.touched)
    }

    /// Самая жёсткая ступень от 20 px у обычных инструментов: образец рисует её отдельной веткой,
    /// у нас — подобранной моделью `DabShape` (docs/08-brush-verification.md).
    ///
    /// Только при полной плотности: если нажим снижает плотность, образец рисует обычной веткой —
    /// сечение линии «куполом», а не плато (сверено с живой SAI через виртуальный планшет,
    /// `docs/14-virtual-pen.md`).
    fn large_flat(&self, d: f32, pressure: f32) -> bool {
        let full = self.params.density_at(pressure) >= self.params.density * 0.999;
        self.params.spacing <= 0.0 && !self.params.linework && self.params.hardness >= 1.0 && d >= 20.0 && full
    }

    /// Отпечаток по модели образца (docs/13-thin-brushes.md).
    fn sai_dab(&self, pressure: f32) -> Dab {
        let density = self.params.density;
        let factor = if density > 0.0 { self.params.density_at(pressure) / density } else { 0.0 };
        Dab::new(DabInput {
            diameter: self.params.diameter_at(pressure),
            softness: 1.0 - self.params.hardness.clamp(0.0, 1.0),
            density,
            density_factor: factor,
            density_scale: 1.0 - self.params.sharpness.clamp(0.0, 1.0),
            flat: !self.params.linework && self.params.hardness >= 1.0,
            ver1: self.params.ver1_density,
            amplify: self.params.amplify,
        })
    }

    /// Шаг, по которому считается непрозрачность отпечатка.
    fn step_at(&self, pressure: f32) -> f32 {
        let d = self.params.diameter_at(pressure);
        if self.params.spacing > 0.0 {
            return (self.params.spacing * d.max(1.0)).max(0.05);
        }
        if self.large_flat(d, pressure) {
            return (step_fraction(d, self.params.hardness) * d.max(1.0)).max(0.05);
        }
        let dab = self.sai_dab(pressure);
        (step_factor(dab.step_diameter, 1.0 - self.params.hardness.clamp(0.0, 1.0)) * dab.step_size).max(0.05)
    }

    /// Шаг, с которым отпечатки ставятся вдоль мазка. Обычно совпадает с `step_at`. У самой жёсткой
    /// ступени при размере от 20 px образец рисует мазок по отдельной ветке: край линии у него
    /// плотнее, чем у цепочки одиночных отпечатков, и почти не зависит от размера. Это повторяет
    /// шаг не больше 0,85 px (подбор по линиям 20–100 px, docs/08-brush-verification.md).
    /// Поправка подобрана измерением, а не прочитана в коде образца.
    fn placement_step_at(&self, pressure: f32) -> f32 {
        let step = self.step_at(pressure);
        let d = self.params.diameter_at(pressure);
        if self.large_flat(d, pressure) {
            step.min(FLAT_LARGE_STEP_PX)
        } else {
            step
        }
    }

    pub fn add_point(&mut self, p: InputPoint) {
        let Some(prev) = self.last else {
            self.dab(p);
            self.last = Some(p);
            return;
        };
        let (dx, dy) = (p.x - prev.x, p.y - prev.y);
        let len = (dx * dx + dy * dy).sqrt();
        if len <= 0.0 {
            // перо стоит на месте: отпечатков нет, но нажим новой точки запоминается
            self.last = Some(p);
            return;
        }
        let mut pos = 0.0;
        let mut pressure = prev.pressure;
        loop {
            // шаг мог уменьшиться (нажим упал) — тогда следующий отпечаток сразу
            let need = (self.placement_step_at(pressure) - self.carry).max(0.0);
            if pos + need > len {
                self.carry += len - pos;
                break;
            }
            pos += need;
            let t = pos / len;
            pressure = prev.pressure + (p.pressure - prev.pressure) * t;
            self.dab(InputPoint { x: prev.x + dx * t, y: prev.y + dy * t, pressure });
            self.carry = 0.0;
        }
        self.last = Some(p);
    }

    fn dab(&mut self, p: InputPoint) {
        let diameter = self.params.diameter_at(p.pressure);
        let density = self.params.density_at(p.pressure);
        if density <= 0.0 {
            return;
        }
        let shape = if self.large_flat(diameter, p.pressure) {
            let s = DabShape::new(diameter, self.params.hardness);
            let o = dab_opacity(&s, diameter, self.step_at(p.pressure), density);
            Shape::Fitted(s, o)
        } else {
            let d = self.sai_dab(p.pressure);
            let o = d.opacity(self.step_at(p.pressure));
            Shape::Sai(d, o)
        };
        self.dab_count += 1;
        if self.is_mixing() {
            self.pending.push(Pending { x: p.x, y: p.y, pressure: p.pressure, shape });
            return;
        }
        let opacity = shape.opacity();
        let cap = (density * ONE as f32 + 0.5) as u32;
        let reach = shape.outer() + 1.0;
        let (x0, x1) = ((p.x - reach).floor() as i32, (p.x + reach).ceil() as i32);
        let (y0, y1) = ((p.y - reach).floor() as i32, (p.y + reach).ceil() as i32);
        for y in y0..=y1 {
            for x in x0..=x1 {
                // Центр пикселя — в (x + 0.5, y + 0.5).
                let (cx, cy) = (x as f32 + 0.5 - p.x, y as f32 + 0.5 - p.y);
                let cov = shape.coverage((cx * cx + cy * cy).sqrt());
                if cov <= 0.0 {
                    continue;
                }
                let a = (cov * opacity * ONE as f32 + 0.5) as u32;
                let key = key_of(x, y);
                self.touched.insert(key);
                let tile = self.mask.tile_mut(key);
                let i = (y.rem_euclid(TILE) * TILE + x.rem_euclid(TILE)) as usize;
                let old = tile.0[i] as u32;
                // Наложение «поверх», но не выше плотности этого отпечатка и не ниже уже набранного.
                let sum = old + ((ONE - old) * a + ONE / 2) / ONE;
                tile.0[i] = sum.min(cap).max(old) as u16;
            }
        }
    }

    /// Рисует накопленные отпечатки смешивающей кисти прямо в пиксели слоя `pixels`, пачками
    /// (`mix.rs`). Пока перо не поднято (`finish == false`), неполная пачка ждёт. Перед первой
    /// правкой плитки её прежний вид кладётся в `originals` (для отмены). Возвращает плитки,
    /// которые изменились.
    pub fn paint_mixed(
        &mut self,
        pixels: &mut ColorMap,
        rgb: [u8; 3],
        preserve_opacity: bool,
        sel: &crate::selection::Selection,
        originals: &mut HashMap<Key, Option<Tile<Px>>>,
        finish: bool,
    ) -> Vec<Key> {
        let Some(mix) = self.params.mix else { return Vec::new() };
        let softness = 1.0 - self.params.hardness.clamp(0.0, 1.0);
        let mixer = self.mixer.get_or_insert_with(|| Mixer::new(mix, rgb));
        let mut changed = HashSet::new();
        loop {
            let want = crate::mix::batch_dabs(softness).min(mixer.batch + 2);
            if self.pending.is_empty() || (!finish && self.pending.len() < want) {
                break;
            }
            let n = mixer.next_batch(softness).min(self.pending.len());
            let batch: Vec<Pending> = self.pending.drain(..n).collect();
            // Средний цвет холста под пачкой — по всем пикселям, которых она касается.
            let mut seen = HashSet::new();
            let mut sample = Sample::default();
            for d in &batch {
                for_pixels(d, |x, y, _| {
                    if seen.insert((x, y)) {
                        let px = pixels.get(x, y);
                        let k = 1.0 / ONE as f64;
                        sample.add(px.map(|v| v as f64 * k));
                    }
                });
            }
            let paint = mixer.paint(sample, n, batch[0].pressure);
            if mix.blur {
                let size = self.params.diameter_at(batch[0].pressure);
                water_blur(pixels, &batch, size, sel, originals, &mut changed);
            }
            for d in &batch {
                let opacity = d.shape.opacity();
                for_pixels(d, |x, y, cov| {
                    let s = sel.amount(x, y);
                    if s == 0 {
                        return;
                    }
                    let cov = cov * s as f32 / ONE as f32;
                    let key = key_of(x, y);
                    originals.entry(key).or_insert_with(|| pixels.tiles.get(&key).cloned());
                    changed.insert(key);
                    let tile = pixels.tile_mut(key);
                    let i = (y.rem_euclid(TILE) * TILE + x.rem_euclid(TILE)) as usize;
                    tile.0[i] = lay(tile.0[i], paint, (cov * opacity) as f64, preserve_opacity);
                });
            }
        }
        let keys: Vec<Key> = changed.into_iter().collect();
        self.touched.extend(keys.iter().copied());
        keys
    }
}

/// Акварель: пиксели под пачкой сдвигаются к размытому слою на долю, набранную отпечатками.
fn water_blur(
    pixels: &mut ColorMap,
    batch: &[Pending],
    size: f32,
    sel: &crate::selection::Selection,
    originals: &mut HashMap<Key, Option<Tile<Px>>>,
    changed: &mut HashSet<Key>,
) {
    let (r1, r2) = crate::mix::water_blur_radii(size);
    // Доля размытия в каждом пикселе: отпечатки пачки накладываются «поверх».
    let mut weight: HashMap<(i32, i32), f64> = HashMap::new();
    for d in batch {
        let o = d.shape.opacity();
        for_pixels(d, |x, y, cov| {
            let w = weight.entry((x, y)).or_insert(0.0);
            *w += (1.0 - *w) * (cov * o) as f64;
        });
    }
    let Some(x0) = weight.keys().map(|k| k.0).min() else { return };
    let x1 = weight.keys().map(|k| k.0).max().unwrap();
    let y0 = weight.keys().map(|k| k.1).min().unwrap();
    let y1 = weight.keys().map(|k| k.1).max().unwrap();
    let m = (r1 + r2) as i32;
    let (bx, by) = (x0 - m, y0 - m);
    let (w, h) = ((x1 + m - bx + 1) as usize, (y1 + m - by + 1) as usize);
    let k = 1.0 / ONE as f64;
    let mut buf = vec![[0.0f64; 4]; w * h];
    for (i, v) in buf.iter_mut().enumerate() {
        let px = pixels.get(bx + (i % w) as i32, by + (i / w) as i32);
        *v = px.map(|c| c as f64 * k);
    }
    for r in [r1, r2] {
        crate::mix::box_blur(&mut buf, w, h, r, false);
        crate::mix::box_blur(&mut buf, w, h, r, true);
    }
    for (&(x, y), &a) in &weight {
        let a = a * sel.amount(x, y) as f64 / ONE as f64;
        if a <= 0.0 {
            continue;
        }
        let key = key_of(x, y);
        originals.entry(key).or_insert_with(|| pixels.tiles.get(&key).cloned());
        changed.insert(key);
        let blurred = buf[(y - by) as usize * w + (x - bx) as usize];
        let tile = pixels.tile_mut(key);
        let i = (y.rem_euclid(TILE) * TILE + x.rem_euclid(TILE)) as usize;
        tile.0[i] = lay(tile.0[i], blurred, a, false);
    }
}

/// Пиксели под отпечатком и покрытие в каждом.
fn for_pixels(d: &Pending, mut f: impl FnMut(i32, i32, f32)) {
    let reach = d.shape.outer() + 1.0;
    let (x0, x1) = ((d.x - reach).floor() as i32, (d.x + reach).ceil() as i32);
    let (y0, y1) = ((d.y - reach).floor() as i32, (d.y + reach).ceil() as i32);
    for y in y0..=y1 {
        for x in x0..=x1 {
            let (cx, cy) = (x as f32 + 0.5 - d.x, y as f32 + 0.5 - d.y);
            let cov = d.shape.coverage((cx * cx + cy * cy).sqrt());
            if cov > 0.0 {
                f(x, y, cov);
            }
        }
    }
}

/// Пиксель сдвигается к краске `paint` (предумноженной, 0..=1) на долю `a`.
fn lay(px: Px, paint: [f64; 4], a: f64, preserve_opacity: bool) -> Px {
    let k = ONE as f64;
    let mut out = [0.0f64; 4];
    for i in 0..4 {
        let v = px[i] as f64 / k;
        out[i] = v + (paint[i] - v) * a;
    }
    if preserve_opacity {
        // непрозрачность остаётся прежней, меняется только цвет
        let old = px[3] as f64 / k;
        let s = if out[3] > 1e-9 { old / out[3] } else { 0.0 };
        for v in out.iter_mut().take(3) {
            *v *= s;
        }
        out[3] = old;
    }
    let q = |v: f64| (v.clamp(0.0, 1.0) * k + 0.5) as u16;
    let a = q(out[3]);
    [q(out[0]).min(a), q(out[1]).min(a), q(out[2]).min(a), a]
}
