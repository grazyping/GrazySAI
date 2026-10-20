# GrazySAI — user manual

**English** | [Русский](manual.md)

GrazySAI is a painting program modelled on PaintTool SAI 2: brushes, layers, selection, transform,
linework and shape layers, opening and saving `.sai2` files. It runs on Windows and Linux and
works with a graphics tablet pen.

## Installing and running

Ready builds are on the [Releases](https://github.com/grazyping/GrazySAI/releases) page.

- **Windows**: download `GrazySAI-…-windows-x86_64.zip`, unpack it, run `GrazySAI.exe`. No
  installation is needed.
- **Linux**: download `GrazySAI-…-linux-x86_64.tar.gz`, unpack it, run `./GrazySAI` (you can pass a
  file: `./GrazySAI drawing.sai2`).

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
“Help → GrazySAI on GitHub”. On Windows this needs Windows 10 (1803) or newer.

### Tablet

- **Windows**: pressure comes through Windows Ink. It must be enabled in the tablet driver
  settings (for XPPen — “Windows Ink”).
- **Linux**: pressure is read directly (XInput 2), nothing to set up.
- The back end of the pen is an eraser.
- The status bar at the bottom shows where pressure currently comes from (“pen” or “mouse”) and
  its value.

## Language

The interface is translated into 10 languages: English, Русский, Українська, Deutsch,
Français, Español, Português (Brasil), 日本語, 简体中文, 한국어. On first start GrazySAI uses the
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

Shortcut keys in menu items and tooltips can be hidden: “Window → Show shortcut keys”.

“Window → Separate User Interface Panel” (as in SAI): tools, color, layers or navigator — each
panel in its own window that can go to another monitor; closing that window puts the panel back.

“Window → History” lists the steps: click a step to return the picture to the state after it (undone steps are shown dimmed and can be clicked too); “Initial state” goes back to the start.

The “Window” menu has “Dark theme”: dark panels and menus; the canvas, paper and drawing colors stay unchanged.

The “Window” menu, as in SAI: UI scale (100–200%), the color and tool panel and the navigator and
layer panel on the right or left, full screen (F11, Shift+Tab), “Show All User Interface Panels”
(Tab leaves only the canvas), “Show” for each panel separately (navigator, layers, color, tools,
quick bar, view tabs, status bar), a dot cursor for brushes, the brush size circle. The color panel
has a **scratch pad** (last button): paint on it with the current brush (with pen pressure, in screen pixels) to mix a color; Alt + click
or right click picks a color, “×” clears it. The “View” menu:
rotate and mirror the view, “Reset Position of View” (Home), “Reset Angle of View” (Shift+Home),
100% (Ctrl+Alt+0). Zoom percentages, as in SAI, are in screen pixels: with Windows scaling at
125–150%, 100% shows one picture pixel per screen pixel, without blur.

## Painting

### Brushes

In the grid: **Pencil**, **AirBrush**, **Brush**, **Water**, **Marker**, **Eraser**,
**SelPen** (selection pen), **SelErs** (selection eraser), **Gradation**. Defaults match a new
tool in SAI.

As in SAI, the grid can be changed: right-click an empty cell for a new tool of the kind you need
(including the **Binary Pen** — lines without anti-aliasing — and the **Bucket**); right-click a
tool for “Duplicate”, “Delete” and “Property…” (its own name and a second line under it). The grid
is kept between runs.

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

**Form and texture** (the “Form” and “Texture” sections, as in SAI 2):

- “Form” — a simple circle or a blot from the `blotmap` folder: the stroke edge becomes ragged
  following the pattern. “Strength” sets how much, “Zoom” the pattern size (10–500 %). Brush
  hardness changes how sharp the blot is. After the blots the list has the bristles from the
  `bristle` folder: each hair draws its own line along the stroke; “Hair Size”, “Hair Min”,
  “Randomize”, “Direction” (None, Auto, Pen Direction).
- “Texture” — a pattern from the `brushtex` folder or the current layer's paper: in a pattern the
  dark parts hold paint back, in layer paper the light parts do (as in SAI). “Strength”, “Zoom”, “Affect by MinDens” (the pattern is
  weaker on thin brushes and at light pressure).
- “Tip” (in the “Form” section) — a circle or a picture: every dab of the stroke is drawn with that
  picture (its longer side is the brush size), “Rotate” sets its angle. “Add picture…” in the list,
  dragging a picture onto the window with Shift, or opening a GIMP brush `.gbr` turns a picture into the
  current tool's tip: dark on white or opaque on transparent is paint. Tips are kept in the settings
  folder under `tips`. The default dab spacing is 10 % of the size. With a tip, the brush cursor is the
  picture's frame with its rotation.
- Patterns are fixed to the canvas, not to the stroke. The pattern files come from SAI: if the
  lists are empty, enter the SAI folder (with `blotmap` and `brushtex`) right in the section;
  the paper folder works too.

### Color

- Color wheel: the ring is hue, the square is saturation (to the right) and value (up).
- The buttons above the wheel show and hide color panel views: the wheel, RGB sliders (red,
  green, blue, 0–255), HSV sliders (hue 0–359, saturation and value 0–100; “Window → HSV/HSL Mode”: V-HSV as SAI’s default, HSL or HSV). Several can be shown
  at once; the choice is remembered.
- Color Mixer (fourth button): four bars. Clicking an end of a bar puts the current color there;
  clicking or dragging in the middle takes the mixed color — a marker shows where.
- Swatches (fifth button): click a cell to take its color; right-click — “Register Color” (store
  the current one) or “Delete Color”. Two rows are visible, scroll for the rest. The mixer and
  swatches are kept between runs.
- A color you paint with goes into the first empty swatch cell by itself (filled cells stay). A GIMP (`.gpl`) or
  Krita (`.kpl`) palette can be opened with “File → Open” or dropped onto the window — its colors
  fill the swatches in order (replacing the old ones).
- The arrow by the main and background color squares (or the X key) swaps them.
- Color picker: Alt + click (or the Color Picker tool; its panel has “Current layer only”).
- **Straight line** (quick bar button, as in SAI): every stroke is a straight line from where it
  starts. Shift + press draws a line from the end of the last stroke.
- **Rotate view** is a tool: drag on the canvas, double-click to reset; the angle and rotate
  buttons are in the navigator too.
- Tools have their own buttons on the panel: selection — mode (replace, add, subtract,
  intersect; Shift and Alt still work), All, None, Invert; move — flip and rotate the layer;
  zoom, hand and rotate — zoom, Fit, rotate, mirror, reset position.

### Fill

- The **bucket** fills a connected area of similar color with the main color. Tolerance works
  as for the magic wand.
- **Close gaps** (bucket and wand): the fill does not leak through breaks in the line art up to twice
  that width; **Under line** — the fill goes that many pixels under the line edge so no light rim is
  left. On very large canvases filling with these options is slower.
- **Alt+Delete** fills the selection (without a selection — the whole layer).

### Gradation

The “Gradation” cell in the brush grid (as in SAI). Drag across the canvas — the color changes
evenly from start to end; the ends can be moved and the settings changed until you press
“Apply” (Enter); Esc cancels.

- “Form”: “Straight” or “Circle” (from the start outwards).
- “Color”: “FG Color to BG Color” or “FG Color to Transparency”.
- “Reverse” swaps the ends, “S-Curve Gradation” makes it smoother near the ends.
- With a selection the gradient lands only inside it; the transparency lock keeps the opacity.

## Layers

- Buttons and the “Layer” menu: new layer, new folder, new linework layer, new shape layer,
  copy, delete, erase (Delete), transfer down (F), merge down (Ctrl+E), order, flip,
  rotate layer by 90° (around the middle of its painting), “Outline…”, “Luminance to Opacity”,
  “Rasterize Layer”, “Merge Visible Layers” (visible layers into one new layer, hidden ones stay),
  “Flatten Image”, “Layer Property…” (name; also a double-click on the layer).
- **“Outline…”** — as in SAI: outlines the layer's painting with the main color, 1–25 px inside,
  centered or outside; “Apply to Canvas Edges too” also outlines where the painting meets the
  canvas edge.
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

Textures are taken from SAI's `papertex` folder (GrazySAI does not ship them). If the list is
empty, type the path to that folder in the field above the list and press OK; the path is
remembered. `.sai2` files with paper open and save with it.

### Colorize by line art

On a new layer put color spots in the areas to fill (one spot — one area, different colors — different
areas) and choose “Layer → Colorize by line art”. The colors spread up to the lines of all other visible
layers; small gaps in the outline (up to ~6 px) do not let color through when both sides have their own
spots. The result is a new “Colorize” layer in Multiply mode below the spot layer, as plain pixels you
can touch up with a brush. The spot layer stays — hide it if you like.

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
| Lasso (L) | Draw around. With “Polygonal” in the tool settings, clicks place corners, Enter or a double-click closes, Esc cancels; “Anti-Aliasing” gives a soft or stepped edge |
| Magic Wand (W) | Click; tolerance is in the panel |
| SelPen and SelErs | Paint the selection with a brush |

- Shift — add to the selection, Alt — subtract, Shift+Alt — keep the intersection.
- Ctrl+A — select all, Ctrl+D — deselect, Ctrl+H — hide or show the outline.
- The “Selection” menu has invert; “Dilate Selection…” and “Erode Selection…” by 1–25 px with a
  preview (corners are cut at 45°, as in SAI); dilate and erode by 1 px.
- Brushes, fill, filters and Delete act only inside the selection.

## Clipboard

- **Ctrl+C** copies the layer's painting inside the selection (without one — all of it),
  **Ctrl+X** cuts, **Ctrl+V** pastes as a new layer above the current one (our own copy lands in
  the same place, a picture from another program in the middle of the canvas); Move is turned on
  after pasting. As in SAI.
- “Edit → Copy by Selection Bounds” copies the whole selection rectangle, without the soft edge.
- “File → Create Canvas from Clipboard” (Ctrl+B) makes a new canvas the size of the clipboard picture.

## Move and transform

- **Move**: the Move tool, Ctrl + drag with any tool, or the arrow keys, as in SAI: Ctrl+arrows
  cut the selected part and move it with the outline, Alt+arrows move only the selection outline;
  without a selection both move the layer (by 1 px; with Shift — by 10).
- **Transform (Ctrl+T)**:
  - corners and side midpoints — size;
  - Ctrl + corner — free deform (each corner separately);
  - the panel has SAI's modes: “Freeform”, “Resize”, “Deform” (corners without Ctrl), “Rotate”;
    “Perspective” 0–100 — 0 stretches evenly (bilinear), 100 adds perspective;
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
- “Window → Reference image…” (or drag an image onto the window with Ctrl) — a reference picture in its
  own small window over the canvas. The “Opacity” slider makes the window see-through so the canvas shows
  under it; “Zoom” or the wheel over the picture resizes it; the eyedropper or Alt + click picks a color;
  drag the window to move it. Several can be open.
- “View → Wrap-around mode (the canvas repeats)” — the canvas is shown tiled 3×3, and a stroke that leaves
  one edge continues from the opposite one: handy for seamless textures. Fill, selection and filters work
  as usual.
- “View → Pixel grid (from 800%)” — thin lines between canvas pixels at 800% zoom and above.
- “Ruler → Symmetry” repeats every stroke mirrored: “Left ↔ right”, “Top ↔ bottom” or “Both axes”
  (the axis is the middle of the canvas, shown dashed). Works for all brushes, the selection pen and masks.
- “View → New View” adds another tab of the same canvas with its own zoom, position, rotation
  and mirror (the tab shows the view number). Closing a view keeps the canvas while another view
  of it is open. A key can be assigned in Hotkeys.
- “View → New Floating View” opens a second window onto the same canvas with its own zoom,
  position, rotation and mirror; you can paint in it.
- “Canvas → Trim Canvas by Selection” makes the canvas the selection rectangle.
- “Canvas → Background”, as in SAI: transparent (shown as a bright or dark checker, white, black
  or your color) or opaque white, black or your color; “Canvas Background Color…”. The background
  is saved in `.sai2` the same way SAI does.
- “Canvas → Layer Effect Color” sets how fringe effects and paper textures tint a layer: Normal,
  Vivid, Deep, Multiply, Ver.1 Like. It applies to the whole canvas and is saved in `.sai2`.

## Ruler

“Ruler → Show Ruler” (Ctrl+R). While the ruler is shown, every stroke lies on it entirely,
wherever it starts.

- **Straight**: a translucent band, the line is its edge. Ctrl + drag the band to move it;
  with “Lock Pivot” — to rotate it around the pivot (the cross).
- **Ellipse**: a ring; strokes lie on its outer edge. Holding Ctrl shows a frame: inside —
  move, outside — rotate, a side or a corner — stretch.
- **Perspective: 1, 2 or 3 points**: the horizon, rays from the vanishing points (green, blue,
  red) and guides through the cursor. A stroke runs straight from where it starts toward the
  vanishing point its first movement points at (one point also gives horizontals and verticals,
  two points give verticals). Ctrl + drag a vanishing point to move it. Our own version: it is
  not saved in the file.
- “Ignore Ruler” (menu or the quick bar button) — draw freely without hiding it; “Reset
  Ruler” — place it again in the middle of the view.

## Filters

The “Filter” menu; with a selection the filter acts only on the selected part.
A filter window opens with the settings of the last time (even after Cancel); “Reset” restores
the defaults. The preview is computed in the background, so the window does not freeze.

- **Hue and Saturation** (Ctrl+U), with a “Colorize” mode.
- **Brightness and Contrast** (and color “Depth”).
- **Levels** (input and output black and white points, gamma) and **Curves** (tone through a curve
  with two points, like the pressure curve).
- **Gaussian Blur** (radius 0.1–40) in the Blur submenu; next to it, as in Clip Studio Paint:
  Average, Blur and Blur More, Lens Blur (bokeh), Motion, Radial and Spin Blur, Smoothing.
- **Sharpen**, **Sharpen More**, **Unsharp Mask** (radius, amount, threshold) in the Sharpen submenu.
- **Remove Dust** (small specks or holes up to a size) and **Adjust Line Width** (thin or thicken by
  1–20 px) in the Correction submenu.
- **Median** (Correction, radius 1–5): removes isolated specks without blurring edges.
- Effect: chromatic aberration, noise, mosaic, crystallize, retro film; Render: Perlin noise (the
  pattern replaces the layer content).
- Pencil Drawing (thickness, darkness, hatching, keep color) and Artistic (stroke, brightness
  levels, outlines) in the Effect submenu.
- Distort: Pinch, Ripple, Fish-eye Lens, Polar Coordinates, Wave, Twirl; the center and radius are
  set with sliders.

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
  - Edit (I) — drag a control point; a click on a point selects it (or deselects). Dragging a
    selected point moves all selected ones. Curves and points are drawn over the canvas as in SAI:
    points are green dots, ends are yellow diamonds, selected points are large circles.
    “Selection → Select CPs in Selection / Select Strokes with Selection” take the points (or whole
    paths) under the selection outline and drop the outline; “Deselect All CPs” clears them;
  - Pressure (P) — drag a control point up or down.
- **Shape layer**: pick a rectangle, ellipse or triangle in the panel and drag on the canvas.
- **Raster tools**: brushes, fill and filters do not edit these layers. “Layer → Rasterize
  Layer” makes the layer normal.
- **Saving**: curves are saved to `.sai2` as curves.

## Files

| Format | Open | Save |
|---|---|---|
| `.sai2` | Yes (Ctrl+O) | Ctrl+S, Shift+Ctrl+S (in the same format as SAI 2) |
| `.psd` | Yes (RGB and grayscale, 8 and 16 bit, layer masks) | “Save As” or “Export → PSD”: with layers, folders, modes |
| `.psb` | Yes (same as `.psd`) | “Save As” or “Export → PSB”: Photoshop large document, sides up to 300,000 px |
| `.ora` (OpenRaster — Krita, GIMP, MyPaint) | Yes, with layers and folders | “Save As” or “Export → OpenRaster”: layers, folders, modes, opacity |
| `.png`, `.jpg`, `.bmp`, `.tga`, `.tiff`, `.webp`, `.qoi` | Yes, as one layer | “File → Export” (flattened image; JPEG at quality 92, WebP lossless) |
| `.sut`, `.bfps` (Clip Studio Paint brushes) | Yes: the brushes are added at the end of the grid | — |
| `.kpp`, `.bundle` (Krita brushes), `.gbr`, `.gih` (GIMP brushes) | Yes: Krita brushes go to the end of the grid, GIMP brushes become the current brush’s tip | — |

Krita brushes: a `.kpp` preset or a `.bundle` pack open the same way — “File → Open” or drag and drop;
the brushes go to the end of the grid (all of them from a pack). Carried over: size, opacity and flow,
round-brush hardness (“Fade”), spacing, pressure on size and opacity (the low end from the curve), eraser,
smudging, and an image tip (`.png`, `.gbr`, `.gih`) with its rotation. The tip is looked up in the pack,
next to the preset, in a `brushes` folder and in the Krita packs on this computer. Masked brush, texture,
scatter and other Krita sensors are skipped; the status bar lists what was skipped.

Clip Studio Paint brushes: an exported brush `.sut` (in CSP: Sub tool palette → menu → “Export sub tool”)
or the `BrushPreset.bfps` palette open with “File → Open” or by drag and drop. Carried over: size
(millimetres at 350 dpi), opacity and flow (into density), hardness (to the nearest step), fixed spacing,
pressure on size and opacity, color mixing, and the paper texture from a `.sut` (stored in the settings
folder under `brushtex`). The image tip is taken from the CSP material thumbnail (an approximation). What GrazySAI lacks — spray,
dual brush, tip thickness, watercolor edge — is skipped, and the status bar lists it. The stroke looks like CSP but is not identical: the
engines differ. A `.bfps` palette has no images (it refers to CSP materials), so only settings are taken.

“File → Save with number” saves next to the file under the next free number: `picture.sai2` → `picture_001.sai2`, `picture_009.sai2` → `picture_010.sai2`.

“Help → Options…” has “PSD and PSB — 16 bits per channel”: GrazySAI works with 15 bits per channel inside,
and a 16-bit file keeps smooth gradients without 8-bit steps; an sRGB profile is embedded so other programs
do not take the picture as linear (checked in Krita). The file is noticeably larger.

SAI’s Shine, Shade, Shade/Shine, Burn, Dodge and Burn/Dodge modes do not exist in PSD — normally they are
replaced by the nearest Photoshop modes, and the look shifts slightly. “PSD and PSB: keep the exact look”
writes such layers as normal layers holding the finished result (what is under the layer together with
it): the look is the same, but the mode can no longer be changed in another program. Layers inside
non-pass-through folders are written as usual.

Layer masks are written to `.psd`/`.psb` (not to `.ora`). Paper, layer effects and curves do not go
into `.psd` and `.ora` (the layer is written as pixels); other programs show a clipped layer in `.ora`
as a normal one.

Every canvas has its own tab at the bottom of the window, as in SAI: “New”, “Open” and “Create
Canvas from Clipboard” open a new tab (an untouched empty canvas is replaced). Click a tab to
switch to it; the cross, the middle button or Ctrl+W (“File → Close”) closes it, asking first if
there are unsaved changes. Crash recovery keeps a copy of the active canvas.
The “New” window remembers the last size, also after a restart.

“Help → Options…”, as in SAI: layer properties on double-click, reversed wheel zoom, brush size
keys by a quarter, temporary tool, color and function switching while a key is held (threshold in
ms), returning to the previous tool by pressing its key again, protection from accidental layer
moves after Ctrl/Alt shortcuts, history memory and the recovery copy interval.
“Right-click on the canvas opens a palette” (off by default): recent colors, swatches and the grid
brushes appear at the cursor — click to pick, Esc or a click elsewhere closes it.
There is also “Pen pressure curve”: two points on the graph change how the brush answers pressure
(above the line — softer, below — harder); “Straight” restores pressure as is. It affects the pen, not the mouse.

**File viewer**, as in SAI: “File → Open/Save by File Viewer…”, and with “Always use the file
viewer” ticked also the usual Open and Save As. The path as a chain of folders, Explorer and
Bookmarks on the left, picture thumbnails on the right (`.sai2`, `.psd`, `.ora` and images; cached
in the settings folder). Buttons: up (Backspace), refresh, new folder, rename, delete (to the trash
on Linux), bookmark the current folder; View (thumbnails or list), Order (name, date, size), Type.
Double-click opens; Enter is OK, Esc cancels.

“File → Recent” lists the last 10 files opened or saved.

“Open”, “Save As” and “Export” show the system window: File Explorer on Windows, the desktop's
own dialog on Linux (KDE, GNOME). Without one, a simple window with the file path appears.

Text layers from `.sai2` are shown with the font chosen in SAI (if it is installed) and saved as
they were. The Text tool (“T” button) works as in SAI: a click on the canvas creates a
text layer at once and you type right on the canvas (Enter for a new line, arrows, Home/End,
Backspace/Delete; Esc or another tool finishes; click the selected text to edit it). The
settings are on the tool panel: Vertical, color (the main color), size, Bold, Italic and a font
list with search; they change the text immediately. The whole edit is one undo step. Brushes don't paint on text — rasterize it first
(“Layer → Rasterize Layer”). Perspective
rulers from newer SAI versions are not shown but are saved too.

## Graphics card

On laptops with two GPUs Windows picks the integrated one for new programs. GrazySAI asks for
the discrete one (NVIDIA or AMD), so the canvas and big brushes are faster. You can override it in
the driver settings or in “Settings → Display → Graphics”. On Linux choose the discrete GPU at
launch: `DRI_PRIME=1 GrazySAI` (Intel/AMD), `__NV_PRIME_RENDER_OFFLOAD=1
__GLX_VENDOR_LIBRARY_NAME=nvidia GrazySAI` (NVIDIA).

## Settings and recovery

- **Settings** are saved on exit and restored on start: brushes, colors, stabilizer, magic
  wand tolerance, interface language. Where they are:
  - Windows: `%APPDATA%\GrazySAI\`;
  - Linux: `~/.config/GrazySAI/`.
- **Recovery**:
  - every 3 minutes, if there were edits, GrazySAI saves a copy of the canvas in the background;
  - if the program closed abnormally, on the next start it offers to recover the work;
  - the cause of a crash is written to `crash.log` in the same folder — attach it to a bug report
    on GitHub.

## Shortcut keys

Keys can be reassigned: “Window → Shortcut Keys…” or “Help → Keys…” (95 actions: files, edit,
clipboard, view, panels, tools; search by name) — click an action's key, then press the new
combination (Esc — cancel; the key is taken from another action if it was in use); “As in SAI”
restores all defaults. Assignments are saved in the settings. Any grid tool can also get its own
key in its “Property…” (right-click the tool), like SAI's Shortcut Key; it wins over the general
one. The defaults are below.

| Keys | Action |
|---|---|
| Ctrl+Z / Ctrl+Y (Shift+Ctrl+Z) | Undo / redo |
| Ctrl+S / Shift+Ctrl+S | Save / save as |
| Ctrl+C / Ctrl+X / Ctrl+V | Copy / cut / paste as a new layer |
| Ctrl+B | Create canvas from clipboard |
| `[` / `]` | Brush size down / up |
| Ctrl+`+` / Ctrl+`−` | Zoom |
| N, B, V, C, E | Pencil, airbrush, brush, water, eraser (on a linework layer: N — pen, E — linework eraser) |
| I, P | On a linework layer: edit, pressure |
| M, L, W | Rectangle, lasso, magic wand |
| Ctrl+A / Ctrl+D / Ctrl+H | Select all / deselect / hide the outline |
| Delete / Alt+Delete | Erase / fill (within the selection) |
| F / Ctrl+E | Transfer down / merge down |
| Ctrl+T, Enter, Esc | Transform, apply, cancel |
| Ctrl+arrows (with Shift — by 10) | Cut the selection and move it (no selection — the layer) |
| Alt+arrows (with Shift — by 10) | Move the selection outline (no selection — the layer) |
| Ctrl+`[` / Ctrl+`]` | Bring layer forward / send backward |
| Shift+Ctrl+`[` / Shift+Ctrl+`]` | Bring layer to front / send to back |
| Alt+`[` / Alt+`]` | Select the layer above / below |
| Alt+`\` | Open or close the layer folder |
| Ctrl + click a folder's eye | Mark the folder: “Change Visibility of Marked Folder” and “Show Marked Folder Alone” (assignable keys) act on it wherever the current layer is inside it |
| Ctrl + drag | Move the layer or the selection |
| Ctrl+U | Hue and saturation |
| Ctrl+R | Show or hide the ruler |
| Shift+PageUp / Shift+PageDown / Shift+Home / H | Rotate view / reset / flip view |
| X | Swap main and background color |
| Alt + click | Color picker |
| Space + drag | Pan the view |

## How it still differs from SAI

A feature-by-feature comparison is in [docs/32-sai-comparison.md](32-sai-comparison.md). The main
points:
- the perspective ruler is our own (1–3 vanishing points, not a layer, no grid) and is not saved;
- Water Color with “Amplify Density” is fitted to measurements, not taken from SAI's code;
- folder marks for visibility are not saved in the file;
- filters beyond SAI 2017's three working ones are our own, as in Clip Studio Paint.

## If something is wrong

Report a bug on the [Issues](https://github.com/grazyping/GrazySAI/issues) page (English or
Russian):
- what you did;
- what you expected and what happened;
- the GrazySAI version (in the window title) and the system (Windows or Linux).

If you can, attach the file or a screenshot.

**Self-test.** `GrazySAI --self-test report.txt` (on Windows: `Start-Process -Wait .\GrazySAI.exe
"--self-test report.txt"` in PowerShell) checks without a window what depends on the system: the temp and
settings folders, writing and reading every format (with non-Latin file names), SQLite for CSP brushes,
brush tips, system fonts. The result is OK/FAIL lines in the report (without a file name —
`self-test.txt` in the settings folder); your files are not touched. Attach the report to a bug report.
