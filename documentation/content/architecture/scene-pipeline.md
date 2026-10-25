+++
title = "Scene update pipeline"
description = "The ordered streaming and simulation stages performed by a scene update."
weight = 20
[extra]
sources = ["engine_world/src/scenes/fluids", "engine_world/src/scenes/pressure", "engine_world/src/scenes/scene.rs", "engine_world/src/scenes/chunks/chunk_streaming_state.rs", "engine_world/src/scenes/edits/edit_streaming_state.rs", "engine_world/src/scenes/cellular_tiles/cellular_tile_streaming_state.rs", "engine_world/src/scenes/cellular_tiles/cellular_tiles.rs", "engine_world/src/scenes/cellular_tiles/cellular_tile_pass.rs", "engine_world/src/scenes/collisions/collision.rs", "engine_world/src/scenes/collisions/collision_cellular_tile_pass.rs", "engine_world/src/scenes/collisions/collision_rigid_body.wgsl"]
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

When the active region is resident, `Scene::simulate` runs these steps in order. Pressure consumers read the buffer published during the preceding tick. Contact and pressure-breakage readbacks copied during tick N are consumed starting in tick N+1; waits target only the submission containing each copy. The [synchronization page](@/architecture/concurrency-and-gpu-sync.md) describes waits and feedback latency.

1. **Read cellular feedback.** At the start of `CellularTiles::simulate_cellular_tiles`, `CellularTilePass` reads actor contacts, rigid-body contacts, and control counts from prior submissions. Actor contacts use the newest completed sample; rigid-body contacts accumulate. Control results size this tick's cellular work. Contact reads wait only for their copied submission when needed; control sizing never waits.
2. **Simulate cellular tiles.** GPU stages run in order: `collect`, `clear`, `prepare`, `integrate`; then `prepare` and, for each cellular substep, `request`, `pick`, `resolve_begin`, the configured `resolve_even`/`resolve_odd` chain, `stage`, and `commit`; finally `contact`, `rigid_fluid_faces`, and `rigid_fluid`. `integrate`, `contact`, and `rigid_fluid` read previous published pressure; kinematics and rigid-fluid stages read the previous fluid raster. Contacts are copied for later CPU use. Actor records use pre-collision positions and inferred displacement.
3. **Poll fluid actor feedback.** `Scene` polls completed fluid actor samples using the actor identities and collision settings from their submission before calling collision.
4. **Read pressure breakage.** Collision reads the prior tick's broken-tile and broken-rigid-cell copies, waiting only for those copy submissions. It applies broken tile occupancy, removes broken rigid-body cells, and updates affected voxel colliders. Oversized tile lists request a full occupancy refresh.
5. **Apply collision topology and occupancy.** Collision applies detached-cell results and detached groups whose asynchronous material reads have arrived. Static occupancy is read when region/ring changes or edits request it; edit changed-cell reads and required occupancy reads can block. Settled dynamic occupancy uses completed snapshots, falling back to a blocking read when no matching snapshot exists. Collision updates voxel colliders, releases bodies outside the region, applies cellular and fluid impulses to rigid bodies, and updates actors and pawns using prior contacts.
6. **Step Rapier and rasterize.** Pawn movement and Rapier advance actors and rigid bodies. Collision rasterizes rigid bodies into cellular tile buffers, retaining per-cell stored pressure and integrity through rasterization and splits. It requests detached-cell GPU copies for a later collision step.
7. **Simulate fluid particles.** The solver consumes queued edits/imports and raster-contact deltas, predicts, solves, and updates particles against current cellular and rasterized rigid-body geometry. The first `predict` substep reads previous-tick pressure and applies pressure push. The solver rebuilds the fluid raster and queues actor/count feedback. A result copied in tick N is consumed starting in N+1; actor feedback waits only for its copy submission when a response is needed.
8. **Measure pressure volume.** `PressureCellularTilePass` measures open volume and actor drag. It measures the full ring when the region or ring offset changes; otherwise `volume_collect` lists only tiles that may have changed: tiles whose cellular cells changed or were simulated this tick, tiles awake from edits, streaming, or moving gas, tiles near fluid or dragging actors at the last measurement, and tiles within an actor's drag range.
9. **Collect pressure work.** `collect`, `clear`, and `prepare` build a sparse dispatch for awake pressure tiles and neighbors.
10. **Step pressure.** Twelve fixed `step` substeps update staggered gas/solid face flows and cell state, double-buffered between cellular gas storage and scratch. Solid load also reduces integrity in each substep.
11. **Publish and damage.** `publish` writes gauge atmospheres to the shared pressure buffer and wakes dynamic cells next to pushing pressure. `damage` releases broken-cell pressure as phantom air and records changed tile masks and rigid-body cells.
12. **Request breakage copies.** `Collision::request_broken_cells` copies the new broken-tile and broken-rigid-cell lists without waiting. Collision reads them next tick, so damage topology is applied incrementally one tick later. The scene marks a particle export submitted only after successful fluid simulation.

## Ownership

| Interaction | Owner |
| --- | --- |
| Pressure storage and transmission through open cells and same-owner solids | Pressure substrate |
| Pressure load, absorption, relaxation, integrity, and breakage | Pressure substrate |
| Pressure pushes on dynamic cells, actors, rigid bodies, and fluid particles | Their consumer passes (`integrate`, `contact`, `rigid_fluid`, and first `predict` substep) |
| Gravel with gravel and gravel with fluid | Cellular tile GPU pass |
| Fluid particle pressure response | Fluid particle GPU pass |
| Rigid body with terrain and rigid body with rigid body | Rapier |
| Rigid bodies with settled grains | Rapier voxel collider |
| Rigid bodies with moving grains | Cellular tile GPU impulses |
| Breakage, detach, and occupancy topology | Collision world, using asynchronous GPU readbacks |

Settled grains support bodies only through the settled voxel collider, and moving grains push them only through GPU impulses; resting grains still pass their weight to a body under them. See [collision world](@/internals/simulation/collision-world.md) and [rigid bodies](@/internals/simulation/rigid-bodies.md).

## Drawing

Graphics borrows cellular buffers, fluid raster/materials, actor sprites, and projected parallax planes. Fluid classification and shading run before the scene render pass; [scene rendering](@/internals/rendering/scene-renderer.md#pass-order) defines compositing order. Rendering does not advance particle simulation, and CPU body reactions can lag the raster shown on screen.
