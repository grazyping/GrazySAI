//! Мазки щетиной в PNG для сверки на глаз с живой SAI (GRAZYSAI_PAPERTEX — папка papertex SAI,
//! рядом папка bristle; GRAZYSAI_BRISTLE_OUT — куда писать). Без них тест пропускается.
use paint_core::stroke::{Bristle, Grain};
use paint_core::{BrushParams, Document, InputPoint, Stroke};

#[test]
fn bristle_strokes_to_png() {
    let (Some(_), Some(out)) = (std::env::var_os("GRAZYSAI_PAPERTEX"), std::env::var_os("GRAZYSAI_BRISTLE_OUT")) else { return };
    let hairs = paint_core::paper::bristle_of("Bristle").expect("Bristle.bmp");
    let mut doc = Document::new(520, 380);
    let params = BrushParams { size: 50.0, min_size: 0.5, density_by_pressure: true, hardness: 0.0, ..Default::default() };
    let grain = Grain { blot: None, texture: None, paper: false, by_min_density: true, bristle: Some(Bristle { hairs, hair_size: 0.5, hair_min: 0.0, randomize: 0.0, direction: 1 }) };
    for (a, b) in [((40.0f32, 60.0f32), (440.0f32, 60.0f32)), ((40.0, 160.0), (340.0, 360.0))] {
        let mut s = Stroke::new(params);
        s.set_grain(Some(grain.clone()));
        for i in 0..=40 {
            let t = i as f32 / 40.0;
            s.add_point(InputPoint { x: a.0 + (b.0 - a.0) * t, y: a.1 + (b.1 - a.1) * t, pressure: 1.0 });
        }
        doc.commit_stroke(0, &s, [88, 88, 160]);
    }
    paint_core::io::save_png(std::path::Path::new(&out), 520, 380, &doc.flatten()).unwrap();
}
