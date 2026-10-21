//! Языки интерфейса. Исходные подписи в коде — русские; они же ключи переводов.
//!
//! `tr("Слой")` возвращает подпись на выбранном языке, `trf("Холст {0}×{1}", &[&w, &h])` —
//! то же с подстановкой. Переводы лежат в `lang/<код>.txt` строками `ключ = перевод`
//! (`#` — комментарий, `\n` — перенос строки). Нет перевода — берётся английский, нет и его —
//! русский ключ. Тест `every_key_is_translated` проверяет, что все ключи из кода переведены
//! во всех файлах.
//!
//! Японскому, китайскому и корейскому нужен шрифт с иероглифами: встроенные шрифты egui их
//! не содержат, поэтому `install_fonts` подгружает системный (Noto CJK, Yu Gothic, Microsoft
//! YaHei, Malgun Gothic…).

use eframe::egui;
use std::collections::HashMap;
use std::fmt::Display;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

/// Код, название на самом языке, файл переводов (`None` — русский, язык ключей).
pub const LANGS: [(&str, &str, Option<&str>); 10] = [
    ("en", "English", Some(include_str!("../lang/en.txt"))),
    ("ru", "Русский", None),
    ("uk", "Українська", Some(include_str!("../lang/uk.txt"))),
    ("de", "Deutsch", Some(include_str!("../lang/de.txt"))),
    ("fr", "Français", Some(include_str!("../lang/fr.txt"))),
    ("es", "Español", Some(include_str!("../lang/es.txt"))),
    ("pt", "Português (Brasil)", Some(include_str!("../lang/pt.txt"))),
    ("ja", "日本語", Some(include_str!("../lang/ja.txt"))),
    ("zh", "简体中文", Some(include_str!("../lang/zh.txt"))),
    ("ko", "한국어", Some(include_str!("../lang/ko.txt"))),
];

const EN: usize = 0;
const RU: usize = 1;

static CURRENT: AtomicUsize = AtomicUsize::new(EN);

type Table = HashMap<&'static str, &'static str>;

fn tables() -> &'static Vec<Table> {
    static T: OnceLock<Vec<Table>> = OnceLock::new();
    T.get_or_init(|| LANGS.iter().map(|(_, _, text)| text.map(parse).unwrap_or_default()).collect())
}

/// Разобрать файл переводов. Строки с `\n` хранятся один раз на всё время работы.
fn parse(text: &'static str) -> Table {
    let mut t = Table::new();
    for line in text.lines() {
        if line.trim_start().starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once(" = ") else { continue };
        t.insert(unescape(k.trim()), unescape(v.trim()));
    }
    t
}

fn unescape(s: &'static str) -> &'static str {
    if s.contains("\\n") {
        Box::leak(s.replace("\\n", "\n").into_boxed_str())
    } else {
        s
    }
}

/// Выбранный язык (индекс в `LANGS`).
pub fn current() -> usize {
    CURRENT.load(Ordering::Relaxed)
}

pub fn code() -> &'static str {
    LANGS[current()].0
}

pub fn set(code: &str) {
    if let Some(i) = LANGS.iter().position(|(c, _, _)| *c == code) {
        CURRENT.store(i, Ordering::Relaxed);
    }
}

/// Язык системы, если он есть среди наших; иначе английский.
pub fn system() -> &'static str {
    let tag = system_locale().unwrap_or_default().to_lowercase();
    let primary = tag.split(['-', '_', '.', '@']).next().unwrap_or("");
    // белорусский и казахский интерфейс чаще читают по-русски
    let primary = match primary {
        "be" | "kk" => "ru",
        p => p,
    };
    LANGS.iter().find(|(c, _, _)| *c == primary).map_or("en", |(c, _, _)| c)
}

#[cfg(windows)]
fn system_locale() -> Option<String> {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetUserDefaultLocaleName(name: *mut u16, len: i32) -> i32;
    }
    let mut buf = [0u16; 85];
    // SAFETY: буфер на LOCALE_NAME_MAX_LENGTH знаков, функция пишет не больше `len`.
    let n = unsafe { GetUserDefaultLocaleName(buf.as_mut_ptr(), buf.len() as i32) };
    (n > 1).then(|| String::from_utf16_lossy(&buf[..n as usize - 1]))
}

