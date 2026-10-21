# What SAI 2 gained after 2017: analysis and plan

Collected 2026-10-21. The user paints in build 2017-01-19; features from later builds are welcome.
This is the plan: work follows it step by step, each step its own release. Russian version with the
same content: `41-sai2-after-2017.md`.

## Sources

| Source | Gives |
|---|---|
| Changelogs of the official builds (`history.txt` of 2026-07-02b and Alpha 2026-10-11) | what appeared and when, in the developer's words |
| Official builds 2026-07-02b and Alpha 2026-10-11 (from the vendor's site) | live comparison: without a licence everything works except opening and saving; results are taken through the clipboard (bitmap) |
| Labels and parameter names in those builds | the parameter set of each tool |
| Our own Ghidra pass over the official 2026-07-02b, if a formula is needed | understanding only; the code we write is our own |

Rules (public repository): no SAI addresses, code or screenshots in docs or code; the licence part is
not analysed; live checks only on official builds, hidden display, a separate Wine prefix.

## Main constraints

1. **Files must keep opening in SAI 2017.** Nothing new goes into SAI's own blocks. In 2026 the
   symmetric ruler is its own layer type and binary linework strokes live inside `liwk`; neither is
   written that way. Tools live in settings and `.gsbrush`; document state in our own blocks (like
   `gs3d`/`gs3a` of the 3D layer, verified in the user's SAI).
2. **The 2026 brush engine is not the 2017 one** (retuned: Brush defaults 2026-07-12, Ver.1 tapering
   2026-09-20, …). New tools are **separate kinds**; existing kinds calibrated against 2017 stay
   unchanged. Before any calibration — a control step: the same Pencil, Brush and Water strokes in
   2026-07-02b against our 2017 sheets; new tools are measured relative to the 2026 base stroke.
3. **Checks by script, not by clicking.** Test sheets are drawn with the virtual pen
   (`crates/virtual-tablet`), the result is taken from the clipboard; screenshots only for windows
   and defaults.

## What we already have

| Feature | Where | State |
|---|---|---|
| Binary pen (2016) | `crates/paint-brush/src/lib.rs:100` | done |
| Selection pen and eraser (2020) | `crates/paint-brush/src/lib.rs:103` | done |
| Intersect selection Alt+Shift (2019-07-28) | `crates/paint-core/src/selection.rs:17` | done |
| Color history (2020-01-07) | `crates/paint-app/src/main.rs:1142` | done |
| Scratch pad (2017-08-15) | `crates/paint-app/src/main.rs:590` | done |
| Linear Burn/Dodge/Light modes | `crates/paint-core/src/blend.rs:18` | done |
| Shift + press — line from the previous stroke's end | `crates/paint-app/src/main.rs:4167` | done; Shift **after** starting a stroke (2026-09-15) — missing |
| Symmetry | `crates/paint-app/src/main.rs:1361`, `crates/paint-core/src/stroke.rs:14` | two axes through the center only; SAI — line/point, up to 20 parts |
| Rulers | `crates/paint-app/src/ruler.rs:28` | straight, ellipse, perspective; parallel lines, concentric ellipses, vanishing point — missing |
| Crash recovery, history on disk | — | done |
| Hook trimming | `crates/paint-core/src/vectorize.rs:127` | fixed, linework pen only; the 2018-04-28 setting — missing |
| Brush preview | only in the workshop (`paint-studio`) | missing in the program (SAI since 2018-09-17) |
| Blur | filter `crates/paint-core/src/filters.rs:219`, watercolor blur | no Blur tool |
| Smudging without paint at light pressure | `crates/paint-core/src/mix.rs:331` | basis for Smudge |
| Canvas size presets (2018-04-28) | “New canvas” has width and height only | missing |

## What is missing: item by item

For each: what SAI does (changelog date), what we have, design, where the data lives, what to measure,
acceptance, effort (D ≈ a day of work).

1. **Lasso Fill** — circle an area, it is filled (mentioned 2026-08-13). **First confirm** in
   2026-07-02b that the tool can be created and what it has. We: lasso + fill exist → lasso outline →
   mask → fill with edge. Settings. Measure: edge antialiasing, fill over transparency. Accept:
   ±2/255 edge. 1 D.
2. **Blur tool** (2020-01-07) — a brush blurring under itself; “Blur pressure”, “Blur width”. Basis:
   watercolor blur and the Gaussian filter. New kind in `.gsbrush` (format version +1). Measure: blur
   profile of a step edge at several pressures and widths. Accept: ±3/255. 2 D.
3. **Smudge** (2017-11-22) — drags canvas paint; “Coloring” and “Uncolor pressure” (the pressure range
   that only smudges). Basis: `mix.rs`. New kind. Measure: a stroke across colored bars at 3 pressures.
   Accept: tail length and color ±5/255. 2–3 D.
4. **Symmetry: line and point, up to 20 parts** (2019-07-28) — center and angle, mirrored or rotated,
   2–20 parts, applies to all brushes (curves and lines since 2024-02-22). We: two center axes. Design:
   generalize `Mirror` to center, angle, N, mirror flag; on-canvas handles like the ruler. Stored in our
   own document block, **not** as a SAI layer. Measure geometry only. Accept: 20 parts within 0.5 px;
   wrap-around mode intact. 2 D.
5. **Scattering brush** (2018-09-17, 2019-05-18) and **Effect Pen** (2019-05-18) — shapes scattered
   around the stroke; size, angle, aspect and their jitters, count and jitter, spacing, density,
   hardness, scattering, Gaussian, all directions, integer positions, hue/saturation/brightness and
   FG→BG jitters, per shape or per stroke, blend mode as paint mode; Effect Pen paints with a layer
   blend mode. Basis: picture tips, random dynamics, blend modes. New kinds in `.gsbrush`, parameters
   named after SAI's. Measure: distributions (hundreds of dabs, histograms of distance and angle), color
   jitters. Accept: mean and spread within ±10 %; Effect Pen ±3/255. 4–5 D.
6. **Liquify** — **not** among the changelog's new features; only the tool name exists. **First step**:
   check in 2026-07-02b whether it can be created and what modes it has. If hidden or unfinished, it
   moves to the end; no modes promised until seen. If present: displacement grid, brush modes as found
   in SAI, preview outside history, the result is one step. 3–4 D after the check.
7. **Pixel Art Pen, binary eraser, binary selection pen and eraser** (2020-01-07) — no antialiasing,
   exact pixels. We have the binary pen. `.gsbrush`. Measure: small sizes 1–10 px. Accept: pixel-exact.
   1–2 D.
8. **Brush details** — minimum pressure for a stroke (with the minimum-size circle, 2024-08-14), the
   hook trimming setting of the linework pen (2018-04-28), Anti-Ripple for flat large brushes
   (2017-11-22), Shift after starting a stroke — horizontal/vertical (2026-09-15), brush preview in the
   program (2018-09-17). Settings and `.gsbrush`. 2–3 D.
9. **Canvas size presets** (2018-04-28), **reserve and restore tool settings** (2020-01-07). Settings.
   1–2 D.
10. **Rulers: parallel lines, concentric ellipses, vanishing point** (2017-11-24) — three kinds in
    `ruler.rs`; stored in our own block (SAI 2026 rulers are layers — not written). 2 D.
11. **Groups and Asset Manager** (2020-01-07), swatch sets — groups for tools, forms, textures, swatches,
    scratch pads; a window for ordering and moving. Settings. 3 D. Lower value — later.
12. **Binary linework tools** (2020-01-07) — pen, eraser, curve, weight for binary strokes. In SAI 2026
    files they live inside `liwk`, which SAI 2017 probably cannot read. Only if we store them ourselves
    and write a compatible `liwk`. Last.
13. **From 2026, as time allows** — wand/bucket “Reduce small omissions” (2026-08-31), selection operation
    in tool settings (2026-07-27), lasso stabilizer (2026-10-11), grayscale conversion with a warning,
    the view list in the View menu, clearing Recent files.

**Not doing:** settings import from SAI Ver.1, the “Ver.1-like” WinTab mode, Intel P-core scheduling —
not relevant (we have our own pen input and thread pool).

## Order

Daily-use, file-safe, easy-to-verify items go first; therefore the cheap frequent ones come before the
scattering brush, Liquify and rulers mentioned earlier:

1. Control step: 2026-07-02b base strokes against the 2017 sheets.
2. Lasso Fill (after confirming the tool exists). 3. Blur. 4. Smudge. 5. Line/point symmetry, up to 20
parts. 6. Scattering brush and Effect Pen. 7. Liquify (after the check). 8. Pixel Art Pen and binary
erasers. 9. Brush details. 10. Canvas presets; reserve/restore tool settings. 11. New rulers. 12. Groups
and Asset Manager. 13. 2026 details. 14. Binary linework — if compatible storage is found.

## Checklist for every painting tool

- History (one stroke — one step) and the process recording journal.
- Symmetry, wrap-around, stabilizer, rulers.
- Blocked on 3D, linework, shape and text layers.
- Translations into 9 languages; manual ru/en; roadmap ru/en.
- A self-test line (`--self-test`) so it is checked on Windows.
- Saving and reading `.gsbrush`, old brush files still open.
