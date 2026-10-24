//! Признак `xdg` (Linux и FreeBSD) — для общих с рисовалкой файлов (`paint-app/src/files.rs` и др.).

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo::rustc-check-cfg=cfg(xdg)");
    if matches!(std::env::var("CARGO_CFG_TARGET_OS").as_deref(), Ok("linux" | "freebsd")) {
        println!("cargo:rustc-cfg=xdg");
    }
}