#[cfg(not(windows))]
fn system_locale() -> Option<String> {
    ["LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"]
        .iter()
        .filter_map(|v| std::env::var(v).ok())
        .find(|v| !v.is_empty() && v != "C" && v != "POSIX")
}

/// Подпись на выбранном языке. Пробелы в начале (отступ в панели) сохраняются.
pub fn tr(key: &'static str) -> &'static str {
    let lang = current();
    if lang == RU {
        return key;
    }
    let body = key.trim_start();
    let found = |l: usize| tables()[l].get(body).copied();
    let Some(text) = found(lang).or_else(|| found(EN)) else { return key };
    let indent = &key[..key.len() - body.len()];
    if indent.is_empty() {
        return text;
    }
    static INDENTED: OnceLock<Mutex<HashMap<(usize, &'static str), &'static str>>> = OnceLock::new();
    let mut cache = INDENTED.get_or_init(Default::default).lock().unwrap();
    cache.entry((lang, key)).or_insert_with(|| Box::leak(format!("{indent}{text}").into_boxed_str()))
}

/// Подпись с подстановкой `{0}`, `{1}`… (порядок в переводе может быть другим).
pub fn trf(key: &'static str, args: &[&dyn Display]) -> String {
    let mut s = tr(key).to_string();
    for (i, a) in args.iter().enumerate() {
        s = s.replace(&format!("{{{i}}}"), &a.to_string());
    }
    s
}

/// Подгрузить системный шрифт с иероглифами для выбранного языка (или для любого, если
/// язык не азиатский — чтобы имена слоёв из японских файлов не были квадратиками).
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    if let Some((data, index)) = cjk_font(code()) {
        let mut fd = egui::FontData::from_owned(data);
        fd.index = index;
        // Иероглифы крупнее и плотнее светлой латиницы интерфейса — чуть уменьшаем.
        let scale = std::env::var("PAINT_CJK_SCALE").ok().and_then(|v| v.parse().ok()).unwrap_or(CJK_SCALE);
        fd.tweak = egui::FontTweak { scale, ..Default::default() };
        fonts.font_data.insert("cjk".into(), fd);
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            fonts.families.entry(family).or_default().push("cjk".into());
        }
    }
    ctx.set_fonts(fonts);
}

const CJK_SCALE: f32 = 0.94;

