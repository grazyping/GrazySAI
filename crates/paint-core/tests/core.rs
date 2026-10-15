use paint_core::blend::{channel, composite};
use paint_core::pixel::{from_rgba8, to_rgba8, ONE};
use paint_core::*;

fn line(stroke: &mut Stroke, from: (f32, f32), to: (f32, f32), pressure: f32) {
    stroke.add_point(InputPoint { x: from.0, y: from.1, pressure });
    stroke.add_point(InputPoint { x: to.0, y: to.1, pressure });
}

fn pixel(doc: &Document, x: u32, y: u32) -> [u8; 4] {
    let img = doc.flatten();
    let o = ((y * doc.width + x) * 4) as usize;
    [img[o], img[o + 1], img[o + 2], img[o + 3]]
}

#[test]
fn rgba8_roundtrip_is_exact_for_opaque_and_stable_for_translucent() {
    for v in [0u8, 1, 2, 127, 128, 200, 254, 255] {
        assert_eq!(to_rgba8(from_rgba8([v, 255 - v, 77, 255])), [v, 255 - v, 77, 255]);
    }
    let c = to_rgba8(from_rgba8([200, 100, 50, 128]));
    assert_eq!(c[3], 128);
    for (got, want) in c.iter().zip([200u8, 100, 50]) {
        assert!((*got as i32 - want as i32).abs() <= 1, "{c:?}");
    }
}

#[test]
fn blend_identities() {
    for b in [0.0f32, 0.2, 0.5, 0.9, 1.0] {
        assert!((channel(BlendMode::Multiply, b, 1.0) - b).abs() < 1e-6);
        assert!((channel(BlendMode::Screen, b, 0.0) - b).abs() < 1e-6);
        assert!((channel(BlendMode::Overlay, b, 0.5) - b).abs() < 1e-6);
        assert!((channel(BlendMode::SoftLight, b, 0.5) - b).abs() < 1e-6);
        assert!((channel(BlendMode::Difference, b, 0.0) - b).abs() < 1e-6);
        assert!((channel(BlendMode::LinearDodge, b, 0.0) - b).abs() < 1e-6);
        assert!((channel(BlendMode::Divide, b, 1.0) - b).abs() < 1e-6);
    }
    for mode in BlendMode::ALL {
        for b in [0.0f32, 0.3, 1.0] {
            for s in [0.0f32, 0.6, 1.0] {
                let v = channel(mode, b, s);
                assert!((0.0..=1.0).contains(&v), "{mode:?} {b} {s} -> {v}");
            }
        }
    }
}

#[test]
fn composite_respects_opacity_and_transparency() {
    let dst = [0.2, 0.4, 0.6, 1.0];
    let src = [1.0, 0.0, 0.0, 1.0];
    assert_eq!(composite(BlendMode::Normal, dst, src, 0.0), dst);
    assert_eq!(composite(BlendMode::Normal, dst, src, 1.0), src);
    // Любой режим поверх прозрачного — это просто сам слой.
    for mode in BlendMode::ALL {
        let out = composite(mode, [0.0; 4], [0.5, 0.25, 0.0, 0.5], 1.0);
        assert_eq!(out, [0.5, 0.25, 0.0, 0.5], "{mode:?}");
    }
    let half = composite(BlendMode::Multiply, [1.0, 1.0, 1.0, 1.0], [0.5, 0.5, 0.5, 1.0], 0.5);
    assert!((half[0] - 0.75).abs() < 1e-6);
}

#[test]
fn dab_is_round_and_symmetric() {
    // Жёсткая кисть диаметром 20: сплошная до радиуса ~8, край шириной около 2,8 px.

    let mut s = Stroke::new(BrushParams { size: 20.0, size_by_pressure: false, ..Default::default() });
    s.add_point(InputPoint { x: 50.0, y: 50.0, pressure: 1.0 });
    for (dx, dy) in [(3, 7), (9, 2), (0, 9), (6, 6)] {
        let a = s.mask.get(50 + dx, 50 + dy);
        assert_eq!(a, s.mask.get(49 - dx, 50 + dy));
        assert_eq!(a, s.mask.get(50 + dx, 49 - dy));
        assert_eq!(a, s.mask.get(49 - dx, 49 - dy));
    }
    // Одиночный отпечаток полупрозрачен (у образца 0,373 при размере 20): плотность набирает мазок.
    let centre = s.mask.get(50, 50) as f32 / ONE as f32;
    assert!((centre - 0.373).abs() < 0.008, "{centre}");
    assert_eq!(s.mask.get(50 + 12, 50), 0);
}

#[test]
fn stroke_never_exceeds_density_and_has_no_gaps() {
    let params = BrushParams { size: 10.0, density: 0.5, size_by_pressure: false, ..Default::default() };
    let mut s = Stroke::new(params);
    line(&mut s, (10.0, 30.5), (200.0, 30.5), 1.0);
    // Проходим тем же путём ещё раз — темнее стать не должно.
    line(&mut s, (200.0, 30.5), (10.0, 30.5), 1.0);
    let limit = (0.5 * ONE as f32 + 0.5) as u16;
    // Вдали от концов мазка плотность ровно заданная; у самых концов отпечатков меньше — там светлее.
    for x in 30..180 {
        assert_eq!(s.mask.get(x, 30), limit, "x={x}");
    }
    assert!(s.mask.get(10, 30) <= limit && s.mask.get(10, 30) > 0);
    assert!(s.dab_count > 100, "{}", s.dab_count);
}

#[test]
fn pressure_changes_width() {
    let mut s = Stroke::new(BrushParams { size: 30.0, ..Default::default() });
    s.add_point(InputPoint { x: 20.0, y: 100.0, pressure: 0.1 });
    s.add_point(InputPoint { x: 220.0, y: 100.0, pressure: 1.0 });
    let width_at = |x: i32| (60..140).filter(|&y| s.mask.get(x, y) > 0).count();
    assert!(width_at(40) < width_at(120));
    assert!(width_at(120) < width_at(200));
}

#[test]
fn undo_and_redo_restore_pixels_exactly() {
    let mut doc = Document::new(128, 128);
    let before = doc.flatten();
    let mut s = Stroke::new(BrushParams { size: 16.0, size_by_pressure: false, ..Default::default() });
    line(&mut s, (20.0, 20.0), (100.0, 90.0), 1.0);
    doc.commit_stroke(0, &s, [200, 30, 30]);
    let painted = doc.flatten();
    assert_ne!(before, painted);
    assert_eq!(pixel(&doc, 60, 55), [200, 30, 30, 255]);

    assert!(doc.undo().is_some());
    assert_eq!(doc.flatten(), before);
    assert!(doc.layers[0].pixels.tiles.is_empty());
    assert!(doc.redo().is_some());
    assert_eq!(doc.flatten(), painted);
    assert!(doc.redo().is_none());
}

