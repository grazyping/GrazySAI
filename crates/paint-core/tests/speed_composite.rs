//! Сведение 4K для показа (плитка за плиткой во всех потоках, как в окне):
//! `cargo test --release -p paint-core --test speed_composite -- --ignored --nocapture`.
use paint_core::{blend::BlendMode, layer::Layer, pixel::from_rgba8, tile::TILE, Document};
#[test]
#[ignore = "замер"]
fn composite_4k() {
    for n in [3usize, 20, 60] {
        let mut d = Document::new(3840, 2160);
        for i in 0..n {
            let mut l = Layer::new("л");
            if i % 3 == 1 { l.mode = BlendMode::Multiply; }
            if i % 3 == 2 { l.opacity = 0.7; }
            for y in 0..2160 {
                for x in 0..3840 {
                    l.pixels.set(x, y, from_rgba8([(x + i as i32) as u8, y as u8, i as u8 * 9, ((x / 8 + y / 8) as u8) | 1]));
                }
            }
            d.layers.push(l);
        }
        let keys: Vec<(i32, i32)> = (0..2160 / TILE + 1).flat_map(|y| (0..3840 / TILE + 1).map(move |x| (x, y))).collect();
        let next = std::sync::atomic::AtomicUsize::new(0);
        let t = std::time::Instant::now();
        std::thread::scope(|s| {
            for _ in 0..paint_core::threads() {
                s.spawn(|| loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some(k) = keys.get(i) else { break };
                    std::hint::black_box(d.render_tile(*k));
                });
            }
        });
        let all = t.elapsed();
        let tt = std::time::Instant::now();
        for k in keys.iter().take(64) { std::hint::black_box(d.render_tile(*k)); }
        eprintln!("слоёв {n}: весь холст {all:?}; одна плитка 64×64 {:?}", tt.elapsed() / 64);
    }
}
