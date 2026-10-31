# GrazySAI roadmap

**English** | [Русский](ROADMAP.md)

The version number shows readiness: **1.0.0** means GrazySAI has everything people use in
PaintTool SAI 2 for everyday work, and `.sai2` files open and save without loss. The second
number is the last stage in which everything is done; fixes and features within a stage are the
third number.

On 16 October 2026 **1.0.0** was released: everything on the 1.0 list is done. Next come
feedback, Clip Studio Paint-like filters and ideas from Krita.

On 23 October 2026 **1.1.0** was released: new features from SAI 2 after 2017. From this version on, releases come by sections: a section is done in full, tested and released as one version with the next second number: 1.2.0 — interface as in SAI 2 (checked on Windows), 1.3.0 (24 October 2026) — brush library, SAI 2 and Photoshop brushes, 3D poses and lines, menu icons, tablets (WinTab, tilt), FreeBSD and Windows 7 builds, 1.4.0 (24 October 2026) — from Clip Studio Paint and After Effects: auto actions, layer styles and correction layers, FBX/VRM/PMX/COLLADA import, command search and quick access, timeline and animation, puppet.

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

### 1.0 — everything from SAI 2 for everyday work

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

### 1.1 — new in SAI 2 after 2017

Plan, analysis and measurements — `docs/41-sai2-after-2017.en.md`. New tools are separate kinds;
brushes calibrated against SAI 2017 stay unchanged; files still open in SAI 2017.

- [x] Control step: SAI 2026 base strokes against the 2017 sheets (engine differs: −20…35 % paint, −10…15 % diameter)
- [x] Lasso Fill: freeform and polygonal, enclosed regions with gap tolerance, dilation, “ignore painted”, source (1.0.40)
- [x] Blur tool: blur only, “blur width”; checked against live SAI 2026 within 16 levels (1.0.41)
- [x] Smudge: drags paint, “Coloring”; our own transport model fitted to live SAI 2026 within ~30 levels at the blob edge (1.0.42)
- [x] Radial symmetry: up to 20 parts, mirrored or rotated, angle and centre (1.1.0)
- [x] Scatter brush: shapes with random offset, size, angle, width, count and colour; distributions checked against SAI 2026 (1.1.0)
- [x] Effect Pen: the stroke goes on with a layer blend mode (1.1.0)
- [x] Grid deformation Lattice and Mesh (instead of Liquify, which SAI 2026 lacks) (1.1.0)
- [x] Pixel Art Pen, binary eraser, binary selection pen and eraser; dots as in SAI (1.1.0)
- [x] Brush details: stroke preview, minimum pressure for a stroke, Anti-Hook, Shift after starting a stroke (1.1.0)
- [x] Canvas size presets; reserve and restore tool settings (1.1.0)
- [x] Rulers: parallel lines, concentric ellipses, vanishing point (1.1.0)
- [x] Asset Manager: swatch sets, scratch pads, papers, forms, textures and tips, groups, asset from a layer (1.1.0)
- [x] 2026 details: lasso stabilizer, grayscale, canvas list in View, clearing Recent (1.1.0)

### 1.2 — interface as in SAI 2

Checked on real Windows next to the official SAI 2 2026.

