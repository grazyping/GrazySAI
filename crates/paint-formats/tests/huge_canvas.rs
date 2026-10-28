//! Холст 100000×100000 (как наибольший у SAI 2) с небольшим рисунком в дальнем углу: каждая
//! операция — отдельный тест, с временем и памятью процесса. Запуск под пределом памяти:
//! `ulimit -v 16000000; cargo test --release -p paint-formats --test huge_canvas -- --ignored --nocapture --test-threads=1`
use paint_core::brush::BrushParams;
use paint_core::document::{Document, LiveStroke};
use paint_core::stroke::{InputPoint, Stroke};
use std::time::Instant;

const SIDE_MAX: u32 = 100_000;

/// Сторона холста: 100000 или `HUGE_SIDE`.
fn side() -> u32 {
    std::env::var("HUGE_SIDE").ok().and_then(|v| v.parse().ok()).unwrap_or(SIDE_MAX)
}

fn rss_mb() -> u64 {
    let s = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    s.lines().find(|l| l.starts_with("VmRSS:")).and_then(|l| l.split_whitespace().nth(1)).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0) / 1024
}

fn report(name: &str, t: Instant) {
    println!("{name:<34} {:>8.0} мс   память {:>6} МБ", t.elapsed().as_secs_f64() * 1000.0, rss_mb());
}

/// Холст с мазком и замкнутым кольцом у дальнего угла (95000, 95000).
fn drawing() -> Document {
    let mut doc = Document::new(side(), side());
    let mut live = LiveStroke::new(vec![0], Stroke::new(BrushParams { size: 60.0, ..Default::default() }), [20, 20, 20]);
    for i in 0..=400 {
        let a = i as f32 / 400.0 * std::f32::consts::TAU;
        live.stroke.add_point(InputPoint { x: (side() - 5_000) as f32 + a.cos() * 900.0, y: (side() - 5_000) as f32 + a.sin() * 900.0, pressure: 1.0 });
        if i % 8 == 0 {
            doc.live_update(&mut live);
        }
    }
    doc.live_finish(live);
    doc
}

#[test]
#[ignore = "проверка большого холста, только по запросу"]
fn huge_1_create_draw_show_undo() {
    let t = Instant::now();
    let doc = Document::new(side(), side());
    report("новый холст", t);
    drop(doc);
    let t = Instant::now();
    let mut doc = drawing();
    report("мазок 60 px у дальнего угла", t);
    let t = Instant::now();
    // плитка на мазке (кольцо радиусом 900 вокруг (95000, 95000)): не пустая
    let tile = doc.render_tile(((side() as i32 - 4_100) / 64, (side() as i32 - 5_000) / 64));
    assert!(tile.as_chunks::<4>().0.iter().any(|p| p[3] != 0));
    report("показ плитки", t);
    let t = Instant::now();
    assert!(doc.undo().is_some());
    assert!(doc.redo().is_some());
    report("отмена и возврат", t);
}

#[test]
#[ignore = "проверка большого холста, только по запросу"]
fn huge_2_wand_fill_inside_ring() {
    let mut doc = drawing();
    let t = Instant::now();
    let m = paint_core::selection::magic_wand(&doc.layers[0].pixels, side(), side(), side() as i32 - 5_000, side() as i32 - 5_000, 10);
    report("палочка внутри кольца", t);
    let t = Instant::now();
    doc.fill(&[0], &m, [200, 40, 40]);
    report("заливка внутри кольца", t);
}

#[test]
#[ignore = "проверка большого холста, только по запросу"]
fn huge_3_filters_and_transform() {
    let doc = drawing();
    let all = |_: i32, _: i32| 32768u32;
    let t = Instant::now();
    let _ = paint_core::filters::gaussian_blur(&doc.layers[0].pixels, side(), side(), 20.0, &all);
    report("размытие по Гауссу r=20", t);
    let t = Instant::now();
    let _ = paint_core::filters::median(&doc.layers[0].pixels, side(), side(), 2, &all);
    report("медиана r=2", t);
    let t = Instant::now();
    let sel = paint_core::selection::Selection::default();
    let _ = paint_core::selection::transform_pixels(&doc.layers[0].pixels, &sel, [0.9, -0.2, 9000.0, 0.2, 0.9, -9000.0], paint_core::resample::Mode::Bicubic);
    report("поворот рисунка", t);
}

#[test]
#[ignore = "проверка большого холста, только по запросу"]
fn huge_4_save_and_open_sai2() {
    let doc = drawing();
    let dir = std::env::temp_dir().join(format!("gs-huge-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("huge.sai2");
    let t = Instant::now();
    let saved = paint_formats::sai2_write::save(&doc, &p);
    report("сохранение .sai2", t);
    saved.unwrap();
    println!("размер файла: {} КБ", std::fs::metadata(&p).map(|m| m.len() / 1024).unwrap_or(0));
    let t = Instant::now();
    let back = paint_formats::sai2_doc::load(&p);
    report("открытие .sai2", t);
    let back = back.unwrap().document;
    assert_eq!((back.width, back.height), (side(), side()));
    // HUGE_KEEP=путь — оставить файл (для проверки программы на нём)
    if let Some(keep) = std::env::var_os("HUGE_KEEP") {
        std::fs::copy(&p, keep).unwrap();
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
#[ignore = "проверка большого холста, только по запросу"]
fn huge_5_save_psd_ora() {
    let doc = drawing();
    let dir = std::env::temp_dir().join(format!("gs-huge5-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for ext in ["ora", "psb"] {
        let p = dir.join(format!("huge.{ext}"));
        let t = Instant::now();
        let r = paint_formats::convert::save(&doc, &p, Default::default());
        report(&format!("сохранение .{ext}"), t);
        println!("  итог: {:?}", r.as_ref().err().map(|e| e.to_string()));
    }
    let _ = std::fs::remove_dir_all(&dir);
}
