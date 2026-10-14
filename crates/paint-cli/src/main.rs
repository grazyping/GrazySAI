//! Проверочный рендер без окна: рисует набор штрихов и сохраняет PNG.

use paint_core::*;
use std::path::Path;

fn stroke_along(params: BrushParams, stabilizer: usize, points: impl Iterator<Item = InputPoint>) -> Stroke {
    let mut stroke = Stroke::new(params);
    let mut st = Stabilizer::new(stabilizer);
    for p in points {
        let q = st.push(p);
        stroke.add_point(q);
    }
    for q in st.finish() {
        stroke.add_point(q);
    }
    stroke
}

/// Волна слева направо; давление нарастает к середине и спадает к концу.
fn wave(y: f32, amp: f32, jitter: f32) -> impl Iterator<Item = InputPoint> {
    (0..=300).map(move |i| {
        let t = i as f32 / 300.0;
        let shake = if i % 2 == 0 { jitter } else { -jitter };
        InputPoint {
            x: 40.0 + t * 720.0,
            y: y + (t * 12.0).sin() * amp + shake,
            pressure: (t * std::f32::consts::PI).sin(),
        }
    })
}

fn shrink_rgba(rgba: &[u8], w: usize, h: usize, k: usize) -> (Vec<u8>, usize, usize) {
    let (ow, oh) = (w / k, h / k);
    let mut out = vec![0u8; ow * oh * 4];
    for oy in 0..oh {
        for ox in 0..ow {
            let mut sum = [0u32; 4];
            for dy in 0..k {
                for dx in 0..k {
                    let o = ((oy * k + dy) * w + ox * k + dx) * 4;
                    for c in 0..4 {
                        sum[c] += rgba[o + c] as u32;
                    }
                }
            }
            for c in 0..4 {
                out[(oy * ow + ox) * 4 + c] = (sum[c] / (k * k) as u32) as u8;
            }
        }
    }
    (out, ow, oh)
}

/// `paint-cli sai2 <файл> <out.png> [уменьшение]` — сводит файл `.sai2` своим кодом и сравнивает
/// результат со сведённой картинкой, которую сохранил в файле сам образец.
fn render_sai2(path: &str, out: &str, shrink: usize) -> Result<(), Box<dyn std::error::Error>> {
    let loaded = paint_formats::sai2_doc::load(Path::new(path))?;
    let doc = &loaded.document;
    let (w, h) = (doc.width as usize, doc.height as usize);
    let mine = doc.flatten();
    for note in &loaded.warnings {
        println!("замечание: {note}");
    }
    match paint_formats::sai2_doc::read_flattened(Path::new(path)) {
        Ok(reference) => {
            let (mut exact, mut close, mut sum, mut worst) = (0usize, 0usize, 0u64, 0u8);
            let mut diff = vec![0u8; w * h * 4];
            for (i, (a, b)) in mine.as_chunks::<4>().0.iter().zip(reference.as_chunks::<4>().0.iter()).enumerate() {
                // поверх белого: у прозрачного пикселя цвет не важен
                let ow = |p: &[u8; 4], c: usize| ((p[c] as u32 * p[3] as u32 + 255 * (255 - p[3] as u32) + 127) / 255) as u8;
                let d = (0..3).map(|c| ow(a, c).abs_diff(ow(b, c))).max().unwrap_or(0);
                exact += usize::from(d == 0);
                close += usize::from(d <= 2);
                sum += d as u64;
                worst = worst.max(d);
                let v = d.saturating_mul(8);
                diff[i * 4..i * 4 + 4].copy_from_slice(&[v, v, v, 255]);
            }
            let n = (w * h) as f64;
            println!("сверка с образцом: точно {:.2}%, в пределах 2 единиц {:.2}%, средняя разница {:.3}, наибольшая {}", exact as f64 / n * 100.0, close as f64 / n * 100.0, sum as f64 / n, worst);
            let base = out.trim_end_matches(".png");
            let (r, rw, rh) = shrink_rgba(&reference, w, h, shrink);
            io::save_png(Path::new(&format!("{base}-reference.png")), rw as u32, rh as u32, &r)?;
            let (d, dw, dh) = shrink_rgba(&diff, w, h, shrink);
            io::save_png(Path::new(&format!("{base}-diff.png")), dw as u32, dh as u32, &d)?;
        }
        Err(e) => println!("сведённая картинка образца не прочитана: {e}"),
    }
    let (img, ow, oh) = shrink_rgba(&mine, w, h, shrink);
    io::save_png(Path::new(out), ow as u32, oh as u32, &img)?;
    println!("saved {out}: {ow}x{oh}, слоёв {}", loaded.info.layers.len());
    Ok(())
}

