//! Бумага слоя (`ptex` в `.sai2`): серая текстура, по которой краска слоя где-то густеет и
//! темнеет, а где-то светлеет. Как в SAI 2: текстура повторяется по холсту с масштабом
//! 10–500 %, светлее середины — краска светлеет, темнее — густеет; сила 0–100 % задаёт, насколько
//! далеко пиксель уходит к этим двум крайним значениям (не больше чем на половину пути).
//!
//! Сами текстуры — файлы BMP из папки `papertex` программы SAI; в проект они не входят. Где
//! искать: `SOSAI_PAPERTEX` (папка), иначе `papertex` рядом с программой.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

/// Бумага слоя: имя текстуры и параметры, как в файле.
#[derive(Clone, Debug)]
pub struct Paper {
    pub name: String,
    /// Сила, 0–100 %.
    pub strength: u16,
    /// Масштаб, 10–500 %.
    pub scale: u16,
    /// Третий параметр файла (0 или 1); на вывод не влияет, хранится для записи.
    pub flag: u16,
    /// Текстура, если файл найден.
    pub texture: Option<Arc<PaperTexture>>,
}

impl Paper {
    pub fn new(name: &str, strength: u16, scale: u16, flag: u16) -> Self {
        Self {
            name: name.to_string(),
            strength: strength.min(100),
            scale: scale.clamp(10, 500),
            flag: u16::from(flag != 0),
            texture: texture(name),
        }
    }
}

/// Серая текстура, 8 бит на точку.
pub struct PaperTexture {
    pub width: usize,
    pub height: usize,
    pub gray: Vec<u8>,
}

impl std::fmt::Debug for PaperTexture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PaperTexture({}×{})", self.width, self.height)
    }
}

fn le16(b: &[u8], o: usize) -> Option<u32> {
    Some(u16::from_le_bytes(b.get(o..o + 2)?.try_into().ok()?) as u32)
}

fn le32(b: &[u8], o: usize) -> Option<i32> {
    Some(i32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?))
}

impl PaperTexture {
    /// BMP без сжатия: 8 бит (берётся номер цвета), 24 или 32 бита (переводится в серый).
    pub fn from_bmp(b: &[u8]) -> Option<Self> {
        if b.get(0..2)? != b"BM" || le32(b, 30)? != 0 && le32(b, 30)? != 3 {
            return None;
        }
        let off = le32(b, 10)? as usize;
        let width = le32(b, 18)?;
        let h = le32(b, 22)?;
        let bits = le16(b, 28)?;
        let height = h.unsigned_abs() as usize;
        if width <= 0 || height == 0 || width > 8192 || height > 8192 {
            return None;
        }
        let width = width as usize;
        let row = (width * bits as usize).div_ceil(32) * 4;
        let mut gray = vec![0u8; width * height];
        let lum = |r: u8, g: u8, bl: u8| ((r as u32 * 77 + g as u32 * 150 + bl as u32 * 29 + 128) >> 8) as u8;
        for y in 0..height {
            // строки снизу вверх, если высота положительная
            let src = if h > 0 { height - 1 - y } else { y };
            let line = b.get(off + src * row..off + src * row + row)?;
            for x in 0..width {
                gray[y * width + x] = match bits {
                    // как в SAI: номер цвета и есть значение, палитра не смотрится
                    8 => line[x],
                    24 => lum(line[x * 3 + 2], line[x * 3 + 1], line[x * 3]),
                    32 => lum(line[x * 4 + 2], line[x * 4 + 1], line[x * 4]),
                    _ => return None,
                };
            }
        }
        Some(Self { width, height, gray })
    }

    fn at(&self, x: i64, y: i64) -> u32 {
        let x = x.rem_euclid(self.width as i64) as usize;
        let y = y.rem_euclid(self.height as i64) as usize;
        self.gray[y * self.width + x] as u32
    }

