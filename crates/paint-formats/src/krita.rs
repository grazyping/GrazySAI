//! Пресеты кистей Krita: `.kpp` (PNG, в котором XML с параметрами) и наборы `.bundle` (zip с
//! пресетами и кончиками). Переносится то, что ложится на движок GrazySAI: размер, непрозрачность,
//! поток, жёсткость круглой кисти, шаг, нажим на размер и непрозрачность, ластик, кончик-картинка.

use crate::brush_tip::ImportedBrush;
use paint_core::Stamp;
use std::path::{Path, PathBuf};

type Res<T> = Result<T, Box<dyn std::error::Error>>;

/// Один `.kpp`. Кончик-картинку пресет хранит только по имени: он ищется рядом с файлом, в
/// соседней папке `brushes` и в папках ресурсов Krita.
pub fn load_kpp(path: &Path) -> Res<ImportedBrush> {
    let d = std::fs::read(path)?;
    let xml = preset_xml(&d).ok_or("в файле нет пресета Krita")?;
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(p) = path.parent() {
        dirs.push(p.to_path_buf());
        dirs.push(p.join("brushes"));
        if let Some(pp) = p.parent() {
            dirs.push(pp.join("brushes"));
        }
    }
    dirs.extend(resource_dirs());
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    parse(&xml, &stem, |name| dirs.iter().find_map(|d| std::fs::read(d.join(name)).ok()).or_else(|| in_bundles(name)))
}

/// Кончик из наборов `.bundle`, установленных в Krita (стандартные кончики лежат в них).
fn in_bundles(tip: &str) -> Option<Vec<u8>> {
    let mut dirs = Vec::new();
    if let Some(a) = std::env::var_os("APPDATA") {
        dirs.push(PathBuf::from(a).join("krita").join("bundles"));
    }
    if let Some(h) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(h).join(".local/share/krita/bundles"));
    }
    dirs.push(PathBuf::from("/usr/share/krita/bundles"));
    if let Some(p) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
        // Krita для Windows: share\krita\bundles рядом с программой
        dirs.push(p.join("share").join("krita").join("bundles"));
    }
    for d in dirs {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if !p.extension().is_some_and(|x| x == "bundle") {
                continue;
            }
            let Ok(files) = std::fs::read(&p).map_err(|_| ()).and_then(|d| crate::ora::unzip(&d).map_err(|_| ())) else { continue };
            if let Some((_, data)) = files.into_iter().find(|(n, _)| n.starts_with("brushes/") && n.ends_with(&format!("/{tip}"))) {
                return Some(data);
            }
        }
    }
    None
}

/// Все пресеты набора `.bundle` с кончиками из него же (или из папок Krita).
pub fn load_bundle(path: &Path) -> Res<Vec<ImportedBrush>> {
    let files = crate::ora::unzip(&std::fs::read(path)?)?;
    let dirs = resource_dirs();
    let mut out = Vec::new();
    for (name, data) in &files {
        if !(name.starts_with("paintoppresets/") && name.ends_with(".kpp")) {
            continue;
        }
        let Some(xml) = preset_xml(data) else { continue };
        let stem = name.rsplit('/').next().unwrap_or(name).trim_end_matches(".kpp").to_string();
        let find = |tip: &str| {
            files.iter().find(|(n, _)| n.starts_with("brushes/") && n.ends_with(&format!("/{tip}"))).map(|(_, d)| d.clone()).or_else(|| dirs.iter().find_map(|d| std::fs::read(d.join(tip)).ok()))
        };
        if let Ok(b) = parse(&xml, &stem, find) {
            out.push(b);
        }
    }
    if out.is_empty() {
        return Err("в наборе нет пресетов кистей".into());
    }
    Ok(out)
}

/// Папки кончиков Krita на этой машине.
fn resource_dirs() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(a) = std::env::var_os("APPDATA") {
        out.push(PathBuf::from(a).join("krita").join("brushes"));
    }
    if let Some(h) = std::env::var_os("HOME") {
        out.push(PathBuf::from(h).join(".local/share/krita/brushes"));
    }
    out.push(PathBuf::from("/usr/share/krita/brushes"));
    out
}

