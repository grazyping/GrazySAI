//! «Растушёвка» против живой SAI 2026-07-02b (docs/41): чёрная вертикальная полоса x = 282…312
//! (карандаш 30 px) на прозрачном слое; мазок «Растушёвкой» (50 px, самая мягкая ступень,
//! плотность 90, нажим на размер и плотность, «Окрашивание» выключено, мышь, стабилизатор 3) по
//! y = 280 от x = 230 до 430. Значения — после наложения на белое, x = 270…450 через 6.

use paint_core::{BrushParams, Document, InputPoint, LiveStroke, Stabilizer, Stroke};

pub const SAI: [(i32, [u8; 31]); 4] = [
    (280, [255, 255, 255, 255, 255, 255, 255, 255, 252, 245, 231, 207, 177, 145, 118, 103, 103, 116, 139, 165, 189, 210, 226, 237, 246, 252, 255, 255, 255, 255, 255]),
    (270, [255, 255, 255, 255, 255, 249, 221, 162, 95, 45, 24, 43, 99, 165, 214, 240, 251, 254, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255]),
    (260, [255, 255, 252, 36, 1, 1, 1, 8, 227, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255]),
    (255, [255, 255, 199, 1, 1, 1, 1, 89, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255]),
];

pub fn run(params: BrushParams) -> Vec<(i32, Vec<u8>)> {
    let mut doc = Document::new(700, 560);
    let bar = Stroke::new(BrushParams { size: 30.0, hardness: 1.0, size_by_pressure: false, ..Default::default() });
    let mut bar = bar;
    for y in (5..=555).step_by(20) {
        bar.add_point(InputPoint { x: 297.0, y: y as f32, pressure: 1.0 });
    }
    doc.commit_stroke(0, &bar, [0, 0, 0]);
    let mut live = LiveStroke::new(vec![0], Stroke::new(params), [0, 0, 0]);
    let mut st = Stabilizer::new(3);
    for x in (230..=430).step_by(4) {
        let q = st.push(InputPoint { x: x as f32, y: 280.0, pressure: 1.0 });
        live.stroke.add_point(q);
        doc.live_update(&mut live);
    }
    for q in st.finish() {
        live.stroke.add_point(q);
    }
    doc.live_finish(live);
    let flat = doc.flatten();
    SAI.iter().map(|(y, _)| (*y, (0..31).map(|i| flat[((*y as usize) * 700 + 270 + 6 * i) * 4]).collect())).collect()
}

pub fn worst(rows: &[(i32, Vec<u8>)]) -> i32 {
    rows.iter().zip(SAI.iter()).map(|((_, o), (_, s))| o.iter().zip(s).map(|(a, b)| (*a as i32 - *b as i32).abs()).max().unwrap()).max().unwrap()
}

#[test]
#[ignore]
fn sweep_brush() {
    let env = |k: &str, d: f32| std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d);
    let mut p: BrushParams = paint_brush::ToolKind::Brush.new_brush().params;
    p.size = 50.0;
    p.hardness = 0.0;
    p.density = 0.9;
    p.min_size = 0.0;
    p.size_by_pressure = true;
    p.density_by_pressure = true;
    let m = p.mix.as_mut().unwrap();
    m.blending = env("B", 1.0);
    m.dilution = env("D", 1.0);
    m.persistence = env("P", 0.5);
    m.blend_by_pressure = env("BP", 0.0) != 0.0;
    let rows = run(p);
    for ((y, o), (_, s)) in rows.iter().zip(SAI.iter()) {
        eprintln!("y{y} ours {o:?}\n     sai  {s:?}");
    }
    eprintln!("WORST {}", worst(&rows));
}

#[test]
fn smudge_matches_live_sai_2026() {
    let rows = run(paint_brush::ToolKind::Smudge.new_brush().params);
    for ((y, o), (_, s)) in rows.iter().zip(SAI.iter()) {
        eprintln!("y{y} ours {o:?}\n     sai  {s:?}");
    }
    let w = worst(&rows);
    // модель переноса с запасом (не код SAI): до ~30 уровней у края пятна, в среднем — до 10
    assert!(w <= 30, "расхождение с SAI: {w}");
}

/// Мазок растушёвки — один шаг отмены: после отмены полоса как была.
#[test]
fn smudge_undo_restores() {
    let mut doc = Document::new(200, 120);
    let mut bar = Stroke::new(BrushParams { size: 20.0, hardness: 1.0, size_by_pressure: false, ..Default::default() });
    for y in (0..=120).step_by(10) {
        bar.add_point(InputPoint { x: 100.0, y: y as f32, pressure: 1.0 });
    }
    doc.commit_stroke(0, &bar, [0, 0, 0]);
    let before = doc.layers[0].pixels.clone();
    let mut live = LiveStroke::new(vec![0], Stroke::new(paint_brush::ToolKind::Smudge.new_brush().params), [0, 0, 0]);
    for x in (60..=160).step_by(4) {
        live.stroke.add_point(InputPoint { x: x as f32, y: 60.0, pressure: 1.0 });
        doc.live_update(&mut live);
    }
    doc.live_finish(live);
    assert_ne!(doc.layers[0].pixels, before, "растушёвка ничего не сделала");
    doc.undo();
    assert_eq!(doc.layers[0].pixels, before, "отмена не вернула");
}
