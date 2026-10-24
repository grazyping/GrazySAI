# Graphics tablets

**English** | [Русский](42-tablets.md)

How GrazySAI gets the pen and what the driver must provide. Compiled from vendor documentation,
Linux kernel news and user discussions (October 2026).

## Where the pen comes from

| System | Method | Gives | How to enable |
|---|---|---|---|
| Windows | **WinTab** (`Wintab32.dll`, installed by the driver) | pressure, tilt, rotation, eraser end | on by default; “Settings → Windows: pen via WinTab” |
| Windows | **Windows Ink** (system) | pressure | when WinTab is missing or off; “Windows Ink” checkbox in the driver |
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

## Checked models

| Model | System | Method | Checked |
|---|---|---|---|
| XP-Pen Artist 24 Pro | Linux (X11, XWayland) | XInput 2 | pressure, eraser end |
| Virtual pen (`PAINT_VIRTUAL_PEN`) | Linux, Windows (Wine) | Windows Ink | pressure, taps, eraser |
| — | Windows under Wine, no tablet | WinTab | starts; without a tablet stays on Windows Ink |

Other models — from feedback: tell us in [Issues](https://github.com/grazyping/GrazySAI/issues)
which model and what the status bar shows.

## Sources

- Huion: [pressure not working in an app](https://support.huion.com/en/support/solutions/articles/44002618549), [V15 driver settings](https://support.huion.com/en/support/solutions/articles/44002356041)
- Krita: [WinTab vs Windows Ink](https://krita-artists.org/t/difference-between-wintab-and-windows-ink-input/12781)
- Linux kernel: [XP-Pen Artist 24 Pro in 6.19](https://www.phoronix.com/news/Linux-6.19-XP-PEN-Artist-24-Pro), [UC-Logic additions in 5.1](https://www.phoronix.com/news/Linux-5.1-UC-Logic-Additions), [XP-Pen Deco L](https://www.phoronix.com/news/Linux-5.20-XP-PEN-Deco-L), [DIGImend](https://github.com/DIGImend/digimend-kernel-drivers/releases)
