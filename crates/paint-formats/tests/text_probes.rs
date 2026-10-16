//! Пробные файлы для проверки в SAI (`PROBE_DIR=<папка> cargo test --test text_probes -- --ignored`):
//! в каждом меняется одно — так видно, что SAI не принимает.

use paint_core::{Document, Layer};
use paint_formats::sai2_text::{self, TextLayer, TextRun};
use std::path::PathBuf;

fn text_layer(font: &str, text: &str, color: [u8; 3]) -> Layer {
    styled(font, text, color, 0, false)
}

fn styled(font: &str, text: &str, color: [u8; 3], style: u16, vertical: bool) -> Layer {
    let mut t = TextLayer::new(100.0, 100.0, TextRun { font: font.into(), size: 30.0, color, style, text: text.into() });
    if vertical {
        t.flags[0] = sai2_text::FLAG_VERTICAL;
    }
    let mut l = Layer::new("Text");
    let mut px = paint_core::tile::ColorMap::new();
    sai2_text::render(&t, &mut px);
    l.pixels = px;
    l.vector = Some(paint_core::layer::VectorData { kind: *b"text", raw: sai2_text::to_bytes(&t) });
    l
}

#[test]
#[ignore]
fn make_probes() {
    let dir = PathBuf::from(std::env::var("PROBE_DIR").expect("PROBE_DIR"));
    std::fs::create_dir_all(&dir).unwrap();
    let save = |name: &str, layers: Vec<Layer>| {
        let mut doc = Document::new(400, 300);
        doc.layers = layers;
        paint_formats::sai2_write::save(&doc, &dir.join(name)).unwrap();
    };
    save("1-без-текста.sai2", vec![Layer::new("Слой1")]);
    save("2-текст-noto.sai2", vec![Layer::new("Слой1"), text_layer("Noto Serif Thai Thin", "abc", [0, 38, 58])]);
    save("3-текст-noto-две-строки.sai2", vec![Layer::new("Слой1"), text_layer("Noto Serif Thai Thin", "abc\ndef", [0, 38, 58])]);
    save("4-текст-dejavu.sai2", vec![Layer::new("Слой1"), text_layer("DejaVu Sans", "abc", [0, 38, 58])]);
    save("5-только-текст.sai2", vec![text_layer("Noto Serif Thai Thin", "abc", [0, 38, 58])]);
    let f = "Noto Serif Thai Thin";
    save("7-жирный.sai2", vec![Layer::new("Слой1"), styled(f, "abc", [0, 38, 58], sai2_text::STYLE_BOLD, false)]);
    save("8-курсив.sai2", vec![Layer::new("Слой1"), styled(f, "abc", [0, 38, 58], sai2_text::STYLE_ITALIC, false)]);
    save("9-вертикально.sai2", vec![Layer::new("Слой1"), styled(f, "abc", [0, 38, 58], 0, true)]);
    save("10-жирный-курсив-вертикально.sai2", vec![Layer::new("Слой1"), styled(f, "abc", [180, 25, 47], sai2_text::STYLE_BOLD | sai2_text::STYLE_ITALIC, true)]);
    // файл пользователя из SAI, прочитанный и записанный GrazySAI
    let home = std::env::var("HOME").unwrap();
    if let Ok(l) = paint_formats::sai2_doc::load(&PathBuf::from(home).join("SAI2/НовыйХолст1.sai2")) {
        paint_formats::sai2_write::save(&l.document, &dir.join("6-ваш-файл-пересохранён.sai2")).unwrap();
    }
}
