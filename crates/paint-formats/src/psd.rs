//! Photoshop PSD (версия 1, RGB, 8 бит на канал): запись и чтение слоёв.
//!
//! Пишутся: слои с именами (`luni` — юникод), непрозрачность, видимость, прикрепление, режимы
//! наложения, папки (`lsct`), сведённая картинка. Каналы сжаты PackBits построчно.
//! Читаются те же сведения, а ещё 16 бит, оттенки серого, сжатие ZIP и маски слоя; если слоёв
//! нет — сведённая картинка становится одним слоем.

use paint_core::blend::BlendMode;
use paint_core::pixel::to_rgba8;
use paint_core::tile::ColorMap;
use paint_core::{Document, Layer};
use std::io::Write;
use std::path::Path;

type Res<T> = Result<T, Box<dyn std::error::Error>>;

fn psd_key(mode: BlendMode) -> &'static [u8; 4] {
    use BlendMode::*;
    match mode {
        Normal => b"norm",
        Multiply => b"mul ",
        Screen => b"scrn",
        Overlay => b"over",
        Darken => b"dark",
        Lighten => b"lite",
        ColorDodge | Dodge => b"div ",
        ColorBurn | Burn => b"idiv",
        LinearDodge | Shine => b"lddg",
        LinearBurn | Shade => b"lbrn",
        HardLight => b"hLit",
        SoftLight => b"sLit",
        Difference => b"diff",
        Exclusion => b"smud",
        Subtract => b"fsub",
        Divide => b"fdiv",
        DarkerColor => b"dkCl",
        LighterColor => b"lgCl",
        VividLight | BurnDodge => b"vLit",
        LinearLight | ShadeShine => b"lLit",
        PinLight => b"pLit",
        HardMix => b"hMix",
    }
}

fn mode_of(key: &[u8]) -> BlendMode {
    use BlendMode::*;
    match key {
        b"mul " => Multiply,
        b"scrn" => Screen,
        b"over" => Overlay,
        b"dark" => Darken,
        b"lite" => Lighten,
        b"div " => ColorDodge,
        b"idiv" => ColorBurn,
        b"lddg" => LinearDodge,
        b"lbrn" => LinearBurn,
        b"hLit" => HardLight,
        b"sLit" => SoftLight,
        b"diff" => Difference,
        b"smud" => Exclusion,
        b"fsub" => Subtract,
        b"fdiv" => Divide,
        b"dkCl" => DarkerColor,
        b"lgCl" => LighterColor,
        b"vLit" => VividLight,
        b"lLit" => LinearLight,
        b"pLit" => PinLight,
        b"hMix" => HardMix,
        _ => Normal,
    }
}

/// PackBits одной строки.
fn packbits(row: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < row.len() {
        // повтор
        let mut run = 1;
        while i + run < row.len() && run < 128 && row[i + run] == row[i] {
            run += 1;
        }
        if run >= 2 {
            out.push((1i32 - run as i32) as i8 as u8);
            out.push(row[i]);
            i += run;
            continue;
        }
        // буквально, пока не начнётся повтор
        let start = i;
        while i < row.len() && i - start < 128 {
            if i + 1 < row.len() && row[i] == row[i + 1] && (i + 2 >= row.len() || row[i + 1] == row[i + 2]) {
                break;
            }
            i += 1;
        }
        out.push((i - start - 1) as u8);
        out.extend_from_slice(&row[start..i]);
    }
    out
}

fn unpackbits(data: &[u8], len: usize) -> Res<Vec<u8>> {
    let mut out = Vec::with_capacity(len);
    let mut i = 0;
    while out.len() < len && i < data.len() {
        let n = data[i] as i8;
        i += 1;
        if n >= 0 {
            let k = n as usize + 1;
            out.extend_from_slice(data.get(i..i + k).ok_or("PSD: обрыв PackBits")?);
            i += k;
        } else if n != -128 {
            let k = (1 - n as i32) as usize;
            let v = *data.get(i).ok_or("PSD: обрыв PackBits")?;
            out.extend(std::iter::repeat_n(v, k));
            i += 1;
        }
    }
    out.resize(len, 0);
    Ok(out)
}