/// `paint-cli brushes <out.png>` — лист с отпечатками и мазками на пяти ступенях жёсткости.
fn brush_sheet(out: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut doc = Document::new(900, 620);
    for (row, hardness) in paint_core::brush::HARDNESS_STEPS.iter().enumerate() {
        let y = 60.0 + row as f32 * 115.0;
        let mut x = 30.0;
        for size in [1.0f32, 2.0, 5.0, 10.0, 30.0, 100.0] {
            let params = BrushParams { size, hardness: *hardness, size_by_pressure: false, ..Default::default() };
            x += size * 0.9 + 14.0;
            doc.commit_stroke(0, &stroke_along(params, 0, std::iter::once(InputPoint { x, y, pressure: 1.0 })), [20, 20, 20]);
            x += size * 0.9;
        }
        // Мазок с нажимом и мазок половинной плотности.
        let pen = BrushParams { size: 24.0, hardness: *hardness, ..Default::default() };
        let line = (0..=120).map(|i| {
            let t = i as f32 / 120.0;
            InputPoint { x: 430.0 + t * 430.0, y: y - 22.0 + (t * 9.0).sin() * 12.0, pressure: (t * std::f32::consts::PI).sin() }
        });
        doc.commit_stroke(0, &stroke_along(pen, 0, line), [30, 60, 160]);
        let half = BrushParams { size: 20.0, hardness: *hardness, density: 0.5, size_by_pressure: false, ..Default::default() };
        let straight = (0..=1).map(|i| InputPoint { x: 430.0 + i as f32 * 430.0, y: y + 28.0, pressure: 1.0 });
        doc.commit_stroke(0, &stroke_along(half, 0, straight), [160, 40, 40]);
    }
    io::save_png(Path::new(out), doc.width, doc.height, &doc.flatten())?;
    println!("saved {out}");
    Ok(())
}

/// `paint-cli refsheet <out.png> [dx dy]` — тот же лист, что нарисован в образце для сверки
/// (docs/08-brush-verification.md): на каждой из пяти ступеней жёсткости одиночные отпечатки
/// размеров 1, 2, 5, 10, 20, 30, 50, 100 и прямая линия размером 20. Чёрным по белому, 700×600.
fn reference_sheet(out: &str, dx: f32, dy: f32) -> Result<(), Box<dyn std::error::Error>> {
    let mut doc = Document::new(700, 600);
    let xs = [20.0f32, 40.0, 65.0, 95.0, 135.0, 185.0, 255.0, 360.0];
    let sizes = [1.0f32, 2.0, 5.0, 10.0, 20.0, 30.0, 50.0, 100.0];
    for (row, hardness) in paint_core::brush::HARDNESS_STEPS.iter().enumerate() {
        let y = 60.0 + row as f32 * 110.0 + dy;
        for (x, size) in xs.iter().zip(sizes) {
            let params = BrushParams { size, hardness: *hardness, size_by_pressure: false, ..Default::default() };
            let dab = std::iter::once(InputPoint { x: x + dx, y, pressure: 1.0 });
            doc.commit_stroke(0, &stroke_along(params, 0, dab), [0, 0, 0]);
        }
        let params = BrushParams { size: 20.0, hardness: *hardness, size_by_pressure: false, ..Default::default() };
        let line = [450.0f32, 670.0].into_iter().map(|x| InputPoint { x: x + dx, y, pressure: 1.0 });
        doc.commit_stroke(0, &stroke_along(params, 0, line), [0, 0, 0]);
    }
    io::save_png(Path::new(out), doc.width, doc.height, &doc.flatten())?;
    println!("saved {out}");
    Ok(())
}

