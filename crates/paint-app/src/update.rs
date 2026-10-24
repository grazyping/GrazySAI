//! Обновление программы с выпусков на GitHub — без своего сервера.
//!
//! Проверка: запрос `releases/latest` к API GitHub (без ключа, лимит 60 запросов в час с адреса —
//! раз за запуск с запасом). Загрузка и распаковка — системными `curl` и `tar`: они есть в
//! Windows 10 (1803 и новее) и почти в любом Linux, так что TLS и архиваторы в программу не
//! тянем. Скачанный архив сверяется с SHA-256 из ответа API, затем новый исполняемый файл встаёт
//! на место старого: в Windows работающий `GrazySAI.exe` можно переименовать (в `GrazySAI.exe.old`,
//! удаляется при следующем запуске), в Linux файл заменяется переименованием поверх. Новая
//! версия начинает работать после перезапуска. Нет прав на папку программы, нет `curl` —
//! предлагается страница выпуска.
//!
//! Для проверок: `PAINT_FAKE_VERSION=0.8.0` — считать себя этой версией; `PAINT_UPDATE_EXE=путь` —
//! ставить новый файл туда, а не поверх себя; `PAINT_UPDATE_CHECK=off` — не проверять при запуске.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

pub const REPO: &str = "https://github.com/grazyping/GrazySAI";
const API: &str = "https://api.github.com/repos/grazyping/GrazySAI/releases/latest";

#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub version: String,
    pub page: String,
    pub notes: String,
    /// Архив для этой системы и его SHA-256 (если GitHub его сообщил).
    pub asset: Option<(String, Option<String>)>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum State {
    Idle,
    Checking,
    UpToDate,
    Available(Release),
    Downloading(Release),
    /// Новая версия на месте, заработает после перезапуска.
    Installed(Release),
    Failed(String, Option<Release>),
}

pub type Shared = Arc<Mutex<State>>;

pub fn current() -> String {
    std::env::var("PAINT_FAKE_VERSION").unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string())
}

/// `0.8.10` > `0.8.9`; лишние части и `v` в начале допускаются.
pub fn newer(candidate: &str, than: &str) -> bool {
    let parse = |s: &str| -> Vec<u64> {
        s.trim_start_matches('v').split(['.', '-']).map_while(|p| p.parse().ok()).collect()
    };
    parse(candidate) > parse(than)
}

/// Конец имени архива своей сборки: Windows 10+, Windows 7/8 (цель `win7`), FreeBSD, Linux.
fn suffix() -> &'static str {
    if cfg!(target_vendor = "win7") {
        "windows7-x86_64.zip"
    } else if cfg!(windows) {
        "windows-x86_64.zip"
    } else if cfg!(target_os = "freebsd") {
        "freebsd-x86_64.tar.gz"
    } else {
        "linux-x86_64.tar.gz"
    }
}

fn exe_name() -> &'static str {
    if cfg!(windows) {
        "GrazySAI.exe"
    } else {
        "GrazySAI"
    }
}

/// Системная программа: в Windows — из System32 (Git и прочие кладут в PATH свои `tar` без zip).
fn tool(name: &str) -> Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let sys = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        let full = PathBuf::from(sys).join("System32").join(format!("{name}.exe"));
        let mut c = Command::new(if full.exists() { full } else { PathBuf::from(name) });
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: без мелькающей консоли
        c
    }
    #[cfg(not(windows))]
    Command::new(name)
}

fn run(mut c: Command) -> Result<Vec<u8>, String> {
    let out = c.output().map_err(|e| format!("{:?}: {e}", c.get_program()))?;
    if out.status.success() {
        Ok(out.stdout)
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        Err(format!("{:?}: {}", c.get_program(), err.lines().last().unwrap_or("").trim()))
    }
}

/// Открыть адрес в браузере.
pub fn open_url(url: &str) {
    #[cfg(windows)]
    let c = {
        let mut c = tool("rundll32");
        c.args(["url.dll,FileProtocolHandler", url]);
        c
    };
    #[cfg(not(windows))]
    let c = {
        let mut c = Command::new("xdg-open");
        c.arg(url);
        c
    };
    let mut c = c;
    let _ = c.spawn();
}

