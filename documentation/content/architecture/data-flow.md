+++
title = "Data flow end to end"
description = "How world cells and input move through streaming, simulation, and rendering."
weight = 30
[extra]
sources = ["engine_world/src/scenes", "engine_world/src/materials/material_table.rs", "engine/src/games/game_instance.rs", "engine_host/src/windows/window.rs", "engine_graphics/src/renders/scene_renderer.rs", "engine_user_interface/src/user_interface_renderer.rs", "engine_graphics/src/world/scenes", "engine_host/src/system_events"]
+++

This page traces two paths through the engine: a world cell from generation to pixels and back to disk, and keyboard input to a rendered frame. It is for contributors who need the big picture before reading the subsystem pages.

The scene connects CPU-owned chunk data to GPU-resident cellular tiles. Streaming copies tile properties and gas into the [tile ring](@/architecture/glossary.md#world-terms), simulation updates them, and graphics reads material identifiers, colors, and gas. Tile readback returns the current state to chunks for persistence. It normally happens as tiles leave the [active region](@/architecture/glossary.md#world-terms). `Scene::save` and scene teardown request readback of all active tiles.

## Overview

World cells:

```text
generator ──> Chunk (CPU) <──── load / save ────> chunk file

Chunk (CPU) ──── copy in ────> tile ring (GPU) ──> scene renderer ──> pixels
tile ring (GPU) ──── tile readback (on leave / save) ────> Chunk (CPU)
edit writes ──> tile ring (GPU)
simulation ──> tile ring (GPU)
```

Input:

```text
keyboard ──> held keys ──> possessed pawn control ──> simulation
camera position ──> scene graphics ──> scene renderer ──> UI ──> present
```

## A cell through the world

### Generation and storage

When no chunk file exists, a chunk-I/O worker asks the scene generator to create the chunk. The worker adjusts generated cell colors through `MaterialTable::cellular_color`; see [material table compilation](@/internals/simulation/material-table.md#color-lookup) for the albedo and tint behavior. Loaded or generated `Chunk` data stays CPU-side, because chunks are the persistence unit. See [chunk residency](@/internals/world-streaming/chunk-residency.md), [chunk I/O](@/internals/world-streaming/chunk-io.md), and [chunk format](@/internals/world-streaming/chunk-format.md).

### Tiles in the ring

- **Entering tiles:** once all chunks for entering tiles are active, tile streaming copies each tile's material identifiers, colors, kinematics, and gas into its physical slot in the GPU tile ring.
- **Simulation:** the scene simulates dynamic cells and gas in the ring. The [cellular tile pass](@/internals/simulation/cellular-kinematics.md) and [collision world](@/internals/simulation/collision-world.md) describe the two cellular simulation branches; [gas simulation](@/internals/simulation/gases.md) describes gas.
- **Rendering:** the [scene renderer](@/internals/rendering/scene-renderer.md) samples material identifiers and colors using the same logical-region and ring-offset mapping.
- **Leaving tiles:** for a tile leaving the active region, a GPU buffer copy and an asynchronous map return the material identifier, color, kinematics, and gas arrays to the chunk. Readbacks are applied in request order, and the destination chunk region stays retained while the request is in flight.

### Edits and saving

Edits target active tiles and are queued for the next scene stream. The edit pass writes them to GPU buffers; later tile readback or scene teardown transfers the tile state back to CPU chunks. Saving requests chunk persistence after tile data has been returned, and chunk workers write the chunk files. See [tile streaming](@/internals/world-streaming/tile-streaming.md), [edit pipeline](@/internals/simulation/edit-pipeline.md), and [saving and lifetimes](@/architecture/resources-and-lifetimes.md).

## Input to rendered output

### Input to simulation

The host translates keyboard events and `GameInstance` tracks held key codes. When the game has a scene, before each `Game::update`, `GameInstance` writes the derived movement, jump, and crouch values to possessed actors' pawn-control components. The game update can make other state changes. Scene streaming then follows its camera actor, and simulation advances cells and actors. See [pawn movement](@/internals/actors/pawn-movement.md) and [frame lifecycle](@/architecture/frame-lifecycle.md).

### Simulation to pixels

`Scene::graphics` collects actor sprites relative to the camera, and `Scene::cellular_graphics` exposes the active tile buffers and ring metadata. `GameInstance` passes that scene representation to the renderer, then composes the UI over the scene before the host presents the frame.

Rendered pixels therefore reflect scene state at redraw time, and redraw scheduling is independent of fixed updates. See the [scene renderer](@/internals/rendering/scene-renderer.md) for render-pass details.

## A fluid particle through the world

```text
fluid edit / chunk record -> particle pool (GPU)
  particle solver -> mass/momentum raster -> fluid shading -> pixels
  raster <-> cellular contacts -> collision / body response
  pool -> outgoing export or resident snapshot -> Chunk (CPU) -> chunk file
```

Particle admission and saved copies have separate ownership rules from cellular tile arrays. See [fluid persistence](@/internals/world-streaming/fluid-streaming.md) and [system interactions](@/internals/simulation/fluid-coupling.md).

## Gas through the world

```text
gas edit / chunk gas tile -> tile ring gas (GPU)
  gas solver (cells, fluid raster, actors close cells) -> tile ring gas -> gas shading -> pixels
  tile readback -> Chunk gas tile (only when it differs from ambient phantom air) -> chunk file
```

Gas lives in the tile ring with the cells and returns to chunks with them. See [gas simulation](@/internals/simulation/gases.md), [gas rendering](@/internals/rendering/gas-pass.md), and [chunk format](@/internals/world-streaming/chunk-format.md#gas-tile-section).