/// `paint-cli hardlines <out.png> [dx dy]` — второй лист сверки: самая жёсткая ступень, размеры
/// 10–30, по одиночному отпечатку и прямой линии на размер. 540×540.
fn hard_lines_sheet(out: &str, dx: f32, dy: f32) -> Result<(), Box<dyn std::error::Error>> {
    let mut doc = Document::new(540, 540);
    if std::env::var_os("PAINT_SHEET_LARGE").is_some() {
        // Третий лист: размеры 50 и 100 (отпечатки и линии на своих местах).
        for (size, dab, line) in [(50.0f32, (40.0f32, 60.0f32), (100.0f32, 520.0f32, 60.0f32)), (100.0, (70.0, 230.0), (70.0, 480.0, 420.0))] {
            let params = BrushParams { size, hardness: 1.0, size_by_pressure: false, ..Default::default() };
            let p = std::iter::once(InputPoint { x: dab.0 + dx, y: dab.1 + dy, pressure: 1.0 });
            doc.commit_stroke(0, &stroke_along(params, 0, p), [0, 0, 0]);
            let l = [line.0, line.1].into_iter().map(|x| InputPoint { x: x + dx, y: line.2 + dy, pressure: 1.0 });
            doc.commit_stroke(0, &stroke_along(params, 0, l), [0, 0, 0]);
        }
        io::save_png(Path::new(out), doc.width, doc.height, &doc.flatten())?;
        println!("saved {out}");
        return Ok(());
    }
    for (row, size) in [10.0f32, 12.0, 14.0, 16.0, 20.0, 25.0, 30.0].into_iter().enumerate() {
        let y = 30.0 + row as f32 * 70.0 + dy;
        let params = BrushParams { size, hardness: 1.0, size_by_pressure: false, ..Default::default() };
        let dab = std::iter::once(InputPoint { x: 30.0 + dx, y, pressure: 1.0 });
        doc.commit_stroke(0, &stroke_along(params, 0, dab), [0, 0, 0]);
        let line = [80.0f32, 520.0].into_iter().map(|x| InputPoint { x: x + dx, y, pressure: 1.0 });
        doc.commit_stroke(0, &stroke_along(params, 0, line), [0, 0, 0]);
    }
    io::save_png(Path::new(out), doc.width, doc.height, &doc.flatten())?;
    println!("saved {out}");
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("alpha") {
        // `paint-cli alpha <файл.sai2> <номер слоя сверху> [шаг]` — альфа слоя цифрами 0..9 (и # для 100%).
        let loaded = paint_formats::sai2_doc::load(Path::new(args.get(2).ok_or("нужен файл")?))?;
        let top_down: Vec<&Layer> = loaded.document.layers.iter().rev().collect();
        let layer = top_down.get(args.get(3).and_then(|s| s.parse::<usize>().ok()).unwrap_or(0)).ok_or("нет такого слоя")?;
        let step = args.get(4).and_then(|s| s.parse::<i32>().ok()).unwrap_or(1);
        println!("слой «{}», непрозрачность {:.0}%", layer.name, layer.opacity * 100.0);
        let mut levels = std::collections::BTreeMap::new();
        for y in (0..loaded.document.height as i32).step_by(step as usize) {
            let mut row = String::new();
            for x in (0..loaded.document.width as i32).step_by(step as usize) {
                let a = layer.pixels.get(x, y)[3] as u32;
                *levels.entry(a * 100 / 32768).or_insert(0u32) += 1;
                row.push(match a * 100 / 32768 {
                    0 => '.',
                    100 => '#',
                    v => char::from_digit(v / 10, 10).unwrap_or('?'),
                });
            }
            println!("{row}");
        }
        println!("уровни альфы, %: {levels:?}");
        return Ok(());
    }
    if args.get(1).map(String::as_str) == Some("resize") {
        // `paint-cli resize <in.png> <out.png> <ширина> <высота> <режим 0..4>` — пересэмплирование
        // по правилам образца (0 ближайший, 1 билинейный, 2 бикубика, 3 Lanczos 2, 4 Lanczos 3).
        let num = |i: usize| args.get(i).and_then(|s| s.parse::<usize>().ok()).ok_or("нужны размеры и режим");
        let (sw, sh, rgba) = paint_core::io::load_png(Path::new(args.get(2).ok_or("нужен файл")?))?;
        let mode = paint_core::resample::Mode::from_index(num(6)? as u32).ok_or("режим 0..4")?;
        let src: Vec<[f32; 4]> = rgba
            .chunks(4)
            .map(|c| {
                let a = c[3] as f32 / 255.0;
                [c[0] as f32 / 255.0 * a, c[1] as f32 / 255.0 * a, c[2] as f32 / 255.0 * a, a]
            })
            .collect();
        let (dw, dh) = (num(4)?, num(5)?);
        let out = paint_core::resample::resize(&src, sw as usize, sh as usize, dw, dh, mode);
        let bytes: Vec<u8> = out
            .iter()
            .flat_map(|p| {
                let a = p[3];
                let f = |v: f32| if a > 0.0 { (v / a * 255.0 + 0.5).clamp(0.0, 255.0) as u8 } else { 0 };
                [f(p[0]), f(p[1]), f(p[2]), (a * 255.0 + 0.5) as u8]
            })
            .collect();
        io::save_png(Path::new(args.get(3).ok_or("нужен выходной файл")?), dw as u32, dh as u32, &bytes)?;
        return Ok(());
    }
    if args.get(1).map(String::as_str) == Some("resave") {
        // `paint-cli resave <вход.sai2> <выход.sai2>` — прочитать и записать заново, сверить.
        let (src, dst) = (Path::new(args.get(2).ok_or("нужен файл")?), Path::new(args.get(3).ok_or("нужен выходной файл")?));
        let t = std::time::Instant::now();
        let a = paint_formats::sai2_doc::load(src)?;
        let t_load = t.elapsed();
        paint_formats::sai2_write::save(&a.document, dst)?;
        let t_save = t.elapsed() - t_load;
        let b = paint_formats::sai2_doc::load(dst)?;
        let (fa, fb) = (a.document.flatten(), b.document.flatten());
        let worst = fa.iter().zip(&fb).map(|(x, y)| x.abs_diff(*y)).max().unwrap_or(0);
        let (sa, sb) = (std::fs::metadata(src)?.len(), std::fs::metadata(dst)?.len());
        println!(
            "слоёв {} → {}, сведение: наибольшая разница {worst}; размер {:.1} → {:.1} МБ; чтение {:.1} с, запись {:.1} с; замечания: {:?}",
            a.info.layers.len(),
            b.info.layers.len(),
            sa as f64 / 1e6,
            sb as f64 / 1e6,
            t_load.as_secs_f32(),
            t_save.as_secs_f32(),
            b.warnings
        );
        return Ok(());
    }
    if args.get(1).map(String::as_str) == Some("strokes") {
        // `paint-cli strokes <файл.csv> <out.png> <размер> [ступень 1..5] [x0 y0 sw sh]` — мазки из
        // CSV `t_ms,x,y,pressure,phase` (как у virtual-tablet) нашим движком на холсте 512×512.
        // Если даны x0 y0 sw sh, x и y — доли экрана sw×sh, а холст стоит на экране в (x0, y0).
        let num = |i: usize, d: f32| args.get(i).and_then(|s| s.parse::<f32>().ok()).unwrap_or(d);
        let (size, step) = (num(4, 30.0), num(5, 5.0) as usize);
        let (x0, y0, sw, sh) = (num(6, 0.0), num(7, 0.0), num(8, 1.0), num(9, 1.0));
        let params = BrushParams { size, hardness: paint_core::brush::HARDNESS_STEPS[step.clamp(1, 5) - 1], ..Default::default() };
        let mut doc = Document::new(512, 512);
        let mut stroke: Option<Stroke> = None;
        for line in std::fs::read_to_string(args.get(2).ok_or("нужен файл")?)?.lines() {
            let f: Vec<&str> = line.split(',').collect();
            if f.len() < 5 || line.starts_with('t') || line.starts_with('#') {
                continue;
            }
            let (x, y, p) = (f[1].parse::<f32>()? * sw - x0, f[2].parse::<f32>()? * sh - y0, f[3].parse::<f32>()?);
            match f[4] {
                "up" => {
                    if let Some(s) = stroke.take() {
                        doc.commit_stroke(0, &s, [0, 0, 0]);
                    }
                }
                _ => stroke.get_or_insert_with(|| Stroke::new(params)).add_point(InputPoint { x, y, pressure: p }),
            }
        }
        let out = args.get(3).ok_or("нужен выходной файл")?;
        io::save_png(Path::new(out), 512, 512, &doc.flatten())?;
        return Ok(());
    }
    if args.get(1).map(String::as_str) == Some("vectors") {
        // `paint-cli vectors <файл.sai2>` — кривые слоёв контура и фигур, как они записаны в файле.
        let loaded = paint_formats::sai2_doc::load(Path::new(args.get(2).ok_or("нужен файл")?))?;
        for w in &loaded.warnings {
            println!("! {w}");
        }
        for (id, shapes) in &loaded.vector_layers {
            let name = loaded.info.layers.iter().find(|l| l.id == *id).map_or("?", |l| l.name.as_str());
            println!("слой {id} «{name}»: объектов {}", shapes.len());
            for sh in shapes {
                println!("  объект {} (следующий {}): {:?}", sh.id, sh.next, sh.style);
                for p in &sh.paths {
                    println!(
                        "    путь {} сдвиг ({:.3}, {:.3}) вид {:?} замкнут {} прочее {:?}, узлов {}",
                        p.id, p.offset[0], p.offset[1], p.curve, p.closed, p.rest, p.points.len()
                    );
                    for c in &p.points {
                        println!(
                            "      узел {} ({:.3}, {:.3}) вход ({:.3}, {:.3}) выход ({:.3}, {:.3}) нажим {:.3}/{:.3} острый {} прочее {:?}",
                            c.id, c.point[0], c.point[1], c.handle_in[0], c.handle_in[1], c.handle_out[0], c.handle_out[1],
                            c.weight, c.second, c.pointed, c.rest
                        );
                        for pp in &c.pressure {
                            println!("        нажим {} на {:.4}: {:.3}/{:.3} прочее {:?}", pp.id, pp.pos, pp.weight, pp.second, pp.rest);
                        }
                    }
                }
            }
        }
        return Ok(());
    }
    if args.get(1).map(String::as_str) == Some("hardlines") {
        let num = |i: usize| args.get(i).and_then(|s| s.parse::<f32>().ok()).unwrap_or(0.0);
        return hard_lines_sheet(args.get(2).map(String::as_str).unwrap_or("hardlines.png"), num(3), num(4));
    }
    if args.get(1).map(String::as_str) == Some("refsheet") {
        let num = |i: usize| args.get(i).and_then(|s| s.parse::<f32>().ok()).unwrap_or(0.0);
        return reference_sheet(args.get(2).map(String::as_str).unwrap_or("refsheet.png"), num(3), num(4));
    }
    if args.get(1).map(String::as_str) == Some("brushes") {
        return brush_sheet(args.get(2).map(String::as_str).unwrap_or("brushes.png"));
    }
    if args.get(1).map(String::as_str) == Some("sai2") {
        let (Some(src), Some(dst)) = (args.get(2), args.get(3)) else {
            return Err("использование: paint-cli sai2 <файл.sai2> <out.png> [уменьшение]".into());
        };
        let shrink = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(1usize).max(1);
        return render_sai2(src, dst, shrink);
    }
    let out = args.get(1).cloned().unwrap_or_else(|| "demo.png".to_string());
    let mut doc = Document::new(800, 520);

    // 1. Жёсткое перо: толщина по давлению.
    let pen = BrushParams { size: 14.0, ..Default::default() };
    doc.commit_stroke(0, &stroke_along(pen, 0, wave(60.0, 22.0, 0.0)), [20, 20, 20]);

    // 2. Мягкая кисть: плотность и размер по давлению.
    let soft = BrushParams { size: 46.0, hardness: 0.0, density: 0.8, density_by_pressure: true, min_size: 0.3, ..Default::default() };
    doc.commit_stroke(0, &stroke_along(soft, 0, wave(150.0, 22.0, 0.0)), [30, 90, 200]);

    // 3. Дрожащая рука без стабилизатора и со стабилизатором.
    let thin = BrushParams { size: 5.0, size_by_pressure: false, ..Default::default() };
    doc.commit_stroke(0, &stroke_along(thin, 0, wave(240.0, 18.0, 4.0)), [160, 40, 40]);
    doc.commit_stroke(0, &stroke_along(thin, 12, wave(300.0, 18.0, 4.0)), [40, 130, 40]);

    // 4. Слои с режимами наложения: три круга.
    let blob = BrushParams { size: 150.0, size_by_pressure: false, ..Default::default() };
    let dot = |x: f32, y: f32| stroke_along(blob, 0, std::iter::once(InputPoint { x, y, pressure: 1.0 }));
    for (group_x, mode) in [(130.0, BlendMode::Normal), (400.0, BlendMode::Multiply), (670.0, BlendMode::Screen)] {
        if mode == BlendMode::Screen {
            let dark = doc.add_layer("Тёмная подложка");
            let pad = BrushParams { size: 250.0, size_by_pressure: false, ..Default::default() };
            let s = stroke_along(pad, 0, std::iter::once(InputPoint { x: group_x, y: 415.0, pressure: 1.0 }));
            doc.commit_stroke(dark, &s, [25, 25, 25]);
        }
        for (dx, dy, color) in [(-45.0, 25.0, [230, 40, 40]), (45.0, 25.0, [40, 200, 60]), (0.0, -45.0, [50, 80, 230])] {
            let l = doc.add_layer("Круг");
            doc.layers[l].mode = mode;
            doc.commit_stroke(l, &dot(group_x + dx, 415.0 + dy), color);
        }
    }

    // 5. Ластик поперёк первых двух штрихов и проверка отмены.
    let eraser = BrushParams { size: 18.0, erase: true, size_by_pressure: false, ..Default::default() };
    let cut = (0..=40).map(|i| InputPoint { x: 560.0 + i as f32, y: 20.0 + i as f32 * 4.5, pressure: 1.0 });
    doc.commit_stroke(0, &stroke_along(eraser, 0, cut), [0, 0, 0]);
    let scribble = stroke_along(pen, 0, wave(100.0, 40.0, 0.0));
    doc.commit_stroke(0, &scribble, [255, 0, 255]);
    doc.undo(); // розовой линии на картинке быть не должно

    io::save_png(Path::new(&out), doc.width, doc.height, &doc.flatten())?;
    println!("saved {out}: {}x{}, layers {}", doc.width, doc.height, doc.layers.len());
    Ok(())
}
