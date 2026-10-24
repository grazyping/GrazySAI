# Systems

**English** | [Русский](43-systems.md)

Where GrazySAI runs, how it is built and what has been checked (October 2026).

## Summary

| System | Build | Checked |
|---|---|---|
| Linux x86_64 (X11, Wayland via XWayland) | main, `cargo build --release` | developer machine (KDE Plasma), hidden Xvfb display |
| Windows 10 and 11 x86_64 | main, target `x86_64-pc-windows-gnullvm` | real Windows 11, Wine |
| Windows 7, 8, 8.1 x86_64 | separate, `tools/build-win7.sh` | Wine in Windows 7 mode (not on a real Windows 7) |
| FreeBSD 15 x86_64 | separate, `tools/build-freebsd.sh` (on Linux) | FreeBSD 15.1 virtual machine, Xvfb: window, menus, a stroke |
| Windows Vista and XP | none | see “Windows XP and Vista” |

## Linux: desktops and window managers

The program is an ordinary X11 window (in a Wayland session — via XWayland: only this way the pen
comes with pressure). Separate windows (floating views, brush library, panels) are system windows.

Checked on a hidden display (`tools/wm-check.sh`: window, menu, keys): Openbox (base of LXDE and
LXQt), Fluxbox, IceWM, i3, bspwm, Metacity (GNOME Flashback). KDE Plasma is the developer's
machine (X11 and Wayland). GNOME (Mutter), Xfce (xfwm4), Cinnamon (Muffin), MATE (Marco), awesome,
Sway and Hyprland could not be started without a full session — feedback welcome; on Wayland
compositors the program runs through XWayland.

## FreeBSD

Built on Linux: target `x86_64-unknown-freebsd`, clang and lld from llvm-mingw, the FreeBSD 15.1
`base.txz` as sysroot. The pen comes through XInput 2, as on Linux (the `xdg` cfg in `build.rs`:
Linux and FreeBSD are freedesktop desktops). Packages needed: `xorg` (or `xorg-vfbserver` for
checks), `mesa-dri`, `mesa-libs`, `libxkbcommon`.

## Windows 7, 8 and 8.1

Rust's standard library requires Windows 10 since 1.78. For Windows 7 — the
`x86_64-win7-windows-gnu` target (tier 3): nightly Rust and `-Zbuild-std`. On top of that:

- in the import table `combase.dll` becomes `ole32.dll` (where `CoTaskMemFree` lives on Windows
  7), and `PathCchStripPrefix` (missing before Windows 8, used by the clipboard) comes from a tiny
  `gspathcch.dll` placed next to the program;
- the Universal C Runtime is needed (update KB2999226; usually present on an updated Windows 7
  SP1) and a graphics driver with OpenGL 2.0 or newer.

The built-in updater (“Help → Check for updates”) uses the system `curl` and `tar`: Windows 7/8
and FreeBSD lack them by default — download the release archive by hand there.

## Windows XP and Vista

Theory, no build:

- Rust does not officially support XP; there is the rust9x project (Rust 1.76 and
  `*-rust9x-windows-*` targets), but GrazySAI uses language features newer than 1.76, and rust9x
  itself is “not stable”.
- The window and OpenGL layer (winit, glutin) target Windows 7+: they need Windows 7 APIs (touch
  input, DPI, DWM). On XP the window layer would have to be replaced and drawing done in software.
- XP drivers with OpenGL 2.0+ are rare; old cards have only OpenGL 1.1 (Windows software renderer).
- Vista: the `win7` target uses Windows 7 APIs in places; running on Vista is neither checked nor
  promised.

Conclusion: XP and Vista would need a separate light program (32-bit, software rendering, its own
Win32 window) — a separate project, not a GrazySAI build.

## Sources

- Rust: [rust9x](https://github.com/rust9x/rust/wiki), [XP status](https://github.com/rust-lang/compiler-team/issues/378), [discussion](https://internals.rust-lang.org/t/is-there-a-timeline-for-officially-ending-xp-support/11109)
- FreeBSD: [VM images and base.txz](https://download.freebsd.org/releases/)