/// Каналы прямоугольника (RGBA по 8 бит) в виде плоскостей, сжатые PackBits: [A, R, G, B].
fn encode_planes(rgba: &[u8], w: usize, h: usize, order: &[usize]) -> Vec<Vec<u8>> {
    order
        .iter()
        .map(|&c| {
            let rows: Vec<Vec<u8>> = (0..h).map(|y| packbits(&(0..w).map(|x| rgba[(y * w + x) * 4 + c]).collect::<Vec<_>>())).collect();
            let mut out = Vec::new();
            out.extend(1u16.to_be_bytes());
            for r in &rows {
                out.extend((r.len() as u16).to_be_bytes());
            }
            for r in rows {
                out.extend(r);
            }
            out
        })
        .collect()
}

/// Пиксели слоя в рамке содержимого: (лево, верх, ширина, высота, RGBA прямой).
fn layer_rgba(px: &ColorMap) -> (i32, i32, usize, usize, Vec<u8>) {
    let Some((x0, y0, x1, y1)) = paint_core::selection::content_bounds(px) else { return (0, 0, 0, 0, Vec::new()) };
    let (w, h) = ((x1 - x0) as usize, (y1 - y0) as usize);
    let mut out = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            out[(y * w + x) * 4..][..4].copy_from_slice(&to_rgba8(px.get(x0 + x as i32, y0 + y as i32)));
        }
    }
    (x0, y0, w, h, out)
}

struct Record {
    rect: [i32; 4],
    channels: Vec<(i16, Vec<u8>)>,
    key: [u8; 4],
    opacity: u8,
    clipping: bool,
    visible: bool,
    name: String,
    /// `lsct`: 0 — обычный слой, 1 — открытая папка, 2 — закрытая, 3 — граница папки.
    section: u32,
}

fn records(layers: &[Layer], out: &mut Vec<Record>) {
    for l in layers {
        let base = |rect, channels, section| Record {
            rect,
            channels,
            key: *psd_key(l.mode),
            opacity: (l.opacity.clamp(0.0, 1.0) * 255.0).round() as u8,
            clipping: l.clipping,
            visible: l.visible,
            name: l.name.clone(),
            section,
        };
        let empty = || [-1i16, 0, 1, 2].iter().map(|&id| (id, 0u16.to_be_bytes().to_vec())).collect::<Vec<_>>();
        match &l.children {
            Some(ch) => {
                out.push(Record { name: "</Layer group>".into(), key: *b"norm", opacity: 255, clipping: false, visible: true, ..base([0; 4], empty(), 3) });
                records(ch, out);
                let mut folder = base([0; 4], empty(), 1);
                if l.pass_through {
                    folder.key = *b"pass";
                }
                out.push(folder);
            }
            None => {
                let (x, y, w, h, rgba) = layer_rgba(&l.pixels);
                let planes = encode_planes(&rgba, w, h, &[3, 0, 1, 2]);
                let channels = [-1i16, 0, 1, 2].into_iter().zip(planes).collect();
                out.push(base([y, x, y + h as i32, x + w as i32], channels, 0));
            }
        }
    }
}

fn pascal(name: &str) -> Vec<u8> {
    let bytes: Vec<u8> = name.chars().map(|c| if c.is_ascii() { c as u8 } else { b'?' }).take(255).collect();
    let mut out = vec![bytes.len() as u8];
    out.extend(bytes);
    while out.len() % 4 != 0 {
        out.push(0);
    }
    out
}

fn block(key: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = b"8BIM".to_vec();
    out.extend(key);
    let mut d = data.to_vec();
    if !d.len().is_multiple_of(2) {
        d.push(0);
    }
    out.extend((d.len() as u32).to_be_bytes());
    out.extend(d);
    out
}

