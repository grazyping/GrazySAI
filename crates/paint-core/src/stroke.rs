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
use crate::pixel::ONE;
use crate::tile::{key_of, Key, MaskMap, TILE};
use std::collections::HashSet;

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
}

impl Stroke {
    pub fn new(params: BrushParams) -> Self {
        Self { params, mask: MaskMap::new(), last: None, carry: 0.0, dab_count: 0, touched: HashSet::new() }
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
    fn large_flat(&self, d: f32) -> bool {
        self.params.spacing <= 0.0 && !self.params.linework && self.params.hardness >= 1.0 && d >= 20.0
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
        })
    }

    /// Шаг, по которому считается непрозрачность отпечатка.
    fn step_at(&self, pressure: f32) -> f32 {
        let d = self.params.diameter_at(pressure);
        if self.params.spacing > 0.0 {
            return (self.params.spacing * d.max(1.0)).max(0.05);
        }
        if self.large_flat(d) {
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
        if self.large_flat(d) {
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
        let mut pos = 0.0;
        let mut pressure = prev.pressure;
        loop {
            let need = self.placement_step_at(pressure) - self.carry;
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
        let shape = if self.large_flat(diameter) {
            let s = DabShape::new(diameter, self.params.hardness);
            let o = dab_opacity(&s, diameter, self.step_at(p.pressure), density);
            Shape::Fitted(s, o)
        } else {
            let d = self.sai_dab(p.pressure);
            let o = d.opacity(self.step_at(p.pressure));
            Shape::Sai(d, o)
        };
        let opacity = shape.opacity();
        let cap = (density * ONE as f32 + 0.5) as u32;
        self.dab_count += 1;
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
}
