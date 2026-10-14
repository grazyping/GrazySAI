//! Проверка пера: окно для рисования и журнал всех событий пера в CSV рядом с программой.
//!
//! Windows:
//!   pen-test.exe            — события через Windows Ink (сообщения WM_POINTER)
//!   pen-test.exe --wintab   — события через WinTab (как у образца)
//! Linux:
//!   pen-test                — события через X11 (XInput 2); в сеансе Wayland работает через XWayland
//! Везде:
//!   --selftest out.png      — без окна: проверка, что рисование и журнал работают
//!   --autoclose N           — закрыть окно через N секунд
//!
//! Клавиши: C — очистить, Z — отменить, S — сохранить PNG, 1/2/3 — размер кисти, Esc — выход.

#![cfg_attr(windows, windows_subsystem = "windows")]

mod app;
#[cfg(windows)]
mod win;
#[cfg(target_os = "linux")]
mod x11;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--selftest") {
        let out = args.get(i + 1).cloned().unwrap_or_else(|| "selftest.png".into());
        if let Err(e) = app::selftest(&out) {
            let _ = std::fs::write(app::exe_dir().join("pen-test-crash.txt"), format!("selftest: {e}\n"));
            eprintln!("самопроверка не прошла: {e}");
            std::process::exit(1);
        }
        return;
    }
    let autoclose = args.iter().position(|a| a == "--autoclose").and_then(|i| args.get(i + 1)).and_then(|s| s.parse::<u32>().ok());
    #[cfg(windows)]
    win::run(args.iter().any(|a| a == "--wintab"), autoclose);
    #[cfg(target_os = "linux")]
    if let Err(e) = x11::run(autoclose) {
        eprintln!("ошибка: {e}");
        std::process::exit(1);
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = autoclose;
        eprintln!("pen-test: эта система не поддерживается");
    }
}
