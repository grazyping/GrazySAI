//! Кисти Clip Studio Paint: `.sut` (одна кисть) и `.bfps` (палитра). Только чтение настроек
//! и текстуры; описание формата своими словами — docs/33-csp-brushes.md.

use paint_core::paper::PaperTexture;
use rusqlite::{Connection, OpenFlags};
use std::path::Path;

/// Кисть CSP в величинах GrazySAI. Где движок другой — приближение, пропущенное в `skipped`.
#[derive(Clone, Debug)]
pub struct CspBrush {
    pub name: String,
    /// Диаметр в пикселях (миллиметры пересчитаны при 350 dpi — разрешении нового холста CSP).
    pub size: f32,
    /// Непрозрачность × поток, 0..=1.
    pub density: f32,
    /// Жёсткость 0..=1.
    pub hardness: f32,
    /// Постоянный шаг в долях диаметра; `None` — автоматический.
    pub spacing: Option<f32>,
    /// Нажим на размер: доля размера при нулевом нажиме.
    pub size_pressure: Option<f32>,
    /// Нажим на непрозрачность: доля при нулевом нажиме.
    pub density_pressure: Option<f32>,
    /// Смешивание с холстом: (смешивание, разбавление) 0..=1.
    pub mix: Option<(f32, f32)>,
    /// Текстура бумаги из кисти: серая, светлое — краска ложится.
    pub texture: Option<PaperTexture>,
    /// Сила текстуры 0..=1 и масштаб, %.
    pub texture_density: f32,
    pub texture_scale: u16,
    /// Настройки CSP, которых у GrazySAI нет (имена — для сообщения).
    pub skipped: Vec<&'static str>,
}

type Res<T> = Result<T, Box<dyn std::error::Error>>;

/// Все кисти файла: у `.sut` — одна, у `.bfps` — все кисти палитры (без картинок: палитра хранит
/// только ссылки на материалы CSP).
pub fn load(path: &Path) -> Res<Vec<CspBrush>> {
    let db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
    let tool: i64 = db.query_row("SELECT ToolType FROM Manager", [], |r| r.get(0))?;
    if tool != 0 {
        return Err("в файле не кисти CSP".into());
    }
    let cols: Vec<String> = db.prepare("PRAGMA table_info(Variant)")?.query_map([], |r| r.get(1))?.collect::<Result<_, _>>()?;
    let texture = first_texture(&db);
    let mut nodes = db.prepare("SELECT NodeName, NodeVariantID, NodeInitVariantID FROM Node WHERE NodeVariantID > 0")?;
    let nodes: Vec<(String, i64, Option<i64>)> =
        nodes.query_map([], |r| Ok((r.get::<_, Option<String>>(0)?.unwrap_or_default(), r.get(1)?, r.get(2)?)))?.collect::<Result<_, _>>()?;
    let mut out = Vec::new();
    for (name, active, initial) in &nodes {
        // У `.sut` — текущее состояние (вложенные материалы — его). У палитры текущие строки
        // в поставке одинаковые для всех кистей (размер 15 и т. п.), настоящие значения — в исходных.
        let id = if nodes.len() > 1 { initial.unwrap_or(*active) } else { *active };
        let v = Variant::read(&db, &cols, id)?;
        out.push(v.brush(name, if nodes.len() == 1 { texture.clone() } else { None }));
    }
    if out.is_empty() {
        return Err("в файле нет кистей".into());
    }
    Ok(out)
}

/// Значения одной строки `Variant` по именам колонок (набор колонок зависит от версии схемы).
struct Variant {
    vals: std::collections::HashMap<String, rusqlite::types::Value>,
}

impl Variant {
    fn read(db: &Connection, cols: &[String], id: i64) -> Res<Self> {
        let mut vals = std::collections::HashMap::new();
        db.query_row("SELECT * FROM Variant WHERE VariantID = ?1", [id], |r| {
            for (i, c) in cols.iter().enumerate() {
                vals.insert(c.clone(), r.get::<_, rusqlite::types::Value>(i)?);
            }
            Ok(())
        })?;
        Ok(Self { vals })
    }

    fn num(&self, k: &str) -> Option<f64> {
        match self.vals.get(k)? {
            rusqlite::types::Value::Integer(i) => Some(*i as f64),
            rusqlite::types::Value::Real(f) => Some(*f),
            _ => None,
        }
    }

