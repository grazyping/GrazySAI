//! Чтение и запись картинок.

use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

/// Временный файл рядом с `path` (`имя.расширение.tmp`).
fn tmp_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(".tmp");
    path.with_file_name(name)
}

/// Подменить `path` готовым временным файлом: сначала данные — на диск (`sync_all`), потом
/// переименование. Сбой или пропажа питания посреди записи оставляют прежний файл целым.
pub fn replace_with(tmp: &Path, path: &Path) -> std::io::Result<()> {
    // открыть для записи: в Windows сброс на диск (FlushFileBuffers) без права записи — «Доступ
    // запрещён» (os error 5), и сохранение не проходило (с 0.9.13 до 1.0.22; найдено самопроверкой)
    std::fs::OpenOptions::new().write(true).open(tmp)?.sync_all()?;
    std::fs::rename(tmp, path)?;
    #[cfg(unix)]
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        // и сама запись о переименовании — тоже на диск
        let _ = File::open(dir).and_then(|d| d.sync_all());
    }
    Ok(())
}

/// Записать файл целиком так, что прежний не пострадает при сбое (`replace_with`).
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = tmp_path(path);
    std::fs::write(&tmp, bytes)?;
    replace_with(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

pub fn save_png(path: &Path, width: u32, height: u32, rgba: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let mut enc = png::Encoder::new(BufWriter::new(File::create(path)?), width, height);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()?.write_image_data(rgba)?;
    Ok(())
}

/// Прочитать PNG как RGBA по 8 бит (палитра и серый разворачиваются).
pub fn load_png(path: &Path) -> Result<(u32, u32, Vec<u8>), Box<dyn std::error::Error>> {
    decode_png(&std::fs::read(path)?)
}

/// Буфер под картинку PNG — только если размер правдоподобен: не больше 2^29 точек и не больше,
/// чем могут дать сжатые данные (Deflate — до ~1032:1). Иначе испорченный заголовок требует
/// гигабайты (в Windows это сразу падение программы).
fn png_buffer<R: std::io::Read>(reader: &png::Reader<R>, data_len: usize) -> Res<Vec<u8>> {
    let info = reader.info();
    let need = reader.output_buffer_size();
    if info.width as u64 * info.height as u64 > 1 << 29 || need > data_len.saturating_mul(1100).saturating_add(1 << 16) {
        return Err(format!("PNG: неверный размер {}×{}", info.width, info.height).into());
    }
    Ok(vec![0; need])
}

/// Прочитать JPEG как RGBA по 8 бит.
pub fn load_jpeg(path: &Path) -> Result<(u32, u32, Vec<u8>), Box<dyn std::error::Error>> {
    let data = std::fs::read(path)?;
    let opts = zune_core::options::DecoderOptions::default().jpeg_set_out_colorspace(zune_core::colorspace::ColorSpace::RGBA);
    let mut dec = zune_jpeg::JpegDecoder::new_with_options(&data, opts);
    let pixels = dec.decode()?;
    let (w, h) = dec.dimensions().ok_or("JPEG: нет размеров")?;
    Ok((w as u32, h as u32, pixels))
}

type Image = (u32, u32, Vec<u8>);
type Res<T> = Result<T, Box<dyn std::error::Error>>;

/// Расширения плоских картинок, которые программа открывает и сохраняет.
pub const IMAGE_EXTS: [&str; 9] = ["png", "jpg", "jpeg", "bmp", "tga", "tif", "tiff", "webp", "qoi"];

/// Прочитать картинку по расширению (RGBA по 8 бит без предумножения).
pub fn load_image(path: &Path) -> Res<Image> {
    match path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref() {
        Some("jpg") | Some("jpeg") => load_jpeg(path),
        Some("bmp") => decode_bmp(&std::fs::read(path)?),
        Some("tga") => decode_tga(&std::fs::read(path)?),
        Some("tif") | Some("tiff") => load_tiff(path),
        Some("webp") => load_webp(path),
        Some("qoi") => decode_qoi(&std::fs::read(path)?),
        _ => load_png(path),
    }
}

/// Прочитать картинку из байтов по первым байтам (PNG, JPEG, WebP, BMP, QOI) — для текстур моделей.
pub fn decode_image(data: &[u8]) -> Res<Image> {
    if data.starts_with(b"\x89PNG") {
        decode_png(data)
    } else if data.starts_with(&[0xFF, 0xD8]) {
        let opts = zune_core::options::DecoderOptions::default().jpeg_set_out_colorspace(zune_core::colorspace::ColorSpace::RGBA);
        let mut dec = zune_jpeg::JpegDecoder::new_with_options(data, opts);
        let pixels = dec.decode()?;
        let (w, h) = dec.dimensions().ok_or("JPEG: нет размеров")?;
        Ok((w as u32, h as u32, pixels))
    } else if data.len() > 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        let mut dec = image_webp::WebPDecoder::new(std::io::Cursor::new(data))?;
        let (w, h) = dec.dimensions();
        let need = dec.output_buffer_size().ok_or("WebP: слишком большой")?;
        if w as u64 * h as u64 > 1 << 29 || need > data.len().saturating_mul(1100).saturating_add(1 << 20) {
            return Err(format!("WebP: неверный размер {w}×{h}").into());
        }
        let mut buf = vec![0u8; need];
        dec.read_image(&mut buf)?;
        Ok((w, h, if dec.has_alpha() { buf } else { buf.chunks(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect() }))
    } else if data.starts_with(b"BM") {
        decode_bmp(data)
    } else if data.starts_with(b"qoif") {
        decode_qoi(data)
    } else {
        Err("картинка неизвестного вида".into())
    }
}

/// Сохранить картинку по расширению: PNG, JPEG (92), BMP, TGA, TIFF, WebP (без потерь), QOI.
pub fn save_image(path: &Path, width: u32, height: u32, rgba: &[u8]) -> Res<()> {
    // пишется во временный файл, потом подменяет прежний (сбой не портит старый экспорт)
    let tmp = tmp_path(path);
    let ext = path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase());
    let r = save_image_as(&tmp, ext.as_deref(), width, height, rgba).and_then(|_| Ok(replace_with(&tmp, path)?));
    if r.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    r
}

