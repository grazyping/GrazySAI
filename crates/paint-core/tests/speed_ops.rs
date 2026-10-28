//! Скорость тяжёлых операций на холсте 4K: время и отпечаток результата (ускорение не должно его
//! менять). `cargo test --release -p paint-core --test speed_ops -- --ignored --nocapture`;
//! `SPEED_ONLY=имя` — только одна операция.
use paint_core::brush::BrushParams;
use paint_core::document::{Document, LiveStroke};
use paint_core::pixel::from_rgba8;
use paint_core::stroke::{InputPoint, Stroke};
use paint_core::tile::{ColorMap, MaskMap};
use std::time::Instant;

const W: u32 = 3840;
const H: u32 = 2160;

fn fnv(h: &mut u64, v: u64) {
    *h ^= v;
    *h = h.wrapping_mul(0x100000001b3);
}

fn hash_map<T: Copy + Default + Into<u64>>(m: &paint_core::tile::TileMap<[T; 4]>) -> u64 {
    let mut keys: Vec<_> = m.tiles.keys().copied().collect();
    keys.sort_unstable();
    let mut h = 0xcbf29ce484222325;
    for k in keys {
        fnv(&mut h, (k.0 as u64) << 32 | k.1 as u32 as u64);
        for p in m.tiles[&k].0.iter() {
            for c in p {
                fnv(&mut h, (*c).into());
            }
        }
    }
    h
}

fn hash_mask(m: &MaskMap) -> u64 {
    let mut keys: Vec<_> = m.tiles.keys().copied().collect();
    keys.sort_unstable();
    let mut h = 0xcbf29ce484222325;
    for k in keys {
        fnv(&mut h, (k.0 as u64) << 32 | k.1 as u32 as u64);
        for v in m.tiles[&k].0.iter() {
            fnv(&mut h, u64::from(*v));
        }
    }
    h
}

/// Цветной рисунок на весь холст: плавные полосы и круги — у фильтров есть что размывать.
fn picture() -> ColorMap {
    let mut m = ColorMap::new();
    for y in 0..H as i32 {
        for x in 0..W as i32 {
            let r = ((x / 7) % 256) as u8;
            let g = ((y / 5) % 256) as u8;
            let d = ((x - 1900) * (x - 1900) + (y - 1080) * (y - 1080)) as f32;
            let b = if d.sqrt() % 300.0 < 150.0 { 220 } else { 40 };
            m.set(x, y, from_rgba8([r, g, b, 255]));
        }
    }
    m
}

/// Контур: сетка линий толщиной 3 px с разрывами по 4 px — для заливки с «Закрыть щели».
fn line_art() -> ColorMap {
    let mut m = ColorMap::new();
    for y in 0..H as i32 {
        for x in 0..W as i32 {
            let on_v = x % 240 < 3 && (y % 200) > 4;
            let on_h = y % 200 < 3 && (x % 240) > 4;
            if on_v || on_h {
                m.set(x, y, from_rgba8([0, 0, 0, 255]));
            }
        }
    }
    m
}

fn wave(n: usize) -> Vec<InputPoint> {
    (0..=n)
        .map(|i| {
            let t = i as f32 / n as f32;
            InputPoint { x: 200.0 + t * 3400.0, y: 1080.0 + (t * 9.0).sin() * 700.0, pressure: 0.3 + 0.7 * (t * std::f32::consts::PI).sin() }
        })
        .collect()
}

/// Мазок, как в программе: точки приходят по одной, слой обновляется каждые 4 точки.
fn live_stroke(doc: &mut Document, params: BrushParams) {
    let mut live = LiveStroke::new(vec![0], Stroke::new(params), [180, 40, 60]);
    for (i, p) in wave(600).into_iter().enumerate() {
        live.stroke.add_point(p);
        if i % 4 == 0 {
            doc.live_update(&mut live);
        }
    }
    doc.live_finish(live);
}

fn run(name: &str, f: impl FnOnce() -> u64) {
    if let Ok(only) = std::env::var("SPEED_ONLY") {
        if only != name {
            return;
        }
    }
    let t = Instant::now();
    let h = f();
    println!("{name:<28} {:>8.0} мс   отпечаток {h:016x}", t.elapsed().as_secs_f64() * 1000.0);
}

