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

- **Windows**: the pen comes through WinTab (pressure, tilt, rotation) when the tablet driver
  provides it, otherwise through Windows Ink (enable it in the driver; for XPPen — “Windows
  Ink”). WinTab can be turned off: “Settings → Windows: pen via WinTab”.
- **Linux**: pressure, tilt and rotation are read directly (XInput 2), nothing to set up.
- Pen tilt and rotation are brush dynamics inputs (“Pen tilt”, “Tilt direction”, “Pen
  rotation”). Brands, drivers and checked models — `docs/42-tablets.en.md`.
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

The layout follows SAI 2 (checked on Windows next to SAI 2026):

- **At the left edge**: navigator, layer properties (blend mode, opacity, locks, “Clipping
  Group”, “Selection Source”), layer buttons, layer list with thumbnails.
- **Next to the canvas**: color wheel, tool group tabs, common tools (selection, lasso, magic wand,
  bucket, move, zoom, hand, color picker), main and background color, brush grid, stroke preview,
  current brush settings, size grid.
- **Top**: menus and the quick bar (undo, selection, zoom, view rotation and flip, stabilizer).
  **Bottom** — the status bar: messages, pen and pressure, “Drive Usage” (the drive holding the
  undo history) and memory use.

Either column can move right: “Window → Show Navigator and Layer Panels on Right Side” and “Show
Color and Tool Panels on Right Side”.

The **navigator**, as in SAI: the whole canvas as a small copy, a dark frame for what the window
shows, with a line from its centre to the top of the view (it turns with the view). Dragging from
any point moves the view by the same amount; a click does not jump the view.

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
tool for “Duplicate”, “Delete”, “Property…” (its own name and a second line under it) and “Move to
Group”. The new-tool list shows icons. The grid is kept between runs.

**Tool groups** are tabs above the common tools, as in SAI 2: “Basic” and “Binary” (binary and pixel
art pens, binary erasers, bucket). Right-click a tab for “New Group”, “Rename…”, “Delete Group” (its
tools move to the first group). Picking a tool by its key opens its group.

| Setting | What it does |
|---|---|
| Hardness | Five edge steps, from the softest to the hardest |
| Size | Diameter in pixels; `[` and `]` — previous or next size from the size grid. The list left of the bar is the bar range, as in SAI: ×0.1 (0.1–10), ×1 (1–100), ×5 (5–500), ×10 (10–1000), ×50 (50–5000) |
| Minimum | Size at the lightest pressure; the pen icon in the row turns pressure on size on |
| Density, Min Density | Stroke opacity and its value at light pressure; the pen icon — pressure on density |
| “Pressure affects blending” | Mixing brushes: pressure changes blending |
| Form, Texture | The list and strength are always shown; the triangle on the left opens the rest |
| Blending, Dilution, Persistence | Brush, Water and Marker only (see below) |
| Advanced Settings: Sharpness, Amplify Dens., “Ver1 pressure on density” | Fine tuning of edge and density, as in SAI; the icons on the right of the header are “Restore Reserved Settings” and “Reserve Current Settings” |

The size grid can show numbers only and sit above the brush settings: “Window → Show Brush Size List
Items in Numerals Only” and “Show Brush Size List on Upper Side”. The bottom of the View menu lists
the open canvases with their zoom, as in SAI.

**Stabilizer** (quick bar): 0–15 and S-1…S-7 — the higher, the smoother the line and the more
it lags behind the pen.

**Mixing with the canvas** (Brush, Water, Marker):

- “Blending” — how much of the canvas color mixes into the paint;
- “Dilution” — how much of the canvas transparency the paint picks up;
- “Persistence” — how long the paint keeps being dragged along;
- “Blur Pressure” — below this pressure the brush only smears.

Water also blurs the layer under the stroke.

