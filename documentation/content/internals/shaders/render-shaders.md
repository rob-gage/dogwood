+++
title = "Render shaders"
description = "Documents cellular and sprite WGSL inputs, coordinate transforms, ring lookup, and color output."
weight = 20
[extra]
sources = ["engine_graphics/src/renders/scene_renderer_fluid.wgsl", "engine_graphics/src/renders/scene_renderer_pressure.wgsl", "engine_graphics/src/renders/scene_renderer_overlay.wgsl", "engine_graphics/src/renders/scene_renderer_gas.wgsl", "engine_graphics/src/renders/scene_renderer_fluid_pass.rs", "engine_graphics/src/renders/scene_renderer_cellular.wgsl", "engine_graphics/src/renders/scene_renderer_surface.wgsl", "engine_graphics/src/renders/scene_renderer_sprites.wgsl", "engine_graphics/src/renders/scene_renderer_shaded.wgsl", "engine_graphics/src/renders/scene_renderer_shaded_pass.rs", "engine_graphics/src/renders/scene_renderer_cellular_pass.rs", "engine_graphics/src/renders/scene_renderer_surface_pass.rs", "engine_graphics/src/renders/scene_renderer_sprites_pass.rs", "engine_graphics/src/renders/scene_renderer_parallax_batches_pass.rs", "engine_graphics/src/sprites/sprite_sheet.rs", "engine_graphics/src/renders/scene_renderer_lighting_pass.rs", "engine_graphics/src/renders/scene_renderer_lighting_clear.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_cache.wgsl", "engine_graphics/src/renders/scene_renderer_lighting.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_cascades.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_mips.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_optical.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_spread.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_sprites.wgsl", "engine_graphics/src/renders/scene_renderer_decorations.wgsl", "engine_graphics/src/renders/scene_renderer_decoration_field.wgsl", "engine_graphics/src/world/materials/material_radiance.wgsl"]
+++

This page references the cellular, fluid, gas, pressure, overlay, sprite, shaded-body, decoration, and field shaders. It covers bindings, record layouts, coordinate math, and color output. Decoration behavior is described in [chunk decorations](@/internals/rendering/decorations.md); pass resources and draw setup are in [cellular rendering](@/internals/rendering/cellular-pass.md) and [sprite rendering](@/internals/rendering/sprite-pass.md); pass order is in the [scene renderer](@/internals/rendering/scene-renderer.md).

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
| 52 | Surface-art X offset modulo 256 cells |
| 56 | Surface-art Y offset modulo 256 cells |
| 60 | Padding |

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

1. Selects one of four corners centered on the sprite and scales it by the sprite size. The batch entry point adds the batch origin first.
2. Rotates using sine and cosine and adds the sprite position.
3. Converts from camera-relative tile coordinates to clip space using pixels per tile and viewport dimensions.

It returns `VertexOutput`: builtin clip-space `position`, plus interpolated `texture_coordinates` and `color` varyings. The supplied texture edges are interpolated across the quad; vertical UV runs from the supplied bottom edge toward the top edge.

### Fragment stage and color

The fragment stage samples mip level zero, multiplies the texel by the instance color, then premultiplies the resulting RGB by the resulting alpha. Sprite textures are straight-alpha sRGB, so sampling their `Rgba8UnormSrgb` views yields linear RGB while alpha stays linear. The shader multiplies the sampled linear texel by the instance color values as supplied; it does not convert the tint.

### Sampler and batching

The Rust pass uses a nearest, clamp-to-edge sampler. It groups consecutive instances with the same texture view into batches and draws four vertices per instance, as seen in `SceneRendererSpritesPass::prepare` and `draw`.

Static parallax batches use an 80-byte instance record: position/size (16 bytes), rotation and padding (8), texture edges (16), color (16), radiance UV rectangle (16), and radiance intensity/padding (8). `vertex_batch` adds the batch origin; its 32-byte uniform stores origin, cellular-region origin, and cell size in five `f32` values. Batch uniforms use a 256-byte stride. `fragment_batch` uses the surface field for shadows cast by solid cells and applies lighting from the radiance texture.

The cellular bindings also include the surface field (`texture_2d<f32>`), a read-only material-to-layer `array<u32>`, and a `texture_2d_array<f32>` for world-aligned surface art. The field comes from `scene_renderer_surface.wgsl`: R is closeness, GB is direction toward open space encoded around 0.5, and A is solidity.

Cells of a styled material within the field's reach but beyond their surface art shift color in three steps of closeness: lighter and warmer facing up, darker and cooler facing down. Materials without a surface are unchanged. The surface field wraps by world position, and each frame writes alternate rows unless the region jumped, so its compute cost stays low; solid cells checked by a sparse pattern skip full sampling. `fragment_batch` skips texels behind solid cells and does not sample radiance. `Scene::set_material_edges` turns the surface art, this shift, contact shadows, and the surface field's computation on or off, and is off by default.

## Fluid shader

`scene_renderer_fluid.wgsl` adds `classify` and `shade` compute entry points, each with 8×8 workgroups, plus fullscreen `vertex` and `fragment` entry points. Its bindings, record layout, and shading are documented on [fluid rendering](@/internals/rendering/fluid-pass.md).

## Gas shader

`scene_renderer_gas.wgsl` has fullscreen `vertex` and `fragment` entry points and no compute stage. Its bindings, record layout, and color math are documented on [gas rendering](@/internals/rendering/gas-pass.md).


The pressure view shader maps the published buffer through the pressure gradient; the overlay shader draws cell-aligned circles and rectangles after scene content. See [scene renderer](@/internals/rendering/scene-renderer.md#pass-order).

## Shaded soft-body shader

`scene_renderer_shaded.wgsl` draws a triangle mesh while checking each fragment against the body's outline. It discards points outside the outline and calls the actor-supplied `shade(ShadeInput) -> ShadeOutput` function inside it. The input contains camera-relative position, outline center, distance inward from the outline, the maximum outline distance to the body's deepest point, and the direction toward the nearest outline point, plus four parameter vectors. `ShadeOutput` supplies linear albedo and emission; albedo receives scene lighting and emission is added afterward. When `is_cell_snapped` is set, the point used for outline and shading is aligned to the center of a world cell.

## Lighting shaders

The lighting pipeline is assembled from `scene_renderer_lighting*.wgsl` modules after the shared `MATERIAL_RADIANCE_WGSL` functions. The Rust pass defines the lighting bind group index and dispatches optical construction, cascades, spread, and mip generation. Layout and constants are documented in the [lighting pass reference](@/internals/rendering/lighting-pass.md).
