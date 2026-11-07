+++
title = "Lighting"
description = "Radiance data, scene light settings, and actor lights."
weight = 95
[extra]
sources = ["engine_graphics/src/world/materials/material_radiance.rs", "engine_graphics/src/world/scenes/lighting_quality.rs", "engine_graphics/src/world/scenes/scene_graphics_lighting.rs", "engine_graphics/src/world/materials/material_radiance.wgsl", "engine_graphics/src/sprites/sprite_sheet.rs", "engine_graphics/src/sprites/sprite_sheet_error.rs", "engine_world/src/materials/material_properties_fluid.rs", "engine_world/src/materials/material_properties_chemical.rs", "engine_world/src/materials/material_amount_color.rs", "engine_graphics/src/world/materials/material_graphics_gas.rs", "engine_world/src/materials/material_transition.rs", "engine_world/src/materials/material_temperature_color.rs", "engine_world/src/materials/material_properties_thermal.rs", "engine_world/src/materials/material_table.rs", "engine_world/src/scenes/scene.rs", "engine_world/src/scenes/scene_error.rs", "engine_world/src/actors/components/actor_light.rs", "engine_world/src/actors/components/actor_sprites.rs", "engine_world/src/actors/components/actor_shader.rs", "engine_world/src/actors/components/mod.rs", "engine/src/lib.rs", "template/src/template_game.rs", "template/assets/sprites/glowstone_red.png", "template/assets/sprites/glowstone_green.png", "template/assets/sprites/glowstone_blue.png", "template/assets/sprites/glowstone_cyan.png", "template/assets/sprites/glowstone_magenta.png", "template/assets/sprites/glowstone_yellow.png", "template/assets/sprites/glowstone_gravel_red.png", "template/assets/sprites/glowstone_gravel_green.png", "template/assets/sprites/glowstone_gravel_blue.png", "template/assets/sprites/glowstone_gravel_cyan.png", "template/assets/sprites/glowstone_gravel_magenta.png", "template/assets/sprites/glowstone_gravel_yellow.png"]
+++

Lighting data uses linear HDR emission RGB and occlusion. Occlusion is the fraction of light blocked by one tile thickness; emission is per cell of light path. See [lighting transport](@/internals/rendering/lighting-pass.md) for how the renderer uses these values.

## Radiance

`pack_radiance` packs emission and occlusion into a `u32`; the engine facade re-exports it. Emission channels are clamped to `0..=127`, and occlusion to `0..=1`. Cells and fluid particles carry packed radiance alongside color. Gas radiance is a material property. Default cellular radiance is sampled from the material's `edit_radiance` sprite frame and scaled by `edit_radiance_intensity`; fluid and gas use their material radiance properties. `u32::MAX` in edit and fill APIs requests the material value.

Sprites use a separate radiance `SpriteSheet`. Its sRGB-decoded RGB multiplied by the sprite's
radiance intensity is emission per cell along the path; alpha is occlusion per tile. The sampled
radiance frame is `frame % radiance_sheet.frame_count()`, independent of the albedo frame count.
A common nonemissive, transparent choice is `SpriteSheet::from_color(accelerator, 0, 0, 0, 0)`.

## Scene and actor controls

`LightingQuality` controls radiance cascade resolution. `Low` uses one texel for each 4 × 4 cells, `Medium` for each 2 × 2 cells, and `High` for each cell. `High` is the default. Backlight uses one texel per 4 × 4 world cells at every quality. `Scene::lighting_quality()` returns the setting and `set_lighting_quality` changes it.

`Scene::ambient_light()` returns the linear RGB ambient floor. It defaults to `[1, 1, 1]`; `set_ambient_light` accepts finite nonnegative channels and reports `SceneError::InvalidAmbientLight` otherwise. `light_penetration()` defaults to `8` tiles; `set_light_penetration` accepts finite values of at least `1` and reports `InvalidLightPenetration` otherwise.

`ActorLightComponent` has linear RGB `color`, `intensity`, and `radius` in tiles. Its default is white, intensity `1`, radius `0.5`; it requires `ActorKinematicsPositionComponent`. Invalid (nonfinite or negative) values are ignored by graphics collection. Actor sprite radiance and light discs are rasterized with texel coverage so their contribution remains stable across lighting qualities; see [lighting transport](@/internals/rendering/lighting-pass.md).

`ActorShaderComponent` can give the scene light and stop it through `ShadeOutput`: its `radiance` lights the scene like material radiance, and its `occlusion` stops light like material radiance alpha. Both are zero unless the shader sets them; `emission` only changes how the body looks. `shade()` runs once per world cell of a shaded body at every lighting quality, so it must not use derivatives. See [actor components](@/external-api/actor-components.md).

Parallax actors use their radiance sheets to emit background light and receive gameplay light with a depth-dependent blur and attenuation. This is an approximation described in [lighting transport](@/internals/rendering/lighting-pass.md).

## Template example

The template sets ambient light to `[0.75, 0.78, 0.85]`. It uses flat radiance sheets for stone
(black, occlusion `0.9`), gravel (`0.85`), ice (`0.15`), and snow (`0.5`). Glowstone uses a flat
color sheet with occlusion `0.4` and intensity `2.0`; its gravel uses the same color and occlusion
with intensity `0.5`. Background and player sprites share a black transparent radiance square.

## Temperature colors

Thermal material keys apply visual color and radiance transforms at render time; they do not change stored cell colors. Amount-color keys apply first for cellular and fluid materials, followed by temperature transforms. Keys must have strictly increasing finite temperatures and zero transform variances. See [materials](@/external-api/materials.md) for the material API.

> **Limitation:** Light transport is two-dimensional in the gameplay plane. Parallax light is a depth-blur approximation; there are no multiple-bounce reflections or normal maps. Only resident cells contribute, and gas radiance is material-wide. Fluid temperature colors apply only where the fluid heat raster has temperature data.

{{ <rustdoc path="engine/world/materials/fn.pack_radiance.html" text="pack_radiance" /> }} · {{ <rustdoc path="engine/actors/components/struct.ActorLightComponent.html" text="ActorLightComponent" /> }}
