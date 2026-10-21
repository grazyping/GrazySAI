//! «Размытие» против живой SAI 2026-07-02b (docs/41): полоса чёрного до x = 446 включительно (как
//! лист SAI), мазок «Размытием» (ступень самая мягкая, плотность 100, ширина размытия 50 %, мышь,
//! стабилизатор 3) по y = 280 от x = 350 до 550. Профили — значения после наложения на белое.

use paint_core::{BrushParams, Document, InputPoint, LiveStroke, Stabilizer, Stroke};

/// (размер, x от, шаг, [(y, значения)]).
type Case = (f32, i32, i32, &'static [(i32, [u8; 17])]);

const CASES: [Case; 2] = [
    (50.0, 416, 4, &[
        (280, [4, 7, 13, 26, 46, 71, 98, 126, 154, 178, 197, 213, 225, 234, 240, 245, 248]),
        (270, [4, 6, 12, 24, 43, 67, 94, 122, 156, 180, 200, 216, 227, 236, 242, 246, 249]),
        (260, [2, 3, 6, 13, 24, 39, 58, 81, 189, 208, 222, 233, 241, 247, 250, 251, 253]),
        (250, [2, 2, 3, 5, 7, 10, 15, 23, 246, 249, 251, 253, 254, 254, 255, 255, 255]),
    ]),
    (100.0, 384, 8, &[
        (280, [2, 4, 9, 21, 40, 64, 91, 120, 149, 173, 194, 210, 223, 233, 240, 244, 248]),
        (260, [1, 3, 7, 17, 34, 57, 84, 113, 152, 178, 199, 215, 227, 236, 242, 246, 249]),
        (240, [0, 1, 3, 7, 15, 28, 43, 63, 196, 213, 226, 236, 243, 248, 251, 252, 253]),
        (230, [0, 0, 1, 3, 7, 12, 20, 31, 230, 238, 243, 248, 251, 253, 254, 254, 255]),
    ]),
];

#[test]
fn blur_tool_matches_live_sai_2026() {
    let mut worst = 0i32;
    for (size, x0, step, rows) in CASES {
        let mut doc = Document::new(700, 560);
        for y in 0..560 {
            for x in 0..447 {
                doc.layers[0].pixels.set(x, y, [0, 0, 0, paint_core::pixel::ONE as u16]);
            }
        }
        let mut params: BrushParams = paint_brush::ToolKind::Blur.new_brush().params;
        params.size = size;
        let mut live = LiveStroke::new(vec![0], Stroke::new(params), [0, 0, 0]);
        let mut st = Stabilizer::new(3);
        for x in (350..=550).step_by(4) {
            let q = st.push(InputPoint { x: x as f32, y: 280.0, pressure: 1.0 });
            live.stroke.add_point(q);
            doc.live_update(&mut live);
        }
        for q in st.finish() {
            live.stroke.add_point(q);
        }
        doc.live_finish(live);
        let at = |x: i32, y: i32| 255 - paint_core::pixel::to_rgba8(doc.layers[0].pixels.get(x, y))[3];
        for (y, sai) in rows {
            let ours: Vec<u8> = (0..17).map(|i| at(x0 + step * i, *y)).collect();
            let d = ours.iter().zip(sai).map(|(a, b)| (*a as i32 - *b as i32).abs()).max().unwrap();
            worst = worst.max(d);
            eprintln!("size {size} y{y} ours {ours:?}\n               sai  {sai:?}  max diff {d}");
        }
    }
    // середина мазка — до 4–7 уровней, у края мазка (самая мягкая ступень) — до 16
    assert!(worst <= 16, "расхождение с SAI: {worst}");
}