/// Сохранить документ в PSD.
pub fn save(doc: &Document, path: &Path) -> Res<()> {
    let (w, h) = (doc.width as usize, doc.height as usize);
    let mut recs = Vec::new();
    records(&doc.layers, &mut recs);
    // запись слоёв
    let mut info = Vec::new();
    info.extend((recs.len() as i16).to_be_bytes());
    for r in &recs {
        for v in r.rect {
            info.extend(v.to_be_bytes());
        }
        info.extend((r.channels.len() as u16).to_be_bytes());
        for (id, data) in &r.channels {
            info.extend(id.to_be_bytes());
            info.extend((data.len() as u32).to_be_bytes());
        }
        info.extend(b"8BIM");
        info.extend(r.key);
        info.push(r.opacity);
        info.push(r.clipping as u8);
        info.push(if r.visible { 0 } else { 2 });
        info.push(0);
        let mut extra = Vec::new();
        extra.extend(0u32.to_be_bytes()); // маска
        extra.extend(0u32.to_be_bytes()); // диапазоны наложения
        extra.extend(pascal(&r.name));
        let units: Vec<u16> = r.name.encode_utf16().collect();
        let mut luni = (units.len() as u32).to_be_bytes().to_vec();
        for u in units {
            luni.extend(u.to_be_bytes());
        }
        extra.extend(block(b"luni", &luni));
        if r.section != 0 {
            let mut lsct = r.section.to_be_bytes().to_vec();
            if r.section != 3 {
                lsct.extend(b"8BIM");
                lsct.extend(r.key);
            }
            extra.extend(block(b"lsct", &lsct));
        }
        info.extend((extra.len() as u32).to_be_bytes());
        info.extend(extra);
    }
    for r in &recs {
        for (_, data) in &r.channels {
            info.extend(data);
        }
    }
    if info.len() % 2 != 0 {
        info.push(0);
    }
    let mut out = Vec::new();
    out.extend(b"8BPS");
    out.extend(1u16.to_be_bytes());
    out.extend([0u8; 6]);
    out.extend(4u16.to_be_bytes()); // R, G, B, альфа
    out.extend((h as u32).to_be_bytes());
    out.extend((w as u32).to_be_bytes());
    out.extend(8u16.to_be_bytes());
    out.extend(3u16.to_be_bytes()); // RGB
    out.extend(0u32.to_be_bytes()); // цветовой режим
    out.extend(0u32.to_be_bytes()); // ресурсы
    let mut lm = Vec::new();
    lm.extend((info.len() as u32).to_be_bytes());
    lm.extend(info);
    lm.extend(0u32.to_be_bytes()); // общая маска
    out.extend((lm.len() as u32).to_be_bytes());
    out.extend(lm);
    // сведённая картинка: R, G, B, A — заголовок со всеми длинами строк, затем строки
    let flat = doc.flatten();
    let planes = encode_planes(&flat, w, h, &[0, 1, 2, 3]);
    out.extend(1u16.to_be_bytes());
    let mut counts = Vec::new();
    let mut rows = Vec::new();
    for p in &planes {
        counts.extend_from_slice(&p[2..2 + 2 * h]);
        rows.extend_from_slice(&p[2 + 2 * h..]);
    }
    out.extend(counts);
    out.extend(rows);
    let tmp = path.with_extension("psd.tmp");
    std::fs::File::create(&tmp)?.write_all(&out)?;
    std::fs::rename(tmp, path)?;
    Ok(())
}

struct Reader<'a> {
    d: &'a [u8],
    p: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Res<&[u8]> {
        let s = self.d.get(self.p..self.p + n).ok_or("PSD: файл обрывается")?;
        self.p += n;
        Ok(s)
    }
    fn u8(&mut self) -> Res<u8> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Res<u16> {
        Ok(u16::from_be_bytes(self.take(2)?.try_into()?))
    }
    fn u32(&mut self) -> Res<u32> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into()?))
    }
    fn i32(&mut self) -> Res<i32> {
        Ok(i32::from_be_bytes(self.take(4)?.try_into()?))
    }
}

