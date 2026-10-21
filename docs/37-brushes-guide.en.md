# Your own GrazySAI brushes: making and sharing them

A brush is a `.gsbrush` file (one brush) or `.gsbrushes` (a set). The easiest place to make one is the
**GrazySAI Studio** workshop that ships next to GrazySAI (`GrazySAI-Studio`, on Windows
`GrazySAI-Studio.exe`). File layout — [docs/35-gsbrush-format.md](35-gsbrush-format.md).

## In 5 minutes

1. Open GrazySAI and GrazySAI Studio at the same time.
2. In the workshop, “Brush” tab: “New” → a kind (pencil, brush, watercolor…), a name.
3. “Tip” → “Create: spray” (or “Add picture…” for your own picture).
4. “Dynamics” → “+ Link…” → “Random → Scatter”, raise the right point of the curve.
5. Watch the sample stroke — it is redrawn on every change; you can draw on the test strip.
6. **“Export to GrazySAI”** — the brush appears at the end of the painting program's grid, labelled
   “Studio”. Paint with it, go back to the workshop, adjust, “Export” again — the brush is replaced.
7. Done — “Save as…” (`.gsbrush`) and send the file. The recipient opens it with “File → Open” or drags it
   onto the GrazySAI window.

## What a brush is made of

| Section | What it gives |
|---|---|
| Kind | behaviour: pencil — even paint; brush and watercolor — mixing with the canvas; eraser |
| Size and density | diameter, opacity, pressure on them; edge hardness; dab spacing |
| Mixing | for brush, watercolor, marker: how much canvas color and transparency is picked up |
| Tip | a circle or a picture; “Rotate”, “Thickness” (squash), “along stroke” (turn with the stroke), a picture set “random” |
| Texture | a pattern that holds paint back (its dark parts) |
| Dynamics | “input → curve → parameter” links: pressure, speed, direction, distance, random → size, density, rotation, thickness, scatter |
| From plugin | custom dynamics written in Luau (docs/36-plugins.en.md) |

## Recipes

| Brush | How |
|---|---|
| Calligraphy pen | circle tip, thickness 20 %, rotation 45° |
| Pen tapering at the end | “Distance → Size” with a curve falling to the right; scale 300–600 px |
| Spray | “spray” tip, spacing 0.5–1; “Random → Scatter” and “Random → Rotate” |
| Chalk | “chalk” tip, “Random → Rotate”, density 70 % |
| Dry brush | “Brush” kind, a texture, “Speed → Density” with a falling curve |
| Stamp pattern | your own picture, spacing 1.2–2 (dabs do not merge) |

## Tip pictures

- PNG with transparency: paint is the opaque part; without transparency: paint is the dark part.
- GIMP brushes (`.gbr`, `.gih` — a set), Photoshop (`.abr`), tips from Krita and Clip Studio Paint brushes
  work too (just open them in GrazySAI).
- The picture's longer side is shrunk to 512 px — a brush needs no more.

## Sharing

A brush file is an ordinary zip: send it, put it on a website or in a repository. It holds only settings and
pictures; the program checks them on opening (sizes, names without paths). There is no shared brush catalog
yet; when there is, it will be an open repository with a list of files.
