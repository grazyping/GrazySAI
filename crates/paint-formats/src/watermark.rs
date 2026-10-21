//! Водяной знак: текст или картинка поверх готовой картинки при экспорте и в записи процесса.
//! В документ и в `.sai2` не пишется — только в то, что уходит наружу.

use paint_core::tile::ColorMap;
use std::path::PathBuf;

/// Где ставить знак.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Center,
    /// Повторить по всей картинке со сдвигом рядов.
    Tiled,
}

impl Place {
    pub const ALL: [Place; 6] = [Place::BottomRight, Place::BottomLeft, Place::TopRight, Place::TopLeft, Place::Center, Place::Tiled];

    pub fn key(self) -> &'static str {
        match self {
            Place::TopLeft => "top-left",
            Place::TopRight => "top-right",
            Place::BottomLeft => "bottom-left",
            Place::BottomRight => "bottom-right",
            Place::Center => "center",
            Place::Tiled => "tiled",
        }
    }

    pub fn from_key(k: &str) -> Option<Place> {
        Place::ALL.into_iter().find(|p| p.key() == k)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Watermark {
    /// Текст знака (если нет картинки).
    pub text: String,
    /// Картинка знака (PNG с прозрачностью и др.); есть — вместо текста.
    pub image: Option<PathBuf>,
    pub font: String,
    pub color: [u8; 3],
    /// Непрозрачность 0..1.
    pub opacity: f32,
    /// Высота знака в долях меньшей стороны картинки.
    pub size: f32,
    pub place: Place,
    /// Отступ от края в долях меньшей стороны.
    pub margin: f32,
}

impl Default for Watermark {
    fn default() -> Self {
        Self { text: "© ".into(), image: None, font: "Sans".into(), color: [255, 255, 255], opacity: 0.6, size: 0.05, place: Place::BottomRight, margin: 0.02 }
    }
}

/// Знак как картинка RGBA (без предумножения) нужной высоты: (ширина, высота, точки).
pub fn sprite(wm: &Watermark, height: u32) -> Option<(u32, u32, Vec<u8>)> {
    let height = height.clamp(4, 4096);
    if let Some(path) = &wm.image {
        let (w, h, rgba) = paint_core::io::load_image(path).ok()?;
        if w == 0 || h == 0 {
            return None;
        }
        let nw = ((w as f64 * height as f64 / h as f64).round() as u32).clamp(1, 8192);
        return Some((nw, height, scale(&rgba, w, h, nw, height)));
    }
    if wm.text.trim().is_empty() {
        return None;
    }
    // текст — тем же способом, что текстовые слои; высота строки ≈ размер шрифта
    let run = crate::sai2_text::TextRun { font: wm.font.clone(), size: height as f32 * 0.8, color: wm.color, style: 0, text: wm.text.clone() };
    let layer = crate::sai2_text::TextLayer::new(0.0, 0.0, run);
    let mut px = ColorMap::new();
    crate::sai2_text::render(&layer, &mut px);
    let (x0, y0, x1, y1) = paint_core::selection::content_bounds(&px)?;
    let (w, h) = ((x1 - x0) as u32, (y1 - y0) as u32);
    let mut out = vec![0u8; (w * h * 4) as usize];
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let c = paint_core::pixel::to_rgba8(px.get(x0 + x, y0 + y));
            let i = ((y as u32 * w + x as u32) * 4) as usize;
            out[i..i + 4].copy_from_slice(&c);
        }
    }
    Some((w, h, out))
}

/// Билинейное уменьшение/увеличение RGBA (с учётом прозрачности).
fn scale(src: &[u8], w: u32, h: u32, nw: u32, nh: u32) -> Vec<u8> {
    let mut out = vec![0u8; (nw * nh * 4) as usize];
    for y in 0..nh {
        for x in 0..nw {
            let (fx, fy) = ((x as f32 + 0.5) * w as f32 / nw as f32 - 0.5, (y as f32 + 0.5) * h as f32 / nh as f32 - 0.5);
            let (x0, y0) = (fx.floor().clamp(0.0, (w - 1) as f32) as u32, fy.floor().clamp(0.0, (h - 1) as f32) as u32);
            let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
            let (tx, ty) = ((fx - x0 as f32).clamp(0.0, 1.0), (fy - y0 as f32).clamp(0.0, 1.0));
            let at = |xx: u32, yy: u32| -> [f32; 4] {
                let i = ((yy * w + xx) * 4) as usize;
                let a = src[i + 3] as f32 / 255.0;
                [src[i] as f32 * a, src[i + 1] as f32 * a, src[i + 2] as f32 * a, src[i + 3] as f32]
            };
            let (p00, p10, p01, p11) = (at(x0, y0), at(x1, y0), at(x0, y1), at(x1, y1));
            let v: [f32; 4] = std::array::from_fn(|k| (p00[k] * (1.0 - tx) + p10[k] * tx) * (1.0 - ty) + (p01[k] * (1.0 - tx) + p11[k] * tx) * ty);
            let a = v[3] / 255.0;
            let i = ((y * nw + x) * 4) as usize;
            for k in 0..3 {
                out[i + k] = if a > 0.0 { (v[k] / a).round().clamp(0.0, 255.0) as u8 } else { 0 };
            }
            out[i + 3] = v[3].round().clamp(0.0, 255.0) as u8;
        }
    }
    out
}

