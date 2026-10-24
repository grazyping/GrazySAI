//! Отпечатки кистей из файлов: любая картинка или кисть GIMP `.gbr`.

use paint_core::Stamp;
use std::path::Path;

type Res<T> = Result<T, Box<dyn std::error::Error>>;

/// Чужая кисть (Clip Studio Paint, Krita) в величинах GrazySAI. Где движок другой — приближение,
/// пропущенное перечислено в `skipped`.
#[derive(Clone, Debug)]
pub struct ImportedBrush {
    pub name: String,
    /// Диаметр в пикселях (у CSP миллиметры пересчитаны при 350 dpi — разрешении нового холста CSP).
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
    pub texture: Option<paint_core::paper::PaperTexture>,
    /// Отпечаток-картинка (миниатюра материала-отпечатка; приближение) и его поворот в градусах.
    pub tip: Option<paint_core::Stamp>,
    pub tip_angle: f32,
    /// Следующие картинки набора и смена вразнобой (иначе по порядку).
    pub tip_more: Vec<paint_core::Stamp>,
    pub tip_random: bool,
    /// Толщина кончика (1 — как есть, меньше — сплюснут) и поворот по направлению мазка.
    pub tip_ratio: f32,
    pub tip_follow: bool,
    /// Сила текстуры 0..=1 и масштаб, %.
    pub texture_density: f32,
    pub texture_scale: u16,
    /// Ластик.
    pub erase: bool,
    /// Размазывающая кисть (смешивание с холстом без своего цвета).
    pub smudge: bool,
    /// Россыпь и разброс (кисть «Россыпь»).
    pub scatter: Option<paint_core::scatter::ScatterParams>,
    /// Настройки, которых у GrazySAI нет (имена — для сообщения).
    pub skipped: Vec<&'static str>,
}

impl ImportedBrush {
    /// Простая кисть: круглая, жёсткая, без нажима.
    pub fn new(name: &str, size: f32) -> Self {
        Self {
            name: name.to_string(),
            size,
            density: 1.0,
            hardness: 1.0,
            spacing: None,
            size_pressure: None,
            density_pressure: None,
            mix: None,
            texture: None,
            tip: None,
            tip_angle: 0.0,
            tip_more: Vec::new(),
            tip_random: false,
            tip_ratio: 1.0,
            tip_follow: false,
            texture_density: 1.0,
            texture_scale: 100,
            erase: false,
            smudge: false,
            scatter: None,
            skipped: Vec::new(),
        }
    }
}


/// Наибольшая сторона хранимого отпечатка: крупнее — уменьшается (кисть всё равно рисует
/// отпечаток размером с кисть, а уменьшение из огромной картинки давало бы рябь).
pub const MAX_SIDE: usize = 512;

/// Имя, все картинки набора и смена вразнобой: у `.gih` — все клетки, у остальных — одна.
pub fn load_all(path: &Path) -> Res<(String, Vec<Stamp>, bool)> {
    let ext = path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).unwrap_or_default();
    if ext == "gih" {
        let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let (name, cells, random) = gih_all(&std::fs::read(path)?)?;
        let cells = cells.into_iter().map(|s| shrink(s, MAX_SIDE)).collect();
        return Ok((if name.trim().is_empty() { stem } else { name }, cells, random));
    }
    load(path).map(|(n, s)| (n, vec![s], false))
}

/// Кончик-картинка SVG (как у Krita): рисуется на белом шириной до `MAX_SIDE`, серое
/// становится краской — чёрное полная, белое нет.
pub fn svg(d: &[u8]) -> Res<Stamp> {
    let tree = resvg::usvg::Tree::from_data(d, &resvg::usvg::Options::default())?;
    let size = tree.size();
    let (w, h) = (size.width(), size.height());
    if !(w > 0.0 && h > 0.0) {
        return Err("SVG: пустой размер".into());
    }
    let k = MAX_SIDE as f32 / w.max(h);
    let (pw, ph) = (((w * k).round() as u32).max(1), ((h * k).round() as u32).max(1));
    let mut pix = resvg::tiny_skia::Pixmap::new(pw, ph).ok_or("SVG: неверный размер")?;
    pix.fill(resvg::tiny_skia::Color::WHITE);
    resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(k, k), &mut pix.as_mut());
    // на белом фоне точки непрозрачны: цвет — уже итог наложения
    let alpha = pix.data().as_chunks::<4>().0.iter().map(|p| 255 - ((p[0] as u32 * 77 + p[1] as u32 * 150 + p[2] as u32 * 29 + 128) >> 8) as u8).collect();
    Ok(Stamp { width: pw as usize, height: ph as usize, alpha })
}

