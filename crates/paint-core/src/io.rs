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