/// Наложить знак на картинку RGBA `w`×`h` (без предумножения).
pub fn apply(rgba: &mut [u8], w: u32, h: u32, wm: &Watermark) {
    let short = w.min(h) as f32;
    let Some((sw, sh, s)) = sprite(wm, (short * wm.size.clamp(0.005, 0.5)).round() as u32) else { return };
    let m = (short * wm.margin.clamp(0.0, 0.3)).round() as i32;
    let (w_, h_, sw_, sh_) = (w as i32, h as i32, sw as i32, sh as i32);
    let spots: Vec<(i32, i32)> = match wm.place {
        Place::TopLeft => vec![(m, m)],
        Place::TopRight => vec![(w_ - sw_ - m, m)],
        Place::BottomLeft => vec![(m, h_ - sh_ - m)],
        Place::BottomRight => vec![(w_ - sw_ - m, h_ - sh_ - m)],
        Place::Center => vec![((w_ - sw_) / 2, (h_ - sh_) / 2)],
        Place::Tiled => {
            let (stepx, stepy) = ((sw_ * 2).max(1), (sh_ * 4).max(1));
            let mut v = Vec::new();
            let mut row = 0;
            let mut y = -sh_;
            while y < h_ && v.len() < 20_000 {
                let mut x = -sw_ + if row % 2 == 1 { stepx / 2 } else { 0 };
                while x < w_ && v.len() < 20_000 {
                    v.push((x, y));
                    x += stepx;
                }
                y += stepy;
                row += 1;
            }
            v
        }
    };
    let op = wm.opacity.clamp(0.0, 1.0);
    for (ox, oy) in spots {
        for y in 0..sh_ {
            let ty = oy + y;
            if ty < 0 || ty >= h_ {
                continue;
            }
            for x in 0..sw_ {
                let tx = ox + x;
                if tx < 0 || tx >= w_ {
                    continue;
                }
                let si = ((y * sw_ + x) * 4) as usize;
                let a = s[si + 3] as f32 / 255.0 * op;
                if a <= 0.0 {
                    continue;
                }
                let di = ((ty * w_ + tx) * 4) as usize;
                let da = rgba[di + 3] as f32 / 255.0;
                let oa = a + da * (1.0 - a);
                for k in 0..3 {
                    let c = (s[si + k] as f32 * a + rgba[di + k] as f32 * da * (1.0 - a)) / oa.max(1e-6);
                    rgba[di + k] = c.round().clamp(0.0, 255.0) as u8;
                }
                rgba[di + 3] = (oa * 255.0).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image_mark(name: &str) -> (Watermark, PathBuf) {
        let dir = std::env::temp_dir().join(format!("grazysai-wm-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join(format!("{name}.png"));
        // 4×2: красный, непрозрачный
        paint_core::io::save_image(&p, 4, 2, &[255, 0, 0, 255].repeat(8)).unwrap();
        (Watermark { image: Some(p.clone()), opacity: 1.0, size: 0.1, margin: 0.0, ..Default::default() }, p)
    }

    #[test]
    fn corner_mark_lands_in_the_corner() {
        let (wm, p) = image_mark("угол");
        let (w, h) = (200u32, 100u32);
        let mut img = [0u8, 0, 255, 255].repeat((w * h) as usize);
        apply(&mut img, w, h, &wm);
        // знак высотой 10 px (10 % от 100), шириной 20 — в правом нижнем углу
        let at = |x: u32, y: u32| img[((y * w + x) * 4) as usize];
        assert_eq!(at(195, 95), 255, "в углу — знак");
        assert_eq!(at(10, 10), 0, "в стороне — как было");
        let _ = std::fs::remove_file(p);
    }

    #[test]
    fn tiled_and_opacity() {
        let (mut wm, p) = image_mark("плитка");
        wm.place = Place::Tiled;
        wm.opacity = 0.5;
        let (w, h) = (300u32, 300u32);
        let mut img = [0u8, 0, 0, 255].repeat((w * h) as usize);
        apply(&mut img, w, h, &wm);
        let marked = img.as_chunks::<4>().0.iter().filter(|p| p[0] > 100).count();
        assert!(marked > 2000, "знаков много: {marked}");
        assert!(img.as_chunks::<4>().0.iter().all(|p| p[0] <= 128), "непрозрачность 50 %");
        let _ = std::fs::remove_file(p);
    }

    #[test]
    fn empty_text_does_nothing() {
        let wm = Watermark { text: "  ".into(), ..Default::default() };
        let mut img = vec![7u8; 4 * 16];
        apply(&mut img, 4, 4, &wm);
        assert!(img.iter().all(|&v| v == 7));
    }
}
