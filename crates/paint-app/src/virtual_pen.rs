//! Виртуальное перо для проверок без планшета: подаёт в окно те же события, что Windows Ink
//! даёт от настоящего пера (касание с силой нажима, движение и кнопка указателя). Включается
//! переменной окружения `PAINT_VIRTUAL_PEN`:
//!
//! - `demo` — встроенный набор мазков: нарастание и спад нажима, лёгкий и сильный нажим,
//!   короткие касания, мазок обратным концом пера (ластик);
//! - путь к CSV `t_ms,x,y,pressure,phase[,eraser]` — x, y в точках холста, нажим 0..1,
//!   phase: `down`, `move`, `up`.
//!
//! Нажим идёт в окно через событие касания, как у Windows Ink, — проверяется тот же путь, что
//! на планшете под Windows (`main.rs`, обработка `Touch { force }`).

use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Phase {
    Down,
    Move,
    Up,
}

#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub t: Duration,
    /// Точка холста.
    pub x: f32,
    pub y: f32,
    pub pressure: f32,
    pub phase: Phase,
    pub eraser: bool,
}

pub struct VirtualPen {
    samples: Vec<Sample>,
    next: usize,
    started: Option<std::time::Instant>,
    /// Ждать столько кадров перед началом — пока окно и холст разложатся.
    warmup: u32,
}

impl VirtualPen {
    pub fn from_env() -> Option<VirtualPen> {
        let spec = std::env::var("PAINT_VIRTUAL_PEN").ok()?;
        let samples = if spec == "demo" { demo() } else { load_csv(&spec).ok()? };
        Some(VirtualPen { samples, next: 0, started: None, warmup: 30 })
    }

    pub fn finished(&self) -> bool {
        self.next >= self.samples.len()
    }

    /// Образцы, время которых пришло.
    pub fn due(&mut self) -> Vec<Sample> {
        if self.warmup > 0 {
            self.warmup -= 1;
            return Vec::new();
        }
        let started = *self.started.get_or_insert_with(std::time::Instant::now);
        let now = started.elapsed();
        let mut out = Vec::new();
        while self.next < self.samples.len() && self.samples[self.next].t <= now {
            out.push(self.samples[self.next]);
            self.next += 1;
        }
        out
    }
}

fn load_csv(path: &str) -> Result<Vec<Sample>, Box<dyn std::error::Error>> {
    let mut out = Vec::new();
    for line in std::fs::read_to_string(path)?.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with("t_ms") {
            continue;
        }
        let f: Vec<&str> = line.split(',').map(str::trim).collect();
        if f.len() < 5 {
            continue;
        }
        let phase = match f[4] {
            "down" => Phase::Down,
            "up" => Phase::Up,
            _ => Phase::Move,
        };
        out.push(Sample {
            t: Duration::from_secs_f64(f[0].parse::<f64>()? / 1000.0),
            x: f[1].parse()?,
            y: f[2].parse()?,
            pressure: f[3].parse::<f32>()?.clamp(0.0, 1.0),
            phase,
            eraser: f.get(5).is_some_and(|v| *v == "1" || *v == "eraser"),
        });
    }
    Ok(out)
}

/// Встроенные мазки: что обычно проверяют на живом пере.
fn demo() -> Vec<Sample> {
    let mut out = Vec::new();
    let mut t = 0.0f64;
    let mut stroke = |out: &mut Vec<Sample>, n: usize, f: &dyn Fn(f32) -> (f32, f32, f32), eraser: bool| {
        for i in 0..=n {
            let u = i as f32 / n as f32;
            let (x, y, p) = f(u);
            let phase = if i == 0 {
                Phase::Down
            } else if i == n {
                Phase::Up
            } else {
                Phase::Move
            };
            out.push(Sample { t: Duration::from_secs_f64(t), x, y, pressure: p, phase, eraser });
            t += 0.008; // 125 точек в секунду, как у планшетов
        }
        t += 0.25;
    };
    // 1. Нарастание и спад нажима по волне
    stroke(&mut out, 160, &|u| (60.0 + u * 520.0, 120.0 + (u * 9.0).sin() * 30.0, (u * std::f32::consts::PI).sin()), false);
    // 2. Лёгкий нажим 0,15 и сильный 1,0 — две прямые
    stroke(&mut out, 80, &|u| (60.0 + u * 520.0, 220.0, 0.15), false);
    stroke(&mut out, 80, &|u| (60.0 + u * 520.0, 270.0, 1.0), false);
    // 3. Короткие касания с разным нажимом
    for k in 0..8 {
        let x = 80.0 + k as f32 * 60.0;
        let p = (k + 1) as f32 / 8.0;
        stroke(&mut out, 6, &move |u| (x + u * 3.0, 340.0, p), false);
    }
    // 4. Быстрый штрих с резким нажатием в начале — проверка «кляксы» на старте
    stroke(&mut out, 40, &|u| (60.0 + u * 520.0, 420.0 - u * 40.0, if u < 0.1 { 1.0 } else { 0.4 }), false);
    // 5. Обратный конец пера стирает поперёк первой волны
    stroke(&mut out, 60, &|u| (320.0, 60.0 + u * 140.0, 0.8), true);
    out
}