/// XML пресета из PNG: текстовый блок `preset` (`zTXt` сжатый или `tEXt`).
fn preset_xml(d: &[u8]) -> Option<String> {
    if d.get(..8)? != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    let mut p = 8;
    while p + 12 <= d.len() {
        let n = u32::from_be_bytes(d[p..p + 4].try_into().ok()?) as usize;
        let kind = &d[p + 4..p + 8];
        let body = d.get(p + 8..p + 8 + n)?;
        if kind == b"zTXt" || kind == b"tEXt" {
            let k = body.iter().position(|&b| b == 0)?;
            if &body[..k] == b"preset" {
                let v = &body[k + 1..];
                let text = if kind == b"zTXt" { miniz_oxide::inflate::decompress_to_vec_zlib(v.get(1..)?).ok()? } else { v.to_vec() };
                return Some(String::from_utf8_lossy(&text).into_owned());
            }
        }
        p += 12 + n;
    }
    None
}

/// Значение параметра `<param name="…"><![CDATA[…]]></param>`.
fn param<'a>(xml: &'a str, name: &str) -> Option<&'a str> {
    let at = xml.find(&format!("name=\"{name}\""))?;
    let rest = &xml[at..];
    let start = rest.find("<![CDATA[")? + 9;
    let end = rest[start..].find("]]>")? + start;
    // CDATA должна относиться к этому же параметру
    (!rest[..start].contains("</param>")).then(|| &rest[start..end])
}

/// Атрибут `name="…"` в теге, который начинается с `tag`.
fn attr(xml: &str, tag: &str, name: &str) -> Option<String> {
    let at = xml.find(tag)?;
    let end = xml[at..].find('>')? + at;
    let t = &xml[at..end];
    let key = format!(" {name}=\"");
    let s = t.find(&key)? + key.len();
    let e = t[s..].find('"')? + s;
    Some(t[s..e].to_string())
}

/// Доля при нулевом нажиме для параметра `option` (`Size`, `Opacity`, `Flow`): `None`, если
/// параметр не включён или зависит не от нажима. Значение — y кривой при x = 0.
fn pressure(xml: &str, option: &str) -> Option<f32> {
    if param(xml, &format!("Pressure{option}")) == Some("false") {
        return None;
    }
    let sensor = param(xml, &format!("{option}Sensor"))?;
    let at = sensor.find("id=\"pressure\"")? + 13;
    // кривая этого датчика — до следующего датчика; если её нет — прямая 0..1
    let rest = &sensor[at..];
    let own = &rest[..rest.find("id=\"").unwrap_or(rest.len())];
    let y0 = own
        .find("<curve>")
        .and_then(|c| own[c + 7..].split(';').next())
        .and_then(|first| first.split(',').nth(1))
        .and_then(|v| v.trim().parse::<f32>().ok())
        .unwrap_or(0.0);
    Some(y0.clamp(0.0, 1.0))
}