**Dynamics** (the “Dynamics” section): “input → curve → parameter” links on top of SAI pressure. Inputs:
pressure, speed, stroke direction, distance from the stroke start, random (speed and distance have a
“Scale” — how many pixels count as the full range). Parameters: size, density (multipliers), tip rotation
and thickness, scatter across the stroke. The curve has 4 points: the end ones move up and down, the
middle ones anywhere; the curve may also fall. Examples: “Distance → Size” with a falling curve tapers the
stroke; “Random → Scatter” gives splatter; “Speed → Density” falling makes fast strokes lighter. Several
links to one parameter multiply (rotation and scatter add up). “From plugin” — custom dynamics from an
installed plugin (a function per dab, docs/36-plugins.en.md). Dynamics are saved in the brush file.

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
- “Thickness” squashes the tip along its second axis (a round brush becomes an ellipse), “Rotate” sets the
  angle, “along stroke” turns the tip with the stroke direction: that makes a calligraphy pen (for
  example, thickness 20 %, rotation 45°).
- A picture set (GIMP `.gih` brushes, sets inside Krita and CSP brushes) — the dabs change along the
  stroke: in order or “random” (the “Set” row in the “Form” section).
- Photoshop brushes `.abr` (“File → Open”, drag and drop or “Add picture…”): every brush in the file
  becomes a separate tip in the “Tip” list, the first one goes to the current brush. Only the brush
  pictures are taken; Photoshop settings (spacing, scatter and so on) are not carried over.
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
  - **Lattice and Mesh** (grid deformation from SAI 2 2026): a grid of nodes lies over the picture —
    Division 2×2…5×5 or your own up to 32×32. With Lattice the nodes pull the picture softly (the
    picture does not reach the node), with Mesh the picture passes through the nodes. Drag the
    nodes; inside — move the whole grid. Overlapping: where the picture folds over itself —
    Overwrite or Blend. If the frame was already moved, it is applied first and the grid lies on
    the result;
  - Enter — apply, Esc — cancel.

## Canvas and view

- The “Canvas” menu:
  - change resolution (the image is resampled);
  - change canvas size (the image is not scaled, 3×3 anchor);
  - flip;
  - rotate by 90°;
  - “Convert to Grayscale…” — all layers become gray (with a warning; only Undo brings colour back).
- “File → New” has size presets: pick one from the list, “+” adds the current size, “Edit” gives the
  preset the current size, ↑ ↓ reorder, “Delete”; the preset name is edited below.
- The bottom of the View menu lists the open canvases to switch to. “File → Recent → Clear List”.
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
  **Radial** — 1–20 parts around a centre: “Mirrored” gives as many axes as parts (2N copies),
  without it N rotated copies; “Angle” turns the first axis, “Centre” is a canvas point. These
  settings are kept by the program, not in `.sai2`.
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
- **Parallel Lines**: the same band as Straight, but the stroke goes straight, parallel to it, from
  where it starts.
- **Concentric Ellipses**: the same ring as Ellipse, but the stroke follows the ellipse with the same
  centre, shape and tilt through the start point.
- **Vanishing Point (rays)**: the stroke goes straight from its start toward the point; Ctrl + drag
  moves the point. A guide through the cursor shows where the stroke will go.
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

## Plugins

A plugin adds commands to the “Plugins” menu (a `.gsplugin` file). To install: “File → Open” or drag the
file onto the window → a window with the description → “Install”. A command with parameters opens a small
window (sliders, checkboxes, choices) → “OK”. The plugin changes the current layer (or makes new ones) and
runs in a sandbox: no access to files, network or the system, with memory and time limits; everything can
be undone. “Plugins → Manage plugins…” — the list, removal, errors and the log; “Open plugins folder” —
`plugins/` in the settings folder. How to write your own — [docs/36-plugins.en.md](36-plugins.en.md),
examples — `plugins/examples/` in the repository.

## Brush workshop (GrazySAI Studio)

Step by step: making and sharing a brush — [docs/37-brushes-guide.en.md](37-brushes-guide.en.md).

A separate program next to GrazySAI: `GrazySAI-Studio` (`GrazySAI-Studio.exe` on Windows). There you make a
brush and try it at once, without touching the painting program.

