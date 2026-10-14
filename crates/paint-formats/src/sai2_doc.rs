//! Превращает файл `.sai2` в документ редактора: дерево слоёв с папками, режимами и обтравкой.

use crate::sai2::{self, LayerInfo, Sai2Info};
use crate::{sai2_pixels, sai2_vector};
use paint_core::{BlendMode, Document, Layer};
use std::collections::BTreeSet;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

/// Режим наложения по метке из файла. `None` — режим пока не реализован.
pub fn blend_from_tag(tag: &str) -> Option<BlendMode> {
    Some(match tag {
        "norm" | "pass" => BlendMode::Normal,
        "mult" => BlendMode::Multiply,
        "scrn" => BlendMode::Screen,
        "over" => BlendMode::Overlay,
        "dark" => BlendMode::Darken,
        "litn" => BlendMode::Lighten,
        "cbrn" => BlendMode::ColorBurn,
        "cddg" => BlendMode::ColorDodge,
        "lbrn" => BlendMode::LinearBurn,
        "lddg" => BlendMode::LinearDodge,
        "slit" => BlendMode::SoftLight,
        "hlit" => BlendMode::HardLight,
        "diff" => BlendMode::Difference,
        "excl" => BlendMode::Exclusion,
        "fsub" => BlendMode::Subtract,
        "fdiv" => BlendMode::Divide,
        "add" => BlendMode::Shine,
        "sub" => BlendMode::Shade,
        "drkc" => BlendMode::DarkerColor,
        "litc" => BlendMode::LighterColor,
        "sbad" => BlendMode::ShadeShine,
        "burn" => BlendMode::Burn,
        "ddge" => BlendMode::Dodge,
        "bndg" => BlendMode::BurnDodge,
        "vlit" => BlendMode::VividLight,
        "llit" => BlendMode::LinearLight,
        "plit" => BlendMode::PinLight,
        "hmix" => BlendMode::HardMix,
        _ => return None,
    })
}

/// Приближение для режимов, которых в ядре ещё нет: чем заменить, чтобы картинка была похожа.
fn blend_fallback(tag: &str) -> BlendMode {
    let _ = tag;
    BlendMode::Normal
}

pub struct Loaded {
    pub document: Document,
    pub info: Sai2Info,
    /// Что прочитано приближённо или пропущено.
    pub warnings: Vec<String>,
    /// Кривые слоёв контура и фигур: номер слоя и его объекты. Пикселей у таких слоёв в файле
    /// нет, вывод кривых в пиксели ещё не сделан.
    pub vector_layers: Vec<(u32, Vec<sai2_vector::Shape>)>,
}

fn build(infos: &[LayerInfo], pos: &mut usize, depth: u8, file: &mut File, info: &Sai2Info, notes: &mut BTreeSet<String>) -> io::Result<Vec<Layer>> {
    // В файле слои идут сверху вниз; в документе — снизу вверх.
    let mut top_down = Vec::new();
    while *pos < infos.len() && infos[*pos].depth >= depth {
        let li = &infos[*pos];
        *pos += 1;
        let mut layer = Layer::new(&li.name);
        layer.opacity = li.opacity.min(100) as f32 / 100.0;
        layer.visible = li.visible;
        layer.selection_source = li.selection_source;
        layer.clipping = li.clipping;
        layer.preserve_opacity = li.locks[0];
        layer.lock_paint = li.locks[1];
        layer.lock_move = li.locks[2];
        layer.lock_all = li.locks[3];
        layer.mode = blend_from_tag(&li.blend).unwrap_or_else(|| {
            notes.insert(format!("режим «{}» заменён приближённым", li.blend));
            blend_fallback(&li.blend)
        });
        if li.is_folder {
            layer.pass_through = li.blend == "pass";
            layer.children = Some(build(infos, pos, depth + 1, file, info, notes)?);
        } else {
            layer.pixels = sai2_pixels::read_layer(file, &info.entries, li)?;
            match li.kind.as_str() {
                "norm" => {}
                "shap" => match read_vector(file, info, li) {
                    Ok(shapes) => {
                        render_shapes(&mut layer.pixels, &shapes);
                        layer.vector = read_raw(file, info, li);
                    }
                    Err(e) => {
                        notes.insert(format!("фигуры слоя «{}» не прочитаны: {e}", li.name));
                    }
                },
                "liwk" => match read_vector(file, info, li) {
                    Ok(shapes) => {
                        render_linework(&mut layer, &shapes);
                        layer.vector = read_raw(file, info, li);
                    }
                    Err(e) => {
                        notes.insert(format!("контур слоя «{}» не прочитан: {e}", li.name));
                    }
                },
                k => {
                    notes.insert(format!("слой вида «{k}» прочитан как пустой"));
                }
            }
        }
        top_down.push(layer);
    }
    top_down.reverse();
    Ok(top_down)
}

/// Сырые байты векторного блока слоя (для записи обратно без изменений).
fn read_raw(file: &mut File, info: &Sai2Info, li: &LayerInfo) -> Option<paint_core::layer::VectorData> {
    let e = info.entries.iter().find(|e| e.tag == li.kind.as_bytes() && e.id == li.id)?;
    if e.len > 1 << 30 {
        return None;
    }
    let mut raw = vec![0u8; e.len as usize];
    file.seek(SeekFrom::Start(e.offset)).ok()?;
    file.read_exact(&mut raw).ok()?;
    Some(paint_core::layer::VectorData { kind: e.tag, raw })
}

