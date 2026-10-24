//! Вывод анимации: GIF, анимированный PNG (APNG), набор PNG по кадрам и видео через ffmpeg
//! (если он установлен). Кадры — готовые RGBA по 8 бит одного размера.

use std::io::Write;
use std::path::{Path, PathBuf};

type Res<T> = Result<T, String>;

/// GIF: 256 цветов на кадр (NeuQuant), задержка — по частоте кадров (в сотых секунды).
pub fn gif(path: &Path, w: u32, h: u32, fps: u16, looped: bool, frames: impl Iterator<Item = Vec<u8>>) -> Res<()> {
    if w > u16::MAX as u32 || h > u16::MAX as u32 {
        return Err("GIF: холст больше 65535 точек".into());
    }
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut enc = gif::Encoder::new(std::io::BufWriter::new(file), w as u16, h as u16, &[]).map_err(|e| e.to_string())?;
    enc.set_repeat(if looped { gif::Repeat::Infinite } else { gif::Repeat::Finite(0) }).map_err(|e| e.to_string())?;
    let delay = (100.0 / fps.max(1) as f64).round().max(1.0) as u16;
    for mut f in frames {
        let mut frame = gif::Frame::from_rgba_speed(w as u16, h as u16, &mut f, 10);
        frame.delay = delay;
        frame.dispose = gif::DisposalMethod::Background;
        enc.write_frame(&frame).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Анимированный PNG: полный цвет и прозрачность.
pub fn apng(path: &Path, w: u32, h: u32, fps: u16, looped: bool, frames: Vec<Vec<u8>>) -> Res<()> {
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.set_animated(frames.len().max(1) as u32, if looped { 0 } else { 1 }).map_err(|e| e.to_string())?;
    enc.set_frame_delay(1, fps.max(1)).map_err(|e| e.to_string())?;
    let mut wr = enc.write_header().map_err(|e| e.to_string())?;
    for f in &frames {
        wr.write_image_data(f).map_err(|e| e.to_string())?;
    }
    wr.finish().map_err(|e| e.to_string())
}

/// Кадры отдельными PNG: `<имя>_0001.png`, `<имя>_0002.png`… рядом с `path`. Возвращает пути.
pub fn png_sequence(path: &Path, w: u32, h: u32, frames: impl Iterator<Item = Vec<u8>>) -> Res<Vec<PathBuf>> {
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "frame".into());
    let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let mut out = Vec::new();
    for (i, f) in frames.enumerate() {
        let p = dir.join(format!("{stem}_{:04}.png", i + 1));
        paint_core::io::save_png(&p, w, h, &f).map_err(|e| e.to_string())?;
        out.push(p);
    }
    Ok(out)
}

/// Есть ли ffmpeg (для видео).
pub fn has_ffmpeg() -> bool {
    std::process::Command::new("ffmpeg").arg("-version").stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status().is_ok_and(|s| s.success())
}

/// Видео MP4 (H.264) через ffmpeg: кадры идут ему по трубе; нечётный размер дополняется на точку.
pub fn video(path: &Path, w: u32, h: u32, fps: u16, frames: impl Iterator<Item = Vec<u8>>) -> Res<()> {
    let mut child = std::process::Command::new("ffmpeg")
        .args(["-y", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "rgba", "-s", &format!("{w}x{h}"), "-r", &fps.max(1).to_string(), "-i", "-"])
        .args(["-vf", "pad=ceil(iw/2)*2:ceil(ih/2)*2", "-c:v", "libx264", "-pix_fmt", "yuv420p"])
        .arg(path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("ffmpeg не запустился: {e}"))?;
    {
        let mut stdin = child.stdin.take().ok_or("ffmpeg: нет входа")?;
        for f in frames {
            if stdin.write_all(&f).is_err() {
                break;
            }
        }
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!("ffmpeg: {}", String::from_utf8_lossy(&out.stderr).trim()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frames(n: usize) -> Vec<Vec<u8>> {
        (0..n).map(|k| (0..16 * 8).flat_map(|i| if i % 16 == k { [255, 0, 0, 255] } else { [255, 255, 255, 255] }).collect()).collect()
    }

    #[test]
    fn gif_and_apng_decode_back() {
        let dir = std::env::temp_dir().join(format!("gs-anim-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let g = dir.join("a.gif");
        gif(&g, 16, 8, 10, true, frames(5).into_iter()).unwrap();
        let mut opts = gif::DecodeOptions::new();
        opts.set_color_output(gif::ColorOutput::RGBA);
        let mut dec = opts.read_info(std::fs::File::open(&g).unwrap()).unwrap();
        let mut n = 0;
        while let Some(f) = dec.read_next_frame().unwrap() {
            assert_eq!(f.delay, 10);
            n += 1;
        }
        assert_eq!(n, 5);
        let a = dir.join("a.png");
        apng(&a, 16, 8, 12, true, frames(4)).unwrap();
        let dec = png::Decoder::new(std::fs::File::open(&a).unwrap());
        let r = dec.read_info().unwrap();
        let ac = r.info().animation_control().unwrap();
        assert_eq!((ac.num_frames, ac.num_plays), (4, 0));
        let seq = png_sequence(&dir.join("k.png"), 16, 8, frames(3).into_iter()).unwrap();
        assert_eq!(seq.len(), 3);
        assert!(seq[2].ends_with("k_0003.png"));
        std::fs::remove_dir_all(dir).ok();
    }
}