fn save_image_as(path: &Path, ext: Option<&str>, width: u32, height: u32, rgba: &[u8]) -> Res<()> {
    match ext {
        Some("jpg") | Some("jpeg") => save_jpeg(path, width, height, rgba, 92),
        Some("bmp") => Ok(std::fs::write(path, encode_bmp(width, height, rgba))?),
        Some("tga") => Ok(std::fs::write(path, encode_tga(width, height, rgba))?),
        Some("tif") | Some("tiff") => save_tiff(path, width, height, rgba),
        Some("webp") => save_webp(path, width, height, rgba),
        Some("qoi") => Ok(std::fs::write(path, encode_qoi(width, height, rgba))?),
        _ => save_png(path, width, height, rgba),
    }
}

/// PNG в память.
pub fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Res<Vec<u8>> {
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, width, height);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()?.write_image_data(rgba)?;
    Ok(out)
}

/// PNG из памяти.
pub fn decode_png(data: &[u8]) -> Res<Image> {
    let mut dec = png::Decoder::new(std::io::Cursor::new(data));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = dec.read_info()?;
    let mut buf = png_buffer(&reader, data.len())?;
    let info = reader.next_frame(&mut buf)?;
    buf.truncate(info.buffer_size());
    Ok((info.width, info.height, expand(info.color_type, buf)?))
}

