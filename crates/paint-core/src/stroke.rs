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
    /// Квадрат радиуса, внутри которого покрытие постоянно (`coverage(0)`): там не нужны ни
    /// корень, ни степень — у большой кисти это почти весь отпечаток.
    fn flat_r2(&self) -> f32 {
        let r = match self {
            Shape::Sai(d, _) => d.inner_radius(),
            Shape::Fitted(s, _) => s.r_in,
        };
        // чуть меньше, чтобы округление квадрата не задело край
        let r = (r - 1e-3).max(0.0);
        r * r
    }
}
use crate::mix::{Mixer, Sample};
use crate::pixel::{Px, ONE};
use crate::tile::{key_of, ColorMap, Key, MaskMap, Tile, TILE};
use std::collections::{HashMap, HashSet};

/// Отпечаток обычной кисти, ждущий наложения в покрытие штриха (`flush_dabs`).
struct Placed {
    p: InputPoint,
    shape: Shape,
    opacity: f32,
    cap: u32,
    reach: f32,
    x0: i32,
    x1: i32,
    y0: i32,
    y1: i32,
    round: bool,
    flat_cov: f32,
    flat_r2: f32,
    flat_a: u32,
}

impl Placed {
    fn new(p: InputPoint, shape: Shape, density: f32) -> Self {
        let opacity = shape.opacity();
        let cap = (density * ONE as f32 + 0.5) as u32;
        let reach = shape.outer() + 1.0;
        // За пределами круга `reach` покрытие нулевое — строки обходятся только внутри круга.
        let round = shape.coverage(reach - 1.0) <= 0.0;
        let flat_cov = shape.coverage(0.0);
        let flat_r2 = shape.flat_r2();
        Self {
            p,
            opacity,
            cap,
            reach,
            x0: (p.x - reach).floor() as i32,
            x1: (p.x + reach).ceil() as i32,
            y0: (p.y - reach).floor() as i32,
            y1: (p.y + reach).ceil() as i32,
            round,
            flat_cov,
            flat_r2,
            flat_a: (flat_cov * opacity * ONE as f32 + 0.5) as u32,
            shape,
        }
    }

