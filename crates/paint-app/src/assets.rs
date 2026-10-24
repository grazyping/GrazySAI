//! «Менеджер ресурсов» (Asset Manager, SAI 2 2020+): свои бумаги слоя, формы кисти, текстуры
//! кисти, отпечатки (и формы россыпи), черновики — файлами в своей папке программы; наборы
//! образцов — в настройках. Ресурс можно сделать из слоя: «чёрное на белом», «белое на чёрном»
//! или по прозрачности слоя, отпечаток — ещё и в цвете (ARGB), как в окне «New … » SAI.

use crate::i18n::{tr, trf};
use std::path::PathBuf;

/// Вид ресурса-файла: папка в своей папке программы и расширение.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Paper,
    Form,
    Texture,
    Tip,
    Scratch,
}

impl Kind {
    pub const ALL: [Kind; 5] = [Kind::Paper, Kind::Form, Kind::Texture, Kind::Tip, Kind::Scratch];

    pub fn dir(self) -> &'static str {
        match self {
            Kind::Paper => "papertex",
            Kind::Form => "blotmap",
            Kind::Texture => "brushtex",
            Kind::Tip => "tips",
            Kind::Scratch => "scratch",
        }
    }

    fn ext(self) -> &'static str {
        match self {
            Kind::Tip | Kind::Scratch => "png",
            _ => "bmp",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Kind::Paper => "Бумага слоя",
            Kind::Form => "Формы кисти",
            Kind::Texture => "Текстуры кисти",
            Kind::Tip => "Отпечатки и формы россыпи",
            Kind::Scratch => "Черновики",
        }
    }

    pub fn path(self, name: &str) -> Option<PathBuf> {
        Some(paint_core::paper::user_dir(self.dir())?.join(format!("{name}.{}", self.ext())))
    }

    /// Свои ресурсы этого вида (в своей папке программы), по алфавиту; у отпечатков — без
    /// следующих картинок набора (`имя.2.png`).
    pub fn items(self) -> Vec<String> {
        let Some(dir) = paint_core::paper::user_dir(self.dir()) else { return Vec::new() };
        let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
        let mut out: Vec<String> = rd
            .flatten()
            .filter_map(|e| {
                let p = e.path();
                p.extension().is_some_and(|x| x.eq_ignore_ascii_case(self.ext())).then(|| p.file_stem()?.to_str().map(str::to_string)).flatten()
            })
            .filter(|n| !(self == Kind::Tip && n.rsplit_once('.').is_some_and(|(_, k)| k.parse::<u32>().is_ok())))
            .collect();
        out.sort_by_key(|n| n.to_lowercase());
        out
    }
}

/// Как понимать картинку слоя при создании ресурса.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ImageType {
    /// Чёрное на белом: тёмное — краска.
    BlackOnWhite,
    /// Белое на чёрном: светлое — краска.
    WhiteOnBlack,
    /// Только прозрачность слоя.
    Alpha,
    /// В цвете (ARGB, только отпечатки): цвет и прозрачность слоя как есть.
    Argb,
}

/// Имя файла без запрещённых знаков.
pub fn clean_name(name: &str) -> String {
    let n: String = name.chars().take(64).map(|c| if c.is_alphanumeric() || " -_()".contains(c) { c } else { '_' }).collect();
    let n = n.trim().to_string();
    if n.is_empty() { tr("ресурс").to_string() } else { n }
}

/// «Краска» (0..255) точки `px` слоя (8 бит RGBA, без предумножения) по виду картинки.
fn ink(px: [u8; 4], t: ImageType) -> u8 {
    let a = px[3] as u32;
    // на белом или на чёрном фоне — как видно на холсте
    let lum = (px[0] as u32 * 77 + px[1] as u32 * 150 + px[2] as u32 * 29 + 128) >> 8;
    match t {
        ImageType::Alpha | ImageType::Argb => px[3],
        ImageType::BlackOnWhite => (255 - (lum * a + 255 * (255 - a)) / 255) as u8,
        ImageType::WhiteOnBlack => ((lum * a) / 255) as u8,
    }
}

/// Сделать ресурс вида `kind` из картинки `rgba` (`w`×`h`, без предумножения). Бумаги, формы и
/// текстуры — серые BMP: краска — тёмное; отпечатки — PNG с прозрачностью; черновик — как есть.
/// Возвращает имя (свободное: с номером, если такое уже есть).
pub fn create(kind: Kind, name: &str, w: usize, h: usize, rgba: &[u8], t: ImageType) -> Result<String, String> {
    if w == 0 || h == 0 || w > 4096 || h > 4096 {
        return Err(trf("Размер {0}×{1}: нужно от 1 до 4096 точек по стороне", &[&w, &h]));
    }
    let dir = paint_core::paper::user_dir(kind.dir()).ok_or(tr("нет папки настроек"))?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let base = clean_name(name);
    let mut n = base.clone();
    let mut i = 2;
    while dir.join(format!("{n}.{}", kind.ext())).exists() {
        n = format!("{base} {i}");
        i += 1;
    }
    let path = dir.join(format!("{n}.{}", kind.ext()));
    let inks: Vec<u8> = rgba.as_chunks::<4>().0.iter().map(|p| ink(*p, t)).collect();
    let res = match kind {
        Kind::Scratch => paint_core::io::save_image(&path, w as u32, h as u32, rgba).map_err(|e| e.to_string()),
        Kind::Tip if t == ImageType::Argb => {
            // метка `<имя>.argb` рядом: отпечаток кладёт свои цвета
            paint_core::io::save_image(&path, w as u32, h as u32, rgba).map_err(|e| e.to_string())?;
            std::fs::write(dir.join(format!("{n}.argb")), b"").map_err(|e| e.to_string())
        }
        Kind::Tip => {
            let out: Vec<u8> = inks.iter().flat_map(|&a| [0, 0, 0, a]).collect();
            paint_core::io::save_image(&path, w as u32, h as u32, &out).map_err(|e| e.to_string())
        }
        _ => {
            let tex = paint_core::paper::PaperTexture { width: w, height: h, gray: inks.iter().map(|&v| 255 - v).collect() };
            paint_core::io::write_atomic(&path, &tex.to_bmp()).map_err(|e| e.to_string())
        }
    };
    res.map(|_| n)
}