    /// Среднее по площади прямоугольника; координаты в 1/256 точки.
    fn area(&self, u0: i64, v0: i64, u1: i64, v1: i64) -> u32 {
        let (mut sum, mut total) = (0u64, 0u64);
        for j in v0.div_euclid(256)..(v1 + 255).div_euclid(256) {
            let wy = (v1.min((j + 1) * 256) - v0.max(j * 256)).max(0) as u64;
            for i in u0.div_euclid(256)..(u1 + 255).div_euclid(256) {
                let wx = (u1.min((i + 1) * 256) - u0.max(i * 256)).max(0) as u64;
                sum += self.at(i, j) as u64 * wx * wy;
                total += wx * wy;
            }
        }
        sum.checked_div(total).unwrap_or(0) as u32
    }

    /// Двулинейно, веса по 14 бит с отбрасыванием дробной части; координаты в 1/65536 точки.
    fn bilinear(&self, u: i64, v: i64) -> u32 {
        let (x, y) = (u >> 16, v >> 16);
        let (fx, fy) = ((u & 0xffff) as u32 >> 2, (v & 0xffff) as u32 >> 2);
        let row = |y| (self.at(x, y) * (0x4000 - fx) + self.at(x + 1, y) * fx) >> 14;
        (row(y) * (0x4000 - fy) + row(y + 1) * fy) >> 14
    }
}

/// Значения текстуры (0–255) для области холста `x0..x0+w`, `y0..y0+h` при масштабе `scale` %.
/// Текстура привязана к началу холста; шаг считается от начала каждого блока 32 точки, как в SAI.
pub fn sample(tex: &PaperTexture, scale: u16, x0: i32, y0: i32, w: usize, h: usize) -> Vec<u8> {
    let s = scale.clamp(10, 500) as f64 / 100.0;
    let axis = |size: usize| {
        let scaled = ((size as f64 * s) as i64).max(1);
        let ratio = size as f64 / scaled as f64;
        (scaled, ratio, ratio.max(1.0))
    };
    let (sw, rx, bx) = axis(tex.width);
    let (sh, ry, by) = axis(tex.height);
    let up = (tex.width as i64) < sw;
    let up_y = (tex.height as i64) < sh;
    // начало блока в 1/256 точки текстуры и шаг на точку
    let start = |p: i32, ratio: f64, boxw: f64, size: usize| {
        let block = p.div_euclid(32) * 32;
        let u = (((block as f64 + 0.5) * ratio - boxw * 0.5) * 256.0) as i64;
        (block, u.rem_euclid(size as i64 * 256))
    };
    let mut out = vec![0u8; w * h];
    for row in 0..h {
        let y = y0 + row as i32;
        let (by0, v0) = start(y, ry, by, tex.height);
        let v = v0 + (y - by0) as i64 * (ry * 256.0) as i64;
        for col in 0..w {
            let x = x0 + col as i32;
            let (bx0, u0) = start(x, rx, bx, tex.width);
            let u = u0 + (x - bx0) as i64 * (rx * 256.0) as i64;
            out[row * w + col] = if up || up_y {
                // при увеличении: двулинейно, шаг в 1/65536
                let su = u0 * 256 + (x - bx0) as i64 * (rx * 65536.0) as i64;
                let sv = v0 * 256 + (y - by0) as i64 * (ry * 65536.0) as i64;
                tex.bilinear(su, sv)
            } else {
                tex.area(u, v, u + (bx * 256.0) as i64, v + (by * 256.0) as i64)
            } as u8;
        }
    }
    out
}

/// Накладывает бумагу на пиксель: предумноженный RGBA в долях 1/16384 (`0x4000` — единица).
/// `g` — значение текстуры, `strength` — сила 0–100.
pub fn apply_q14(p: [i32; 4], g: u8, strength: u16) -> [i32; 4] {
    if p.iter().all(|&c| c <= 0) {
        return p;
    }
    let one = 0x4000i32;
    let s = (strength.min(100) as i32) * one / 100;
    let t = (g as i32 * 257) >> 1;
    let k = (((t - one) * s) >> 16) * 2;
    let w = k.abs();
    let target = if k < 0 { dense_q14(p) } else { light_q14(p) };
    toward_q14(p, target, w)
}

