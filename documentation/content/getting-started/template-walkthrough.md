+++
title = "Template game walkthrough"
description = "Explore the generated sample game, its materials and player actor, and the controls that drive it."
weight = 40
[extra]
sources = ["template/src", "template/assets", "template/Cargo.toml.template", "cli/src/main.rs", "cli/src/commands/run.rs", "cli/src/commands/edit.rs", "cli/src/commands/run_template.rs", "cli/src/commands/edit_template.rs", "cli/src/template/template_cache.rs", "engine/src/games/game_instance.rs", "engine_world/src/actors/components/actor_pawn_movement.rs", "engine_world/src/actors/components/actor_pawn_possession.rs", "engine_world/src/actors/components/actor_camera.rs", "engine_graphics/src/sprites/sprite_sheet.rs"]
+++

This page tours the code of the template game that `dogwood new` generates: its entry points, setup, and controls. Terms such as material form, actor, and pawn are defined in [core concepts](@/getting-started/core-concepts.md).

To try it without creating a project in the current directory, run `dogwood run-template` or `dogwood edit-template` (see the [CLI reference](@/external-api/cli.md)).

## Entry points

- `src/main.rs` creates the default [`Engine`](@/external-api/engine-and-games.md), passes its [`Accelerator`](@/external-api/host.md)—the GPU resource used to load sprite sheets and compile materials—to `TemplateGame::new`, then launches the game.
- `src/lib.rs` exposes `TemplateGame` from the project library crate.
- `src/bin/editor.rs` follows the same setup and calls [`Editor::launch`](@/external-api/editor.md); the binary is available when the `editor` feature is enabled.

## Game setup

`TemplateGame::new` registers four materials, compiles them into a material table, then creates the scene and the player.

### Materials

- `gravel` is cellular dynamic, with mass, friction, and restitution properties.
- `stone` is cellular static, uses `gravel` as its debris material, and has strength `10` atmospheres with absorption `0.1`.
- `water` is fluid, with density `0.6`, viscosity `0.05`, cohesion `0.2`, artificial pressure `0.001`, drag `1.0`, and zero friction and restitution. Its packed RGBA color is `[40, 110, 200, 200]`. It is available for editor painting; the generator does not place water.
- `water vapor` is gas, with mass `18.0`, diffusivity `0.04`, and cohesion `0.1`, so it rises through the default ambient gas mass of `29.0`. Its packed RGBA color is `[225, 230, 240, 150]`. It is available for editor painting; the generator does not place gas.
- Their sprite sheets come from `assets/sprites/gravel.png` and `assets/sprites/stone.png`, each loaded as a one-frame sheet.

See [defining materials](@/examples/materials-and-sprites.md) for details.

### World

The game creates a `Scene` with seed `0` and chunk storage in the project's `world` directory. The scene uses a generator to create chunk data when no saved chunk exists. The template generator ignores the world seed: it fills chunks below Y 0 with stone and leaves chunks at Y ≥ 0 empty. See [generating worlds](@/examples/scene-generators.md) for how generators work.

> **Limitation:** `TemplateGame::new` deletes the `world` directory at startup, so edits do not survive a relaunch. Deletion errors are ignored; if deletion fails, previously saved chunks may be reused.

### Player

The player actor has these components (see [actors and pawns](@/examples/actors-and-pawns.md)):

- Starts at scene tile `(0, 4)`.
- Default pawn movement with keyboard possession.
- A circular kinematic collider.
- A white square sprite.
- An `80 x 45` cellular-tile camera.
- A persistence marker.

## Controls

| Action | Controls |
|---|---|
| Move horizontally | `A`/`D` or Left/Right Arrow |
| Jump | `W` or Up Arrow |
| Crouch | `S` or Down Arrow |
| Zoom in | `+` (Equal or Numpad Add) |
| Zoom out | `-` (Minus or Numpad Subtract) |

Zoom is clamped to 0.25–2.0 and scales the visible view; the camera's base dimensions stay `80 x 45` tiles.

Continue with [guides and examples](@/examples/_index.md) for individual engine tasks.

## Parallax backgrounds

The scene spawns a sky actor and three landscape actors with `ActorParallaxComponent`: far, middle, and near rough stone slopes at depths 24, 8, and 2 tiles. Each landscape actor holds one sprite from `assets/sprites/parallax_*.png`, repeated horizontally using `ParallaxRepeat::Mirror`. The taller textures place the surface near their center and extend dark background stone below it. Layer centers are offset by 2, 1, and 0.5 projected tiles respectively; the near layer reaches about 26 tiles underground, with farther layers extending deeper. The artwork uses modest pixel-art facets, sparse cracks and chips: cool blue-gray far slopes, purple-gray middle slopes, and warm gray near slopes. It contains no trees or vegetation. Source pixel clusters are enlarged by the sprite scale and sampled with nearest filtering. Sprite scales set the world size independently of depth; the far, middle, and near layers are 120, 112, and 80 tiles wide after projection. The far, middle, and near vertical scroll factors are 0.1, 0.25, and 0.5 respectively; horizontal factors are 1.0. The sky is a separate large, finite blue sprite at depth 10,000 tiles, with no tiling or scroll overrides; it is reserved for a future ambient radiance texture.

Move horizontally or zoom with `+` and `-` to compare the layers. These actors are recreated at startup. See [parallax backgrounds](@/external-api/scenes.md#parallax-backgrounds) for the API and independent scrolling controls.