    /// Накладывает отпечаток на одну плитку покрытия (создаёт её при первом попадании).
    /// Возвращает, было ли попадание.
    fn paint_tile(&self, key: Key, tile: &mut Option<Tile<u16>>) -> bool {
        let p = self.p;
        let (tx, ty) = key;
        let (ya, yb) = ((ty * TILE).max(self.y0), (ty * TILE + TILE - 1).min(self.y1));
        let (xa, xb) = ((tx * TILE).max(self.x0), (tx * TILE + TILE - 1).min(self.x1));
        let mut hit = false;
        for y in ya..=yb {
            let cy = y as f32 + 0.5 - p.y;
            let (mut ra, mut rb) = (xa, xb);
            if self.round {
                let w2 = self.reach * self.reach - cy * cy;
                if w2 < 0.0 {
                    continue;
                }
                let w = w2.sqrt() + 1.0;
                ra = ra.max((p.x - 0.5 - w).floor() as i32);
                rb = rb.min((p.x - 0.5 + w).ceil() as i32);
            }
            let row = (y - ty * TILE) * TILE - tx * TILE;
            for x in ra..=rb {
                let i = (row + x) as usize;
                let old = tile.as_ref().map_or(0, |t| t.0[i] as u32);
                if old >= self.cap {
                    continue; // уже набрано до плотности этого отпечатка — не изменится
                }
                // Центр пикселя — в (x + 0.5, y + 0.5).
                let cx = x as f32 + 0.5 - p.x;
                let d2 = cx * cx + cy * cy;
                let a = if d2 < self.flat_r2 {
                    if self.flat_cov <= 0.0 {
                        continue;
                    }
                    self.flat_a
                } else {
                    let cov = self.shape.coverage(d2.sqrt());
                    if cov <= 0.0 {
                        continue;
                    }
                    (cov * self.opacity * ONE as f32 + 0.5) as u32
                };
                hit = true;
                let t = tile.get_or_insert_with(|| Tile::filled(0));
                // Наложение «поверх», но не выше плотности этого отпечатка и не ниже уже набранного.
                let sum = old + ((ONE - old) * a + ONE / 2) / ONE;
                t.0[i] = sum.min(self.cap).max(old) as u16;
            }
        }
        hit
    }
}

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
    /// Отпечатки обычной кисти, ещё не наложенные в покрытие.
    queued: Vec<Placed>,
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
            queued: Vec::new(),
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
            self.flush_dabs();
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
        self.flush_dabs();
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
        self.queued.push(Placed::new(p, shape, density));
    }

    /// Кладёт отложенные отпечатки в покрытие штриха. Каждая плитка получает свои отпечатки в
    /// прежнем порядке, поэтому результат тот же, что при поштучном наложении; плитки
    /// независимы и считаются параллельно, если работы много.
    fn flush_dabs(&mut self) {
        let dabs = std::mem::take(&mut self.queued);
        if dabs.is_empty() {
            return;
        }
        // плитка → номера отпечатков, задевающих её, по порядку
        let mut per_tile: HashMap<Key, Vec<usize>> = HashMap::new();
        let mut work = 0usize;
        for (n, d) in dabs.iter().enumerate() {
            for ty in d.y0.div_euclid(TILE)..=d.y1.div_euclid(TILE) {
                for tx in d.x0.div_euclid(TILE)..=d.x1.div_euclid(TILE) {
                    per_tile.entry((tx, ty)).or_default().push(n);
                }
            }
            work += ((d.x1 - d.x0 + 1) * (d.y1 - d.y0 + 1)) as usize;
        }
        let mut jobs: Vec<(Key, Option<Tile<u16>>, Vec<usize>)> =
            per_tile.into_iter().map(|(k, list)| (k, self.mask.tiles.remove(&k), list)).collect();
        let run = |job: &mut (Key, Option<Tile<u16>>, Vec<usize>)| -> bool {
            let (key, tile, list) = job;
            let mut hit = false;
            for &n in list.iter() {
                hit |= dabs[n].paint_tile(*key, tile);
            }
            hit
        };
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
        let hits: Vec<bool> = if work < 200_000 || threads < 2 || jobs.len() < 2 {
            jobs.iter_mut().map(run).collect()
        } else {
            let chunk = jobs.len().div_ceil(threads);
            std::thread::scope(|scope| {
                let handles: Vec<_> = jobs.chunks_mut(chunk).map(|c| scope.spawn(move || c.iter_mut().map(run).collect::<Vec<bool>>())).collect();
                handles.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect()
            })
        };
        for ((key, tile, _), hit) in jobs.into_iter().zip(hits) {
            if let Some(t) = tile {
                self.mask.tiles.insert(key, t);
            }
            if hit {
                self.touched.insert(key);
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
            let mut seen = Grid::over(&batch, false);
            let mut sample = Sample::default();
            let k = 1.0 / ONE as f64;
            for d in &batch {
                for_rows(d, |y, ra, rb, cov_at| {
                    let mut cached: (Key, Option<&Tile<Px>>) = ((i32::MIN, 0), None);
                    for x in ra..=rb {
                        if cov_at(x) <= 0.0 {
                            continue;
                        }
                        let first = seen.at(x, y);
                        if *first {
                            continue;
                        }
                        *first = true;
                        let key = key_of(x, y);
                        if cached.0 != key {
                            cached = (key, pixels.tiles.get(&key));
                        }
                        let px = cached.1.map_or([0; 4], |t| t.0[(y.rem_euclid(TILE) * TILE + x.rem_euclid(TILE)) as usize]);
                        sample.add(px.map(|v| v as f64 * k));
                    }
                });
            }
            let paint = mixer.paint(sample, n, batch[0].pressure);
            if mix.blur {
                let size = self.params.diameter_at(batch[0].pressure);
                water_blur(pixels, &batch, size, sel, originals, &mut changed);
            }
            let marker = (mix.marker && !preserve_opacity).then(|| rgb.map(|c| c as f64 / 255.0));
            lay_batch(pixels, &batch, paint, marker, preserve_opacity, sel, originals, &mut changed);
        }
        let keys: Vec<Key> = changed.into_iter().collect();
        self.touched.extend(keys.iter().copied());
        keys
    }
}

