//! Текстовые слои `.sai2` (блок `text`): разбор и отрисовка (`docs/30-text-layers.md`).
//!
//! Блок (по файлу пользователя, сохранённому в SAI 2 сборки 2017-01-19):
//! - `text`, u32 0, f64 x, f64 y — точка привязки (левый верх строки) на холсте;
//! - u32 (`00 00 01 64`, ?);
//! - вложенный `tvec` (32 байта): матрица 2×2 из f64 — поворот и масштаб;
//! - вложенный `data`: u32 число отрезков, затем на отрезок: u16 длина и имя шрифта в UTF-16,
//!   u16 0, вложенный `text`: 12 байт заголовка (размер в сотых долях пикселя — u16 по смещению
//!   4, цвет BGR — по смещению 8) и символы по 4 байта (UTF-32), в конце нули.
//!
//! Пустой слой — `data` длины 0. Файл пишется обратно как прочитан (`VectorData`), а пиксели
//! рисуются здесь системным шрифтом с тем же именем.

use ab_glyph::{Font, FontVec, PxScale, ScaleFont};
use paint_core::tile::ColorMap;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Clone, Debug, PartialEq)]
pub struct TextRun {
    pub font: String,
    /// Размер, px.
    pub size: f32,
    pub color: [u8; 3],
    pub text: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextLayer {
    pub x: f64,
    pub y: f64,
    /// Матрица 2×2: (a, b, c, d) — точка (u, v) строки уходит в (x + a·u + c·v, y + b·u + d·v).
    pub matrix: [f64; 4],
    pub runs: Vec<TextRun>,
}

fn u32_at(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?))
}

fn f64_at(b: &[u8], o: usize) -> Option<f64> {
    Some(f64::from_le_bytes(b.get(o..o + 8)?.try_into().ok()?))
}

/// Вложенные блоки «метка, u32 длина, данные» начиная с `from`.
fn sub_blocks(b: &[u8], mut p: usize) -> Vec<([u8; 4], &[u8])> {
    let mut out = Vec::new();
    while p + 8 <= b.len() {
        let tag: [u8; 4] = b[p..p + 4].try_into().unwrap();
        let len = u32_at(b, p + 4).unwrap_or(0) as usize;
        let Some(body) = b.get(p + 8..p + 8 + len) else { break };
        out.push((tag, body));
        p += 8 + len;
    }
    out
}

pub fn parse(raw: &[u8]) -> Option<TextLayer> {
    if raw.get(0..4)? != b"text" {
        return None;
    }
    let (x, y) = (f64_at(raw, 4)?, f64_at(raw, 12)?);
    let mut layer = TextLayer { x, y, matrix: [1.0, 0.0, 0.0, 1.0], runs: Vec::new() };
    for (tag, body) in sub_blocks(raw, 24) {
        match &tag {
            b"tvec" if body.len() >= 32 => {
                layer.matrix = [f64_at(body, 0)?, f64_at(body, 8)?, f64_at(body, 16)?, f64_at(body, 24)?];
            }
            b"data" if body.len() >= 4 => {
                let count = u32_at(body, 0)? as usize;
                let mut q = 4;
                for _ in 0..count.min(1000) {
                    let n = u16::from_le_bytes(body.get(q..q + 2)?.try_into().ok()?) as usize;
                    let name: Vec<u16> = body.get(q + 2..q + 2 + 2 * n)?.chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
                    q += 2 + 2 * n + 2;
                    let (_, t) = sub_blocks(body, q).into_iter().find(|(tag, _)| tag == b"text")?;
                    let tlen = u32_at(body, q + 4)? as usize;
                    q += 8 + tlen;
                    let size = u16::from_le_bytes(t.get(4..6)?.try_into().ok()?) as f32 / 100.0;
                    let color = [*t.get(10)?, *t.get(9)?, *t.get(8)?];
                    let text: String = t.get(12..)?.as_chunks::<4>().0.iter().filter_map(|c| char::from_u32(u32::from_le_bytes(*c))).filter(|c| *c != '\0').collect();
                    layer.runs.push(TextRun { font: String::from_utf16_lossy(&name), size, color, text });
                }
            }
            _ => {}
        }
    }
    Some(layer)
}

/// Шрифт по имени семейства (с начертанием, как в SAI: «Noto Sans Mono Black»).
fn font(name: &str) -> Option<Arc<FontVec>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<Arc<FontVec>>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(f) = cache.lock().unwrap().get(name) {
        return f.clone();
    }
    let found = find_font_file(name).and_then(|(path, index)| {
        let data = std::fs::read(path).ok()?;
        FontVec::try_from_vec_and_index(data, index).ok().map(Arc::new)
    });
    cache.lock().unwrap().insert(name.to_string(), found.clone());
    found
}

