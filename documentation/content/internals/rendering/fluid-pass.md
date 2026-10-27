+++
title = "Fluid render pass"
description = "Raster classification, cell shading, and fluid compositing."
weight = 35
[extra]
sources = ["engine_graphics/src/renders/scene_renderer_fluid_pass.rs", "engine_graphics/src/renders/scene_renderer_fluid.wgsl", "engine_graphics/src/world/scenes/scene_graphics_fluid.rs", "engine_graphics/src/renders/scene_renderer.rs", "engine_world/src/scenes/scene.rs", "engine_graphics/src/renders/scene_renderer_lighting_pass.rs", "engine_graphics/src/renders/scene_renderer_lighting.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_cascades.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_mips.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_optical.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_spread.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_sprites.wgsl"]
+++

`SceneRendererFluidPass` renders the particle simulation's cell raster. `Scene::fluid_graphics` supplies borrowed raster, material-property, and particle-color buffers, camera-relative region placement, ring offsets, and tile size. [Scene renderer](@/internals/rendering/scene-renderer.md#pass-order) defines compositing order.

## Preparation and resources

Preparation caches render pipelines by target format and uploads an 80-byte uniform record. It bounds shading to a cell rectangle around the viewport, including a one-cell margin clipped to one cell beyond the region, grows classification and shaded-color buffers when needed, and rebuilds bindings when inputs or scratch buffers change.

| Group 0 binding | Resource |
| --- | --- |
| 0 | `FluidUniforms` |
| 1 | Read-only fixed-point fluid raster (`array<i32>`) |
| 2 | Read-only material records (`array<vec4<f32>>`) |
| 3 | Read-only summed particle color channels (`array<u32>`) |
| 4 | Classified material IDs (`array<u32>`) |
| 5 | Shaded colors (`array<vec2<u32>>`) |

Uniform offsets are viewport origin 0, viewport size 8, zoom 16, camera-relative region 24, region dimensions 32/36, ring offsets 40/44, tile size 48, shaded origin 56, and shaded size 64; other words are padding.

## Classification and shading

Before the render pass, `classify` and `shade` dispatch in 8×8 workgroups. Classification includes eight rows above the shaded rectangle for depth lookup. It smooths raster mass over a 3×3 neighborhood with weights 4 at the center, 2 along axes, and 1 at corners, then divides by sixteen and the selected material density. Occupancy must exceed `0.5` to draw.

> **Limitation:** When multiple materials occupy a sampled neighborhood, classification selects the highest nonempty material identifier, rather than the material contributing the most mass.

Shading averages summed sRGB color channels over the 3×3 cells with raster mass, weighted by their fixed-point mass. If none of those cells has mass, it uses the classified material's packed edit color. RGB converts from sRGB to linear light. Up to eight occupied cells above darken the color by `0.035` per cell; a cell without water directly above receives a `1.08` surface multiplier. The particle-weighted alpha is retained and does not increase with depth or occupancy above the threshold. Premultiplied color is packed into four half-floats (eight bytes per cell).

The fullscreen triangle's fragment stage maps pixels to logical cells and fetches their shaded colors. Empty colors are discarded. The draw uses premultiplied-alpha blending, between negative-depth sprites and cellular matter.

Rendering shades the cell raster, not individual particle sprites or a continuous reconstructed surface. Ring lookup reads physical cells; samples outside the region contribute no mass. The eight-row depth search stops at any empty classified cell, regardless of material changes. Scratch buffers grow but do not shrink, and pipelines are retained per target format. See [fluid coupling](@/internals/simulation/fluid-coupling.md#raster-bridge) for how raster mass and material selection are produced.

Fluid appearance uses the radiance accumulated in the fluid raster and samples the lighting texture. The raster cell has eight `u32` words: four existing color sums, three emission sums, and an occlusion sum. Fluid heat records also supply render-time temperature colors. See [lighting pass](@/internals/rendering/lighting-pass.md).
