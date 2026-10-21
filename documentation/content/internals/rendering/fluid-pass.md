+++
title = "Fluid render pass"
description = "Raster classification, cell shading, and fluid compositing."
weight = 35
[extra]
sources = ["engine_graphics/src/renders/scene_renderer_fluid_pass.rs", "engine_graphics/src/renders/scene_renderer_fluid.wgsl", "engine_graphics/src/world/scenes/scene_graphics_fluid.rs", "engine_graphics/src/renders/scene_renderer.rs", "engine_world/src/scenes/scene.rs"]
+++

`SceneRendererFluidPass` renders the particle simulation's cell raster. `Scene::fluid_graphics` supplies borrowed raster and material-property buffers, camera-relative region placement, ring offsets, and tile size. [Scene renderer](@/internals/rendering/scene-renderer.md#pass-order) defines compositing order.

## Preparation and resources

Preparation caches render pipelines by target format and uploads an 80-byte uniform record. It bounds shading to the viewport's intersection with the active region, grows classification and shaded-color buffers when needed, and rebuilds bindings when inputs or scratch buffers change.

| Group 0 binding | Resource |
| --- | --- |
| 0 | `FluidUniforms` |
| 1 | Read-only fixed-point fluid raster (`array<i32>`) |
| 2 | Read-only material records (`array<vec4<f32>>`) |
| 3 | Classified material IDs (`array<u32>`) |
| 4 | Shaded colors (`array<vec2<u32>>`) |

Uniform offsets are viewport origin 0, viewport size 8, zoom 16, camera-relative region 24, region dimensions 32/36, ring offsets 40/44, tile size 48, shaded origin 56, and shaded size 64; other words are padding.

## Classification and shading

Before the render pass, `classify` and `shade` dispatch in 8×8 workgroups. Classification includes eight rows above the shaded rectangle for depth lookup. It smooths raster mass over a 3×3 neighborhood with weights 4 at the center, 2 along axes, and 1 at corners, then divides by sixteen and material density. Occupancy must exceed `0.5` to draw.

> **Limitation:** When multiple materials occupy a sampled neighborhood, classification selects the highest nonempty material identifier, rather than the material contributing the most mass.

Shading reads packed RGBA from word 7 of each 32-byte material record. RGB converts from sRGB to linear light. Up to eight occupied cells above darken the color by `0.035` per cell; a cell without water directly above receives a `1.08` surface multiplier. Material alpha is retained. Premultiplied color is packed into four half-floats (eight bytes per cell).

The fullscreen triangle's fragment stage maps pixels to logical cells and fetches their shaded colors. Empty colors are discarded. The draw uses premultiplied-alpha blending, between negative-depth sprites and cellular matter.
