//! Самопроверка без окна: `GrazySAI --self-test [отчёт.txt]`. Прогоняет то, что зависит от
//! системы (пути с не-латинскими именами, папка настроек, запись и чтение всех форматов, SQLite
//! для кистей CSP, шрифты, отпечатки), и пишет текстовый отчёт — его можно прислать вместо
//! снимков экрана. Файлы пользователя не трогаются: всё пишется во временную папку и удаляется.

use paint_core::pixel::from_rgba8;
use paint_core::{BrushParams, Document, InputPoint, Layer, Stroke};
use std::path::{Path, PathBuf};
use std::time::Instant;

type Res = Result<String, Box<dyn std::error::Error>>;
/// Проверка: имя и действие над временной папкой.
type Check = (&'static str, Box<dyn Fn(&Path) -> Res>);

/// Запустить проверки и записать отчёт; возвращает, прошли ли все.
pub fn run(report: Option<PathBuf>) -> bool {
    let dir = std::env::temp_dir().join(format!("GrazySAI-проверка-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut lines = vec![
        format!("GrazySAI {} — самопроверка / self-test", env!("CARGO_PKG_VERSION")),
        format!("{} {}, потоков: {}", std::env::consts::OS, std::env::consts::ARCH, paint_core::threads()),
        format!("папка проверки: {}", dir.display()),
        String::new(),
    ];
    let mut failed = 0;
    let checks: Vec<Check> = vec![
        ("временная папка с русским именем", Box::new(|d| {
            std::fs::create_dir_all(d)?;
            std::fs::write(d.join("проба.txt"), "ок")?;
            Ok(std::fs::read_to_string(d.join("проба.txt"))?)
        })),
        ("папка настроек", Box::new(|_| {
            let s = crate::settings::dir().ok_or("не найдена")?;
            std::fs::create_dir_all(&s)?;
            let probe = s.join(".self-test");
            std::fs::write(&probe, "x")?;
            std::fs::remove_file(&probe)?;
            Ok(s.display().to_string())
        })),
        ("мазок кистью и сведение", Box::new(|_| {
            let doc = sample();
            let px = doc.flatten();
            let dark = px.as_chunks::<4>().0.iter().filter(|p| p[0] < 128).count();
            if dark < 50 {
                return Err(format!("мазок почти пуст: {dark} тёмных точек").into());
            }
            Ok(format!("{dark} тёмных точек"))
        })),
        ("форматы: запись и чтение", Box::new(formats)),
        ("кисти CSP (SQLite)", Box::new(|d| {
            let p = d.join("кисть.sut");
            let db = rusqlite::Connection::open(&p)?;
            db.execute_batch(
                "CREATE TABLE Manager(ToolType INTEGER); INSERT INTO Manager VALUES(0);
                 CREATE TABLE Node(NodeName TEXT, NodeVariantID INTEGER, NodeInitVariantID INTEGER); INSERT INTO Node VALUES('Проба', 1, 2);
                 CREATE TABLE Variant(VariantID INTEGER, BrushSize REAL, Opacity INTEGER); INSERT INTO Variant VALUES(1, 25.0, 50);",
            )?;
            drop(db);
            let b = paint_formats::csp::load(&p)?;
            Ok(format!("«{}», {} px", b[0].name, b[0].size))
        })),
        ("отпечаток кисти", Box::new(|d| {
            let s = paint_core::Stamp { width: 3, height: 2, alpha: vec![0, 128, 255, 255, 128, 0] };
            paint_core::paper::set_user_dir(Some(d.to_path_buf()));
            let name = paint_core::paper::save_tip("проба", &s)?;
            let back = paint_core::paper::stamp_of(&name).ok_or("не прочитан")?;
            paint_core::paper::set_user_dir(crate::settings::dir());
            if back.alpha != s.alpha {
                return Err("значения изменились".into());
            }
            Ok(name)
        })),
        ("системные шрифты", Box::new(|_| {
            let n = paint_formats::sai2_text::system_fonts().len();
            if n == 0 {
                return Err("не найдено ни одного".into());
            }
            Ok(format!("{n}"))
        })),
    ];
    for (name, f) in &checks {
        let t = Instant::now();
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&dir)));
        let ms = t.elapsed().as_millis();
        match r {
            Ok(Ok(v)) => lines.push(format!("OK    {name}: {v} ({ms} мс)")),
            Ok(Err(e)) => {
                failed += 1;
                lines.push(format!("FAIL  {name}: {e} ({ms} мс)"));
            }
            Err(_) => {
                failed += 1;
                lines.push(format!("FAIL  {name}: сбой (паника)"));
            }
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    lines.push(String::new());
    lines.push(if failed == 0 { "Все проверки прошли / all checks passed".to_string() } else { format!("Не прошло проверок: {failed} / failed: {failed}") });
    let text = lines.join("\n") + "\n";
    let out = report.unwrap_or_else(|| crate::settings::dir().unwrap_or_else(std::env::temp_dir).join("self-test.txt"));
    let _ = std::fs::write(&out, &text);
    print!("{text}");
    println!("отчёт / report: {}", out.display());
    failed == 0
}

/// Документ из двух слоёв с мазком и прозрачностью.
fn sample() -> Document {
    let mut doc = Document::new(96, 64);
    for y in 0..64 {
        for x in 0..96 {
            doc.layers[0].pixels.set(x, y, from_rgba8([250, 240, 220, 255]));
        }
    }
    let mut top = Layer::new("Слой с мазком");
    top.opacity = 0.8;
    doc.layers.push(top);
    let mut s = Stroke::new(BrushParams { size: 9.0, size_by_pressure: false, ..Default::default() });
    for i in 0..=20 {
        s.add_point(InputPoint { x: 10.0 + i as f32 * 3.8, y: 20.0 + i as f32 * 1.5, pressure: 1.0 });
    }
    doc.commit_stroke(1, &s, [20, 30, 60]);
    doc
}

/// Каждый формат: записать в файл с русским именем, прочитать, сравнить сведение.
fn formats(d: &Path) -> Res {
    let doc = sample();
    let want = doc.flatten();
    let mut done = Vec::new();
    let close = |a: &[u8], b: &[u8], tol: i32| a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (*x as i32 - *y as i32).abs() <= tol);
    // со слоями
    for ext in ["sai2", "psd", "psb", "ora"] {
        let p = d.join(format!("рисунок.{ext}"));
        match ext {
            "sai2" => paint_formats::sai2_write::save(&doc, &p)?,
            "psd" => paint_formats::psd::save(&doc, &p)?,
            "psb" => paint_formats::psd::save_psb(&doc, &p)?,
            _ => paint_formats::ora::save(&doc, &p)?,
        }
        let back = match ext {
            "sai2" => paint_formats::sai2_doc::load(&p)?.document,
            "psd" | "psb" => paint_formats::psd::load(&p)?,
            _ => paint_formats::ora::load(&p)?,
        };
        if back.layers.len() != 2 || !close(&back.flatten(), &want, 2) {
            return Err(format!("{ext}: прочитано не то, что записано").into());
        }
        done.push(ext);
    }
    let p = d.join("рисунок-16.psd");
    paint_formats::psd::save_with(&doc, &p, paint_formats::psd::SaveOptions { deep: true, ..Default::default() })?;
    if !close(&paint_formats::psd::load(&p)?.flatten(), &want, 2) {
        return Err("psd 16 бит: прочитано не то".into());
    }
    done.push("psd16");
    // сведённая картинка; JPEG — с потерями
    for ext in ["png", "bmp", "tga", "tiff", "webp", "qoi", "jpg"] {
        let p = d.join(format!("картинка.{ext}"));
        paint_core::io::save_image(&p, doc.width, doc.height, &want)?;
        let (w, h, px) = paint_core::io::load_image(&p)?;
        let tol = if ext == "jpg" { 40 } else { 1 };
        if (w, h) != (doc.width, doc.height) || !close(&px, &want, tol) {
            return Err(format!("{ext}: прочитано не то").into());
        }
        done.push(ext);
    }
    Ok(done.join(" "))
}
