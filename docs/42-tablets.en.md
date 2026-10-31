# Graphics tablets

**English** | [Русский](42-tablets.md)

How GrazySAI gets the pen and what the driver must provide. Compiled from vendor documentation,
Linux kernel news and user discussions (October 2026).

## Where the pen comes from

| System | Method | Gives | How to enable |
|---|---|---|---|
| Windows | **WinTab** (`Wintab32.dll`, installed by the driver) | pressure, tilt, rotation, eraser end | “Settings → Windows: pen via WinTab” (off by default for now: not yet checked with a real tablet — tell us whether it works for you) |
| Windows | **Windows Ink** (system) | pressure | default; “Windows Ink” checkbox in the driver |
| Linux, X11 and XWayland | **XInput 2** | pressure, tilt, rotation, eraser end | nothing to do |
| Linux, native Wayland | `zwp_tablet_v2` | — | not yet: the window opens through XWayland |
| FreeBSD | XInput 2 (as Linux) | pressure, tilt | see “Systems” in the roadmap |

If both Windows methods work, WinTab is used while it sends packets; otherwise Windows Ink. The
status bar shows the source: “pen (WinTab)”, “pen (Windows Ink)”, “pen (XInput 2)” or “mouse”.

Tilt and rotation feed the brush dynamics: inputs “Pen tilt”, “Tilt direction”, “Pen rotation”
(brush panel → “Dynamics”).

## Brands and drivers

| Brand | Lines | Windows: WinTab / Ink | Linux |
|---|---|---|---|
| Wacom | One, Intuos, Intuos Pro, Cintiq, Cintiq Pro, Movink | both (WinTab is native to Wacom) | `wacom` kernel driver and `xf86-input-wacom` / libinput |
| XP-Pen | Deco, Deco Pro, Artist, Artist Pro, Innovator | both; Ink is a checkbox in the driver | `hid-uclogic` in the kernel (UGEE v2 models, Artist 24 Pro since 6.19) and XP-Pen's own driver |
| Huion | Inspiroy, Kamvas, Kamvas Pro | both; Huion advises matching the driver mode to the app | `hid-uclogic` / DIGImend and Huion's own driver |
| Gaomon | S, M, PD | both | `hid-uclogic` (many models share Huion controllers) |
| UGEE | M, S, U, Ugee 2150 | both | `hid-uclogic` (Linux 5.1 and later) |
| Veikk | A, VK, Studio | both | Veikk's own driver; partly in the kernel |
| Xencelabs | Pen Tablet, Pen Display | both | Xencelabs' own driver |
| Parblo, Artisul, Simbans, BOSTO | various | usually both | mostly `hid-uclogic` or generic HID |

Windows tip: if there is no pressure, toggle “Windows Ink” in the driver and restart the driver
and GrazySAI; WinTab can be turned off in GrazySAI's settings.

Wacom: the X11 driver `xf86-input-wacom` 1.2.3 (checked in its source) labels the axes the same way — “Abs Pressure”, “Abs Tilt X/Y”, and the Art Pen rotation and wheel “Abs Rotation Z” and “Abs Wheel”, which GrazySAI also reads. The Windows drivers of Wacom, Huion and Gaomon come only from model-selection pages with no direct links, so they are not analysed.

## What the drivers install (checked on the drivers themselves, October 2026)

Official Linux drivers were downloaded and taken apart: XP-Pen 4.0.15, Huion 15.0.0.162, UGEE
4.3.4, Veikk 3.5.10, Xencelabs 1.3.0 (the Gaomon 16.0.0.49 link returned no file). All of them read
the tablet themselves and create a virtual pen through `/dev/uinput`; in X11 its axes are named
“Abs Pressure”, “Abs Tilt X”, “Abs Tilt Y” — exactly what GrazySAI reads. So pressure and tilt
arrive the same way with or without the vendor driver (kernel `wacom` / `hid-uclogic`).

Windows: the XP-Pen 4.0.19 driver was installed on the test PC — it puts `Wintab32.dll` in place;
GrazySAI with WinTab on loads it and, with no tablet attached, says “WinTab is there but no tablet
found — Windows Ink”.

## Checked models

| Model | System | Method | Checked |
|---|---|---|---|
| XP-Pen Artist 24 Pro | Linux (X11, XWayland) | XInput 2 | pressure, eraser end |
| Virtual pen (`PAINT_VIRTUAL_PEN`) | Linux, Windows (Wine) | Windows Ink | pressure, taps, eraser |
| — | Windows under Wine, no tablet | WinTab | starts; without a tablet stays on Windows Ink; WinTab not yet checked with a real tablet |

Other models — from feedback: tell us in [Issues](https://github.com/grazyping/GrazySAI/issues)
which model and what the status bar shows.

## Sources

- Huion: [pressure not working in an app](https://support.huion.com/en/support/solutions/articles/44002618549), [V15 driver settings](https://support.huion.com/en/support/solutions/articles/44002356041)
- Krita: [WinTab vs Windows Ink](https://krita-artists.org/t/difference-between-wintab-and-windows-ink-input/12781)
- Linux kernel: [XP-Pen Artist 24 Pro in 6.19](https://www.phoronix.com/news/Linux-6.19-XP-PEN-Artist-24-Pro), [UC-Logic additions in 5.1](https://www.phoronix.com/news/Linux-5.1-UC-Logic-Additions), [XP-Pen Deco L](https://www.phoronix.com/news/Linux-5.20-XP-PEN-Deco-L), [DIGImend](https://github.com/DIGImend/digimend-kernel-drivers/releases)
