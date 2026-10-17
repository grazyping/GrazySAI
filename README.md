# GrazySAI

**English** | [Русский](README.ru.md)

A painting program in Rust modelled on PaintTool SAI 2 — a personal project. Version 1.0.2.
Formerly called **SOsai** (renamed in 0.8.42; older versions update to GrazySAI by themselves).
How to use it is in the [manual](docs/manual.en.md); what is done and what comes next is in
[ROADMAP.en.md](ROADMAP.en.md) and on the [Milestones](https://github.com/grazyping/GrazySAI/milestones)
tab; ready builds are under Releases. Linux and Windows, tablet pen (XPPen Artist 24 Pro),
opens `.sai2` files.

## Features

- Opens `.sai2`: raster layers, folders, clipping groups, blend modes (all 28), linework and
  shape layers (curves are rebuilt the way SAI does it).
- Brush following SAI's model: dab, edge, opacity, spacing (`docs/13-thin-brushes.md`).
- Brush, Water and Marker mix paint with the canvas by SAI's rules: blending, dilution,
  persistence, blur pressure (`docs/16-mixing.md`).
- Stabilizer as in SAI: 1–15 and S-1…S-7 (`docs/15-stabilizer.md`).
- Saves `.sai2` (`docs/03-sai2-format.md`).
- Selection (rectangle, lasso, magic wand), moving and transforming (Ctrl+T) a layer or the
  selection (`docs/17-selection-move.md`).
- Canvas resolution, size, flip and rotation; view rotation and flip (`docs/18-canvas-view.md`).
- Bucket fill, Flatten Image, layer locks (`docs/19-fill-locks.md`).
- Layer paper texture, Fringe and Binary Color layer effects, layer masks — as in SAI
  (`docs/24-paper.md`, `docs/25-layer-effects.md`, `docs/26-layer-masks.md`).
- Filters: hue and saturation, brightness and contrast, Gaussian blur (`docs/20-filters.md`).
- Opens and exports PSD with layers, PNG, JPEG (`docs/21-file-formats.md`).
- Settings kept between runs and recovery after a crash (`docs/22-settings-recovery.md`).
- Linework and shape layers: the pen draws curves, curve editing, shapes — as in SAI
  (`docs/11-linework-shapes.md`).
- Brush forms (blots) and textures, layer paper as a brush texture — as in SAI 2017
  (`docs/29-brush-form-texture.md`).
- Text layers: drawn with the system font and saved byte for byte (`docs/30-text-layers.md`).
- Gradation tool, polygonal lasso, dilate and erode the selection by 1–25 px — checked against the live SAI (`docs/31-gradient-selection.md`).
- Rulers: straight line and ellipse, as in SAI 2017 (`docs/27-rulers.md`).
- Five resampling methods, as in SAI (`docs/12-resampling.md`).
- Window laid out like SAI 2 (`docs/04-ui-layout.md`, `docs/ref/paint-rs-ui.png`).
- Interface in 10 languages: English, Russian, Ukrainian, German, French, Spanish, Portuguese
  (Brazil), Japanese, Simplified Chinese, Korean. The system language is used by default;
  change it in “Window → Language”. Translations live in `crates/paint-app/lang/`.
- Updates itself from GitHub releases: a quiet notice in the menu bar, download, checksum,
  replace, restart — no server of its own (`docs/28-updates.md`).
- Pen pressure: XInput 2 on Linux, Windows Ink on Windows; a virtual pen and a virtual tablet
  for testing without a tablet (`docs/14-virtual-pen.md`).

The documents in `docs/` other than the manual are written in Russian.

## Building

```bash
cargo build --release -p paint-app      # editor: target/release/GrazySAI [file.sai2]
cargo build --release -p paint-cli      # command line: comparisons, rendering, .sai2 inspection
cargo test --workspace --release
```

Windows build from Linux: target `x86_64-pc-windows-gnullvm`, linker llvm-mingw (path in
`.cargo/config.toml` — adjust it for your machine).

Tests that use the author's sample files look for them in `~/SAI2` (or in `SAI2_SAMPLES`); if
the files are missing, those tests are skipped.

## Translations

Each language is a plain text file `crates/paint-app/lang/<code>.txt` with lines
`Russian source text = translation`; `{0}`, `{1}` are values inserted by the program. The test
`every_key_is_translated` (`cargo test -p paint-app`) checks that every interface string has a
translation in every file. Corrections and new languages are welcome as pull requests.

## Layout

| Directory | What |
|---|---|
| `crates/paint-core` | Pixels, tiles, layers, blending, brush (`dab.rs`), strokes, vector shapes, resampling |
| `crates/paint-formats` | `.sai2` reading and writing: table of contents, layers, pixels, vector blocks; PSD |
| `crates/paint-app` | Editor window (egui), translations in `lang/` |
| `crates/paint-cli` | Command line for comparisons with the reference program |
| `crates/pen-test` | Pen event logger (Windows Ink, WinTab, XInput 2) |
| `crates/virtual-tablet` | Virtual tablet via `/dev/uinput` (Linux) |
| `docs/` | Plan, `.sai2` format, SAI's rules in our own words, comparisons, interface |
| `tools/` | Virtual pen check |

## License

MIT — see [LICENSE](LICENSE). GrazySAI is an independent project and is not affiliated with
SYSTEMAX Software Development, the author of PaintTool SAI; the names SAI and PaintTool SAI
belong to their owner. The code is written from scratch based on the program's observed
behaviour and file format; there is no PaintTool SAI code in this repository.
