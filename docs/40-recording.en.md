# Process recording: file layout (`.gsrec`)

A recording is a file next to the picture (`picture.sai2.gsrec`) that is only ever appended to. Code —
`crates/paint-formats/src/record.rs`, menu and window — `crates/paint-app/src/recording.rs`.

## How it is written

- Every layer has a number `Layer::uid` (a copied layer gets its own). Numbering starts at “launch second
  × 2²⁰”, so numbers do not repeat across launches either; when a recording is continued, layers get back
  the numbers from its last frame (same depth, kind and name).
- The document keeps a change journal (`Document::journal`): which 256×256 tiles of which layer changed,
  which layer changed as a whole (insert, delete, curves, mask), whether the canvas changed. History
  steps, undo and redo fill it.
- Once per window frame (not during a stroke) the recorder takes the journal and writes tiles, then a
  frame — the layer tree with properties. Compression and disk writes run on a separate thread; the file
  is flushed after every frame, so a crash loses at most one step.
- Every recording start begins with a full frame (canvas and all layers).

## Records

Header `GSREC\x01\0\0`, then records: kind (1 byte), length (u32 LE), data.

| Kind | Data |
|---|---|
| `K` canvas | width u32, height u32, background RGBA (A = 0 — transparent) |
| `C` layer cleared | layer number u64 |
| `T` tile | layer number u64, x i32, y i32 (in tiles), flag: 1 — then straight RGBA8 256×256, deflate-compressed; 0 — tile is empty |
| `S` stopped | empty: the user stopped recording (opening the picture will not continue it automatically) |
| `F` frame (step) | time, ms since 1970 (u64), background RGBA, node count u32, nodes: number u64, depth u8, flags u8 (1 visible, 2 folder, 4 clipping, 8 pass-through folder), opacity u8, mode u8 (index in `BlendMode::ALL`), name length u16, UTF-8 name |

A torn last record (crash mid-write) is skipped on reading.

## Export

Tiles are collected per layer number; at every chosen step a document is built from the frame's tree
(excluded layers are invisible), flattened, scaled down (even sides), watermarked and turned into an APNG
frame (`png` crate, only the changed rectangle). The file is read one record at a time, tiles are inflated
when needed, and the video goes straight to disk: 1500 strokes on a 3000×3000 canvas (95 MB recording)
export in ~35 s using ~110 MB of memory. Steps are picked evenly so the video lasts the chosen time; the
last step is always included. With a password the video goes into a zip with AES-256 encryption (`zip`
crate).

Why APNG rather than MP4: H.264 and similar codecs are encumbered by patents, while APNG opens in any web
browser and is written by our own code. AV1 (`rav1e`) can be added later.