/// Все клетки набора GIMP и смена вразнобой (`sel0:random` в строке параметров).
pub fn gih_all(d: &[u8]) -> Res<(String, Vec<Stamp>, bool)> {
    let mut lines = d.splitn(3, |&b| b == b'\n');
    let name = String::from_utf8_lossy(lines.next().ok_or("GIH: пусто")?).trim().to_string();
    let params = String::from_utf8_lossy(lines.next().ok_or("GIH: нет строки параметров")?).into_owned();
    let mut rest = lines.next().ok_or("GIH: нет клеток")?;
    let mut cells = Vec::new();
    // клеток не больше заявленного и не больше 256 (испорченный файл не займёт память без меры)
    let want = params.split_whitespace().next().and_then(|n| n.parse::<usize>().ok()).unwrap_or(1).clamp(1, 256);
    while cells.len() < want && rest.len() >= 28 {
        let be = |o: usize| u32::from_be_bytes([rest[o], rest[o + 1], rest[o + 2], rest[o + 3]]) as usize;
        let size = be(0).saturating_add(be(8).saturating_mul(be(12)).saturating_mul(be(16)));
        let (_, s) = gbr(rest)?;
        cells.push(s);
        rest = rest.get(size..).unwrap_or(&[]);
    }
    if cells.is_empty() {
        return Err("GIH: нет клеток".into());
    }
    Ok((name, cells, params.contains(":random")))
}

/// Имя и отпечаток из файла.
pub fn load(path: &Path) -> Res<(String, Stamp)> {
    let ext = path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).unwrap_or_default();
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let (name, stamp) = if ext == "gbr" {
        gbr(&std::fs::read(path)?).map(|(n, s)| (if n.trim().is_empty() { stem } else { n }, s))?
    } else if ext == "gih" {
        gih(&std::fs::read(path)?).map(|(n, s)| (if n.trim().is_empty() { stem } else { n }, s))?
    } else if ext == "svg" {
        (stem, svg(&std::fs::read(path)?)?)
    } else {
        let (w, h, rgba) = paint_core::io::load_image(path)?;
        (stem, Stamp::from_rgba(w as usize, h as usize, &rgba))
    };
    if stamp.width == 0 || stamp.height == 0 {
        return Err("пустая картинка".into());
    }
    Ok((name, shrink(stamp, MAX_SIDE)))
}

/// Кисть GIMP: заголовок из чисел big-endian (длина заголовка, версия, ширина, высота, байт на
/// точку, «GIMP», шаг), имя UTF-8 до конца заголовка, затем точки: 1 байт — маска (255 —
/// краска), 4 — RGBA (краска — непрозрачность).
pub fn gbr(d: &[u8]) -> Res<(String, Stamp)> {
    let be = |o: usize| d.get(o..o + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize).ok_or("GBR: файл обрывается");
    let (header, version, w, h, bytes) = (be(0)?, be(4)?, be(8)?, be(12)?, be(16)?);
    if version == 2 && d.get(20..24) != Some(b"GIMP") {
        return Err("GBR: нет подписи GIMP".into());
    }
    if !(1..=8192).contains(&w) || !(1..=8192).contains(&h) || !(bytes == 1 || bytes == 4) || header > d.len() {
        return Err("GBR: неверный заголовок".into());
    }
    let name_at = if version == 2 { 28 } else { 20 };
    let name = String::from_utf8_lossy(d.get(name_at.min(header)..header).unwrap_or(&[])).trim_end_matches('\0').to_string();
    let data = d.get(header..header + w * h * bytes).ok_or("GBR: мало данных")?;
    let alpha = if bytes == 1 { data.to_vec() } else { data.as_chunks::<4>().0.iter().map(|p| p[3]).collect() };
    Ok((name, Stamp { width: w, height: h, alpha }))
}