/// Канал: сжатие и данные → `n` байт на строку × `h` строк.
/// Распаковать `rows` строк по `row_bytes` байт: 0 — без сжатия, 1 — PackBits (счётчики строк
/// лежат в `counts`), 2 — ZIP, 3 — ZIP с предсказанием (разность соседних отсчётов в строке).
fn unpack(comp: u16, body: &[u8], counts: &[usize], rows: usize, row_bytes: usize, depth: u16) -> Res<Vec<u8>> {
    let need = rows * row_bytes;
    let mut out = match comp {
        0 => body.get(..need).ok_or("PSD: короткий канал")?.to_vec(),
        1 => {
            let mut pos = 0;
            let mut out = Vec::with_capacity(need);
            for &c in counts {
                out.extend(unpackbits(body.get(pos..pos + c).ok_or("PSD: короткий канал")?, row_bytes)?);
                pos += c;
            }
            out
        }
        2 | 3 => miniz_oxide::inflate::decompress_to_vec_zlib(body).map_err(|e| format!("PSD: ZIP: {e:?}"))?,
        _ => return Err(format!("PSD: неизвестное сжатие {comp}").into()),
    };
    out.resize(need, 0);
    if comp == 3 {
        for row in out.chunks_mut(row_bytes.max(1)) {
            if depth == 16 {
                let mut prev = 0u16;
                for c in row.as_chunks_mut::<2>().0 {
                    prev = prev.wrapping_add(u16::from_be_bytes(*c));
                    *c = prev.to_be_bytes();
                }
            } else {
                for i in 1..row.len() {
                    row[i] = row[i].wrapping_add(row[i - 1]);
                }
            }
        }
    }
    Ok(out)
}

/// Отсчёты канала, приведённые к 16 битам (0..=65535).
fn samples(raw: &[u8], depth: u16) -> Vec<u16> {
    if depth == 16 {
        raw.as_chunks::<2>().0.iter().map(|c| u16::from_be_bytes(*c)).collect()
    } else {
        raw.iter().map(|&v| v as u16 * 257).collect()
    }
}

/// Канал слоя: два байта сжатия и данные.
fn read_channel(data: &[u8], w: usize, h: usize, depth: u16) -> Res<Vec<u16>> {
    if data.len() < 2 {
        return Ok(vec![0; w * h]);
    }
    let comp = u16::from_be_bytes([data[0], data[1]]);
    let mut body = &data[2..];
    let row_bytes = w * depth as usize / 8;
    let mut counts = Vec::new();
    if comp == 1 {
        counts = (0..h).map(|y| body.get(2 * y..2 * y + 2).map(|c| u16::from_be_bytes([c[0], c[1]]) as usize)).collect::<Option<_>>().ok_or("PSD: короткий канал")?;
        body = &body[2 * h..];
    }
    Ok(samples(&unpack(comp, body, &counts, h, row_bytes, depth)?, depth))
}

/// Встроенный профиль цвета (ресурс 1039) линейный — гамма 1 (так Krita по умолчанию пишет
/// 16 бит: «sRGB-elle-V2-g10»). Тогда цвета переводятся в sRGB, иначе берутся как есть.
fn linear_profile(res: &[u8]) -> bool {
    let be32 = |d: &[u8], o: usize| d.get(o..o + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize);
    let mut p = 0;
    while p + 12 <= res.len() && &res[p..p + 4] == b"8BIM" {
        let id = u16::from_be_bytes([res[p + 4], res[p + 5]]);
        let nl = res[p + 6] as usize;
        let q = p + 6 + (nl + 2) / 2 * 2; // имя: байт длины и текст, до чётного
        let Some(size) = be32(res, q) else { return false };
        let data = res.get(q + 4..q + 4 + size).unwrap_or(&[]);
        if id == 1039 {
            let Some(count) = be32(data, 128) else { return false };
            for t in 0..count.min(64) {
                let e = 132 + t * 12;
                if data.get(e..e + 4) != Some(b"rTRC") {
                    continue;
                }
                let Some(off) = be32(data, e + 4) else { return false };
                let tag = data.get(off..).unwrap_or(&[]);
                return match tag.get(..4) {
                    Some(b"curv") => match be32(tag, 8) {
                        Some(0) => true,
                        Some(1) => tag.get(12..14) == Some(&[1, 0]),
                        _ => false,
                    },
                    Some(b"para") => be32(tag, 12).is_some_and(|g| (g as i64 - 0x10000).abs() < 0x100) && tag.get(8..10) == Some(&[0, 0]),
                    _ => false,
                };
            }
            return false;
        }
        p = q + 4 + size.div_ceil(2) * 2;
    }
    false
}

