+++
title = "Scenes"
description = "Scene construction, streaming and simulation contracts, generators, and exact positions."
weight = 30
[extra]
sources = ["engine_world/src/scenes/scene.rs", "engine_world/src/scenes/scene_error.rs", "engine_world/src/scenes/scene_generator.rs", "engine_world/src/scenes/scene_position.rs", "engine_world/src/scenes/mod.rs", "engine_world/src/scenes/chunks/chunk_streaming_state.rs", "engine_world/src/scenes/cellular_tiles/cellular_tile_streaming_state.rs", "engine_world/src/materials/material_table.rs", "engine/src/lib.rs"]
+++

This page covers the `Scene` type, the `SceneGenerator` trait, `ScenePosition`, and `SceneError`. It is for game programmers who build and drive a scene; games that use `Engine::launch` get the lifecycle calls from `GameInstance` automatically.

`Scene` owns actor state, materials, chunk persistence, streaming, simulation, and queued edits. Construct one with `Scene::new(chunk_data, material_table, generator, world_seed)`. The generator is shared through `Arc<dyn SceneGenerator>`, and the seed is passed to generation. See [Coordinates, tiles, and chunks](@/external-api/world-storage.md) for storage types, and [Actors](@/external-api/actors.md) and [Materials](@/external-api/materials.md) for the owned data.

## Accessors

