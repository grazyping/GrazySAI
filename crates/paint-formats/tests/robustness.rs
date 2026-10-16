//! Устойчивость чтения к испорченным файлам (аудит, OWASP: вход от пользователя — недоверенный).
//! Каждый образец обрезается, портится по байту и по 4-байтным полям (0, 0x7FFFFFFF, 0xFFFFFFFF,
//! 0x80000000); чтение должно вернуть ошибку или документ — но не упасть, не зависнуть и не
//! выделить неразумно много памяти. Образцы — свои (создаются тут же) и, если задана папка
//! `GRAZYSAI_FUZZ_DIR`, файлы из неё (копии, не оригиналы пользователя). Запуск:
//! `cargo test --release -p paint-formats --test robustness -- --ignored --nocapture`
//! (`GRAZYSAI_FUZZ_N` — сколько порч на образец, по умолчанию 300).

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Детерминированный генератор (xorshift) — случаи повторяются от запуска к запуску.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

fn own_samples(dir: &Path) -> Vec<PathBuf> {
    let mut doc = paint_core::Document::new(70, 50);
    for y in 0..50 {
        for x in 0..70 {
            doc.layers[0].pixels.set(x, y, paint_core::pixel::from_rgba8([x as u8 * 3, y as u8 * 4, 90, if x > 50 { 128 } else { 255 }]));
        }
    }
    let mut out = Vec::new();
    for ext in ["png", "bmp", "tga", "tiff", "webp", "qoi"] {
        let p = dir.join(format!("own.{ext}"));
        let (w, h) = (doc.width, doc.height);
        if paint_core::io::save_image(&p, w, h, &doc.flatten()).is_ok() {
            out.push(p);
        }
    }
    let p = dir.join("own.ora");
    paint_formats::ora::save(&doc, &p).unwrap();
    out.push(p);
    let p = dir.join("own.sai2");
    paint_formats::sai2_write::save(&doc, &p).unwrap();
    out.push(p);
    let p = dir.join("own.psd");
    if paint_formats::psd::save(&doc, &p).is_ok() {
        out.push(p);
    }
    out
}

/// Прочитать файл всем, чем программа его читает (по расширению), и миниатюрой обозревателя.
fn read_all(path: &Path) {
    let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "sai2" => {
            let _ = paint_formats::sai2_doc::load(path);
        }
        "psd" => {
            let _ = paint_formats::psd::load(path);
        }
        "ora" => {
            let _ = paint_formats::ora::load(path);
        }
        _ => {
            let _ = paint_core::io::load_image(path);
        }
    }
    let _ = paint_formats::preview::thumbnail(path, 128);
}

/// Все случаи одного образца: обрезка, порча битов, порча 4-байтных полей.
fn run_sample(sample: &Path, k: usize, n: usize, work: &Path, last: &dyn Fn() -> String) -> Vec<String> {
    let orig = std::fs::read(sample).unwrap();
    let ext = sample.extension().unwrap().to_string_lossy().to_string();
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15 ^ orig.len() as u64);
    let mut cases: Vec<(String, Vec<u8>)> = Vec::new();
    for k in 0..48 {
        let at = orig.len() * k / 48;
        cases.push((format!("обрезка до {at}"), orig[..at].to_vec()));
    }
    for _ in 0..n {
        let mut b = orig.clone();
        let i = rng.below(b.len());
        b[i] ^= 1 << rng.below(8);
        cases.push((format!("бит в {i}"), b));
    }
    for _ in 0..n / 3 {
        let mut b = orig.clone();
        let i = rng.below(b.len().saturating_sub(4));
        let v: u32 = [0, 0x7FFF_FFFF, 0xFFFF_FFFF, 0x8000_0000][rng.below(4)];
        b[i..i + 4].copy_from_slice(&v.to_le_bytes());
        if rng.below(2) == 0 {
            b[i..i + 4].reverse(); // и в обратном порядке байтов (PSD — big-endian)
        }
        cases.push((format!("поле {i} = {v:#x}"), b));
    }
    let name = sample.file_name().unwrap().to_string_lossy().to_string();
    let log = work.join(format!("last-case-{k}.txt"));
    let p = work.join(format!("case-{k}.{ext}"));
    let started = Instant::now();
    let mut problems = Vec::new();
    for (what, bytes) in cases {
        std::fs::write(&p, &bytes).unwrap();
        // если процесс упадёт (нехватка памяти), здесь останется последний случай
        std::fs::write(&log, format!("{name}: {what}\n")).unwrap();
        let t = Instant::now();
        let r = std::panic::catch_unwind(|| read_all(&p));
        let took = t.elapsed();
        if r.is_err() {
            problems.push(format!("ПАДЕНИЕ {name}: {what}: {}", last()));
        } else if took > Duration::from_secs(5) {
            problems.push(format!("ДОЛГО {:.1} с {name}: {what}", took.as_secs_f32()));
        }
    }
    eprintln!("{name}: {:.1} с", started.elapsed().as_secs_f32());
    problems
}