/// Переименовать ресурс (у отпечатков — со всем набором картинок).
pub fn rename(kind: Kind, from: &str, to: &str) -> Result<String, String> {
    let to = clean_name(to);
    let (a, b) = (kind.path(from).ok_or(tr("нет папки настроек"))?, kind.path(&to).ok_or(tr("нет папки настроек"))?);
    if b.exists() {
        return Err(trf("«{0}» уже есть", &[&to]));
    }
    std::fs::rename(&a, &b).map_err(|e| e.to_string())?;
    if kind == Kind::Tip {
        let dir = paint_core::paper::user_dir(kind.dir()).ok_or(tr("нет папки настроек"))?;
        std::fs::rename(dir.join(format!("{from}.argb")), dir.join(format!("{to}.argb"))).ok();
        for i in 2.. {
            if std::fs::rename(dir.join(format!("{from}.{i}.png")), dir.join(format!("{to}.{i}.png"))).is_err() {
                break;
            }
        }
    }
    Ok(to)
}

/// Удалить ресурс (у отпечатков — со всем набором картинок).
pub fn delete(kind: Kind, name: &str) -> Result<(), String> {
    std::fs::remove_file(kind.path(name).ok_or(tr("нет папки настроек"))?).map_err(|e| e.to_string())?;
    if kind == Kind::Tip {
        if let Some(dir) = paint_core::paper::user_dir(kind.dir()) {
            std::fs::remove_file(dir.join(format!("{name}.argb"))).ok();
            for i in 2.. {
                if std::fs::remove_file(dir.join(format!("{name}.{i}.png"))).is_err() {
                    break;
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Чёрное на белом и белое на чёрном дают одно и то же; по прозрачности — сама прозрачность.
    #[test]
    fn image_types() {
        assert_eq!(ink([0, 0, 0, 255], ImageType::BlackOnWhite), 255);
        assert_eq!(ink([255, 255, 255, 255], ImageType::BlackOnWhite), 0);
        assert_eq!(ink([0, 0, 0, 0], ImageType::BlackOnWhite), 0);
        assert_eq!(ink([255, 255, 255, 255], ImageType::WhiteOnBlack), 255);
        assert_eq!(ink([10, 20, 30, 128], ImageType::Alpha), 128);
    }

    #[test]
    fn create_rename_delete() {
        let dir = std::env::temp_dir().join(format!("gs-assets-{}", std::process::id()));
        paint_core::paper::set_user_dir(Some(dir.clone()));
        let rgba: Vec<u8> = (0..16).flat_map(|i| [0, 0, 0, if i % 2 == 0 { 255 } else { 0 }]).collect();
        let n = create(Kind::Texture, "проба/1", 4, 4, &rgba, ImageType::Alpha).unwrap();
        assert_eq!(n, "проба_1");
        let t = paint_core::paper::texture_of("brushtex", &n).unwrap();
        assert_eq!((t.gray[0], t.gray[1]), (0, 255));
        assert_eq!(create(Kind::Texture, "проба/1", 4, 4, &rgba, ImageType::Alpha).unwrap(), "проба_1 2");
        let r = rename(Kind::Texture, "проба_1 2", "другое").unwrap();
        assert_eq!(Kind::Texture.items(), vec!["другое".to_string(), "проба_1".to_string()]);
        delete(Kind::Texture, &r).unwrap();
        // отпечаток в цвете сохраняет цвет и становится цветным
        let col: Vec<u8> = (0..16).flat_map(|i| if i % 2 == 0 { [200, 30, 10, 255] } else { [9, 9, 9, 0] }).collect();
        let tn = create(Kind::Tip, "цвет", 4, 4, &col, ImageType::Argb).unwrap();
        let st = paint_core::paper::stamp_of(&tn).unwrap();
        assert_eq!((st.alpha[0], st.alpha[1]), (255, 0));
        assert_eq!(st.color.as_ref().map(|c| c[0]), Some([200, 30, 10]));
        let tn2 = rename(Kind::Tip, &tn, "цвет2").unwrap();
        assert!(paint_core::paper::user_dir("tips").unwrap().join("цвет2.argb").exists());
        delete(Kind::Tip, &tn2).unwrap();
        assert!(!paint_core::paper::user_dir("tips").unwrap().join("цвет2.argb").exists());
        assert_eq!(Kind::Texture.items(), vec!["проба_1".to_string()]);
        std::fs::remove_dir_all(dir).ok();
        paint_core::paper::set_user_dir(None);
    }
}