/// «Густая» цель (её же использует кайма): цвет чуть сводится к серому (на 1/8 разброса
/// каналов), затем удваивается с поправкой у ярких каналов; непрозрачность растёт примерно в
/// полтора раза, избыток сверх единицы вычитается из всех каналов. Без ограничения 0..единица.
pub(crate) fn dense_q14(p: [i32; 4]) -> [i32; 4] {
    let one = 0x4000;
    let [c0, c1, c2, a] = p;
    let mn = c0.min(c1).min(c2);
    let mx = c0.max(c1).max(c2);
    let d = (mx - mn) >> 3;
    let cp = [c0, c1, c2].map(|c| (c - d).max(0));
    let col = cp.map(|c| 2 * c - (a >> 1).min((2 * (mx - c)).min(0x7fff)));
    let alpha = a + ((a.min(mx) + a) >> 1);
    let e = (alpha - one).max(0);
    [col[0] - e, col[1] - e, col[2] - e, alpha - e]
}

/// «Светлая» цель: каждый канал сдвигается к непрозрачности (к белому), непрозрачность прежняя.
fn light_q14(p: [i32; 4]) -> [i32; 4] {
    let [c0, c1, c2, a] = p;
    let l = |c: i32| ((((3 * c + a) & 0xffff) >> 2) + c).min(a);
    [l(c0), l(c1), l(c2), a]
}

/// Пиксель на долю `w` (единица — 16384) к цели `target`, с ограничением цели 0..единица.
pub(crate) fn toward_q14(p: [i32; 4], target: [i32; 4], w: i32) -> [i32; 4] {
    let mut out = p;
    for i in 0..4 {
        let diff = target[i].clamp(0, 0x4000) - p[i];
        out[i] = p[i] + ((diff * w) >> 14);
    }
    out
}

/// Бумага на плитку предумноженных пикселей в долях единицы. `vals` — значения текстуры.
pub fn apply(buf: &mut [[f32; 4]], vals: &[u8], strength: u16) {
    for (px, &g) in buf.iter_mut().zip(vals) {
        if px[3] <= 0.0 {
            continue;
        }
        let q = px.map(|v| (v * 16384.0).round() as i32);
        let r = apply_q14(q, g, strength);
        *px = r.map(|v| v as f32 / 16384.0);
    }
}

static FOLDER: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Папка с текстурами, выбранная в программе (ищется первой).
pub fn set_folder(dir: Option<PathBuf>) {
    *FOLDER.lock().unwrap() = dir;
}

/// Папки, где искать текстуры вида `kind` (`papertex`, `brushtex`, `blotmap`). Выбранная в
/// программе папка может быть самой `papertex` или папкой SAI — соседние папки ищутся рядом.
fn dirs(kind: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(d) = FOLDER.lock().unwrap().clone() {
        if kind == "papertex" {
            out.push(d.clone());
        }
        out.push(d.join(kind));
        if let Some(parent) = d.parent() {
            out.push(parent.join(kind));
        }
    }
    if let Some(d) = std::env::var_os("SOSAI_PAPERTEX") {
        let d = PathBuf::from(d);
        if kind == "papertex" {
            out.push(d.clone());
        }
        if let Some(parent) = d.parent() {
            out.push(parent.join(kind));
        }
    }
    if let Some(exe) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
        out.push(exe.join(kind));
    }
    out
}

/// Имена текстур бумаги (файлы `.bmp`) в первой найденной папке, по алфавиту.
pub fn names() -> Vec<String> {
    names_of("papertex")
}

/// Имена текстур вида `kind` в первой найденной папке, по алфавиту.
pub fn names_of(kind: &str) -> Vec<String> {
    for d in dirs(kind) {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        let mut out: Vec<String> = rd
            .flatten()
            .filter_map(|e| {
                let p = e.path();
                let bmp = p.extension().is_some_and(|x| x.eq_ignore_ascii_case("bmp"));
                bmp.then(|| p.file_stem()?.to_str().map(str::to_string)).flatten()
            })
            .collect();
        if !out.is_empty() {
            out.sort_by_key(|n| n.to_lowercase());
            return out;
        }
    }
    Vec::new()
}