/// Файл шрифта и номер начертания в коллекции `.ttc`. Берём светлые начертания: шрифт
/// интерфейса (Ubuntu Light) тонкий, обычные иероглифы рядом с ним выглядят жирно.
fn cjk_font(lang: &str) -> Option<(Vec<u8>, u32)> {
    // В NotoSansCJK.ttc начертания идут JP, KR, SC, TC, HK — порядок важен для вида иероглифов.
    let noto = match lang {
        "ko" => 1,
        "zh" => 2,
        _ => 0,
    };
    let mut candidates: Vec<(String, u32)> = Vec::new();
    if cfg!(windows) {
        let dir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into()) + "\\Fonts\\";
        let names: &[&str] = match lang {
            "zh" => &["msyhl.ttc", "msyh.ttc", "simsun.ttc", "YuGothR.ttc", "malgun.ttf"],
            "ko" => &["malgunsl.ttf", "malgun.ttf", "gulim.ttc", "msyh.ttc", "YuGothR.ttc"],
            _ => &["YuGothL.ttc", "YuGothR.ttc", "YuGothM.ttc", "meiryo.ttc", "msgothic.ttc", "msyh.ttc", "malgun.ttf"],
        };
        candidates.extend(names.iter().map(|n| (format!("{dir}{n}"), 0)));
    } else {
        let weight = std::env::var("PAINT_CJK_WEIGHT").unwrap_or_else(|_| "Light".into());
        for w in [weight.as_str(), "Light", "DemiLight", "Regular"] {
            for d in ["/usr/share/fonts/opentype/noto", "/usr/share/fonts/noto-cjk", "/usr/share/fonts/google-noto-cjk", "/usr/share/fonts/noto"] {
                candidates.push((format!("{d}/NotoSansCJK-{w}.ttc"), noto));
            }
        }
        candidates.push(("/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf".into(), 0));
        // что подскажет fontconfig
        let pattern = match lang {
            "zh" => ":lang=zh-cn",
            "ko" => ":lang=ko",
            _ => ":lang=ja",
        };
        if let Ok(out) = std::process::Command::new("fc-match").args(["-f", "%{file}\n%{index}", pattern]).output() {
            let text = String::from_utf8_lossy(&out.stdout).to_string();
            let mut it = text.lines();
            if let (Some(f), i) = (it.next(), it.next().and_then(|i| i.parse().ok()).unwrap_or(0)) {
                candidates.push((f.to_string(), i));
            }
        }
    }
    candidates.into_iter().find_map(|(p, i)| std::fs::read(p).ok().map(|d| (d, i)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Все строки с кириллицей в коде приложения — это ключи (кроме комментариев, тестов и
    /// вывода в терминал). Так тест ловит и подписи, которые переводятся при показе: режимы,
    /// кисти, действия клавиш.
    fn keys_in_code() -> Vec<String> {
        // рисовалка, мастерская и общая библиотека кистей: переводы у них одни (модуль тот же)
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/..");
        let dirs = ["paint-app/src", "paint-studio/src", "paint-brush/src"].map(|d| format!("{root}/{d}"));
        let mut keys = Vec::new();
        for e in dirs.iter().filter_map(|d| std::fs::read_dir(d).ok()).flatten() {
            let path = e.unwrap().path();
            // отчёт самопроверки — для разработчика, по-русски и по-английски сразу, без перевода
            if path.ends_with("i18n.rs") || path.ends_with("self_test.rs") {
                continue;
            }
            let src = std::fs::read_to_string(path).unwrap();
            let src = src.split("#[cfg(test)]").next().unwrap();
            for line in src.lines() {
                let code = line.trim_start();
                if code.starts_with("//") || code.starts_with("eprintln!") || code.contains("eprintln!(\"") || line.ends_with("// не перевод") {
                    continue;
                }
                let mut rest = line;
                while let Some(a) = rest.find('"') {
                    if rest[..a].contains("//") {
                        break;
                    }
                    let body = &rest[a + 1..];
                    let Some(b) = body.find('"') else { break };
                    let lit = &body[..b];
                    if lit.chars().any(|c| ('а'..='я').contains(&c.to_lowercase().next().unwrap()) || c == 'ё') {
                        keys.push(lit.trim_start().to_string());
                    }
                    rest = &body[b + 1..];
                }
            }
        }
        keys.sort();
        keys.dedup();
        keys
    }

    fn placeholders(s: &str) -> Vec<String> {
        let mut v: Vec<String> = (0..10).map(|i| format!("{{{i}}}")).filter(|p| s.contains(p.as_str())).collect();
        v.sort();
        v
    }

    #[test]
    fn every_key_is_translated() {
        let keys = keys_in_code();
        assert!(keys.len() > 200, "ключей мало: {}", keys.len());
        let mut problems = Vec::new();
        for (l, (code, _, text)) in LANGS.iter().enumerate() {
            if text.is_none() {
                continue;
            }
            let t = &tables()[l];
            for k in &keys {
                let k = unescape(Box::leak(k.clone().into_boxed_str()));
                match t.get(k) {
                    None => problems.push(format!("{code}: нет перевода «{k}»")),
                    Some(v) if placeholders(v) != placeholders(k) => {
                        problems.push(format!("{code}: подстановки не совпадают «{k}» → «{v}»"))
                    }
                    _ => {}
                }
            }
            for k in t.keys() {
                if !keys.iter().any(|x| unescape(Box::leak(x.clone().into_boxed_str())) == *k) {
                    problems.push(format!("{code}: лишний ключ «{k}»"));
                }
            }
        }
        assert!(problems.is_empty(), "{} проблем:\n{}", problems.len(), problems.join("\n"));
    }

    #[test]
    fn indent_and_substitution() {
        set("en");
        assert_eq!(tr("Кисть"), "Brush");
        assert_eq!(tr("   Ширина"), "   Width");
        assert_eq!(trf("Холст {0}×{1}", &[&10, &20]), "Canvas 10×20");
        set("ru");
        assert_eq!(trf("Холст {0}×{1}", &[&10, &20]), "Холст 10×20");
        set("en");
    }
}
