//! Пересэмплирование против живого образца (сборка 2017-01-19): `before.png` — холст 100×100,
//! снятый с экрана при 100%; `upNNN-mK.png` — тот же холст после Canvas → Change Canvas
//! Resolution на NNN% в режиме K (0 ближайший, 1 билинейный, 2 бикубика, 3 Lanczos 2,
//! 4 Lanczos 3). Холст непрозрачный, поэтому сравнение по RGB.

use paint_core::io::load_png;
use paint_core::resample::{resize, Mode};
use std::path::Path;

fn image(name: &str) -> (usize, usize, Vec<[f32; 4]>) {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/resample").join(name);
    let (w, h, rgba) = load_png(&p).unwrap();
    let px = rgba.chunks(4).map(|c| [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, 1.0]).collect();
    (w as usize, h as usize, px)
}

#[test]
fn resampling_matches_the_reference_program() {
    let (sw, sh, src) = image("before.png");
    for pct in [300, 150, 37] {
        for m in 0..5 {
            let (dw, dh, want) = image(&format!("up{pct}-m{m}.png"));
            let got = resize(&src, sw, sh, dw, dh, Mode::from_index(m).unwrap());
            let worst = got
                .iter()
                .zip(&want)
                .flat_map(|(a, b)| (0..3).map(move |c| ((a[c] * 255.0).round() - b[c] * 255.0).abs()))
                .fold(0f32, f32::max);
            assert!(worst <= 1.0, "{pct}% режим {m}: расхождение {worst}");
        }
    }
}

#[test]
fn bicubic_kernel_is_keys_with_minus_0_8() {
    let k = |x| Mode::Bicubic.kernel(x);
    assert_eq!(k(0.0), 1.0);
    assert!(k(1.0).abs() < 1e-12 && k(2.0).abs() < 1e-12);
    // a = −0,8: значение в 1,5 равно a/8
    assert!((k(1.5) + 0.1).abs() < 1e-12);
}
