//! OpenRaster (`.ora`) — открытый формат со слоями (Krita, GIMP, MyPaint): zip-архив с
//! `mimetype`, `stack.xml`, PNG каждого слоя, сведённой картинкой и миниатюрой.
//!
//! Пишется: слои и папки (имя, видимость, непрозрачность, режим `composite-op`, положение),
//! «насквозь» у папки — `isolation="auto"`. Прикрепление в OpenRaster не описано: слой с ним
//! получает свой признак `grazysai:clipping` (его читает только GrazySAI), а при обычном режиме —
//! ещё и `svg:src-atop`, чтобы в других программах выглядел так же. Маски, бумага, эффекты и кривые
//! в OpenRaster не попадают (слой пишется пикселями).

use paint_core::blend::BlendMode;
use paint_core::pixel::from_rgba8;
use paint_core::{Document, Layer};
use std::path::Path;

type Res<T> = Result<T, Box<dyn std::error::Error>>;

fn op_of(mode: BlendMode) -> &'static str {
    use BlendMode::*;
    match mode {
        Normal => "svg:src-over",
        Multiply => "svg:multiply",
        Screen => "svg:screen",
        Overlay => "svg:overlay",
        Darken => "svg:darken",
        Lighten => "svg:lighten",
        ColorDodge | Dodge => "svg:color-dodge",
        ColorBurn | Burn => "svg:color-burn",
        LinearDodge | Shine => "svg:plus",
        HardLight => "svg:hard-light",
        SoftLight => "svg:soft-light",
        Difference => "svg:difference",
        // не из стандарта OpenRaster — имена Krita
        LinearBurn | Shade => "krita:linear_burn",
        Exclusion => "krita:exclusion",
        Subtract => "krita:subtract",
        Divide => "krita:divide",
        DarkerColor => "krita:darker color",
        LighterColor => "krita:lighter color",
        VividLight | BurnDodge => "krita:vivid_light",
        LinearLight | ShadeShine => "krita:linear light",
        PinLight => "krita:pin_light",
        HardMix => "krita:hard mix",
    }
}

fn mode_of(op: &str) -> BlendMode {
    use BlendMode::*;
    BlendMode::ALL.into_iter().find(|m| op_of(*m) == op && !matches!(m, Dodge | Burn | Shine | Shade | BurnDodge | ShadeShine)).unwrap_or(match op {
        "krita:linear_dodge" | "krita:add" => LinearDodge,
        _ => Normal,
    })
}

// --- zip: запись без сжатия (PNG уже сжаты), чтение — без сжатия и Deflate ---

fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (i, t) in table.iter_mut().enumerate() {
        let mut c = i as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
        }
        *t = c;
    }
    !data.iter().fold(!0u32, |c, &b| table[((c ^ b as u32) & 0xff) as usize] ^ (c >> 8))
}

fn zip(entries: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data) in entries {
        let offset = out.len() as u32;
        let crc = crc32(data);
        let head = |sig: u32| {
            let mut h = Vec::new();
            h.extend(sig.to_le_bytes());
            if sig == 0x0201_4b50 {
                h.extend(20u16.to_le_bytes());
            }
            h.extend(20u16.to_le_bytes());
            h.extend(0x0800u16.to_le_bytes()); // имена в UTF-8
            h.extend(0u16.to_le_bytes()); // без сжатия
            h.extend([0u8; 4]); // время и дата
            h.extend(crc.to_le_bytes());
            h.extend((data.len() as u32).to_le_bytes());
            h.extend((data.len() as u32).to_le_bytes());
            h.extend((name.len() as u16).to_le_bytes());
            h.extend(0u16.to_le_bytes());
            h
        };
        out.extend(head(0x0403_4b50));
        out.extend(name.as_bytes());
        out.extend(data);
        let mut c = head(0x0201_4b50);
        c.extend([0u8; 6]); // комментарий, диск, внутренние атрибуты
        c.extend(0u32.to_le_bytes());
        c.extend(offset.to_le_bytes());
        c.extend(name.as_bytes());
        central.extend(c);
    }
    let start = out.len() as u32;
    out.extend(&central);
    out.extend(0x0605_4b50u32.to_le_bytes());
    out.extend([0u8; 4]);
    out.extend((entries.len() as u16).to_le_bytes());
    out.extend((entries.len() as u16).to_le_bytes());
    out.extend((central.len() as u32).to_le_bytes());
    out.extend(start.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    out
}