/// Разбор XML пресета; `find` даёт байты файла кончика по имени.
fn parse(xml: &str, fallback_name: &str, find: impl Fn(&str) -> Option<Vec<u8>>) -> Res<ImportedBrush> {
    let paintop = attr(xml, "<Preset", "paintopid").unwrap_or_default();
    let name = attr(xml, "<Preset", "name").filter(|n| !n.is_empty()).unwrap_or_else(|| fallback_name.to_string());
    let mut skipped: Vec<&'static str> = Vec::new();
    if paintop != "paintbrush" && paintop != "colorsmudge" {
        skipped.push("движок кисти (взяты только общие настройки)");
    }
    let num = |k: &str, d: f32| param(xml, k).and_then(|v| v.trim().parse::<f32>().ok()).unwrap_or(d);
    let def = param(xml, "brush_definition").unwrap_or("");
    let kind = attr(def, "<Brush", "type").unwrap_or_default();
    let spacing = attr(def, "<Brush", "spacing").and_then(|v| v.parse::<f32>().ok());
    let auto_spacing = attr(def, "<Brush", "useAutoSpacing").as_deref() == Some("1");
    let angle = attr(def, "<Brush", "angle").and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0).to_degrees();
    let (mut size, mut hardness, mut tip, mut ratio) = (20.0f32, 1.0f32, None, 1.0f32);
    let (mut more, mut tip_random) = (Vec::new(), false);
    match kind.as_str() {
        "auto_brush" => {
            size = attr(def, "<MaskGenerator", "diameter").and_then(|v| v.parse().ok()).unwrap_or(20.0);
            // «Fade» круглой кисти: 1 — край резкий, меньше — мягче
            let fade: f32 = attr(def, "<MaskGenerator", "hfade").and_then(|v| v.parse().ok()).unwrap_or(1.0);
            hardness = fade.clamp(0.0, 1.0);
            ratio = attr(def, "<MaskGenerator", "ratio").and_then(|v| v.parse::<f32>().ok()).unwrap_or(1.0).clamp(0.01, 1.0);
        }
        "png_brush" | "gbr_brush" | "abr_brush" | "svg_brush" => {
            let file = attr(def, "<Brush", "filename").unwrap_or_default();
            let scale: f32 = attr(def, "<Brush", "scale").and_then(|v| v.parse().ok()).unwrap_or(1.0);
            let cells = find(&file).and_then(|d| tip_from(&file, &d));
            match cells {
                Some((mut c, random)) => {
                    let s = c.remove(0);
                    size = s.width.max(s.height) as f32 * scale;
                    tip = Some(crate::brush_tip::shrink(s, crate::brush_tip::MAX_SIDE));
                    more = c.into_iter().map(|s| crate::brush_tip::shrink(s, crate::brush_tip::MAX_SIDE)).collect();
                    tip_random = random;
                }
                None => skipped.push("кончик (файла нет рядом с пресетом и в папках Krita)"),
            }
        }
        "" => {}
        _ => skipped.push("вид кончика (векторный или текстовый)"),
    }
    if param(xml, "Texture/Pattern/Enabled") == Some("true") {
        skipped.push("текстура");
    }
    if param(xml, "MaskingBrush/Enabled") == Some("true") {
        skipped.push("двойная кисть");
    }
    let size = (size * num("SizeValue", 1.0).max(0.01)).clamp(1.0, 5000.0);
    let smudge = paintop == "colorsmudge";
    Ok(ImportedBrush {
        name,
        size,
        density: (num("OpacityValue", 1.0) * num("FlowValue", 1.0)).clamp(0.0, 1.0),
        hardness,
        spacing: if auto_spacing { None } else { spacing.map(|s| s.clamp(0.01, 10.0)) },
        size_pressure: pressure(xml, "Size"),
        density_pressure: pressure(xml, "Opacity").or_else(|| pressure(xml, "Flow")),
        mix: smudge.then_some((0.5, 0.0)),
        texture: None,
        tip,
        tip_angle: angle,
        tip_more: more,
        tip_random,
        tip_ratio: ratio,
        // поворот по направлению мазка — датчик «drawingangle» у параметра поворота
        tip_follow: param(xml, "PressureRotation") != Some("false") && param(xml, "RotationSensor").is_some_and(|v| v.contains("id=\"drawingangle\"")),
        texture_density: 1.0,
        texture_scale: 100,
        erase: param(xml, "EraserMode") == Some("true") || param(xml, "CompositeOp") == Some("erase"),
        smudge,
        scatter: None,
        skipped,
    })
}

