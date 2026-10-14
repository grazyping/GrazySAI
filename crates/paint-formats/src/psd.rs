//! Photoshop PSD (версия 1, RGB, 8 бит на канал): запись и чтение слоёв.
//!
//! Пишутся: слои с именами (`luni` — юникод), непрозрачность, видимость, прикрепление, режимы
//! наложения, папки (`lsct`), сведённая картинка. Каналы сжаты PackBits построчно.
//! Читаются те же сведения; если слоёв нет — сведённая картинка становится одним слоем.

use paint_core::blend::BlendMode;
use paint_core::pixel::{from_rgba8, to_rgba8};
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
fn read_channel(data: &[u8], w: usize, h: usize) -> Res<Vec<u8>> {
    if data.len() < 2 {
        return Ok(vec![0; w * h]);
    }
    let comp = u16::from_be_bytes([data[0], data[1]]);
    let body = &data[2..];
    match comp {
        0 => {
            let mut v = body.get(..w * h).ok_or("PSD: короткий канал")?.to_vec();
            v.resize(w * h, 0);
            Ok(v)
        }
        1 => {
            let counts: Vec<usize> = (0..h).map(|y| u16::from_be_bytes([body[2 * y], body[2 * y + 1]]) as usize).collect();
            let mut pos = 2 * h;
            let mut out = Vec::with_capacity(w * h);
            for c in counts {
                out.extend(unpackbits(body.get(pos..pos + c).ok_or("PSD: короткий канал")?, w)?);
                pos += c;
            }
            Ok(out)
        }
        _ => Err("PSD: сжатие ZIP не поддерживается".into()),
    }
}

/// Открыть PSD (RGB, 8 бит).
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
    if depth != 8 || mode != 3 {
        return Err(format!("PSD: поддерживается только RGB по 8 бит (глубина {depth}, режим {mode})").into());
    }
    let n = r.u32()? as usize;
    r.take(n)?;
    let n = r.u32()? as usize;
    r.take(n)?;
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
                r.take(ml)?;
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
                        name = String::from_utf16_lossy(&units);
                    }
                    if (&k == b"lsct" || &k == b"lsdk") && data.len() >= 4 {
                        section = u32::from_be_bytes(data[..4].try_into()?);
                    }
                }
                r.p = end;
                recs.push(Rec { rect, chans, key, opacity, clipping, visible: flags & 2 == 0, name, section });
            }
            for rec in recs {
                let [top, left, bottom, right] = rec.rect;
                let (lw, lh) = ((right - left).max(0) as usize, (bottom - top).max(0) as usize);
                let mut planes = [vec![255u8; lw * lh], vec![0u8; lw * lh], vec![0u8; lw * lh], vec![0u8; lw * lh]];
                for (id, len) in &rec.chans {
                    let data = r.take(*len)?.to_vec();
                    let idx = match id {
                        -1 => 0,
                        0 => 1,
                        1 => 2,
                        2 => 3,
                        _ => continue,
                    };
                    planes[idx] = read_channel(&data, lw, lh)?;
                }
                let mut layer = Layer::new(&rec.name);
                layer.opacity = rec.opacity as f32 / 255.0;
                layer.visible = rec.visible;
                layer.clipping = rec.clipping;
                layer.mode = mode_of(&rec.key);
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
                                let a = planes[0][i];
                                if a > 0 {
                                    layer.pixels.set(left + x as i32, top + y as i32, from_rgba8([planes[1][i], planes[2][i], planes[3][i], a]));
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
        let mut planes = Vec::new();
        if comp == 1 {
            let counts: Vec<usize> = (0..channels * h).map(|_| r.u16().map(|v| v as usize)).collect::<Res<_>>()?;
            for c in 0..channels.min(4) {
                let mut plane = Vec::with_capacity(w * h);
                for y in 0..h {
                    plane.extend(unpackbits(r.take(counts[c * h + y])?, w)?);
                }
                planes.push(plane);
            }
        } else if comp == 0 {
            for _ in 0..channels.min(4) {
                planes.push(r.take(w * h)?.to_vec());
            }
        } else {
            return Err("PSD: сжатие ZIP не поддерживается".into());
        }
        let mut layer = Layer::new("Слой 1");
        for i in 0..w * h {
            let a = planes.get(3).map_or(255, |p| p[i]);
            if a > 0 {
                layer.pixels.set((i % w) as i32, (i / w) as i32, from_rgba8([planes[0][i], planes[1][i], planes[2][i], a]));
            }
        }
        layers.push(layer);
    }
    doc.layers = layers;
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packbits_round_trip() {
        let row: Vec<u8> = (0..300).map(|i| if i < 100 { 7 } else { (i * 13 % 251) as u8 }).collect();
        assert_eq!(unpackbits(&packbits(&row), row.len()).unwrap(), row);
    }
}
