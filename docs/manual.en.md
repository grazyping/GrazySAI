# SOsai — user manual

**English** | [Русский](manual.md)

SOsai is a painting program modelled on PaintTool SAI 2: brushes, layers, selection, transform,
linework and shape layers, opening and saving `.sai2` files. It runs on Windows and Linux and
works with a graphics tablet pen.

## Installing and running

Ready builds are on the [Releases](https://github.com/grazyping/SOsai/releases) page.

- **Windows**: download `SOsai-…-windows-x86_64.zip`, unpack it, run `SOsai.exe`. No
  installation is needed.
- **Linux**: download `SOsai-…-linux-x86_64.tar.gz`, unpack it, run `./SOsai` (you can pass a
  file: `./SOsai drawing.sai2`).

A file can also be opened by dropping it onto the window.

### Updating

At startup the program checks GitHub for a newer version. If there is one, “⬆ Version … is
out” appears at the right of the menu bar. Click it to see what changed and press “Download and
Install”: the program downloads the archive, verifies it and puts the new version in place of
the old one. The new version runs after a restart (“Restart Now” — once the canvas is saved).
If updating fails (no network, no write access to the program folder), the window offers the
release page so you can download it by hand.

The startup check can be turned off: “Help → Check for updates at startup”. To check by hand —
“Help → Check for Updates…”. The project page is “GitHub” at the right of the menu bar or
“Help → SOsai on GitHub”. On Windows this needs Windows 10 (1803) or newer.

### Tablet

- **Windows**: pressure comes through Windows Ink. It must be enabled in the tablet driver
  settings (for XPPen — “Windows Ink”).
- **Linux**: pressure is read directly (XInput 2), nothing to set up.
- The back end of the pen is an eraser.
- The status bar at the bottom shows where pressure currently comes from (“pen” or “mouse”) and
  its value.

## Language

The interface is translated into 10 languages: English, Русский, Українська, Deutsch,
Français, Español, Português (Brasil), 日本語, 简体中文, 한국어. On first start SOsai uses the
system language (English if it is not in the list). To change it: “Window → Language” — it
switches at once, without a restart, and the choice is remembered. Japanese, Chinese and Korean
need a system font with CJK characters (always present on Windows; the Noto CJK package on
Linux).

## Window

The layout follows SAI 2:

- **Left**: color wheel, common tools (selection, lasso, magic wand, bucket, move, zoom, hand,
  color picker), main and background color, brush grid, current brush settings, size grid.
- **Top**: menus and the quick bar (undo, selection, zoom, view rotation and flip, stabilizer).
- **Right**: navigator, layer properties (blend mode, opacity, locks, “Clipping Group”,
  “Selection Source”), layer buttons, layer list with thumbnails.

## Painting

### Brushes

In the grid: **Pencil**, **AirBrush**, **Brush**, **Water**, **Marker**, **Eraser**,
**SelPen** (selection pen), **SelErs** (selection eraser). Defaults match a new tool in SAI.

| Setting | What it does |
|---|---|
| Hardness | Five edge steps, from the softest to the hardest |
| Size | Diameter in pixels; `[` and `]` — previous or next size from the size grid |
| Minimum | Size at the lightest pressure (if pressure on size is on) |
| Density, Min Density | Stroke opacity and its value at light pressure |
| Pressure: Size, Density, Blend | What pen pressure changes |
| Blending, Dilution, Persistence | Brush, Water and Marker only (see below) |
| Advanced Settings: Sharpness, Amplify Dens., “Ver1 pressure on density” | Fine tuning of edge and density, as in SAI |

**Stabilizer** (quick bar): 0–15 and S-1…S-7 — the higher, the smoother the line and the more
it lags behind the pen.

**Mixing with the canvas** (Brush, Water, Marker):

- “Blending” — how much of the canvas color mixes into the paint;
- “Dilution” — how much of the canvas transparency the paint picks up;
- “Persistence” — how long the paint keeps being dragged along;
- “Blur Pressure” — below this pressure the brush only smears.

Water also blurs the layer under the stroke.

### Color

- Color wheel: the ring is hue, the square is saturation (to the right) and value (up).
- The buttons above the wheel show and hide color panel views: the wheel, RGB sliders (red,
  green, blue, 0–255), HSV sliders (hue 0–359, saturation and value 0–255). Several can be shown
  at once; the choice is remembered.
- The arrow by the main and background color squares (or the X key) swaps them.
- Color picker: Alt + click (or the Color Picker tool).

### Fill

- The **bucket** fills a connected area of similar color with the main color. Tolerance works
  as for the magic wand.
- **Alt+Delete** fills the selection (without a selection — the whole layer).

## Layers

- Buttons and the “Layer” menu: new layer, new folder, new linework layer, new shape layer,
  copy, delete, erase (Delete), transfer down (F), merge down (Ctrl+E), order, flip,
  “Luminance to Opacity”, “Rasterize Layer”, “Flatten Image”.
- **Order**: drag a layer in the list with the mouse or the pen. The grabbed layer fades in
  place and a label follows the cursor. A blue bar with end marks shows where it will go (the
  top of a row — above that layer, the bottom — below it, above or below the whole list — to
  the very top or bottom); hovering the middle of a folder outlines it — the layer goes inside
  and the folder opens. A “not allowed” cursor means it cannot be dropped there (same place, or
  a folder into itself). The list scrolls at its edges, Esc cancels the drag, Ctrl+Z undoes a
  finished move.
- **Properties**: blend mode (SAI's 28 modes), opacity, visibility (the eye in the list),
  “Clipping Group” (visible only within the layer below), folder “Pass Through”.
- **Locks**: “Preserve Opacity” (the brush changes only color), protect from painting, protect
  from moving, protect from modifying.
- **“Selection Source”**: the magic wand and the bucket read color from all marked layers
  together. Handy for filling color under line art kept on a separate layer.
- **Ctrl + click on a thumbnail** — select by the layer's opacity. With Shift — add, with
  Alt — subtract.

### Paper (layer texture)

In layer properties, the “Texture” section: choose a texture, scale (10–500 %) and strength
(0–100). The layer's paint becomes uneven, as on paper; works for folders too.

Textures are taken from SAI's `papertex` folder (SOsai does not ship them). If the list is
empty, type the path to that folder in the field above the list and press OK; the path is
remembered. `.sai2` files with paper open and save with it.

### Layer masks

- **Create**: the “New Layer Mask” button under the layer list or “Layer → New Layer Mask”. A
  mask thumbnail appears to the right of the layer thumbnail, and the brush paints in the mask
  right away.
- **Paint in the mask**: click the mask thumbnail (it gets outlined). Black hides the layer,
  white shows it, grey shows it partly; the eraser shows. Click the layer thumbnail to paint in
  the layer again.
- “Layer → Apply Mask” erases the hidden part from the layer and removes the mask; “Delete
  Mask” removes the mask without changing the layer.
- The mask moves, transforms and flips together with its layer. Merge Down works with masked
  layers too: the lower layer's mask is applied to it.
- Masks from SAI files open and save.

### Layer effects

Layer properties, the “Layer Effects” section:
- **Fringe** — paint thickens along the edge, like watercolor; width 1–15, strength 0–100;
- **Binary Color** — every color channel is either full or empty; threshold 0–100.

Normal layers only (not folders).

## Selection

| Tool | How |
|---|---|
| Rectangle (M) | Drag; a click without dragging deselects |
| Lasso (L) | Draw around |
| Magic Wand (W) | Click; tolerance is in the panel |
| SelPen and SelErs | Paint the selection with a brush |

- Shift — add to the selection, Alt — subtract, Shift+Alt — keep the intersection.
- Ctrl+A — select all, Ctrl+D — deselect, Ctrl+H — hide or show the outline.
- The “Selection” menu has invert, expand and shrink by 1 px.
- Brushes, fill, filters and Delete act only inside the selection.

## Move and transform

- **Move**: the Move tool, Ctrl + drag with any tool, or Alt+arrows (by 1 px; with Shift — by
  10). With a selection only the selected part moves.
- **Transform (Ctrl+T)**:
  - corners and side midpoints — size;
  - Ctrl + corner — free deform (each corner separately);
  - drag inside the frame — move, outside — rotate;
  - the resampling method is in the panel;
  - Enter — apply, Esc — cancel.

## Canvas and view

- The “Canvas” menu:
  - change resolution (the image is resampled);
  - change canvas size (the image is not scaled, 3×3 anchor);
  - flip;
  - rotate by 90°.
- **View**:
  - mouse wheel — zoom at the cursor; Space or the middle button + drag — pan;
  - Shift+PageUp / Shift+PageDown — rotate by 15°, Shift+Home — reset the angle;
  - H — flip the view.

  View rotation and flip change only the display, not the image.

## Ruler

“Ruler → Show Ruler” (Ctrl+R). While the ruler is shown, every stroke lies on it entirely,
wherever it starts.

- **Straight**: a translucent band, the line is its edge. Ctrl + drag the band to move it;
  with “Lock Pivot” — to rotate it around the pivot (the cross).
- **Ellipse**: a ring; strokes lie on its outer edge. Holding Ctrl shows a frame: inside —
  move, outside — rotate, a side or a corner — stretch.
- “Ignore Ruler” (menu or the quick bar button) — draw freely without hiding it; “Reset
  Ruler” — place it again in the middle of the view.

## Filters

The “Filter” menu; with a selection the filter acts only on the selected part.

- **Hue and Saturation** (Ctrl+U), with a “Colorize” mode.
- **Brightness and Contrast** (and color “Depth”).
- **Gaussian Blur** (radius 0.1–40).

Changes show on the canvas at once. OK applies, “Cancel” reverts.

## Linework and shape layers

- **Linework layer**:
  - the pen draws curves, not pixels;
  - when the pen lifts, the stroke becomes a path with control points and pressure, as in SAI.
- **Linework tools** (in the panel when a linework layer is selected):
  - Pen (N);
  - Eraser (E) — deletes a whole path;
  - Weight — sets the brush size;
  - Color — recolors with the main color;
  - Edit (I) — drag a control point;
  - Pressure (P) — drag a control point up or down.
- **Shape layer**: pick a rectangle, ellipse or triangle in the panel and drag on the canvas.
- **Raster tools**: brushes, fill and filters do not edit these layers. “Layer → Rasterize
  Layer” makes the layer normal.
- **Saving**: curves are saved to `.sai2` as curves.

## Files

| Format | Open | Save |
|---|---|---|
| `.sai2` | Yes | Ctrl+S, Shift+Ctrl+S (in the same format as SAI 2) |
| `.psd` | Yes (RGB, 8 bit) | “File → Export → PSD”: with layers, folders, modes |
| `.png`, `.jpg` | Yes, as one layer | “File → Export → PNG / JPEG” (flattened image) |

Text layers and perspective rulers from `.sai2` are not read yet: a note appears when such a
file is opened.

## Settings and recovery

- **Settings** are saved on exit and restored on start: brushes, colors, stabilizer, magic
  wand tolerance, interface language. Where they are:
  - Windows: `%APPDATA%\SOsai\`;
  - Linux: `~/.config/SOsai/`.
- **Recovery**:
  - every 3 minutes, if there were edits, SOsai saves a copy of the canvas in the background;
  - if the program closed abnormally, on the next start it offers to recover the work.

## Shortcut keys

Keys can be reassigned: “Help → Shortcut Keys…” — click an action's key, then press the new
combination (Esc — cancel; the key is taken from another action if it was in use); “As in SAI”
restores all defaults. Assignments are saved in the settings. The defaults are below.

| Keys | Action |
|---|---|
| Ctrl+Z / Ctrl+Y (Shift+Ctrl+Z) | Undo / redo |
| Ctrl+S / Shift+Ctrl+S | Save / save as |
| `[` / `]` | Brush size down / up |
| Ctrl+`+` / Ctrl+`−` | Zoom |
| N, B, V, C, E | Pencil, airbrush, brush, water, eraser (on a linework layer: N — pen, E — linework eraser) |
| I, P | On a linework layer: edit, pressure |
| M, L, W | Rectangle, lasso, magic wand |
| Ctrl+A / Ctrl+D / Ctrl+H | Select all / deselect / hide the outline |
| Delete / Alt+Delete | Erase / fill (within the selection) |
| F / Ctrl+E | Transfer down / merge down |
| Ctrl+T, Enter, Esc | Transform, apply, cancel |
| Alt+arrows (with Shift — by 10) | Move the layer or the selection |
| Ctrl + drag | Move the layer or the selection |
| Ctrl+U | Hue and saturation |
| Ctrl+R | Show or hide the ruler |
| Shift+PageUp / Shift+PageDown / Shift+Home / H | Rotate view / reset / flip view |
| X | Swap main and background color |
| Alt + click | Color picker |
| Space + drag | Pan the view |

## How it still differs from SAI

The full list is in [ROADMAP.en.md](../ROADMAP.en.md). The main points:
- No brush shapes and textures, text or perspective rulers yet.

## If something is wrong

Report a bug on the [Issues](https://github.com/grazyping/SOsai/issues) page (English or
Russian):
- what you did;
- what you expected and what happened;
- the SOsai version (in the window title) and the system (Windows or Linux).

If you can, attach the file or a screenshot.