    fn on(&self, k: &str) -> bool {
        self.num(k).is_some_and(|v| v != 0.0)
    }

    fn blob(&self, k: &str) -> Option<&[u8]> {
        match self.vals.get(k)? {
            rusqlite::types::Value::Blob(b) => Some(b),
            _ => None,
        }
    }

    /// Нажим из «эффектора»: доля при нулевом нажиме, если зависимость от нажима включена.
    fn pressure(&self, k: &str) -> Option<f32> {
        let e = Effector::parse(self.blob(k)?)?;
        e.pressure.then_some((e.min as f32 / 100.0).clamp(0.0, 1.0))
    }

    fn brush(&self, name: &str, texture: Option<PaperTexture>) -> CspBrush {
        let mut skipped = Vec::new();
        let raw = self.num("BrushSize").unwrap_or(10.0) as f32;
        let size = match self.num("BrushSizeUnit").unwrap_or(0.0) as i64 {
            2 => raw * 350.0 / 25.4,
            _ => raw,
        };
        let pct = |k: &str, d: f64| (self.num(k).unwrap_or(d) / 100.0).clamp(0.0, 1.0) as f32;
        let spacing = (self.num("BrushAutoIntervalType") == Some(0.0)).then(|| (self.num("BrushInterval").unwrap_or(10.0) / 100.0).clamp(0.01, 10.0) as f32);
        let mix = self.on("BrushUseWaterColor").then(|| (1.0 - pct("BrushMixColor", 100.0), 1.0 - pct("BrushMixAlpha", 100.0)));
        for (k, what) in [
            ("BrushUseSpray", "распыление"),
            ("UseDualBrush", "двойная кисть"),
            ("BrushUsePatternImage", "форма кончика из картинки"),
            ("BrushChangeStrokeColor", "смена цвета по мазку"),
            ("BrushUseWaterEdge", "кайма акварели"),
        ] {
            if self.on(k) {
                skipped.push(what);
            }
        }
        if self.num("BrushThickness").is_some_and(|t| t != 100.0) {
            skipped.push("толщина кончика");
        }
        let texture = texture.filter(|_| self.blob("TextureImage").is_some()).map(|t| {
            if self.on("TextureReverseDensity") {
                PaperTexture { gray: t.gray.iter().map(|g| 255 - g).collect(), ..t }
            } else {
                t
            }
        });
        if texture.is_none() && self.blob("TextureImage").is_some_and(|b| b.len() > 16) {
            skipped.push("текстура (картинки нет в файле)");
        }
        CspBrush {
            name: name.to_string(),
            size: size.clamp(1.0, 5000.0),
            density: pct("Opacity", 100.0) * pct("BrushFlow", 100.0),
            hardness: pct("BrushHardness", 100.0),
            spacing,
            size_pressure: self.pressure("BrushSizeEffector"),
            density_pressure: self.pressure("BrushOpacityEffector"),
            mix,
            texture,
            texture_density: pct("TextureDensity", 100.0),
            texture_scale: self.num("TextureScale").unwrap_or(100.0).clamp(1.0, 1000.0) as u16,
            skipped,
        }
    }
}

/// Зависимость параметра от входов пера: заголовок из восьми чисел (big-endian), затем кривые.
struct Effector {
    pressure: bool,
    min: i32,
}

impl Effector {
    fn parse(b: &[u8]) -> Option<Self> {
        let at = |i: usize| b.get(i * 4..i * 4 + 4).map(|s| i32::from_be_bytes([s[0], s[1], s[2], s[3]]));
        if at(0)? != 44 {
            return None;
        }
        Some(Self { pressure: at(2)? & 0x10 != 0, min: at(3)? })
    }
}

/// Первая текстура бумаги среди вложенных материалов. Порядок строк в разных экспортах разный,
/// но основная текстура в известных образцах идёт раньше текстуры двойной кисти; вид материала
/// узнаётся по метке в `icedata/layerData.xml`. Картинка — миниатюра `thumbnail/thumbnail.png`:
/// сам слой материала обычным PNG не читается.
fn first_texture(db: &Connection) -> Option<PaperTexture> {
    let mut q = db.prepare("SELECT FileData FROM MaterialFile ORDER BY _PW_ID").ok()?;
    let rows = q.query_map([], |r| r.get::<_, Option<Vec<u8>>>(0)).ok()?;
    for data in rows.flatten().flatten() {
        let files = untar(&data);
        let tag = files.iter().find(|(n, _)| n.ends_with("layerData.xml")).map(|(_, d)| String::from_utf8_lossy(d).into_owned());
        if !tag.is_some_and(|t| t.contains("PaperTexture")) {
            continue;
        }
        let Some((_, png)) = files.iter().find(|(n, _)| n.ends_with("thumbnail.png")) else { continue };
        if let Some(t) = gray_of_png(png) {
            return Some(t);
        }
    }
    None
}

