# SOsai roadmap

**English** | [Русский](ROADMAP.md)

The version number shows readiness: **1.0.0** means SOsai has everything people use in
PaintTool SAI 2 for everyday work, and `.sai2` files open and save without loss. The second
number is the last stage in which everything is done; fixes and features within a stage are the
third number.

Revised on 15 October 2026: stages 0.5–0.8 turned out to be done except for a few brush items.
Those items moved to 0.9, so the version is now **0.8**.

On GitHub every stage is a milestone with a progress bar
([Milestones](https://github.com/grazyping/SOsai/milestones)) and every item is an issue
([Issues](https://github.com/grazyping/SOsai/issues), written in Russian); finished items are closed.
The Russian [ROADMAP.md](ROADMAP.md) is the master copy.

## Done

### 0.1 — core and `.sai2` reading
- [x] Pixels, tiles, layers, folders, clipping groups
- [x] Reading `.sai2`: header, table of contents, layer records, compressed layer pixels, flattened image
- [x] Checked against the file's own flattened image (exact on the probe files; a drawing with 191 layers)

### 0.2 — first window and brush
- [x] Editor window: canvas, zoom, hand, color picker, undo and redo, PNG export
- [x] Brush following SAI's model: dab, edge, opacity, spacing — compared with live SAI
- [x] All 28 blend modes — compared with live SAI within 2 levels
- [x] Pen pressure: XInput 2 (Linux), Windows Ink (Windows)

### 0.3 — interface and vector layers
- [x] Window laid out like SAI 2: color wheel, brush and size grids, quick bar, navigator, layer list with thumbnails
- [x] Thin brushes by SAI's rules (edge tables) — steps 1–4 match up to rounding
- [x] Linework and shape layers from `.sai2`: format, curves, rendering (checked on the user's files)
- [x] Layer operations with undo: copy, delete, erase, transfer down, merge, order
- [x] Five resampling methods as in SAI (core; compared with live SAI)
- [x] Virtual pen and virtual tablet for testing without a tablet

### 0.4 — saving
- [x] Writing `.sai2`: layers, folders, properties, pixels, flattened image
- [x] Save and Save As; the user's files re-save without loss
- [x] A licensed SAI opens our files (checked by the user)

### 0.5 — painting as in SAI
- [x] Checked on a real XPPen Artist 24 Pro tablet (checked by the user)
- [x] Stabilizer as in SAI: levels 1–15 and S-1…S-7 (`docs/15-stabilizer.md`)
- [x] Pressure on density — compared with live SAI (dome-shaped profile, middle within 2 levels on average)
- [x] “Ver1 Prs Spec.” — version-1 pressure on density, checked on Marker and Water
- [x] Amplify Density by SAI's rules, checked on Marker; minimum density uses the same formula as minimum size
- [x] Mixing with the canvas for Brush, Water, Marker by SAI's rules; Brush compared with live SAI (`docs/16-mixing.md`)
- [x] Water: blur by SAI's rules, compared with live SAI
- [x] AirBrush: SAI uses the Pencil engine for it; defaults as in SAI, compared with live SAI

### 0.6 — selection and transform
- [x] Selection: rectangle, lasso, magic wand, Shift/Alt modes, invert, deselect, expand/shrink (`docs/17-selection-move.md`)
- [x] Selection pen and eraser, selection from layer opacity
- [x] Moving a layer or the selection: tool, Ctrl + drag, Alt+arrows
- [x] Transform (Ctrl+T): size and rotation with a handle frame, five resampling methods
- [x] Free deform by corners (Ctrl + corner)
- [x] View rotation and flip, canvas flip and rotation, layer flip, canvas size and resolution (`docs/18-canvas-view.md`)

### 0.7 — layers in full
- [x] Layer masks: reading, display, writing `.sai2` — checked against a SAI file; create, paint in the mask, apply, delete (`docs/26-layer-masks.md`)
- [x] Layer paper (`papertex`) by SAI's rules, checked against the file's flattened image: 92 % of pixels exact, the rest within 1–3 levels (`docs/24-paper.md`)
- [x] Fringe and Binary Color layer effects by SAI's rules, compared with live SAI (`docs/25-layer-effects.md`)
- [x] Locks: painting, moving, everything (`docs/19-fill-locks.md`)
- [x] Selection Source for the magic wand and the bucket
- [x] Bucket fill (Alt+Delete), Flatten Image
- [x] Luminance to Opacity

### 0.8 — linework and shapes (current version 0.8.6)
- [x] New linework layer; pen with vectorization by SAI's rules (“Normal” handles match SAI exactly)
- [x] Linework tools: eraser, weight, color, edit, pressure (SAI's behaviour not compared)
- [x] Shape tools: rectangle, ellipse, triangle — as SAI's templates
- [x] Curves of linework and shape layers from `.sai2` are saved as they were (byte for byte); brushes do not paint on such layers, “Rasterize Layer” makes them normal

### Beyond the plan
- [x] Filters: hue and saturation, brightness and contrast, Gaussian blur — by SAI's rules, compared with live SAI (`docs/20-filters.md`); the other filters do not work in SAI itself
- [x] Brush, color and stabilizer settings kept between runs (`docs/22-settings-recovery.md`)
- [x] Custom shortcuts: “Help → Shortcut Keys…”, 33 actions, SAI's defaults
- [x] PSD (with layers) and JPEG export; opening PSD, PNG and JPEG (`docs/21-file-formats.md`)
- [x] Work recovery after a crash (a copy every 3 minutes)
- [x] Undo history: 100 steps, as in SAI, and no more than 2 GB of memory
- [x] Fast on large canvases: 2300×4096 with 191 layers — reading 0.3 s, display 0.3 s (`docs/23-performance.md`)
- [x] User manual (`docs/manual.en.md`, updated with every version)
- [x] RGB and HSV color sliders, dragging layers in the list
- [x] Large brushes 4–40 times faster with identical results (`docs/23-performance.md`)
- [x] Layer dragging with clear feedback: the layer fades in place, a label at the cursor, an insertion bar, folder highlight, scrolling at the edges, Esc
- [x] Interface in 10 languages (English, Russian, Ukrainian, German, French, Spanish, Portuguese, Japanese, Chinese, Korean), system language by default, switchable without a restart; labels and columns adapt to the length of each translation, a light CJK font
- [x] Updating from the program: GitHub releases are checked at startup, a “Version is out” notice in the menu bar, download with SHA-256 check and file replacement, restart; a GitHub link in the menu bar (`docs/28-updates.md`)

## Ahead

### 0.9 — brushes in full, text and rulers
- [ ] Brush shapes and textures (`blotmap`, `brushtex`)
- [x] Marker at light pressure — as in SAI: its own compositing from SAI's code, paint amount from live SAI (`docs/16-mixing.md`)
- [ ] Water with Amplify Density
- [ ] The hardest edge step from 20 px by SAI's rules (currently fitted)
- [ ] Text layers and perspective rulers (reading and writing)
- [x] Rulers: straight line and ellipse, as in SAI 2017 — compared with live SAI (`docs/27-rulers.md`)
- [x] The mask moves and transforms with its layer; Merge Down for a layer with a mask
- [ ] Color mixer and swatches in the color panel
- [ ] Large Water brush: blur over a reduced copy of the layer, as in SAI (now 350 px takes 7 s per 1000 px of line)

### 1.0 — finished
- [ ] `.sai2` read and written without loss for every kind of layer
- [ ] Undo history on disk (for very large canvases)
