+++
title = "Cellular render pass"
description = "GPU resources and draw setup for resident cellular tile colors."
weight = 30
[extra]
sources = ["engine_graphics/src/renders/scene_renderer_cellular_pass.rs", "engine_graphics/src/renders/scene_renderer_cellular.wgsl", "engine_graphics/src/renders/scene_renderer_surface_pass.rs", "engine_graphics/src/renders/scene_renderer_surface.wgsl", "engine_graphics/src/renders/scene_renderer_surface_tests.rs", "engine_graphics/src/world/scenes/scene_graphics_cellular.rs", "engine_graphics/src/world/scenes/scene_graphics_material_surfaces.rs", "engine_graphics/src/renders/scene_renderer_lighting_pass.rs", "engine_graphics/src/renders/scene_renderer_lighting_clear.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_cache.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_blocks.wgsl", "engine_graphics/src/renders/scene_renderer_lighting.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_cascades.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_mips.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_optical.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_spread.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_sprites.wgsl", "engine_graphics/src/world/materials/material_radiance.wgsl", "engine_world/src/materials/material_table.rs", "engine_world/src/scenes/scene_material_surfaces.rs"]
+++

## Resources

This page is for engine contributors. It describes the GPU resources and draw setup of the pass that renders resident cellular tile colors.

`SceneRendererCellularPass` owns a 96-byte uniform buffer, a bind group layout, a WGSL module, and render pipelines cached by target `TextureFormat`. The bind group has material IDs and packed colors as read-only storage buffers, plus the surface field texture, per-thermal-index surface layer storage, and a 2D array of surface art.

| Group 0 binding | Resource | Access |
| --- | --- | --- |
| 0 | Per-view uniform buffer | Uniform |
| 1 | Material identifier buffer | Read-only storage |
| 2 | Packed color buffer | Read-only storage |
| 3 | Matter buffer | Read-only storage |
| 4 | Surface field (`Rgba8Unorm`) | Texture |
| 5 | Material surface layer indices | Read-only storage |
| 6 | Surface art (`Rgba8UnormSrgb` array) | Texture |

The region data comes from {{ <rustdoc path="engine_graphics/world/scenes/struct.SceneGraphicsCellular.html" text="SceneGraphicsCellular" /> }}. The view uniform contains viewport origin/size, zoom, camera-relative region origin, region size, ring offsets, cell size, and world-aligned surface-art offsets. The final three words are surface X/Y offsets modulo 256 and padding.


## Cell cache

Each cell covers several pixels, so `SceneRendererCellularPass::shade` runs the `cache_cells` compute entry point before the render pass, over the cells under the viewport plus two on each side, and writes one 16-byte record per cell into `cell_cache` (binding 7, read-write in both stages like the gas scratch buffers): the cell's color and radiance after the surface art and the amount and temperature transforms, as half floats, with a one beside the radiance for a cell holding a material and zero for none. The fragment shader reads the record, discards a zero one, and applies per-pixel light with `lit_color`. `cell_colors` computes the color and radiance for both `cache_cells` and the fragment shader's own path, which pixels outside the cached rectangle use. Prepare skips the cache, leaving its size zero, when a cell would cover fewer than about 1.25 pixels, since shading each pixel's cell is then cheaper. Setting `DOGWOOD_CELLULAR_FULL_SHADE=1` at renderer creation shades in every pixel as before. The cache stores half floats, so its pixels can differ from the per-pixel path by under a thousandth of the value; the GPU test renders random cells with surface art through both and compares every pixel.

## Material surfaces

`SceneRendererSurfacePass` computes an `Rgba8Unorm` field for the visible cell region before drawing. Each texel stores R closeness to the opposite occupancy state (1 at a boundary, decaying over four cells), G/B direction toward open space mapped from -1..1 to 0..1, and A solidity (1 solid, 0 open). It searches neighboring cells up to four cells away; outside-region cells count as solid. The surface pass uses 8×8 workgroups and an eight-cell field margin. The `compute_shared` entry point, used by default, has each workgroup load its 16×16 cells (the 8×8 it writes plus a four-cell halo) into workgroup memory once, so every offset reads that copy instead of recomputing ring positions in storage; it writes exactly the field of the reference `compute` entry point, which `DOGWOOD_SURFACE_SHARED=0` selects. The cellular shader selects a material's two-layer art, chooses facing from the field direction, and draws a texel when closeness exceeds `1 - alpha`; the art alpha therefore encodes reach.

## Preparation and draw

### Pipeline

`prepare` creates a pipeline on first use of each target format. The pipeline has these properties:

- `vertex` and `fragment` entry points in `scene_renderer_cellular.wgsl`.
- Triangle-list primitive without culling, one sample, and no depth state.
- Premultiplied-alpha blending.
- No vertex buffers.

### Uniform

The uniform encodes:

- viewport origin and dimensions in pixels;
- `pixels_per_tile`;
- the negated camera-relative region origin;
- active region width and height;
- tile-ring offsets and cells per tile.

The WGSL defines the matching layout and maps fragments through the tile ring; see [Render shaders](@/internals/shaders/render-shaders.md) for the mapping and bindings.

### Draw

`draw` selects the pipeline for the current target format, binds the prepared group at index 0, and draws vertices `0..3` for one instance. That range forms a full-screen triangle over the active viewport. The pass runs between sprites of negative depth and sprites of nonnegative depth; see [Scene renderer](@/internals/rendering/scene-renderer.md).

The cellular fragment shader reads radiance from the second word paired with packed color, applies thermal color/radiance transforms, and samples the light texture. The cellular GPU color buffer is therefore two `u32` words per cell. See [lighting pass](@/internals/rendering/lighting-pass.md).