- [x] Navigator as in SAI: the whole canvas as a small copy, the view frame with an “up” line (turns with the view), dragging moves the view, a click does not jump (1.2.0)
- [x] Navigator and layers at the window edge, color and tools next to the canvas (SAI's default) (1.2.0)
- [x] Tool groups — “Basic”, “Binary” and your own tabs (1.2.0)
- [x] Brush panel: size range ×0.1…×50, pressure icons in the rows, form and texture always shown, reserve / restore in Misc, stroke preview on a checkerboard (1.2.0)
- [x] Size list as numbers only and above the settings; open canvases at the bottom of the View menu (1.2.0)
- [x] “Drive Usage” in the status bar (1.2.0)
- [x] Icons in the new-tool list and on new menu items; labels fit in all 10 languages (1.2.0)

### 1.3 — brush library, other programs' brushes, 3D, menu icons, tablets, systems (1.3.0)

#### Brush library

Like Krita's preset docker, Clip Studio's material palette and Photoshop's brushes panel.

- [x] A library of brushes from other programs in the program's own folder: SAI 2, Clip Studio, Krita, Photoshop, GIMP, GrazySAI; the originals are copied (1.3.0)
- [x] Adding in bulk: many files or a whole folder by drag and drop, “Open”, “Add…”; duplicates are skipped (1.3.0)
- [x] Library window: “All”, “Favorites”, “New”, by program, set and tag; search; cards with a stroke sample or a list (1.3.0)
- [x] A separate system window (another monitor, always on top) or inside the program (1.3.0)
- [x] Double-click puts a brush into the grid, in the “Library” tab (1.3.0)

#### Other programs' brushes

- [x] SAI 2 custom tools (`custool` folder: `.saitdat`, `.saitgrp` groups) go into the library; forms, bristles and textures by name from the SAI 2 folder, including `brushfom`. Linework and shape layer tools are not converted yet (1.3.0)
- [x] `.svg` tips (Krita and any SVG picture) (1.3.0)
- [x] Photoshop brush settings from the `desc` section: size, spacing, hardness, angle and roundness, pressure, scatter and jitter (Scatter brush), colour dynamics (1.3.0)

#### 3D layer

- [x] 3D: poses — save, apply (by joint names, works with another model), mirror (left ↔ right) (1.3.0)
- [x] 3D: “drag the hand or foot” (inverse kinematics) (1.3.0)
- [x] 3D: camera from the perspective ruler (1.3.0)
- [x] 3D: turning the model into lines — outline and creases as a new linework layer (1.3.0)

#### Graphics card (Vulkan, DirectX 12 through wgpu)

- [x] Window drawing through wgpu, a switch in the settings, OpenGL by default, fallback when a start fails; checked on Linux (software Vulkan) and Windows (Vulkan, DirectX 12) (1.4.3)
- [x] Gaussian blur and unsharp mask preview on the graphics card; the final result is computed by the CPU to stay exact (4K on integrated AMD: 0.3 s instead of 0.55–1.2 s) (1.4.3)
- [ ] Other filters on the graphics card: lens, median, rotation (0.6–1 s at 4K) — less gain: long kernels or sorting, and moving the pixels costs 0.25 s
- [x] Layer compositing for display on the graphics card: all 28 modes, plain layers and pass-through folders; mask, clipping, effect, paper and style stay on the CPU. It switches on by itself only where it is faster here: on a Ryzen 5700G with its integrated graphics the CPU wins (4K, 20 layers: 84 ms vs 120 ms); not tested on a discrete card
- [x] 3D layer on the graphics card: preview while a scene is being moved, for scenes from 100,000 triangles, without outline; the final result is computed by the CPU (1M triangles, 4K: 22–24 ms vs 120–160 ms)

### Leftovers of new SAI 2 features

- [x] Layer ARGB colour picker for the Pixel Art Pen: a transparent pixel makes the pen erase (1.3.0)
- [x] SAI 2 scatter shapes (`.ini` and `.bmp` from `settings/scatter`) go into the library as Scatter brushes (1.3.0)
- [x] Ready-made “Ver.1” and “Artistic” tool groups, as in SAI 2 (right-click a group tab → “Group as in SAI 2”) (1.3.0)

#### Menu icons

- [x] An icon on every item of drop-down and context menus (open, save, new folder, new 3D layer, etc.) — the Phosphor set, as in familiar programs (1.3.0)
- [x] Menus checked in all 10 languages: labels line up, nothing is cut off (1.3.0)

#### Graphics tablets

- [x] Popular brands and lines, their drivers and input methods (Wacom, XP-Pen, Huion, Gaomon, Veikk, UGEE, Xencelabs, Parblo, Artisul…) — `docs/42-tablets.en.md` (1.3.0)
- [x] WinTab support on Windows — optional in the settings (off by default for now: not checked with a real tablet) (1.3.0)
- [x] Pen tilt and rotation (WinTab, XInput 2) as brush dynamics inputs; eraser end; a table of checked models (1.3.0)

#### Systems

- [x] FreeBSD: build (`tools/build-freebsd.sh`), pen through XInput 2, checked in a FreeBSD 15.1 virtual machine (window, menus, a stroke), FreeBSD archive (1.3.0)
- [x] Linux: checked under Openbox (LXDE, LXQt), Fluxbox, IceWM, i3, bspwm, Metacity (GNOME Flashback) and KDE Plasma (`tools/wm-check.sh`); the separate library window comes on top (1.3.0)
- [x] Windows 10 and 11 — the main build (checked on Windows 11); Windows 7, 8, 8.1 — a separate build (`tools/build-win7.sh`), checked under Wine in Windows 7 mode (1.3.0)
- [x] Windows XP and Vista — worked out in theory (`docs/43-systems.en.md`): a separate light program would be needed (1.3.0)

### Filters as in Clip Studio Paint

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

### Beyond SAI — ideas from Krita, other programs' brushes

Everything SAI has comes first; these come later. They do not hurt compatibility: in `.sai2` everything lands as plain pixels, the rest goes to the program settings. The window keeps SAI's look: new things live in menus and windows, not on the panels. No Krita code (GPL) is copied — only ideas and formulas, checked against SAI or Krita.

- [x] Clip Studio Paint brushes (`.sut`, `.bfps`): main settings and texture (1.0.14)
- [x] Image tip for any brush (PNG etc., GIMP brushes `.gbr`); for Clip Studio Paint brushes — from the material thumbnail (1.0.20)
- [x] Krita brushes: `.kpp` presets and `.bundle` packs, `.png`, `.gbr`, `.gih` tips (1.0.21)
- [x] Photoshop `.abr` brushes — brush pictures as tips (1.0.26)
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

### Own brushes and plugins (GrazySAI Studio workshop)

Anyone may write them, so plugin code runs in a sandbox. Details and measurements:
`docs/34-brushes-and-plugins.md`.

- [x] Own brush file `.gsbrush` and set `.gsbrushes`: save and open a grid tool (1.0.29)
- [x] Brush dynamics: inputs (pressure, speed, direction, distance, random) → curve → parameters (1.0.30; tilt once the pen input passes it)
- [x] GrazySAI Studio workshop: brush editor with a test strip, tip generator, “Export to GrazySAI” with live reload (1.0.31)
- [x] Luau plugins (sandboxed): menu commands, parameter windows, live reload from `plugins/dev/` (1.0.32)
- [x] Workshop: editing and testing a plugin, “Export to GrazySAI” (1.0.33)
- [x] Plugins: custom brush dynamics (a function per dab) (1.0.34)
- [x] WebAssembly plugins (wasmi): tiled filters on several threads, with margins for neighbours (1.0.35)
- [x] Guides for authors (ru, en), templates and examples: `docs/36-plugins.en.md`, `docs/37-brushes-guide.en.md`, `plugins/examples/` (1.0.36)
- [x] We do not run a shared catalog; the index format is described for anyone who wants to host one — `docs/38-catalog.md`

### Process recording, watermark, 3D

- [x] Watermark on export (text or picture, position, opacity) (1.0.37)
- [x] Drawing process recording: sped-up video, without chosen layers, with a password (AES zip) (1.0.38)
- [x] 3D layer: glTF/GLB and OBJ models (your own — none bundled), camera, display (color, clay figure, silhouette, outline), posing by joints with the mouse and sliders, morphs, hiding and moving parts, several models in a scene; stored in `.sai2`, SAI sees an ordinary layer (1.0.39; plan — `docs/39-3d-reference.md`)

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

### 1.4 — from Clip Studio Paint and After Effects (current version 1.4.3; research — `docs/44-plan-1.4.en.md`)

- [x] Auto Actions: record steps and replay them with one button, action sets (1.4.0)
- [x] Layer styles (stroke, shadow, glow, colour overlay) and correction layers: gradient map, levels, hue/saturation, brightness/contrast, invert, posterize, threshold (1.4.0)
- [x] Import from other programs: FBX 3D models (Cinema 4D, Maya, 3ds Max, Mixamo, Daz), VRM (VRoid), PMX (MikuMikuDance), COLLADA `.dae` (SketchUp); USD and Alembic via export to glTF/FBX (1.4.0)
- [x] User customisation, as in Clip Studio and other programs: command search (Ctrl+Shift+P), a “Quick access” panel with sets of any commands, workspaces, own shortcuts and pen buttons (1.4.0)
- [x] Timeline and animation as in Clip Studio Paint: cels (animation folders), timeline, onion skin, playback, export to GIF/APNG/video/frames (1.4.0)
- [x] Puppet animation as in After Effects: a triangle mesh over the layer outline, pins (move, rotate, starch — stiffness, overlap — what is in front), the picture follows the mesh; pins keyed on the timeline (1.4.0)

From what was left after 1.3.0:

- [x] Tablet drivers (Linux: XP-Pen, Huion, UGEE, Veikk, Xencelabs — the pen axes are the ones the program reads; Windows: XP-Pen on the test PC, WinTab loads) (1.4.0)
- [x] Pen buttons: hand, picker, palette at cursor, undo (the driver sends express keys as keyboard keys — shortcuts work) (1.4.0)
- [x] Anti-Ripple for Flatten Brush and the wand/bucket “Reduce small omissions” — checked against live SAI 2026: omissions done, our flat brush edge already behaves as Anti-Ripple (1.4.0)
- [x] Colour (ARGB) assets: coloured tips and scatter shapes, colour assets from a layer (1.4.0)
- [x] FreeBSD: a `pkg` package with every release (`tools/freebsd/mkpkg.sh`; added to 1.3.0) (1.4.0)

From the tester's feedback:

- [x] Quick fixes: timeline menu, theme after restart, shadows, tool panel scrolling, recent colors vs. recent files, window position, layer moved out of an animation folder (1.4.1)
- [x] 4K speed: compositing about 3× faster, no "waves" on refresh, playback shows whole frames (1.4.1)
- [x] Puppet as in Clip Studio: the whole mesh (one pin moves it, a second one rotates and stretches it), puppet keys on the timeline, copying and dragging frames (1.4.1)
- [x] Dockable panels as in Clip Studio: panels, canvas and timeline are tabs, tear off into floating windows, drop spot highlighted, layout kept in settings and workspaces (1.4.1)
- [x] GrazySAI Convert, a format converter: pictures (SAI 2, PSD, PSB, OpenRaster, images) and 3D models (glTF, VRM, OBJ, FBX, PMX, COLLADA → GLB, OBJ) in batches (1.4.1)
- [x] Faster filters, rotation and transform, magic wand, smudge (2–10×); canvases up to 30000×30000 in the window (big ones shown as reduced copies), up to 100000×100000 in `.sai2` and the converter; the program no longer loads the CPU while idle (1.4.2)

## Ahead

Not in 1.4.0.

### From the tester's feedback

- [ ] A less bulky interface — waiting for the tester's details
- [ ] Clip Studio brushes that import poorly — need `.sut` samples

### Leftovers of new SAI 2 features

- [x] SAI 2 custom tools for linework and shape layers: line and curve by clicks; on shape layers curve, color, edit; pen, line and curve import as brushes
- [ ] Binary linework tools — if compatible storage is found

### Left for the next version

- [x] Tablet drivers: dropped — Linux and Windows of Wacom, Gaomon, XP-Pen analysed, the others have no direct links; a real tablet on Windows waits for feedback
- [ ] Pen directly on Wayland, without XWayland — waits for the window library: winit 0.31 (still beta) has the Wayland pen, but egui/eframe, even the newest, is on winit 0.30
- [x] FreeBSD: a port in the ports tree — dropped: anyone who wants it can build from source; a ready archive and `.pkg` exist, `tools/freebsd/mkport.sh` makes the port files
