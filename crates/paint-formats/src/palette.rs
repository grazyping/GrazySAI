//! Палитры других программ: GIMP (`.gpl`, текст) и Krita (`.kpl`, zip с `colorset.xml`).
//! Читаются цвета RGB с именами; остальное (группы, раскладка, другие цветовые пространства) —
//! пропускается.

use std::path::Path;

type Res<T> = Result<T, Box<dyn std::error::Error>>;

/// Палитра: имя и цвета (RGB 0..255, имя цвета).
#[derive(Debug, Default, PartialEq)]
pub struct Palette {
    pub name: String,
    pub colors: Vec<([u8; 3], String)>,
}

/// Открыть `.gpl` или `.kpl` по расширению.
pub fn load(path: &Path) -> Res<Palette> {
    let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    let data = std::fs::read(path)?;
    let mut p = match ext.as_str() {
        "gpl" => parse_gpl(&String::from_utf8_lossy(&data))?,
        "kpl" => parse_kpl(&data)?,
        _ => return Err("палитра: нужен файл .gpl или .kpl".into()),
    };
    if p.name.is_empty() {
        p.name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    }
    Ok(p)
}

/// GIMP: первая строка `GIMP Palette`, затем `Name:`/`Columns:`, комментарии `#`, строки
/// `R G B имя`.
pub fn parse_gpl(text: &str) -> Res<Palette> {
    let mut lines = text.lines();
    if !lines.next().is_some_and(|l| l.trim_start_matches('\u{feff}').trim() == "GIMP Palette") {
        return Err("палитра GIMP: нет заголовка «GIMP Palette»".into());
    }
    let mut p = Palette::default();
    for l in lines {
        let t = l.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        if let Some(n) = t.strip_prefix("Name:") {
            p.name = n.trim().to_string();
            continue;
        }
        if t.starts_with("Columns:") {
            continue;
        }
        let mut it = t.split_whitespace();
        let mut rgb = [0u8; 3];
        let mut ok = true;
        for c in &mut rgb {
            match it.next().and_then(|v| v.parse::<u16>().ok()) {
                Some(v) => *c = v.min(255) as u8,
                None => ok = false,
            }
        }
        if ok {
            p.colors.push((rgb, it.collect::<Vec<_>>().join(" ")));
        }
    }
    Ok(p)
}

/// Значение атрибута `key="…"` внутри тега.
fn attr<'a>(tag: &'a str, key: &str) -> Option<&'a str> {
    let i = tag.find(&format!(" {key}=\""))? + key.len() + 3;
    let rest = &tag[i..];
    Some(&rest[..rest.find('"')?])
}

/// Krita: zip, в нём `colorset.xml` — `<ColorSet name=…>` и `<ColorSetEntry name=…>` с вложенным
/// `<RGB r="0..1" g=… b=…/>`.
pub fn parse_kpl(data: &[u8]) -> Res<Palette> {
    let files = crate::ora::unzip(data)?;
    let xml = files.iter().find(|(n, _)| n == "colorset.xml").ok_or("палитра Krita: нет colorset.xml")?;
    let xml = String::from_utf8_lossy(&xml.1);
    let mut p = Palette::default();
    if let Some(i) = xml.find("<ColorSet") {
        let tag = &xml[i..i + xml[i..].find('>').unwrap_or(0)];
        p.name = attr(tag, "name").unwrap_or("").to_string();
    }
    let mut pos = 0;
    while let Some(i) = xml[pos..].find("<ColorSetEntry") {
        let start = pos + i;
        let end = xml[start..].find("</ColorSetEntry>").map_or(xml.len(), |e| start + e);
        let entry = &xml[start..end];
        let head = &entry[..entry.find('>').unwrap_or(entry.len())];
        let name = attr(head, "name").unwrap_or("").to_string();
        if let Some(j) = entry.find("<RGB") {
            let tag = &entry[j..j + entry[j..].find('>').unwrap_or(entry.len() - j)];
            let ch = |k| attr(tag, k).and_then(|v| v.parse::<f32>().ok()).map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
            if let (Some(r), Some(g), Some(b)) = (ch("r"), ch("g"), ch("b")) {
                p.colors.push(([r, g, b], name));
            }
        }
        pos = end;
    }
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gimp_palette() {
        let p = parse_gpl("GIMP Palette\nName: Тест\nColumns: 4\n# комментарий\n255   0   0\tКрасный\n  0 128 255 Синий цвет\n\n1 2\n").unwrap();
        assert_eq!(p.name, "Тест");
        assert_eq!(p.colors, vec![([255, 0, 0], "Красный".to_string()), ([0, 128, 255], "Синий цвет".to_string())]);
        assert!(parse_gpl("не палитра").is_err());
    }

    #[test]
    fn krita_palette() {
        let xml = r#"<?xml version="1.0"?><ColorSet name="Набор" columns="8" version="1.0"><ColorSetEntry name="Красный" spot="false" bitdepth="U8"><RGB r="1" g="0" b="0" space="sRGB-elle-V2-srgbtrc.icc"/><Position row="0" column="0"/></ColorSetEntry><ColorSetEntry name="Серый" bitdepth="U8"><RGB r="0.5" g="0.5" b="0.5"/></ColorSetEntry><ColorSetEntry name="CMYK"><CMYK c="1" m="0" y="0" k="0"/></ColorSetEntry></ColorSet>"#;
        // zip из одного несжатого файла
        let mut z = Vec::new();
        let name = b"colorset.xml";
        let crc = {
            let mut c = !0u32;
            for &b in xml.as_bytes() {
                c ^= b as u32;
                for _ in 0..8 {
                    c = if c & 1 != 0 { (c >> 1) ^ 0xEDB8_8320 } else { c >> 1 };
                }
            }
            !c
        };
        let n = xml.len() as u32;
        z.extend(b"PK\x03\x04\x14\0\0\0\0\0\0\0\0\0");
        z.extend(crc.to_le_bytes());
        z.extend(n.to_le_bytes());
        z.extend(n.to_le_bytes());
        z.extend((name.len() as u16).to_le_bytes());
        z.extend(0u16.to_le_bytes());
        z.extend(name);
        z.extend(xml.as_bytes());
        let cd = z.len() as u32;
        z.extend(b"PK\x01\x02\x14\0\x14\0\0\0\0\0\0\0\0\0");
        z.extend(crc.to_le_bytes());
        z.extend(n.to_le_bytes());
        z.extend(n.to_le_bytes());
        z.extend((name.len() as u16).to_le_bytes());
        z.extend([0u8; 12]);
        z.extend(0u32.to_le_bytes());
        z.extend(name);
        let cd_len = z.len() as u32 - cd;
        z.extend(b"PK\x05\x06\0\0\0\0\x01\0\x01\0");
        z.extend(cd_len.to_le_bytes());
        z.extend(cd.to_le_bytes());
        z.extend(0u16.to_le_bytes());
        let p = parse_kpl(&z).unwrap();
        assert_eq!(p.name, "Набор");
        assert_eq!(p.colors, vec![([255, 0, 0], "Красный".to_string()), ([128, 128, 128], "Серый".to_string())]);
    }
}
