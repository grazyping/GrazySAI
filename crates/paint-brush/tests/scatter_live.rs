//! «Разброс» и «Перо эффектов» против живой SAI 2026-07-02b (docs/41). Лист: 8 мазков мышью по
//! горизонтали (x 22…492, через 60 px по y), размер 10, шаг 201 %; в SAI сдвиг форм от линии мазка
//! при разбросе 201 % — равномерно ±20 px (σ 11,5–11,8), по Гауссу при 99 % — σ 4,4; размер при
//! разбросе размера 100 % — в среднем половина.

use paint_brush::ToolKind;
use paint_core::{Document, InputPoint};

fn sheet(sc: impl Fn(&mut paint_core::scatter::ScatterParams)) -> Vec<(f32, f32, f32)> {
    let mut b = ToolKind::Scatter.new_brush();
    b.params.size = 10.0;
    b.params.spacing = 2.01;
    sc(b.params.scatter.as_mut().unwrap());
    let mut doc = Document::new(512, 512);
    for (n, y) in (34..=454).step_by(60).enumerate() {
        let mut st = b.stroke(None);
        st.seed = n as u64 * 7919;
        for x in (22..=492).step_by(4) {
            st.add_point(InputPoint { x: x as f32, y: y as f32, pressure: 1.0 });
        }
        doc.commit_stroke(0, &st, [0, 0, 0]);
    }
    // пятна: связные области с альфой > 50 %, центр и площадь
    let flat = doc.layers[0].pixels.clone();
    let mut seen = vec![false; 512 * 512];
    let mut out = Vec::new();
    for y0 in 0..512 {
        for x0 in 0..512 {
            let on = |x: usize, y: usize| paint_core::pixel::to_rgba8(flat.get(x as i32, y as i32))[3] > 128;
            if seen[y0 * 512 + x0] || !on(x0, y0) {
                continue;
            }
            let (mut sx, mut sy, mut n, mut stack) = (0.0, 0.0, 0.0, vec![(x0, y0)]);
            seen[y0 * 512 + x0] = true;
            while let Some((x, y)) = stack.pop() {
                sx += x as f32;
                sy += y as f32;
                n += 1.0;
                for (dx, dy) in [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)] {
                    let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                    if (0..512).contains(&nx) && (0..512).contains(&ny) && !seen[ny as usize * 512 + nx as usize] && on(nx as usize, ny as usize) {
                        seen[ny as usize * 512 + nx as usize] = true;
                        stack.push((nx as usize, ny as usize));
                    }
                }
            }
            let (cx, cy) = (sx / n, sy / n);
            let line = ((cy - 34.0) / 60.0).round() * 60.0 + 34.0;
            out.push((cx, cy - line, n));
        }
    }
    out
}

fn sd(v: &[f32]) -> f32 {
    let m = v.iter().sum::<f32>() / v.len() as f32;
    (v.iter().map(|x| (x - m) * (x - m)).sum::<f32>() / v.len() as f32).sqrt()
}

#[test]
fn scattering_matches_sai() {
    let blobs = sheet(|s| s.scattering = 2.01);
    let dy: Vec<f32> = blobs.iter().map(|b| b.1).collect();
    assert!(blobs.len() > 150, "{}", blobs.len());
    assert!(dy.iter().all(|d| d.abs() <= 21.0));
    let s = sd(&dy);
    assert!((10.4..12.9).contains(&s), "σ {s} (SAI 11,5–11,8)");
    let g = sheet(|s| {
        s.scattering = 0.99;
        s.gaussian = true;
    });
    let s = sd(&g.iter().map(|b| b.1).collect::<Vec<_>>());
    assert!((3.9..4.9).contains(&s), "σ {s} (SAI 4,4)");
}

#[test]
fn size_jitter_matches_sai() {
    let full = sheet(|_| {});
    let half = sheet(|s| s.size_jitter = 1.0);
    let ink = |v: &[(f32, f32, f32)]| v.iter().map(|b| b.2.sqrt()).sum::<f32>() / v.len() as f32;
    let k = ink(&half) / ink(&full);
    // в SAI средний «диаметр по краске» 4,0 из 8,5 — 0,47
    assert!((0.38..0.6).contains(&k), "{k}");
}

/// Перо эффектов, «Умножение»: красное по красному (252, 0, 0) — 249, как в SAI; на прозрачном —
/// обычная краска.
#[test]
fn effect_pen_multiply() {
    let mut doc = Document::new(100, 100);
    let mut base = ToolKind::Pencil.new_brush();
    base.params.size = 30.0;
    let mut st = base.stroke(None);
    for x in 0..=100 {
        st.add_point(InputPoint { x: x as f32, y: 30.0, pressure: 1.0 });
    }
    doc.commit_stroke(0, &st, [252, 0, 0]);
    let mut b = ToolKind::EffectPen.new_brush();
    b.params.effect = Some(paint_core::blend::BlendMode::Multiply);
    let mut st = b.stroke(None);
    for y in 0..=100 {
        st.add_point(InputPoint { x: 50.0, y: y as f32, pressure: 1.0 });
    }
    doc.commit_stroke(0, &st, [252, 0, 0]);
    let px = |x, y| paint_core::pixel::to_rgba8(doc.layers[0].pixels.get(x, y));
    let over = px(50, 30);
    assert!((over[0] as i32 - 249).abs() <= 1 && over[3] == 255, "{over:?}");
    assert_eq!(px(50, 80), [252, 0, 0, 255]);
}

/// Цвет у каждой формы: разброс оттенка 100 % даёт разные цвета внутри одного мазка.
#[test]
fn hue_jitter_each_shape() {
    let mut b = ToolKind::Scatter.new_brush();
    b.params.size = 10.0;
    b.params.spacing = 2.01;
    let sc = b.params.scatter.as_mut().unwrap();
    sc.hue_jitter = 1.0;
    sc.each_shape = true;
    let mut doc = Document::new(300, 40);
    let mut live = paint_core::LiveStroke::new(vec![0], b.stroke(None), [255, 0, 0]);
    live.stroke.shape_colors = Some(([255, 0, 0], [255, 255, 255]));
    for x in (10..=290).step_by(4) {
        live.stroke.add_point(InputPoint { x: x as f32, y: 20.0, pressure: 1.0 });
        doc.live_update(&mut live);
    }
    doc.live_finish(live);
    let greens = (0..300).filter(|&x| {
        let p = paint_core::pixel::to_rgba8(doc.layers[0].pixels.get(x, 20));
        p[3] > 200 && p[1] > 150
    });
    assert!(greens.count() > 5);
    // .gsbrush сохраняет настройки разброса
    let mut kv = paint_brush::Kv::default();
    paint_brush::write(&b, &mut kv, "");
    let mut back = ToolKind::Scatter.new_brush();
    paint_brush::read(&mut back, &kv, "");
    assert_eq!(back.params.scatter, b.params.scatter);
}