pub(crate) fn unzip(d: &[u8]) -> Res<Vec<(String, Vec<u8>)>> {
    let u16_at = |i: usize| -> Res<usize> { Ok(u16::from_le_bytes(d.get(i..i + 2).ok_or("zip: обрезан")?.try_into()?) as usize) };
    let u32_at = |i: usize| -> Res<usize> { Ok(u32::from_le_bytes(d.get(i..i + 4).ok_or("zip: обрезан")?.try_into()?) as usize) };
    let end = (0..d.len().saturating_sub(21)).rev().find(|&i| d[i..].starts_with(&[0x50, 0x4b, 5, 6])).ok_or("не zip")?;
    let (count, mut p) = (u16_at(end + 10)?, u32_at(end + 16)?);
    let mut out = Vec::new();
    for _ in 0..count {
        if u32_at(p)? != 0x0201_4b50 {
            return Err("zip: испорчено оглавление".into());
        }
        let (method, csize, nlen, xlen, clen, local) = (u16_at(p + 10)?, u32_at(p + 20)?, u16_at(p + 28)?, u16_at(p + 30)?, u16_at(p + 32)?, u32_at(p + 42)?);
        let name = String::from_utf8_lossy(d.get(p + 46..p + 46 + nlen).ok_or("zip: обрезан")?).into_owned();
        p += 46 + nlen + xlen + clen;
        let data_at = local + 30 + u16_at(local + 26)? + u16_at(local + 28)?;
        let raw = d.get(data_at..data_at + csize).ok_or("zip: обрезан")?;
        let data = match method {
            0 => raw.to_vec(),
            8 => miniz_oxide::inflate::decompress_to_vec(raw).map_err(|e| format!("zip: {e:?}"))?,
            m => return Err(format!("zip: сжатие {m} не поддержано").into()),
        };
        out.push((name, data));
    }
    Ok(out)
}

// --- запись ---

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn write_stack(layers: &[Layer], depth: usize, xml: &mut String, files: &mut Vec<(String, Vec<u8>)>) -> Res<()> {
    let pad = " ".repeat(depth + 1);
    // в OpenRaster первый — верхний, у нас первый — нижний
    for l in layers.iter().rev() {
        let common = format!(
            "name=\"{}\" visibility=\"{}\" opacity=\"{:.3}\"",
            esc(&l.name),
            if l.visible { "visible" } else { "hidden" },
            l.opacity.clamp(0.0, 1.0)
        );
        let op = if l.clipping && l.mode == BlendMode::Normal { "svg:src-atop" } else { op_of(l.mode) };
        let clip = if l.clipping { " grazysai:clipping=\"true\"" } else { "" };
        match &l.children {
            Some(ch) => {
                let iso = if l.pass_through { "auto" } else { "isolate" };
                xml.push_str(&format!("{pad}<stack {common} composite-op=\"{op}\" isolation=\"{iso}\"{clip}>\n"));
                write_stack(ch, depth + 1, xml, files)?;
                xml.push_str(&format!("{pad}</stack>\n"));
            }
            None => {
                let (x, y, w, h, rgba) = crate::psd::layer_rgba(&l.pixels);
                let src = format!("data/layer{}.png", files.len());
                let png = if w == 0 { paint_core::io::encode_png(1, 1, &[0; 4])? } else { paint_core::io::encode_png(w as u32, h as u32, &rgba)? };
                files.push((src.clone(), png));
                xml.push_str(&format!("{pad}<layer {common} composite-op=\"{op}\" src=\"{src}\" x=\"{x}\" y=\"{y}\"{clip}/>\n"));
            }
        }
    }
    Ok(())
}

/// Сохранить в OpenRaster.
pub fn save(doc: &Document, path: &Path) -> Res<()> {
    let mut files = Vec::new();
    let dpi = doc.resolution as f64 / 65536.0 * 2.54;
    let mut xml = format!(
        "<?xml version='1.0' encoding='UTF-8'?>\n<image version=\"0.0.5\" w=\"{}\" h=\"{}\" xres=\"{:.0}\" yres=\"{:.0}\" xmlns:grazysai=\"https://github.com/grazyping/GrazySAI\">\n <stack>\n",
        doc.width, doc.height, dpi, dpi
    );
    write_stack(&doc.layers, 1, &mut xml, &mut files)?;
    xml.push_str(" </stack>\n</image>\n");
    let flat = doc.flatten();
    // миниатюра — не больше 256 по большей стороне
    let k = (doc.width.max(doc.height) as f64 / 256.0).max(1.0);
    let (tw, th) = (((doc.width as f64 / k).round() as u32).max(1), ((doc.height as f64 / k).round() as u32).max(1));
    let mut thumb = Vec::with_capacity((tw * th * 4) as usize);
    for y in 0..th {
        for x in 0..tw {
            let sx = ((x as f64 + 0.5) * k) as usize;
            let sy = ((y as f64 + 0.5) * k) as usize;
            let i = (sy.min(doc.height as usize - 1) * doc.width as usize + sx.min(doc.width as usize - 1)) * 4;
            thumb.extend_from_slice(&flat[i..i + 4]);
        }
    }
    let mut entries = vec![("mimetype".to_string(), b"image/openraster".to_vec()), ("stack.xml".to_string(), xml.into_bytes())];
    entries.extend(files);
    entries.push(("mergedimage.png".to_string(), paint_core::io::encode_png(doc.width, doc.height, &flat)?));
    entries.push(("Thumbnails/thumbnail.png".to_string(), paint_core::io::encode_png(tw, th, &thumb)?));
    paint_core::io::write_atomic(path, &zip(&entries))?;
    Ok(())
}

