+++
title = "Fluid render pass"
description = "Raster classification, cell shading, and fluid compositing."
weight = 35
[extra]
sources = ["engine_graphics/src/renders/scene_renderer_fluid_pass.rs", "engine_graphics/src/renders/scene_renderer_fluid.wgsl", "engine_graphics/src/world/scenes/scene_graphics_fluid.rs", "engine_graphics/src/renders/scene_renderer.rs", "engine_world/src/scenes/scene.rs", "engine_graphics/src/renders/scene_renderer_lighting_pass.rs", "engine_graphics/src/renders/scene_renderer_lighting_clear.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_cache.wgsl", "engine_graphics/src/renders/scene_renderer_lighting.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_cascades.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_mips.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_optical.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_spread.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_sprites.wgsl", "engine_world/src/materials/material_table.rs", "engine_graphics/src/world/materials/material_graphics_fluid.rs", "engine_world/src/scenes/fluids/fluid_particle.wgsl"]
+++

`SceneRendererFluidPass` renders the particle simulation's cell raster. `Scene::fluid_graphics` supplies borrowed raster, material-property, material-graphics, particle-color, and cellular material identifier buffers, camera-relative region placement, ring offsets, tile size, and `time`, the scene's simulated time in seconds (`simulated_ticks / 60`, the value the decorations pass uses), so the texture pauses with the game. [Scene renderer](@/internals/rendering/scene-renderer.md#pass-order) defines compositing order.

## Preparation and resources

Preparation caches render pipelines by target format and uploads an 80-byte uniform record. It bounds shading to a cell rectangle around the viewport, including a one-cell margin clipped to one cell beyond the region, grows classification and shaded-cell buffers when needed, and rebuilds bindings when inputs or scratch buffers change.

| Group 0 binding | Resource |
| --- | --- |
| 0 | `FluidUniforms` |
| 1 | Read-only fixed-point fluid raster (`array<i32>`) |
| 2 | Read-only material records (`array<vec4<f32>>`) |
| 3 | Read-only summed particle color channels (`array<u32>`) |
| 4 | Classified material IDs (`array<u32>`) |
| 5 | Shaded cells, 32 bytes each (two `vec4<u32>` records): premultiplied color and surface band, then texture parameters (`array<vec4<u32>>`) |
| 6 | Read-only fluid heat records |
| 7 | Read-only species amounts, a section of the raster buffer |
| 8 | Read-only cellular material identifiers of every cell, ring layout (`array<u32>`), compute only |
| 9 | Read-only `fluid_graphics`: three `vec4<f32>` per fluid material (48 bytes), indexed by `material_identifier & 0x3fffffff` in the order of binding 2; absorption RGB and depth opacity, then surface brightness, surface sheen, surface band, and a reserved `0.0`, then `[shimmer, texture_scale, texture_speed, crust]` (`array<vec4<f32>>`), compute only |


Uniform offsets are viewport origin 0, viewport size 8, zoom 16, camera-relative region 24, region dimensions 32/36, ring offsets 40/44, tile size 48, shaded origin 56, and shaded size 64, and `time` (`f32`) 72; other words are padding.

## Classification and shading

Before the render pass, `classify` and `shade` dispatch in 8×8 workgroups. Classification includes 16 rows above the shaded rectangle for depth lookup. It smooths raster mass over a 3×3 neighborhood with weights 4 at the center, 2 along axes, and 1 at corners, then divides by the selected material density. Solid neighbors, found through the cellular material identifiers, are excluded from the smoothing and from its normalization (the weight sum). Solid cells are never classified as fluid. A cell is fluid where its smoothed occupancy exceeds `0.5`.

The dominant material is the one with the most weighted mass in the 3×3 neighborhood; it was previously the highest nonempty material identifier.

Shading averages summed sRGB color channels over the 3×3 cells with raster mass, weighted by their fixed-point mass. If none of those cells has mass, it uses the classified material's packed edit color. RGB converts from sRGB to linear light, then amount and temperature colors apply. The depth is the count of consecutive fluid cells directly above, searched up to 16 rows. Per material, from `fluid_graphics`, the lit color is absorbed exponentially by depth, `exp(-absorption * depth)` per channel, and the alpha rises toward opaque as `1 - (1 - alpha) * exp(-depth_opacity * depth)`. Light is sampled at the cell center. Premultiplied color is body color times alpha plus radiance emission.

An open-surface cell has neither fluid nor solid matter directly above it. If its material's `surface_band` is above zero, `shade` also records a band color, `body * alpha * (surface_brightness - 1) + light * surface_sheen * alpha`, and the band height. The first 16-byte record of the shaded cell holds the premultiplied color in words 0 and 1 and the band in words 2 and 3, as half-floats; the second carries the material's texture parameters. The lighting optics never see the texture; they use the per-cell averages.

The fullscreen triangle's fragment stage maps pixels to logical cells and discards pixels whose own cell was not shaded, so fluid edges follow the cell grid. Inside, color is interpolated bilinearly between the shaded cells around the pixel, normalized over those cells so edge cells keep their own color up to the edge. On an open-surface cell the fragment adds the band color along the top `surface_band` of the cell, antialiased over one pixel at the band's lower edge. The draw uses premultiplied-alpha blending, between negative-depth sprites and cellular matter.

The fragment then textures the result with two octaves of value noise anchored to world cell coordinates, divided by `texture_scale` cells per feature, averaging zero and churning with `time * texture_speed`. `shimmer` scales the lit color's brightness by `1 + shimmer * noise`. `crust` modulates the emission with dark crust and thin bright seams where the noise crosses zero, normalized so the multiplier averages one and the mean glow is kept. Both fade out with a smoothstep from 2 to 6 screen pixels per feature, so zoomed-out fluid does not sparkle.

Rendering shades the cell raster, not individual particle sprites; edges follow classified cells. Ring lookup reads physical cells; samples outside the region contribute no mass. The 16-row depth search stops at any empty classified cell, regardless of material changes. Scratch buffers grow but do not shrink, and pipelines are retained per target format. See [fluid coupling](@/internals/simulation/fluid-coupling.md#raster-bridge) for how raster mass and material selection are produced.

Fluid appearance applies amount color/radiance transforms before temperature transforms, uses radiance accumulated in the fluid raster, and samples the lighting texture. The raster cell has eight `u32` words: four existing color sums, three emission sums, and an occlusion sum. Fluid heat records also supply render-time temperature colors. See [lighting pass](@/internals/rendering/lighting-pass.md).