/// Проверить в фоне; по окончании — перерисовка окна.
pub fn check(state: &Shared, ctx: &eframe::egui::Context) {
    *state.lock().unwrap() = State::Checking;
    let (state, ctx) = (state.clone(), ctx.clone());
    std::thread::spawn(move || {
        let r = fetch_latest();
        *state.lock().unwrap() = match r {
            Ok(rel) if newer(&rel.version, &current()) => State::Available(rel),
            Ok(_) => State::UpToDate,
            Err(e) => State::Failed(e, None),
        };
        ctx.request_repaint();
    });
}

fn fetch_latest() -> Result<Release, String> {
    let mut c = tool("curl");
    // только HTTPS, и при перенаправлениях тоже
    c.args(["-fsSL", "--proto", "=https", "--proto-redir", "=https", "--max-time", "20", "-H", "Accept: application/vnd.github+json", API]);
    let body = String::from_utf8_lossy(&run(c)?).into_owned();
    parse_release(&body).ok_or_else(|| "GitHub: unexpected answer".into())
}

/// Разбор ответа `releases/latest`: нужны лишь несколько строковых полей.
pub fn parse_release(body: &str) -> Option<Release> {
    let tag = json_str(body, "tag_name", 0)?.0;
    let version = tag.trim_start_matches('v').to_string();
    let page = json_str(body, "html_url", 0).map(|v| v.0).unwrap_or_else(|| format!("{REPO}/releases"));
    let notes = json_str(body, "body", 0).map(|v| v.0).unwrap_or_default();
    let name = format!("GrazySAI-{version}-{}", suffix());
    let asset = body.find(&format!("\"name\":\"{name}\"")).or_else(|| body.find(&format!("\"name\": \"{name}\""))).and_then(|at| {
        let (url, end) = json_str(body, "browser_download_url", at)?;
        let digest = json_str(&body[..end], "digest", at)
            .and_then(|d| d.0.strip_prefix("sha256:").map(str::to_string));
        Some((url, digest))
    });
    Some(Release { version, page, notes, asset })
}

/// Значение строкового поля `"key": "…"` начиная с `from`; вместе с концом значения.
fn json_str(body: &str, key: &str, from: usize) -> Option<(String, usize)> {
    let pat = format!("\"{key}\"");
    let mut at = from;
    loop {
        let k = body.get(at..)?.find(&pat)? + at + pat.len();
        let rest = body[k..].trim_start();
        let Some(rest) = rest.strip_prefix(':') else {
            at = k;
            continue;
        };
        let rest = rest.trim_start();
        let start = body.len() - rest.len();
        let rest = rest.strip_prefix('"')?;
        let mut out = String::new();
        let mut chars = rest.char_indices();
        while let Some((i, ch)) = chars.next() {
            match ch {
                '"' => return Some((out, start + 1 + i + 1)),
                '\\' => match chars.next()?.1 {
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    'r' => {}
                    'u' => {
                        let hex: String = (0..4).filter_map(|_| chars.next().map(|c| c.1)).collect();
                        let mut code = u32::from_str_radix(&hex, 16).ok()?;
                        if (0xD800..0xDC00).contains(&code) {
                            // суррогатная пара: \uD83D\uDE00
                            let low: String = (0..6).filter_map(|_| chars.next().map(|c| c.1)).collect();
                            let low = u32::from_str_radix(low.get(2..)?, 16).ok()?;
                            code = 0x10000 + ((code - 0xD800) << 10) + (low - 0xDC00);
                        }
                        out.extend(char::from_u32(code));
                    }
                    c => out.push(c),
                },
                c => out.push(c),
            }
        }
        return None;
    }
}

/// Раздел заметок выпуска на языке интерфейса (заметки пишутся с «## English» и «## Русский»).
pub fn notes_for(notes: &str, lang: &str) -> String {
    let want = if matches!(lang, "ru" | "uk") { "## Русский" } else { "## English" }; // не перевод
    let Some(at) = notes.find(want) else { return notes.trim().to_string() };
    let part = &notes[at + want.len()..];
    let end = part.find("\n## ").or_else(|| part.find("\n### ")).unwrap_or(part.len());
    part[..end].trim().to_string()
}

