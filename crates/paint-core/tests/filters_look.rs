//! Эффекты фильтров в PNG для просмотра глазами (SOSAI_FILTERS_OUT — папка). Без неё пропускается.
use paint_core::filters::*;
use paint_core::pixel::{from_rgba8, ONE};
use paint_core::tile::ColorMap;

#[test]
fn filters_to_png() {
    let Some(dir) = std::env::var_os("SOSAI_FILTERS_OUT") else { return };
    let dir = std::path::PathBuf::from(dir);
    let (w, h) = (160u32, 120u32);
    let mut src = ColorMap::new();
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let ring = ((x - 80).pow(2) + (y - 60).pow(2)) < 40 * 40;
            let c = if ring { [230, 120, 40, 255] } else { [(x * 255 / 160) as u8, (y * 255 / 120) as u8, 160, 255] };
            src.set(x, y, from_rgba8(c));
        }
    }
    let all = |_: i32, _: i32| ONE;
    let outs = [
        ("aberration", chromatic_aberration(&src, w, h, true, 6.0, 0.0, &all)),
        ("noise", noise(&src, w, h, true, 0.4, &all)),
        ("mosaic", mosaic(&src, w, h, 12, &all)),
        ("crystallize", crystallize(&src, w, h, 14, 0.8, false, &all)),
        ("retro", retro_film(&src, w, h, RetroFilm { preset: 2, effect: 3, intensity: 0.8, noise: 0.2 }, &all)),
        ("perlin", perlin_noise(&src, w, h, Perlin { scale: 40.0, amplitude: 0.8, attenuation: 0.5, repetition: 5, offset: [0.0, 0.0] }, &all)),
        ("sharpen", sharpen(&src, w, h, 1.0, &all)),
    ];
    // искажения — на сетке, чтобы видеть сдвиг
    let mut grid = ColorMap::new();
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let line = x % 16 < 2 || y % 16 < 2;
            grid.set(x, y, from_rgba8(if line { [20, 20, 60, 255] } else { [(x * 255 / 160) as u8, 200, (y * 255 / 120) as u8, 255] }));
        }
    }
    let d = Distort { kind: DistortKind::Pinch, center: (80.0, 60.0), radius: 55.0, amount: 0.7, amplitude: 6.0, wavelength: 24.0, angle: 0.0, shape: 0, to_polar: true };
    let outs = outs.into_iter().chain([
        ("pinch", distort(&grid, w, h, &d, &all)),
        ("bulge", distort(&grid, w, h, &Distort { amount: -0.7, ..d }, &all)),
        ("fisheye", distort(&grid, w, h, &Distort { kind: DistortKind::Fisheye, amount: 0.6, ..d }, &all)),
        ("twirl", distort(&grid, w, h, &Distort { kind: DistortKind::Twirl, amount: 180.0, ..d }, &all)),
        ("ripple", distort(&grid, w, h, &Distort { kind: DistortKind::Ripple, ..d }, &all)),
        ("wave", distort(&grid, w, h, &Distort { kind: DistortKind::Wave, ..d }, &all)),
        ("polar", distort(&grid, w, h, &Distort { kind: DistortKind::Polar, ..d }, &all)),
    ]);
    for (name, m) in outs {
        let mut doc = paint_core::Document::new(w, h);
        doc.layers[0].pixels = m;
        paint_core::io::save_png(&dir.join(format!("{name}.png")), w, h, &doc.flatten()).unwrap();
    }
}
