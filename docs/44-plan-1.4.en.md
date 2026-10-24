# 1.4.0 plan — from Clip Studio Paint

**English** | [Русский](44-plan-1.4.md)

Research before the work (October 2026): how Clip Studio Paint (CSP) and other programs do it,
what GrazySAI already has, how to build it. No code for this section yet.

## 1. Auto Actions

**CSP:** an Auto Actions palette (Window): record, perform steps, stop; play by double-click or
Play, optionally starting from the selected step. Steps can be duplicated, reordered, deleted or
switched off by a check mark. Actions live in sets (like folders) shared through Clip Studio
Assets. Menu and dialog commands are recorded (filters, layers, selection, transform); drawing,
tool switching, undo and layer reordering are not. Photoshop's Actions are similar.

**GrazySAI now:** process recording (`.gsrec`) records every drawing step for video, not
commands; Lua plugins call program commands.

**How:** give every menu command a name and parameters (the same ones plugins see); a recording
is a list of “command + dialog parameters” in a set file (text, like `.gsbrush`); an actions
palette with sets, check marks, “play from selected”; one action is one history step. Sharing by
set file.

## 2. Layer styles and Gradient Map

**CSP:** Layer Property: **border** (a solid line of given width and colour, or a watercolour
edge), **tone** (halftone dots), **layer colour** (recolour to one colour). **Gradient Map** is a
correction layer: lower layers' colours are mapped by brightness onto a gradient (nodes from dark
to light); gradient sets, edit by double-click, a layer mask; the Edit menu version is applied
destructively. Photoshop has Layer Styles (stroke, drop/inner shadow, glow, bevel, colour/gradient
overlay) and a Gradient Map fill layer.

**GrazySAI now:** SAI-style layer effects (texture and effect), masks, blend modes, Hue/Saturation
and Brightness/Contrast filters; no correction layers.

**How:** correction layers (a new layer kind: parameters instead of pixels, affecting everything
below inside its folder) — gradient map, levels, curves, hue/saturation; layer styles — a list of
effects on a layer applied when compositing: border, shadow, glow, colour overlay, tone. SAI 2017's
`.sai2` has none of this — keep it in our own block (as for the 3D layer) and write the finished
picture into the layer so SAI sees the result. PSD has native blocks for styles and gradient maps
(`lfx2`, `grdm`) to read and write.

## 3. Import from other programs (3D)

**CSP** imports its own `cs3c`, `cs3o`, `cs3s` plus FBX, 6KT, 6KH (Shade), LWO, LWS (LightWave),
OBJ, GLB, glTF, VRM. **Cinema 4D** does not expose its `.c4d` format but saves FBX, glTF, Alembic,
COLLADA (1.4/1.5), OBJ and USD — so the way from C4D to us is **FBX or glTF**. The same holds for
Blender, Maya, 3ds Max, Houdini, ZBrush, Daz Studio, VRoid, MMD (PMX via Blender).

**GrazySAI now:** glTF/GLB and OBJ (skeleton, poses, morphs, textures); 1.4.0 adds FBX (ufbx), VRM, PMX and COLLADA.

**To add (by usefulness):**
1. **FBX** — the main exchange format (C4D, Maya, Max, Mixamo, Daz). Reader: the ufbx library
   (C, two files, MIT / public domain; Rust bindings `ufbx`).
2. **VRM** — glTF with extensions (VRoid, avatars): the skeleton is already read; add VRM bone
   names, facial expressions (blend shapes), simplified MToon materials.
3. **COLLADA (`.dae`)** — XML, older packages and SketchUp.
4. **USD/USDZ**, **Alembic** — heavy C++ libraries, mostly for animation and caches; postpone,
   recommend exporting glTF.
5. **PMX** (MikuMikuDance) — a simple binary format with many character models.

## 4. User customisation

**CSP:** Command Bar (icons for menu commands and tools), Quick Access (several sets, with
labels, movable), workspaces (palette layouts saved and switched), custom shortcuts for everything
(with conflicts), modifier keys (what Shift/Ctrl/Alt/Space do per tool), tablet side keys.
**Krita:** dockers anywhere saved as workspaces, toolbars edited by right-click (order, labels,
icon size), command search, two shortcuts per command, canvas input (pen and mouse gestures).
**Photoshop:** workspaces, custom menus (hide or colour items), custom toolbar.

**GrazySAI now:** “Shortcuts…” (custom keys), panel layout (left/right, separate panels), UI
scale, dark theme, Lua plugins.

**How:** workspaces (name → panel layout and visibility, scale, theme); a Quick Access panel with
any commands (Phosphor icons from the menus) and several sets; command search (like
Ctrl+Shift+P); canvas gesture settings (what Space, right button, pen buttons do); pen buttons and
tablet keys mapped to commands (from the Tablets section).

## Sources

- CSP: [Auto Actions](https://help.clip-studio.com/en-us/manual_en/720_preferences/Auto_Actions.htm), [layer properties](https://help.clip-studio.com/en-us/manual_en/180_layers/Layer_properties.htm), [gradient map](https://www.clip-studio.com/site/gd_en/csp/userguide/csp_userguide/500_menu/500_menu_layer_shikichou_gradation.htm), [3D import](https://help.clip-studio.com/en-us/manual_en/660_3d/Importing_3D_Files.htm), [workspace](https://tips.clip-studio.com/en-us/articles/4626), [shortcuts](https://tips.clip-studio.com/en-us/articles/5167)
- Cinema 4D: [exchange formats](https://help.maxon.net/c4d/en-us/)
- FBX: [ufbx for Rust](https://docs.rs/crate/ufbx/0.11.0)
- Krita: [navigation and customisation](https://docs.krita.org/Navigation)
