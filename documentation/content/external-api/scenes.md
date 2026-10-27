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
- `set_ambient_gas_pressure(f32)` sets the pressure of the phantom air filling empty cells, in atmospheres. Values must be finite and in `[0, 256)`; otherwise it returns `InvalidAmbientGasPressure`. The default is `1.0`; zero leaves empty cells a vacuum. Tiles without stored gas start with it.
- `ambient_gas_temperature()` returns the configured phantom-air temperature in kelvin. `set_ambient_gas_temperature(f32)` accepts finite nonnegative values and controls the temperature assigned to newly initialized gas.
- `set_ambient_gas_mass(f32)` sets the mass of one atmosphere of phantom air, compared with gas material masses. Values must be finite and positive; otherwise it returns `InvalidAmbientGasMass`. The default is `29.0`.
- `edit(Edit)` queues an edit and returns `EditError` on failure. See [Collisions and edits](@/external-api/collisions-and-edits.md).
- `save()` requests persistence; it is a no-op before streaming is initialized. Save requests are nonblocking; drive the normal `stream`/`simulate` lifecycle until `is_saved()` is true. [Particle persistence](@/internals/world-streaming/fluid-streaming.md#resident-saves) describes the pending phases.
- `is_saved()` reports whether requested saves have completed.
- `active_cellular_tile_region()` returns the active region, or `None` before tiles become resident.

## Lifecycle

Call `stream` and `simulate` each fixed update, and `graphics` when drawing. `GameInstance` already does this for games launched with `Engine::launch`; see [Frame and update lifecycle](@/architecture/frame-lifecycle.md) for the order and internal steps.

- `stream(&Accelerator, &Processor, camera_actor) -> Result<(), SceneError>` moves the active region toward a region centered on the camera, twice its unzoomed width and height. It does not wait for I/O. The actor needs camera and kinematics-position components, and the processor needs a `chunk_io` pool ([Host types](@/external-api/host.md)).
- `simulate(&Accelerator) -> Result<(), SceneError>` returns immediately before the active region exists. Otherwise it simulates dynamic cells against actors and rigid bodies, then performs actor and rigid-body collisions against active tiles, advances fluid particles, and simulates gas pressure and flow. Temperatures are stored and carried with matter and fluid particles, but are not simulated. Collision processing may wait for GPU readbacks when it reads settled occupancies or changed cells; gas submits its work without waiting or reading back.
- `graphics(camera_actor)` returns borrowed graphics data, or an `ActorError` if the actor data is missing. Its actor also needs camera and kinematics-position components.
- `cellular_graphics(camera_position)` returns active cellular graphics if available.
- `fluid_graphics(camera_position)` returns optional borrowed raster/material data for fluid rendering, included automatically by `graphics`.
- `gas_graphics(camera_position)` returns optional borrowed gas cell and material data for gas rendering, included automatically by `graphics`.

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
| Collision and kinematics | `CollisionCellularTileResponse`, `CollisionCellularTilePoll`, `CellularTileResponse`, `CellularTilePoll` |
| Fluid configuration and transfers | `InvalidFluidCapacityFactor`, `FluidCapacityFixed`, `FluidParticleStreamingRecordInvalid`, `FluidParticleStreamingResponseChunkInactive`, `FluidParticleResponse`, `FluidParticlePoll`, `FluidParticleExportBufferSizeInvalid`, `FluidParticleExportRecordInvalid`, `FluidParticleSnapshotResponseChannelClosed` |
| Gas configuration | `InvalidAmbientGasPressure`, `InvalidAmbientGasMass`, `InvalidAmbientGasTemperature` |
| Actors and colliders | `CameraActor`, `InvalidCollider`, `ActorDespawn` |

See the linked rustdoc for variant payloads.

{{ <rustdoc path="engine/scenes/struct.Scene.html" text="Scene" /> }} · {{ <rustdoc path="engine/scenes/enum.SceneError.html" text="SceneError" /> }} · {{ <rustdoc path="engine/scenes/trait.SceneGenerator.html" text="SceneGenerator" /> }} · {{ <rustdoc path="engine/scenes/struct.ScenePosition.html" text="ScenePosition" /> }}

## Fluid capacity

`fluid_capacity_factor()` returns the configured pool fraction. `set_fluid_capacity_factor(factor)` accepts finite values in `(0, 1]` before the first stream creates the pool; otherwise it returns `InvalidFluidCapacityFactor` or `FluidCapacityFixed`. The default is `0.25`, and capacity is `floor(factor * active-region cell count)`.

The pool is allocated even when no fluid material is registered. A very small positive factor can round to zero particles. The factor sets particle capacity, not all region-sized scratch memory. See [fluid streaming](@/internals/world-streaming/fluid-streaming.md) for capacity reservations and persistence.

## Parallax backgrounds

A parallax layer is an ordinary actor with `ActorKinematicsPositionComponent`, `ActorSpritesComponent`, and `ActorParallaxComponent { depth, ..Default::default() }`. One actor can contain several coplanar sprites; use separate actors for different physical distances. Spawn, despawn, or modify these components through `actor_registry_mutable()` during gameplay. Removing the parallax component routes its sprites back into ordinary gameplay rendering on the next frame.

`depth` is the distance behind gameplay in scene tiles. Projection uses a fixed one-tile focal distance: camera-relative position, sprite offsets, and sprite dimensions are multiplied by `1 / (1 + depth)`. At depth zero, movement and size match gameplay; depth three gives quarter-speed movement and quarter size. Camera zoom applies normally. Exact tile subtraction precedes projection, preserving nearby precision at large scene coordinates.

Sprites retain the existing scale convention: at scale one, each source pixel spans one cell. Unprojected width in tiles is `sheet.width() * scale_x / CELLULAR_TILE_SIZE`, with the same rule for height. Signed scales flip sprites. Set scale to `desired_world_width * CELLULAR_TILE_SIZE / sheet.width()` to author an explicit world width. Offsets are in scene tiles, rotation is counterclockwise radians, and tint is linear RGBA.

All parallax sprites draw behind gameplay, farthest first. At equal physical depth, the sprite tuple's integer depth controls draw order. Equal physical depth and integer order preserve ECS traversal order and sprite vector order; ordering across different actors is not a persistent contract. Nonfinite scroll factors or negative or nonfinite physical depth skip that actor's sprites. Invalid frames or nonfinite offsets, scale, rotation, or tint skip the affected sprite. Zero scale is invisible.

The template creates a separate finite blue sky actor at depth 10,000 tiles and far, middle, and near stone-slope actors with generated transparent PNGs in `template/assets/sprites/` with `parallax_` prefixes. Each landscape actor repeats horizontally with mirroring; the sky is a single large static sprite with no repetition. These are runtime actors, not saved chunk data. Repetition is configured independently on each sprite-local axis. `ActorParallaxComponent` automatically inserts `ActorPersistenceComponent`, so background anchors outside the simulation region remain alive. Removing parallax does not remove that persistence marker. Explicit despawn remains available. Collision and movement retain their ordinary contracts.

```rust
use engine::actors::components::{
    ActorKinematicsPositionComponent, ActorParallaxComponent, ActorSpritesComponent,
};
// Add alongside the actor's position and existing sprite records:
let parallax = ActorParallaxComponent {
    depth: 3.0,
    scroll_factor: [1.0, 0.2],
    ..Default::default()
};
```

> **Not implemented:** Lighting, radiance textures, parallax light spill, and normal maps. Graphics records retain physical depth separately from integer sprite order for later lighting.

### Independent scroll axes

`scroll_factor: [horizontal, vertical]` defaults to `[1.0, 1.0]`. Each axis multiplies the camera-motion response already derived from depth. For example, `[1.0, 0.1]` gives distant mountains ten times less vertical scrolling than horizontal scrolling for equal camera displacement. Zero follows the camera on that axis, values above one move faster, and negative values reverse scrolling. Sprite dimensions are unaffected by these factors.

`scroll_origin: ScenePosition` defaults to the world origin and anchors camera motion: at this camera position, scroll factors do not change the actor's world placement. With actor coordinate `P`, camera coordinate `C`, origin `O`, factor `F`, layer offset `L`, and depth `D`, projected center is `((P - C) + (C - O) * (1 - F) + L + sprite_offset) / (1 + D)`, independently for each axis. Use a nearby origin in very large worlds when overriding scroll factors. Changing a factor at runtime can reposition the background according to this anchor.

### Placement and tiling

`ActorParallaxComponent::offset: [f32; 2]` sets horizontal and vertical layer placement in unprojected scene tiles, in addition to actor position and each sprite's offset. It defaults to zero and applies to both finite and repeating layers. Nonfinite offsets skip the actor's sprites.

`repeat: [ParallaxRepeat; 2]` controls the sprite-local horizontal and vertical axes independently. `None` draws a finite sprite, `Repeat` repeats its frame, and `Mirror` alternates flipped copies to join nonseamless artwork. Both axes default to `None`. Import `ParallaxRepeat` from `engine::actors::components`. The actor position plus offsets anchors the original copy; repetition continues in both directions on enabled axes. Rotation rotates the repetition axes with the sprite. Each sprite on the actor repeats at its own scaled width and height.

Only viewport-overlapping copy ranges are expanded. Zero-sized sprites and patterns requiring more than 16,384 candidate copies per source sprite are skipped. Zoom, depth, scale, and offsets all affect the visible range. Renderer records retain physical depth and draw order for every source plane.

```rust
use engine::actors::components::{ActorParallaxComponent, ParallaxRepeat};
let mountains = ActorParallaxComponent {
    depth: 24.0,
    offset: [0.0, 400.0], // World tiles; projects to 16 tiles above the actor
    repeat: [ParallaxRepeat::Mirror, ParallaxRepeat::None],
    scroll_factor: [1.0, 0.1],
    ..Default::default()
};
```