/// Цвета из линейных в sRGB (альфа как есть).
fn encode(c: [u16; 4], linear: bool) -> [u16; 4] {
    if !linear {
        return c;
    }
    let f = |v: u16| -> u16 {
        let l = v as f64 / 65535.0;
        let s = if l <= 0.003_130_8 { l * 12.92 } else { 1.055 * l.powf(1.0 / 2.4) - 0.055 };
        (s * 65535.0).round() as u16
    };
    [f(c[0]), f(c[1]), f(c[2]), c[3]]
}

/// Пиксель из 16-битных RGBA без предумножения.
fn from_rgba16(c: [u16; 4]) -> paint_core::pixel::Px {
    let one = paint_core::pixel::ONE as u64;
    let a = (c[3] as u64 * one + 32767) / 65535;
    let f = |v: u16| -> u16 { (((v as u64 * one + 32767) / 65535 * a + one / 2) / one) as u16 };
    [f(c[0]), f(c[1]), f(c[2]), a as u16]
}

/// Открыть PSD: RGB или оттенки серого, 8 или 16 бит; слои, папки, маски слоя.
pub fn load(path: &Path) -> Res<Document> {
    let d = std::fs::read(path)?;
    let mut r = Reader { d: &d, p: 0 };
    if r.take(4)? != b"8BPS" || r.u16()? != 1 {
        return Err("не PSD версии 1".into());
    }
    r.take(6)?;
    let channels = r.u16()? as usize;
    let h = r.u32()? as usize;
    let w = r.u32()? as usize;
    let depth = r.u16()?;
    let mode = r.u16()?;
    if !matches!(depth, 8 | 16) || !matches!(mode, 1 | 3) {
        return Err(format!("PSD: поддерживаются RGB и оттенки серого по 8 и 16 бит (глубина {depth}, режим {mode})").into());
    }
    let gray = mode == 1;
    // номера каналов цвета: у серого один канал на все три
    let color_ids: [i16; 3] = if gray { [0, 0, 0] } else { [0, 1, 2] };
    let n = r.u32()? as usize;
    r.take(n)?;
    let n = r.u32()? as usize;
    let linear = linear_profile(r.take(n)?);
    let lm_len = r.u32()? as usize;
    let lm_end = r.p + lm_len;
    let mut doc = Document::new(w as u32, h as u32);
    doc.layers.clear();
    // стопка открытых папок: (слои внутри), снизу вверх
    let mut stack: Vec<Vec<Layer>> = vec![Vec::new()];
    if lm_len > 0 {
        let info_len = r.u32()? as usize;
        if info_len > 0 {
            let count = (r.u16()? as i16).unsigned_abs() as usize;
            struct Rec {
                rect: [i32; 4],
                chans: Vec<(i16, usize)>,
                key: [u8; 4],
                opacity: u8,
                clipping: bool,
                visible: bool,
                name: String,
                section: u32,
                /// Маска слоя: прямоугольник, цвет за ним, выключена ли.
                mask: Option<([i32; 4], u8, bool)>,
            }
            let mut recs = Vec::new();
            for _ in 0..count {
                let rect = [r.i32()?, r.i32()?, r.i32()?, r.i32()?];
                let nc = r.u16()? as usize;
                let mut chans = Vec::new();
                for _ in 0..nc {
                    chans.push((r.u16()? as i16, r.u32()? as usize));
                }
                r.take(4)?;
                let key: [u8; 4] = r.take(4)?.try_into()?;
                let opacity = r.u8()?;
                let clipping = r.u8()? != 0;
                let flags = r.u8()?;
                r.u8()?;
                let extra = r.u32()? as usize;
                let end = r.p + extra;
                let ml = r.u32()? as usize;
                let md = r.take(ml)?;
                let mask = (ml >= 18).then(|| {
                    let i = |o: usize| i32::from_be_bytes([md[o], md[o + 1], md[o + 2], md[o + 3]]);
                    ([i(0), i(4), i(8), i(12)], md[16], md[17] & 2 != 0)
                });
                let bl = r.u32()? as usize;
                r.take(bl)?;
                let nl = r.u8()? as usize;
                let mut name = String::from_utf8_lossy(r.take(nl)?).into_owned();
                let pad = (4 - (nl + 1) % 4) % 4;
                r.take(pad)?;
                let mut section = 0;
                while r.p + 12 <= end {
                    let sig = r.take(4)?.to_vec();
                    if sig != b"8BIM" && sig != b"8B64" {
                        break;
                    }
                    let k: [u8; 4] = r.take(4)?.try_into()?;
                    let len = r.u32()? as usize;
                    let data = r.take(len)?.to_vec();
                    if &k == b"luni" && data.len() >= 4 {
                        let n = u32::from_be_bytes(data[..4].try_into()?) as usize;
                        let units: Vec<u16> = data[4..].chunks(2).take(n).map(|c| u16::from_be_bytes([c[0], *c.get(1).unwrap_or(&0)])).collect();
                        // Krita считает в длине и завершающий нуль
                        name = String::from_utf16_lossy(&units).trim_end_matches('\0').to_string();
                    }
                    if (&k == b"lsct" || &k == b"lsdk") && data.len() >= 4 {
                        section = u32::from_be_bytes(data[..4].try_into()?);
                    }
                }
                r.p = end;
                recs.push(Rec { rect, chans, key, opacity, clipping, visible: flags & 2 == 0, name, section, mask });
            }
            for rec in recs {
                let [top, left, bottom, right] = rec.rect;
                let (lw, lh) = ((right - left).max(0) as usize, (bottom - top).max(0) as usize);
                let mut alpha = vec![65535u16; lw * lh];
                let mut color = [vec![0u16; lw * lh], vec![0u16; lw * lh], vec![0u16; lw * lh]];
                let mut user_mask = None;
                for (id, len) in &rec.chans {
                    let data = r.take(*len)?.to_vec();
                    match id {
                        -1 => alpha = read_channel(&data, lw, lh, depth)?,
                        -2 => {
                            if let Some((mr, _, _)) = rec.mask {
                                let (mw, mh) = ((mr[3] - mr[1]).max(0) as usize, (mr[2] - mr[0]).max(0) as usize);
                                user_mask = Some(read_channel(&data, mw, mh, depth)?);
                            }
                        }
                        &c if c >= 0 => {
                            let plane = read_channel(&data, lw, lh, depth)?;
                            for (i, &cid) in color_ids.iter().enumerate() {
                                if cid == c {
                                    color[i] = plane.clone();
                                }
                            }
                        }
                        _ => {}
                    }
                }
                let mut layer = Layer::new(&rec.name);
                layer.opacity = rec.opacity as f32 / 255.0;
                layer.visible = rec.visible;
                layer.clipping = rec.clipping;
                layer.mode = mode_of(&rec.key);
                if let (Some((mr, default, disabled)), Some(m)) = (rec.mask, &user_mask) {
                    if !disabled {
                        layer.mask = Some(paint_core::layer::LayerMask { hide: mask_hide(m, mr, default, w, h), flags: [1, 1, 1, 0] });
                    }
                }
                match rec.section {
                    3 => {
                        stack.push(Vec::new());
                        continue;
                    }
                    1 | 2 => {
                        let children = stack.pop().unwrap_or_default();
                        layer.children = Some(children);
                        layer.pass_through = &rec.key == b"pass";
                        if layer.pass_through {
                            layer.mode = BlendMode::Normal;
                        }
                    }
                    _ => {
                        for y in 0..lh {
                            for x in 0..lw {
                                let i = y * lw + x;
                                let a = alpha[i];
                                if a > 0 {
                                    layer.pixels.set(left + x as i32, top + y as i32, from_rgba16(encode([color[0][i], color[1][i], color[2][i], a], linear)));
                                }
                            }
                        }
                    }
                }
                let depth = stack.len().saturating_sub(1);
                stack[depth].push(layer);
            }
        }
    }
    let mut layers = stack.into_iter().next().unwrap_or_default();
    if layers.is_empty() {
        // слоёв нет — сведённая картинка
        r.p = lm_end;
        let comp = r.u16()?;
        let row_bytes = w * depth as usize / 8;
        let mut counts = Vec::new();
        if comp == 1 {
            counts = (0..channels * h).map(|_| r.u16().map(|v| v as usize)).collect::<Res<_>>()?;
        }
        let body = &d[r.p..];
        let planes: Vec<Vec<u16>> = if comp == 1 {
            let mut pos = 0;
            let mut planes = Vec::new();
            for c in 0..channels.min(4) {
                let cs = &counts[c * h..(c + 1) * h];
                let len: usize = cs.iter().sum();
                planes.push(samples(&unpack(1, body.get(pos..pos + len).ok_or("PSD: короткий канал")?, cs, h, row_bytes, depth)?, depth));
                pos += len;
            }
            planes
        } else {
            let all = samples(&unpack(comp, body, &[], channels * h, row_bytes, depth)?, depth);
            all.chunks(w * h).take(channels.min(4)).map(|c| c.to_vec()).collect()
        };
        let (ci, ai) = if gray { ([0, 0, 0], 1) } else { ([0, 1, 2], 3) };
        let mut layer = Layer::new("Слой 1");
        for i in 0..w * h {
            let a = planes.get(ai).map_or(65535, |p| p[i]);
            if a > 0 {
                layer.pixels.set((i % w) as i32, (i / w) as i32, from_rgba16(encode([planes[ci[0]][i], planes[ci[1]][i], planes[ci[2]][i], a], linear)));
            }
        }
        layers.push(layer);
    }
    doc.layers = layers;
    Ok(doc)
}

