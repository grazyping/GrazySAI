//! Скорость показа холста 4K с многими слоями: `cargo test --release -p paint-core --test speed_4k -- --ignored --nocapture`.
use paint_core::document::Document;
use paint_core::layer::Layer;
use paint_core::pixel::from_rgba8;

fn doc_4k(layers: usize, cover: f32) -> Document {
    let mut doc = Document::new(3840, 2160);
    for n in 0..layers {
        let mut l = Layer::new(&format!("{n}"));
        // полоса через холст: доля `cover` высоты, у каждого слоя своя
        let h = (2160.0 * cover) as u32;
        let y0 = (n as u32 * 97) % (2160 - h.min(2159));
        for y in y0..y0 + h {
            for x in 0..3840 {
                l.pixels.set(x, y as i32, from_rgba8([(n * 9) as u8, 120, 200, 180]));
            }
        }
        doc.layers.push(l);
    }
    doc
}

fn all_tiles(doc: &Document) -> std::time::Duration {
    let keys: Vec<(i32, i32)> = (0..34).flat_map(|ty| (0..60).map(move |tx| (tx, ty))).collect();
    let threads = paint_core::threads();
    let t = std::time::Instant::now();
    std::thread::scope(|s| {
        for part in keys.chunks(keys.len().div_ceil(threads)) {
            s.spawn(move || part.iter().map(|k| doc.render_tile(*k).len()).sum::<usize>());
        }
    });
    t.elapsed()
}

#[test]
#[ignore]
fn speed_4k() {
    for (layers, cover) in [(1, 1.0), (10, 0.5), (25, 0.5), (25, 1.0)] {
        let doc = doc_4k(layers, cover);
        let _ = all_tiles(&doc);
        eprintln!("слоёв {layers:2}, покрытие {cover}: весь холст {:?} ({} потоков)", all_tiles(&doc), paint_core::threads());
    }
}