- On the left — settings: name, kind (pencil, brush, watercolor…), size and density with pressure,
  hardness, spacing, mixing, tip (a picture, GIMP brushes, or “Create”: circle, square, spray, chalk),
  rotation and thickness, texture, “input → curve → parameter” dynamics.
- In the middle — the test strip: the sample stroke is redrawn on every change, and on the rest of the
  strip you can draw with the mouse (pressure from the “Mouse pressure” slider) or a pen. “Clear” wipes it.
- “Save” / “Save as…” — a `.gsbrush` brush file you can share.
- **“Export to GrazySAI”** — the brush appears in the running GrazySAI at once (at the end of the grid,
  labelled “Studio”); exporting again replaces it instead of adding a new one. No need to close either
  program. The brush is kept in the settings folder under `brushes/dev/`.
- You can drag a picture or a brush file onto the workshop window.

The **“Plugin”** tab is for plugins: `plugin.txt` and `main.lua` in the editor on the left, “Test” on the
right — choose a command, set its fields and press “▶ Test on the sample picture” to see “before” and
“after”, the log and an error with its line number. “New from example” (invert, grid, noise), “Save”
(`.gsplugin`) and **“Export to GrazySAI”** — the plugin appears in the “Plugins” menu of the running
GrazySAI at once (`plugins/dev/` folder).


| Format | Open | Save |
|---|---|---|
| `.sai2` | Yes (Ctrl+O) | Ctrl+S, Shift+Ctrl+S (in the same format as SAI 2) |
| `.psd` | Yes (RGB and grayscale, 8 and 16 bit, layer masks) | “Save As” or “Export → PSD”: with layers, folders, modes |
| `.psb` | Yes (same as `.psd`) | “Save As” or “Export → PSB”: Photoshop large document, sides up to 300,000 px |
| `.ora` (OpenRaster — Krita, GIMP, MyPaint) | Yes, with layers and folders | “Save As” or “Export → OpenRaster”: layers, folders, modes, opacity |
| `.png`, `.jpg`, `.bmp`, `.tga`, `.tiff`, `.webp`, `.qoi` | Yes, as one layer | “File → Export” (flattened image; JPEG at quality 92, WebP lossless) |
| `.gsbrush`, `.gsbrushes` (own brushes) | Yes: into the brush library | Right-click a cell → “Save brush…”, “File → Save brush set…” |
| `.sut`, `.bfps` (Clip Studio Paint brushes) | Yes: into the brush library | — |
| `.kpp`, `.bundle` (Krita brushes), `.gbr`, `.gih` (GIMP brushes), `.abr` (Photoshop brushes) | Yes: into the brush library | — |

Own brushes: right-click a grid cell → “Save brush…” (a `.gsbrush` file) or “File → Save brush set…”
(all grid tools — `.gsbrushes`). The file holds all settings, the image tip, the texture and a sample
stroke; open it with “File → Open” or by drag and drop — the brushes go to the end of the grid. Blots
and bristles from the SAI folder travel by name only. File layout: `docs/35-gsbrush-format.md`.

### Brush library

Brushes from other programs are collected in the **library** — the program's own folder (`library/`
in the settings folder; the original files are copied there too, into `originals/`).

- **Add**: drop brush files onto the program window — `.sut`, `.bfps` (Clip Studio Paint), `.kpp`,
  `.bundle` (Krita), `.abr` (Photoshop), `.gbr`, `.gih` (GIMP), `.gsbrush`, `.gsbrushes` (GrazySAI) —
  **many at once or a whole folder** (subfolders are searched too). Same with “File → Open” or the
  “Add…” button in the library window (several files can be picked). A brush of the same set that is
  already there is not added twice. With Shift a GIMP or Photoshop image brush still becomes the tip
  of the current brush.
