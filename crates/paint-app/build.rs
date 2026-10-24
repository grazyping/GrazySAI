//! Windows: таблица экспорта программы. Драйверы NVIDIA (Optimus) и AMD (PowerXpress) ищут в ней
//! `NvOptimusEnablement` и `AmdPowerXpressRequestHighPerformance` и тогда запускают программу
//! на дискретной видеокарте (на ноутбуках с двумя картами иначе выбирается встроенная).
//! Rust сам из исполняемого файла ничего не экспортирует — нужен `.def`-файл для компоновщика.
//!
//! Linux и FreeBSD: признак `xdg` — рабочий стол X11 / Wayland с правилами freedesktop (перо
//! XInput 2, корзина, системные окна выбора файла).

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo::rustc-check-cfg=cfg(xdg)");
    if matches!(std::env::var("CARGO_CFG_TARGET_OS").as_deref(), Ok("linux" | "freebsd")) {
        println!("cargo:rustc-cfg=xdg");
    }
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let def = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("exports.def");
    std::fs::write(&def, "EXPORTS\n    NvOptimusEnablement DATA\n    AmdPowerXpressRequestHighPerformance DATA\n").unwrap();
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg-bins=/DEF:{}", def.display());
    } else {
        // MinGW (ld и lld) принимают .def как обычный входной файл
        println!("cargo:rustc-link-arg-bins={}", def.display());
    }
}
