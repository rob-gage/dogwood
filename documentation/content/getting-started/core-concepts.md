+++
title = "Core concepts"
description = "Understand how games, scenes, cells, actors, and updates fit together in Dogwood."
weight = 30
[extra]
sources = ["engine_world/src/scenes/mod.rs", "engine_world/src/scenes/scene_position.rs", "engine_world/src/scenes/scene.rs", "engine_world/src/scenes/cellular_tiles/mod.rs", "engine_world/src/scenes/chunks/mod.rs", "engine_world/src/scenes/edits/edit_streaming_state.rs", "engine_world/src/materials/material_table.rs", "engine_graphics/src/renders/scene_renderer_cellular.wgsl", "engine/src/games/game.rs", "engine/src/games/game_instance.rs", "engine_world/src/actors/actor.rs", "engine_world/src/actors/actor_registry.rs", "engine_world/src/actors/components", "engine_world/src/materials/material.rs", "engine_world/src/materials/material_registry.rs", "engine_world/src/scenes/kinematics/kinematics_pass.rs", "engine/src/lib.rs", "engine_world/src/scenes/kinematics/kinematics_cellular_tile.wgsl", "engine/src/runtime/engine.rs"]
+++

This page gives game programmers the short mental model of Dogwood: games, scenes, the cell grid, materials, actors, and the update loop. Each section links to the page that covers it in depth.

## Game and scene

An `Engine` runs a game that implements `Game`. The game supplies an optional `Scene` and identifies one of its actors as the camera. Dogwood uses that camera to stream and draw the scene by default.

The scene groups the world's actors, materials, generator, and chunk storage. See [Engine and Game](@/external-api/engine-and-games.md) and [Scenes](@/external-api/scenes.md) for API details.

## World scale

The world is a grid with three nested units:

| Unit | Size | Role |
|---|---|---|
| cell | 1 grid square | smallest simulated unit |
| cellular tile | 8 x 8 cells | unit moved to and from the GPU |
| chunk | 64 x 64 tiles (512 x 512 cells) | CPU-side storage and persistence unit |

See [Coordinates, tiles, and chunks](@/external-api/world-storage.md) and the [coordinate glossary](@/architecture/glossary.md) for coordinate details.

The **tile ring** is a GPU buffer holding tiles near the camera. The **active region** is the rectangle of tiles currently in that buffer; only those tiles are simulated. `Scene::stream` requests tiles around the camera. The active region moves after the needed chunk data loads. See the [scene pipeline](@/architecture/scene-pipeline.md) for the order.

## Materials, actors, and pawns

### Materials

A **material** defines how a kind of cell behaves and looks. A game registers materials in a `MaterialRegistry` and compiles them into a `MaterialTable`, which it passes to the scene. A material has one form:

- **cellular static:** static cells that the dynamic-cell simulation does not move, such as stone.
- **cellular dynamic:** cells moved by the kinematics simulation, such as gravel, which falls and piles.
- **fluid** or **gas.**

See [Defining materials](@/examples/materials-and-sprites.md) for setup.

Fluid materials use [GPU particles](@/internals/simulation/fluids.md) separate from cellular slots and render from a cell raster.

> **Not implemented:** Gas simulation and form-specific rendering.

### Actors and pawns

An [**actor**](@/external-api/actors.md) is a scene-local identity for a game entity. Components hold data such as position, camera, or collision properties. The actor used as the camera needs camera and position components.

A **pawn** is an actor with pawn movement. A possessed pawn also has `ActorPawnPossessionComponent`, so the engine supplies keyboard control. See [Actors, cameras, and pawns](@/examples/actors-and-pawns.md) for a working example.

## Fixed update and frame

Dogwood advances game logic in fixed updates at 60 Hz. During an update, the engine supplies control to possessed pawns, calls `Game::update`, then streams and simulates the scene.

Rendering and presentation happen separately per frame, so a frame is not the same as one simulation step. Read [Frame and update lifecycle](@/architecture/frame-lifecycle.md) for the full sequence.

Next, see these ideas in code in the [template walkthrough](@/getting-started/template-walkthrough.md).
