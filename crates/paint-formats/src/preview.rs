//! Миниатюры файлов для обозревателя: быстро, без загрузки всего рисунка, где можно.
//!
//! `.sai2` — встроенная миниатюра `thum` (`jssf`), без неё — сведённое изображение `intg`;
//! `.ora` — `Thumbnails/thumbnail.png`; `.psd` — сведение (только у небольших файлов);
//! плоские картинки — сами. Всё уменьшается до `max` по большей стороне.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// Картинка: ширина, высота, RGBA по 8 бит без предумножения.
pub type Rgba = (u32, u32, Vec<u8>);

/// Расширения файлов, которые обозреватель показывает и открывает.
pub fn supported(path: &Path) -> bool {
    let e = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    e == "sai2" || e == "psd" || e == "psb" || e == "ora" || e == "gpl" || e == "kpl" || paint_core::io::IMAGE_EXTS.contains(&e.as_str())
}

/// Миниатюра файла не больше `max` по большей стороне; `None` — прочитать не удалось.
pub fn thumbnail(path: &Path, max: u32) -> Option<Rgba> {
    let e = path.extension()?.to_string_lossy().to_ascii_lowercase();
    let full = match e.as_str() {
        "sai2" => sai2_preview(path)?,
        "ora" => crate::ora::thumbnail(path).ok()?,
        "gpl" | "kpl" => {
            // миниатюра палитры — её цвета сеткой по 8 в ряд
            let pal = crate::palette::load(path).ok()?;
            let n = pal.colors.len().clamp(1, 256);
            let (cols, cell) = (8usize, 16usize);
            let rows = n.div_ceil(cols);
            let (w, h) = (cols * cell, rows * cell);
            let mut img = vec![0u8; w * h * 4];
            for (i, (c, _)) in pal.colors.iter().take(n).enumerate() {
                let (x0, y0) = ((i % cols) * cell, (i / cols) * cell);
                for y in y0 + 1..y0 + cell - 1 {
                    for x in x0 + 1..x0 + cell - 1 {
                        img[(y * w + x) * 4..][..4].copy_from_slice(&[c[0], c[1], c[2], 255]);
                    }
                }
            }
            (w as u32, h as u32, img)
        }
        "psd" | "psb" => {
            // сведение PSD — целым рисунком; большие файлы не трогаем
            if std::fs::metadata(path).ok()?.len() > 64 << 20 {
                return None;
            }
            let doc = crate::psd::load(path).ok()?;
            (doc.width, doc.height, doc.flatten())
        }
        _ => paint_core::io::load_image(path).ok()?,
    };
    Some(shrink(full, max))
}

fn sai2_preview(path: &Path) -> Option<Rgba> {
    let info = crate::sai2::read_info(path).ok()?;
    let mut file = File::open(path).ok()?;
    let mut read = |tag: &[u8; 4]| -> Option<Vec<u8>> {
        let e = info.entries.iter().find(|e| &e.tag == tag)?;
        let mut data = vec![0u8; e.len as usize];
        file.seek(SeekFrom::Start(e.offset)).ok()?;
        file.read_exact(&mut data).ok()?;
        Some(data)
    };
    if let Some(t) = read(b"thum") {
        // u32 ширина, u32 высота, метка `jssf`, поток
        if t.get(8..12) == Some(b"jssf") {
            if let Some((w, h, rgb)) = crate::jssf::decode(&t[12..]) {
                return Some((w, h, rgb.chunks(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect()));
            }
        }
    }
    let intg = read(b"intg")?;
    let rgba = crate::sai2_pixels::decode_intg(&intg, info.width, info.height).ok()?;
    Some((info.width, info.height, rgba))
}

/// Уменьшить средним по квадрату до `max` по большей стороне (меньшие — как есть).
pub fn shrink(img: Rgba, max: u32) -> Rgba {
    let (w, h, px) = img;
    if w.max(h) <= max || w == 0 || h == 0 {
        return (w, h, px);
    }
    let k = w.max(h) as f64 / max as f64;
    let (tw, th) = (((w as f64 / k).round() as u32).max(1), ((h as f64 / k).round() as u32).max(1));
    let mut out = vec![0u8; (tw * th * 4) as usize];
    for ty in 0..th {
        let (y0, y1) = ((ty as f64 * k) as u32, (((ty + 1) as f64 * k) as u32).clamp(ty + 1, h));
        for tx in 0..tw {
            let (x0, x1) = ((tx as f64 * k) as u32, (((tx + 1) as f64 * k) as u32).clamp(tx + 1, w));
            let mut acc = [0u64; 4];
            let mut n = 0u64;
            for y in y0..y1.min(h) {
                for x in x0..x1.min(w) {
                    let p = &px[((y * w + x) * 4) as usize..][..4];
                    let a = p[3] as u64;
                    for c in 0..3 {
                        acc[c] += p[c] as u64 * a;
                    }
                    acc[3] += a;
                    n += 1;
                }
            }
            let o = &mut out[((ty * tw + tx) * 4) as usize..][..4];
            for c in 0..3 {
                o[c] = acc[c].checked_div(acc[3]).unwrap_or(0) as u8;
            }
            o[3] = (acc[3] / n.max(1)) as u8;
        }
    }
    (tw, th, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shrinks_and_reads_own_files() {
        let (w, h, px) = shrink((300, 100, vec![255; 300 * 100 * 4]), 60);
        assert_eq!((w, h), (60, 20));
        assert!(px.iter().all(|v| *v == 255));
        let dir = std::env::temp_dir().join(format!("grazysai-preview-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let doc = paint_core::Document::new(800, 600);
        let p = dir.join("t.sai2");
        crate::sai2_write::save(&doc, &p).unwrap();
        let (tw, th, _) = thumbnail(&p, 128).expect("миниатюра своего .sai2");
        assert_eq!(tw.max(th), 128);
        let o = dir.join("t.ora");
        crate::ora::save(&doc, &o).unwrap();
        assert!(thumbnail(&o, 128).is_some());
        std::fs::remove_dir_all(dir).ok();
    }
}