/// Серая картинка по PNG: прозрачное — как белое (краска ложится), яркость по BT.601.
fn gray_of_png(png: &[u8]) -> Option<PaperTexture> {
    let (w, h, rgba) = paint_core::io::decode_png(png).ok()?;
    let (w, h) = (w as usize, h as usize);
    if w == 0 || h == 0 || w > 4096 || h > 4096 {
        return None;
    }
    let gray = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| {
            let a = p[3] as u32;
            let l = (p[0] as u32 * 77 + p[1] as u32 * 150 + p[2] as u32 * 29) >> 8;
            ((l * a + 255 * (255 - a)) / 255) as u8
        })
        .collect();
    Some(PaperTexture { width: w, height: h, gray })
}

/// Файлы tar (ustar, pax, длинные имена GNU): имя и содержимое.
pub fn untar(d: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    let mut pos = 0;
    let mut long: Option<String> = None;
    while pos + 512 <= d.len() {
        let h = &d[pos..pos + 512];
        if h.iter().all(|&b| b == 0) {
            break;
        }
        let text = |r: std::ops::Range<usize>| {
            let s = &h[r];
            String::from_utf8_lossy(&s[..s.iter().position(|&b| b == 0).unwrap_or(s.len())]).into_owned()
        };
        let Ok(size) = usize::from_str_radix(text(124..136).trim(), 8) else { break };
        let body = pos + 512;
        let Some(data) = d.get(body..body + size) else { break };
        let kind = h[156];
        let mut name = text(0..100);
        let prefix = text(345..500);
        if &h[257..262] == b"ustar" && !prefix.is_empty() {
            name = format!("{prefix}/{name}");
        }
        match kind {
            b'x' => long = pax_path(data),
            b'L' => long = Some(String::from_utf8_lossy(data).trim_end_matches('\0').to_string()),
            b'0' | 0 => out.push((long.take().unwrap_or(name), data.to_vec())),
            _ => long = None,
        }
        pos = body + size.div_ceil(512) * 512;
    }
    out
}

