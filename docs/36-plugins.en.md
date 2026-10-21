# GrazySAI plugins: writing your own

A plugin is a small program in **Luau** (a Lua variant) that adds commands to the “Plugins” menu and
changes the open picture. Any text editor will do; no compiler is needed. Examples are in
[`plugins/examples/`](../plugins/examples/).

The easiest place to write a plugin is the **GrazySAI Studio** workshop (the “Plugin” tab): it has
templates, a test on a sample picture with errors by line, and an “Export to GrazySAI” button that puts the
plugin into the menu of the running painting program at once. Below is how to do the same by hand.

## A first plugin in 10 minutes

1. Make a folder with two files.

   `plugin.txt`:

   ```
   id=myname.brighten
   name=Brighten
   version=1.0
   author=Me
   description=Makes the layer lighter.
   api=1
   command.0.menu=Mine/Brighten
   command.0.function=brighten
   command.0.param.0=amount slider 0 100 1 30 How much, %
   ```

   `main.lua`:

   ```lua
   function brighten(p)
     local k = p.amount / 100
     canvas.map(function(r, g, b, a)
       return r + (255 - r) * k, g + (255 - g) * k, b + (255 - b) * k
     end)
   end
   ```

2. Zip both files (no folder inside) and rename the zip to `brighten.gsplugin`.
3. In GrazySAI: “File → Open” that file (or drag it onto the window) → “Install”.
4. “Plugins → Mine → Brighten” opens a window with the “How much, %” slider; press “OK”.

A code error is shown in the status bar and in “Plugins → Manage plugins…” with the line number, for
example `myname.brighten/main.lua:3: attempt to index nil`.

## `plugin.txt`

One `key=value` per line; lines starting with `#` are comments.

| Key | Meaning |
|---|---|
| `id` | plugin name in Latin letters, digits and `._-`, best “author.plugin”; a new version replaces the plugin with the same id |
| `name`, `version`, `author`, `description` | shown on install and in the “Plugins” window |
| `api` | API version (currently `1`); an older program will not run a plugin with a higher one |
| `command.N.menu` | path in the “Plugins” menu: `Submenu/Command` (commands of different plugins with one submenu share it) |
| `command.N.function` | the function in `main.lua` that the command runs |
| `command.N.param.M` | a field of the parameter window (see below); without fields the command runs at once |

Commands are numbered from 0 in a row (up to 64), fields as well (up to 32).

Fields:

| Kind | Syntax | Value in the function |
|---|---|---|
| slider | `id slider min max step default Label` | a number (step 1 — whole) |
| checkbox | `id check 0_or_1 Label` | `0` or `1` |
| choice | `id choice index Option1\|Option2\|… Label` | option index from 0 |

The function receives a table of values: `p.amount`, `p.mode` …

## API

A plugin sees only these functions and the safe part of the Luau standard library (`math`, `string`,
`table`, `utf8`, `bit32`). Colors are numbers 0..255, not premultiplied by alpha; coordinates are canvas
pixels, (0, 0) is the top left corner.

| Function | What it does |
|---|---|
| `canvas.width()`, `canvas.height()` | canvas size |
| `canvas.get(x, y)` → `r, g, b, a` | a point of the current layer (outside — 0, 0, 0, 0) |
| `canvas.set(x, y, r, g, b[, a])` | set a point (a defaults to 255); fractions are rounded, values clamped to 0..255 |
| `canvas.map(f[, x0, y0, x1, y1])` | call `f(r, g, b, a, x, y)` for every point of the frame and store what it returns (`r, g, b[, a]`); the frame defaults to the selection, without one to the whole canvas. Faster than `get`/`set` one by one |
| `canvas.selection()` → `x0, y0, x1, y1` or `nil` | selection frame (right and bottom edges excluded) |
| `canvas.selected(x, y)` → 0..1 | how much the point is selected (1 without a selection) |
| `canvas.bounds()` → `x0, y0, x1, y1` or `nil` | frame of the layer's non-empty points |
| `canvas.new_layer([name])` | a new layer above the current one; `get`/`set`/`map` work with it from then on |
| `color.get()` → `r, g, b` | the main color |
| `color.set(r, g, b)` | change the main color |
| `log(…)` | a line in the “Plugins” window log (up to 1000 lines per run) |

A plugin works on the current layer if it is a normal layer (not a folder or a linework layer).

## What a plugin cannot do

The code runs in a sandbox:

- no access to files, network, the system or other programs (`io`, `os`, `debug`, `require`, `dofile`,
  `load` are absent);
- shared tables (`math`, `string` …) are read-only;
- memory up to 256 MB, time up to 60 seconds, then the plugin is stopped;
- every run starts from scratch: a plugin keeps nothing between runs.

Everything a plugin does to the picture can be undone (“Edit → Undo”): changes to each layer as one
step, a new layer as another.

## Speed

Luau is interpreted: about a million points per second through `canvas.map`. For filters over a whole
large canvas that means seconds; heavy filters can later be written in WebAssembly
(`docs/34-brushes-and-plugins.md`).

## Testing and updates

- Plugins live in the settings folder under `plugins/` (“Plugins → Open plugins folder”).
- A package in `plugins/dev/` replaces the installed one with the same `id` and is picked up live when
  the marker file `plugins/dev/.changed` changes; that is how the GrazySAI Studio workshop will work with
  plugins.
- New version: raise `version` and install the package again — it replaces the old one.
