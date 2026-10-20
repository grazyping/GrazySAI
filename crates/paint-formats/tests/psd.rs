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
    let dir = std::env::temp_dir().join(format!("grazysai-psd-{}", std::process::id()));
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

/// PSB (большой документ): версия 2 в заголовке, длины по 8 байт — слои и картинка те же, что
/// у PSD; один и тот же документ читается обратно одинаково из обоих форматов.
#[test]
fn psb_round_trip_matches_psd() {
    let doc = sample();
    let dir = std::env::temp_dir().join(format!("grazysai-psb-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (psd, psb) = (dir.join("проба.psd"), dir.join("проба.psb"));
    paint_formats::psd::save(&doc, &psd).unwrap();
    paint_formats::psd::save_psb(&doc, &psb).unwrap();
    let raw = std::fs::read(&psb).unwrap();
    assert_eq!(&raw[..6], b"8BPS\0\x02", "версия PSB");
    let (a, b) = (paint_formats::psd::load(&psd).unwrap(), paint_formats::psd::load(&psb).unwrap());
    let names = |d: &Document| d.layers.iter().map(|l| l.name.clone()).collect::<Vec<_>>();
    assert_eq!(names(&a), names(&b));
    assert_eq!(b.layers[1].mode, BlendMode::Multiply);
    assert!(b.layers[2].pass_through);
    assert_eq!(a.flatten(), b.flatten());
    if let Ok(keep) = std::env::var("PSB_KEEP") {
        std::fs::copy(&psb, keep).unwrap();
    }
    std::fs::remove_dir_all(dir).ok();
}

/// Маска слоя уходит в PSD (канал −2 на весь холст) и читается обратно; сведение то же.
#[test]
fn psd_keeps_layer_mask() {
    let mut doc = Document::new(48, 32);
    for y in 0..32 {
        for x in 0..48 {
            doc.layers[0].pixels.set(x, y, from_rgba8([40, 90, 200, 255]));
        }
    }
    let mut hide = paint_core::tile::MaskMap::new();
    for y in 0..32 {
        for x in 0..24 {
            hide.set(x, y, 0x4000);
        }
        hide.set(30, y, 0x2000);
    }
    doc.layers[0].mask = Some(paint_core::layer::LayerMask { hide, flags: [1, 1, 1, 0] });
    let dir = std::env::temp_dir().join(format!("grazysai-psdmask-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("маска.psd");
    paint_formats::psd::save(&doc, &path).unwrap();
    let back = paint_formats::psd::load(&path).unwrap();
    let m = &back.layers[0].mask.as_ref().expect("маска прочитана").hide;
    assert_eq!((m.get(5, 5), m.get(40, 5)), (0x4000, 0));
    assert!((m.get(30, 7) as i32 - 0x2000).abs() <= 0x4000 / 255 + 1, "{}", m.get(30, 7));
    let worst = back.flatten().iter().zip(doc.flatten()).map(|(a, b)| (*a as i32 - b as i32).abs()).max().unwrap();
    assert!(worst <= 2, "наибольшая разница {worst}");
    if let Ok(keep) = std::env::var("PSD_MASK_KEEP") {
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
        // смешивает в линейном свете, а GrazySAI (как SAI) — в sRGB; поэтому для 16 бит сверяются
        // точки вне полупрозрачного слоя top (8..48 × 8..38)
        let skip = |i: usize| bits == 16 && (8..48).contains(&(i / 4 % w as usize)) && (8..38).contains(&(i / 4 / w as usize));
        let worst = got.iter().zip(&want).enumerate().filter(|(i, _)| !skip(*i)).map(|(_, (a, b))| (*a as i32 - *b as i32).abs()).max().unwrap();
        assert!(worst <= 2, "{bits} бит: наибольшая разница {worst}");
    }
}

/// 16 бит на канал: в заголовке глубина 16, пиксели слоёв (15 бит внутри) возвращаются без потерь
/// 8-битного PSD — плавный переход тоньше 1/255 сохраняется; маска и PSB — тоже.
#[test]
fn psd_16_bit_keeps_fine_steps() {
    let mut doc = sample();
    // переход, который в 8 битах слился бы в один уровень
    for x in 0..64 {
        doc.layers[0].pixels.set(x, 0, [16000 + x as u16 * 3, 8000, 4000, paint_core::pixel::ONE as u16]);
    }
    let mut hide = paint_core::tile::MaskMap::new();
    hide.set(3, 3, 0x1234);
    doc.layers[1].mask = Some(paint_core::layer::LayerMask { hide, flags: [1, 1, 1, 0] });
    let dir = std::env::temp_dir().join(format!("grazysai-psd16-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for big in [false, true] {
        let path = dir.join(if big { "16.psb" } else { "16.psd" });
        paint_formats::psd::save_with(&doc, &path, big, true).unwrap();
        let raw = std::fs::read(&path).unwrap();
        assert_eq!(u16::from_be_bytes([raw[22], raw[23]]), 16, "глубина в заголовке");
        let back = paint_formats::psd::load(&path).unwrap();
        for x in 0..64 {
            let (a, b) = (back.layers[0].pixels.get(x, 0), doc.layers[0].pixels.get(x, 0));
            assert!((a[0] as i32 - b[0] as i32).abs() <= 1, "x={x}: {a:?} против {b:?}");
        }
        let m = back.layers[1].mask.as_ref().expect("маска").hide.get(3, 3);
        assert!((m as i32 - 0x1234).abs() <= 1, "маска {m:#x}");
        let worst = back.flatten().iter().zip(doc.flatten()).map(|(a, b)| (*a as i32 - b as i32).abs()).max().unwrap();
        assert!(worst <= 1, "сведение: {worst}");
        if let Ok(keep) = std::env::var("PSD16_KEEP") {
            std::fs::copy(&path, format!("{keep}.{}", if big { "psb" } else { "psd" })).unwrap();
        }
    }
    std::fs::remove_dir_all(dir).ok();
}
