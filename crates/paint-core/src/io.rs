//! Чтение и запись картинок.

use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

pub fn save_png(path: &Path, width: u32, height: u32, rgba: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let mut enc = png::Encoder::new(BufWriter::new(File::create(path)?), width, height);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()?.write_image_data(rgba)?;
    Ok(())
}

/// Прочитать PNG как RGBA по 8 бит (палитра и серый разворачиваются).
pub fn load_png(path: &Path) -> Result<(u32, u32, Vec<u8>), Box<dyn std::error::Error>> {
    let mut dec = png::Decoder::new(File::open(path)?);
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = dec.read_info()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf)?;
    buf.truncate(info.buffer_size());
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf.chunks(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => buf.chunks(2).flat_map(|c| [c[0], c[0], c[0], c[1]]).collect(),
        png::ColorType::Grayscale => buf.iter().flat_map(|&v| [v, v, v, 255]).collect(),
        png::ColorType::Indexed => return Err("палитра не развёрнута".into()),
    };
    Ok((info.width, info.height, rgba))
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

/// Прочитать картинку по расширению: PNG или JPEG.
pub fn load_image(path: &Path) -> Result<(u32, u32, Vec<u8>), Box<dyn std::error::Error>> {
    match path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref() {
        Some("jpg") | Some("jpeg") => load_jpeg(path),
        _ => load_png(path),
    }
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
