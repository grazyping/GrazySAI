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

pub fn render() -> Vec<u8> {
    let pencil = BrushParams { size: 30.0, hardness: 1.0, ..Default::default() };
    let brush = BrushParams {
        size: 50.0,
        min_size: 0.5,
        hardness: 1.0,
        density_by_pressure: true,
        mix: Some(MixParams::new(0.0, 0.0, 0.5)),
        ..Default::default()
    };
    let mut doc = Document::new(512, 512);
    let mut live: Option<LiveStroke> = None;
    let mut st = Stabilizer::new(3);
    let mut strokes = 0;
    for line in std::fs::read_to_string(data("brush-strokes.csv")).unwrap().lines().skip(1) {
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

fn gray(img: &[u8], x: usize, y: usize) -> f32 {
    let i = (y * 512 + x) * 4;
    0.299 * img[i] as f32 + 0.587 * img[i + 1] as f32 + 0.114 * img[i + 2] as f32
}

#[test]
fn brush_drags_paint_like_the_reference_program() {
    let (_, _, sai) = paint_core::io::load_png(&data("sai-brush-default.png")).unwrap();
    let ours = render();
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
