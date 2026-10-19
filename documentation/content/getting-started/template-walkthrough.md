+++
title = "Template game walkthrough"
description = "Explore the generated sample game, its materials and player actor, and the controls that drive it."
weight = 40
[extra]
sources = ["template/src", "template/assets", "template/Cargo.toml", "cli/src/main.rs", "cli/src/commands/run.rs", "cli/src/commands/edit.rs", "cli/src/commands/run_template.rs", "cli/src/commands/edit_template.rs", "cli/src/template/template_cache.rs", "engine/src/games/game_instance.rs", "engine_world/src/actors/components/actor_pawn_movement.rs", "engine_world/src/actors/components/actor_pawn_possession.rs", "engine_world/src/actors/components/actor_camera.rs", "engine_graphics/src/sprites/sprite_sheet.rs"]
+++

This page tours the code of the template game that `dogwood new` generates: its entry points, setup, and controls. Terms such as material form, actor, and pawn are defined in [core concepts](@/getting-started/core-concepts.md).

To try it without creating a project in the current directory, run `dogwood run-template` or `dogwood edit-template` (see the [CLI reference](@/external-api/cli.md)).

## Entry points

- `src/main.rs` creates the default [`Engine`](@/external-api/engine-and-games.md), passes its [`Accelerator`](@/external-api/host.md)—the GPU resource used to load sprite sheets and compile materials—to `TemplateGame::new`, then launches the game.
- `src/lib.rs` exposes `TemplateGame` from the project library crate.
- `src/bin/editor.rs` follows the same setup and calls [`Editor::launch`](@/external-api/editor.md); the binary is available when the `editor` feature is enabled.

## Game setup

`TemplateGame::new` registers two materials, compiles them into a material table, then creates the scene and the player.

### Materials

- `gravel` is cellular dynamic, with mass, friction, and restitution properties.
- `stone` is cellular static and uses `gravel` as its debris material.
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
