//! Текстовые слои `.sai2` (блок `text`): разбор, запись и отрисовка (`docs/30-text-layers.md`).
//!
//! Блок (файл пользователя из SAI 2 сборки 2017-01-19 и код записи `FUN_14017e540`,
//! `FUN_140087a20`, `FUN_140087780`, чтения `FUN_1400878b0`):
//! - `text`, f64 x, f64 y (с 4-го байта) — точка привязки (левый верх строки) на холсте;
//! - два байта флагов слоя (поля 0x789, 0x78a; у образцов 0) и `01 64`;
//! - вложенный `tvec` (32 байта): матрица 2×2 из f64;
//! - вложенный `data`: u32 число шрифтов, на шрифт u16 длина и имя в UTF-16 с нулём на конце;
//!   затем вложенный `text` — поток u32: символ — его код; старший байт 1 — номер шрифта,
//!   2 — стиль, 3 — размер в px, 5 — размер в сотых px, 4 — цвет 0xRRGGBB; в конце u32 0;
//! - в конце блока u32 0.
//!
//! Атрибут меняется только там, где меняется (начальные значения — нули), поэтому чёрный цвет
//! и стиль 0 тегов не пишут. Стиль (жирный, курсив) по коду не дочитан — пишется 0.

use ab_glyph::{Font, FontVec, PxScale, ScaleFont};
use paint_core::tile::ColorMap;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

/// Отрезок текста с одними атрибутами.
#[derive(Clone, Debug, PartialEq)]
pub struct TextRun {
    pub font: String,
    /// Размер, px.
    pub size: f32,
    pub color: [u8; 3],
    /// Стиль из файла (биты не разобраны; новый текст — 0).
    pub style: u16,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextLayer {
    pub x: f64,
    pub y: f64,
    /// Флаги слоя из заголовка (поля 0x789, 0x78a образца).
    pub flags: [u8; 2],
    /// Матрица 2×2: (a, b, c, d) — точка (u, v) строки уходит в (x + a·u + c·v, y + b·u + d·v).
    pub matrix: [f64; 4],
    pub runs: Vec<TextRun>,
}

impl TextLayer {
    pub fn new(x: f64, y: f64, run: TextRun) -> Self {
        Self { x, y, flags: [0, 0], matrix: [1.0, 0.0, 0.0, 1.0], runs: vec![run] }
    }

