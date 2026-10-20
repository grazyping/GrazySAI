# GrazySAI roadmap

**English** | [Русский](ROADMAP.md)

The version number shows readiness: **1.0.0** means GrazySAI has everything people use in
PaintTool SAI 2 for everyday work, and `.sai2` files open and save without loss. The second
number is the last stage in which everything is done; fixes and features within a stage are the
third number.

On 16 October 2026 **1.0.0** was released: everything on the 1.0 list is done. Next come
feedback, Clip Studio Paint-like filters and ideas from Krita.

On GitHub every stage is a milestone with a progress bar
([Milestones](https://github.com/grazyping/GrazySAI/milestones)) and every item is an issue
([Issues](https://github.com/grazyping/GrazySAI/issues), written in Russian); finished items are closed.
The Russian [ROADMAP.md](ROADMAP.md) is the master copy.

## Done

### 0.1 — core and `.sai2` reading
- [x] Pixels, tiles, layers, folders, clipping groups
- [x] Reading `.sai2`: header, table of contents, layer records, compressed layer pixels, flattened image
- [x] Checked against the file's own flattened image (exact on the probe files; a drawing with 191 layers)

### 0.2 — first window and brush
- [x] Editor window: canvas, zoom, hand, color picker, undo and redo, PNG export
- [x] Brush following SAI's model: dab, edge, opacity, spacing
- [x] All 28 blend modes
- [x] Pen pressure: XInput 2 (Linux), Windows Ink (Windows)

### 0.3 — interface and vector layers
- [x] Window laid out like SAI 2: color wheel, brush and size grids, quick bar, navigator, layer list with thumbnails
- [x] Thin brushes by SAI's rules (edge tables) — steps 1–4 match up to rounding
- [x] Linework and shape layers from `.sai2`: format, curves, rendering
- [x] Layer operations with undo: copy, delete, erase, transfer down, merge, order
- [x] Five resampling methods as in SAI (core; compared with live SAI)
- [x] Virtual pen and virtual tablet for testing without a tablet

### 0.4 — saving
- [x] Writing `.sai2`: layers, folders, properties, pixels, flattened image
- [x] Save and Save As; the user's files re-save without loss
- [x] A licensed SAI opens our files

### 0.5 — painting as in SAI
- [x] Checked on a real XPPen Artist 24 Pro tablet
- [x] Stabilizer as in SAI: levels 1–15 and S-1…S-7
- [x] Pressure on density — compared with live SAI (dome-shaped profile, middle within 2 levels on average)
- [x] “Ver1 Prs Spec.” — version-1 pressure on density, checked on Marker and Water
- [x] Amplify Density by SAI's rules, checked on Marker; minimum density uses the same formula as minimum size
- [x] Mixing with the canvas for Brush, Water, Marker by SAI's rules; Brush compared with live SAI
- [x] Water: blur by SAI's rules, compared with live SAI
- [x] AirBrush: SAI uses the Pencil engine for it; defaults as in SAI, compared with live SAI

### 0.6 — selection and transform
- [x] Selection: rectangle, lasso, magic wand, Shift/Alt modes, invert, deselect, expand/shrink
- [x] Selection pen and eraser, selection from layer opacity
- [x] Moving a layer or the selection: tool, Ctrl + drag, Alt+arrows
- [x] Transform (Ctrl+T): size and rotation with a handle frame, five resampling methods
- [x] Free deform by corners (Ctrl + corner)
- [x] View rotation and flip, canvas flip and rotation, layer flip, canvas size and resolution

### 0.7 — layers in full
- [x] Layer masks: reading, display, writing `.sai2`; create, paint in the mask, apply, delete
- [x] Layer paper (`papertex`) by SAI's rules, checked against the file's flattened image: 92 % of pixels exact, the rest within 1–3 levels
- [x] Fringe and Binary Color layer effects by SAI's rules, compared with live SAI
- [x] Locks: painting, moving, everything
- [x] Selection Source for the magic wand and the bucket
- [x] Bucket fill (Alt+Delete), Flatten Image
- [x] Luminance to Opacity

### 0.8 — linework and shapes
- [x] New linework layer; pen with vectorization by SAI's rules (“Normal” handles match SAI exactly)
- [x] Linework tools: eraser, weight, color, edit, pressure (SAI's behaviour not compared)
- [x] Shape tools: rectangle, ellipse, triangle — as SAI's templates
- [x] Curves of linework and shape layers from `.sai2` are saved as they were (byte for byte); brushes do not paint on such layers, “Rasterize Layer” makes them normal

### 0.9 — brushes in full, text and rulers
- [x] Brush forms and textures (`blotmap`, `brushtex`) — from SAI's code, checked against live SAI
- [x] Bristle brush form (`bristle`) after SAI's code; band width and stroke ends checked against the live SAI
- [x] Marker at light pressure — as in SAI: its own compositing from SAI's code, paint amount from live SAI
- [x] Water with Amplify Density — closer to SAI, fitted to measurements (0.8.47)
- [x] The hardest edge step by SAI's code: its own renderer and a stroke mask from 20 px — lines match live SAI to 0.0–0.3 levels
- [x] Text layers and perspective rulers (reading and writing): text is drawn with the system font, both are saved byte for byte
- [x] Text tool: creating and editing text (font, size, color, several lines); bold, italic and vertical text — from the user's sample, look checked against SAI under Wine; font list as in SAI (GDI)
- [x] Rulers: straight line and ellipse, as in SAI 2017
- [x] The mask moves and transforms with its layer; Merge Down for a layer with a mask
- [x] Color mixer and swatches in the color panel — as in SAI 2017
- [x] Large Water brush: blur over a reduced copy of the layer, as in SAI — 350 px: 7.4 → 2.0 s per 1000 px of line
- [x] Perspective rulers: display and drawing along them (reference: Krita's perspective and vanishing point assistants); our own version, not written to the file (0.8.38)

### 1.0 — everything from SAI 2 for everyday work (current version 1.0.19)

Everything people use in PaintTool SAI 2017 for everyday work is there and checked against the original (feature-by-feature comparison — `docs/32-sai-comparison.md`); `.sai2` files open and save without loss. A code audit came before the release (`docs/audit-2026-10-16.md`).

#### Core
- [x] `.sai2` read and written without loss for every kind of layer: checked by resaving the user's files — same flattened picture (difference 0), text and linework byte for byte, masks, effects, paper, background and resolution kept
- [x] The `thum` thumbnail in `.sai2` (SAI's own JPEG-like `jssf` format) — for SAI's file browser; size and layout as in SAI 2017 (0.8.39)
- [x] Undo history on disk (for very large canvases): beyond 1 GB, steps go to temporary files and all 100 steps are kept
- [x] Gradient: linear and radial, main colour → background or transparent, S-curve
- [x] Polygonal lasso and anti-aliasing switch
- [x] Dilate and erode the selection by 1–25 px with a preview, corners as in SAI
- [x] Transform: SAI modes and “Perspective” 0–100 — a blend of bilinear and perspective deform, checked against the live SAI
- [x] Several canvases in tabs at the bottom, as in SAI; “File → Close” (Ctrl+W)
- [x] Floating view of the canvas: its own zoom, position, rotation and mirror; painting works in it
- [x] “New View” — a second view of the same canvas in a tab (0.8.37)
- [x] Layer outline (inside, centre, outside, canvas edges)
- [x] Layer menu as in SAI: “Merge Visible Layers”, “Rotate Layer 90°”, “Layer Property…” — after the live SAI
- [x] Binary pen: points by SAI's code — stairs at slopes 0.2 and 0.4 and line ends match SAI (0.9.12)
- [x] PSD: opening 16-bit files and layer masks, ZIP compression, grayscale
- [x] Custom tools in the grid: right-click an empty cell to add a tool of the needed kind; duplicate, delete, property (name, second line), as in SAI
- [x] Edit: cut, copy, paste (as a new layer), copy by selection bounds; new canvas from the clipboard — system clipboard
- [x] File: recent files (up to 10, kept between runs)
- [x] Canvas: trim by selection, canvas background (white, checker, etc.) — from SAI's code, read and written in `.sai2`
- [x] View: reset position and angle, 100% (Ctrl+Alt+0), rotate and mirror in the menu
- [x] Window: full screen (F11, Shift+Tab), hide panels (Tab), panels on the right or left, UI scale, dot cursor and brush size circle
- [x] Window: V-HSV, HSL and HSV slider modes from SAI's code; S and V/L 0–100, as in SAI
- [x] Window: swatch size (Small, Middle, Large — 10, 12, 14 px, measured on the live SAI)

#### Fixes from the user's list

- [x] “New Linework Layer” and “Perspective Ruler” buttons on the layer panel (0.8.40)
- [x] Brush size grid up to 5000, as in SAI (0.8.40)
- [x] New name — GrazySAI; old versions update by themselves (0.8.42)
- [x] More formats: OpenRaster with layers; export and open BMP, TGA, TIFF, WebP, QOI (0.8.43)
- [x] Icons for new menu items (0.8.44)
- [x] Text is typed right on the canvas, settings on the tool panel, as in SAI (0.8.45)
- [x] Tools get their useful buttons on the panel; Straight line, rotate view as a tool and in the navigator (0.8.46)

#### From feedback

- [x] Watercolor with a pen no longer crashes the program; crash causes go to `crash.log` (0.9.6)
- [x] The New Canvas window remembers the last choice across restarts (0.9.7)
- [x] Scratch pad and the canvas at 100% are sharp with Windows display scaling; the scratch pad takes every pen move and pressure (0.9.11)
- [x] Laptops with two GPUs (Windows): the program asks for the discrete NVIDIA or AMD GPU by itself (0.9.15)
- [x] Smoother and faster: filter previews in the background; brushes up to 5000 px are 5–14 times faster, the window does not freeze (0.9.7)
- [x] Filter, Dilate/Erode Selection and Canvas Size windows remember the last settings, even after Cancel; “Reset” (0.9.7)

#### More from SAI 2017 (found by comparison, `docs/32-sai-comparison.md`)

- [x] Layer effect color (Canvas → Layer Effect Color), checked against SAI (0.9.5)
- [x] Layers: show alone, bring to front and send to back, select the layer above/below by key (0.8.48)
- [x] Layers: cut and nudge, marking folders for visibility, SAI layer keys (0.9.8)
- [x] Selection from layer opacity in the menu (0.8.48)
- [x] Linework points and strokes in the selection; points shown on the canvas, selected ones drag together (0.9.9)
- [x] View: close all views, previous and next view, scrolling and zoom at the cursor by keys (0.8.48)
- [x] Window: show and hide each panel separately; scratch pad (0.9.1)
- [x] Floating panels: tools, color, layers, navigator — each in its own window (0.9.10)
- [x] Options window, as in SAI, plus history memory and recovery copy interval (0.9.2)
- [x] Transparent color; keys for the pickers and brush density 10…100 (0.8.48)
- [x] “Save changes?” question on exit (0.8.48)
- [x] Own file viewer with thumbnails, as in SAI (0.9.4)

### Beyond the plan
- [x] Dark interface theme: “Window → Dark theme” (the canvas and drawing colors stay as they are)
- [x] Filters: hue and saturation, brightness and contrast, Gaussian blur — by SAI's rules, compared with live SAI; the other filters do not work in SAI itself
- [x] Brush, color and stabilizer settings kept between runs
- [x] Custom shortcuts: “Help → Shortcut Keys…”, 34 actions, SAI's defaults
- [x] PSD (with layers) and JPEG export; opening PSD, PNG and JPEG
- [x] Work recovery after a crash (a copy every 3 minutes)
- [x] Undo history: 100 steps, as in SAI; beyond 1 GB — on disk
- [x] Fast on large canvases: 2300×4096 with 191 layers — reading 0.3 s, display 0.3 s
- [x] User manual (`docs/manual.en.md`, updated with every version)
- [x] RGB and HSV color sliders, dragging layers in the list
- [x] Large brushes 4–40 times faster with identical results
- [x] Layer dragging with clear feedback: the layer fades in place, a label at the cursor, an insertion bar, folder highlight, scrolling at the edges, Esc
- [x] Interface in 10 languages (English, Russian, Ukrainian, German, French, Spanish, Portuguese, Japanese, Chinese, Korean), system language by default, switchable without a restart; labels and columns adapt to the length of each translation, a light CJK font
- [x] Updating from the program: GitHub releases are checked at startup, a “Version is out” notice in the menu bar, download with SHA-256 check and file replacement, restart; a GitHub link in the menu bar
- [x] System “Open”, “Save As” and “Export” windows (File Explorer on Windows, the desktop dialog on Linux), Ctrl+O
- [x] Shortcut keys in menus and tooltips can be hidden (“Window → Show shortcut keys”)

## Ahead

### First after 1.0 — filters as in Clip Studio Paint

A Filter menu with CSP's submenus; windows with a preview, like our current filters. They only change layer pixels, so `.sai2` compatibility is kept. CSP is closed source: built from its manual's descriptions, checked against Krita and GIMP where they have a match.

- [x] Blur filters: average, blur and strong blur, motion, radial, spin, smoothing, lens blur (bokeh) (0.8.34)
- [x] Sharpen filters: sharpen, sharpen more, unsharp mask (0.8.33)
- [x] Effect filters: chromatic aberration, noise, mosaic, crystallize, retro film (0.8.33)
- [x] Distort filters: pinch, ripple, fish-eye lens, polar coordinates, wave, twirl (0.8.35)
- [x] Render filter: Perlin noise (0.8.33)
- [x] Correction filters: remove dust, adjust line width (0.8.33)
- [x] Stylizing filters “Pencil drawing” and “Artistic” (our own take — CSP's method is closed) (0.8.36)

### Remaining SAI 2017 features (check of 20 Oct 2026)

- [x] PSB: export and opening of large Photoshop files (side over 30,000 px) (1.0.5)

### Beyond SAI — ideas from Krita (after 1.0)

Everything SAI has comes first; these come later. They do not hurt compatibility: in `.sai2` everything lands as plain pixels, the rest goes to the program settings. The window keeps SAI's look: new things live in menus and windows, not on the panels. No Krita code (GPL) is copied — only ideas and formulas, checked against SAI or Krita.

- [ ] Third-party SAI 2 brushes: the `settings2.ssd` tool set (encrypted differently from SAI 1 — decode from SAI 2's code). Blot, brush, bristle and paper textures (`blotmap`, `brushtex`, `bristle`, `papertex`) are already picked up from the SAI folder
- [x] Clip Studio Paint brushes (`.sut`, `.bfps`): main settings and texture (1.0.14)
- [x] Image tip for any brush (PNG etc., GIMP brushes `.gbr`); for Clip Studio Paint brushes — from the material thumbnail (1.0.20)
- [ ] Krita brushes: `.kpp` presets and `.bundle` packs (pixel and colour-smudge engines — 97 of the 117 defaults), `.gbr`, `.gih`, `.png`, `.svg` tips and Photoshop `.abr` brushes — a new brush with an image tip
- [x] Mirror painting (symmetry) (1.0.9)
- [x] Fill: closing gaps in line art and growing the edge under the line (1.0.10)
- [x] Recent colours; GIMP (`.gpl`) and Krita (`.kpl`) palettes (1.0.6)
- [x] History window: list of steps, jump back with a click (1.0.7)
- [x] Pen pressure curve in the settings (1.0.6)
- [x] PSD: layer masks on export (1.0.7)
- [x] PSD and PSB: 16 bits on save (1.0.17)
- [x] PSD: “Keep the exact look” for SAI-only modes (1.0.18)
- [x] OpenRaster (`.ora`): open and save with layers (0.8.43)
- [x] “Save incremental” (picture_002.sai2) (1.0.6)
- [x] History memory and recovery copy interval in a settings window (0.9.2)
- [x] Pixel grid at high zoom (1.0.9)
- [x] Wrap-around mode (the canvas repeats at its edges) (1.0.15)
- [x] Filters: levels, curves, median (1.0.8)
- [x] Colouring by line art (like Krita's colorize mask); the result is plain pixels (1.0.19)
- [x] Reference images over the canvas (1.0.12)
- [x] Pop-up palette on right click (off by default) (1.0.11)