#[test]
#[ignore]
fn corrupted_files_do_not_crash() {
    let work = std::env::temp_dir().join(format!("grazysai-robust-{}", std::process::id()));
    std::fs::create_dir_all(&work).unwrap();
    let mut samples = own_samples(&work);
    if let Some(d) = std::env::var_os("GRAZYSAI_FUZZ_DIR") {
        let mut extra: Vec<PathBuf> = std::fs::read_dir(d).unwrap().flatten().map(|e| e.path()).filter(|p| paint_formats::preview::supported(p)).collect();
        extra.sort();
        samples.extend(extra);
    }
    // место падения — в находки, а не в общий вывод (у каждого потока своё)
    thread_local!(static LAST: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) });
    std::panic::set_hook(Box::new(|info| {
        let msg = info.to_string().replace('\n', " ");
        LAST.with(|l| *l.borrow_mut() = msg);
    }));
    let n: usize = std::env::var("GRAZYSAI_FUZZ_N").ok().and_then(|v| v.parse().ok()).unwrap_or(300);
    // образцы — параллельно, по потоку на образец (у каждого свой файл случая и свой журнал)
    let next = std::sync::atomic::AtomicUsize::new(0);
    let problems = std::sync::Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..paint_core::threads().min(samples.len()) {
            scope.spawn(|| loop {
                let k = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let Some(sample) = samples.get(k) else { break };
                let found = run_sample(sample, k, n, &work, &|| LAST.with(|l| l.borrow().clone()));
                problems.lock().unwrap().extend(found);
            });
        }
    });
    let problems = problems.into_inner().unwrap();
    std::fs::remove_dir_all(&work).ok();
    for p in &problems {
        eprintln!("{p}");
    }
    assert!(problems.is_empty(), "{} проблем", problems.len());
}

/// Разбор одного файла (для отладки находок): `GRAZYSAI_REPRO=путь`.
#[test]
#[ignore]
fn repro_one() {
    if let Some(p) = std::env::var_os("GRAZYSAI_REPRO") {
        read_all(Path::new(&p));
    }
}

/// Стартовый набор для фаззера: свои образцы в папку `GRAZYSAI_SEED_DIR/<цель>/` (картинки — с
/// байтом выбора формата впереди, как ждёт цель `image`).
#[test]
#[ignore]
fn write_fuzz_seeds() {
    let Some(dir) = std::env::var_os("GRAZYSAI_SEED_DIR") else { return };
    let dir = PathBuf::from(dir);
    let work = std::env::temp_dir().join(format!("grazysai-seeds-{}", std::process::id()));
    std::fs::create_dir_all(&work).unwrap();
    for p in own_samples(&work) {
        let ext = p.extension().unwrap().to_string_lossy().to_string();
        let bytes = std::fs::read(&p).unwrap();
        let (target, data) = match ext.as_str() {
            "sai2" | "psd" | "ora" => (ext.clone(), bytes),
            e => {
                let k = ["png", "jpg", "bmp", "tga", "tiff", "webp", "qoi"].iter().position(|x| *x == e).unwrap() as u8;
                ("image".to_string(), [vec![k], bytes].concat())
            }
        };
        std::fs::create_dir_all(dir.join(&target)).unwrap();
        std::fs::write(dir.join(&target).join(format!("own-{ext}")), data).unwrap();
    }
    std::fs::remove_dir_all(work).ok();
}
