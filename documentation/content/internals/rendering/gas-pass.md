+++
title = "Gas render pass"
description = "Smoothly edged gas cloud drawing and compositing."
weight = 36
[extra]
sources = ["engine_graphics/src/renders/scene_renderer_gas_pass.rs", "engine_graphics/src/renders/scene_renderer_gas.wgsl", "engine_graphics/src/world/scenes/scene_graphics_gas.rs", "engine_graphics/src/world/materials/material_graphics_gas.rs", "engine_world/src/materials/material_table.rs", "engine_graphics/src/renders/scene_renderer.rs", "engine_world/src/scenes/scene.rs", "engine_graphics/src/renders/scene_renderer_lighting_pass.rs", "engine_graphics/src/renders/scene_renderer_lighting_clear.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_cache.wgsl", "engine_graphics/src/renders/scene_renderer_lighting.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_cascades.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_mips.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_optical.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_spread.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_sprites.wgsl"]
+++

`SceneRendererGasPass` draws the gas held in resident cellular tiles. `Scene::gas_graphics` supplies borrowed gas, material identifier, gas material-property, and gas material-graphics buffers, camera-relative region placement, ring offsets, tile size, `GAS_AMOUNT_PER_ATMOSPHERE`, and `time`, the scene's simulated time in seconds (`simulated_ticks / 60`, the value the decorations pass uses), so the texture pauses with the game. [Scene renderer](@/internals/rendering/scene-renderer.md#pass-order) defines compositing order, and the [tile ring](@/internals/world-streaming/tile-ring.md) defines the gas cell layout.

## Preparation and resources

Preparation caches render pipelines by target format, uploads an 80-byte uniform record, and rebuilds the bind group when any borrowed buffer or scratch buffer changes. It also sizes the shaded rectangle: the cells under the viewport plus four past each edge, clamped to at most four cells past the region (`-4` to `region_cells + 3`). The B-spline taps reach two cells and the checker filter reads two more. Two scratch buffers of 32 bytes per shaded cell (two `vec4<u32>` records) grow on demand. `SceneRendererGasPass::shade` runs before the render pass, only when gas graphics and the lighting bind group exist, and dispatches `shade` then `cancel_checker` in one compute pass over the shaded rectangle in 8×8 workgroups. Group 1 is the lighting bind group for both compute and fragment stages.

| Group 0 binding | Resource | Stages |
| --- | --- | --- |
| 0 | `GasUniforms` | compute, fragment |
| 1 | Read-only material identifiers (`array<u32>`) | compute |
| 2 | Read-only gas cells (`array<vec4<u32>>`, two per cell) | compute |
| 3 | Read-only gas material records (`array<vec4<f32>>`) | compute |
| 4 | Read-only matter (`array<vec2<u32>>`, temperature bits then amount) | compute |
| 5 | `shaded_cells`, read-write, two `vec4<u32>` per cell (`array<vec4<u32>>`) | compute |
| 6 | `filtered_cells`, read-write, two `vec4<u32>` per cell (`array<vec4<u32>>`) | compute, fragment |
| 7 | Read-only `gas_graphics`: one `vec4<f32>` per gas in gas index order, `[texture, texture_scale, drift, flicker]` (`array<vec4<f32>>`) | compute |

Uniform offsets are viewport origin 0, viewport size 8, zoom 16, camera-relative region 24, region dimensions 32/36, ring offsets 40/44, tile size 48, amount per atmosphere 52, shaded origin (`vec2<i32>`) 56, and shaded size (`vec2<u32>`) 64, and `time` (`f32`) 72; other words are padding.

## Cell color

`shade` computes two packed records per shaded cell: the premultiplied color (two half-float pairs) and the cell's texture parameters, storing zero where no gas is visible. A cell's color is empty outside the region or when its material identifier is not empty. Otherwise every nonempty species slot adds an optical depth `-ln(1 - a) * n * opacity`, where `a` is the temperature-adjusted material alpha (clamped so the depth stays finite), `n` the atmospheres in the slot, and `opacity` the material's opacity. The cell's opacity is `1 - exp(-total depth)` and its color is the linear-light slot colors weighted by optical depth, multiplied by the light at the cell, plus the glow radiance of each slot (`min(n, 1)` times its radiance). A cell is visible when its opacity or any color channel exceeds `0.02`. Phantom air is not drawn. The texture parameters stored in the second record are the per-gas `gas_graphics` vec4s averaged by how much each slot shows, through opacity or glow. The lighting optics never see the texture; they use these per-cell averages of density and emission.

## Checker cancelling

`cancel_checker` reads `shaded_cells` and writes `filtered_cells`. The pressure solver can leave its 2×2 pressure cells alternating, so each visible cell is mixed half-and-half with the average of the visible cells two cells away on each side along both axes. Invisible cells stay zero, and visible results never pack to zero.

## Drawing

The fullscreen triangle's fragment stage maps each pixel to a continuous cell position and evaluates a uniform cubic B-spline over the 4×4 cells around it, counting cells without visible gas as empty. The color and glow are not renormalized, so a cloud's edge fades smoothly over about a cell and a half instead of following the cell grid; this hides the 2×2 steps the pressure solver leaves. The texture parameters are normalized over the visible cells only. Pixels with no gas weight, or with color and glow below a tenth of `0.02`, are discarded. The draw uses premultiplied-alpha blending, between fluid and cellular matter.

## Texturing

The fragment stage multiplies the interpolated cell by a noise texture. The noise is three octaves of value noise anchored to world cell coordinates, divided by `texture_scale` cells per feature, so it does not swim with the camera. It averages zero, rises at `drift` cells per second, and churns slowly sideways using `time`. `texture` scales the cell's optical depth by `1 + texture * noise`, and `flicker` scales its emission by `1 + flicker * noise`; both average out so the cloud keeps its mean density and glow. The effect fades out with a smoothstep from 2 to 6 screen pixels per feature, so distant or zoomed-out gas does not sparkle.

Gas appearance uses material-wide radiance and samples the lighting texture. Gas material record 1 stores packed radiance in `.y`; see [lighting pass](@/internals/rendering/lighting-pass.md).