- `actor_registry_mutable()` gives mutable access to the `ActorRegistry`.
- `material_table()` returns the compiled material table.
- `set_gravity(f32)` sets downward acceleration in tiles per second squared. The source default is `50.0`. The setter does not validate; negative values produce upward acceleration.
- `edit(Edit)` queues an edit and returns `EditError` on failure. See [Collisions and edits](@/external-api/collisions-and-edits.md).
- `save()` requests persistence; it is a no-op before streaming is initialized. Save requests are nonblocking; drive the normal `stream`/`simulate` lifecycle until `is_saved()` is true. [Particle persistence](@/internals/world-streaming/fluid-streaming.md#resident-saves) describes the pending phases.
- `is_saved()` reports whether requested saves have completed.
- `active_cellular_tile_region()` returns the active region, or `None` before tiles become resident.

## Lifecycle

Call `stream` and `simulate` each fixed update, and `graphics` when drawing. `GameInstance` already does this for games launched with `Engine::launch`; see [Frame and update lifecycle](@/architecture/frame-lifecycle.md) for the order and internal steps.

- `stream(&Accelerator, &Processor, camera_actor) -> Result<(), SceneError>` moves the active region toward a region centered on the camera, twice its unzoomed width and height. It does not wait for I/O. The actor needs camera and kinematics-position components, and the processor needs a `chunk_io` pool ([Host types](@/external-api/host.md)).
- `simulate(&Accelerator) -> Result<(), SceneError>` returns immediately before the active region exists. Otherwise it simulates dynamic cells against actors and rigid bodies, then performs actor and rigid-body collisions against active tiles, then advances fluid particles. Collision processing may wait for GPU readbacks when it reads settled occupancies or changed cells.
- `graphics(camera_actor)` returns borrowed graphics data, or an `ActorError` if the actor data is missing. Its actor also needs camera and kinematics-position components.
- `cellular_graphics(camera_position)` returns active cellular graphics if available.
- `fluid_graphics(camera_position)` returns optional borrowed raster/material data for fluid rendering, included automatically by `graphics`.

### Edge cases

- Before the first `stream` call, `camera.width` and `camera.height` must both be greater than zero; that call fixes the tile-ring dimensions from them. Changing either later returns `SceneError::InvalidCellularTileRegion`.
- When a streamed scene is dropped, it applies pending edits and returns active tiles to chunk storage. Chunk streaming waits for outstanding saves and attempts to save each remaining dirty chunk up to three times, including the first attempt. A failure while applying pending edits or returning tiles, or an unsuccessful final save after retries, panics. See [Resources and lifetimes](@/architecture/resources-and-lifetimes.md).

## Generator contract

Implement `SceneGenerator: Send + Sync` to produce chunks that have no persisted file.

```rust
use engine::scenes::{
    chunks::{Chunk, ChunkCoordinates},
    SceneGenerator,
};
use std::error::Error;

struct EmptyGenerator;

impl SceneGenerator for EmptyGenerator {
    fn generate_chunk(
        &self,
        coordinates: ChunkCoordinates,
        _world_seed: u64,
    ) -> Result<Chunk, Box<dyn Error + Send + Sync>> {
        Ok(Chunk::new(coordinates))
    }
}

fn main() {}
```

- `generate_chunk` is called when a requested chunk has no persisted file.
- It runs on the `chunk_io` worker pool.
- The returned chunk must use the requested coordinates; mismatches are rejected as generation errors.
- A panic in `generate_chunk` is caught and reported as `SceneError::ChunkGeneration`.
- Generated cell colors are applied to cellular materials; see [Materials](@/external-api/materials.md).

## Position representation

`ScenePosition` is `{ tile_x: i64, tile_y: i64, offset_x: u32, offset_y: u32 }`. Offsets are fixed-point fractions in units of `1 / 2^32` tile; `OFFSET_UNITS_PER_TILE` is `4_294_967_296.0`. The [glossary](@/architecture/glossary.md) explains the exact arithmetic.

| Method | Behavior |
| --- | --- |
| `relative_position(origin)` | Returns the `(f64, f64)` offset from `origin` in tiles |
| `translate(x, y)` | Moves by tile units and returns a `ScenePosition`; a non-finite distance leaves that axis unchanged |
| `cell_coordinates()` | Returns the containing cell |

Tile and cell coordinate conversion, including negative coordinates, is covered in [Coordinates, tiles, and chunks](@/external-api/world-storage.md).

## Scene errors

`SceneError` variants group into these failure areas:

| Area | Variants |
| --- | --- |
| Chunk requests and storage | `ChunkRegionRequestLimitReached`, `InvalidChunkRegion`, `ChunkResidentLimitReached`, `ChunkRegionRequestNotFound`, `ChunkRetryUnavailable`, `ChunkStreamingCapacityReached`, `ChunkStreamingRequestAlreadyActive`, `ChunkStreamingResponseChannelClosed`, `ChunkLoad`, `ChunkSave`, `ChunkGeneration`, `ChunkThreadPoolUnavailable` |
| Tile streaming | `CellularTileStreamingRequestNotFound`, `InvalidCellularTileRegion`, `CellularTileStreamingResponse`, `CellularTileStreamingResponseChunkInactive`, `CellularTileStreamingPoll` |
| Collision and kinematics | `CollisionCellularTileResponse`, `CollisionCellularTilePoll`, `KinematicsCellularTileResponse`, `KinematicsCellularTilePoll` |
| Fluid configuration and transfers | `InvalidFluidCapacityFactor`, `FluidCapacityFixed`, `FluidParticleStreamingRecordInvalid`, `FluidParticleStreamingResponseChunkInactive`, `FluidParticleResponse`, `FluidParticlePoll`, `FluidParticleExportBufferSizeInvalid`, `FluidParticleExportRecordInvalid`, `FluidParticleSnapshotResponseChannelClosed` |
| Actors and colliders | `CameraActor`, `InvalidCollider`, `ActorDespawn` |

See the linked rustdoc for variant payloads.

{{ <rustdoc path="engine/scenes/struct.Scene.html" text="Scene" /> }} · {{ <rustdoc path="engine/scenes/enum.SceneError.html" text="SceneError" /> }} · {{ <rustdoc path="engine/scenes/trait.SceneGenerator.html" text="SceneGenerator" /> }} · {{ <rustdoc path="engine/scenes/struct.ScenePosition.html" text="ScenePosition" /> }}

## Fluid capacity

`fluid_capacity_factor()` returns the configured pool fraction. `set_fluid_capacity_factor(factor)` accepts finite values in `(0, 1]` before the first stream creates the pool; otherwise it returns `InvalidFluidCapacityFactor` or `FluidCapacityFixed`. The default is `0.25`, and capacity is `floor(factor * active-region cell count)`.

The pool is allocated even when no fluid material is registered. A very small positive factor can round to zero particles. The factor sets particle capacity, not all region-sized scratch memory. See [fluid streaming](@/internals/world-streaming/fluid-streaming.md) for capacity reservations and persistence.
