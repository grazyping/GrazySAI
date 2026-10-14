//! Проверка на образцах пользователя. Если папки с образцами нет, тесты пропускаются.

use paint_formats::sai2::read_info;
use std::path::PathBuf;

fn sample(name: &str) -> Option<PathBuf> {
    let dir = std::env::var("SAI2_SAMPLES").unwrap_or_else(|_| format!("{}/SAI2", std::env::var("HOME").unwrap_or_default()));
    let p = PathBuf::from(dir).join(name);
    p.exists().then_some(p)
}

#[test]
fn one_layer_sample() {
    let Some(p) = sample("пинг.sai2") else { return };
    let info = read_info(&p).unwrap();
    assert_eq!((info.width, info.height), (100, 100));
    assert!((info.dpi - 96.0).abs() < 0.01);
    assert_eq!(info.layers.len(), 1);
    let l = &info.layers[0];
    assert_eq!((l.name.as_str(), l.blend.as_str(), l.opacity, l.visible, l.is_folder), ("Слой", "norm", 100, true, false));
    assert_eq!(l.bounds, [0, 0, 128, 128]);
    assert_eq!(l.block_rect, [0, 0, 4, 4]);
    assert!(l.has_pixels);
}

#[test]
fn two_layer_sample() {
    let Some(p) = sample("пинг1.sai2") else { return };
    let info = read_info(&p).unwrap();
    assert_eq!(info.layers.iter().map(|l| l.id).collect::<Vec<_>>(), vec![3, 2]);
}

#[test]
fn real_drawing() {
    let Some(p) = sample("YCH JimTheDoggo1.sai2") else { return };
    let info = read_info(&p).unwrap();
    assert_eq!((info.width, info.height), (2300, 4096));
    assert!((info.dpi - 72.0).abs() < 0.01);
    assert_eq!(info.layers.len(), 191);
    assert_eq!(info.layers.iter().filter(|l| l.is_folder).count(), 20);
    assert_eq!(info.layers.iter().filter(|l| l.has_pixels).count(), 171);
    assert!(info.layers.iter().all(|l| l.opacity <= 100 && !l.name.is_empty()));
    assert!(info.layers.iter().any(|l| l.blend == "scrn"));
}

#[test]
fn rejects_foreign_files() {
    let p = std::env::temp_dir().join("paint-rs-not-sai2.bin");
    std::fs::write(&p, vec![0u8; 200]).unwrap();
    assert!(read_info(&p).is_err());
    let _ = std::fs::remove_file(&p);
}

use paint_core::pixel::to_rgba8;
use paint_formats::sai2_pixels::read_layer;

#[test]
fn pixels_of_probe_files() {
    let Some(p) = sample("пинг1.sai2") else { return };
    let info = read_info(&p).unwrap();
    let mut f = std::fs::File::open(&p).unwrap();
    // Верхний слой: ровная красноватая заливка холста 100×100.
    let top = read_layer(&mut f, &info.entries, &info.layers[0]).unwrap();
    for (x, y) in [(0, 0), (50, 50), (99, 99), (97, 3)] {
        assert_eq!(to_rgba8(top.get(x, y)), [212, 66, 66, 255], "точка {x},{y}");
    }
    // За краем холста пусто.
    // Заливка в образце выходит за холст на одну полупрозрачную точку, дальше пусто.
    assert!(top.get(100, 50)[3] < 32768);
    assert_eq!(top.get(101, 50)[3], 0);
    assert_eq!(top.get(50, 101)[3], 0);
    let bottom = read_layer(&mut f, &info.entries, &info.layers[1]).unwrap();
    assert!(!bottom.tiles.is_empty());
}

#[test]
fn every_layer_of_real_drawing_decodes() {
    let Some(p) = sample("YCH JimTheDoggo1.sai2") else { return };
    let info = read_info(&p).unwrap();
    let mut f = std::fs::File::open(&p).unwrap();
    let mut painted = 0;
    for layer in info.layers.iter().filter(|l| l.has_pixels) {
        let map = read_layer(&mut f, &info.entries, layer).unwrap_or_else(|e| panic!("слой {} «{}»: {e}", layer.id, layer.name));
        painted += usize::from(!map.tiles.is_empty());
        // Предумноженный цвет не может превышать альфу.
        for tile in map.tiles.values().take(3) {
            assert!(tile.0.iter().all(|p| p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3] && p[3] <= 32768));
        }
    }
    assert!(painted > 100, "слоёв с краской: {painted}");
}

use paint_formats::sai2_doc;

/// Моё сведение слоёв должно совпадать со сведённой картинкой, которую сохранил сам образец.
fn match_with_reference(name: &str) -> Option<(f64, f64)> {
    let p = sample(name)?;
    let loaded = sai2_doc::load(&p).unwrap();
    let mine = loaded.document.flatten();
    let reference = sai2_doc::read_flattened(&p).unwrap();
    assert_eq!(mine.len(), reference.len());
    let (mut exact, mut sum) = (0usize, 0u64);
    for (a, b) in mine.as_chunks::<4>().0.iter().zip(reference.as_chunks::<4>().0.iter()) {
        let d = (0..3).map(|c| a[c].abs_diff(b[c])).max().unwrap();
        exact += usize::from(d == 0);
        sum += d as u64;
    }
    let n = (mine.len() / 4) as f64;
    Some((exact as f64 / n, sum as f64 / n))
}

#[test]
fn probes_flatten_exactly_like_the_reference() {
    for name in ["пинг.sai2", "пинг1.sai2"] {
        let Some((exact, _)) = match_with_reference(name) else { return };
        assert_eq!(exact, 1.0, "{name}");
    }
}

