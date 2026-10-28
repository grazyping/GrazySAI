//! Какие форматы рисунка читаются и пишутся — одна таблица для рисовалки и конвертера.

use paint_core::document::Document;
use std::path::Path;

type Res<T> = Result<T, Box<dyn std::error::Error>>;

/// Со слоями: SAI 2, PSD, PSB, OpenRaster; остальное — картинки одним слоем.
pub const LAYERED: [&str; 4] = ["sai2", "psd", "psb", "ora"];

fn ext(path: &Path) -> String {
    path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase()
}

/// Читается ли рисунок с таким расширением.
pub fn can_read(path: &Path) -> bool {
    let e = ext(path);
    LAYERED.contains(&e.as_str()) || paint_core::io::IMAGE_EXTS.contains(&e.as_str())
}

/// Рисунок любого читаемого формата; заметки — что прочитано приближённо.
pub fn load(path: &Path) -> Res<(Document, Vec<String>)> {
    Ok(match ext(path).as_str() {
        "sai2" => {
            let l = crate::sai2_doc::load(path)?;
            (l.document, l.warnings)
        }
        "psd" | "psb" => (crate::psd::load(path)?, Vec::new()),
        "ora" => (crate::ora::load(path)?, Vec::new()),
        _ => {
            let (w, h, rgba) = paint_core::io::load_image(path)?;
            (Document::from_rgba(w, h, &rgba), Vec::new())
        }
    })
}

/// Копия с «применёнными» стилями слоя (обводка, тень, свечение — пикселями): PSD и OpenRaster
/// наших стилей не знают и без этого теряют их.
fn with_baked_styles(doc: &Document) -> Option<Document> {
    fn collect(layers: &[paint_core::Layer], prefix: &[usize], out: &mut Vec<Vec<usize>>) {
        for (i, l) in layers.iter().enumerate() {
            let path = [prefix, &[i]].concat();
            match &l.children {
                Some(ch) => collect(ch, &path, out),
                None if l.styles.as_deref().is_some_and(|s| !s.is_empty()) => out.push(path),
                None => {}
            }
        }
    }
    let mut paths = Vec::new();
    collect(&doc.layers, &[], &mut paths);
    if paths.is_empty() {
        return None;
    }
    let mut d = doc.snapshot();
    for p in paths {
        d.bake_styles(&p);
    }
    Some(d)
}

/// Записать рисунок по расширению: форматы со слоями — со слоями, остальные — сведённым.
pub fn save(doc: &Document, path: &Path, psd: crate::psd::SaveOptions) -> Res<()> {
    let e = ext(path);
    // всем форматам, кроме .sai2, нужна вся сведённая картинка разом
    if e != "sai2" && !doc.fits_flat() {
        return Err(format!("холст {}×{} слишком велик для .{e} — сохраните его в .sai2", doc.width, doc.height).into());
    }
    let baked = if matches!(e.as_str(), "psd" | "psb" | "ora") { with_baked_styles(doc) } else { None };
    let doc = baked.as_ref().unwrap_or(doc);
    match e.as_str() {
        "sai2" => crate::sai2_write::save(doc, path)?,
        "psd" => crate::psd::save_with(doc, path, crate::psd::SaveOptions { big: false, ..psd })?,
        "psb" => crate::psd::save_with(doc, path, crate::psd::SaveOptions { big: true, ..psd })?,
        "ora" => crate::ora::save(doc, path)?,
        e if paint_core::io::IMAGE_EXTS.contains(&e) => paint_core::io::save_image(path, doc.width, doc.height, &doc.flatten())?,
        e => return Err(format!("не умею писать .{e}").into()),
    }
    Ok(())
}