fn expand(color: png::ColorType, buf: Vec<u8>) -> Res<Vec<u8>> {
    Ok(match color {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf.chunks(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => buf.chunks(2).flat_map(|c| [c[0], c[0], c[0], c[1]]).collect(),
        png::ColorType::Grayscale => buf.iter().flat_map(|&v| [v, v, v, 255]).collect(),
        png::ColorType::Indexed => return Err("палитра не развёрнута".into()),
    })
}

/// BMP: 32 бита с альфой (заголовок BITMAPV4, маски каналов), строки сверху вниз.
pub fn encode_bmp(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let data_len = width * height * 4;
    let mut out = Vec::with_capacity(122 + data_len as usize);
    out.extend(b"BM");
    out.extend((122 + data_len).to_le_bytes());
    out.extend([0u8; 4]);
    out.extend(122u32.to_le_bytes());
    out.extend(108u32.to_le_bytes());
    out.extend((width as i32).to_le_bytes());
    out.extend((-(height as i32)).to_le_bytes());
    out.extend(1u16.to_le_bytes());
    out.extend(32u16.to_le_bytes());
    out.extend(3u32.to_le_bytes()); // BI_BITFIELDS
    out.extend(data_len.to_le_bytes());
    out.extend(2835u32.to_le_bytes());
    out.extend(2835u32.to_le_bytes());
    out.extend([0u8; 8]);
    for m in [0x00ff_0000u32, 0x0000_ff00, 0x0000_00ff, 0xff00_0000] {
        out.extend(m.to_le_bytes());
    }
    out.extend(b"BGRs");
    out.extend([0u8; 48]);
    for p in rgba.chunks(4) {
        out.extend([p[2], p[1], p[0], p[3]]);
    }
    out
}

/// BMP: 24 и 32 бита (без сжатия или с масками каналов), 8 бит с палитрой.
pub fn decode_bmp(d: &[u8]) -> Res<Image> {
    let u16_at = |i: usize| -> Res<u16> { Ok(u16::from_le_bytes(d.get(i..i + 2).ok_or("BMP: обрезан")?.try_into()?)) };
    let u32_at = |i: usize| -> Res<u32> { Ok(u32::from_le_bytes(d.get(i..i + 4).ok_or("BMP: обрезан")?.try_into()?)) };
    if d.get(..2) != Some(b"BM") {
        return Err("не BMP".into());
    }
    let off = u32_at(10)? as usize;
    let hsize = u32_at(14)?;
    let w = u32_at(18)? as i32;
    let h = u32_at(22)? as i32;
    let bpp = u16_at(28)?;
    let comp = u32_at(30)?;
    if w <= 0 || h == 0 || !matches!(comp, 0 | 3) || !matches!(bpp, 8 | 24 | 32) {
        return Err(format!("BMP: такой вид не поддержан ({bpp} бит, сжатие {comp})").into());
    }
    let (w, top_down, h) = (w as usize, h < 0, h.unsigned_abs() as usize);
    let masks = if comp == 3 {
        let base = if hsize >= 52 { 54 } else { 14 + hsize as usize };
        [u32_at(base)?, u32_at(base + 4)?, u32_at(base + 8)?, if hsize >= 56 { u32_at(base + 12)? } else { 0 }]
    } else {
        [0x00ff_0000, 0x0000_ff00, 0x0000_00ff, 0]
    };
    let chan = |v: u32, m: u32| if m == 0 { 255 } else { ((v & m) >> m.trailing_zeros()) as u8 };
    let palette = |i: u8| -> [u8; 4] {
        let p = 14 + hsize as usize + i as usize * 4;
        d.get(p..p + 3).map_or([0, 0, 0, 255], |c| [c[2], c[1], c[0], 255])
    };
    let row = (w * bpp as usize).div_ceil(32) * 4;
    // все строки должны быть в файле — до того, как выделять память под картинку
    if row.checked_mul(h).and_then(|n| n.checked_add(off)).is_none_or(|end| end > d.len()) {
        return Err("BMP: обрезан".into());
    }
    let mut out = vec![0u8; w * h * 4];
    let mut any_alpha = false;
    for y in 0..h {
        let src = off + row * if top_down { y } else { h - 1 - y };
        let line = d.get(src..src + row).ok_or("BMP: обрезан")?;
        for x in 0..w {
            let px = match bpp {
                8 => palette(line[x]),
                24 => [line[x * 3 + 2], line[x * 3 + 1], line[x * 3], 255],
                _ => {
                    let v = u32::from_le_bytes(line[x * 4..x * 4 + 4].try_into()?);
                    let a = chan(v, masks[3]);
                    any_alpha |= masks[3] != 0 && a != 0;
                    [chan(v, masks[0]), chan(v, masks[1]), chan(v, masks[2]), a]
                }
            };
            out[(y * w + x) * 4..][..4].copy_from_slice(&px);
        }
    }
    // 32 бита, где альфа вся нулевая, — на деле без альфы
    if bpp == 32 && masks[3] != 0 && !any_alpha {
        out.chunks_mut(4).for_each(|p| p[3] = 255);
    }
    Ok((w as u32, h as u32, out))
}

/// TGA: 32 бита с альфой, без сжатия, строки сверху вниз.
pub fn encode_tga(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let mut out = vec![0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    out.extend((width as u16).to_le_bytes());
    out.extend((height as u16).to_le_bytes());
    out.extend([32, 0x28]);
    for p in rgba.chunks(4) {
        out.extend([p[2], p[1], p[0], p[3]]);
    }
    out
}

/// TGA: цветной 24/32 и серый 8 бит, без сжатия и RLE.
pub fn decode_tga(d: &[u8]) -> Res<Image> {
    if d.len() < 18 {
        return Err("TGA: обрезан".into());
    }
    let (idlen, cmap, kind) = (d[0] as usize, d[1], d[2]);
    let w = u16::from_le_bytes([d[12], d[13]]) as usize;
    let h = u16::from_le_bytes([d[14], d[15]]) as usize;
    let (bpp, desc) = (d[16], d[17]);
    if cmap != 0 || !matches!(kind, 2 | 3 | 10 | 11) || !matches!(bpp, 8 | 24 | 32) || w == 0 || h == 0 {
        return Err(format!("TGA: такой вид не поддержан (вид {kind}, {bpp} бит)").into());
    }
    let bytes = bpp as usize / 8;
    // данных должно хватать: без сжатия — на все точки, с RLE — пакет даёт не больше 128 точек
    let body = d.len().saturating_sub(18 + idlen);
    let enough = if kind >= 9 { w * h <= body.saturating_mul(128) } else { w * h * bytes <= body };
    if !enough {
        return Err("TGA: обрезан".into());
    }
    let mut px = Vec::with_capacity(w * h * bytes);
    let mut i = 18 + idlen;
    if kind >= 9 {
        while px.len() < w * h * bytes {
            let head = *d.get(i).ok_or("TGA: обрезан")?;
            i += 1;
            let n = (head & 0x7f) as usize + 1;
            if head & 0x80 != 0 {
                let p = d.get(i..i + bytes).ok_or("TGA: обрезан")?;
                for _ in 0..n {
                    px.extend_from_slice(p);
                }
                i += bytes;
            } else {
                px.extend_from_slice(d.get(i..i + n * bytes).ok_or("TGA: обрезан")?);
                i += n * bytes;
            }
        }
        px.truncate(w * h * bytes);
    } else {
        px.extend_from_slice(d.get(i..i + w * h * bytes).ok_or("TGA: обрезан")?);
    }
    let top = desc & 0x20 != 0;
    let mut out = vec![0u8; w * h * 4];
    for y in 0..h {
        let sy = if top { y } else { h - 1 - y };
        for x in 0..w {
            let p = &px[(sy * w + x) * bytes..][..bytes];
            let c = match bytes {
                1 => [p[0], p[0], p[0], 255],
                3 => [p[2], p[1], p[0], 255],
                _ => [p[2], p[1], p[0], p[3]],
            };
            out[(y * w + x) * 4..][..4].copy_from_slice(&c);
        }
    }
    Ok((w as u32, h as u32, out))
}

/// TIFF: RGBA 8 бит, сжатие Deflate.
pub fn save_tiff(path: &Path, width: u32, height: u32, rgba: &[u8]) -> Res<()> {
    let mut enc = tiff::encoder::TiffEncoder::new(BufWriter::new(File::create(path)?))?;
    enc.write_image_with_compression::<tiff::encoder::colortype::RGBA8, _>(width, height, tiff::encoder::compression::Deflate::default(), rgba)?;
    Ok(())
}

/// TIFF: RGB/RGBA/серый 8 и 16 бит (первая страница).
pub fn load_tiff(path: &Path) -> Res<Image> {
    use tiff::decoder::{Decoder, DecodingResult};
    use tiff::ColorType;
    let mut dec = Decoder::new(std::io::BufReader::new(File::open(path)?))?;
    let (w, h) = dec.dimensions()?;
    let color = dec.colortype()?;
    let samples: Vec<u8> = match dec.read_image()? {
        DecodingResult::U8(v) => v,
        DecodingResult::U16(v) => v.iter().map(|x| (x >> 8) as u8).collect(),
        _ => return Err("TIFF: такая глубина не поддержана".into()),
    };
    let out = match color {
        ColorType::RGBA(_) => samples,
        ColorType::RGB(_) => samples.chunks(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
        ColorType::GrayA(_) => samples.chunks(2).flat_map(|c| [c[0], c[0], c[0], c[1]]).collect(),
        ColorType::Gray(_) => samples.iter().flat_map(|&v| [v, v, v, 255]).collect(),
        c => return Err(format!("TIFF: цвет {c:?} не поддержан").into()),
    };
    if out.len() != (w * h * 4) as usize {
        return Err("TIFF: размер данных не сходится".into());
    }
    Ok((w, h, out))
}

/// WebP без потерь.
pub fn save_webp(path: &Path, width: u32, height: u32, rgba: &[u8]) -> Res<()> {
    let enc = image_webp::WebPEncoder::new(BufWriter::new(File::create(path)?));
    enc.encode(rgba, width, height, image_webp::ColorType::Rgba8)?;
    Ok(())
}

pub fn load_webp(path: &Path) -> Res<Image> {
    let mut dec = image_webp::WebPDecoder::new(std::io::BufReader::new(File::open(path)?))?;
    let (w, h) = dec.dimensions();
    let need = dec.output_buffer_size().ok_or("WebP: слишком большой")?;
    // 42-байтный файл, заявивший 16383×16383, разбирался 46 с: размер должен сходиться с данными
    let len = std::fs::metadata(path)?.len() as usize;
    if w as u64 * h as u64 > 1 << 29 || need > len.saturating_mul(1100).saturating_add(1 << 20) {
        return Err(format!("WebP: неверный размер {w}×{h}").into());
    }
    let mut buf = vec![0u8; need];
    dec.read_image(&mut buf)?;
    let out = if dec.has_alpha() { buf } else { buf.chunks(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect() };
    Ok((w, h, out))
}

fn qoi_hash(p: [u8; 4]) -> usize {
    (p[0] as usize * 3 + p[1] as usize * 5 + p[2] as usize * 7 + p[3] as usize * 11) % 64
}

/// QOI (qoiformat.org): быстрый сжатый формат без потерь.
pub fn encode_qoi(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(14 + rgba.len() / 2);
    out.extend(b"qoif");
    out.extend(width.to_be_bytes());
    out.extend(height.to_be_bytes());
    out.extend([4, 0]);
    let mut seen = [[0u8; 4]; 64];
    let mut prev = [0u8, 0, 0, 255];
    let mut run = 0u8;
    let px: Vec<[u8; 4]> = rgba.chunks(4).map(|c| [c[0], c[1], c[2], c[3]]).collect();
    for (i, &p) in px.iter().enumerate() {
        if p == prev {
            run += 1;
            if run == 62 || i == px.len() - 1 {
                out.push(0xc0 | (run - 1));
                run = 0;
            }
            continue;
        }
        if run > 0 {
            out.push(0xc0 | (run - 1));
            run = 0;
        }
        let h = qoi_hash(p);
        if seen[h] == p {
            out.push(h as u8);
        } else {
            seen[h] = p;
            if p[3] == prev[3] {
                let d = [0, 1, 2].map(|c| p[c].wrapping_sub(prev[c]) as i8);
                let (dr_dg, db_dg) = (d[0].wrapping_sub(d[1]), d[2].wrapping_sub(d[1]));
                if d.iter().all(|v| (-2..=1).contains(v)) {
                    out.push(0x40 | (((d[0] + 2) as u8) << 4) | (((d[1] + 2) as u8) << 2) | (d[2] + 2) as u8);
                } else if (-32..=31).contains(&d[1]) && (-8..=7).contains(&dr_dg) && (-8..=7).contains(&db_dg) {
                    out.push(0x80 | (d[1] + 32) as u8);
                    out.push((((dr_dg + 8) as u8) << 4) | (db_dg + 8) as u8);
                } else {
                    out.extend([0xfe, p[0], p[1], p[2]]);
                }
            } else {
                out.extend([0xff, p[0], p[1], p[2], p[3]]);
            }
        }
        prev = p;
    }
    out.extend([0, 0, 0, 0, 0, 0, 0, 1]);
    out
}

pub fn decode_qoi(d: &[u8]) -> Res<Image> {
    if d.len() < 22 || &d[..4] != b"qoif" {
        return Err("не QOI".into());
    }
    let w = u32::from_be_bytes(d[4..8].try_into()?);
    let h = u32::from_be_bytes(d[8..12].try_into()?);
    let n = (w as usize).checked_mul(h as usize).filter(|n| *n <= 1 << 28).ok_or("QOI: слишком большой")?;
    // байт данных даёт не больше 62 точек (QOI_OP_RUN) — иначе размер ложный
    if n > d.len().saturating_mul(62) {
        return Err("QOI: размер не сходится с данными".into());
    }
    let mut out = Vec::with_capacity(n * 4);
    let mut seen = [[0u8; 4]; 64];
    let mut p = [0u8, 0, 0, 255];
    let mut i = 14;
    while out.len() < n * 4 {
        let b = *d.get(i).ok_or("QOI: обрезан")?;
        i += 1;
        let mut run = 1;
        match b {
            0xfe => {
                p[..3].copy_from_slice(d.get(i..i + 3).ok_or("QOI: обрезан")?);
                i += 3;
            }
            0xff => {
                p.copy_from_slice(d.get(i..i + 4).ok_or("QOI: обрезан")?);
                i += 4;
            }
            _ => match b >> 6 {
                0 => p = seen[b as usize],
                1 => {
                    p[0] = p[0].wrapping_add((b >> 4) & 3).wrapping_sub(2);
                    p[1] = p[1].wrapping_add((b >> 2) & 3).wrapping_sub(2);
                    p[2] = p[2].wrapping_add(b & 3).wrapping_sub(2);
                }
                2 => {
                    let b2 = *d.get(i).ok_or("QOI: обрезан")?;
                    i += 1;
                    let dg = (b & 0x3f).wrapping_sub(32);
                    p[0] = p[0].wrapping_add(dg).wrapping_add(b2 >> 4).wrapping_sub(8);
                    p[1] = p[1].wrapping_add(dg);
                    p[2] = p[2].wrapping_add(dg).wrapping_add(b2 & 15).wrapping_sub(8);
                }
                _ => run = (b & 0x3f) as usize + 1,
            },
        }
        seen[qoi_hash(p)] = p;
        for _ in 0..run.min(n - out.len() / 4) {
            out.extend(p);
        }
    }
    Ok((w, h, out))
}

/// Сохранить JPEG с качеством 1–100 (прозрачность ложится на белый).
pub fn save_jpeg(path: &Path, width: u32, height: u32, rgba: &[u8], quality: u8) -> Result<(), Box<dyn std::error::Error>> {
    let rgb: Vec<u8> = rgba
        .chunks(4)
        .flat_map(|p| {
            let a = p[3] as u32;
            [0, 1, 2].map(|c| ((p[c] as u32 * a + 255 * (255 - a) + 127) / 255) as u8)
        })
        .collect();
    let enc = jpeg_encoder::Encoder::new_file(path, quality.clamp(1, 100))?;
    enc.encode(&rgb, width as u16, height as u16, jpeg_encoder::ColorType::Rgb)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> (u32, u32, Vec<u8>) {
        let (w, h) = (13u32, 7u32);
        let mut v = Vec::new();
        for y in 0..h {
            for x in 0..w {
                v.extend([(x * 19) as u8, (y * 33) as u8, if (x + y) % 3 == 0 { 7 } else { 200 }, if x < 4 { 255 } else { (x * 20) as u8 }]);
            }
        }
        (w, h, v)
    }

    /// Каждый формат без потерь: записали — прочитали то же самое.
    #[test]
    fn lossless_formats_round_trip() {
        let (w, h, rgba) = sample();
        let dir = std::env::temp_dir().join(format!("grazysai-io-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for ext in ["png", "bmp", "tga", "tiff", "webp", "qoi"] {
            let p = dir.join(format!("t.{ext}"));
            save_image(&p, w, h, &rgba).unwrap();
            let back = load_image(&p).unwrap();
            assert_eq!((back.0, back.1), (w, h), "{ext}");
            assert!(back.2 == rgba, "{ext}: пиксели отличаются");
        }
        // RLE-TGA и 24-битный BMP снизу вверх, собранные вручную
        let tga = [0u8, 0, 10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3, 0, 1, 0, 24, 0, 0x82, 1, 2, 3];
        assert_eq!(decode_tga(&tga).unwrap().2, [3, 2, 1, 255, 3, 2, 1, 255, 3, 2, 1, 255]);
        std::fs::remove_dir_all(dir).ok();
    }
}