/// Скачать, сверить, распаковать и поставить на место — в фоне.
pub fn install(state: &Shared, rel: Release, ctx: &eframe::egui::Context) {
    *state.lock().unwrap() = State::Downloading(rel.clone());
    let (state, ctx) = (state.clone(), ctx.clone());
    std::thread::spawn(move || {
        let r = download_and_replace(&rel);
        *state.lock().unwrap() = match r {
            Ok(()) => State::Installed(rel),
            Err(e) => State::Failed(e, Some(rel)),
        };
        ctx.request_repaint();
    });
}

fn target_exe() -> Result<PathBuf, String> {
    if let Ok(p) = std::env::var("PAINT_UPDATE_EXE") {
        return Ok(p.into());
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    // в Linux после замены файла путь работающей программы — «…/GrazySAI (deleted)»
    let exe = match exe.to_str().and_then(|s| s.strip_suffix(" (deleted)")) {
        Some(s) => PathBuf::from(s),
        None => exe,
    };
    Ok(exe.canonicalize().unwrap_or(exe))
}

fn download_and_replace(rel: &Release) -> Result<(), String> {
    let (url, digest) = rel.asset.clone().ok_or("no archive for this system")?;
    let exe = target_exe()?;
    let dir = exe.parent().ok_or("no program folder")?.to_path_buf();
    // рабочая папка рядом с программой: заодно проверка, что в папку можно писать
    let work = dir.join(".grazysai-update");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| format!("{}: {e}", dir.display()))?;
    let r = (|| {
        // архив — только из выпусков нашего репозитория и только с суммой SHA-256 от GitHub:
        // без неё подлинность не проверить, и программа не подменяется
        if !url.starts_with(&format!("{REPO}/releases/download/")) {
            return Err(format!("unexpected download address: {url}"));
        }
        let want = digest.ok_or("GitHub gave no SHA-256 for the archive")?;
        // имя файла — своё, не из ответа сервера
        let archive = work.join(if cfg!(windows) { "update.zip" } else { "update.tar.gz" });
        let mut c = tool("curl");
        c.args(["-fsSL", "--proto", "=https", "--proto-redir", "=https", "--max-time", "600", "-o"]).arg(&archive).arg(&url);
        run(c)?;
        let data = std::fs::read(&archive).map_err(|e| e.to_string())?;
        let got = sha256_hex(&data);
        if !got.eq_ignore_ascii_case(&want) {
            return Err(format!("SHA-256 mismatch: {got}"));
        }
        let mut c = tool("tar");
        c.arg(if cfg!(windows) { "-xf" } else { "-xzf" }).arg(&archive).arg("-C").arg(&work);
        run(c)?;
        let folder = work.join(format!("GrazySAI-{}-{}", rel.version, suffix().split('.').next().unwrap_or("")));
        let new_exe = folder.join(exe_name());
        if !new_exe.is_file() {
            return Err(format!("{} not in archive", exe_name()));
        }
        replace(&new_exe, &exe)?;
        // справка рядом с программой — тоже новая (только те файлы, что уже лежат рядом);
        // мастерская GrazySAI Studio — всегда (с 1.0.31 она в архиве)
        let studio = if cfg!(windows) { "GrazySAI-Studio.exe" } else { "GrazySAI-Studio" };
        for entry in std::fs::read_dir(&folder).map_err(|e| e.to_string())?.flatten() {
            let p = entry.path();
            let name = entry.file_name();
            if p != new_exe && (dir.join(&name).is_file() || name == studio) {
                let _ = std::fs::copy(&p, dir.join(&name));
            }
        }
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&work);
    r
}

/// Новый файл на место работающего; при ошибке старый остаётся.
fn replace(new_exe: &Path, exe: &Path) -> Result<(), String> {
    let staged = exe.with_file_name(format!("{}.new", exe_name()));
    std::fs::copy(new_exe, &staged).map_err(|e| format!("{}: {e}", staged.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755));
    }
    #[cfg(windows)]
    {
        let old = old_exe(exe);
        let _ = std::fs::remove_file(&old);
        std::fs::rename(exe, &old).map_err(|e| format!("{}: {e}", exe.display()))?;
        if let Err(e) = std::fs::rename(&staged, exe) {
            let _ = std::fs::rename(&old, exe);
            return Err(format!("{}: {e}", exe.display()));
        }
    }
    #[cfg(not(windows))]
    std::fs::rename(&staged, exe).map_err(|e| format!("{}: {e}", exe.display()))?;
    Ok(())
}

