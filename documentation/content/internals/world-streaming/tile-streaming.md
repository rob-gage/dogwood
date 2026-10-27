+++
title = "Tile streaming"
description = "How the scene moves its GPU tile ring and returns tile data to chunk storage."
weight = 50
[extra]
sources = ["engine_world/src/scenes/cellular_tiles/cellular_tile_streaming_state.rs", "engine_world/src/scenes/cellular_tiles/cellular_tile_buffers.rs", "engine_world/src/scenes/cellular_tiles/cellular_tile_streaming_request.rs", "engine_world/src/scenes/cellular_tiles/cellular_tile_streaming_response.rs", "engine_world/src/scenes/chunks/chunk_streaming_state.rs", "engine_world/src/scenes/mod.rs", "engine_world/src/scenes/scene.rs", "engine_world/src/scenes/scene_error.rs"]
+++

## Overview

`CellularTileStreamingState` moves the fixed-size active region of cellular tiles (tiles) over chunk-backed world data. This page is for engine contributors working on region movement and tile return. It covers chunk retention, outgoing readback, and save and teardown.

- Chunks provide the contents of incoming tiles.
- Outgoing GPU tiles are copied to a staging buffer for asynchronous readback.
- Their chunks stay retained until the readback is applied.
- A tile save requests chunk saving after that return completes.

The [tile ring](@/internals/world-streaming/tile-ring.md) page covers GPU buffer layout, tile indexing, and logical-to-physical slot mapping. For the chunk-side lifecycle, see [chunk residency](@/internals/world-streaming/chunk-residency.md) and [chunk I/O scheduling](@/internals/world-streaming/chunk-io.md).

### Requested and resident regions

The state tracks a requested region separately from the resident active region. `is_cellular_tile_region_active` reports whether the requested region has become resident. Each `stream_cellular_tiles` call tries to advance the transfer without waiting for GPU work.

A new request must have the same dimensions as the original region because the ring does not resize. A mismatch returns `SceneError::InvalidCellularTileRegion`. Construction also returns this error if the initial region is empty or exceeds the supplied buffer capacity.

## Moving the region

### Retention

Before moving, streaming retains the chunks covered by the requested region. While a move waits, both the resident and requested chunk regions remain retained. If the caller requests the currently resident region again, any pending requested-region retention is released.

### Conditions for a shift

A shift waits, and the call returns without moving, when any of these holds:

- Four requests already await application. Save and teardown readbacks can add requests outside this check.
- An earlier outgoing request still contains a tile in the requested region. This prevents a tile from being uploaded again before its previous readback has reached its chunk.
- Any chunk covering an entering tile is not active yet.

The overall call may still return a polling or mapping error from response processing.

### Performing the shift

Once incoming data is available, the state does the following in order:

1. Copies departing tiles under the old ring mapping.
2. Shifts the ring offsets so overlapping logical tiles keep their physical slots.
3. Marks the requested region resident.
4. Queues writes for entering tiles.

The queued writes upload material identifiers, paired color/radiance words, kinematics, and pressure state. The color/radiance buffer uses 8 bytes per cell. A tile without stored gas gets default gas: phantom air at the scene's ambient amount in each empty cell, with no species and no flow. They set the awake value to `CELLULAR_TILE_AWAKE`, set the gas/pressure awake value to `CELLULAR_TILE_GAS_AWAKE`, and clear rigid-body cell data.

When the ring shifts, the old region's retention moves to the outgoing request. It is released after the departing tiles reach their chunks.

## Returning outgoing tiles

### Readback

Each outgoing request copies the selected tiles' material identifiers, paired color/radiance words, kinematics, gas, and matter into a staging buffer. Each tile's staging record is 3840 bytes: material IDs at 0–255, color/radiance at 256–767, kinematics at 768–1279, gas at 1280–3327, and matter at 3328–3839. It submits the GPU copy and requests a read mapping with `map_async`. A callback sends the mapping result over a channel.

The regular streaming path polls the device without waiting and incorporates any responses already received. After a mapping failure, response processing immediately requests another mapping. Its result is observed on a later poll.

### Applying responses

Responses are applied as an ascending-ID prefix. Processing stops at the first request that is not ready or whose destination chunks are not all active, so later ready responses wait.

- Applied responses write tile data into their destination chunks and release any retained chunk region attached to the request.
- A tile's gas is stored only when it differs from default gas for the scene's current ambient amount; otherwise its stored gas is removed.
- Applying in order prevents an older readback from overwriting newer data for the same logical tile.
- Mapping errors are returned as `SceneError::CellularTileStreamingResponse`. A failed request remains pending for another mapping attempt.

The chunk region for departing tiles stays retained while their readback is in flight, which protects the destination chunks from unloading.

## Save and teardown

`save_cellular_tiles` requests a return of every currently resident tile. Once that request has been applied to its chunks, the state calls `ChunkStreamingState::save_chunks`. If no active region exists yet, it requests chunk saving directly. The save request stays pending until the active tiles have reached their chunks.

`return_cellular_tiles` requests a readback of all resident tiles, then polls with `wait_indefinitely` while requests remain and applies ready responses; on success, the latest GPU tile data has reached chunks. For `Scene::drop` sequencing and failure behavior, see [resources and lifetimes](@/architecture/resources-and-lifetimes.md).

See the [edit pipeline](@/internals/simulation/edit-pipeline.md) for how queued edits enter this lifecycle, and [chunk I/O scheduling](@/internals/world-streaming/chunk-io.md) for saving dirty chunks.

## Fluid handoff

Outgoing tile requests include a particle export buffer. Fluid compaction writes outgoing particles during simulation; streaming applies the mapped export to retained chunks before releasing the request. Pending imports or resident save snapshots can defer region movement. See [fluid streaming](@/internals/world-streaming/fluid-streaming.md).
