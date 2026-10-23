+++
title = "Scene update pipeline"
description = "The ordered streaming and simulation stages performed by a scene update."
weight = 20
[extra]
sources = ["engine_world/src/scenes/fluids", "engine_world/src/scenes/gases", "engine_world/src/scenes/scene.rs", "engine_world/src/scenes/chunks/chunk_streaming_state.rs", "engine_world/src/scenes/edits/edit_streaming_state.rs", "engine_world/src/scenes/cellular_tiles/cellular_tile_streaming_state.rs", "engine_world/src/scenes/cellular_tiles/cellular_tiles.rs", "engine_world/src/scenes/cellular_tiles/cellular_tile_pass.rs", "engine_world/src/scenes/collisions/collision.rs", "engine_world/src/scenes/collisions/collision_cellular_tile_pass.rs", "engine_world/src/scenes/collisions/collision_rigid_body.wgsl"]
+++

This page defines the ordered work inside `Scene::stream` and `Scene::simulate`. The [frame lifecycle](@/architecture/frame-lifecycle.md) calls both after `Game::update`; rendering is a separate redraw operation.

```text
Scene::stream
  request camera region; initialize chunk, tile, and particle storage
  return rejected imports; apply completed resident particle snapshot
  start requested particle snapshot when imports permit
  write queued cellular edits
  advance chunk I/O; release outgoing rigid bodies
  apply tile/export responses; advance save / ring / dormant imports

Scene::simulate (when an active region exists)
  cellular kinematics using prior body state and prior fluid raster
  poll completed fluid actor feedback
  collision / pawn movement / Rapier step / rigid-body rasterization
  fluid particles using current tile geometry and shared actor records
  gas using current tile geometry, the new fluid raster, and shared actor records
  mark outgoing particle export submitted

Scene::graphics -> renderer compute shading -> scene draw -> UI -> present
```

## Streaming order

The camera requests a region twice its unzoomed dimensions. The first stream creates chunk state, the GPU tile ring, the fixed fluid pool, and tile streaming. Later requests must keep the same dimensions. Missing incoming chunks leave the previous active region in place.

Completed rejected imports return to their source chunks before a resident snapshot can replace saved copies. A snapshot request retains the pool's region and waits for pending imports. [Fluid streaming](@/internals/world-streaming/fluid-streaming.md) defines ownership, capacity reservations, and save gating.

Queued cellular edits are written before slot reuse; fluid effects remain queued for particle simulation. If edits write tiles, existing collision state requests a cell reread. Chunk streaming next applies completed I/O and schedules more work.

Before tile movement, collision releases rigid bodies extending outside the requested region. Their rasterized cells remain in the departing tiles. Tile streaming applies completed cellular/particle responses, advances save phases, and attempts ring movement. A shift copies outgoing tile arrays before incoming writes reuse slots; overlapping tiles retain their physical slots. It then queues eligible dormant particle imports.

Ring movement can be deferred by chunk readiness, readback capacity, resident saving, pending imports, an unsubmitted particle export, or an earlier return intersecting incoming tiles. See [tile streaming](@/internals/world-streaming/tile-streaming.md) and [fluid handoffs](@/internals/world-streaming/fluid-streaming.md#region-movement-and-exports).

## Simulation order

### Cellular tile simulation

Before the first active region, simulation returns immediately. Otherwise collision and cellular tile simulation are created lazily. `CellularTiles` receives tile buffers, ring placement, collision-relative region origin, gravity, eligible actor colliders, rigid-body records, and the shared fluid raster.

`CellularTilePass` advances cellular matter and computes fluid/cell and fluid/rigid-body contacts against the prior fluid submission's raster. It uploads actor records based on pre-collision positions and inferred displacement since the preceding tick. Completed earlier contact/control readbacks are consumed without waiting. See [cellular kinematics](@/internals/simulation/cellular-kinematics.md) and [fluid coupling](@/internals/simulation/fluid-coupling.md).

### Collision

The scene polls fluid actor feedback with the same actor identities/settings, then passes it and kinematics contacts to collision. Collision synchronizes actors, applies responses, controls walking/swimming pawns, steps Rapier, and rasterizes cellular rigid bodies.

Static occupancy refreshes block when needed. Settled dynamic occupancy for rigid bodies uses completed asynchronous snapshots, falling back to a blocking read when no held snapshot matches the region/ring offset. See [collision ordering](@/internals/simulation/collision-world.md) and [occupancy readback](@/internals/simulation/static-cell-colliders.md#occupancy-readback).

### Fluid step

The particle solver runs after the collision submission, so queue order exposes updated cellular and rasterized rigid-body geometry. Actor records are copied from `CellularTiles` rather than uploaded again after collision; moving actor projection extrapolates those records through substeps.

Fluid simulation consumes queued edits/imports and raster contact deltas, rebases and exports when needed, solves particles, rebuilds the raster, and queues actor/count feedback. The scene marks an outgoing export submitted only on success. [Fluid simulation](@/internals/simulation/fluids.md) gives the solver stages; [GPU synchronization](@/architecture/concurrency-and-gpu-sync.md) describes readback waits.

### Gas step

Gas runs last, after the particle solver rebuilds the raster, so it sees the tick's cells, fluid, and cellular tile actor records. It is created on its first step and submits its passes without waiting or reading back. [Gas simulation](@/internals/simulation/gases.md) gives the passes.

## Drawing

Graphics borrows cellular buffers, fluid raster/materials, and actor sprites. Fluid classification and shading run before the scene render pass; [scene rendering](@/internals/rendering/scene-renderer.md#pass-order) defines compositing order. Rendering does not advance particle simulation, and CPU body reactions can lag the raster shown on screen.
