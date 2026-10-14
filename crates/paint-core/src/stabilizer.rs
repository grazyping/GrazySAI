//! Стабилизатор: сглаживает дрожание руки усреднением последних точек.

use crate::stroke::InputPoint;
use std::collections::VecDeque;

pub struct Stabilizer {
    window: usize,
    buf: VecDeque<InputPoint>,
}

impl Stabilizer {
    /// `level` 0 — выключен; чем больше, тем сильнее сглаживание и заметнее отставание.
    pub fn new(level: usize) -> Self {
        Self { window: level + 1, buf: VecDeque::new() }
    }

    fn average(&self) -> InputPoint {
        let n = self.buf.len() as f32;
        let mut s = InputPoint { x: 0.0, y: 0.0, pressure: 0.0 };
        for p in &self.buf {
            s.x += p.x;
            s.y += p.y;
            s.pressure += p.pressure;
        }
        InputPoint { x: s.x / n, y: s.y / n, pressure: s.pressure / n }
    }

    /// Принимает сырую точку, возвращает сглаженную.
    pub fn push(&mut self, p: InputPoint) -> InputPoint {
        self.buf.push_back(p);
        if self.buf.len() > self.window {
            self.buf.pop_front();
        }
        self.average()
    }

    /// Перо поднято: довести линию до последней настоящей точки.
    pub fn finish(&mut self) -> Vec<InputPoint> {
        let mut out = Vec::new();
        while self.buf.len() > 1 {
            self.buf.pop_front();
            out.push(self.average());
        }
        self.buf.clear();
        out
    }
}