/// Файл шрифта и номер в коллекции.
#[cfg(not(windows))]
fn find_font_file(name: &str) -> Option<(std::path::PathBuf, u32)> {
    let out = std::process::Command::new("fc-match").args(["-f", "%{file}\n%{index}", name]).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let mut lines = text.lines();
    let file = lines.next()?.trim();
    let index = lines.next().and_then(|v| v.trim().parse().ok()).unwrap_or(0);
    (!file.is_empty()).then(|| (file.into(), index))
}

/// В Windows — по таблицам имён шрифтов в папках системы и пользователя.
#[cfg(windows)]
fn find_font_file(name: &str) -> Option<(std::path::PathBuf, u32)> {
    static INDEX: OnceLock<Vec<(String, std::path::PathBuf, u32)>> = OnceLock::new();
    let index = INDEX.get_or_init(|| {
        let mut dirs = vec![std::path::PathBuf::from(std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into())).join("Fonts")];
        if let Some(l) = std::env::var_os("LOCALAPPDATA") {
            dirs.push(std::path::PathBuf::from(l).join(r"Microsoft\Windows\Fonts"));
        }
        let mut out = Vec::new();
        for d in dirs {
            let Ok(rd) = std::fs::read_dir(d) else { continue };
            for e in rd.flatten() {
                let p = e.path();
                let ext = p.extension().map(|x| x.to_string_lossy().to_lowercase()).unwrap_or_default();
                if !matches!(ext.as_str(), "ttf" | "otf" | "ttc" | "otc") {
                    continue;
                }
                let Ok(data) = std::fs::read(&p) else { continue };
                let n = ttf_parser::fonts_in_collection(&data).unwrap_or(1);
                for i in 0..n {
                    if let Ok(face) = ttf_parser::Face::parse(&data, i) {
                        for rec in face.names() {
                            // полное имя (4) и имя семейства с начертанием (1, 16)
                            if matches!(rec.name_id, 1 | 4 | 16) {
                                if let Some(s) = rec.to_string() {
                                    out.push((s.to_lowercase(), p.clone(), i));
                                }
                            }
                        }
                    }
                }
            }
        }
        out
    });
    let want = name.to_lowercase();
    index.iter().find(|(n, _, _)| *n == want).or_else(|| index.iter().find(|(n, _, _)| want.starts_with(n.as_str()))).map(|(_, p, i)| (p.clone(), *i))
}

/// Запасной шрифт для символов, которых нет в выбранном (как делает система у SAI).
fn fallback() -> Option<Arc<FontVec>> {
    if cfg!(windows) {
        font("Segoe UI").or_else(|| font("Arial"))
    } else {
        font("sans-serif")
    }
}

/// Нарисовать текстовый слой в `pixels` (предумноженные, `pixel::ONE` = 1).
pub fn render(layer: &TextLayer, pixels: &mut ColorMap) {
    let fb = fallback();
    let [a, b, c, d] = layer.matrix;
    let mut pen_x = 0.0f32;
    for run in &layer.runs {
        let Some(main) = font(&run.font).or_else(|| fb.clone()) else { continue };
        let scale = PxScale::from(run.size);
        let ascent = main.as_scaled(scale).ascent();
        for ch in run.text.chars() {
            let f = if main.glyph_id(ch).0 == 0 { fb.as_ref().unwrap_or(&main) } else { &main };
            let sf = f.as_scaled(scale);
            let g = f.glyph_id(ch).with_scale_and_position(scale, ab_glyph::point(pen_x, ascent));
            pen_x += sf.h_advance(g.id);
            let Some(outline) = f.outline_glyph(g) else { continue };
            let bb = outline.px_bounds();
            let rgb = run.color.map(|v| v as u32);
            outline.draw(|gx, gy, cov| {
                if cov <= 0.0 {
                    return;
                }
                // точка строки (u, v) → холст через матрицу слоя
                let (u, v) = (bb.min.x as f64 + gx as f64 + 0.5, bb.min.y as f64 + gy as f64 + 0.5);
                let (px, py) = (layer.x + a * u + c * v, layer.y + b * u + d * v);
                let (ix, iy) = (px.floor() as i32, py.floor() as i32);
                let one = paint_core::pixel::ONE;
                let k = (cov.min(1.0) * one as f32).round() as u32;
                let old = pixels.get(ix, iy);
                let add = [rgb[0] * k / 255, rgb[1] * k / 255, rgb[2] * k / 255, k];
                let px = std::array::from_fn(|i| (add[i] + old[i] as u32 * (one - k) / one).min(one) as u16);
                pixels.set(ix, iy, px);
            });
        }
    }
}