/// Кисти Photoshop `.abr`: картинки отпечатков. Версии 1 и 2 — список кистей (у «вычисляемых»
/// картинки нет, они пропускаются), 6 — раздел `8BIM samp` с картинками. Значение точки —
/// количество краски (255 — полная), 8 или 16 бит, без сжатия или PackBits по строкам.
pub fn abr(d: &[u8]) -> Res<Vec<(String, Stamp)>> {
    let out: Vec<_> = abr_parts(d)?.0.into_iter().map(|(n, _, st)| (n, st)).collect();
    if out.is_empty() {
        return Err("ABR: в файле нет картинок кистей".into());
    }
    Ok(out)
}

/// Картинки `.abr`: имя, ключ картинки (у версии 6) и картинка.
type AbrImages = Vec<(String, String, Stamp)>;

/// Картинки `.abr` и раздел `desc` с настройками.
fn abr_parts(d: &[u8]) -> Res<(AbrImages, Option<&[u8]>)> {
    struct R<'a> {
        d: &'a [u8],
        p: usize,
    }
    impl R<'_> {
        fn take(&mut self, n: usize) -> Res<&[u8]> {
            let s = self.d.get(self.p..self.p.checked_add(n).ok_or("ABR: файл обрывается")?).ok_or("ABR: файл обрывается")?;
            self.p += n;
            Ok(s)
        }
        fn u16(&mut self) -> Res<u16> {
            let b = self.take(2)?;
            Ok(u16::from_be_bytes([b[0], b[1]]))
        }
        fn u32(&mut self) -> Res<u32> {
            let b = self.take(4)?;
            Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
        }
        fn i32(&mut self) -> Res<i32> {
            Ok(self.u32()? as i32)
        }
        /// Прямоугольник, глубина, сжатие и точки одной картинки.
        fn image(&mut self) -> Res<Stamp> {
            let (top, left, bottom, right) = (self.i32()?, self.i32()?, self.i32()?, self.i32()?);
            let depth = self.u16()?;
            let comp = self.take(1)?[0];
            let (w, h) = ((right as i64 - left as i64), (bottom as i64 - top as i64));
            if !(1..=8192).contains(&w) || !(1..=8192).contains(&h) || !(depth == 8 || depth == 16) {
                return Err("ABR: неверный размер картинки".into());
            }
            let (w, h, bpp) = (w as usize, h as usize, depth as usize / 8);
            let row = w * bpp;
            let raw = if comp == 0 {
                self.take(row * h)?.to_vec()
            } else {
                let counts: Vec<usize> = (0..h).map(|_| self.u16().map(|c| c as usize)).collect::<Res<_>>()?;
                // PackBits разворачивается не больше чем в 128 раз: иначе файл испорчен, а память под
                // заявленный размер не нужна
                if row * h > counts.iter().sum::<usize>().saturating_mul(128) {
                    return Err("ABR: размер картинки не сходится с данными".into());
                }
                let mut out = Vec::with_capacity(row * h);
                for c in counts {
                    out.extend(crate::psd::unpackbits(self.take(c)?, row)?);
                }
                out
            };
            let alpha = if bpp == 1 { raw } else { raw.as_chunks::<2>().0.iter().map(|c| c[0]).collect() };
            // уменьшить сразу: в наборе до тысячи кистей, а держать нужно только хранимый размер
            Ok(shrink(Stamp { width: w, height: h, alpha }, MAX_SIDE))
        }
    }
    let mut r = R { d, p: 0 };
    let version = r.u16()?;
    let mut out = Vec::new();
    let mut desc = None;
    match version {
        1 | 2 => {
            let count = r.u16()?.min(1000);
            for i in 0..count {
                let kind = r.u16()?;
                let size = r.u32()? as usize;
                let next = r.p.saturating_add(size);
                if kind == 2 {
                    r.take(6)?; // прочее и шаг
                    let mut name = String::new();
                    if version == 2 {
                        let n = r.u32()?.min(1024) as usize;
                        let units: Vec<u16> = (0..n).map(|_| r.u16()).collect::<Res<_>>()?;
                        name = String::from_utf16_lossy(&units).trim_end_matches('\0').to_string();
                    }
                    r.take(9)?; // сглаживание и короткий прямоугольник
                    let st = r.image()?;
                    out.push((if name.is_empty() { format!("{}", i + 1) } else { name }, String::new(), st));
                }
                r.p = next;
            }
        }
        6..=10 => {
            let sub = r.u16()?;
            // разделы «8BIM» + имя + длина; картинки — в «samp»
            while r.p + 12 <= d.len() {
                if r.take(4)? != b"8BIM" {
                    return Err("ABR: нет раздела 8BIM".into());
                }
                let key = r.take(4)?.to_vec();
                let size = r.u32()? as usize;
                let end = r.p.saturating_add(size).min(d.len());
                if key == b"desc" {
                    desc = d.get(r.p..end);
                }
                if key != b"samp" {
                    r.p = end;
                    continue;
                }
                while r.p + 4 <= end && out.len() < 1000 {
                    let len = r.u32()? as usize;
                    let next = r.p.saturating_add(len.div_ceil(4) * 4);
                    // ключ кисти: длина и строка (UUID, по нему на картинку ссылается `desc`) в 37
                    // байтах, затем 10 (подверсия 1) или 264 байта прочего
                    let k = r.take(37)?;
                    let id = String::from_utf8_lossy(&k[1..1 + (k[0] as usize).min(36)]).into_owned();
                    r.take(if sub == 1 { 10 } else { 264 })?;
                    match r.image() {
                        Ok(st) => out.push((format!("{}", out.len() + 1), id, st)),
                        Err(e) if out.is_empty() => return Err(e),
                        Err(_) => {}
                    }
                    r.p = next;
                }
                r.p = end;
            }
        }
        v => return Err(format!("ABR: версия {v} не поддерживается").into()),
    }
    if out.is_empty() && desc.is_none() {
        return Err("ABR: в файле нет картинок кистей".into());
    }
    Ok((out, desc))
}