/// Векторный блок слоя контура или фигур (метка блока = вид слоя, номер = слой).
fn read_vector(file: &mut File, info: &Sai2Info, li: &LayerInfo) -> io::Result<Vec<sai2_vector::Shape>> {
    let e = info
        .entries
        .iter()
        .find(|e| e.tag == li.kind.as_bytes() && e.id == li.id)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "нет векторного блока"))?;
    if e.len > 1 << 30 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "векторный блок слишком велик"));
    }
    let mut data = vec![0u8; e.len as usize];
    file.seek(SeekFrom::Start(e.offset))?;
    file.read_exact(&mut data)?;
    sai2_vector::parse(&data, li.kind == "liwk")
}

/// Цвет из файла (`fcol`, `scol`): u16 синий, зелёный, красный, альфа; 0x4000 = 1.0, предумножен.
pub fn color_from_file(c: &[u8; 8]) -> paint_core::pixel::Px {
    let v = |i: usize| (u16::from_le_bytes([c[i * 2], c[i * 2 + 1]]) as u32 * 2).min(paint_core::pixel::ONE) as u16;
    [v(2), v(1), v(0), v(3)]
}

/// Заливка фигур слоя. Узлы и ручки в файле — относительно сдвига пути.
fn render_shapes(pixels: &mut paint_core::tile::ColorMap, shapes: &[sai2_vector::Shape]) {
    use paint_core::vector::{fill_shape, Node};
    for sh in shapes {
        let sai2_vector::Style::Fill { color } = &sh.style else { continue };
        let paths: Vec<(Vec<Node>, bool)> = sh
            .paths
            .iter()
            .map(|p| {
                let at = |q: [f64; 2]| [q[0] + p.offset[0], q[1] + p.offset[1]];
                let nodes = p.points.iter().map(|c| Node { point: at(c.point), handle_in: at(c.handle_in), handle_out: at(c.handle_out) }).collect();
                (nodes, p.closed)
            })
            .collect();
        fill_shape(pixels, &paths, color_from_file(color));
    }
}

/// Штрихи слоя контура нашим движком кисти: точки вдоль кривой через 0,25 px с нажимом по длине
/// дуги (docs/11-linework-shapes.md, разделы 4–5).
fn render_linework(layer: &mut Layer, shapes: &[sai2_vector::Shape]) {
    use paint_core::brush::HARDNESS_STEPS;
    use paint_core::vector::{sample_path, Node};
    use paint_core::{BrushParams, InputPoint, Stroke};
    for sh in shapes {
        let sai2_vector::Style::Linework { color, size, density, hardness, params, .. } = &sh.style else { continue };
        let px = color_from_file(color);
        let rgba = paint_core::pixel::to_rgba8(px);
        for p in &sh.paths {
            let at = |q: [f64; 2]| [q[0] + p.offset[0], q[1] + p.offset[1]];
            let nodes: Vec<Node> = p.points.iter().map(|c| Node { point: at(c.point), handle_in: at(c.handle_in), handle_out: at(c.handle_out) }).collect();
            let weights: Vec<f64> = p.points.iter().map(|c| c.weight as f64).collect();
            let pps: Vec<Vec<(f64, f64)>> = p.points.iter().map(|c| c.pressure.iter().map(|q| (q.pos, q.weight as f64)).collect()).collect();
            let params = BrushParams {
                size: *size,
                density: *density,
                hardness: HARDNESS_STEPS[(*hardness as usize).min(4)],
                size_by_pressure: true,
                linework: true,
                // inkd: пятое число — то, что движок берёт как 1 − x (Sharpness)
                sharpness: params[2],
                ..Default::default()
            };
            let mut stroke = Stroke::new(params);
            for (q, pr) in sample_path(&nodes, p.closed, &weights, &pps, 0.25) {
                stroke.add_point(InputPoint { x: q[0] as f32, y: q[1] as f32, pressure: pr as f32 });
            }
            layer.apply_stroke(&stroke, [rgba[0], rgba[1], rgba[2]], &Default::default());
        }
    }
}

pub fn load(path: &Path) -> io::Result<Loaded> {
    let info = sai2::read_info(path)?;
    let mut file = File::open(path)?;
    let mut notes = BTreeSet::new();
    let mut pos = 0;
    let layers = build(&info.layers, &mut pos, 0, &mut file, &info, &mut notes)?;
    let mut vector_layers = Vec::new();
    for li in info.layers.iter().filter(|l| l.kind == "liwk" || l.kind == "shap") {
        let Some(e) = info.entries.iter().find(|e| e.tag == li.kind.as_bytes() && e.id == li.id) else { continue };
        if e.len > 1 << 30 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "векторный блок слишком велик"));
        }
        let mut data = vec![0u8; e.len as usize];
        file.seek(SeekFrom::Start(e.offset))?;
        file.read_exact(&mut data)?;
        match sai2_vector::parse(&data, li.kind == "liwk") {
            Ok(shapes) => vector_layers.push((li.id, shapes)),
            Err(err) => {
                notes.insert(format!("кривые слоя «{}» не прочитаны: {err}", li.name));
            }
        }
    }
    for tag in ["mask", "mpix"] {
        if info.entries.iter().any(|e| e.tag == tag.as_bytes()) {
            notes.insert("маски слоёв не прочитаны".to_string());
        }
    }
    let mut document = Document::new(info.width, info.height);
    document.layers = layers;
    Ok(Loaded { document, info, warnings: notes.into_iter().collect(), vector_layers })
}

/// Сведённая картинка, которую сохранила сама программа-образец (RGBA, 8 бит).
pub fn read_flattened(path: &Path) -> io::Result<Vec<u8>> {
    let info = sai2::read_info(path)?;
    let e = info.entries.iter().find(|e| &e.tag == b"intg").ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "в файле нет сведённого изображения"))?;
    let mut data = vec![0u8; e.len as usize];
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(e.offset))?;
    file.read_exact(&mut data)?;
    sai2_pixels::decode_intg(&data, info.width, info.height)
}
