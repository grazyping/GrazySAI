//! Стабилизатор как в образце (`sai2.bak`: начало штриха, точка, отрыв
//! пера):
//!
//! - уровень N (1–15) — среднее последних N сырых точек пера, отдельно по x, y и нажиму;
//!   в начале штриха буфер заполнен N копиями первой точки, поэтому линия «тянется» с места;
//! - S-k (S-1…S-7) — две ступени: среднее последних 15 точек, затем среднее последних k+1
//!   результатов первой ступени (тоже с заполнением при начале);
//! - при отрыве пера последняя сырая точка проводится через фильтр ещё N раз (у S-k — 15·(k+1)),
//!   и линия доходит ровно до места отрыва.
//!
//! Уровень 0 — без сглаживания.

use crate::stroke::InputPoint;
use std::collections::VecDeque;

/// Уровень сглаживания в обозначениях образца.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    /// 0–15.
    Plain(u8),
    /// S-1…S-7.
    S(u8),
}

impl Level {
    /// Все значения списка «Сглаживание»: 0…15, затем S-1…S-7.
    pub fn all() -> Vec<Level> {
        (0..=15).map(Level::Plain).chain((1..=7).map(Level::S)).collect()
    }

    pub fn label(self) -> String {
        match self {
            Level::Plain(n) => n.to_string(),
            Level::S(k) => format!("S-{k}"),
        }
    }
}

#[derive(Clone, Copy, Default)]
struct Avg {
    x: f32,
    y: f32,
    p: f32,
}

/// Кольцо последних значений с средним по `len` последним.
struct Ring {
    len: usize,
    buf: VecDeque<Avg>,
}

impl Ring {
    fn new(len: usize) -> Ring {
        Ring { len: len.max(1), buf: VecDeque::new() }
    }

    fn fill(&mut self, v: Avg) {
        self.buf = std::iter::repeat_n(v, self.len).collect();
    }

    fn push(&mut self, v: Avg) -> Avg {
        self.buf.push_back(v);
        while self.buf.len() > self.len {
            self.buf.pop_front();
        }
        let n = self.buf.len() as f32;
        let mut s = Avg::default();
        for a in &self.buf {
            s.x += a.x;
            s.y += a.y;
            s.p += a.p;
        }
        Avg { x: s.x / n, y: s.y / n, p: s.p / n }
    }
}

pub struct Stabilizer {
    first: Ring,
    second: Option<Ring>,
    flush: usize,
    last: Option<InputPoint>,
}

impl Stabilizer {
    pub fn with_level(level: Level) -> Self {
        match level {
            Level::Plain(n) => {
                let n = (n as usize).max(1);
                Self { first: Ring::new(n), second: None, flush: if n > 1 { n } else { 0 }, last: None }
            }
            Level::S(k) => {
                let k = k.clamp(1, 7) as usize + 1;
                Self { first: Ring::new(15), second: Some(Ring::new(k)), flush: 15 * k, last: None }
            }
        }
    }

    /// Обычный уровень 0–15 (для совместимости).
    pub fn new(level: usize) -> Self {
        Self::with_level(Level::Plain(level.min(15) as u8))
    }

    /// Принимает сырую точку, возвращает сглаженную.
    pub fn push(&mut self, p: InputPoint) -> InputPoint {
        let v = Avg { x: p.x, y: p.y, p: p.pressure };
        if self.last.is_none() {
            self.first.fill(v);
            if let Some(s) = self.second.as_mut() {
                s.fill(v);
            }
        }
        self.last = Some(p);
        let mut a = self.first.push(v);
        if let Some(s) = self.second.as_mut() {
            a = s.push(a);
        }
        InputPoint { x: a.x, y: a.y, pressure: a.p }
    }

    /// Перо поднято: последняя точка проводится через фильтр, пока линия не дойдёт до неё.
    pub fn finish(&mut self) -> Vec<InputPoint> {
        let Some(last) = self.last.take() else { return Vec::new() };
        let mut out = Vec::with_capacity(self.flush);
        for _ in 0..self.flush {
            let v = Avg { x: last.x, y: last.y, p: last.pressure };
            let mut a = self.first.push(v);
            if let Some(s) = self.second.as_mut() {
                a = s.push(a);
            }
            out.push(InputPoint { x: a.x, y: a.y, pressure: a.p });
        }
        out
    }
}