/// Кисти Photoshop `.abr` с настройками из раздела `desc` (версия 6 и новее): размер, шаг,
/// жёсткость, угол и сплюснутость кончика, нажим на размер и непрозрачность, россыпь и разброс
/// (кисть «Россыпь»), смена цвета. Без `desc` (версии 1, 2) — картинки с обычными настройками.
pub fn abr_brushes(d: &[u8]) -> Res<Vec<ImportedBrush>> {
    use crate::descriptor::Value;
    let (images, desc) = abr_parts(d)?;
    let plain = |name: String, tip: Stamp| ImportedBrush { name, tip: Some(tip), ..ImportedBrush::new("", 40.0) };
    let Some(desc) = desc.and_then(|d| crate::descriptor::read(d).ok()) else {
        let out: Vec<_> = images.into_iter().map(|(n, _, st)| ImportedBrush { spacing: Some(0.1), ..plain(n, st) }).collect();
        return if out.is_empty() { Err("ABR: в файле нет картинок кистей".into()) } else { Ok(out) };
    };
    let num = |v: &Value, k: &str| v.get(k).and_then(Value::num);
    let on = |v: &Value, k: &str| v.get(k).and_then(Value::bool).unwrap_or(false);
    // разброс 0..1 и управление (0 — нет, 2 — нажим, 6 и 7 — направление) у «brVr»
    let var = |v: &Value, k: &str| v.get(k).map_or((0.0, 0), |b| ((num(b, "jitter").unwrap_or(0.0) / 100.0).clamp(0.0, 1.0) as f32, num(b, "bVTy").unwrap_or(0.0) as i32));
    let mut out = Vec::new();
    for p in desc.get("Brsh").map_or(&[][..], Value::list) {
        let Some(tip) = p.get("Brsh") else { continue };
        let name = p.get("Nm  ").or(tip.get("Nm  ")).and_then(Value::text).unwrap_or("").to_string();
        let name = if name.is_empty() { format!("{}", out.len() + 1) } else { name };
        let mut b = ImportedBrush::new(&name, num(tip, "Dmtr").unwrap_or(40.0).clamp(1.0, 5000.0) as f32);
        match tip.class() {
            Some("sampledBrush") => {
                let id = tip.get("sampledData").and_then(Value::text).unwrap_or("");
                let Some((_, _, st)) = images.iter().find(|(_, k, _)| k == id) else { continue };
                b.tip = Some(st.clone());
            }
            Some("computedBrush") => b.hardness = (num(tip, "Hrdn").unwrap_or(100.0) / 100.0).clamp(0.0, 1.0) as f32,
            _ => continue,
        }
        // шаг — у формы кончика (у пресета бывает свой, Photoshop его не показывает); выключен —
        // отпечатки по скорости, как авто
        let spacing = num(tip, "Spcn").or(num(p, "Spcn")).unwrap_or(25.0);
        b.spacing = (on(tip, "Intr") || tip.get("Intr").is_none()).then_some((spacing / 100.0).clamp(0.01, 10.0) as f32);
        b.tip_angle = num(tip, "Angl").unwrap_or(0.0) as f32;
        b.tip_ratio = (num(tip, "Rndn").unwrap_or(100.0) / 100.0).clamp(0.01, 1.0) as f32;
        if on(tip, "flipX") || on(tip, "flipY") || on(p, "flipX") || on(p, "flipY") {
            b.skipped.push("отражение кончика");
        }
        let mut sc = paint_core::scatter::ScatterParams::default();
        let mut scatter = false;
        if on(p, "useTipDynamics") {
            let (size_j, size_c) = var(p, "szVr");
            let min = (num(p, "minimumDiameter").unwrap_or(0.0) / 100.0).clamp(0.0, 1.0) as f32;
            if size_c == 2 && min < 0.999 {
                b.size_pressure = Some(min);
            }
            let (angle_j, angle_c) = var(p, "angleDynamics");
            b.tip_follow = angle_c == 6 || angle_c == 7;
            let (ratio_j, _) = var(p, "roundnessDynamics");
            sc.size_jitter = size_j;
            sc.angle_jitter = angle_j;
            sc.ratio_jitter = ratio_j * (1.0 - (num(p, "minimumRoundness").unwrap_or(0.0) / 100.0).clamp(0.0, 1.0) as f32);
            scatter |= size_j > 0.0 || angle_j > 0.0 || sc.ratio_jitter > 0.0;
        }
        if on(p, "useScatter") {
            // россыпь в Photoshop — разброс «scatterDynamics», до 1000 % диаметра; у «Россыпи» —
            // до 200 %
            let amount = p.get("scatterDynamics").and_then(|v| num(v, "jitter")).or(num(p, "Scat")).unwrap_or(0.0);
            sc.scattering = (amount / 100.0).clamp(0.0, 2.0) as f32;
            sc.all_directions = on(p, "bothAxes");
            sc.count = num(p, "Cnt ").unwrap_or(1.0).round().clamp(1.0, 20.0) as u8;
            sc.count_jitter = var(p, "countDynamics").0;
            scatter |= sc.scattering > 0.0 || sc.count > 1 || sc.count_jitter > 0.0;
        }
        if on(p, "usePaintDynamics") {
            if var(p, "opVr").1 == 2 || var(p, "prVr").1 == 2 {
                b.density_pressure = Some(0.0);
            }
            if var(p, "opVr").0 > 0.0 || var(p, "prVr").0 > 0.0 {
                b.skipped.push("разброс непрозрачности");
            }
        }
        if on(p, "useColorDynamics") {
            sc.fg_bg_jitter = var(p, "clVr").0;
            let pct = |k: &str| (num(p, k).unwrap_or(0.0) / 100.0).clamp(0.0, 1.0) as f32;
            sc.hue_jitter = pct("H   ");
            sc.sat_jitter = pct("Strt");
            sc.bri_jitter = pct("Brgh");
            sc.each_shape = on(p, "colorDynamicsPerTip") || p.get("colorDynamicsPerTip").is_none();
            scatter |= sc.fg_bg_jitter > 0.0 || sc.hue_jitter > 0.0 || sc.sat_jitter > 0.0 || sc.bri_jitter > 0.0;
        }
        if scatter {
            // угол и толщина кончика — у самого отпечатка; формы поворачиваются по мазку, только
            // если так задано
            sc.angle_control = if b.tip_follow { paint_core::scatter::AngleControl::Auto } else { paint_core::scatter::AngleControl::None };
            b.scatter = Some(sc);
        }
        if on(p, "useTexture") {
            b.skipped.push("текстура");
        }
        if p.get("dualBrush").is_some_and(|v| on(v, "useDualBrush")) {
            b.skipped.push("двойная кисть");
        }
        if on(p, "Wtdg") {
            b.skipped.push("кайма акварели");
        }
        if on(p, "Nose") {
            b.skipped.push("шум");
        }
        if let Some(o) = p.get("toolOptions") {
            b.density = ((num(o, "Opct").unwrap_or(100.0) / 100.0) * (num(o, "flow").unwrap_or(100.0) / 100.0)).clamp(0.01, 1.0) as f32;
        }
        out.push(b);
    }
    // в `desc` нет пресетов — картинки с обычными настройками
    if out.is_empty() {
        out = images.into_iter().map(|(n, _, st)| ImportedBrush { spacing: Some(0.1), ..plain(n, st) }).collect();
    }
    if out.is_empty() {
        return Err("ABR: в файле нет кистей".into());
    }
    Ok(out)
}