/// Текстура бумаги по имени (имя файла без `.bmp`).
pub fn texture(name: &str) -> Option<Arc<PaperTexture>> {
    texture_of("papertex", name)
}

/// Текстура вида `kind` по имени; прочитанные держатся в памяти, ненайденные ищутся снова в
/// следующий раз (папку могли указать позже).
pub fn texture_of(kind: &str, name: &str) -> Option<Arc<PaperTexture>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Arc<PaperTexture>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    let key = format!("{kind}/{name}");
    if let Some(t) = cache.lock().unwrap().get(&key) {
        return Some(t.clone());
    }
    let found = dirs(kind).into_iter().find_map(|d| {
        let bytes = std::fs::read(d.join(format!("{name}.bmp"))).ok()?;
        PaperTexture::from_bmp(&bytes).map(Arc::new)
    });
    if let Some(t) = &found {
        cache.lock().unwrap().insert(key, t.clone());
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bmp8(w: usize, h: usize, px: &[u8]) -> Vec<u8> {
        let row = w.div_ceil(4) * 4;
        let off = 14 + 40 + 1024;
        let mut b = vec![0u8; off + row * h];
        b[0..2].copy_from_slice(b"BM");
        b[10..14].copy_from_slice(&(off as u32).to_le_bytes());
        b[14..18].copy_from_slice(&40u32.to_le_bytes());
        b[18..22].copy_from_slice(&(w as i32).to_le_bytes());
        b[22..26].copy_from_slice(&(h as i32).to_le_bytes());
        b[28..30].copy_from_slice(&8u16.to_le_bytes());
        // палитра не серая — значение всё равно номер цвета
        for i in 0..256 {
            b[54 + i * 4] = 255 - i as u8;
        }
        for y in 0..h {
            for x in 0..w {
                b[off + (h - 1 - y) * row + x] = px[y * w + x];
            }
        }
        b
    }

    #[test]
    fn bmp_takes_color_index_and_flips_rows() {
        let t = PaperTexture::from_bmp(&bmp8(3, 2, &[1, 2, 3, 4, 5, 6])).unwrap();
        assert_eq!((t.width, t.height, t.gray.clone()), (3, 2, vec![1, 2, 3, 4, 5, 6]));
    }

    #[test]
    fn full_scale_repeats_texture_exactly() {
        let t = PaperTexture { width: 4, height: 2, gray: (0..8).collect() };
        let v = sample(&t, 100, -2, 0, 8, 3);
        assert_eq!(&v[0..8], &[2, 3, 0, 1, 2, 3, 0, 1]);
        assert_eq!(&v[8..16], &[6, 7, 4, 5, 6, 7, 4, 5]);
        assert_eq!(&v[16..24], &v[0..8]);
        // вдвое крупнее: соседние точки — между значениями текстуры
        let t = PaperTexture { width: 2, height: 1, gray: vec![0, 200] };
        let v = sample(&t, 200, 0, 0, 4, 1);
        assert!(v.iter().any(|&g| g > 0 && g < 200), "{v:?}");
    }

    #[test]
    fn dark_paper_thickens_light_paper_lightens() {
        let one = 0x4000;
        let px = [one / 4, one / 2, one / 4, one / 2];
        // середина текстуры почти ничего не меняет, нулевая сила — ничего
        let mid = apply_q14(px, 128, 100);
        assert!((0..4).all(|i| (mid[i] - px[i]).abs() <= 16), "{mid:?}");
        assert_eq!(apply_q14(px, 0, 0), px);
        let dark = apply_q14(px, 0, 100);
        assert!(dark[3] > px[3], "{dark:?}");
        let light = apply_q14(px, 255, 100);
        assert_eq!(light[3], px[3]);
        assert!(light[0] > px[0] && light[0] <= light[3], "{light:?}");
        // прозрачное не трогается
        assert_eq!(apply_q14([0; 4], 0, 100), [0; 4]);
    }
}
