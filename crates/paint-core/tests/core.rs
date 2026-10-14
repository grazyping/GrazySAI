use paint_core::blend::{channel, composite};
use paint_core::brush::round_coverage;
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
    assert_eq!(round_coverage(0.0, 20.0, 1.0), 1.0);
    assert_eq!(round_coverage(8.0, 20.0, 1.0), 1.0);
    assert_eq!(round_coverage(11.0, 20.0, 1.0), 0.0);
    let edge = round_coverage(9.1, 20.0, 1.0);
    assert!(edge > 0.3 && edge < 0.7, "{edge}");
    // Мягкая кисть: сплошной сердцевины нет, спад заходит за номинальный радиус.
    assert!(round_coverage(5.0, 20.0, 0.0) < 1.0);
    assert!(round_coverage(5.0, 20.0, 0.0) > round_coverage(8.0, 20.0, 0.0));
    assert!(round_coverage(12.0, 20.0, 0.0) > 0.0);
    assert_eq!(round_coverage(18.0, 20.0, 0.0), 0.0);
    // Покрытие не растёт с расстоянием ни на одной ступени жёсткости.
    for h in paint_core::brush::HARDNESS_STEPS {
        let mut prev = 1.0f32;
        for i in 0..400 {
            let c = round_coverage(i as f32 * 0.1, 30.0, h);
            assert!(c <= prev + 1e-6 && (0.0..=1.0).contains(&c), "жёсткость {h}, d={}", i as f32 * 0.1);
            prev = c;
        }
    }
    // Шаг: у крупной жёсткой кисти около 4% диаметра, у мягкой — до 15%.
    let hard = paint_core::brush::step_fraction(200.0, 1.0);
    let soft = paint_core::brush::step_fraction(200.0, 0.0);
    assert!(hard > 0.04 && hard < 0.045, "{hard}");
    assert!(soft > 0.145 && soft < 0.151, "{soft}");
    assert_eq!(paint_core::brush::step_fraction(0.8, 1.0), 0.15);
    // Кисть тоньше пикселя не пропадает, а бледнеет.
    let thin = round_coverage(0.0, 0.5, 1.0);
    assert!(thin > 0.2 && thin < 0.8, "{thin}");

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
    clip.apply_stroke(&dot(48.0), [0, 0, 255]); // синий круг правее, заходит на красный
    doc.layers.push(clip);
    assert_eq!(pixel(&doc, 40, 32), [0, 0, 255, 255]); // пересечение — синее
    assert_eq!(pixel(&doc, 22, 32), [255, 0, 0, 255]); // только красный
    assert_eq!(pixel(&doc, 60, 32), [255, 255, 255, 255]); // вне основы синего не видно

    // Папка с половинной непрозрачностью ослабляет всё своё содержимое.
    let mut inner = Layer::new("внутри");
    inner.apply_stroke(&dot(80.0), [0, 0, 0]);
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
fn dab_opacity_reaches_the_target() {
    use paint_core::brush::{dab_opacity, DabShape};
    let shape = DabShape::new(30.0, 1.0);
    let a = dab_opacity(&shape, 30.0, 1.2, 0.5);
    assert!(a > 0.0 && a < 0.1, "{a}");
    assert_eq!(dab_opacity(&shape, 30.0, 1.2, 0.0), 0.0);
}

/// Непрозрачность одиночного отпечатка полной плотности, измеренная на образце (сборка
/// 2017-01-19, карандаш, щелчок мышью): строки — пять ступеней жёсткости от мягкой к жёсткой,
/// столбцы — размеры 20, 30, 50 и 100 px. Точность снимка — 1/255.
#[test]
fn dab_opacity_matches_the_reference_program() {
    use paint_core::brush::{dab_opacity, step_fraction, DabShape, HARDNESS_STEPS};
    let measured = [
        [0.969, 0.973, 0.973, 0.973],
        [0.682, 0.675, 0.663, 0.659],
        [0.506, 0.490, 0.475, 0.463],
        [0.388, 0.369, 0.349, 0.333],
        [0.373, 0.349, 0.325, 0.306],
    ];
    for (row, hardness) in HARDNESS_STEPS.iter().enumerate() {
        for (col, d) in [20.0f32, 30.0, 50.0, 100.0].iter().enumerate() {
            let shape = DabShape::new(*d, *hardness);
            let a = dab_opacity(&shape, *d, step_fraction(*d, *hardness) * d, 1.0);
            let want = measured[row][col];
            assert!((a - want).abs() < 0.008, "ступень {row}, размер {d}: {a} вместо {want}");
        }
    }
}

/// Край отпечатка самой жёсткой ступени, измеренный на образце (карандаш, плотность 100, щелчок):
/// непрозрачность пикселей на расстоянии `d` от центра отпечатка, центр — в центре пикселя.
#[test]
fn hard_dab_edge_matches_the_reference_program() {
    let cases: [(f32, [(i32, f32); 3]); 2] = [
        (10.0, [(3, 0.361), (4, 0.314), (5, 0.051)]),
        (20.0, [(8, 0.373), (9, 0.251), (10, 0.043)]),
    ];
    for (size, points) in cases {
        let mut s = Stroke::new(BrushParams { size, hardness: 1.0, size_by_pressure: false, ..Default::default() });
        s.add_point(InputPoint { x: 50.5, y: 50.5, pressure: 1.0 });
        for (d, want) in points {
            let got = s.mask.get(50 + d, 50) as f32 / ONE as f32;
            assert!((got - want).abs() < 0.015, "размер {size}, расстояние {d}: {got} вместо {want}");
        }
    }
}

/// Все режимы наложения образца (сборка 2017-01-19), снятые с работающей программы: пять полос
/// серого под слоем, залитым серым 129, при непрозрачности слоя 100% и 50%; последний столбец —
/// белый фон. Значения в уровнях 0..255.
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
