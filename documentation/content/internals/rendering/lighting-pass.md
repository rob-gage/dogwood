+++
title = "Lighting pass"
description = "Optical field construction, radiance cascades, spread, and appearance inputs."
weight = 37
[extra]
sources = ["engine_graphics/src/renders/scene_renderer_lighting_pass.rs", "engine_graphics/src/renders/scene_renderer_lighting_blocks.wgsl", "engine_graphics/src/renders/scene_renderer_lighting.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_cascades.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_mips.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_optical.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_spread.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_sprites.wgsl", "engine_graphics/src/world/materials/material_radiance.wgsl", "engine_graphics/src/world/scenes/scene_graphics_lighting.rs", "engine_graphics/src/world/scenes/scene_graphics_parallax.rs", "engine_graphics/src/world/scenes/scene_graphics_sprites.rs"]
+++

`SceneRendererLightingPass` computes one light texture per rendered view over the whole resident cellular region, at one texel per cell. The scene renderer runs it before fluid shading and the main render pass. Public radiance data and scene controls are described on [Lighting](@/external-api/lighting.md).

## Stages

1. Actor optics raster adds gameplay sprite emission multiplied by sprite tint and intensity and multiplies transmission for each covered cell. `ActorLightComponent` adds a disc emitter.
2. Backlight raster composites parallax backgrounds far to near at quarter resolution. Radiance-map mip level increases with physical depth.
3. Optical-field construction combines cellular and fluid radiance, gas species and atmospheres, actor optics, and temperature transforms. Per-cell extinction is `-ln(1 - occlusion)` per tile, divided by cells per tile and capped at `30`.
4. An 8 × 8 workgroup classifies uniform optical blocks; cascade rays cross a uniform block in one step. Radiance cascades otherwise trace rays with grid DDA and Beer-Lambert transmission. Level `i` probes every `2^i` cells, uses `4 * 2^i` directions, and covers `[2(2^i - 1), 2(2^(i+1) - 1)]` cells. Levels extend until the domain diagonal is covered. The coarsest level runs first; each interval combines its trace with rays from surrounding coarser probes. A ray is used per coarser probe only when its plain ray crosses matter or light; otherwise the plain ray and interpolation suffice. The finest interval is integrated at every cell into fluence rather than stored as a level.
5. Three separable max-scan spread passes (rows, columns, rows) move light into occluding cells, attenuated by `exp(-extinction / penetration)`. Spread buffers pack RGB as two `pack2x16float` words. Backlight enters the first pass and lights portions not hidden by a tile thickness.
6. Box-downsampled light mips support appearance shading.

For a normal appearance pass, lit color is `albedo * (ambient + light) + emission`. Pressure and temperature view tints and overlays remain unlit. With no resident region, appearance uses ambient only. Parallax planes sample by `log2(depth in cells)` and scale gameplay light by the fraction of the emitter kernel inside the domain.

## Resources and layouts

The optical field and light textures use `Rgba16Float`; the backlight texture is quarter resolution. Temperature keys arrive in `SceneGraphicsLighting.temperature_color_buffer`; its records have a `vec4<u32>` header per thermal material followed by eleven `vec4<f32>` values per key. `SceneGraphicsLighting` also carries ambient, penetration, thermal offsets, and collected lights. Sprite instances carry an 80-byte record: radiance region starts at byte 56 and intensity at byte 72. `SceneGraphicsSprites` entries contain 12 values, including separate albedo and radiance views, independent regions, color, and intensity. Parallax records carry `radiance_texture_region` and `radiance_intensity` alongside their radiance view. The lighting raster uses a 64-byte instance stride.

Appearance bind groups expose a view uniform, the sampled `Rgba16Float` light texture, and its filtering sampler; cellular, fluid, gas, and sprite passes each append this lighting group after their local bindings. Optical construction reads cell/material/gas/fluid/temperature data plus actor optics and writes an `Rgba16Float` field. Cascade dispatches use two ping-pong interval buffers with per-level dynamic uniforms; spread dispatches use three ping-pong storage-texture bindings and dynamic uniforms; mip dispatches bind adjacent source and destination mip views.

The pass uses a 256-byte dynamic-uniform stride for cascade and spread records. Cascade uniforms are allocated for up to 16 levels. The private tuning constants are `LIGHTING_INTERVAL = 2`, `LIGHTING_DIRECTIONS = 4`, `LIGHTING_BACKLIGHT_CELLS = 4`, and `LIGHTING_LEVEL_MAXIMUM = 16`. The shared radiance WGSL packs three 7-bit mantissas, a shared 4-bit exponent, and 7-bit occlusion into a `u32`; see [resource constants](@/architecture/resources-and-lifetimes.md).

> **Limitation:** There is no multiple-bounce reflection; spread is the only indirect term. Penetration follows rows and columns and is slightly shorter diagonally. Only the resident domain contributes, so distant lights do not reach the view. Parallax transport is approximate and backgrounds cover only visible copies.
