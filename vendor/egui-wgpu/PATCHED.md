# egui-wgpu 0.29.1 (patched copy)

Copy of the crate `egui-wgpu` 0.29.1 from crates.io (https://github.com/emilk/egui, MIT OR
Apache-2.0, Copyright (c) 2018-2021 Emil Ernerfeldt), used through `[patch.crates-io]` in the
workspace `Cargo.toml`.

Two changes, in `src/renderer.rs`.

1. `Renderer::free_texture`: the texture is dropped instead of
`destroy()`ed. egui frees a texture in the same frame that still draws it, and the frame is
submitted after the free, so the immediate destroy made wgpu fail with "Texture … has been
destroyed" (GrazySAI replaces layer thumbnails and previews while painting). Dropping lets wgpu
release the texture once the GPU is done with it.

2. Mipmaps: `TextureOptions::mipmap_mode` had no effect in 0.29.1 (one mip level, no mipmap filter in
   the sampler), so zoomed-out canvas tiles looked sharper and shimmered compared with OpenGL. Textures
   that ask for mipmaps now get a full chain, built by a small blit pipeline (`generate_mips`) after each
   write.

Remove this copy when GrazySAI moves to an egui version where this is fixed upstream.
