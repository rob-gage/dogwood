+++
title = "Chunk I/O scheduling"
description = "Follows background chunk loads, generation, saves, retries, and shutdown persistence."
weight = 30
[extra]
sources = ["engine_world/src/scenes/chunks/chunk_streaming_state.rs", "engine_world/src/scenes/chunks/chunk_streaming_request.rs", "engine_world/src/scenes/chunks/chunk_streaming_response.rs", "engine_world/src/scenes/scene.rs", "engine_world/src/scenes/scene_generator.rs", "engine_world/src/scenes/mod.rs", "engine_world/src/materials/material_table.rs"]
+++

This page is for contributors working on how chunks are loaded, generated, and saved in the background. It covers request types, scheduling order, error handling, copy saves, and the shutdown drain. The resident states themselves are in [chunk residency](@/internals/world-streaming/chunk-residency.md).

## Requests and worker bound

Each `ChunkStreamingRequest` is one of:

- a load
- a generation request, carrying the generator, world seed, and material table
- a save, owning a chunk

Jobs run on the supplied Rayon pool and send typed `ChunkStreamingResponse` values through a channel. `Scene::stream` supplies the `chunk_io` pool. The scheduler's pending limit is captured from that pool's thread count when `ChunkStreamingState` is constructed. Each stream advance polls responses without waiting.

### Scheduling order

An advance runs these steps in order:

1. Collect coordinates covered by retained regions.
2. Apply ready responses.
3. Unload clean unneeded entries.
4. Schedule work.

Scheduling fills free slots by these rules:

1. When unretained dirty saves and required loads compete for multiple free slots, one slot is reserved for saving. When one slot is free and both are pending, the scheduler alternates which gets it.
2. When `save_chunks` has been requested, retained dirty chunks are cloned for copy saves into any remaining slots.
3. Loads use per-region queues with a rotating starting region, so regions take turns receiving slots. Coordinates within each region are queued in row-major order.

## Load, generate, and color

A load yields a loaded chunk or a `Missing` response. Storage errors produce `SceneError::ChunkLoad`. A missing response records `Missing` while the coordinate remains required, and generation is queued when a request slot is available.

### Generation

Generation calls `SceneGenerator::generate_chunk(coordinates, world_seed)` and rejects a returned chunk with different coordinates. For each non-empty generated cell, the scheduler recalculates its color through `MaterialTable::cellular_color`, passing the material identifier, absolute cell coordinates, and existing color. Generator and save errors produce `SceneError::ChunkGeneration` and `SceneError::ChunkSave`.

### Panics and retries

Load, generation, and save work is wrapped in `catch_unwind`; panics become the corresponding typed errors. Save responses return chunk ownership even on failure, so dirty data can be restored to the resident map and explicitly retried.

- Failed loads can be retried only while their coordinate is still requested.
- A failed generation is retried by retrying the load while the coordinate is requested. If storage still reports `Missing`, generation runs again.
- Failed save data remains dirty.

### Stale responses

Request identifiers guard response application. A load or generation completion must match the current in-flight entry and still be requested. Save completions are checked against the current save ID, with a separate ID map for copy saves. Mismatched responses cannot replace newer state.

## Save copies and shutdown

### Copy saves

When `save_chunks` is requested, dirty retained chunks are cloned for background saves, so the active copy stays resident and can be modified while the save runs. Dirty chunks outside retained regions can transfer ownership to a save job. Once no dirty, failed-dirty, or in-flight save work remains, the save-request flag clears.

If a copy save fails during streaming, the active chunk is kept, not replaced by the saved snapshot. Its entry becomes `Error` (dirty) and owns the newer active chunk until `retry_chunk_streaming_request`.

### Drop

Dropping `ChunkStreamingState` first waits for outstanding normal and copy saves. A failed copy save marks its active chunk dirty again. A failed normal save returns its chunk to an error entry.

Drop then synchronously saves every remaining dirty chunk, trying each at most three times. If any final save fails, drop panics. Scene shutdown first returns GPU tile contents to CPU chunks; the transfer path is described in [tile streaming](@/internals/world-streaming/tile-streaming.md).

Chunk bytes and atomic file replacement are documented in [chunk storage format](@/internals/world-streaming/chunk-format.md).

Chunks also own dormant fluid particles. Imports remove selected whole-tile groups from CPU storage, and GPU-rejected imports are returned. Resident snapshots replace saved particle copies while avoiding dormant import duplicates. See [fluid streaming](@/internals/world-streaming/fluid-streaming.md).
