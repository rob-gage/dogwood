+++
title = "Gas render pass"
description = "Cell-aligned gas cloud drawing and compositing."
weight = 36
[extra]
sources = ["engine_graphics/src/renders/scene_renderer_gas_pass.rs", "engine_graphics/src/renders/scene_renderer_gas.wgsl", "engine_graphics/src/world/scenes/scene_graphics_gas.rs", "engine_graphics/src/renders/scene_renderer.rs", "engine_world/src/scenes/scene.rs", "engine_graphics/src/renders/scene_renderer_lighting_pass.rs", "engine_graphics/src/renders/scene_renderer_lighting.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_cascades.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_mips.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_optical.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_spread.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_sprites.wgsl"]
+++

`SceneRendererGasPass` draws the gas held in resident cellular tiles. `Scene::gas_graphics` supplies borrowed gas, material identifier, and gas material-property buffers, camera-relative region placement, ring offsets, tile size, and `GAS_AMOUNT_PER_ATMOSPHERE`. [Scene renderer](@/internals/rendering/scene-renderer.md#pass-order) defines compositing order, and the [tile ring](@/internals/world-streaming/tile-ring.md) defines the gas cell layout.

## Preparation and resources

Preparation caches render pipelines by target format, uploads a 64-byte uniform record, and rebuilds the bind group when any borrowed buffer changes. It has no compute stage or scratch buffers.

| Group 0 binding | Resource |
| --- | --- |
| 0 | `GasUniforms` |
| 1 | Read-only material identifiers (`array<u32>`) |
| 2 | Read-only gas cells (`array<vec4<u32>>`, two per cell) |
| 3 | Read-only gas material records (`array<vec4<f32>>`) |

Uniform offsets are viewport origin 0, viewport size 8, zoom 16, camera-relative region 24, region dimensions 32/36, ring offsets 40/44, tile size 48, and amount per atmosphere 52; other words are padding.

## Cell color

A cell's color is empty outside the region or when its material identifier is not empty. Otherwise each nonempty species slot contributes its material color, converted from sRGB to linear light. A slot holding `n` atmospheres has opacity `1 - (1 - a)^n`, where `a` is the material alpha, so alpha is the opacity of one atmosphere. Slots composite front to back in slot order into premultiplied color. Phantom air is not drawn.

## Drawing

The fullscreen triangle's fragment stage maps each pixel to its logical cell. It discards the pixel unless that cell's opacity exceeds `0.02`, so cloud edges follow the cell grid. Inside, it blends bilinearly between the four nearest cell centers, weighing only cells above that opacity and normalizing by their weights, so edge cells keep their own color up to the edge. The draw uses premultiplied-alpha blending, between fluid and cellular matter.

> **Limitation:** Gas is drawn per cell from the current gas buffer; it is not lit, shaded by depth, or reconstructed below cell resolution.

Gas appearance uses material-wide radiance and samples the lighting texture. Gas material record 1 stores packed radiance in `.y`; see [lighting pass](@/internals/rendering/lighting-pass.md).