#[cfg(windows)]
fn old_exe(exe: &Path) -> PathBuf {
    exe.with_file_name(format!("{}.old", exe_name()))
}

/// После обновления в Windows остаётся `GrazySAI.exe.old` — убрать при следующем запуске.
pub fn cleanup() {
    #[cfg(windows)]
    if let Ok(exe) = target_exe() {
        let _ = std::fs::remove_file(old_exe(&exe));
    }
}

/// Запустить новую версию с теми же аргументами (окно этой закрывает вызывающий).
pub fn restart() -> bool {
    let Ok(exe) = target_exe() else { return false };
    Command::new(exe).args(std::env::args_os().skip(1)).spawn().is_ok()
}

/// SHA-256 (FIPS 180-4) — чтобы не тянуть зависимость ради одной сверки.
pub fn sha256_hex(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
        0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
        0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
        0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
        0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    ];
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    for block in msg.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes(block[i * 4..i * 4 + 4].try_into().unwrap());
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let mut v = h;
        for i in 0..64 {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let ch = (v[4] & v[5]) ^ (!v[4] & v[6]);
            let t1 = v[7].wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let t2 = s0.wrapping_add(maj);
            v = [t1.wrapping_add(t2), v[0], v[1], v[2], v[3].wrapping_add(t1), v[4], v[5], v[6]];
        }
        for (a, b) in h.iter_mut().zip(v) {
            *a = a.wrapping_add(b);
        }
    }
    h.iter().map(|x| format!("{x:08x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_known_values() {
        assert_eq!(sha256_hex(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        let long = b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq";
        assert_eq!(sha256_hex(long), "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1");
    }

    #[test]
    fn versions() {
        assert!(newer("v0.8.10", "0.8.9"));
        assert!(newer("0.9.0", "0.8.5"));
        assert!(!newer("0.8.5", "0.8.5"));
        assert!(!newer("0.8.4", "0.8.5"));
    }

    #[test]
    fn parses_github_answer() {
        let n = format!("GrazySAI-0.9.0-{}", suffix());
        let body = format!(
            r#"{{"html_url":"https://github.com/grazyping/GrazySAI/releases/tag/v0.9.0","tag_name":"v0.9.0",
            "assets":[{{"name":"other.zip","digest":null,"browser_download_url":"https://x/other.zip"}},
            {{"name":"{n}","digest":"sha256:abc","browser_download_url":"https://x/{n}"}}],
            "body":"**GrazySAI 0.9.0**\r\n\r\n## English\r\n- \u0022New\u0022 \u2014 thing\r\n\r\n## Русский\r\n- Новое\r\n\r\n### Download"}}"#
        );
        let r = parse_release(&body).unwrap();
        assert_eq!(r.version, "0.9.0");
        assert_eq!(r.page, "https://github.com/grazyping/GrazySAI/releases/tag/v0.9.0");
        assert_eq!(r.asset, Some((format!("https://x/{n}"), Some("abc".into()))));
        assert_eq!(notes_for(&r.notes, "en"), "- \"New\" — thing");
        assert_eq!(notes_for(&r.notes, "ru"), "- Новое");
    }

    /// Настоящее обновление с GitHub во временный файл (сеть): адрес, SHA-256, распаковка, подмена.
    #[test]
    #[ignore]
    fn real_update_downloads_and_verifies() {
        let dir = std::env::temp_dir().join(format!("grazysai-upd-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join(exe_name());
        std::fs::write(&target, b"old").unwrap();
        std::env::set_var("PAINT_UPDATE_EXE", &target);
        let rel = fetch_latest().expect("последний выпуск");
        download_and_replace(&rel).expect("обновление");
        assert!(std::fs::metadata(&target).unwrap().len() > 1_000_000, "подменён настоящей программой");
        std::fs::remove_dir_all(dir).ok();
    }
}
