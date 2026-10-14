//! Смешивающая кисть: краска берёт цвет холста и тянет его за собой (`src/mix.rs`).

use paint_core::mix::MixParams;
use paint_core::pixel::{from_rgba8, to_rgba8};
use paint_core::{BrushParams, Document, InputPoint, LiveStroke, Stroke};

const BLUE: [u8; 3] = [0, 0, 255];

/// Холст 200×60: белый слой, на нём красная полоса x 40..60. Синий мазок слева направо по y = 30.
fn smear(mix: MixParams, pressure: f32) -> Document {
    let mut doc = Document::new(200, 60);
    let red = from_rgba8([255, 0, 0, 255]);
    let white = from_rgba8([255, 255, 255, 255]);
    for y in 0..60 {
        for x in 0..200 {
            doc.layers[0].pixels.set(x, y, if (40..60).contains(&x) { red } else { white });
        }
    }
    let params = BrushParams { size: 16.0, hardness: 1.0, size_by_pressure: false, mix: Some(mix), ..Default::default() };
    let mut live = LiveStroke::new(vec![0], Stroke::new(params), BLUE);
    for i in 0..=160 {
        live.stroke.add_point(InputPoint { x: 20.0 + i as f32, y: 30.0, pressure });
        doc.live_update(&mut live);
    }
    doc.live_finish(live);
    doc
}

fn at(doc: &Document, x: i32) -> [u8; 4] {
    to_rgba8(doc.layers[0].pixels.get(x, 30))
}

#[test]
fn no_blending_paints_the_brush_colour() {
    let doc = smear(MixParams { blend_by_pressure: false, ..MixParams::new(0.0, 0.0, 0.0) }, 1.0);
    for x in [30, 50, 100, 170] {
        assert_eq!(at(&doc, x), [0, 0, 255, 255], "x = {x}");
    }
}

#[test]
fn blending_drags_the_stripe_colour_along_the_stroke() {
    let doc = smear(MixParams { blend_by_pressure: false, ..MixParams::new(0.8, 0.0, 0.8) }, 1.0);
    let after = at(&doc, 70);
    let far = at(&doc, 170);
    assert!(after[0] > 100 && after[0] > after[2] / 2, "сразу за полосой краска красноватая: {after:?}");
    assert!(far[0] < after[0], "дальше красного меньше: {far:?} против {after:?}");
    assert_eq!(at(&doc, 5), [255, 255, 255, 255], "вне мазка холст не тронут");
}

#[test]
fn light_pressure_smudges_without_adding_colour() {
    let doc = smear(MixParams::new(0.0, 0.0, 0.5), 0.3);
    let px = at(&doc, 30);
    assert!(px[2] < 10 || (px[0] > 200 && px[1] > 200), "до полосы синего нет: {px:?}");
    assert!(at(&doc, 70)[0] > 200, "красный размазан вправо: {:?}", at(&doc, 70));
}

#[test]
fn mixed_stroke_is_one_undo_step() {
    let mut doc = smear(MixParams::new(0.3, 0.3, 0.5), 1.0);
    assert!(doc.undo().is_some());
    assert_eq!(at(&doc, 100), [255, 255, 255, 255]);
    assert_eq!(at(&doc, 50), [255, 0, 0, 255]);
}

/// Лист для глаза: `MIX_SHEET=файл.png cargo test -p paint-core --test mixing -- sheet`.
#[test]
fn sheet() {
    let Ok(out) = std::env::var("MIX_SHEET") else { return };
    let rows = [
        MixParams { blend_by_pressure: false, ..MixParams::new(0.0, 0.0, 0.5) },
        MixParams { blend_by_pressure: false, ..MixParams::new(0.5, 0.0, 0.5) },
        MixParams { blend_by_pressure: false, ..MixParams::new(0.8, 0.0, 0.8) },
        MixParams { blend_by_pressure: false, ..MixParams::new(0.3, 0.3, 0.5) },
        MixParams { blend_by_pressure: false, ..MixParams::new(0.5, 0.8, 0.5) },
        MixParams::new(0.0, 0.0, 0.5),
    ];
    let mut img = vec![0u8; 200 * 60 * rows.len() * 4];
    for (r, mix) in rows.iter().enumerate() {
        let doc = smear(*mix, if r == 5 { 0.6 } else { 1.0 });
        for y in 0..60 {
            for x in 0..200i32 {
                let i = ((r * 60 + y) * 200 + x as usize) * 4;
                img[i..i + 4].copy_from_slice(&to_rgba8(doc.layers[0].pixels.get(x, y as i32)));
            }
        }
    }
    paint_core::io::save_png(std::path::Path::new(&out), 200, 60 * rows.len() as u32, &img).unwrap();
}
