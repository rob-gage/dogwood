+++
title = "Render shaders"
description = "Documents cellular and sprite WGSL inputs, coordinate transforms, ring lookup, and color output."
weight = 20
[extra]
sources = ["engine_graphics/src/renders/scene_renderer_fluid.wgsl", "engine_graphics/src/renders/scene_renderer_gas.wgsl", "engine_graphics/src/renders/scene_renderer_fluid_pass.rs", "engine_graphics/src/renders/scene_renderer_cellular.wgsl", "engine_graphics/src/renders/scene_renderer_sprites.wgsl", "engine_graphics/src/renders/scene_renderer_cellular_pass.rs", "engine_graphics/src/renders/scene_renderer_sprites_pass.rs", "engine_graphics/src/sprites/sprite_sheet.rs"]
+++

This page is the reference for the cellular, fluid, gas, and sprite render shaders, for engine contributors. It covers bindings, record layouts, coordinate math, and color output. Pass resources and draw setup are in [cellular rendering](@/internals/rendering/cellular-pass.md) and [sprite rendering](@/internals/rendering/sprite-pass.md); pass order is in the [scene renderer](@/internals/rendering/scene-renderer.md).

## Cellular shader

`scene_renderer_cellular.wgsl` draws a fullscreen triangle (`vertex`) and shades each covered fragment (`fragment`). The cellular pass uses no vertex buffers. `SceneRendererCellularPass::prepare` binds the uniform buffer and the two buffers exposed by `SceneGraphicsCellular`.

### Bindings

| Group 0 binding | Resource |
|---:|---|
| 0 | Uniform record `CellularUniforms` |
| 1 | Read-only `array<u32>` of material identifiers |
| 2 | Packed-color array matching the identifiers |

### Uniform record

Rust writes the 64-byte `CellularUniforms` record manually:

| Offset | Field |
|---:|---|
| 0 | `viewport_origin` |
| 8 | `viewport_size` |
| 16 | `zoom` (holds `pixels_per_tile`) |
| 20 | `zoom_padding` |
| 24 | `camera_relative_region` (Rust supplies the negated region x/y) |
| 32 / 36 | Region width / height |
| 40 / 44 | Ring offset x / y |
| 48 | Tile size |
| 52-63 | Three padding words |

### Pixel to cell mapping

For each fragment, the shader:

1. Subtracts the viewport origin and centers the pixel in the viewport.
2. Divides by `pixels_per_tile` to convert pixel offsets to tile units, and flips the Y direction.
3. Adds the result to the camera-relative region, multiplies by tile size, and floors to cell coordinates.
4. Discards coordinates outside the region.
5. Calculates the logical tile and in-tile coordinates, adds the ring offset modulo region dimensions, and indexes row-major tile and cell storage.
6. Discards material ID zero as empty.

### Color

Packed RGBA8 channels are unpacked as normalized values. RGB is converted from sRGB to linear light; alpha is kept as unpacked. The output is premultiplied (`linear_rgb * alpha`, `alpha`), and the Rust render target uses premultiplied-alpha blending.

## Sprite shader

`scene_renderer_sprites.wgsl` uses a per-view uniform and sampler in group 0 (bindings 0 and 1). Group 1 binding 0 is the current sprite's 2D texture.

### Records

The 16-byte `SpriteUniforms` record holds viewport width, viewport height, pixels per tile, and padding. The Rust pass packs a 56-byte instance stream matching the vertex attributes consumed by `vertex`:

| Offset | Field | Format |
|---:|---|---|
| 0 | Position and size | `Float32x4` |
| 16 | Rotation | `Float32` |
| 20 | Padding (in the byte stream, not a shader input) | - |
| 24 | Texture edges | `Float32x4` |
| 40 | Color | `Float32x4` |

### Vertex stage

The vertex stage:

1. Selects one of four corners centered on the sprite and scales it by the sprite size.
2. Rotates using sine and cosine and adds the sprite position.
3. Converts from camera-relative tile coordinates to clip space using pixels per tile and viewport dimensions.

It returns `VertexOutput`: builtin clip-space `position`, plus interpolated `texture_coordinates` and `color` varyings. The supplied texture edges are interpolated across the quad; vertical UV runs from the supplied bottom edge toward the top edge.

### Fragment stage and color

The fragment stage samples mip level zero, multiplies the texel by the instance color, then premultiplies the resulting RGB by the resulting alpha. Sprite textures are straight-alpha sRGB, so sampling their `Rgba8UnormSrgb` views yields linear RGB while alpha stays linear. The shader multiplies the sampled linear texel by the instance color values as supplied; it does not convert the tint.

### Sampler and batching

The Rust pass uses a nearest, clamp-to-edge sampler. It groups consecutive instances with the same texture view into batches and draws four vertices per instance, as seen in `SceneRendererSpritesPass::prepare` and `draw`.

## Fluid shader

`scene_renderer_fluid.wgsl` adds `classify` and `shade` compute entry points, each with 8×8 workgroups, plus fullscreen `vertex` and `fragment` entry points. Its bindings, record layout, and shading are documented on [fluid rendering](@/internals/rendering/fluid-pass.md).

## Gas shader

`scene_renderer_gas.wgsl` has fullscreen `vertex` and `fragment` entry points and no compute stage. Its bindings, record layout, and color math are documented on [gas rendering](@/internals/rendering/gas-pass.md).