- **The window** “Window → Brush Library…” (available when the library has brushes; it opens by
  itself on “New” after adding). Like Krita's preset docker, Clip Studio's material palette and
  Photoshop's brushes panel: sections on the left — “All”, “Favorites”, “New”, “Source” (Clip Studio,
  Krita, Photoshop, GIMP, GrazySAI), “Sets” (by original file), “Tags”; on top a search by name, set
  and tag, card size, “Cards” or “List” view; a card shows a stroke sample, the name, source and set
  (a dot — new, a star — favorite).
- **Separate window** (default) — its own system window: move it to another monitor and turn on
  “Always on top”; unchecked — a floating window inside the program.
- Click a card to select it (below — name and comma-separated tags); **double-click** or “Put into
  the Grid” puts the brush into the grid, in the “Library” tab, and selects it (again — it is just
  selected). Right-click — “Add to Favorites”, “Remove from Library”.

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

“File → Watermark…” — text (your font and color) or a picture (PNG with transparency etc.), position (a corner,
the center or all over the picture), size, opacity, margin; a preview on the picture. With “Add on image
export…” the mark goes into image exports (PNG, JPEG, WebP, BMP, TGA, TIFF, QOI); it is never written into
the picture itself, `.sai2`, `.psd` or `.ora`.

### Scatter and Effect Pen

The “Scatter” grid tool (Scatter from SAI 2 2018+): each stroke step places one or more shapes (the tip
from “Form → Tip” or a circle). The “Scatter” section: “Angle Control” (none, auto, pen direction), angle
and its jitter, shape scaling and size jitter, “Scattering” — a shift across the stroke up to two brush
sizes (“All Direction” adds a shift along it, “Gaussian” keeps shapes closer to the line), “W:H” and its
jitter, “Count” per step and its jitter, “Absolute Size”, “Integer Pos.”. Colour: “FG to BG”, hue,
saturation and brightness jitter — for the whole stroke or “Apply to Each Shape”. The distributions are
checked against live SAI 2026.

The “Effect Pen” is a plain pen whose stroke goes on with the blend mode chosen in the “Mode” list
(multiply, screen, overlay, shade, shine and others): over paint by the mode formula, over transparent
pixels as normal paint. The “Mode” list works for Scatter too.

### Pixel Art Pen and binary tools

The “Pixel Art Pen” (Binary tab in SAI 2) paints dots without anti-aliasing and without pressure: the size
is rounded to an integer and a dot is a circle of pixels exactly as in SAI (checked at sizes 1–20). There
are also a binary eraser, binary selection pen and eraser.

### Brush details

- Above the brush settings — a preview stroke of this brush; it follows the settings.
- “Misc → Anti-Hook” cuts bends at the stroke ends shorter than the given length (in screen pixels) when
  the pen lifts; while the pen moves, the stroke end lags by that length.
- Shift pressed during a stroke keeps it straight — horizontal or vertical (by the first movement);
  release Shift and the stroke is free again.
- “Options → Minimum pressure for brush stroke” — lighter pressure does not paint (guards against
  accidental touches), and brush pressure is counted from the threshold; the cursor shows the
  smallest-size circle.
- Right-click a tool cell: “Reserve Current Settings”, “Restore Reserved Settings”, “Undo Restore”.

### Asset Manager

“Window → Asset Manager…” (from SAI 2 2020+):
- **Swatches sets** — several swatch palettes; switch them also with the list above the swatches.
- **Scratch pads** — save the current scratch pad under a name, open a saved one.
- **Layer paper, brush forms, brush textures, tips and scattering shapes** — your own files of these
  kinds: rename, delete, put into groups. “New from the current layer” makes an asset from the selected
  part of the layer or everything drawn: black on white, white on black or by transparency.

### Smudge

