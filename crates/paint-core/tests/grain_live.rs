//! Форма «пятно» и текстура кисти против живой SAI 2017 (`docs/29-brush-form-texture.md`).
//! Карандаш 20 px, плотность 100, мышью (нажим 1) — горизонтальные линии x 40..560 шагом 4:
//! y 50 — текстура Canvas 100; y 110 — Canvas 56; y 200, 280, 360 — форма Blots 100 % на самой
//! жёсткой, самой мягкой и средней ступени. Текстуры — файлы SAI, в репозиторий не входят:
//! тест ищет их в `SOSAI_SAI_DIR` (папка с `blotmap` и `brushtex`) и без них пропускается.

use paint_core::paper::PaperTexture;
use paint_core::stroke::Grain;
use paint_core::{BrushParams, Document, InputPoint, LiveStroke, Stroke};
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn sai_dir() -> Option<PathBuf> {
    let d = std::env::var_os("SOSAI_SAI_DIR").map(PathBuf::from)?;
    d.join("brushtex").is_dir().then_some(d)
}

fn tex(dir: &Path, sub: &str, name: &str) -> Arc<PaperTexture> {
    Arc::new(PaperTexture::from_bmp(&std::fs::read(dir.join(sub).join(format!("{name}.bmp"))).unwrap()).unwrap())
}

/// Доля краски по синему каналу: фон 255, цвет кисти 58.
fn alpha(img: &[u8], w: usize, x: usize, y: usize) -> f32 {
    (255.0 - img[(y * w + x) * 4 + 2] as f32) / (255.0 - 58.0)
}

fn render(dir: &Path, off: f32) -> Vec<u8> {
    let canvas = tex(dir, "brushtex", "Canvas");
    let blots = tex(dir, "blotmap", "Blots");
    let rows: [(f32, f32, Grain); 5] = [
        (50.0, 1.0, Grain { blot: None, texture: Some((canvas.clone(), 1.0, 100)), paper: false, by_min_density: true }),
        (110.0, 1.0, Grain { blot: None, texture: Some((canvas.clone(), 0.56, 100)), paper: false, by_min_density: true }),
        (200.0, 1.0, Grain { blot: Some((blots.clone(), 1.0, 100)), texture: None, paper: false, by_min_density: true }),
        (280.0, 0.0, Grain { blot: Some((blots.clone(), 1.0, 100)), texture: None, paper: false, by_min_density: true }),
        (360.0, 0.75, Grain { blot: Some((blots, 1.0, 100)), texture: None, paper: false, by_min_density: true }),
    ];
    let mut doc = Document::new(600, 500);
    for (y, hardness, grain) in rows {
        let params = BrushParams { size: 20.0, hardness, ..Default::default() };
        let mut stroke = Stroke::new(params);
        stroke.set_grain(Some(grain));
        let mut live = LiveStroke::new(vec![0], stroke, [0, 38, 58]);
        let mut x = 40;
        loop {
            live.stroke.add_point(InputPoint { x: x as f32 + off, y: y + off, pressure: 1.0 });
            doc.live_update(&mut live);
            if x >= 560 {
                break;
            }
            x += 4;
        }
        doc.live_finish(live);
    }
    doc.flatten()
}

/// Средняя разница доли краски по полосе ±14 px от оси, x 60..540.
fn row_diff(a: &[u8], b: &[u8], y: usize) -> f32 {
    let mut sum = 0.0;
    for yy in y - 14..=y + 14 {
        for x in 60..540 {
            sum += (alpha(a, 600, x, yy) - alpha(b, 600, x, yy)).abs();
        }
    }
    sum / (29.0 * 480.0)
}

#[test]
fn form_and_texture_like_the_reference_program() {
    let Some(dir) = sai_dir() else {
        eprintln!("SOSAI_SAI_DIR не задана — пропуск");
        return;
    };
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/brush/sai-grain.png");
    let (_, _, sai) = paint_core::io::load_png(&path).unwrap();
    // мышь в SAI попадает в центр точки экрана
    let off = 0.5;
    let ours = render(&dir, off);
    if let Ok(out) = std::env::var("DUMP") {
        paint_core::io::save_png(Path::new(&out), 600, 500, &ours).unwrap();
    }
    let limits = [0.025, 0.02, 0.01, 0.02, 0.01];
    let mut bad = Vec::new();
    for (i, y) in [50, 110, 200, 280, 360].into_iter().enumerate() {
        let d = row_diff(&ours, &sai, y);
        eprintln!("y {y}: разница {d:.4}");
        if d > limits[i] {
            bad.push(y);
        }
    }
    assert!(bad.is_empty(), "не совпали ряды {bad:?}");
}
