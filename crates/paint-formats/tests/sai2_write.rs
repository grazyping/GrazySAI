//! Запись `.sai2`: что записано, то и читается обратно (слои, папки, свойства, пиксели,
//! сведённое изображение).

use paint_core::{BlendMode, BrushParams, Document, InputPoint, Layer, Stroke};

fn dab(doc: &mut Document, path: &[usize], x: f32, y: f32, size: f32, rgb: [u8; 3]) {
    let mut s = Stroke::new(BrushParams { size, hardness: 0.5, size_by_pressure: false, ..Default::default() });
    for i in 0..20 {
        s.add_point(InputPoint { x: x + i as f32 * 3.0, y, pressure: 1.0 });
    }
    let layer = doc.layer_mut(path).unwrap();
    layer.apply_stroke(&s, rgb, &Default::default());
}

#[test]
fn written_sai2_reads_back_the_same() {
    let mut doc = Document::new(150, 90);
    doc.layers[0].name = "Фон".into();
    dab(&mut doc, &[0], 10.0, 20.0, 12.0, [200, 30, 40]);
    let mut top = Layer::new("Сверху");
    top.mode = BlendMode::Multiply;
    top.opacity = 0.57;
    doc.layers.push(top);
    dab(&mut doc, &[1], 30.0, 40.0, 20.0, [20, 90, 220]);
    let mut inner = Layer::new("В папке");
    inner.clipping = true;
    inner.mode = BlendMode::Shade;
    let mut folder = Layer::folder("Папка", vec![Layer::new("Низ папки"), inner]);
    folder.visible = false;
    doc.layers.push(folder);
    dab(&mut doc, &[2, 0], 60.0, 70.0, 8.0, [0, 0, 0]);

    let dir = std::env::temp_dir().join(format!("paint-rs-write-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("t.sai2");
    paint_formats::sai2_write::save(&doc, &path).unwrap();
    let back = paint_formats::sai2_doc::load(&path).unwrap();
    assert!(back.warnings.is_empty(), "{:?}", back.warnings);
    let d = &back.document;
    assert_eq!((d.width, d.height, d.layers.len()), (150, 90, 3));
    assert_eq!(d.layers[0].name, "Фон");
    assert_eq!((d.layers[1].name.as_str(), d.layers[1].mode, (d.layers[1].opacity * 100.0).round()), ("Сверху", BlendMode::Multiply, 57.0));
    let f = &d.layers[2];
    assert_eq!((f.name.as_str(), f.visible), ("Папка", false));
    let kids = f.children.as_ref().unwrap();
    assert_eq!(kids.len(), 2);
    assert_eq!((kids[1].name.as_str(), kids[1].clipping, kids[1].mode), ("В папке", true, BlendMode::Shade));
    // пиксели: в файле 14 бит на канал, у нас 15 — младший бит теряется
    for (a, b) in [(&doc.layers[0], &d.layers[0]), (&doc.layers[1], &d.layers[1]), (&doc.layers[2].children.as_ref().unwrap()[0], &kids[0])] {
        for y in 0..90 {
            for x in 0..150 {
                let (p, q) = (a.pixels.get(x, y), b.pixels.get(x, y));
                for c in 0..4 {
                    assert!(p[c].abs_diff(q[c]) <= 1, "({x},{y}) {p:?} {q:?}");
                }
            }
        }
    }
    // сведённое изображение в файле совпадает со сведением документа
    let flat = paint_formats::sai2_doc::read_flattened(&path).unwrap();
    assert_eq!(flat, doc.flatten());
    std::fs::remove_dir_all(&dir).ok();
}

/// Слои контура и фигур из файлов пользователя переживают «открыть → сохранить → открыть»:
/// вид слоя и кривые байт в байт, картинка та же.
#[test]
fn vector_layers_survive_save() {
    let dir = std::env::var("SAI2_SAMPLES").map(std::path::PathBuf::from).unwrap_or_else(|_| {
        std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("SAI2")
    });
    for name in ["пинг4.sai2", "пинг5.sai2"] {
        let path = dir.join(name);
        if !path.exists() {
            continue;
        }
        let a = paint_formats::sai2_doc::load(&path).unwrap();
        let out = std::env::temp_dir().join(format!("grazysai-vec-{}-{name}", std::process::id()));
        paint_formats::sai2_write::save(&a.document, &out).unwrap();
        let b = paint_formats::sai2_doc::load(&out).unwrap();
        let vec_of = |d: &paint_core::Document| -> Vec<(String, Option<Vec<u8>>)> {
            fn walk(ls: &[paint_core::Layer], out: &mut Vec<(String, Option<Vec<u8>>)>) {
                for l in ls {
                    out.push((l.name.clone(), l.vector.as_ref().map(|v| v.raw.clone())));
                    if let Some(ch) = &l.children {
                        walk(ch, out);
                    }
                }
            }
            let mut out = Vec::new();
            walk(&d.layers, &mut out);
            out
        };
        let (va, vb) = (vec_of(&a.document), vec_of(&b.document));
        assert!(va.iter().any(|(_, v)| v.is_some()), "{name}: в файле нет кривых");
        assert_eq!(va, vb, "{name}: кривые изменились");
        assert_eq!(a.vector_layers.len(), b.vector_layers.len(), "{name}");
        assert_eq!(a.document.flatten(), b.document.flatten(), "{name}: картинка изменилась");
        std::fs::remove_file(out).ok();
    }
}

#[test]
fn paper_effect_and_unknown_layer_blocks_survive_save() {
    let mut doc = Document::new(64, 64);
    dab(&mut doc, &[0], 10.0, 20.0, 12.0, [200, 30, 40]);
    doc.layers[0].paper = Some(paint_core::paper::Paper::new("Water Color 2", 30, 120, 1));
    doc.layers[0].effect = Some(paint_core::effects::Effect::Fringe { kind: *b"watr", width: 3, strength: 50 });
    doc.layers[0].extra = vec![(*b"zzzz", vec![1, 2, 3, 4])];
    let out = std::env::temp_dir().join(format!("grazysai-paper-{}.sai2", std::process::id()));
    paint_formats::sai2_write::save(&doc, &out).unwrap();
    let info = paint_formats::sai2::read_info(&out).unwrap();
    let l = &info.layers[0];
    assert_eq!(l.paper, Some(("Water Color 2".to_string(), 30, 120, 1)));
    assert_eq!(l.extra, vec![(*b"frng", vec![b'w', b'a', b't', b'r', 3, 0, 50, 0]), (*b"zzzz", vec![1, 2, 3, 4])]);
    let back = paint_formats::sai2_doc::load(&out).unwrap();
    assert_eq!(back.document.layers[0].effect, doc.layers[0].effect);
    assert_eq!(back.document.layers[0].extra, vec![(*b"zzzz", vec![1, 2, 3, 4])]);
    std::fs::remove_file(&out).ok();
}


/// Фон холста (байт 0x11, цвет 0x38) и «Layer Effect Color» (0x3C) — как у SAI 2017.
#[test]
fn canvas_background_survives_save() {
    let dir = std::env::temp_dir().join(format!("grazysai-bg-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (code, color) in [(0x01, [0x80, 0x80, 0x80]), (0x10, [1, 2, 3]), (0x21, [200, 100, 50]), (0x40, [9, 9, 9])] {
        let mut doc = Document::new(64, 48);
        doc.set_background(code, color);
        doc.effect_color = *b"deep";
        doc.resolution = 0x0011_d4a2;
        let path = dir.join(format!("bg{code}.sai2"));
        paint_formats::sai2_write::save(&doc, &path).unwrap();
        let head = std::fs::read(&path).unwrap();
        assert_eq!((head[0x11], &head[0x38..0x3b], &head[0x3c..0x40]), (code, &color[..], &b"deep"[..]));
        let back = paint_formats::sai2_doc::load(&path).unwrap().document;
        assert_eq!((back.bg_code, back.bg_color, back.background, back.resolution), (code, color, doc.background, 0x0011_d4a2));
    }
    std::fs::remove_dir_all(dir).ok();
}

/// Стили слоя и слой коррекции сохраняются своими блоками и читаются обратно.
#[test]
fn styles_and_correction_round_trip() {
    use paint_core::adjust::{Adjust, Stop};
    use paint_core::style::{Paint, Styles};
    let mut doc = paint_core::Document::new(64, 64);
    doc.layers[0].pixels.set(3, 3, [100, 0, 0, 32768]);
    doc.layers[0].styles = Some(Box::new(Styles { stroke: Some((3, Paint { color: [1, 2, 3], opacity: 90 })), shadow: Some((2, -2, 4, Paint { color: [0, 0, 0], opacity: 40 })), ..Default::default() }));
    let mut g = paint_core::layer::Layer::new("карта");
    g.adjust = Some(Adjust::GradientMap(vec![Stop { at: 0.0, color: [10, 20, 30] }, Stop { at: 0.5, color: [200, 0, 0] }, Stop { at: 1.0, color: [255, 255, 255] }]));
    doc.layers.push(g);
    let dir = std::env::temp_dir().join(format!("grazysai-style-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("s.sai2");
    paint_formats::sai2_write::save(&doc, &p).unwrap();
    let back = paint_formats::sai2_doc::load(&p).unwrap().document;
    std::fs::remove_dir_all(dir).ok();
    assert_eq!(back.layers[0].styles, doc.layers[0].styles);
    assert_eq!(back.layers[1].adjust, doc.layers[1].adjust);
    assert!(back.layers.iter().all(|l| l.extra.iter().all(|(t, _)| t != b"gsst" && t != b"gsad")));
}

/// Файл для проверки на глаз: `STYLE_OUT=путь cargo test … style_sample -- --ignored`.
#[test]
#[ignore]
fn style_sample() {
    use paint_core::adjust::Adjust;
    use paint_core::style::{Paint, Styles};
    let mut doc = paint_core::Document::new(400, 300);
    for y in 80..180 {
        for x in 60..200 {
            let t = (x - 60) as f32 / 140.0;
            doc.layers[0].pixels.set(x, y, paint_core::pixel::from_rgba8([(255.0 * t) as u8, 80, (255.0 * (1.0 - t)) as u8, 255]));
        }
    }
    doc.layers[0].styles = Some(Box::new(Styles { stroke: Some((4, Paint { color: [20, 20, 20], opacity: 100 })), shadow: Some((12, 12, 8, Paint { color: [0, 0, 0], opacity: 50 })), glow: None, overlay: None }));
    let mut g = paint_core::layer::Layer::new("Карта градиента");
    g.adjust = Some(Adjust::gradient_default());
    g.visible = false;
    doc.layers.push(g);
    paint_formats::sai2_write::save(&doc, std::path::Path::new(&std::env::var("STYLE_OUT").unwrap())).unwrap();
}
