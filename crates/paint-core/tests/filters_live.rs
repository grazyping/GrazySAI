//! Фильтры против живой SAI 2017: чёрная линия карандашом на прозрачном слое, затем размытие по
//! Гауссу (радиус 1,0 / 3,0 / 9,9) и яркость +51 (`docs/20-filters.md`). Снимки — холст на
//! белой бумаге, поэтому слой восстанавливается как чёрный с альфой = затемнение.

use paint_core::filters::{gaussian_blur, BrightnessContrast};
use paint_core::pixel::{from_rgba8, to_rgba8};
use paint_core::tile::ColorMap;
use std::path::Path;

fn load(name: &str) -> (u32, u32, Vec<u8>) {
    paint_core::io::load_png(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/filters").join(name)).unwrap()
}

fn gray(img: &[u8], w: u32, x: u32, y: u32) -> f64 {
    let i = ((y * w + x) * 4) as usize;
    0.299 * img[i] as f64 + 0.587 * img[i + 1] as f64 + 0.114 * img[i + 2] as f64
}

/// Слой из снимка: столбцы 150..350, строки около линии.
fn layer() -> ColorMap {
    let (w, _, img) = load("sai-line.png");
    let mut px = ColorMap::new();
    for y in 200..320 {
        for x in 100..400 {
            let a = (255.0 - gray(&img, w, x, y)).round().clamp(0.0, 255.0) as u8;
            if a > 0 {
                px.set(x as i32, y as i32, from_rgba8([0, 0, 0, a]));
            }
        }
    }
    px
}

fn shown(px: &ColorMap, x: i32, y: i32) -> f64 {
    let p = to_rgba8(px.get(x, y));
    let a = p[3] as f64 / 255.0;
    255.0 * (1.0 - a) + p[0] as f64 * a
}

#[test]
fn gaussian_blur_matches_the_reference_program() {
    let src = layer();
    for (radius, file) in [(1.0, "sai-line-gauss-1.0.png"), (3.0, "sai-line-gauss-3.0.png"), (9.9, "sai-line-gauss-9.9.png")] {
        let (w, _, sai) = load(file);
        let ours = gaussian_blur(&src, 520, 520, radius, &|_, _| paint_core::pixel::ONE);
        let mut worst: f64 = 0.0;
        for y in 225..296 {
            worst = worst.max((gray(&sai, w, 250, y) - shown(&ours, 250, y as i32)).abs());
        }
        eprintln!("радиус {radius}: наибольшая разница {worst:.1}");
        assert!(worst <= 3.0, "радиус {radius}: {worst}");
    }
}

#[test]
fn brightness_matches_the_reference_program() {
    let src = layer();
    let (w, _, sai) = load("sai-line-brightness-51.png");
    let f = BrightnessContrast { brightness: 51, ..Default::default() };
    let ours = paint_core::filters::map_pixels(&src, &|_, _| paint_core::pixel::ONE, &|p| f.apply(p));
    for y in 255..266 {
        let d = (gray(&sai, w, 250, y) - shown(&ours, 250, y as i32)).abs();
        assert!(d <= 1.5, "строка {y}: разница {d}");
    }
}
