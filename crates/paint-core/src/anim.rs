//! Анимация как в Clip Studio Paint: папка анимации — дорожка, её вложенные слои — кадры
//! (cel), на шкале времени у кадров (frame) стоят метки «с этого кадра показывать cel»; cel
//! держится до следующей метки (пустая метка — пустой кадр). Таймлайн документа: частота кадров,
//! число кадров, отрезок проигрывания, текущий кадр, Onion skin.

use std::collections::BTreeMap;

/// Onion skin: сколько кадров до и после показывать, непрозрачность ближнего и шаг
/// ослабления, окраска (как «Монохром» в CSP) и её цвета.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Onion {
    pub before: u8,
    pub after: u8,
    pub opacity: f32,
    pub step: f32,
    pub tint: bool,
    pub color_before: [u8; 3],
    pub color_after: [u8; 3],
}

impl Default for Onion {
    fn default() -> Self {
        Self { before: 1, after: 1, opacity: 0.4, step: 0.15, tint: true, color_before: [230, 60, 60], color_after: [60, 110, 230] }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Timeline {
    /// Таймлайн включён: у папок анимации виден только cel текущего кадра.
    pub enabled: bool,
    pub fps: u16,
    /// Число кадров (1…).
    pub frames: u32,
    /// Отрезок проигрывания (с нуля, включительно).
    pub start: u32,
    pub end: u32,
    /// Текущий кадр (с нуля).
    pub frame: u32,
    pub onion_on: bool,
    pub onion: Onion,
    pub looped: bool,
}

impl Default for Timeline {
    fn default() -> Self {
        Self { enabled: false, fps: 12, frames: 24, start: 0, end: 23, frame: 0, onion_on: false, onion: Onion::default(), looped: true }
    }
}

impl Timeline {
    /// Кадр в пределах таймлайна.
    pub fn clamp(&mut self) {
        self.frames = self.frames.clamp(1, 100_000);
        self.fps = self.fps.clamp(1, 120);
        self.end = self.end.min(self.frames - 1);
        self.start = self.start.min(self.end);
        self.frame = self.frame.min(self.frames - 1);
    }

    pub fn to_text(&self) -> String {
        let o = &self.onion;
        format!(
            "fps {}\nframes {}\nrange {} {}\nframe {}\nloop {}\nonion {} {} {} {} {} {} {} {} {} {} {} {} {}\n",
            self.fps,
            self.frames,
            self.start,
            self.end,
            self.frame,
            self.looped as u8,
            self.onion_on as u8,
            o.before,
            o.after,
            o.opacity,
            o.step,
            o.tint as u8,
            o.color_before[0],
            o.color_before[1],
            o.color_before[2],
            o.color_after[0],
            o.color_after[1],
            o.color_after[2],
            self.enabled as u8,
        )
    }

    /// Читает строки таймлайна из текста (незнакомые пропускает).
    pub fn read_text(&mut self, s: &str) {
        for line in s.lines() {
            let mut it = line.split_ascii_whitespace();
            let key = it.next().unwrap_or("");
            let n: Vec<f64> = it.filter_map(|v| v.parse().ok()).filter(|v: &f64| v.is_finite()).collect();
            let u = |i: usize| n.get(i).map(|v| v.max(0.0) as u32);
            match key {
                "fps" => self.fps = u(0).unwrap_or(12) as u16,
                "frames" => self.frames = u(0).unwrap_or(24),
                "range" => {
                    self.start = u(0).unwrap_or(0);
                    self.end = u(1).unwrap_or(0);
                }
                "frame" => self.frame = u(0).unwrap_or(0),
                "loop" => self.looped = u(0) != Some(0),
                "onion" if n.len() >= 13 => {
                    self.onion_on = n[0] != 0.0;
                    let o = &mut self.onion;
                    o.before = n[1].clamp(0.0, 10.0) as u8;
                    o.after = n[2].clamp(0.0, 10.0) as u8;
                    o.opacity = (n[3] as f32).clamp(0.0, 1.0);
                    o.step = (n[4] as f32).clamp(0.0, 1.0);
                    o.tint = n[5] != 0.0;
                    o.color_before = [n[6], n[7], n[8]].map(|v| v.clamp(0.0, 255.0) as u8);
                    o.color_after = [n[9], n[10], n[11]].map(|v| v.clamp(0.0, 255.0) as u8);
                    self.enabled = n[12] != 0.0;
                }
                _ => {}
            }
        }
        self.clamp();
    }
}

/// Дорожка папки анимации: кадр → имя cel (пустое — пустой кадр).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AnimTrack {
    pub keys: BTreeMap<u32, String>,
}

impl AnimTrack {
    /// Метка, действующая в кадре `f` (кадр метки и имя cel).
    pub fn key_at(&self, f: u32) -> Option<(u32, &str)> {
        self.keys.range(..=f).next_back().map(|(k, v)| (*k, v.as_str()))
    }