// --- чтение ---

/// Узел `stack.xml`: имя тега, атрибуты, дети.
struct Node {
    tag: String,
    attrs: Vec<(String, String)>,
    children: Vec<Node>,
}

impl Node {
    fn get(&self, k: &str) -> Option<&str> {
        self.attrs.iter().find(|(a, _)| a == k).map(|(_, v)| v.as_str())
    }
}

fn unesc(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

/// Простой разбор XML: только теги и атрибуты (текст между тегами не нужен).
fn parse_xml(s: &str) -> Res<Node> {
    let mut stack = vec![Node { tag: String::new(), attrs: Vec::new(), children: Vec::new() }];
    let mut rest = s;
    while let Some(lt) = rest.find('<') {
        rest = &rest[lt + 1..];
        let gt = rest.find('>').ok_or("stack.xml: незакрытый тег")?;
        let body = &rest[..gt];
        rest = &rest[gt + 1..];
        if body.starts_with('?') || body.starts_with('!') {
            continue;
        }
        if let Some(name) = body.strip_prefix('/') {
            let node = stack.pop().ok_or("stack.xml: лишний закрывающий тег")?;
            if node.tag != name.trim() {
                return Err(format!("stack.xml: ждали </{}>", node.tag).into());
            }
            stack.last_mut().ok_or("stack.xml: лишний закрывающий тег")?.children.push(node);
            continue;
        }
        let selfclose = body.ends_with('/');
        let body = body.trim_end_matches('/');
        let tag_end = body.find(char::is_whitespace).unwrap_or(body.len());
        let mut node = Node { tag: body[..tag_end].to_string(), attrs: Vec::new(), children: Vec::new() };
        let mut a = &body[tag_end..];
        while let Some(eq) = a.find('=') {
            let key = a[..eq].trim().to_string();
            let after = a[eq + 1..].trim_start();
            let q = after.chars().next().ok_or("stack.xml: атрибут без значения")?;
            // кавычка может оказаться многобайтной буквой (испорченный файл) — режем по её длине
            let ql = q.len_utf8();
            let close = after[ql..].find(q).ok_or("stack.xml: незакрытая кавычка")?;
            node.attrs.push((key, unesc(&after[ql..ql + close])));
            a = &after[ql + close + ql..];
        }
        if selfclose {
            stack.last_mut().ok_or("stack.xml: лишний закрывающий тег")?.children.push(node);
        } else {
            stack.push(node);
        }
    }
    let mut root = stack.pop().ok_or("stack.xml: пусто")?;
    root.children.pop().ok_or_else(|| "stack.xml: нет <image>".into())
}

fn read_stack(node: &Node, files: &[(String, Vec<u8>)], out: &mut Vec<Layer>) -> Res<()> {
    for n in node.children.iter().rev() {
        let mut layer = match n.tag.as_str() {
            "stack" => {
                let mut ch = Vec::new();
                read_stack(n, files, &mut ch)?;
                let mut f = Layer::folder("", ch);
                f.pass_through = n.get("isolation") == Some("auto");
                f
            }
            "layer" => {
                let mut l = Layer::new("");
                if let Some(src) = n.get("src") {
                    let data = &files.iter().find(|(name, _)| name == src).ok_or_else(|| format!("нет {src}"))?.1;
                    let (w, _, rgba) = paint_core::io::decode_png(data)?;
                    let (x0, y0) = (n.get("x").and_then(|v| v.parse::<i32>().ok()).unwrap_or(0), n.get("y").and_then(|v| v.parse::<i32>().ok()).unwrap_or(0));
                    for (i, p) in rgba.chunks(4).enumerate() {
                        if p[3] > 0 {
                            l.pixels.set(x0 + (i as u32 % w) as i32, y0 + (i as u32 / w) as i32, from_rgba8([p[0], p[1], p[2], p[3]]));
                        }
                    }
                }
                l
            }
            _ => continue,
        };
        layer.name = n.get("name").unwrap_or("").to_string();
        layer.visible = n.get("visibility") != Some("hidden");
        layer.opacity = n.get("opacity").and_then(|v| v.parse().ok()).unwrap_or(1.0f32).clamp(0.0, 1.0);
        let op = n.get("composite-op").unwrap_or("svg:src-over");
        layer.clipping = n.get("grazysai:clipping") == Some("true") || op == "svg:src-atop";
        layer.mode = mode_of(op);
        out.push(layer);
    }
    Ok(())
}

/// Миниатюра OpenRaster (`Thumbnails/thumbnail.png`, иначе `mergedimage.png`).
pub fn thumbnail(path: &Path) -> Res<(u32, u32, Vec<u8>)> {
    let files = unzip(&std::fs::read(path)?)?;
    let png = files
        .iter()
        .find(|(n, _)| n == "Thumbnails/thumbnail.png")
        .or_else(|| files.iter().find(|(n, _)| n == "mergedimage.png"))
        .ok_or("в архиве нет миниатюры")?;
    paint_core::io::decode_png(&png.1)
}

/// Открыть OpenRaster.
pub fn load(path: &Path) -> Res<Document> {
    let files = unzip(&std::fs::read(path)?)?;
    let xml = &files.iter().find(|(n, _)| n == "stack.xml").ok_or("в архиве нет stack.xml")?.1;
    let image = parse_xml(&String::from_utf8_lossy(xml))?;
    let w: u32 = image.get("w").and_then(|v| v.parse().ok()).ok_or("stack.xml: нет ширины")?;
    let h: u32 = image.get("h").and_then(|v| v.parse().ok()).ok_or("stack.xml: нет высоты")?;
    let mut doc = Document::new(w, h);
    doc.layers.clear();
    let root = image.children.iter().find(|n| n.tag == "stack").ok_or("stack.xml: нет <stack>")?;
    read_stack(root, &files, &mut doc.layers)?;
    if let Some(dpi) = image.get("xres").and_then(|v| v.parse::<f64>().ok()).filter(|d| *d > 0.0) {
        doc.resolution = (dpi / 2.54 * 65536.0).round() as u32;
    }
    if doc.layers.is_empty() {
        doc.layers.push(Layer::new("Слой 1"));
    }
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ora_round_trip_keeps_tree_and_picture() {
        let mut doc = Document::new(48, 32);
        doc.layers[0].name = "Фон & <низ>".into();
        for y in 0..32 {
            for x in 0..48 {
                doc.layers[0].pixels.set(x, y, from_rgba8([200, 210, 230, 255]));
            }
        }
        let mut a = Layer::new("Линии");
        for x in 5..40 {
            a.pixels.set(x, 9, from_rgba8([10, 20, 30, 200]));
        }
        a.mode = BlendMode::Multiply;
        a.opacity = 0.5;
        let mut c = Layer::new("Тень");
        c.pixels.set(7, 9, from_rgba8([255, 0, 0, 255]));
        c.clipping = true;
        let mut b = Layer::new("Внутри");
        b.pixels.set(30, 20, from_rgba8([0, 255, 0, 255]));
        b.visible = false;
        let mut f = Layer::folder("Папка", vec![b]);
        f.pass_through = true;
        doc.layers.extend([a, c, f]);
        let dir = std::env::temp_dir().join(format!("grazysai-ora-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("проба.ora");
        save(&doc, &p).unwrap();
        let back = load(&p).unwrap();
        let names: Vec<&str> = back.layers.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["Фон & <низ>", "Линии", "Тень", "Папка"]);
        assert_eq!(back.layers[1].mode, BlendMode::Multiply);
        assert!((back.layers[1].opacity - 0.5).abs() < 0.01);
        assert!(back.layers[2].clipping);
        assert!(back.layers[3].pass_through);
        assert!(!back.layers[3].children.as_ref().unwrap()[0].visible);
        let worst = back.flatten().iter().zip(doc.flatten()).map(|(a, b)| (*a as i32 - b as i32).abs()).max().unwrap();
        assert!(worst <= 1, "наибольшая разница {worst}");
        if let Ok(keep) = std::env::var("ORA_KEEP") {
            std::fs::copy(&p, keep).unwrap();
        }
        std::fs::remove_dir_all(dir).ok();
    }
}
