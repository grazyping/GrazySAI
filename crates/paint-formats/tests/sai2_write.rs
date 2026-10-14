//! Запись `.sai2`: что записано, то и читается обратно (слои, папки, свойства, пиксели,
//! сведённое изображение).

use paint_core::{BlendMode, BrushParams, Document, InputPoint, Layer, Stroke};

fn dab(doc: &mut Document, path: &[usize], x: f32, y: f32, size: f32, rgb: [u8; 3]) {
    let mut s = Stroke::new(BrushParams { size, hardness: 0.5, size_by_pressure: false, ..Default::default() });
    for i in 0..20 {
        s.add_point(InputPoint { x: x + i as f32 * 3.0, y, pressure: 1.0 });
    }
    let layer = doc.layer_mut(path).unwrap();
    layer.apply_stroke(&s, rgb);
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