    /// Cel кадра `f` (`None` — пустой кадр или до первой метки).
    pub fn cel_at(&self, f: u32) -> Option<&str> {
        self.key_at(f).map(|(_, v)| v).filter(|v| !v.is_empty())
    }

    /// Соседние cel для Onion skin: `n` предыдущих и `m` следующих меток с cel (ближние
    /// первыми), без cel текущего кадра.
    pub fn neighbours(&self, f: u32, n: usize, m: usize) -> (Vec<&str>, Vec<&str>) {
        let cur = self.key_at(f).map(|(k, _)| k);
        let before = self.keys.range(..cur.unwrap_or(0)).rev().filter(|(_, v)| !v.is_empty()).map(|(_, v)| v.as_str()).take(if cur.is_some() { n } else { 0 }).collect();
        let after = self.keys.range(f + 1..).filter(|(_, v)| !v.is_empty()).map(|(_, v)| v.as_str()).take(m).collect();
        (before, after)
    }

    /// Переименовать cel во всех метках.
    pub fn rename(&mut self, from: &str, to: &str) {
        for v in self.keys.values_mut() {
            if v == from {
                *v = to.to_string();
            }
        }
    }

    pub fn to_text(&self) -> String {
        self.keys.iter().map(|(k, v)| format!("key {k} {v}\n")).collect()
    }

    pub fn from_text(s: &str) -> AnimTrack {
        let mut t = AnimTrack::default();
        for line in s.lines() {
            let Some(rest) = line.strip_prefix("key ") else { continue };
            let (k, name) = rest.split_once(' ').unwrap_or((rest, ""));
            if let Ok(k) = k.trim().parse::<u32>() {
                if t.keys.len() < 100_000 {
                    t.keys.insert(k, name.to_string());
                }
            }
        }
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cels_hold_until_next_key() {
        let mut t = AnimTrack::default();
        t.keys.insert(0, "1".into());
        t.keys.insert(3, "2".into());
        t.keys.insert(5, String::new());
        t.keys.insert(7, "3".into());
        let at: Vec<Option<&str>> = (0..9).map(|f| t.cel_at(f)).collect();
        assert_eq!(at, vec![Some("1"), Some("1"), Some("1"), Some("2"), Some("2"), None, None, Some("3"), Some("3")]);
        assert_eq!(t.neighbours(4, 2, 2), (vec!["1"], vec!["3"]));
        assert_eq!(t.neighbours(7, 1, 1), (vec!["2"], vec![]));
        assert_eq!(AnimTrack::from_text(&t.to_text()), t);
        let mut tl = Timeline { enabled: true, fps: 8, frames: 30, start: 2, end: 20, frame: 5, onion_on: true, looped: false, ..Default::default() };
        tl.onion.before = 3;
        let mut back = Timeline::default();
        back.read_text(&tl.to_text());
        assert_eq!(back, tl);
    }
}