/// Набор отпечатков GIMP: строка с именем, строка с числом клеток и правилами выбора, затем
/// клетки — обычные `.gbr` подряд. Берётся первая клетка (смены отпечатков по ходу мазка нет).
pub fn gih(d: &[u8]) -> Res<(String, Stamp)> {
    let mut lines = d.splitn(3, |&b| b == b'\n');
    let name = String::from_utf8_lossy(lines.next().ok_or("GIH: пусто")?).trim().to_string();
    lines.next().ok_or("GIH: нет строки параметров")?;
    let rest = lines.next().ok_or("GIH: нет клеток")?;
    let (_, stamp) = gbr(rest)?;
    Ok((name, stamp))
}

/// Уменьшить, чтобы большая сторона была не больше `max` (усреднение по клеткам).
pub fn shrink(s: Stamp, max: usize) -> Stamp {
    let side = s.width.max(s.height);
    if side <= max {
        return s;
    }
    let (w, h) = ((s.width * max).div_ceil(side).max(1), (s.height * max).div_ceil(side).max(1));
    let mut alpha = vec![0u8; w * h];
    for y in 0..h {
        let (y0, y1) = (y * s.height / h, ((y + 1) * s.height / h).max(y * s.height / h + 1));
        for x in 0..w {
            let (x0, x1) = (x * s.width / w, ((x + 1) * s.width / w).max(x * s.width / w + 1));
            let mut sum = 0u32;
            for yy in y0..y1 {
                for xx in x0..x1 {
                    sum += s.alpha[yy * s.width + xx] as u32;
                }
            }
            alpha[y * w + x] = (sum / ((y1 - y0) * (x1 - x0)) as u32) as u8;
        }
    }
    Stamp { width: w, height: h, alpha }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_gimp_brush() {
        let name = b"Dot\0";
        let header = 28 + name.len();
        let mut d = Vec::new();
        for v in [header as u32, 2, 3, 2, 1] {
            d.extend(v.to_be_bytes());
        }
        d.extend(b"GIMP");
        d.extend(25u32.to_be_bytes());
        d.extend(name);
        d.extend([0, 128, 255, 10, 20, 30]);
        let (n, s) = gbr(&d).unwrap();
        assert_eq!(n, "Dot");
        assert_eq!((s.width, s.height, s.alpha), (3, 2, vec![0, 128, 255, 10, 20, 30]));
    }

    /// ABR версии 2: одна «вычисляемая» кисть (пропускается) и одна с картинкой 3×2, PackBits.
    #[test]
    fn reads_photoshop_brushes_v2() {
        let mut d = Vec::new();
        d.extend(2u16.to_be_bytes());
        d.extend(2u16.to_be_bytes());
        // вычисляемая: тип 1 и 4 байта данных
        d.extend(1u16.to_be_bytes());
        d.extend(4u32.to_be_bytes());
        d.extend([0; 4]);
        let mut body = vec![0u8; 6];
        let name: Vec<u16> = "Мел".encode_utf16().chain([0]).collect();
        body.extend((name.len() as u32).to_be_bytes());
        for u in name {
            body.extend(u.to_be_bytes());
        }
        body.extend([0u8; 9]);
        for v in [0i32, 0, 2, 3] {
            body.extend(v.to_be_bytes());
        }
        body.extend(8u16.to_be_bytes());
        body.push(1);
        // строки PackBits: «три раза 255» и «0, 128, 9»
        body.extend(2u16.to_be_bytes());
        body.extend(4u16.to_be_bytes());
        body.extend([0xFE, 255]);
        body.extend([2, 0, 128, 9]);
        d.extend(2u16.to_be_bytes());
        d.extend((body.len() as u32).to_be_bytes());
        d.extend(body);
        let v = abr(&d).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].0, "Мел");
        assert_eq!((v[0].1.width, v[0].1.height, v[0].1.alpha.clone()), (3, 2, vec![255, 255, 255, 0, 128, 9]));
        assert!(abr(&[0, 6, 0, 2]).is_err(), "пустой файл версии 6");
    }

    #[test]
    fn reads_photoshop_brush_settings() {
        fn key(out: &mut Vec<u8>, k: &str) {
            out.extend((if k.len() == 4 { 0 } else { k.len() as u32 }).to_be_bytes());
            out.extend(k.as_bytes());
        }
        fn obj(out: &mut Vec<u8>, class: &str, n: u32) {
            out.extend([0, 0, 0, 1, 0, 0]);
            key(out, class);
            out.extend(n.to_be_bytes());
        }
        fn unit(out: &mut Vec<u8>, k: &str, u: &str, v: f64) {
            key(out, k);
            out.extend(b"UntF");
            out.extend(u.as_bytes());
            out.extend(v.to_be_bytes());
        }
        fn flag(out: &mut Vec<u8>, k: &str, v: bool) {
            key(out, k);
            out.extend(b"bool");
            out.push(v as u8);
        }
        fn var(out: &mut Vec<u8>, k: &str, control: i32, jitter: f64) {
            key(out, k);
            out.extend(b"Objc");
            obj(out, "brVr", 2);
            key(out, "bVTy");
            out.extend(b"long");
            out.extend(control.to_be_bytes());
            unit(out, "jitter", "#Prc", jitter);
        }
        let uuid = "fe8cf204-72e3-11e8-99ab-ca24cbfea154";
        // картинка 2×2 без сжатия
        let mut item = vec![36u8];
        item.extend(uuid.as_bytes());
        item.extend([0u8; 264]);
        for v in [0i32, 0, 2, 2] {
            item.extend(v.to_be_bytes());
        }
        item.extend(8u16.to_be_bytes());
        item.push(0);
        item.extend([255, 128, 64, 0]);
        let mut samp = (item.len() as u32).to_be_bytes().to_vec();
        samp.extend(&item);
        while !samp.len().is_multiple_of(4) {
            samp.push(0);
        }
        // описание: один пресет с россыпью и нажимом на размер
        let mut desc = 16u32.to_be_bytes().to_vec();
        obj(&mut desc, "null", 1);
        key(&mut desc, "Brsh");
        desc.extend(b"VlLs");
        desc.extend(1u32.to_be_bytes());
        desc.extend(b"Objc");
        obj(&mut desc, "brushPreset", 8);
        key(&mut desc, "Nm  ");
        desc.extend(b"TEXT");
        desc.extend(4u32.to_be_bytes());
        desc.extend([0, b'M', 0, b'a', 0, b'r', 0, 0]);
        key(&mut desc, "Brsh");
        desc.extend(b"Objc");
        obj(&mut desc, "sampledBrush", 4);
        unit(&mut desc, "Dmtr", "#Pxl", 30.0);
        unit(&mut desc, "Spcn", "#Prc", 50.0);
        unit(&mut desc, "Rndn", "#Prc", 40.0);
        key(&mut desc, "sampledData");
        desc.extend(b"TEXT");
        desc.extend((uuid.len() as u32 + 1).to_be_bytes());
        for c in uuid.encode_utf16().chain([0]) {
            desc.extend(c.to_be_bytes());
        }
        flag(&mut desc, "useTipDynamics", true);
        var(&mut desc, "szVr", 2, 20.0);
        unit(&mut desc, "minimumDiameter", "#Prc", 30.0);
        flag(&mut desc, "useScatter", true);
        var(&mut desc, "scatterDynamics", 0, 150.0);
        key(&mut desc, "Cnt ");
        desc.extend(b"doub");
        desc.extend(3.0f64.to_be_bytes());
        let mut d = vec![0, 6, 0, 2];
        for (k, body) in [(b"samp", &samp), (b"desc", &desc)] {
            d.extend(b"8BIM");
            d.extend(k);
            d.extend((body.len() as u32).to_be_bytes());
            d.extend(body.iter());
        }
        let v = abr_brushes(&d).unwrap();
        assert_eq!(v.len(), 1);
        let b = &v[0];
        assert_eq!(b.name, "Mar");
        assert_eq!(b.size, 30.0);
        assert_eq!(b.spacing, Some(0.5));
        assert!((b.tip_ratio - 0.4).abs() < 1e-6);
        assert_eq!(b.size_pressure, Some(0.3));
        assert_eq!(b.tip.as_ref().map(|t| t.alpha.clone()), Some(vec![255, 128, 64, 0]));
        let sc = b.scatter.as_ref().unwrap();
        assert_eq!((sc.scattering, sc.count), (1.5, 3));
        assert!((sc.size_jitter - 0.2).abs() < 1e-6);
        // обрезанный файл — ошибка, не паника
        for n in 0..d.len() {
            let _ = abr_brushes(&d[..n]);
        }
    }

    #[test]
    fn reads_svg_tip() {
        let d = br#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="50"><rect x="0" y="0" width="50" height="50" fill="black"/></svg>"#;
        let st = svg(d).unwrap();
        assert_eq!((st.width, st.height), (MAX_SIDE, MAX_SIDE / 2));
        // левая половина — краска, правая — нет
        assert_eq!(st.alpha[st.width / 4], 255);
        assert_eq!(st.alpha[st.width * 3 / 4], 0);
        assert!(svg(b"<svg").is_err());
    }

    #[test]
    fn large_tips_are_shrunk() {
        let s = shrink(Stamp { width: 1024, height: 512, alpha: vec![200; 1024 * 512] }, 512);
        assert_eq!((s.width, s.height), (512, 256));
        assert!(s.alpha.iter().all(|&a| a == 200));
    }

    /// Проверка на настоящих кистях: `BRUSH_DIR=папка cargo test -p paint-formats -- --ignored`.
    #[test]
    #[ignore]
    fn reads_local_brushes() {
        let Some(dir) = std::env::var_os("BRUSH_DIR") else { return };
        let mut stack = vec![std::path::PathBuf::from(dir)];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|x| x == "abr") {
                    if let Ok(v) = abr_brushes(&std::fs::read(&p).unwrap()) {
                        for b in v.iter().take(40) {
                            println!(
                                "  «{}» {:.0}px шаг {:?} жёсткость {:.2} угол {:.0} Ш:В {:.2} нажим {:?}/{:?} кончик {} россыпь {:?} пропущено {:?}",
                                b.name, b.size, b.spacing, b.hardness, b.tip_angle, b.tip_ratio, b.size_pressure, b.density_pressure, b.tip.is_some(),
                                b.scatter.as_ref().map(|s| (s.scattering, s.count, s.size_jitter, s.angle_jitter)), b.skipped
                            );
                        }
                    }
                    match abr(&std::fs::read(&p).unwrap()) {
                        Ok(v) => {
                            println!("OK {} кистей {} первая {}x{}", p.display(), v.len(), v[0].1.width, v[0].1.height);
                            if let Some(out) = std::env::var_os("ABR_PNG") {
                                // лист из первых восьми отпечатков по 128 px
                                let (cell, n) = (128usize, v.len().min(8));
                                let mut img = vec![255u8; cell * n * cell * 4];
                                for (k, (_, st)) in v.iter().take(n).enumerate() {
                                    let sc = st.width.max(st.height) as f32 / cell as f32;
                                    for y in 0..cell {
                                        for x in 0..cell {
                                            let (sx, sy) = ((x as f32 * sc) as usize, (y as f32 * sc) as usize);
                                            let a = if sx < st.width && sy < st.height { st.alpha[sy * st.width + sx] } else { 0 };
                                            let o = (y * cell * n + k * cell + x) * 4;
                                            img[o..o + 3].fill(255 - a);
                                        }
                                    }
                                }
                                paint_core::io::save_image(std::path::Path::new(&out), (cell * n) as u32, cell as u32, &img).unwrap();
                            }
                        }
                        Err(e) => println!("ERR {} {e}", p.display()),
                    }
                } else if p.extension().is_some_and(|x| x == "gbr" || x == "gih" || x == "svg") {
                    match load_all(&p) {
                        Ok((n, c, r)) => println!("OK {} «{n}» {}x{} клеток {} вразнобой {r}", p.display(), c[0].width, c[0].height, c.len()),
                        Err(e) => println!("ERR {} {e}", p.display()),
                    }
                }
            }
        }
    }
}
