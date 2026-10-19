+++
title = "Chunk residency"
description = "Explains retained chunk regions, resident states, dirty tracking, and unloading."
weight = 20
[extra]
sources = ["engine_world/src/scenes/chunks/chunk_streaming_state.rs", "engine_world/src/scenes/chunks/chunk_resident.rs", "engine_world/src/scenes/chunks/chunk_region.rs", "engine_world/src/scenes/chunks/chunk_region_index.rs"]
+++

This page is for contributors working on chunk streaming. It covers the states a resident chunk moves through, the retained-region API, dirty tracking, and unloading.

## Resident states

`ChunkStreamingState` tracks entries by `ChunkCoordinates`. Each entry is a `ChunkResident` in one of these states:

- `Loading`: a load job is in flight.
- `Missing`: storage has no file for the chunk.
- `Generating`: a generation job is in flight.
- `Active`: the CPU chunk is available, with an `is_dirty` flag.
- `Saving`: a save job owns the chunk.
- `Error`: a job failed. The entry keeps the `SceneError`.

An entry that is not tracked is absent.

### Transitions

| State | Event | Next |
| --- | --- | --- |
| Absent | Load scheduled for a required coordinate | `Loading` |
| `Loading` | File found | `Active` (clean) |
| `Loading` | File missing | `Missing` |
| `Loading` | Storage error | `Error` |
| `Missing` | Still requested and a request slot is free | `Generating` |
| `Generating` | Success | `Active` (clean) |
| `Generating` | Failure | `Error` |
| `Active` (clean) | Tile contents changed | `Active` (dirty) |
| `Active` (dirty) | Not retained and selected by the scheduler under its load/save priority rules | `Saving` |
| `Saving` | Success, still retained | `Active` (clean) |
| `Saving` | Success, not retained | Absent |
| `Saving` | Failure | `Error` (dirty, owns the chunk) |
| `Error` | `retry_chunk_streaming_request`, failed load | `Loading` |
| `Error` | `retry_chunk_streaming_request`, dirty chunk | `Saving` |

A copy save of a retained dirty chunk does not change its state: the chunk stays `Active` and is marked clean until tiles change it again. If the copy save fails, the entry becomes `Error` (dirty) and keeps the newer active chunk. See [chunk I/O scheduling](@/internals/world-streaming/chunk-io.md) for copy saves.

### Dirty flag and stale responses

Marking a chunk dirty does not itself schedule a save. In-flight states carry request IDs so a completion can be checked against the current entry. Loading or generation results are accepted only if their ID still matches and the coordinate remains requested. See [chunk I/O scheduling](@/internals/world-streaming/chunk-io.md) for completion handling.

## Retained regions and limits

Retained regions keep chunks resident for the scene:

- `retain_chunk_region` creates a persistent request over inclusive chunk-coordinate bounds and returns a `ChunkRegionIndex`, which has public conversions to and from `u64`.
- `update_retained_chunk_region` changes an existing request.
- `release_chunk_region_request` removes one. Unknown IDs fail update and release.
- `is_chunk_active` is true only for an `Active` entry.
- `is_chunk_region_active` checks every coordinate in the indexed inclusive region and returns false for an unknown index.

### Admission checks

A region must have nonzero width and height, within the maximum region size. The state also limits the number of retained region requests and the number of resident coordinates. The values are listed in [capacity limits](@/architecture/resources-and-lifetimes.md#streaming-and-storage-limits).

The admission check counts the exact union of requested coordinates and dirty `Active` or `Error` entries still in the chunk map. It does not count dirty chunks owned by in-flight save jobs. The union deduplicates overlapping regions without filling gaps between them.

### Tile ring retention

The tile ring retains the chunk region under its active tiles and the region requested for its next position. These references keep chunk data available while GPU tile contents are moved; see [the cellular tile ring](@/internals/world-streaming/tile-ring.md) and [tile streaming](@/internals/world-streaming/tile-streaming.md).

## Dirty chunks and unloading

Writing changed tile arrays through `set_cellular_tile_cells` marks the active chunk dirty. Identical values return success without changing the existing dirty flag.

### Unloading

A streaming advance can remove clean entries that are no longer needed:

- clean, unrequested `Active` chunks
- `Missing` entries
- clean `Error` entries

Dirty chunks remain owned until scheduled for persistence. Loading, generating, and saving entries are not unloaded by this pass.

### Saving and retries

Each advance can schedule active dirty chunks outside retained regions when a request slot is available. A failed save remains in `Error` and must be retried with `retry_chunk_streaming_request`. `chunk_error` exposes the error for a chunk in the `Error` state.

`save_chunks` also requests copies of retained dirty chunks so they can stay active. Save requests are nonblocking during regular advances. On drop, outstanding saves are drained and remaining dirty chunks receive final synchronous attempts; details and failure handling are in [chunk I/O scheduling](@/internals/world-streaming/chunk-io.md).