#[test]
fn eraser_and_preserve_opacity() {
    let mut doc = Document::new(128, 64);
    doc.background = None;
    let hard = BrushParams { size: 20.0, size_by_pressure: false, ..Default::default() };
    let mut s = Stroke::new(hard);
    line(&mut s, (10.0, 32.0), (118.0, 32.0), 1.0);
    doc.commit_stroke(0, &s, [0, 0, 255]);

    let mut e = Stroke::new(BrushParams { erase: true, ..hard });
    line(&mut e, (64.0, 0.0), (64.0, 64.0), 1.0);
    doc.commit_stroke(0, &e, [0, 0, 0]);
    assert_eq!(pixel(&doc, 64, 32)[3], 0);
    assert_eq!(pixel(&doc, 30, 32), [0, 0, 255, 255]);

    doc.layers[0].preserve_opacity = true;
    let mut p = Stroke::new(BrushParams { size: 40.0, ..hard });
    line(&mut p, (30.0, 0.0), (30.0, 64.0), 1.0);
    doc.commit_stroke(0, &p, [255, 0, 0]);
    assert_eq!(pixel(&doc, 30, 32), [255, 0, 0, 255]); // перекрасило существующую краску
    assert_eq!(pixel(&doc, 30, 5)[3], 0); // а пустое место осталось пустым
}

#[test]
fn layers_blend_in_order() {
    let mut doc = Document::new(64, 64);
    let brush = BrushParams { size: 40.0, size_by_pressure: false, ..Default::default() };
    let mut a = Stroke::new(brush);
    line(&mut a, (-10.0, 32.0), (74.0, 32.0), 1.0);
    doc.commit_stroke(0, &a, [255, 255, 0]);
    let top = doc.add_layer("Умножение");
    doc.layers[top].mode = BlendMode::Multiply;
    doc.commit_stroke(top, &a, [0, 255, 255]);
    assert_eq!(pixel(&doc, 32, 32), [0, 255, 0, 255]); // жёлтый × голубой = зелёный
    doc.layers[top].visible = false;
    assert_eq!(pixel(&doc, 32, 32), [255, 255, 0, 255]);
    assert_eq!(pixel(&doc, 2, 2), [255, 255, 255, 255]); // фон
}

#[test]
fn stabilizer_smooths_and_reaches_the_end() {
    let mut st = Stabilizer::new(8);
    let mut max_dev: f32 = 0.0;
    let mut last = InputPoint { x: 0.0, y: 0.0, pressure: 1.0 };
    for i in 0..200 {
        let jitter = if i % 2 == 0 { 3.0 } else { -3.0 };
        last = st.push(InputPoint { x: i as f32, y: 50.0 + jitter, pressure: 1.0 });
        if i > 20 {
            max_dev = max_dev.max((last.y - 50.0).abs());
        }
    }
    assert!(max_dev < 0.5, "дрожание осталось: {max_dev}");
    assert!(last.x < 199.0); // сглаженная точка отстаёт
    let tail = st.finish();
    assert_eq!(tail.last().unwrap().x, 199.0); // но в конце догоняет перо
}

#[test]
fn live_stroke_matches_committed_stroke_and_undoes() {
    let params = BrushParams { size: 18.0, hardness: 0.3, ..Default::default() };
    let points: Vec<InputPoint> = (0..60).map(|i| InputPoint { x: 10.0 + i as f32 * 3.0, y: 40.0 + (i as f32 * 0.3).sin() * 20.0, pressure: 0.3 + i as f32 / 100.0 }).collect();

    let mut a = Document::new(256, 128);
    let mut whole = Stroke::new(params);
    points.iter().for_each(|p| whole.add_point(*p));
    a.commit_stroke(0, &whole, [10, 120, 200]);

    // То же самое по частям, с показом на слое после каждой точки.
    let mut b = Document::new(256, 128);
    let before = b.flatten();
    let mut live = LiveStroke::new(vec![0], Stroke::new(params), [10, 120, 200]);
    let mut changed = 0;
    for p in &points {
        live.stroke.add_point(*p);
        changed += b.live_update(&mut live).len();
    }
    assert!(changed > 0);
    b.live_finish(live);
    assert_eq!(a.flatten(), b.flatten());
    assert!(b.undo().is_some());
    assert_eq!(b.flatten(), before);
}

#[test]
fn folders_and_clipping() {
    let brush = BrushParams { size: 30.0, size_by_pressure: false, ..Default::default() };
    let dot = |x: f32| {
        let mut s = Stroke::new(brush);
        line(&mut s, (x, -10.0), (x, 74.0), 1.0);
        s
    };
    let mut doc = Document::new(96, 64);
    doc.commit_stroke(0, &dot(32.0), [255, 0, 0]); // красный круг слева
    let mut clip = Layer::new("обтравка");
    clip.clipping = true;
    clip.apply_stroke(&dot(48.0), [0, 0, 255], &Default::default()); // синий круг правее, заходит на красный
    doc.layers.push(clip);
    assert_eq!(pixel(&doc, 40, 32), [0, 0, 255, 255]); // пересечение — синее
    assert_eq!(pixel(&doc, 22, 32), [255, 0, 0, 255]); // только красный
    assert_eq!(pixel(&doc, 60, 32), [255, 255, 255, 255]); // вне основы синего не видно

    // Папка с половинной непрозрачностью ослабляет всё своё содержимое.
    let mut inner = Layer::new("внутри");
    inner.apply_stroke(&dot(80.0), [0, 0, 0], &Default::default());
    let mut folder = Layer::folder("папка", vec![inner]);
    folder.opacity = 0.5;
    doc.layers.push(folder);
    let p = pixel(&doc, 80, 32);
    assert!((126..=129).contains(&p[0]), "{p:?}");
    doc.layers[2].visible = false;
    assert_eq!(pixel(&doc, 80, 32), [255, 255, 255, 255]);
    // Рисование по пути внутрь папки.
    doc.layers[2].visible = true;
    assert!(doc.layer_mut(&[2, 0]).is_some());
    assert!(doc.layer(&[2, 1]).is_none());
}

#[test]
fn soft_stroke_is_smooth_along_its_axis() {
    // У самой мягкой кисти шаг крупный; покрытие вдоль оси при этом не должно идти «бусами».
    let params = BrushParams { size: 40.0, hardness: 0.0, density: 0.6, size_by_pressure: false, ..Default::default() };
    let mut s = Stroke::new(params);
    line(&mut s, (20.0, 60.5), (420.0, 60.5), 1.0);
    let values: Vec<i32> = (120..320).map(|x| s.mask.get(x, 60) as i32).collect();
    let (lo, hi) = (values.iter().min().unwrap(), values.iter().max().unwrap());
    assert!(hi - lo <= 32768 / 100, "перепад {} из 32768", hi - lo);
    let target = (0.6 * ONE as f32) as i32;
    assert!((hi - target).abs() <= 32768 / 100, "плотность {hi} вместо {target}");
}