#[test]
#[ignore = "замер скорости, только по запросу"]
fn speed_ops() {
    let pic = picture();
    let lines = line_art();
    let all = |_: i32, _: i32| 32768u32;
    println!("потоков: {}", paint_core::threads());

    run("карандаш 400 px", || {
        let mut doc = Document::new(W, H);
        live_stroke(&mut doc, BrushParams { size: 400.0, hardness: 0.8, ..Default::default() });
        hash_map(&doc.layers[0].pixels)
    });
    run("кисть 300 px, смешивание", || {
        let mut doc = Document::new(W, H);
        doc.layers[0].pixels = pic.clone();
        let mix = paint_core::mix::MixParams::new(0.5, 0.3, 0.5);
        live_stroke(&mut doc, BrushParams { size: 300.0, mix: Some(mix), ..Default::default() });
        hash_map(&doc.layers[0].pixels)
    });
    run("акварель 300 px", || {
        let mut doc = Document::new(W, H);
        doc.layers[0].pixels = pic.clone();
        let mix = paint_core::mix::MixParams { blur: true, ..paint_core::mix::MixParams::new(0.5, 0.5, 0.3) };
        live_stroke(&mut doc, BrushParams { size: 300.0, mix: Some(mix), ..Default::default() });
        hash_map(&doc.layers[0].pixels)
    });
    run("растушёвка 300 px", || {
        let mut doc = Document::new(W, H);
        doc.layers[0].pixels = pic.clone();
        let mix = paint_core::mix::MixParams { smudge: true, blend_by_pressure: false, ..paint_core::mix::MixParams::new(0.0, 0.0, 0.5) };
        live_stroke(&mut doc, BrushParams { size: 300.0, mix: Some(mix), ..Default::default() });
        hash_map(&doc.layers[0].pixels)
    });
    run("размытие по Гауссу r=40", || hash_map(&paint_core::filters::gaussian_blur(&pic, W, H, 40.0, &all)));
    run("размытие линзы r=20", || hash_map(&paint_core::filters::lens_blur(&pic, W, H, 20.0, 6, 0.5, &all)));
    run("медиана r=3", || hash_map(&paint_core::filters::median(&pic, W, H, 3, &all)));
    run("нерезкая маска r=10", || hash_map(&paint_core::filters::unsharp_mask(&pic, W, H, 10.0, 1.0, 2, &all)));
    run("палочка", || hash_mask(&paint_core::selection::magic_wand(&lines, W, H, 120, 100, 10)));
    run("заливка, закрыть щели 5", || {
        let o = paint_core::selection::WandOptions { tolerance: 10, gap: 5, expand: 2, omissions: true };
        hash_mask(&paint_core::selection::magic_wand_closing(&lines, W, H, 120, 100, o))
    });
    run("выделение +25 px", || {
        let mut s = paint_core::selection::Selection::default();
        s.combine(&paint_core::selection::rect(500.0, 400.0, 3300.0, 1800.0), paint_core::selection::Combine::Replace);
        s.grow_by(25, true);
        hash_mask(&s.mask)
    });
    run("поворот 4K, Ланцош 3", || {
        let sel = paint_core::selection::Selection::default();
        let (a, c) = (0.3f64.cos(), 0.3f64.sin());
        let m = [a, -c, 1920.0 - a * 1920.0 + c * 1080.0, c, a, 1080.0 - c * 1920.0 - a * 1080.0];
        let (px, _) = paint_core::selection::transform_pixels(&pic, &sel, m, paint_core::resample::Mode::Lanczos3);
        hash_map(&px)
    });
    run("раскраска по контуру", || {
        let w = W as usize;
        let mut rgba = vec![0u8; w * H as usize * 4];
        let mut hints = vec![0u8; w * H as usize * 4];
        for (i, o) in rgba.chunks_mut(4).enumerate() {
            o.copy_from_slice(&paint_core::pixel::to_rgba8(lines.get((i % w) as i32, (i / w) as i32)));
        }
        for k in 0..60 {
            let (x, y) = (120 + (k % 15) * 240, 100 + (k / 15) * 200);
            let o = (y * w + x) * 4;
            hints[o..o + 4].copy_from_slice(&[(k * 37 % 255) as u8, 100, 200, 255]);
        }
        let (px, n) = paint_core::colorize::colorize(&rgba, &hints, w, H as usize, 3).unwrap_or_default();
        let mut h = n as u64;
        for v in px.iter().step_by(97) {
            fnv(&mut h, u64::from(*v));
        }
        h
    });
}
