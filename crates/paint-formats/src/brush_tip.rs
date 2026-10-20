//! Отпечатки кистей из файлов: любая картинка или кисть GIMP `.gbr`.

use paint_core::Stamp;
use std::path::Path;

type Res<T> = Result<T, Box<dyn std::error::Error>>;

/// Чужая кисть (Clip Studio Paint, Krita) в величинах GrazySAI. Где движок другой — приближение,
/// пропущенное перечислено в `skipped`.
#[derive(Clone, Debug)]
pub struct ImportedBrush {
    pub name: String,
    /// Диаметр в пикселях (у CSP миллиметры пересчитаны при 350 dpi — разрешении нового холста CSP).
    pub size: f32,
    /// Непрозрачность × поток, 0..=1.
    pub density: f32,
    /// Жёсткость 0..=1.
    pub hardness: f32,
    /// Постоянный шаг в долях диаметра; `None` — автоматический.
    pub spacing: Option<f32>,
    /// Нажим на размер: доля размера при нулевом нажиме.
    pub size_pressure: Option<f32>,
    /// Нажим на непрозрачность: доля при нулевом нажиме.
    pub density_pressure: Option<f32>,
    /// Смешивание с холстом: (смешивание, разбавление) 0..=1.
    pub mix: Option<(f32, f32)>,
    /// Текстура бумаги из кисти: серая, светлое — краска ложится.
    pub texture: Option<paint_core::paper::PaperTexture>,
    /// Отпечаток-картинка (миниатюра материала-отпечатка; приближение) и его поворот в градусах.
    pub tip: Option<paint_core::Stamp>,
    pub tip_angle: f32,
    /// Следующие картинки набора и смена вразнобой (иначе по порядку).
    pub tip_more: Vec<paint_core::Stamp>,
    pub tip_random: bool,
    /// Толщина кончика (1 — как есть, меньше — сплюснут) и поворот по направлению мазка.
    pub tip_ratio: f32,
    pub tip_follow: bool,
    /// Сила текстуры 0..=1 и масштаб, %.
    pub texture_density: f32,
    pub texture_scale: u16,
    /// Ластик.
    pub erase: bool,
    /// Размазывающая кисть (смешивание с холстом без своего цвета).
    pub smudge: bool,
    /// Настройки, которых у GrazySAI нет (имена — для сообщения).
    pub skipped: Vec<&'static str>,
}


/// Наибольшая сторона хранимого отпечатка: крупнее — уменьшается (кисть всё равно рисует
/// отпечаток размером с кисть, а уменьшение из огромной картинки давало бы рябь).
pub const MAX_SIDE: usize = 512;

/// Имя, все картинки набора и смена вразнобой: у `.gih` — все клетки, у остальных — одна.
pub fn load_all(path: &Path) -> Res<(String, Vec<Stamp>, bool)> {
    let ext = path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).unwrap_or_default();
    if ext == "gih" {
        let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let (name, cells, random) = gih_all(&std::fs::read(path)?)?;
        let cells = cells.into_iter().map(|s| shrink(s, MAX_SIDE)).collect();
        return Ok((if name.trim().is_empty() { stem } else { name }, cells, random));
    }
    load(path).map(|(n, s)| (n, vec![s], false))
}

/// Все клетки набора GIMP и смена вразнобой (`sel0:random` в строке параметров).
pub fn gih_all(d: &[u8]) -> Res<(String, Vec<Stamp>, bool)> {
    let mut lines = d.splitn(3, |&b| b == b'\n');
    let name = String::from_utf8_lossy(lines.next().ok_or("GIH: пусто")?).trim().to_string();
    let params = String::from_utf8_lossy(lines.next().ok_or("GIH: нет строки параметров")?).into_owned();
    let mut rest = lines.next().ok_or("GIH: нет клеток")?;
    let mut cells = Vec::new();
    // клеток не больше заявленного и не больше 256 (испорченный файл не займёт память без меры)
    let want = params.split_whitespace().next().and_then(|n| n.parse::<usize>().ok()).unwrap_or(1).clamp(1, 256);
    while cells.len() < want && rest.len() >= 28 {
        let be = |o: usize| u32::from_be_bytes([rest[o], rest[o + 1], rest[o + 2], rest[o + 3]]) as usize;
        let size = be(0).saturating_add(be(8).saturating_mul(be(12)).saturating_mul(be(16)));
        let (_, s) = gbr(rest)?;
        cells.push(s);
        rest = rest.get(size..).unwrap_or(&[]);
    }
    if cells.is_empty() {
        return Err("GIH: нет клеток".into());
    }
    Ok((name, cells, params.contains(":random")))
}

/// Имя и отпечаток из файла.
pub fn load(path: &Path) -> Res<(String, Stamp)> {
    let ext = path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).unwrap_or_default();
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let (name, stamp) = if ext == "gbr" {
        gbr(&std::fs::read(path)?).map(|(n, s)| (if n.trim().is_empty() { stem } else { n }, s))?
    } else if ext == "gih" {
        gih(&std::fs::read(path)?).map(|(n, s)| (if n.trim().is_empty() { stem } else { n }, s))?
    } else {
        let (w, h, rgba) = paint_core::io::load_image(path)?;
        (stem, Stamp::from_rgba(w as usize, h as usize, &rgba))
    };
    if stamp.width == 0 || stamp.height == 0 {
        return Err("пустая картинка".into());
    }
    Ok((name, shrink(stamp, MAX_SIDE)))
}

