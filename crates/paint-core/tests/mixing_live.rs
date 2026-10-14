//! Кисть против живой SAI 2017 (виртуальный планшет через Wintab; `docs/16-mixing.md`).
//! Карандаш 30 проводит чёрную полосу на прозрачном слое, затем Кисть по умолчанию (50 px,
//! мин. 50 %, смешивание 0, разбавление 0, стойкость 50, порог размытия 50 %) белым цветом
//! проходит поперёк с нажимом 1,0 / 0,75 / 0,5 / 0,3 и с растущим нажимом.

use paint_core::mix::MixParams;
use paint_core::stabilizer::Stabilizer;
use paint_core::{BrushParams, Document, InputPoint, LiveStroke, Stroke};
use std::path::Path;

fn data(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/mixing").join(name)
}

pub fn render(csv: &str, mix: MixParams, ver1: bool) -> Vec<u8> {
    let pencil = BrushParams { size: 30.0, hardness: 1.0, ..Default::default() };
    let brush = BrushParams {
        size: 50.0,
        min_size: 0.5,
        hardness: 1.0,
        density_by_pressure: true,
        mix: Some(mix),
        ver1_density: ver1,
        ..Default::default()
    };
    let mut doc = Document::new(512, 512);
    let mut live: Option<LiveStroke> = None;
    let mut st = Stabilizer::new(3);
    let mut strokes = 0;
    for line in std::fs::read_to_string(data(csv)).unwrap().lines().skip(1) {
        let f: Vec<&str> = line.split(',').collect();
        let x = f[1].parse::<f32>().unwrap() * 1920.0 - 662.0;
        let y = f[2].parse::<f32>().unwrap() * 1080.0 - 204.0;
        let p = f[3].parse::<f32>().unwrap();
        if x < 0.0 {
            continue; // касания панели
        }
        if f[4] == "up" {
            if let Some(mut l) = live.take() {
                for q in st.finish() {
                    l.stroke.add_point(q);
                }
                doc.live_finish(l);
                strokes += 1;
            }
            st = Stabilizer::new(3);
        } else {
            let q = st.push(InputPoint { x, y, pressure: p });
            let l = live.get_or_insert_with(|| {
                let (params, rgb) = if strokes < 3 { (pencil, [0, 0, 0]) } else { (brush, [255, 255, 255]) };
                LiveStroke::new(vec![0], Stroke::new(params), rgb)
            });
            l.stroke.add_point(q);
            doc.live_update(l);
        }
    }
    doc.flatten()
}

/// Средняя разница по области мазка: x 150..450, ±25 px от оси `y`.
fn region_diff(a: &[u8], b: &[u8], y: usize) -> f32 {
    let mut sum = 0.0;
    for yy in y - 25..=y + 25 {
        for x in 150..450 {
            sum += (gray(a, x, yy) - gray(b, x, yy)).abs();
        }
    }
    sum / (51.0 * 300.0)
}

fn gray(img: &[u8], x: usize, y: usize) -> f32 {
    let i = (y * 512 + x) * 4;
    0.299 * img[i] as f32 + 0.587 * img[i + 1] as f32 + 0.114 * img[i + 2] as f32
}

#[test]
fn brush_drags_paint_like_the_reference_program() {
    let (_, _, sai) = paint_core::io::load_png(&data("sai-brush-default.png")).unwrap();
    let ours = render("brush-strokes.csv", MixParams::new(0.0, 0.0, 0.5), false);
    if let Ok(out) = std::env::var("MIX_LIVE_OUT") {
        paint_core::io::save_png(Path::new(&out), 512, 512, &ours).unwrap();
    }
    // След за полосой (x 270..440) по оси каждого мазка: средняя разница в уровнях. Конец мазка
    // не сравнивается: у нас он на ~12 px длиннее (docs/16-mixing.md, «Что не сделано»).
    let limits = [1.0f32, 2.0, 8.0, 8.0, 8.0];
    for (k, y) in [80usize, 160, 240, 320, 400].iter().enumerate() {
        let diff: Vec<f32> = (270..440).map(|x| (gray(&sai, x, *y) - gray(&ours, x, *y)).abs()).collect();
        let mean = diff.iter().sum::<f32>() / diff.len() as f32;
        eprintln!("мазок {k}: средняя разница {mean:.1}, наибольшая {:.0}", diff.iter().cloned().fold(0.0, f32::max));
        assert!(mean <= limits[k], "мазок {k}: средняя разница {mean} больше {}", limits[k]);
    }
}

/// Акварель по умолчанию (смешивание 30, разбавление 30, стойкость 50) тем же сценарием.
#[test]
fn water_color_like_the_reference_program() {
    let (_, _, sai) = paint_core::io::load_png(&data("sai-water-default.png")).unwrap();
    let ours = render("water-strokes.csv", MixParams { blur: true, ..MixParams::new(0.3, 0.3, 0.5) }, true);
    if let Ok(out) = std::env::var("WATER_LIVE_OUT") {
        paint_core::io::save_png(Path::new(&out), 512, 512, &ours).unwrap();
    }
    // Средняя разница по области каждого мазка (x 150..450, ±25 px от оси). Больше всего
    // расходится мазок с нажимом 0,5: у нас краска немного тянется за полосу (docs/16-mixing.md).
    let limits = [6.0f32, 6.0, 12.0, 8.0, 8.0];
    for (k, y) in [80usize, 160, 240, 320, 400].iter().enumerate() {
        let mut sum = 0.0;
        let mut n = 0.0;
        for yy in y - 25..=y + 25 {
            for x in 150..450 {
                sum += (gray(&sai, x, yy) - gray(&ours, x, yy)).abs();
                n += 1.0;
            }
        }
        let mean = sum / n;
        eprintln!("акварель, мазок {k}: средняя разница {mean:.1}");
        assert!(mean <= limits[k], "мазок {k}: средняя разница {mean} больше {}", limits[k]);
    }
}

/// Маркер по умолчанию (смешивание 0, стойкость 50, без порога размытия, «Ver1 Prs Spec.»)
/// тем же сценарием. Расходится больше остальных: у SAI Маркер почти не тянет краску за
/// полосу, у нас тянет (docs/16-mixing.md).
#[test]
fn marker_like_the_reference_program() {
    let (_, _, sai) = paint_core::io::load_png(&data("sai-marker-default.png")).unwrap();
    let ours = render("marker-strokes.csv", MixParams { blur_pressure: 0.0, ..MixParams::new(0.0, 0.0, 0.5) }, true);
    if let Ok(out) = std::env::var("MARKER_LIVE_OUT") {
        paint_core::io::save_png(Path::new(&out), 512, 512, &ours).unwrap();
    }
    let limits = [2.0f32, 6.0, 12.0, 22.0, 12.0];
    for (k, y) in [80usize, 160, 240, 320, 400].iter().enumerate() {
        let mean = region_diff(&sai, &ours, *y);
        eprintln!("маркер, мазок {k}: средняя разница {mean:.1}");
        assert!(mean <= limits[k], "мазок {k}: средняя разница {mean} больше {}", limits[k]);
    }
}
