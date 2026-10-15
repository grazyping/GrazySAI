//! PSD: сохранить и прочитать обратно; папки, имена, режимы, видимость.

use paint_core::blend::BlendMode;
use paint_core::pixel::from_rgba8;
use paint_core::{Document, Layer};

fn sample() -> Document {
    let mut doc = Document::new(64, 40);
    doc.layers[0].name = "Фон слой".into();
    for y in 0..40 {
        for x in 0..64 {
            doc.layers[0].pixels.set(x, y, from_rgba8([200, 220, 240, 255]));
        }
    }
    let mut a = Layer::new("Линии");
    for x in 5..50 {
        a.pixels.set(x, 10, from_rgba8([10, 20, 30, 200]));
    }
    a.mode = BlendMode::Multiply;
    a.opacity = 0.5;
    let mut b = Layer::new("Внутри");
    b.pixels.set(30, 30, from_rgba8([255, 0, 0, 255]));
    b.visible = false;
    let mut folder = Layer::folder("Папка", vec![b]);
    folder.pass_through = true;
    doc.layers.push(a);
    doc.layers.push(folder);
    doc
}

#[test]
fn psd_round_trip_keeps_layers_and_picture() {
    let doc = sample();
    let dir = std::env::temp_dir().join(format!("sosai-psd-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("проба.psd");
    paint_formats::psd::save(&doc, &path).unwrap();
    let back = paint_formats::psd::load(&path).unwrap();
    assert_eq!((back.width, back.height), (64, 40));
    let names: Vec<&str> = back.layers.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, ["Фон слой", "Линии", "Папка"]);
    assert_eq!(back.layers[1].mode, BlendMode::Multiply);
    assert!((back.layers[1].opacity - 0.5).abs() < 0.01);
    let f = &back.layers[2];
    assert!(f.pass_through);
    let inner = &f.children.as_ref().unwrap()[0];
    assert_eq!(inner.name, "Внутри");
    assert!(!inner.visible);
    // внутри 15 бит на канал, в PSD — 8: допустима разница в 1 уровень
    let worst = back.flatten().iter().zip(doc.flatten()).map(|(a, b)| (*a as i32 - b as i32).abs()).max().unwrap();
    assert!(worst <= 1, "наибольшая разница {worst}");
    if let Ok(keep) = std::env::var("PSD_KEEP") {
        std::fs::copy(&path, keep).unwrap();
    }
    std::fs::remove_dir_all(dir).ok();
}

/// PSD, записанный другой программой (photocraft-cli): слои и папка читаются.
#[test]
fn reads_psd_written_by_another_program() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    let doc = paint_formats::psd::load(&dir.join("photocraft-layers.psd")).unwrap();
    let names: Vec<&str> = doc.layers.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, ["Фон слой", "Линии", "Папка"]);
    assert_eq!(doc.layers[1].mode, BlendMode::Multiply);
    assert!(doc.layers[2].children.as_ref().is_some_and(|c| c[0].name == "Внутри" && !c[0].visible));
    let want = sample().flatten();
    let worst = doc.flatten().iter().zip(want).map(|(a, b)| (*a as i32 - b as i32).abs()).max().unwrap();
    assert!(worst <= 1, "наибольшая разница {worst}");
    // однослойный PSD из PNG
    let flat = paint_formats::psd::load(&dir.join("photocraft-flat.psd")).unwrap();
    assert_eq!((flat.width, flat.height), (360, 256));
    assert!(!flat.layers.is_empty());
}

/// PSD из Krita 5.2.9 (8 и 16 бит): слои, маска прозрачности, папка с умножением; сведение
/// сверяется с картинкой, которую сохранила сама Krita (скрипт `make_psd.py` навыка krita).
#[test]
fn reads_krita_psd_with_mask_and_16_bits() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    for bits in [8, 16] {
        let doc = paint_formats::psd::load(&dir.join(format!("krita-probe-{bits}.psd"))).unwrap();
        let top = doc.layers.iter().find(|l| l.name == "top").expect("слой top");
        assert!(top.mask.is_some(), "{bits} бит: маска слоя не прочитана");
        let (w, h, want) = paint_core::io::load_png(&dir.join(format!("krita-probe-{bits}.png"))).unwrap();
        assert_eq!((w, h), (doc.width, doc.height));
        let got = doc.flatten();
        // 16 бит у Krita по умолчанию — линейный профиль: цвета переводятся в sRGB, но слои Krita
        // смешивает в линейном свете, а SOsai (как SAI) — в sRGB; поэтому для 16 бит сверяются
        // точки вне полупрозрачного слоя top (8..48 × 8..38)
        let skip = |i: usize| bits == 16 && (8..48).contains(&(i / 4 % w as usize)) && (8..38).contains(&(i / 4 / w as usize));
        let worst = got.iter().zip(&want).enumerate().filter(|(i, _)| !skip(*i)).map(|(_, (a, b))| (*a as i32 - *b as i32).abs()).max().unwrap();
        assert!(worst <= 2, "{bits} бит: наибольшая разница {worst}");
    }
}