/// `path=` из записей pax («длина ключ=значение\n»).
fn pax_path(d: &[u8]) -> Option<String> {
    let s = String::from_utf8_lossy(d);
    s.lines().find_map(|l| l.split_once(' ').and_then(|(_, kv)| kv.strip_prefix("path=")).map(str::to_string))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tar_entry(out: &mut Vec<u8>, name: &str, kind: u8, data: &[u8]) {
        let mut h = [0u8; 512];
        h[..name.len()].copy_from_slice(name.as_bytes());
        h[124..135].copy_from_slice(format!("{:011o}", data.len()).as_bytes());
        h[156] = kind;
        h[257..263].copy_from_slice(b"ustar\0");
        out.extend_from_slice(&h);
        out.extend_from_slice(data);
        out.resize(out.len().div_ceil(512) * 512, 0);
    }

    fn effector(mask: i32, min: i32) -> Vec<u8> {
        [44, 240, mask, min, 100, 0, 0, 0, 0, 0, 100].iter().flat_map(|v: &i32| v.to_be_bytes()).collect()
    }

    /// Кисть, собранная по описанию формата (своих образцов CSP в репозитории нет).
    #[test]
    fn reads_a_synthetic_sut() {
        let dir = std::env::temp_dir().join(format!("grazysai-csp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("brush.sut");
        let _ = std::fs::remove_file(&path);
        let db = Connection::open(&path).unwrap();
        db.execute_batch(
            "CREATE TABLE Manager(_PW_ID INTEGER PRIMARY KEY, ToolType INTEGER, Version INTEGER);
             INSERT INTO Manager VALUES(1, 0, 142);
             CREATE TABLE Node(_PW_ID INTEGER PRIMARY KEY, NodeName TEXT, NodeVariantID INTEGER, NodeInitVariantID INTEGER);
             INSERT INTO Node VALUES(1, 'Мел', 1, 2);
             CREATE TABLE Variant(_PW_ID INTEGER PRIMARY KEY, VariantID INTEGER, Opacity INTEGER, BrushSize REAL,
               BrushSizeUnit INTEGER, BrushFlow INTEGER, BrushHardness INTEGER, BrushInterval REAL,
               BrushAutoIntervalType INTEGER, BrushThickness INTEGER, BrushUseWaterColor INTEGER, BrushMixColor INTEGER,
               BrushMixAlpha INTEGER, UseDualBrush INTEGER, TextureImage BLOB, TextureDensity INTEGER,
               TextureReverseDensity INTEGER, BrushSizeEffector BLOB, BrushOpacityEffector BLOB);
             CREATE TABLE MaterialFile(_PW_ID INTEGER PRIMARY KEY AUTOINCREMENT, FileData BLOB);",
        )
        .unwrap();
        let ins = "INSERT INTO Variant(VariantID, Opacity, BrushSize, BrushSizeUnit, BrushFlow, BrushHardness, BrushInterval,
             BrushAutoIntervalType, BrushThickness, BrushUseWaterColor, BrushMixColor, BrushMixAlpha, UseDualBrush,
             TextureImage, TextureDensity, TextureReverseDensity, BrushSizeEffector, BrushOpacityEffector)
             VALUES(?1, ?2, 30.0, 0, 50, 80, 200.0, 0, 50, 1, 70, 100, 1, x'00000008', 80, 0, ?3, ?4)";
        db.execute(ins, rusqlite::params![1, 100, effector(0x30, 25), effector(0x80, 60)]).unwrap();
        db.execute(ins, rusqlite::params![2, 10, effector(0, 0), effector(0, 0)]).unwrap();
        // текстура: PNG 2×1 (чёрный и прозрачный) и метка вида материала
        let mut png = Vec::new();
        {
            let mut e = png::Encoder::new(&mut png, 2, 1);
            e.set_color(png::ColorType::Rgba);
            e.write_header().unwrap().write_image_data(&[0, 0, 0, 255, 0, 0, 0, 0]).unwrap();
        }
        let mut tar = Vec::new();
        tar_entry(&mut tar, "PaxHeader", b'x', b"30 path=icedata/layerData.xml\n");
        tar_entry(&mut tar, "ignored", b'0', b"<layer type=\"PaperTexture\"/>");
        tar_entry(&mut tar, "thumbnail/thumbnail.png", b'0', &png);
        db.execute("INSERT INTO MaterialFile(FileData) VALUES(?1)", [tar]).unwrap();
        drop(db);

        let b = load(&path).unwrap();
        assert_eq!(b.len(), 1);
        let b = &b[0];
        assert_eq!(b.name, "Мел");
        assert_eq!(b.size, 30.0);
        assert!((b.density - 0.5).abs() < 1e-6, "текущее состояние, а не исходное");
        assert!((b.hardness - 0.8).abs() < 1e-6);
        assert_eq!(b.spacing, Some(2.0));
        assert_eq!(b.size_pressure, Some(0.25));
        assert_eq!(b.density_pressure, None, "нажим не включён — только скорость");
        let (blend, dilute) = b.mix.unwrap();
        assert!((blend - 0.3).abs() < 1e-6 && dilute.abs() < 1e-6);
        let t = b.texture.as_ref().unwrap();
        assert_eq!((t.width, t.height, t.gray.clone()), (2, 1, vec![0, 255]));
        assert!(b.skipped.contains(&"двойная кисть") && b.skipped.contains(&"толщина кончика"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn effector_header_gives_pressure_and_minimum() {
        assert!(Effector::parse(&effector(0x10, 40)).is_some_and(|e| e.pressure && e.min == 40));
        assert!(Effector::parse(&[0; 8]).is_none());
    }

    /// Проверка на своей кисти или палитре: `CSP_SAMPLE=путь cargo test -p paint-formats -- --ignored`.
    #[test]
    #[ignore]
    fn reads_a_local_file() {
        let Some(p) = std::env::var_os("CSP_SAMPLE") else { return };
        for b in load(Path::new(&p)).unwrap() {
            println!("{} {:.1}px d={:.2} h={:.2} sp={:?} ps={:?} pd={:?} mix={:?} tex={} {:?}", b.name, b.size, b.density, b.hardness, b.spacing, b.size_pressure, b.density_pressure, b.mix, b.texture.is_some(), b.skipped);
        }
    }
}
