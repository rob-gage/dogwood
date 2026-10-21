+++
title = "Scene update pipeline"
description = "The ordered streaming and simulation stages performed by a scene update."
weight = 20
[extra]
sources = ["engine_world/src/scenes/scene.rs", "engine_world/src/scenes/chunks/chunk_streaming_state.rs", "engine_world/src/scenes/edits/edit_streaming_state.rs", "engine_world/src/scenes/cellular_tiles/cellular_tile_streaming_state.rs", "engine_world/src/scenes/kinematics/kinematics.rs", "engine_world/src/scenes/kinematics/kinematics_pass.rs", "engine_world/src/scenes/collisions/collision.rs", "engine_world/src/scenes/collisions/collision_cellular_tile_pass.rs", "engine_world/src/scenes/collisions/collision_rigid_body.wgsl"]
+++

This page lists the ordered work inside `Scene::stream` and `Scene::simulate`, and why the order matters. It is for contributors changing scene, streaming, or collision code.

`Scene::stream` requests a region around the camera and moves the [active region](@/architecture/glossary.md#world-terms) when the entering chunks are ready. `Scene::simulate` runs one fixed simulation step against whichever region is active. Streaming does not wait for missing chunks, so the active region may lag the requested region while data arrives. The [frame lifecycle](@/architecture/frame-lifecycle.md) calls both in sequence after `Game::update`.

```text
scene.stream(accelerator, processor, camera_actor)
  derive requested region; initialize chunk/cellular tile storage if needed
  apply queued edits to resident GPU tiles
  advance CPU chunk I/O state from completed responses
  release rigid bodies leaving the region into static tile cells
  try to advance the tile ring when incoming chunks are active and no pending request blocks the shift

scene.simulate(accelerator)
  if an active region exists:
    GPU cellular kinematics + read available prior contacts
    CPU/Rapier collision and actor/cell updates (may read GPU occupancy)
    GPU fluid particle simulation and streaming exports
```

## Streaming order

### Requested region

The requested region uses the camera actor's unzoomed dimensions; see [the tile-ring page](@/internals/world-streaming/tile-ring.md) for its size and bounds.

On first streaming, `Scene` obtains the `chunk_io` pool, creates chunk streaming state, allocates cellular tile buffers, and initializes the [tile ring](@/architecture/glossary.md#world-terms). Later calls update the requested region but keep the ring dimensions; a request with a different width or height returns `SceneError::InvalidCellularTileRegion`.

### Edits and chunks

Queued edits are applied before movement can reuse their tile slots. Edits are validated when requested, then merged on the CPU during streaming before GPU writes are queued. If the edits produce GPU writes and collision state exists, collision occupancy is marked for refresh.

Next, `ChunkStreamingState::stream_chunks` incorporates completed worker responses and schedules more chunk work without blocking. The tile-ring shift then reads entering tile arrays from active chunks. If a required chunk is not active, the previous active region stays in place for now.

### Moving the tile ring

Before the ring moves, collision releases rasterized rigid bodies whose tile footprint extends beyond the requested region. Static rasterized cells remain static at rest; covered dynamic cells remain dynamic as their body-cell markers are cleared, so the cells stay behind when their tiles move.

Tile streaming then runs these steps:

1. Copy the outgoing tiles' material identifiers, colors, and kinematics to staging buffers.
2. Update the toroidal ring offset so overlapping tiles keep their physical slots.
3. Queue writes for incoming tiles.

Queue ordering places the outgoing copies before the writes that reuse those slots.

The staging buffers are mapped asynchronously. Once the mapping completes and the destination chunks are active, the returned data is applied to the chunks in request order. See [tile streaming](@/internals/world-streaming/tile-streaming.md) for the response flow, and [chunk residency](@/internals/world-streaming/chunk-residency.md) and [the tile ring](@/internals/world-streaming/tile-ring.md) for chunk residency and ring mapping.

## Simulation order

### Kinematics

Simulation returns immediately until tile buffers and an active region exist. The scene lazily creates collision state, then kinematics state. Kinematics receives:

- the active region and ring offset;
- the region origin relative to collision space and gravity;
- eligible nearby actor colliders whose filter includes `COLLISION_GROUP_CELLULAR_TILE`;
- rigid-body records.

Its GPU pass advances dynamic cells using the previous step's state and queues asynchronous contact and control readbacks. Completed contacts are returned to the scene.

### Collision

After kinematics, the scene passes the returned contacts to collision. Dynamic cells therefore move where the previous step left them, then push back on actors and rigid bodies once their contacts are read.

Collision uses Rapier for actor and rigid-body motion, applies cell changes, and may rasterize rigid bodies back into tiles. It reads cellular occupancy from the GPU:

- Static occupancy is read with a GPU wait and refreshed when its snapshot is absent, the region or ring offset changes, or a reread is requested.
- When rigid bodies exist, settled dynamic occupancy uses completed asynchronous snapshots, with a blocking fallback when no held snapshot matches the region and ring offset. See [occupancy readback](@/internals/simulation/static-cell-colliders.md#occupancy-readback).

See [cellular kinematics](@/internals/simulation/cellular-kinematics.md), [collision world](@/internals/simulation/collision-world.md), [static cell colliders](@/internals/simulation/static-cell-colliders.md), and [contact exchange](@/internals/simulation/contact-exchange.md).

The separate graphics call consumes the resulting active tile buffers and actor sprites; rendering is not part of `Scene::simulate`. See [data flow](@/architecture/data-flow.md) and [scene renderer](@/internals/rendering/scene-renderer.md).

## Fluid step

After cellular kinematics, the scene polls earlier fluid actor feedback and runs collision. It then runs fluid simulation against the shared actor and rigid-body records, and marks any outgoing particle export submitted. See [fluid simulation](@/internals/simulation/fluids.md) for streaming and save handoffs.