/// Маска слоя PSD (255 — видно) в маску GrazySAI (0x4000 — скрыто); за прямоугольником маски —
/// цвет `default` по всему холсту `w`×`h`.
fn mask_hide(m: &[u16], rect: [i32; 4], default: u8, w: usize, h: usize) -> paint_core::tile::MaskMap {
    let [top, left, bottom, right] = rect;
    let hide = |v: u16| ((65535 - v as u32) * 0x4000 / 65535) as u16;
    let mut out = paint_core::tile::MaskMap::new();
    let mw = (right - left).max(0) as usize;
    let outside = hide(default as u16 * 257);
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let inside = x >= left && x < right && y >= top && y < bottom;
            let v = if inside { hide(m[(y - top) as usize * mw + (x - left) as usize]) } else { outside };
            if v > 0 {
                out.set(x, y, v);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zip_with_prediction_16_bit() {
        // как пишет Photoshop: в строке разности соседних отсчётов, затем zlib
        let rows: Vec<Vec<u16>> = (0..3).map(|y| (0..5).map(|x| (x * 9000 + y * 77) as u16).collect()).collect();
        let mut packed = Vec::new();
        for row in &rows {
            let mut prev = 0u16;
            for &v in row {
                packed.extend(v.wrapping_sub(prev).to_be_bytes());
                prev = v;
            }
        }
        let z = miniz_oxide::deflate::compress_to_vec_zlib(&packed, 6);
        let got = samples(&unpack(3, &z, &[], 3, 10, 16).unwrap(), 16);
        assert_eq!(got, rows.concat());
    }

    #[test]
    fn packbits_round_trip() {
        let row: Vec<u8> = (0..300).map(|i| if i < 100 { 7 } else { (i * 13 % 251) as u8 }).collect();
        assert_eq!(unpackbits(&packbits(&row), row.len()).unwrap(), row);
    }
}
