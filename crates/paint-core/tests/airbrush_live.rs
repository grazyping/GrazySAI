//! Аэрограф против живой SAI 2017 (виртуальный планшет через Wintab). У образца Аэрограф рисует
//! тем же движком, что Карандаш; отличаются значения по умолчанию: самая мягкая ступень, 50 px,
//! мин. размер 50 %, плотность 100, нажим на размер и на плотность (панель SAI это подтвердила).
//! Мазки с нажимом 1,0 / 0,75 / 0,5 / 0,25 и с растущим 0,1 → 1,0.

use paint_core::stabilizer::Stabilizer;
use paint_core::{BrushParams, Document, InputPoint, Stroke};
use std::path::Path;

fn data(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/airbrush").join(name)
}

fn render() -> Vec<u8> {
    let params = BrushParams {
        size: 50.0,
        min_size: 0.5,
        hardness: 0.0,
        density_by_pressure: true,
        ..Default::default()
    };
    let mut doc = Document::new(512, 512);
    let mut stroke: Option<Stroke> = None;
    let mut st = Stabilizer::new(3);
    for line in std::fs::read_to_string(data("airbrush-strokes.csv")).unwrap().lines().skip(1) {
        let f: Vec<&str> = line.split(',').collect();
        let x = f[1].parse::<f32>().unwrap() * 1920.0 - 662.0;
        let y = f[2].parse::<f32>().unwrap() * 1080.0 - 204.0;
        let p = f[3].parse::<f32>().unwrap();
        if x < 0.0 {
            continue; // касание по панели
        }
        if f[4] == "up" {
            if let Some(mut s) = stroke.take() {
                for q in st.finish() {
                    s.add_point(q);
                }
                doc.commit_stroke(0, &s, [0, 0, 0]);
            }
            st = Stabilizer::new(3);
        } else {
            let q = st.push(InputPoint { x, y, pressure: p });
            stroke.get_or_insert_with(|| Stroke::new(params)).add_point(q);
        }
    }
    doc.flatten()
}

fn gray(img: &[u8], x: usize, y: usize) -> f32 {
    let i = (y * 512 + x) * 4;
    0.299 * img[i] as f32 + 0.587 * img[i + 1] as f32 + 0.114 * img[i + 2] as f32
}

#[test]
fn airbrush_matches_the_reference_program() {
    let (_, _, sai) = paint_core::io::load_png(&data("sai-airbrush-default.png")).unwrap();
    let ours = render();
    if let Ok(out) = std::env::var("AIR_LIVE_OUT") {
        paint_core::io::save_png(Path::new(&out), 512, 512, &ours).unwrap();
    }
    // Сечение посередине мазка (x = 250) и средняя разница по середине мазка (x 120..400).
    let limits = [3.0f32, 3.0, 3.0, 2.0, 5.0];
    for (k, y) in [70usize, 160, 250, 340, 440].iter().enumerate() {
        let a: Vec<i32> = (0..17).map(|i| gray(&sai, 250, y - 32 + i * 4) as i32).collect();
        let b: Vec<i32> = (0..17).map(|i| gray(&ours, 250, y - 32 + i * 4) as i32).collect();
        let mut sum = 0.0;
        for yy in y - 32..=y + 32 {
            for x in 120..400 {
                sum += (gray(&sai, x, yy) - gray(&ours, x, yy)).abs();
            }
        }
        let mean = sum / (65.0 * 280.0);
        eprintln!("аэрограф, мазок {k}: средняя разница {mean:.1}\n  SAI {a:?}\n  наш {b:?}");
        assert!(mean <= limits[k], "мазок {k}: средняя разница {mean} больше {}", limits[k]);
    }
}