    /// Весь текст (для правки).
    pub fn text(&self) -> String {
        self.runs.iter().map(|r| r.text.as_str()).collect()
    }
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
    let mut layer = TextLayer { x, y, flags: [*raw.get(20)?, *raw.get(21)?], matrix: [1.0, 0.0, 0.0, 1.0], runs: Vec::new() };
    for (tag, body) in sub_blocks(raw, 24) {
        match &tag {
            b"tvec" if body.len() >= 32 => {
                layer.matrix = [f64_at(body, 0)?, f64_at(body, 8)?, f64_at(body, 16)?, f64_at(body, 24)?];
            }
            b"data" if body.len() >= 4 => {
                let count = u32_at(body, 0)? as usize;
                let mut fonts = Vec::new();
                let mut q = 4;
                for _ in 0..count.min(1000) {
                    let n = u16::from_le_bytes(body.get(q..q + 2)?.try_into().ok()?) as usize;
                    let name: Vec<u16> = body.get(q + 2..q + 2 + 2 * n)?.chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
                    fonts.push(String::from_utf16_lossy(&name));
                    q += 2 + 2 * n + 2;
                }
                let (_, stream) = sub_blocks(body, q).into_iter().find(|(tag, _)| tag == b"text")?;
                // атрибуты, как в FUN_1400878b0
                let (mut font, mut style, mut size, mut color) = (None::<usize>, 0u16, 0u32, 0u32);
                for c in stream.as_chunks::<4>().0 {
                    let v = u32::from_le_bytes(*c);
                    let (tag, val) = (v & 0xff00_0000, v & 0xff_ffff);
                    match tag {
                        0x0100_0000 => font = Some(val as usize),
                        0x0200_0000 => style = val as u16,
                        0x0300_0000 => size = val * 100,
                        0x0500_0000 => size = val,
                        0x0400_0000 => color = val,
                        0 if val != 0 => {
                            let Some(ch) = char::from_u32(val) else { continue };
                            let f = font.and_then(|i| fonts.get(i)).cloned().unwrap_or_default();
                            let rgb = [(color >> 16) as u8, (color >> 8) as u8, color as u8];
                            let sz = size as f32 / 100.0;
                            match layer.runs.last_mut() {
                                Some(r) if r.font == f && r.size == sz && r.color == rgb && r.style == style => r.text.push(ch),
                                _ => layer.runs.push(TextRun { font: f, size: sz, color: rgb, style, text: ch.to_string() }),
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    Some(layer)
}

/// Блок `text` для файла — в той же раскладке, что пишет образец.
pub fn to_bytes(layer: &TextLayer) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend(b"text");
    out.extend(layer.x.to_le_bytes());
    out.extend(layer.y.to_le_bytes());
    out.extend([layer.flags[0], layer.flags[1], 0x01, 0x64]);
    out.extend(b"tvec");
    out.extend(32u32.to_le_bytes());
    for v in layer.matrix {
        out.extend(v.to_le_bytes());
    }
    let mut data = Vec::new();
    if layer.runs.iter().any(|r| !r.text.is_empty()) {
        let mut fonts: Vec<&str> = Vec::new();
        for r in &layer.runs {
            if !fonts.contains(&r.font.as_str()) {
                fonts.push(&r.font);
            }
        }
        data.extend((fonts.len() as u32).to_le_bytes());
        for f in &fonts {
            let u: Vec<u16> = f.encode_utf16().collect();
            data.extend((u.len() as u16).to_le_bytes());
            for c in u.iter().chain(std::iter::once(&0)) {
                data.extend(c.to_le_bytes());
            }
        }
        // поток: атрибут пишется, только когда меняется (FUN_140087780)
        let mut stream = Vec::new();
        let (mut font, mut style, mut size, mut color) = (None::<usize>, 0u32, 0u32, 0u32);
        for r in &layer.runs {
            let fi = fonts.iter().position(|f| *f == r.font).unwrap_or(0);
            let (st, sz) = (r.style as u32, (r.size * 100.0).round() as u32 & 0xff_ffff);
            let col = (r.color[0] as u32) << 16 | (r.color[1] as u32) << 8 | r.color[2] as u32;
            for ch in r.text.chars() {
                if font != Some(fi) {
                    stream.push(0x0100_0000 | fi as u32);
                    font = Some(fi);
                }
                if style != st {
                    stream.push(0x0200_0000 | st);
                    style = st;
                }
                if size != sz {
                    stream.push(0x0500_0000 | sz);
                    size = sz;
                }
                if color != col {
                    stream.push(0x0400_0000 | col);
                    color = col;
                }
                stream.push(ch as u32);
            }
        }
        data.extend(b"text");
        data.extend(((stream.len() * 4) as u32).to_le_bytes());
        for v in stream {
            data.extend(v.to_le_bytes());
        }
        data.extend(0u32.to_le_bytes());
    }
    out.extend(b"data");
    out.extend((data.len() as u32).to_le_bytes());
    out.extend(data);
    out.extend(0u32.to_le_bytes());
    out
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
    let want = name.to_lowercase();
    let index = font_index();
    let hit = |w: &str| index.iter().find(|(n, _, _, _)| n.to_lowercase() == w);
    hit(&want)
        .or_else(|| index.iter().find(|(n, _, _, _)| want.starts_with(&n.to_lowercase())))
        .map(|(_, p, i, _)| (p.clone(), *i))
}

/// Имена шрифтов Windows: (имя, файл, номер в коллекции, вид имени 1/4/16).
#[cfg(windows)]
fn font_index() -> &'static Vec<(String, std::path::PathBuf, u32, u16)> {
    static INDEX: OnceLock<Vec<(String, std::path::PathBuf, u32, u16)>> = OnceLock::new();
    INDEX.get_or_init(|| {
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
                                    out.push((s, p.clone(), i, rec.name_id));
                                }
                            }
                        }
                    }
                }
            }
        }
        out
    })
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
    let (mut pen_x, mut line_y) = (0.0f32, 0.0f32);
    for run in &layer.runs {
        let Some(main) = font(&run.font).or_else(|| fb.clone()) else { continue };
        let scale = PxScale::from(run.size);
        let ascent = main.as_scaled(scale).ascent();
        let line = main.as_scaled(scale).height() + main.as_scaled(scale).line_gap();
        for ch in run.text.chars() {
            if ch == '\n' {
                pen_x = 0.0;
                line_y += line;
                continue;
            }
            let f = if main.glyph_id(ch).0 == 0 { fb.as_ref().unwrap_or(&main) } else { &main };
            let sf = f.as_scaled(scale);
            let g = f.glyph_id(ch).with_scale_and_position(scale, ab_glyph::point(pen_x, line_y + ascent));
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

/// Шрифты системы — именами, как их показывает SAI («Noto Sans Mono Black»), по алфавиту.
pub fn system_fonts() -> Vec<String> {
    static LIST: OnceLock<Vec<String>> = OnceLock::new();
    LIST.get_or_init(|| {
        let mut out = list_fonts();
        out.sort_by_key(|n| n.to_lowercase());
        out.dedup();
        out
    })
    .clone()
}

#[cfg(not(windows))]
fn list_fonts() -> Vec<String> {
    let Ok(o) = std::process::Command::new("fc-list").args(["-f", "%{fullname[0]}|%{family[0]}\n"]).output() else { return Vec::new() };
    String::from_utf8_lossy(&o.stdout)
        .lines()
        .filter_map(|l| {
            let (full, family) = l.split_once('|')?;
            let n = if full.trim().is_empty() { family } else { full };
            (!n.trim().is_empty()).then(|| n.trim().to_string())
        })
        .collect()
}

#[cfg(windows)]
fn list_fonts() -> Vec<String> {
    font_index().iter().filter(|(_, _, _, id)| *id == 4).map(|(n, _, _, _)| n.clone()).collect()
}