The “Smudge” grid tool (from SAI 2 2017-11+): drags canvas paint along with the brush, like a finger; lays
no paint of its own. Strength follows density and pressure. “Coloring” (off by default) mixes in the brush
color; below “Uncolor pressure” the brush only drags. Behaviour fitted to the live SAI 2026 (our own model,
not SAI's code).

### Blur

The “Blur” grid tool (from SAI 2 2020+; right-click an empty cell): a brush that only blurs what is under
it and lays no paint. “Blur width” sets how far it blurs; density follows pressure (light pressure — a
light blur). Checked against the live official SAI 2 2026.

### Lasso Fill

The “Lasso Fill” grid tool (right-click an empty cell; in SAI 2 since 2026): outline an area and it is
filled with the main color. Outline freeform, polygonal (clicks; double-click or Enter finishes, Esc
cancels) or both (freeform while the button is held).

“Enclosed regions only” fills not the whole outline but what lines enclose inside it (handy for filling
several spots in one move): “transparent”, “transparent (fuzzy)”, “similar color” (the color where the
outline starts), “non-transparent” and their tolerance. **“Gap tolerance”** closes gaps in the lines — a
ring with a gap is filled when the tolerance exceeds half the gap. “Dilate” grows the fill by that many
pixels; “Ignore painted parts” fills only empty pixels; “Search in” — the current layer, layers marked
“Selection Source”, or the merged image. The fill stays inside the selection unless “Ignore selection” is
on.

### 3D layer

“Layer → New 3D layer…” (or “File → Open”, or drag the file into the window) turns a glTF (`.glb`,
`.gltf`) or OBJ model into a layer: a figure for proportions, an object, a room, a street. No models are
bundled — bring your own (Blender saves glTF directly; models from websites, VRoid, Mixamo via Blender
work). Compressed meshes (Draco, meshopt) are not read — save the model without compression.

The “3D” window opens by itself when a 3D layer is selected (again — “Layer → 3D window”):

- “Mouse on canvas”: **camera** — drag to orbit, with Shift to pan, with Ctrl to zoom; **pose** — the
  joints show as dots; grab one and drag: the bone after it turns toward the pointer (the elbow bends
  the forearm, the shoulder the whole arm); **painting** — the mouse works as usual.
- “Camera”: orbit, tilt, distance, field of view (smaller — flat “telephoto” perspective, larger — “wide
  angle”), horizon, “Fit to frame”.
- “Display”: color and light (the model's colors and textures), clay figure (good for copying forms),
  silhouette, outline; light direction, line color and width.
- “Objects”: one scene can hold several models (“Add…”), each with its own position, rotation, size.
- “Joints and parts”: the model tree; the checkbox hides a part; the selected joint has exact X, Y, Z
  angles, “Reset”, “Reset the whole pose”; scene parts without a skeleton can be moved.
- “Morphs (shape)”: shapes built into the model (weight, facial expressions etc.).

Every edit is one undo step (Ctrl+Z). Paint on an ordinary layer above; “Layer → Rasterize layer” turns
the 3D layer into an ordinary picture. The model is stored inside `.sai2`; SAI 2 opens such a file and
shows the layer's last picture. A 3D layer can be excluded from the process recording like any layer.

### Process recording

Some clients ask for proof that you painted the picture yourself. “File → Process recording → Start
recording” writes every step (stroke, fill, layer change, undo) into a file next to the picture:
`picture.sai2` → `picture.sai2.gsrec`. Until the picture is saved, the recording lives in the settings
folder (`recordings`) and moves next to the picture on the first save. The status bar shows a red
“● REC” and the recording size. Opening a picture that has a recording continues it automatically (unless it was stopped from the menu).

“Export video…” makes a sped-up replay of the whole process as an animated PNG (opens in a web browser):
duration, frames per second, frame size, how long to show the finished picture at the end. In “Hide
layers” tick the layers that must not appear in the video (reference pictures, guides) — this works for
layers deleted along the way too (but if an excluded layer was merged into a visible one, its content shows up in the video with it). “Watermark” puts the mark from “File → Watermark…” on every frame. With a
password the video goes into a zip with AES-256 encryption: open it with 7-Zip, WinRAR or Keka (Windows
Explorer cannot open such a zip).

The `.gsrec` file itself keeps all layers, hidden ones included — give clients only the export. File
layout — `docs/40-recording.en.md`.

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