#[test]
fn real_drawing_flattens_close_to_the_reference() {
    let Some((exact, mean)) = match_with_reference("YCH JimTheDoggo1.sai2") else { return };
    // Не учтены бумага слоёв и точное округление образца; порог — чтобы не стало хуже.
    assert!(exact > 0.55 && mean < 1.5, "точно {exact:.3}, средняя разница {mean:.3}");
}

#[test]
fn folders_become_a_tree() {
    let Some(p) = sample("YCH JimTheDoggo1.sai2") else { return };
    let loaded = sai2_doc::load(&p).unwrap();
    fn count(layers: &[paint_core::Layer]) -> (usize, usize, usize) {
        layers.iter().fold((0, 0, 0), |acc, l| {
            let inner = l.children.as_deref().map(count).unwrap_or((0, 0, 0));
            (acc.0 + 1 + inner.0, acc.1 + usize::from(l.children.is_some()) + inner.1, acc.2.max(1 + inner.2))
        })
    }
    let (all, folders, depth) = count(&loaded.document.layers);
    assert_eq!((all, folders), (191, 20));
    assert!(depth >= 3, "глубина {depth}");
    assert!(loaded.document.layers.len() < 191);
}

/// Слои фигур из файла, сохранённого образцом: формат, цвет и заливка против его же сведённой
/// картинки (сравнение поверх белого, только там, где есть краска).
#[test]
fn shape_layers_match_the_reference() {
    use paint_formats::sai2_vector::{CurveType, Style};
    let Some(p) = sample("пинг4.sai2") else { return };
    let loaded = paint_formats::sai2_doc::load(&p).unwrap();
    assert_eq!(loaded.vector_layers.len(), 7);
    let (_, ellipse) = loaded.vector_layers.iter().find(|(id, _)| *id == 10).unwrap();
    let path = &ellipse[0].paths[0];
    assert_eq!((path.curve, path.closed, path.points.len()), (CurveType::Bezier, true, 4));
    assert_eq!(ellipse[0].style, Style::Fill { color: [231, 39, 217, 25, 177, 49, 0, 64] });
    let mine = loaded.document.flatten();
    let reference = paint_formats::sai2_doc::read_flattened(&p).unwrap();
    let over_white = |b: &[u8], i: usize| -> [f32; 3] {
        let a = b[i + 3] as f32 / 255.0;
        std::array::from_fn(|c| b[i + c] as f32 * a + 255.0 * (1.0 - a))
    };
    let w = loaded.document.width as usize;
    // прямоугольники и треугольники — правая и нижняя часть холста, эллипсы — слева
    for (x0, y0, x1, y1, worst_ok, mean_ok) in [(170, 180, 500, 500, 3.5, 0.05), (0, 170, 130, 370, 14.0, 0.3)] {
        let (mut n, mut sum, mut worst) = (0usize, 0f32, 0f32);
        for y in y0..y1 {
            for x in x0..x1 {
                let i = (y * w + x) * 4;
                let (m, r) = (over_white(&mine, i), over_white(&reference, i));
                if r == [255.0; 3] && m == [255.0; 3] {
                    continue;
                }
                let d = (0..3).map(|c| (m[c] - r[c]).abs()).fold(0f32, f32::max);
                n += 1;
                sum += d;
                worst = worst.max(d);
            }
        }
        assert!(n > 10_000);
        assert!(worst <= worst_ok && sum / n as f32 <= mean_ok, "область {x0},{y0}: наибольшая {worst}, средняя {}", sum / n as f32);
    }
}

/// Слой контура из файла, сохранённого образцом (`пинг5.sai2`, 25 штрихов с планшета):
/// кривые, нажим по длине дуги и вывод нашим движком кисти против сведённой картинки образца.
#[test]
fn linework_layer_matches_the_reference() {
    use paint_formats::sai2_vector::Style;
    let Some(p) = sample("пинг5.sai2") else { return };
    let loaded = paint_formats::sai2_doc::load(&p).unwrap();
    let (_, shapes) = &loaded.vector_layers[0];
    assert_eq!(shapes.len(), 25);
    let Style::Linework { size, hardness, .. } = &shapes[24].style else { panic!("не контур") };
    assert_eq!((*size, *hardness), (50.0, 4));
    let mine = loaded.document.flatten();
    let reference = paint_formats::sai2_doc::read_flattened(&p).unwrap();
    let w = loaded.document.width as usize;
    let g = |b: &[u8], x: usize, y: usize| {
        let i = (y * w + x) * 4;
        let a = b[i + 3] as f32 / 255.0;
        b[i + 1] as f32 * a + 255.0 * (1.0 - a)
    };
    // одиночная точка 50 px: один отпечаток непрозрачностью около 1/3
    for x in [30, 47, 64] {
        assert!((g(&mine, x, 440) - g(&reference, x, 440)).abs() <= 2.0);
    }
    let mut sum = 0f64;
    for i in (0..mine.len()).step_by(4) {
        let ow = |b: &[u8], c: usize| b[i + c] as f64 * b[i + 3] as f64 / 255.0 + 255.0 * (1.0 - b[i + 3] as f64 / 255.0);
        sum += (0..3).map(|c| (ow(&mine, c) - ow(&reference, c)).abs()).fold(0f64, f64::max);
    }
    let mean = sum / (mine.len() / 4) as f64;
    assert!(mean < 2.0, "средняя разница {mean}");
}
