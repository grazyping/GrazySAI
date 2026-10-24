# Системы

[English](43-systems.en.md) | **Русский**

Где GrazySAI работает, как собирается и что проверено (октябрь 2026).

## Сводка

| Система | Сборка | Проверено |
|---|---|---|
| Linux x86_64 (X11, Wayland через XWayland) | основная, `cargo build --release` | рабочая машина (KDE Plasma), скрытый экран Xvfb |
| Windows 10 и 11 x86_64 | основная, цель `x86_64-pc-windows-gnullvm` | настоящая Windows 11, Wine |
| Windows 7, 8, 8.1 x86_64 | отдельная, `tools/build-win7.sh` | Wine в режиме Windows 7 (на настоящей Windows 7 — нет) |
| FreeBSD 15 x86_64 | отдельная, `tools/build-freebsd.sh` (на Linux) | виртуальная машина FreeBSD 15.1, Xvfb: окно, меню, мазок |
| Windows Vista и XP | нет | см. «Windows XP и Vista» |

## Linux: окружения и оконные менеджеры

Программа — обычное окно X11 (в сеансе Wayland — через XWayland: только так перо приходит с
нажимом). Отдельные окна (плавающие виды, библиотека кистей, панели) — окна системы.

Проверено на скрытом экране (`tools/wm-check.sh`: окно, меню, клавиши): Openbox (основа LXDE и
LXQt), Fluxbox, IceWM, i3, bspwm, Metacity (GNOME Flashback). KDE Plasma — рабочая машина
разработчика (X11 и Wayland). GNOME (Mutter), Xfce (xfwm4), Cinnamon (Muffin), MATE (Marco),
awesome, Sway и Hyprland без полного сеанса запустить не удалось — ждём отзывов; на Wayland-
композиторах программа работает через XWayland.

## FreeBSD

Сборка на Linux: цель `x86_64-unknown-freebsd`, clang и lld из llvm-mingw, корень системы —
`base.txz` FreeBSD 15.1. Перо — XInput 2, как на Linux (признак `xdg` в `build.rs`: Linux и
FreeBSD — рабочий стол freedesktop). Нужны пакеты: `xorg` (или `xorg-vfbserver` для проверок),
`mesa-dri`, `mesa-libs`, `libxkbcommon`.

## Windows 7, 8 и 8.1

Стандартная библиотека Rust с версии 1.78 требует Windows 10. Для Windows 7 — цель
`x86_64-win7-windows-gnu` (уровень 3): ночной Rust и `-Zbuild-std`. Сверх того:

- в таблице импорта `combase.dll` заменяется на `ole32.dll` (там `CoTaskMemFree` в Windows 7), а
  `PathCchStripPrefix` (её нет до Windows 8, нужна буферу обмена) — своя маленькая
  `gspathcch.dll`, она кладётся рядом с программой;
- нужна универсальная среда C (обновление KB2999226; на Windows 7 SP1 с обновлениями — обычно уже
  есть) и драйвер видеокарты с OpenGL 2.0 и новее.

## Windows XP и Vista

Теория, без сборки:

- Rust официально XP не поддерживает; есть проект rust9x (Rust 1.76 и цели `*-rust9x-windows-*`),
  но GrazySAI использует возможности языка новее 1.76, а сам rust9x — «не стабилен».
- Окно и OpenGL (winit, glutin) рассчитаны на Windows 7+: нужны API Windows 7 (сенсорный ввод,
  DPI, DWM). На XP пришлось бы заменить оконный слой и рисовать программно.
- Видеодрайверов XP с OpenGL 2.0+ мало, на старых картах — только OpenGL 1.1 (программная
  отрисовка Windows).
- Vista: цель `win7` местами использует API Windows 7; запуск на Vista не проверен и не обещан.

Вывод: для XP и Vista нужна отдельная облегчённая программа (32 бита, программная отрисовка,
своё окно на Win32) — это отдельный проект, а не сборка GrazySAI.

## Источники

- Rust: [rust9x](https://github.com/rust9x/rust/wiki), [статус XP](https://github.com/rust-lang/compiler-team/issues/378), [обсуждение](https://internals.rust-lang.org/t/is-there-a-timeline-for-officially-ending-xp-support/11109)
- FreeBSD: [образы ВМ и base.txz](https://download.freebsd.org/releases/)