/// Кисть GIMP: заголовок из чисел big-endian (длина заголовка, версия, ширина, высота, байт на
/// точку, «GIMP», шаг), имя UTF-8 до конца заголовка, затем точки: 1 байт — маска (255 —
/// краска), 4 — RGBA (краска — непрозрачность).
pub fn gbr(d: &[u8]) -> Res<(String, Stamp)> {
    let be = |o: usize| d.get(o..o + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize).ok_or("GBR: файл обрывается");
    let (header, version, w, h, bytes) = (be(0)?, be(4)?, be(8)?, be(12)?, be(16)?);
    if version == 2 && d.get(20..24) != Some(b"GIMP") {
        return Err("GBR: нет подписи GIMP".into());
    }
    if !(1..=8192).contains(&w) || !(1..=8192).contains(&h) || !(bytes == 1 || bytes == 4) || header > d.len() {
        return Err("GBR: неверный заголовок".into());
    }
    let name_at = if version == 2 { 28 } else { 20 };
    let name = String::from_utf8_lossy(d.get(name_at.min(header)..header).unwrap_or(&[])).trim_end_matches('\0').to_string();
    let data = d.get(header..header + w * h * bytes).ok_or("GBR: мало данных")?;
    let alpha = if bytes == 1 { data.to_vec() } else { data.as_chunks::<4>().0.iter().map(|p| p[3]).collect() };
    Ok((name, Stamp { width: w, height: h, alpha }))
}

/// Набор отпечатков GIMP: строка с именем, строка с числом клеток и правилами выбора, затем
/// клетки — обычные `.gbr` подряд. Берётся первая клетка (смены отпечатков по ходу мазка нет).
pub fn gih(d: &[u8]) -> Res<(String, Stamp)> {
    let mut lines = d.splitn(3, |&b| b == b'\n');
    let name = String::from_utf8_lossy(lines.next().ok_or("GIH: пусто")?).trim().to_string();
    lines.next().ok_or("GIH: нет строки параметров")?;
    let rest = lines.next().ok_or("GIH: нет клеток")?;
    let (_, stamp) = gbr(rest)?;
    Ok((name, stamp))
}

/// Уменьшить, чтобы большая сторона была не больше `max` (усреднение по клеткам).
pub fn shrink(s: Stamp, max: usize) -> Stamp {
    let side = s.width.max(s.height);
    if side <= max {
        return s;
    }
    let (w, h) = ((s.width * max).div_ceil(side).max(1), (s.height * max).div_ceil(side).max(1));
    let mut alpha = vec![0u8; w * h];
    for y in 0..h {
        let (y0, y1) = (y * s.height / h, ((y + 1) * s.height / h).max(y * s.height / h + 1));
        for x in 0..w {
            let (x0, x1) = (x * s.width / w, ((x + 1) * s.width / w).max(x * s.width / w + 1));
            let mut sum = 0u32;
            for yy in y0..y1 {
                for xx in x0..x1 {
                    sum += s.alpha[yy * s.width + xx] as u32;
                }
            }
            alpha[y * w + x] = (sum / ((y1 - y0) * (x1 - x0)) as u32) as u8;
        }
    }
    Stamp { width: w, height: h, alpha }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_gimp_brush() {
        let name = b"Dot\0";
        let header = 28 + name.len();
        let mut d = Vec::new();
        for v in [header as u32, 2, 3, 2, 1] {
            d.extend(v.to_be_bytes());
        }
        d.extend(b"GIMP");
        d.extend(25u32.to_be_bytes());
        d.extend(name);
        d.extend([0, 128, 255, 10, 20, 30]);
        let (n, s) = gbr(&d).unwrap();
        assert_eq!(n, "Dot");
        assert_eq!((s.width, s.height, s.alpha), (3, 2, vec![0, 128, 255, 10, 20, 30]));
    }

    #[test]
    fn large_tips_are_shrunk() {
        let s = shrink(Stamp { width: 1024, height: 512, alpha: vec![200; 1024 * 512] }, 512);
        assert_eq!((s.width, s.height), (512, 256));
        assert!(s.alpha.iter().all(|&a| a == 200));
    }

    /// Проверка на настоящих кистях: `BRUSH_DIR=папка cargo test -p paint-formats -- --ignored`.
    #[test]
    #[ignore]
    fn reads_local_brushes() {
        let Some(dir) = std::env::var_os("BRUSH_DIR") else { return };
        let mut stack = vec![std::path::PathBuf::from(dir)];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|x| x == "gbr" || x == "gih" || x == "abr") {
                    match load_all(&p) {
                        Ok((n, c, r)) => println!("OK {} «{n}» {}x{} клеток {} вразнобой {r}", p.display(), c[0].width, c[0].height, c.len()),
                        Err(e) => println!("ERR {} {e}", p.display()),
                    }
                }
            }
        }
    }
}