/// Пачка отпечатков кладёт краску `paint`. Плитки независимы: каждая получает свои отпечатки в
/// прежнем порядке, поэтому результат тот же, что при поштучном счёте; много работы — параллельно.
#[allow(clippy::too_many_arguments)]
fn lay_batch(
    pixels: &mut ColorMap,
    batch: &[Pending],
    paint: [f64; 4],
    marker: Option<[f64; 3]>,
    preserve_opacity: bool,
    sel: &crate::selection::Selection,
    originals: &mut HashMap<Key, Option<Tile<Px>>>,
    changed: &mut HashSet<Key>,
) {
    // плитка → отпечатки, чей прямоугольник её задевает
    let mut per_tile: HashMap<Key, Vec<usize>> = HashMap::new();
    let mut work = 0usize;
    for (n, d) in batch.iter().enumerate() {
        let reach = d.shape.outer() + 1.0;
        let (x0, x1) = ((d.x - reach).floor() as i32, (d.x + reach).ceil() as i32);
        let (y0, y1) = ((d.y - reach).floor() as i32, (d.y + reach).ceil() as i32);
        for ty in y0.div_euclid(TILE)..=y1.div_euclid(TILE) {
            for tx in x0.div_euclid(TILE)..=x1.div_euclid(TILE) {
                per_tile.entry((tx, ty)).or_default().push(n);
            }
        }
        work += ((x1 - x0 + 1) * (y1 - y0 + 1)) as usize;
    }
    let mut jobs: Vec<(Key, Option<Tile<Px>>, Vec<usize>)> =
        per_tile.into_iter().map(|(k, list)| (k, pixels.tiles.remove(&k), list)).collect();
    // одна плитка: отпечатки по очереди, точки — как в `for_pixels`, только в пределах плитки
    let run = |job: &mut (Key, Option<Tile<Px>>, Vec<usize>)| -> bool {
        let ((tx, ty), tile, list) = job;
        let (tx0, ty0) = (*tx * TILE, *ty * TILE);
        let mut hit = false;
        for &n in list.iter() {
            let d = &batch[n];
            let opacity = d.shape.opacity() * if marker.is_some() { marker_dab_scale() } else { 1.0 };
            for_rows(d, |y, ra, rb, cov_at| {
                if y < ty0 || y >= ty0 + TILE {
                    return;
                }
                for x in ra.max(tx0)..=rb.min(tx0 + TILE - 1) {
                    let cov = cov_at(x);
                    if cov <= 0.0 {
                        continue;
                    }
                    let s = sel.amount(x, y);
                    if s == 0 {
                        continue;
                    }
                    let cov = cov * s as f32 / ONE as f32;
                    hit = true;
                    let t = tile.get_or_insert_with(|| Tile::filled([0; 4]));
                    let i = ((y - ty0) * TILE + x - tx0) as usize;
                    t.0[i] = match marker {
                        Some(brush) => lay_marker(t.0[i], paint, brush, (cov * opacity) as f64),
                        None => lay(t.0[i], paint, (cov * opacity) as f64, preserve_opacity),
                    };
                }
            });
        }
        hit
    };
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    // прежний вид плиток — до правки (для отмены)
    let before: Vec<Option<Tile<Px>>> = jobs.iter().map(|(k, t, _)| if originals.contains_key(k) { None } else { t.clone() }).collect();
    let hits: Vec<bool> = if work < 150_000 || threads < 2 || jobs.len() < 2 {
        jobs.iter_mut().map(run).collect()
    } else {
        let chunk = jobs.len().div_ceil(threads);
        std::thread::scope(|scope| {
            let handles: Vec<_> = jobs.chunks_mut(chunk).map(|c| scope.spawn(move || c.iter_mut().map(run).collect::<Vec<bool>>())).collect();
            handles.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect()
        })
    };
    for (((key, tile, _), hit), old) in jobs.into_iter().zip(hits).zip(before) {
        if hit {
            originals.entry(key).or_insert(old);
            changed.insert(key);
        }
        if let Some(t) = tile {
            pixels.tiles.insert(key, t);
        }
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
    // вес < 0 — точку не задел ни один отпечаток
    let mut weight = Grid::over(batch, -1.0f64);
    let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    for d in batch {
        let o = d.shape.opacity();
        for_pixels(d, |x, y, cov| {
            let w = weight.at(x, y);
            if *w < 0.0 {
                *w = 0.0;
            }
            *w += (1.0 - *w) * (cov * o) as f64;
            (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
        });
    }
    if x0 > x1 {
        return;
    }
    let m = (r1 + r2) as i32;
    let (bx, by) = (x0 - m, y0 - m);
    let (w, h) = ((x1 + m - bx + 1) as usize, (y1 + m - by + 1) as usize);
    let k = 1.0 / ONE as f64;
    // буфер копии переиспользуется между пачками: выделять и обнулять десятки мегабайт на каждую
    // пачку дороже самого счёта
    thread_local!(static BUF: std::cell::RefCell<Vec<[f64; 4]>> = const { std::cell::RefCell::new(Vec::new()) });
    let mut buf = BUF.with(|b| std::mem::take(&mut *b.borrow_mut()));
    buf.resize(w * h, [0.0; 4]);
    buf.truncate(w * h);
    // копия области слоя: построчно, кусками по плиткам, строки — параллельно
    let fill = |row0: usize, part: &mut [[f64; 4]]| {
        for (j, line) in part.chunks_mut(w).enumerate() {
            let y = by + (row0 + j) as i32;
            let mut col = 0usize;
            while col < w {
                let x = bx + col as i32;
                let key = key_of(x, y);
                let run = ((TILE - x.rem_euclid(TILE)) as usize).min(w - col);
                match pixels.tiles.get(&key) {
                    Some(t) => {
                        let i0 = (y.rem_euclid(TILE) * TILE + x.rem_euclid(TILE)) as usize;
                        for (v, px) in line[col..col + run].iter_mut().zip(&t.0[i0..i0 + run]) {
                            *v = px.map(|c| c as f64 * k);
                        }
                    }
                    None => line[col..col + run].fill([0.0; 4]),
                }
                col += run;
            }
        }
    };
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    if w * h < 40_000 || threads < 2 {
        fill(0, &mut buf);
    } else {
        let per = h.div_ceil(threads);
        std::thread::scope(|scope| {
            for (n, part) in buf.chunks_mut(per * w).enumerate() {
                let fill = &fill;
                scope.spawn(move || fill(n * per, part));
            }
        });
    }
    for r in [r1, r2] {
        crate::mix::box_blur(&mut buf, w, h, r, false);
        crate::mix::box_blur(&mut buf, w, h, r, true);
    }

    for y in y0..=y1 {
        let mut cur: Option<(Key, &mut Tile<Px>)> = None;
        for x in x0..=x1 {
            let a = *weight.at(x, y);
            if a < 0.0 {
                continue;
            }
            let a = a * sel.amount(x, y) as f64 / ONE as f64;
            if a <= 0.0 {
                continue;
            }
            let key = key_of(x, y);
            if cur.as_ref().is_none_or(|c| c.0 != key) {
                originals.entry(key).or_insert_with(|| pixels.tiles.get(&key).cloned());
                changed.insert(key);
                cur = Some((key, pixels.tile_mut(key)));
            }
            let blurred = buf[(y - by) as usize * w + (x - bx) as usize];
            let tile = &mut cur.as_mut().unwrap().1;
            let i = (y.rem_euclid(TILE) * TILE + x.rem_euclid(TILE)) as usize;
            tile.0[i] = lay(tile.0[i], blurred, a, false);
        }
    }
    BUF.with(|b| *b.borrow_mut() = buf);
}

/// Пиксели под отпечатком и покрытие в каждом — построчно, слева направо (порядок важен для
/// сумм с плавающей точкой). Точки с нулевым покрытием пропускаются; внутри круга постоянного
/// покрытия не считаются ни корень, ни профиль.
fn for_pixels(d: &Pending, mut f: impl FnMut(i32, i32, f32)) {
    for_rows(d, |y, ra, rb, cov_at| {
        for x in ra..=rb {
            let cov = cov_at(x);
            if cov > 0.0 {
                f(x, y, cov);
            }
        }
    });
}

/// Строки отпечатка: `f(y, x_from, x_to, покрытие(x))`.
fn for_rows(d: &Pending, mut f: impl FnMut(i32, i32, i32, &dyn Fn(i32) -> f32)) {
    let reach = d.shape.outer() + 1.0;
    let (x0, x1) = ((d.x - reach).floor() as i32, (d.x + reach).ceil() as i32);
    let (y0, y1) = ((d.y - reach).floor() as i32, (d.y + reach).ceil() as i32);
    let round = d.shape.coverage(reach - 1.0) <= 0.0;
    let flat_cov = d.shape.coverage(0.0);
    let flat_r2 = d.shape.flat_r2();
    for y in y0..=y1 {
        let cy = y as f32 + 0.5 - d.y;
        let (mut ra, mut rb) = (x0, x1);
        if round {
            let w2 = reach * reach - cy * cy;
            if w2 < 0.0 {
                continue;
            }
            let w = w2.sqrt() + 1.0;
            ra = ra.max((d.x - 0.5 - w).floor() as i32);
            rb = rb.min((d.x - 0.5 + w).ceil() as i32);
        }
        let cov_at = |x: i32| {
            let cx = x as f32 + 0.5 - d.x;
            let d2 = cx * cx + cy * cy;
            if d2 < flat_r2 { flat_cov } else { d.shape.coverage(d2.sqrt()) }
        };
        f(y, ra, rb, &cov_at);
    }
}

/// Плоская сетка флажков/весов над прямоугольником (вместо хеш-таблиц по точкам).
struct Grid<T> {
    x0: i32,
    y0: i32,
    w: usize,
    v: Vec<T>,
}

impl<T: Copy> Grid<T> {
    fn over(batch: &[Pending], fill: T) -> Self {
        let mut b = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
        for d in batch {
            let reach = d.shape.outer() + 1.0;
            b.0 = b.0.min((d.x - reach).floor() as i32);
            b.1 = b.1.min((d.y - reach).floor() as i32);
            b.2 = b.2.max((d.x + reach).ceil() as i32);
            b.3 = b.3.max((d.y + reach).ceil() as i32);
        }
        let w = (b.2 - b.0 + 1).max(0) as usize;
        let h = (b.3 - b.1 + 1).max(0) as usize;
        Grid { x0: b.0, y0: b.1, w, v: vec![fill; w * h] }
    }
    fn at(&mut self, x: i32, y: i32) -> &mut T {
        &mut self.v[(y - self.y0) as usize * self.w + (x - self.x0) as usize]
    }
}

/// Пиксель сдвигается к краске `paint` (предумноженной, 0..=1) на долю `a`.
/// Маркер кладёт отпечатки слабее остальных кистей «Ver1»: множитель подобран по живой SAI 2017
/// (перекраска непрозрачного при нажиме 0,3 / 0,5 / 0,75 — 0,66 / 0,84 / 0,954;
/// docs/16-mixing.md). По коду образца не объяснён.
const MARKER_DAB_SCALE: f32 = 0.38;

fn marker_dab_scale() -> f32 {
    MARKER_DAB_SCALE
}

/// Маркер, как в образце: из двух результатов — «перекрасить цветом кисти, сохранив
/// непрозрачность пикселя» и обычного «сдвинуть к краске пачки» — берётся тот, что
/// непрозрачнее; при равенстве — перекраска. Поэтому на непрозрачном Маркер кладёт чистый цвет и
/// не тянет подхваченное дальше, а краска с примесью ложится только туда, где было прозрачнее.
fn lay_marker(px: Px, paint: [f64; 4], brush: [f64; 3], a: f64) -> Px {
    let k = ONE as f64;
    let v = px.map(|c| c as f64 / k);
    let mut normal = [0.0f64; 4];
    let mut recolour = [0.0f64; 4];
    for i in 0..4 {
        normal[i] = v[i] + (paint[i] - v[i]) * a;
        let target = if i < 3 { brush[i] * v[3] } else { v[3] };
        recolour[i] = v[i] + (target - v[i]) * a;
    }
    // сравнение — в шкале образца (0x4000 = 1): разница меньше его шага не считается
    let q14 = |v: f64| (v * 16384.0).round() as i32;
    let out = if q14(recolour[3]) < q14(normal[3]) { normal } else { recolour };
    let q = |v: f64| (v.clamp(0.0, 1.0) * k + 0.5) as u16;
    let al = q(out[3]);
    [q(out[0]).min(al), q(out[1]).min(al), q(out[2]).min(al), al]
}

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