/// Кончик из файла Krita по расширению: PNG (серый — тёмное краска, с прозрачностью —
/// непрозрачное), GIMP `.gbr` / `.gih` (все клетки набора). Возвращает картинки и смену вразнобой.
fn tip_from(name: &str, d: &[u8]) -> Option<(Vec<Stamp>, bool)> {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".gbr") {
        crate::brush_tip::gbr(d).ok().map(|(_, s)| (vec![s], false))
    } else if lower.ends_with(".gih") {
        crate::brush_tip::gih_all(d).ok().map(|(_, c, r)| (c, r))
    } else if lower.ends_with(".svg") {
        crate::brush_tip::svg(d).ok().map(|s| (vec![s], false))
    } else if lower.ends_with(".png") {
        paint_core::io::decode_png(d).ok().map(|(w, h, rgba)| (vec![Stamp::from_rgba(w as usize, h as usize, &rgba)], false))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Пресет, собранный по образцу стандартных пресетов Krita.
    #[test]
    fn reads_preset_xml() {
        let xml = r#"<Preset paintopid="paintbrush" name="Мел"> <param type="string" name="EraserMode"><![CDATA[false]]></param> <param type="string" name="FlowValue"><![CDATA[0.5]]></param> <param type="string" name="OpacityValue"><![CDATA[0.8]]></param> <param type="string" name="OpacitySensor"><![CDATA[<!DOCTYPE params> <params id="pressure"> <curve>0,0.25;1,1;</curve> </params> ]]></param> <param type="string" name="PressureSize"><![CDATA[true]]></param> <param type="string" name="SizeSensor"><![CDATA[<!DOCTYPE params> <params id="fuzzy"/> ]]></param> <param type="string" name="brush_definition"><![CDATA[<Brush type="png_brush" spacing="0.25" useAutoSpacing="0" angle="1.5708" scale="0.5" filename="dot.png"/>]]></param> </Preset>"#;
        let mut png = Vec::new();
        {
            let mut e = png::Encoder::new(&mut png, 4, 2);
            e.set_color(png::ColorType::Rgba);
            e.write_header().unwrap().write_image_data(&[0u8, 0, 0, 255].repeat(8)).unwrap();
        }
        let b = parse(xml, "x", |n| (n == "dot.png").then(|| png.clone())).unwrap();
        assert_eq!(b.name, "Мел");
        assert_eq!(b.size, 2.0, "4 px × 0,5");
        assert!((b.density - 0.4).abs() < 1e-6);
        assert_eq!(b.spacing, Some(0.25));
        assert_eq!(b.size_pressure, None, "размер — от случайности, не от нажима");
        assert_eq!(b.density_pressure, Some(0.25));
        assert!((b.tip_angle - 90.0).abs() < 0.01);
        assert_eq!(b.tip.as_ref().map(|t| (t.width, t.height)), Some((4, 2)));
        assert!(!b.erase && b.skipped.is_empty());
    }

    #[test]
    fn round_brush_fade_is_hardness() {
        let xml = r#"<Preset paintopid="paintbrush" name="Круг"> <param type="string" name="brush_definition"><![CDATA[<Brush type="auto_brush" spacing="0.1" useAutoSpacing="1"> <MaskGenerator ratio="1" type="circle" hfade="0.6" vfade="0.6" diameter="50"/> </Brush>]]></param> <param type="string" name="EraserMode"><![CDATA[true]]></param> </Preset>"#;
        let b = parse(xml, "x", |_| None).unwrap();
        assert_eq!((b.size, b.hardness, b.spacing, b.erase), (50.0, 0.6, None, true));
    }

    /// Проверка на настоящих пресетах: `KRITA_DIR=папка cargo test -p paint-formats -- --ignored`.
    #[test]
    #[ignore]
    fn reads_local_presets() {
        let Some(dir) = std::env::var_os("KRITA_DIR") else { return };
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            let r = match p.extension().and_then(|x| x.to_str()) {
                Some("kpp") => load_kpp(&p).map(|b| vec![b]),
                Some("bundle") => load_bundle(&p),
                _ => continue,
            };
            match r {
                Ok(v) => {
                    for b in v.iter().take(40) {
                        println!("OK {} | {:.1}px d={:.2} h={:.2} sp={:?} ps={:?} pd={:?} tip={:?} er={} sm={} {:?}", b.name, b.size, b.density, b.hardness, b.spacing, b.size_pressure, b.density_pressure, b.tip.as_ref().map(|t| (t.width, t.height)), b.erase, b.smudge, b.skipped);
                    }
                    println!("FILE {} {}", p.display(), v.len());
                }
                Err(e) => println!("ERR {} {e}", p.display()),
            }
        }
    }
}
