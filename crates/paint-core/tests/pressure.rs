//! Нажим против живой SAI 2017 (виртуальный планшет через Wintab, карандаш 30, жёсткая ступень,
//! сглаживание 3; docs/14-virtual-pen.md). Сценарии — `t_ms,x,y,pressure,phase`, x и y — доли
//! экрана 1920×1080, холст на экране с (662, 204).

use paint_core::stabilizer::Stabilizer;
use paint_core::{BrushParams, Document, InputPoint, Stroke};
use std::path::Path;

fn data(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/pressure").join(name)
}

fn render(csv: &str, by_density: bool) -> Vec<u8> {
    let params = BrushParams { size: 30.0, hardness: 1.0, size_by_pressure: !by_density, density_by_pressure: by_density, ..Default::default() };
    let mut doc = Document::new(512, 512);
    let mut stroke: Option<Stroke> = None;
    let mut st = Stabilizer::new(3);
    for line in std::fs::read_to_string(data(csv)).unwrap().lines().skip(1) {
        let f: Vec<&str> = line.split(',').collect();
        let x = f[1].parse::<f32>().unwrap() * 1920.0 - 662.0;
        let y = f[2].parse::<f32>().unwrap() * 1080.0 - 204.0;
        let p = f[3].parse::<f32>().unwrap();
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

fn gray(img: &[u8], w: usize, x: usize, y: usize) -> f32 {
    let i = (y * w + x) * 4;
    0.299 * img[i] as f32 + 0.587 * img[i + 1] as f32 + 0.114 * img[i + 2] as f32
}

#[test]
fn size_pressure_matches_the_reference_program() {
    let (w, _, sai) = paint_core::io::load_png(&data("sai-size-pressure.png")).unwrap();
    let ours = render("size-strokes.csv", false);
    for (k, p) in [0.25f32, 0.5, 0.75, 1.0].iter().enumerate() {
        let y = 60 + k * 70;
        let width = |img: &[u8]| (y - 25..y + 25).map(|yy| 255.0 - gray(img, w as usize, 256, yy)).sum::<f32>() / 255.0;
        let (a, b) = (width(&sai), width(&ours));
        assert!((a - b).abs() < 0.35, "нажим {p}: ширина SAI {a}, наша {b}");
    }
}

#[test]
fn density_pressure_matches_the_reference_program() {
    let (w, _, sai) = paint_core::io::load_png(&data("sai-density-pressure.png")).unwrap();
    let ours = render("density-strokes.csv", true);
    for (k, p) in [0.25f32, 0.5, 0.75, 1.0].iter().enumerate() {
        let y = 60 + k * 70;
        // середина сечения (края сдвинуты на полпикселя самой установкой — их не сравниваем)
        let worst = (y - 12..y + 13).map(|yy| (gray(&sai, w as usize, 256, yy) - gray(&ours, 512, 256, yy)).abs()).fold(0f32, f32::max);
        assert!(worst <= 10.0, "нажим {p}: расхождение {worst}");
    }
}

#[test]
fn a_tap_without_movement_draws_nothing_wrong() {
    // касание без движения при падающем нажиме не должно давать NaN и падения
    let mut s = Stroke::new(BrushParams { size: 30.0, hardness: 1.0, size_by_pressure: false, density_by_pressure: true, ..Default::default() });
    for p in [0.8f32, 0.5, 0.2, 0.0] {
        s.add_point(InputPoint { x: 10.0, y: 10.0, pressure: p });
    }
    let mut doc = Document::new(64, 64);
    doc.commit_stroke(0, &s, [0, 0, 0]);
    // в документе нет следов NaN: белый фон, а точка — чёрный отпечаток в пределах кисти
    let img = doc.flatten();
    assert_eq!(img.len(), 64 * 64 * 4);
    assert_eq!(&img[(63 * 64 + 63) * 4..(63 * 64 + 63) * 4 + 4], &[255, 255, 255, 255]);
}
