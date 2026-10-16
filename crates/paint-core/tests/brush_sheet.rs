//! Лист кистей против живого образца (сборка 2017-01-19, карандаш, мышь): пять ступеней
//! жёсткости × отпечатки 1, 2, 5, 10, 20, 30, 50, 100 px и линия 20 px. Снимок образца —
//! `tests/data/brush/ref-sheet1-pencil-d100.png` (docs/08-brush-verification.md).
//! Сдвиг (1,5; −1,5) — положение холста на экране относительно координат мыши.

use paint_core::{BrushParams, Document, InputPoint, Stroke};
use std::path::Path;

fn sheet() -> Vec<u8> {
    let (dx, dy) = (1.5f32, -1.5f32);
    let mut doc = Document::new(700, 600);
    let xs = [20.0f32, 40.0, 65.0, 95.0, 135.0, 185.0, 255.0, 360.0];
    let sizes = [1.0f32, 2.0, 5.0, 10.0, 20.0, 30.0, 50.0, 100.0];
    let mut commit = |params: BrushParams, pts: &[(f32, f32)]| {
        let mut s = Stroke::new(params);
        for &(x, y) in pts {
            s.add_point(InputPoint { x, y, pressure: 1.0 });
        }
        doc.commit_stroke(0, &s, [0, 0, 0]);
    };
    for (row, hardness) in paint_core::brush::HARDNESS_STEPS.iter().enumerate() {
        let y = 60.0 + row as f32 * 110.0 + dy;
        for (x, size) in xs.iter().zip(sizes) {
            commit(BrushParams { size, hardness: *hardness, size_by_pressure: false, ..Default::default() }, &[(x + dx, y)]);
        }
        let p = BrushParams { size: 20.0, hardness: *hardness, size_by_pressure: false, ..Default::default() };
        commit(p, &[(450.0 + dx, y), (670.0 + dx, y)]);
    }
    doc.flatten()
}

#[test]
fn brush_sheet_matches_the_reference_program() {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/brush/ref-sheet1-pencil-d100.png");
    let (w, _, reference) = paint_core::io::load_png(&p).unwrap();
    let mine = sheet();
    let gray = |b: &[u8], x: usize, y: usize| {
        let i = (y * w as usize + x) * 4;
        // как compare.py: яркость поверх белого
        let a = b[i + 3] as f32 / 255.0;
        let l = 0.299 * b[i] as f32 + 0.587 * b[i + 1] as f32 + 0.114 * b[i + 2] as f32;
        (l * a + 255.0 * (1.0 - a)).round()
    };
    let xs = [20usize, 40, 65, 95, 135, 185, 255, 360];
    let sizes = [1usize, 2, 5, 10, 20, 30, 50, 100];
    for h in 0..5 {
        let y = 58 + 110 * h;
        for (x, s) in xs.iter().zip(sizes) {
            let r = ((s as f32 * 0.95) as usize + 4).min(54);
            let mut worst = 0f32;
            for yy in y - r..=y + r {
                for xx in x + 1 - r..=x + 1 + r {
                    worst = worst.max((gray(&mine, xx, yy) - gray(&reference, xx, yy)).abs());
                }
            }
            // ступени 1–4 — модель образца, совпадение до округления; самая жёсткая — до 4
            let limit = if h < 4 { 1.0 } else { 4.0 };
            if h == 4 { eprintln!("ступень 5 размер {s}: {worst}"); }
            assert!(worst <= limit, "ступень {} размер {s}: расхождение {worst}", h + 1);
        }
    }
}

/// Линии самой жёсткой ступени против живого образца: 10–30 px (`ref-sheet3-hard-lines.png`) и
/// 50, 100 px (`ref-sheet4-hard-lines-50-100.png`); мышь, сдвиг холста (4,5; 1,5). С 20 px
/// образец ограничивает отпечатки маской штриха (docs/13-thin-brushes.md, раздел 4).
#[test]
fn hard_lines_match_the_reference_program() {
    /// Размер, точка отпечатка, линия (x0, x1, y).
    type Item = (f32, (f32, f32), (f32, f32, f32));
    let (dx, dy) = (4.5f32, 1.5f32);
    let draw = |items: &[Item]| {
        let mut doc = Document::new(540, 540);
        for &(size, dab, line) in items {
            let params = BrushParams { size, hardness: 1.0, size_by_pressure: false, ..Default::default() };
            for pts in [vec![dab], vec![(line.0, line.2), (line.1, line.2)]] {
                let mut s = Stroke::new(params);
                for (x, y) in pts {
                    s.add_point(InputPoint { x: x + dx, y: y + dy, pressure: 1.0 });
                }
                doc.commit_stroke(0, &s, [0, 0, 0]);
            }
        }
        doc.flatten()
    };
    let small: Vec<_> = [10.0f32, 12.0, 14.0, 16.0, 20.0, 25.0, 30.0]
        .iter()
        .enumerate()
        .map(|(i, &s)| (s, (30.0, 30.0 + i as f32 * 70.0), (80.0, 520.0, 30.0 + i as f32 * 70.0)))
        .collect();
    let large = [(50.0f32, (40.0, 60.0), (100.0, 520.0, 60.0)), (100.0, (70.0, 230.0), (70.0, 480.0, 420.0))];
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/brush");
    for (file, items, limits) in [
        ("ref-sheet3-hard-lines.png", small.as_slice(), [0.6f32, 0.6, 0.6, 0.6, 0.6, 0.6, 0.6].as_slice()),
        ("ref-sheet4-hard-lines-50-100.png", large.as_slice(), [0.6f32, 2.5].as_slice()),
    ] {
        let (_, _, reference) = paint_core::io::load_png(&dir.join(file)).unwrap();
        let mine = draw(items);
        let gray = |b: &[u8], x: usize, y: usize| {
            let i = (y * 540 + x) * 4;
            let a = b[i + 3] as f32 / 255.0;
            let l = 0.299 * b[i] as f32 + 0.587 * b[i + 1] as f32 + 0.114 * b[i + 2] as f32;
            (l * a + 255.0 * (1.0 - a)).round()
        };
        for (&(size, _, (x0, x1, y)), limit) in items.iter().zip(limits) {
            let (s, y) = (size as usize, y as usize);
            let r = (size * 0.6) as usize + 4;
            let (mut e, mut n) = (0.0f32, 0.0f32);
            for yy in y - r..=y + r {
                for xx in x0 as usize + s..x1 as usize - s {
                    let d = gray(&mine, xx, yy) - gray(&reference, xx, yy);
                    e += d * d;
                    n += 1.0;
                }
            }
            let rms = (e / n).sqrt();
            eprintln!("линия {size}: {rms:.2}");
            assert!(rms <= *limit, "линия {size}: {rms} больше {limit}");
        }
    }
}