#[test]
fn blend_modes_match_the_reference_program() {
    use paint_core::blend::composite;
    use BlendMode::*;
    let base = [205u8, 155, 129, 104, 53, 255];
    let cases: [(BlendMode, [u8; 6], [u8; 6]); 28] = [
        (Normal, [129, 129, 129, 129, 129, 129], [167, 142, 129, 116, 91, 192]),
        (Multiply, [104, 78, 65, 52, 27, 129], [154, 116, 97, 78, 40, 192]),
        (Screen, [230, 205, 193, 180, 155, 255], [218, 180, 161, 142, 104, 255]),
        (Overlay, [206, 156, 131, 105, 53, 255], [205, 155, 130, 104, 53, 255]),
        (Shade, [79, 29, 3, 0, 0, 129], [142, 92, 66, 41, 0, 192]),
        (Shine, [255, 255, 255, 233, 182, 255], [255, 219, 194, 168, 117, 255]),
        (ShadeShine, [208, 158, 132, 107, 56, 255], [206, 156, 131, 105, 54, 255]),
        (Burn, [156, 56, 7, 0, 0, 255], [188, 122, 88, 54, 0, 255]),
        (Dodge, [255, 255, 255, 210, 107, 255], [255, 207, 173, 139, 71, 255]),
        (BurnDodge, [207, 156, 131, 105, 53, 255], [206, 155, 130, 104, 53, 255]),
        (Darken, [129, 129, 129, 104, 53, 129], [167, 142, 129, 104, 53, 192]),
        (ColorBurn, [156, 56, 7, 0, 0, 255], [180, 105, 68, 52, 26, 255]),
        (LinearBurn, [79, 29, 3, 0, 0, 129], [142, 92, 66, 52, 26, 192]),
        (DarkerColor, [129, 129, 129, 104, 53, 129], [167, 142, 129, 104, 53, 192]),
        (Lighten, [205, 155, 129, 129, 129, 255], [205, 155, 129, 116, 91, 255]),
        (ColorDodge, [255, 255, 255, 210, 107, 255], [230, 205, 192, 157, 80, 255]),
        (LinearDodge, [255, 255, 255, 233, 182, 255], [230, 205, 192, 168, 117, 255]),
        (LighterColor, [205, 155, 129, 129, 129, 255], [205, 155, 129, 116, 91, 255]),
        (SoftLight, [205, 155, 130, 104, 54, 255], [205, 155, 130, 104, 53, 255]),
        (HardLight, [206, 156, 131, 105, 55, 255], [205, 155, 130, 105, 54, 255]),
        (VividLight, [207, 156, 131, 105, 53, 255], [206, 155, 130, 104, 53, 255]),
        (LinearLight, [208, 158, 132, 107, 56, 255], [206, 156, 131, 105, 54, 255]),
        (PinLight, [205, 155, 129, 104, 53, 255], [205, 155, 129, 104, 53, 255]),
        (HardMix, [255, 255, 255, 0, 0, 255], [230, 205, 192, 52, 26, 255]),
        (Difference, [76, 26, 0, 25, 76, 126], [140, 90, 65, 64, 64, 191]),
        (Exclusion, [127, 127, 127, 128, 128, 126], [166, 141, 128, 116, 91, 191]),
        (Subtract, [76, 26, 0, 0, 0, 126], [140, 90, 65, 52, 26, 191]),
        (Divide, [255, 255, 255, 205, 104, 255], [230, 205, 192, 154, 79, 255]),
    ];
    let s = 129.0 / 255.0;
    let mut bad = Vec::new();
    for (mode, full, half) in cases {
        for (opacity, want) in [(1.0f32, full), (0.5, half)] {
            for (b8, w) in base.iter().zip(want) {
                let b = *b8 as f32 / 255.0;
                let out = composite(mode, [b, b, b, 1.0], [s, s, s, 1.0], opacity);
                let got = (out[0] * 255.0).round() as i32;
                if (got - w as i32).abs() > 2 {
                    bad.push(format!("{mode:?} {opacity}: основа {b8} → {got} вместо {w}"));
                }
            }
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

#[test]
fn layer_operations_undo_and_redo() {
    use paint_core::{BrushParams, Document, InputPoint, Layer, Stroke};
    let mut doc = Document::new(64, 64);
    // слой 0 — красная точка, слой 1 — синяя точка в другом месте
    let dab = |x: f32| {
        let mut s = Stroke::new(BrushParams { size: 8.0, size_by_pressure: false, ..Default::default() });
        s.add_point(InputPoint { x, y: 32.0, pressure: 1.0 });
        s
    };
    doc.commit_stroke(0, &dab(16.0), [255, 0, 0]);
    doc.insert_layer(&[1], Layer::new("Синий"));
    doc.commit_stroke(1, &dab(48.0), [0, 0, 255]);
    let before = doc.flatten();

    // удаление и отмена
    doc.delete_layer(&[1]);
    assert_eq!(doc.layers.len(), 1);
    doc.undo();
    assert_eq!(doc.layers.len(), 2);
    assert_eq!(doc.flatten(), before);
    doc.redo();
    assert_eq!(doc.layers.len(), 1);
    doc.undo();

    // слияние с нижним не меняет картинку, отмена возвращает оба слоя
    let (below, _) = doc.merge_down(&[1]).unwrap();
    assert_eq!(below, vec![0]);
    assert_eq!(doc.layers.len(), 1);
    let merged = doc.flatten();
    for (a, b) in merged.iter().zip(&before) {
        assert!(a.abs_diff(*b) <= 1);
    }
    doc.undo();
    assert_eq!(doc.layers.len(), 2);
    assert_eq!(doc.flatten(), before);

    // очистка и перенос
    doc.clear_layer(&[0]);
    assert!(doc.layers[0].pixels.tiles.is_empty());
    doc.undo();
    let (moved, _) = doc.move_layer(&[1], -1).unwrap();
    assert_eq!(moved, vec![0]);
    assert_eq!(doc.layers[0].name, "Синий");
    doc.undo();
    assert_eq!(doc.layers[1].name, "Синий");
    assert_eq!(doc.flatten(), before);
}

#[test]
fn stabilizer_follows_the_reference_rules() {
    use paint_core::stabilizer::{Level, Stabilizer};
    // уровень 4: буфер заполнен первой точкой — вторая выходит на 1/4 пути
    let mut st = Stabilizer::with_level(Level::Plain(4));
    st.push(InputPoint { x: 0.0, y: 0.0, pressure: 1.0 });
    let p = st.push(InputPoint { x: 8.0, y: 0.0, pressure: 1.0 });
    assert_eq!(p.x, 2.0);
    // S-7: две ступени (15 и 8) — отстаёт сильнее уровня 15, но в конце доходит до пера
    let mut s7 = Stabilizer::with_level(Level::S(7));
    let mut l15 = Stabilizer::with_level(Level::Plain(15));
    let (mut a, mut b) = (0.0, 0.0);
    for i in 0..60 {
        let q = InputPoint { x: i as f32, y: 0.0, pressure: 1.0 };
        a = s7.push(q).x;
        b = l15.push(q).x;
    }
    assert!(a < b, "S-7 {a} должен отставать больше, чем 15 {b}");
    let tail = s7.finish();
    assert_eq!(tail.len(), 15 * 8);
    assert!((tail.last().unwrap().x - 59.0).abs() < 1e-3);
    assert_eq!(Level::all().len(), 23);
}

#[test]
fn selection_limits_painting_clearing_and_is_undoable() {
    use paint_core::selection::{rect, Combine};
    let mut doc = Document::new(100, 40);
    doc.select(&rect(0.0, 0.0, 50.0, 40.0), Combine::Replace);
    // мазок через весь холст ложится только на левую половину
    let mut s = Stroke::new(BrushParams { size: 10.0, size_by_pressure: false, ..Default::default() });
    for x in 0..=100 {
        s.add_point(InputPoint { x: x as f32, y: 20.0, pressure: 1.0 });
    }
    doc.commit_stroke(0, &s, [0, 0, 0]);
    assert_eq!(pixel(&doc, 30, 20), [0, 0, 0, 255]);
    assert_eq!(pixel(&doc, 70, 20), [255, 255, 255, 255]);
    // очистка — только выделенное
    doc.select(&rect(20.0, 0.0, 40.0, 40.0), Combine::Replace);
    doc.clear_layer(&[0]);
    assert_eq!(pixel(&doc, 30, 20), [255, 255, 255, 255]);
    assert_eq!(pixel(&doc, 10, 20), [0, 0, 0, 255]);
    // отмена: очистка, затем выделение
    doc.undo();
    assert_eq!(pixel(&doc, 30, 20), [0, 0, 0, 255]);
    doc.undo();
    assert_eq!(doc.selection.amount(10, 20), paint_core::pixel::ONE);
    doc.deselect();
    assert!(doc.selection.is_empty());
}

#[test]
fn moving_a_layer_and_a_selected_part() {
    use paint_core::selection::{rect, Combine};
    let black = paint_core::pixel::from_rgba8([0, 0, 0, 255]);
    let mut doc = Document::new(60, 20);
    for x in 0..10 {
        for y in 0..10 {
            doc.layers[0].pixels.set(x, y, black);
            doc.layers[0].pixels.set(x + 30, y, black);
        }
    }
    // весь слой на 5 вправо
    doc.move_layer_pixels(&[0], 5, 0);
    assert_eq!(pixel(&doc, 2, 5), [255, 255, 255, 255]);
    assert_eq!(pixel(&doc, 12, 5), [0, 0, 0, 255]);
    doc.undo();
    assert_eq!(pixel(&doc, 2, 5), [0, 0, 0, 255]);
    // только выделенный левый квадрат — вниз на 10, вместе с выделением
    doc.select(&rect(0.0, 0.0, 20.0, 10.0), Combine::Replace);
    doc.move_layer_pixels(&[0], 0, 10);
    assert_eq!(pixel(&doc, 2, 5), [255, 255, 255, 255]);
    assert_eq!(pixel(&doc, 2, 15), [0, 0, 0, 255]);
    assert_eq!(pixel(&doc, 32, 5), [0, 0, 0, 255], "невыделенное осталось на месте");
    assert_eq!(doc.selection.bounds(), Some((0, 10, 20, 20)));
    doc.undo();
    assert_eq!(pixel(&doc, 2, 5), [0, 0, 0, 255]);
    assert_eq!(doc.selection.bounds(), Some((0, 0, 20, 10)));
}

#[test]
fn transform_scales_and_rotates_a_layer() {
    use paint_core::resample::Mode;
    let black = paint_core::pixel::from_rgba8([0, 0, 0, 255]);
    let mut doc = Document::new(80, 80);
    for x in 10..20 {
        for y in 10..20 {
            doc.layers[0].pixels.set(x, y, black);
        }
    }
    let original = doc.layers[0].pixels.clone();
    let sel = doc.selection.clone();
    // вдвое вокруг (10, 10)
    doc.transform_preview(&[0], &original, None, &sel, [2.0, 0.0, -10.0, 0.0, 2.0, -10.0], Mode::Bilinear);
    assert_eq!(pixel(&doc, 28, 28), [0, 0, 0, 255]);
    assert_eq!(pixel(&doc, 32, 15), [255, 255, 255, 255]);
    doc.move_commit(&[0], original.clone(), None, sel.clone());
    doc.undo();
    assert_eq!(pixel(&doc, 28, 28), [255, 255, 255, 255]);
    // поворот на 90° вокруг (40, 40): квадрат 10..20 уходит в x 60..70, y 10..20
    doc.transform_preview(&[0], &original, None, &sel, [0.0, -1.0, 80.0, 1.0, 0.0, 0.0], Mode::Bicubic);
    assert_eq!(pixel(&doc, 65, 15), [0, 0, 0, 255]);
    assert_eq!(pixel(&doc, 15, 15), [255, 255, 255, 255]);
}

#[test]
fn canvas_flip_rotate_resize_and_resolution() {
    use paint_core::resample::Mode;
    let black = paint_core::pixel::from_rgba8([0, 0, 0, 255]);
    let mut doc = Document::new(40, 20);
    doc.layers[0].pixels.set(1, 2, black);
    doc.flip_canvas(true);
    assert_eq!(pixel(&doc, 38, 2), [0, 0, 0, 255]);
    doc.undo();
    doc.rotate_canvas(true);
    assert_eq!((doc.width, doc.height), (20, 40));
    assert_eq!(pixel(&doc, 17, 1), [0, 0, 0, 255]);
    doc.undo();
    assert_eq!((doc.width, doc.height), (40, 20));
    assert_eq!(pixel(&doc, 1, 2), [0, 0, 0, 255]);
    // больше холст, привязка по центру: точка сдвигается на половину прибавки
    doc.resize_canvas(60, 40, (1, 1));
    assert_eq!(pixel(&doc, 11, 12), [0, 0, 0, 255]);
    doc.undo();
    doc.resample_canvas(80, 40, Mode::Nearest);
    assert_eq!(pixel(&doc, 3, 5), [0, 0, 0, 255]);
    doc.undo();
    // отражение слоя
    doc.flip_layer(&[0], false);
    assert_eq!(pixel(&doc, 1, 17), [0, 0, 0, 255]);
}

#[test]
fn free_deform_maps_corners_to_a_quad() {
    use paint_core::resample::{mul3, square_to_quad, Mode};
    use paint_core::selection::Warp;
    let black = paint_core::pixel::from_rgba8([0, 0, 0, 255]);
    let mut doc = Document::new(100, 100);
    for x in 10..30 {
        for y in 10..30 {
            doc.layers[0].pixels.set(x, y, black);
        }
    }
    let original = doc.layers[0].pixels.clone();
    let sel = doc.selection.clone();
    // квадрат 10..30 → трапеция: верх 10..30, низ 0..60 на y = 50
    let to_unit = [1.0 / 20.0, 0.0, -0.5, 0.0, 1.0 / 20.0, -0.5, 0.0, 0.0, 1.0];
    let h = mul3(square_to_quad([[10.0, 10.0], [30.0, 10.0], [60.0, 50.0], [0.0, 50.0]]), to_unit);
    let w = Warp::Projective(h);
    let (x, y) = w.apply(30.0, 30.0);
    assert!((x - 60.0).abs() < 1e-9 && (y - 50.0).abs() < 1e-9);
    doc.warp_preview(&[0], &original, None, &sel, w, Mode::Bilinear);
    assert_eq!(pixel(&doc, 20, 12), [0, 0, 0, 255]);
    assert_eq!(pixel(&doc, 6, 44), [0, 0, 0, 255], "низ растянут влево");
    assert_eq!(pixel(&doc, 5, 12), [255, 255, 255, 255]);
}

#[test]
fn fill_flatten_and_locks() {
    use paint_core::selection::{magic_wand, rect, Combine};
    let mut doc = Document::new(40, 20);
    // заливка выделения
    doc.select(&rect(0.0, 0.0, 10.0, 20.0), Combine::Replace);
    let all = paint_core::selection::Selection::all(40, 20);
    doc.fill(&[0], &all, [255, 0, 0]);
    assert_eq!(pixel(&doc, 5, 5), [255, 0, 0, 255]);
    assert_eq!(pixel(&doc, 15, 5), [255, 255, 255, 255]);
    doc.deselect();
    // заливка «ведром»: пустая область справа
    let area = magic_wand(&doc.layers[0].pixels, 40, 20, 30, 5, 0);
    doc.fill(&[0], &area, [0, 0, 255]);
    assert_eq!(pixel(&doc, 30, 5), [0, 0, 255, 255]);
    assert_eq!(pixel(&doc, 5, 5), [255, 0, 0, 255]);
    // запрет рисования
    doc.layers[0].lock_paint = true;
    assert!(doc.fill(&[0], &all, [0, 255, 0]).is_empty());
    doc.layers[0].lock_paint = false;
    // свести всё: два слоя → один, картинка та же
    doc.add_layer("верх");
    doc.layers[1].pixels.set(20, 10, paint_core::pixel::from_rgba8([0, 0, 0, 255]));
    let before = doc.flatten();
    doc.flatten_layers();
    assert_eq!(doc.layers.len(), 1);
    assert_eq!(doc.flatten(), before);
    doc.undo();
    assert_eq!(doc.layers.len(), 2);
}

#[test]
fn luminance_to_alpha_and_selection_source() {
    use paint_core::pixel::from_rgba8;
    let mut doc = Document::new(10, 10);
    doc.layers[0].pixels.set(1, 1, from_rgba8([255, 255, 255, 255]));
    doc.layers[0].pixels.set(2, 1, from_rgba8([0, 0, 0, 255]));
    doc.layers[0].pixels.set(3, 1, from_rgba8([128, 128, 128, 255]));
    doc.luminance_to_alpha(&[0]);
    let px = |x| paint_core::pixel::to_rgba8(doc.layers[0].pixels.get(x, 1));
    assert_eq!(px(1)[3], 0, "белое прозрачно");
    assert_eq!(px(2), [0, 0, 0, 255], "чёрное осталось");
    let g = px(3);
    assert!(g[0] < 5 && (126..=129).contains(&g[3]), "серое — полупрозрачное чёрное: {g:?}");
    assert_eq!(pixel(&doc, 3, 1), [128, 128, 128, 255], "поверх белого картинка та же");
    // источник выбора: палочка смотрит на слой-источник, а не на текущий
    doc.add_layer("пустой");
    let src = doc.pick_source(&[1]);
    assert!(src.tiles.is_empty(), "без источников — текущий (пустой) слой");
    doc.layers[0].selection_source = true;
    let src = doc.pick_source(&[1]);
    assert_eq!(src.get(2, 1)[3], paint_core::pixel::ONE as u16);
}

#[test]
fn jpeg_and_png_round_trip_through_document() {
    let dir = std::env::temp_dir().join(format!("sosai-img-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut rgba = Vec::new();
    for y in 0..32u32 {
        for x in 0..48u32 {
            rgba.extend([(x * 5) as u8, (y * 7) as u8, 128, 255]);
        }
    }
    let jpg = dir.join("проба.jpg");
    paint_core::io::save_jpeg(&jpg, 48, 32, &rgba, 95).unwrap();
    let (w, h, back) = paint_core::io::load_image(&jpg).unwrap();
    assert_eq!((w, h), (48, 32));
    let worst = back.iter().zip(&rgba).map(|(a, b)| (*a as i32 - *b as i32).abs()).max().unwrap();
    assert!(worst <= 12, "JPEG теряет немного: {worst}");
    let png = dir.join("проба.png");
    paint_core::io::save_png(&png, 48, 32, &rgba).unwrap();
    let (_, _, p) = paint_core::io::load_image(&png).unwrap();
    let doc = Document::from_rgba(48, 32, &p);
    assert_eq!(doc.flatten(), rgba);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn vector_layers_are_protected_until_rasterized() {
    let mut doc = Document::new(20, 20);
    doc.layers[0].vector = Some(paint_core::layer::VectorData { kind: *b"liwk", raw: vec![1, 2, 3] });
    assert!(!doc.layers[0].paintable() && !doc.layers[0].movable());
    let all = paint_core::selection::Selection::all(20, 20);
    assert!(doc.fill(&[0], &all, [0, 0, 0]).is_empty(), "слой контура не заливается");
    doc.rasterize_layer(&[0]);
    assert!(doc.layers[0].vector.is_none() && doc.layers[0].paintable());
    doc.undo();
    assert!(doc.layers[0].vector.is_some(), "отмена возвращает кривые");
}

#[test]
fn history_keeps_at_most_100_steps_like_sai() {
    let mut doc = Document::new(16, 16);
    let all = paint_core::selection::Selection::all(16, 16);
    for i in 0..130u8 {
        doc.fill(&[0], &all, [i, 0, 0]);
    }
    assert_eq!(doc.undo_steps(), paint_core::document::HISTORY_STEPS);
    for _ in 0..100 {
        assert!(doc.undo().is_some());
    }
    assert!(doc.undo().is_none(), "старше 100 шагов отменить нельзя");
    assert_eq!(pixel(&doc, 3, 3), [29, 0, 0, 255], "остался 30-й шаг");
}

/// Кайма и двухцветный — сверено с живой SAI: заливка 201×201 цветом (130,4,4) на белом.
#[test]
fn layer_effects_match_live_sai() {
    use paint_core::effects::Effect;
    let shot = |effect: Effect| {
        let mut doc = paint_core::Document::new(400, 400);
        doc.background = Some([255, 255, 255]);
        for y in 100..=300 {
            for x in 100..=300 {
                doc.layers[0].pixels.set(x, y, paint_core::pixel::from_rgba8([130, 4, 4, 255]));
            }
        }
        doc.layers[0].effect = Some(effect);
        let img = doc.flatten();
        move |x: usize, y: usize| -> [u8; 3] {
            let o = (y * 400 + x) * 4;
            [img[o], img[o + 1], img[o + 2]]
        }
    };
    let p = shot(Effect::Fringe { kind: *b"watr", width: 2, strength: 100 });
    assert_eq!([p(100, 200), p(101, 200), p(102, 200), p(101, 101)], [[5, 0, 0], [18, 0, 0], [130, 4, 4], [5, 0, 0]]);
    let p = shot(Effect::Fringe { kind: *b"watr", width: 13, strength: 85 });
    assert_eq!([p(100, 200), p(106, 200), p(107, 200)], [[23, 1, 1], [23, 1, 1], [24, 1, 1]]);
    let p = shot(Effect::Binary { threshold: 50, rest: 0 });
    assert_eq!([p(200, 200), p(100, 100), p(99, 200)], [[255, 0, 0], [255, 0, 0], [255, 255, 255]]);
}

/// Слой на весь холст: у края холста каймы нет (за краем берутся крайние точки).
#[test]
fn fringe_ignores_canvas_border() {
    let mut doc = paint_core::Document::new(100, 80);
    for y in 0..80 {
        for x in 0..100 {
            doc.layers[0].pixels.set(x, y, paint_core::pixel::from_rgba8([130, 4, 4, 255]));
        }
    }
    doc.layers[0].effect = Some(paint_core::effects::Effect::Fringe { kind: *b"watr", width: 5, strength: 100 });
    let img = doc.flatten();
    assert!(img.as_chunks::<4>().0.iter().all(|p| p[..3] == [130, 4, 4]));
}

#[test]
fn layers_move_anywhere_and_undo() {
    let mut doc = paint_core::Document::new(64, 64);
    let names = |d: &paint_core::Document| -> Vec<String> {
        fn walk(ls: &[paint_core::Layer], out: &mut Vec<String>) {
            for l in ls {
                out.push(l.name.clone());
                if let Some(ch) = &l.children {
                    out.push("[".into());
                    walk(ch, out);
                    out.push("]".into());
                }
            }
        }
        let mut out = Vec::new();
        walk(&d.layers, &mut out);
        out
    };
    doc.layers[0].name = "A".into();
    for n in ["B", "C"] {
        doc.layers.push(paint_core::Layer::new(n));
    }
    let mut f = paint_core::Layer::new("F");
    f.children = Some(vec![paint_core::Layer::new("x")]);
    doc.layers.push(f);
    // A B C F[x]: A — на самый верх
    let (p, _) = doc.move_layer_to(&[0], &[], 4).unwrap();
    assert_eq!(p, vec![3]);
    assert_eq!(names(&doc).join(" "), "B C F [ x ] A");
    // B — в папку F поверх x (F после изъятия B становится вторым)
    let (p, _) = doc.move_layer_to(&[0], &[2], 1).unwrap();
    assert_eq!(p, vec![1, 1]);
    assert_eq!(names(&doc).join(" "), "C F [ x B ] A");
    // папку в саму себя нельзя
    assert!(doc.move_layer_to(&[1], &[1], 0).is_none());
    doc.undo();
    doc.undo();
    assert_eq!(names(&doc).join(" "), "A B C F [ x ]");
}

#[test]
fn mask_create_paint_apply_undo() {
    use paint_core::tile::MaskMap;
    let mut doc = paint_core::Document::new(64, 64);
    doc.background = None;
    for y in 0..64 {
        for x in 0..64 {
            doc.layers[0].pixels.set(x, y, paint_core::pixel::from_rgba8([200, 100, 50, 255]));
        }
    }
    doc.new_mask(&[0]);
    assert!(doc.layers[0].mask.is_some());
    // мазок «скрыть» по левой половине с полным покрытием
    let orig = doc.layers[0].mask.clone().unwrap();
    let mut cover = MaskMap::new();
    for y in 0..64 {
        for x in 0..32 {
            cover.set(x, y, paint_core::pixel::ONE as u16);
        }
    }
    doc.mask_stroke_preview(&[0], &orig.hide, &cover, 0x4000);
    doc.mask_stroke_commit(&[0], orig);
    let a = |d: &paint_core::Document, x: usize| d.flatten()[x * 4 + 3];
    assert_eq!((a(&doc, 5), a(&doc, 50)), (0, 255));
    doc.undo();
    assert_eq!(a(&doc, 5), 255, "отмена мазка");
    doc.redo();
    doc.apply_mask(&[0]);
    assert!(doc.layers[0].mask.is_none());
    assert_eq!((a(&doc, 5), a(&doc, 50)), (0, 255), "применённая маска");
    doc.undo();
    assert!(doc.layers[0].mask.is_some());
    assert_eq!(doc.layers[0].pixels.get(5, 5)[3], paint_core::pixel::ONE as u16);
}

/// Маска едет со слоем: сдвиг, отражение, трансформация; отмена возвращает и маску.
#[test]
fn mask_moves_and_transforms_with_layer() {
    use paint_core::tile::MaskMap;
    let mut doc = paint_core::Document::new(64, 64);
    doc.background = None;
    for y in 0..64 {
        for x in 0..64 {
            doc.layers[0].pixels.set(x, y, paint_core::pixel::from_rgba8([200, 100, 50, 255]));
        }
    }
    doc.new_mask(&[0]);
    // скрыт квадрат 8..16
    let mut hide = MaskMap::new();
    for y in 8..16 {
        for x in 8..16 {
            hide.set(x, y, 0x4000);
        }
    }
    doc.layers[0].mask.as_mut().unwrap().hide = hide.clone();
    let m = |d: &paint_core::Document, x, y| d.layers[0].mask.as_ref().unwrap().hide.get(x, y);

    doc.move_layer_pixels(&[0], 10, 0);
    assert_eq!((m(&doc, 9, 9), m(&doc, 19, 9), m(&doc, 25, 9)), (0, 0x4000, 0x4000), "маска сдвинулась на 10");
    doc.undo();
    assert_eq!(doc.layers[0].mask.as_ref().unwrap().hide, hide, "отмена вернула маску");
    doc.redo();
    assert_eq!(m(&doc, 19, 9), 0x4000);
    doc.undo();

    doc.flip_layer(&[0], true);
    assert_eq!((m(&doc, 9, 9), m(&doc, 50, 9)), (0, 0x4000), "маска отражена вместе со слоем");
    doc.undo();

    // трансформация: увеличение вдвое от угла
    let original = doc.layers[0].pixels.clone();
    let mask = doc.mask_snapshot(&[0]);
    let sel = doc.selection.clone();
    doc.transform_preview(&[0], &original, mask.as_ref(), &sel, [2.0, 0.0, 0.0, 0.0, 2.0, 0.0], paint_core::resample::Mode::Nearest);
    doc.move_commit(&[0], original, mask, sel);
    assert_eq!((m(&doc, 12, 12), m(&doc, 20, 20), m(&doc, 30, 30), m(&doc, 34, 34)), (0, 0x4000, 0x4000, 0), "маска увеличена вдвое");
    doc.undo();
    assert_eq!(doc.layers[0].mask.as_ref().unwrap().hide, hide);
}

/// «Слить с нижним» работает и со слоями с масками: картинка не меняется, у итогового слоя
/// маски нет, отмена возвращает всё.
#[test]
fn merge_down_with_masks() {
    use paint_core::tile::MaskMap;
    let mut doc = paint_core::Document::new(64, 64);
    doc.background = Some([255, 255, 255]);
    for y in 0..64 {
        for x in 0..64 {
            doc.layers[0].pixels.set(x, y, paint_core::pixel::from_rgba8([200, 100, 50, 255]));
        }
    }
    doc.add_layer("top");
    for y in 20..40 {
        for x in 0..64 {
            doc.layers[1].pixels.set(x, y, paint_core::pixel::from_rgba8([0, 0, 255, 255]));
        }
    }
    let half = |x0: i32, x1: i32| {
        let mut h = MaskMap::new();
        for y in 0..64 {
            for x in x0..x1 {
                h.set(x, y, 0x4000);
            }
        }
        h
    };
    for (i, h) in [(0, half(0, 16)), (1, half(48, 64))] {
        doc.new_mask(&[i]);
        doc.layers[i].mask.as_mut().unwrap().hide = h;
    }
    let before = doc.flatten();
    let (below, _) = doc.merge_down(&[1]).expect("слияние на слой с маской");
    assert_eq!(below, vec![0]);
    assert_eq!(doc.layers.len(), 1);
    assert!(doc.layers[0].mask.is_none(), "маска нижнего применена");
    for (a, b) in doc.flatten().iter().zip(&before) {
        assert!(a.abs_diff(*b) <= 1);
    }
    doc.undo();
    assert_eq!(doc.layers.len(), 2);
    assert!(doc.layers[0].mask.is_some() && doc.layers[1].mask.is_some());
    assert_eq!(doc.flatten(), before);
}

/// Скорость большого карандаша (запуск: `cargo test --release big_pencil -- --ignored --nocapture`)
/// и отпечаток результата — чтобы ускорение не меняло ни одной точки.
#[test]
#[ignore]
fn big_pencil_speed() {
    use paint_core::{BrushParams, InputPoint, Stroke};
    for size in [350.0f32, 100.0, 20.0] {
        let mut s = Stroke::new(BrushParams { size, hardness: 1.0, ..Default::default() });
        let t = std::time::Instant::now();
        for i in 0..=100 {
            s.add_point(InputPoint { x: 400.0 + i as f32 * 10.0, y: 400.0 + (i as f32 * 0.1).sin() * 50.0, pressure: 0.6 + 0.4 * (i as f32 * 0.07).cos().abs() });
        }
        let mut h: u64 = 0;
        let mut keys: Vec<_> = s.mask.tiles.keys().copied().collect();
        keys.sort();
        for k in keys {
            for &v in s.mask.tiles[&k].0.iter() {
                h = h.wrapping_mul(1_000_003).wrapping_add(v as u64);
            }
        }
        eprintln!("size {size}: {:.2} s, hash {h:x}", t.elapsed().as_secs_f32());
    }
}

/// Мазок большим карандашом так, как его ведёт окно: точка за точкой, после каждой — показ.
#[test]
#[ignore]
fn big_pencil_live_speed() {
    use paint_core::{BrushParams, InputPoint, LiveStroke, Stroke};
    let mut doc = paint_core::Document::new(2000, 1500);
    let t = std::time::Instant::now();
    let mut live = LiveStroke::new(vec![0], Stroke::new(BrushParams { size: 350.0, hardness: 1.0, ..Default::default() }), [10, 20, 30]);
    let (mut paint, mut layer, mut show) = (0.0f32, 0.0f32, 0.0f32);
    for i in 0..=500 {
        let a = std::time::Instant::now();
        live.stroke.add_point(InputPoint { x: 400.0 + i as f32 * 2.0, y: 600.0, pressure: 1.0 });
        paint += a.elapsed().as_secs_f32();
        let b = std::time::Instant::now();
        let keys = doc.live_update(&mut live);
        layer += b.elapsed().as_secs_f32();
        let c = std::time::Instant::now();
        for k in keys {
            let _ = doc.render_tile(k);
        }
        show += c.elapsed().as_secs_f32();
    }
    doc.live_finish(live);
    eprintln!("всего {:.2} с: отпечатки {paint:.2} с, слой {layer:.2} с, показ {show:.2} с (показ в окне идёт во все ядра)", t.elapsed().as_secs_f32());
}

/// Скорость смешивающих кистей большого размера (Кисть, Акварель) — мазок, как в окне.
#[test]
#[ignore]
fn big_mixing_speed() {
    use paint_core::mix::MixParams;
    use paint_core::{BrushParams, InputPoint, LiveStroke, Stroke};
    for (name, mix, size) in [("Кисть", MixParams::new(0.0, 0.0, 0.5), 350.0), ("Акварель", MixParams { blur: true, ..MixParams::new(0.3, 0.3, 0.5) }, 350.0), ("Акварель", MixParams { blur: true, ..MixParams::new(0.3, 0.3, 0.5) }, 100.0), ("Кисть", MixParams::new(0.0, 0.0, 0.5), 100.0)] {
        let mut doc = paint_core::Document::new(2000, 1500);
        for y in 0..1500 {
            for x in 0..2000 {
                doc.layers[0].pixels.set(x, y, paint_core::pixel::from_rgba8([(x / 8) as u8, (y / 6) as u8, 90, 255]));
            }
        }
        let params = BrushParams { size, hardness: 1.0, min_size: 0.5, density_by_pressure: true, mix: Some(mix), ..Default::default() };
        let t = std::time::Instant::now();
        let mut live = LiveStroke::new(vec![0], Stroke::new(params), [10, 20, 30]);
        for i in 0..=250 {
            live.stroke.add_point(InputPoint { x: 400.0 + i as f32 * 4.0, y: 600.0, pressure: 1.0 });
            let _ = doc.live_update(&mut live);
        }
        doc.live_finish(live);
        eprintln!("{name} {size}: {:.2} с на 1000 px", t.elapsed().as_secs_f32());
    }
}

/// Отпечаток результата смешивающих кистей — ускорение не должно менять ни одной точки.
#[test]
fn mixing_brushes_fingerprint() {
    use paint_core::mix::MixParams;
    use paint_core::{BrushParams, InputPoint, LiveStroke, Stroke};
    let mut out = Vec::new();
    for (mix, hardness) in [(MixParams::new(0.0, 0.0, 0.5), 1.0), (MixParams { blur: true, ..MixParams::new(0.3, 0.3, 0.5) }, 1.0), (MixParams::new(0.4, 0.2, 0.7), 0.5)] {
        for size in [9.0f32, 60.0] {
            let mut doc = paint_core::Document::new(200, 160);
            for y in 0..160 {
                for x in 0..200 {
                    doc.layers[0].pixels.set(x, y, paint_core::pixel::from_rgba8([(x + 30) as u8, (y * 3 / 2) as u8, 90, if x > 150 { 120 } else { 255 }]));
                }
            }
            let params = BrushParams { size, hardness, min_size: 0.5, density_by_pressure: true, mix: Some(mix), ..Default::default() };
            let mut live = LiveStroke::new(vec![0], Stroke::new(params), [10, 20, 200]);
            for i in 0..=40 {
                live.stroke.add_point(InputPoint { x: 20.0 + i as f32 * 4.0, y: 80.0 + (i as f32 * 0.3).sin() * 20.0, pressure: 0.3 + 0.7 * (i as f32 / 40.0) });
                let _ = doc.live_update(&mut live);
            }
            doc.live_finish(live);
            let mut h: u64 = 0;
            for v in doc.flatten() {
                h = h.wrapping_mul(1_000_003).wrapping_add(v as u64);
            }
            out.push(format!("{h:x}"));
        }
    }
    eprintln!("MIXHASH {}", out.join(" "));
    // записано до ускорения (0.8.0)
    assert_eq!(out.join(" "), "d0c09e9e3c419e2c 55076abdac3621d4 f020dae6de1425c2 a4f6a3039e08a2d9 6b59d6783d952f3b 7ee04a9e9b1bf76a");
}

#[test]
#[ignore]
fn box_blur_speed() {
    let (w, h) = (706usize, 706usize);
    let mut buf: Vec<[f64; 4]> = (0..w * h).map(|i| [(i % 7) as f64, 0.5, 0.2, 1.0]).collect();
    let t = std::time::Instant::now();
    for _ in 0..10 {
        for r in [88, 88] {
            paint_core::mix::box_blur(&mut buf, w, h, r, false);
            paint_core::mix::box_blur(&mut buf, w, h, r, true);
        }
    }
    eprintln!("BLUR {:.1} ms за пачку", t.elapsed().as_secs_f32() * 100.0);
}

/// То же для крупных кистей (параллельный счёт): отпечаток записан на версии 0.8.0.
#[test]
fn mixing_brushes_fingerprint_large() {
    use paint_core::mix::MixParams;
    use paint_core::{BrushParams, InputPoint, LiveStroke, Stroke};
    let mut out = Vec::new();
    for (mix, hardness, size) in [(MixParams::new(0.0, 0.0, 0.5), 1.0, 300.0f32), (MixParams { blur: true, ..MixParams::new(0.3, 0.3, 0.5) }, 1.0, 240.0), (MixParams::new(0.4, 0.2, 0.7), 0.5, 260.0)] {
        let mut doc = paint_core::Document::new(700, 500);
        for y in 0..500 {
            for x in 0..700 {
                doc.layers[0].pixels.set(x, y, paint_core::pixel::from_rgba8([(x / 3) as u8, (y / 2) as u8, 90, if x > 500 { 120 } else { 255 }]));
            }
        }
        let params = BrushParams { size, hardness, min_size: 0.5, density_by_pressure: true, mix: Some(mix), ..Default::default() };
        let mut live = LiveStroke::new(vec![0], Stroke::new(params), [10, 20, 200]);
        for i in 0..=12 {
            live.stroke.add_point(InputPoint { x: 200.0 + i as f32 * 6.0, y: 250.0 + (i as f32 * 0.3).sin() * 20.0, pressure: 0.4 + 0.6 * (i as f32 / 12.0) });
            let _ = doc.live_update(&mut live);
        }
        doc.live_finish(live);
        let mut h: u64 = 0;
        for v in doc.flatten() {
            h = h.wrapping_mul(1_000_003).wrapping_add(v as u64);
        }
        out.push(format!("{h:x}"));
    }
    assert_eq!(out.join(" "), "fd35badc61b48b13 184298a9dd1016b2 92bf592505a8c9e");
}

#[test]
fn free_deform_blends_bilinear_and_perspective_like_sai() {
    use paint_core::selection::Warp;
    // замер живой SAI 2017: угол (1062, 258) рамки 739..1062 × 258..561 утянут в (960, 320)
    let quad = [[739.0, 258.0], [960.0, 320.0], [1062.0, 561.0], [739.0, 561.0]];
    let w = |p: f64| Warp::Quad { src: [739.0, 258.0, 1062.0, 561.0], quad, perspective: p };
    // билинейно: середина рамки — среднее углов
    let (x, y) = w(0.0).apply(900.5, 409.5);
    assert!((x - 875.0).abs() < 1e-9 && (y - 425.0).abs() < 1e-9);
    // линия сетки y = 410 пересекает столбец x = 800 (SAI: 417,0 / 409,0 / 400,5 / 392,5 / 383,5)
    for (p, want) in [(0.0, 417.0), (0.25, 409.0), (0.5, 400.5), (0.75, 392.5), (1.0, 383.5)] {
        let (mut lo, mut hi) = (739.0, 1062.0);
        for _ in 0..60 {
            let m = (lo + hi) / 2.0;
            if w(p).apply(m, 410.0).0 < 800.0 { lo = m } else { hi = m }
        }
        let y = w(p).apply(lo, 410.0).1;
        assert!((y - want).abs() < 1.0, "перспектива {p}: {y} вместо {want}");
    }
    // обратное отображение через warp_preview: квадрат остаётся на месте при тождественной рамке
    let id = Warp::Quad { src: [0.0, 0.0, 10.0, 10.0], quad: [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]], perspective: 0.3 };
    let (x, y) = id.apply(3.0, 7.0);
    assert!((x - 3.0).abs() < 1e-9 && (y - 7.0).abs() < 1e-9);
}
