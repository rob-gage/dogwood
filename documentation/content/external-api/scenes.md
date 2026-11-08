+++
title = "Scenes"
description = "Scene construction, streaming and simulation contracts, generators, and exact positions."
weight = 30
[extra]
sources = ["engine_world/src/scenes/scene.rs", "engine_world/src/scenes/actor_sensing", "engine_world/src/scenes/soft_bodies", "engine_world/src/scenes/soft_bodies/soft_body_sampling_pass.rs", "engine_world/src/scenes/cellular_tiles/cellular_tile_pass.rs", "engine_world/src/scenes/chemistry", "engine_world/src/scenes/thermal", "engine_world/src/scenes/scene_error.rs", "engine_world/src/scenes/scene_generator.rs", "engine_world/src/scenes/scene_position.rs", "engine_world/src/scenes/mod.rs", "engine_world/src/scenes/chunks/chunk_streaming_state.rs", "engine_world/src/scenes/chunks/chunk_background.rs", "engine_world/src/scenes/chunks/chunk_decoration.rs", "engine_world/src/scenes/chunks/chunk_decoration.rs", "engine_world/src/scenes/scene_backgrounds.rs", "engine_world/src/scenes/scene_material_surfaces.rs", "engine_world/src/scenes/cellular_tiles/cellular_tile_streaming_state.rs", "engine_world/src/scenes/collisions/collision.rs", "engine_world/src/materials/material_table.rs", "engine/src/lib.rs", "engine_world/src/actors/components/actor_light.rs"]
+++

This page covers the `Scene` type, the `SceneGenerator` trait, `ScenePosition`, and `SceneError`. It is for game programmers who build and drive a scene; games that use `Engine::launch` get the lifecycle calls from `GameInstance` automatically.

`Scene` owns actor state, materials, chunk persistence, streaming, simulation, and queued edits. Construct one with `Scene::new(chunk_data, material_table, generator, world_seed)`. The generator is shared through `Arc<dyn SceneGenerator>`, and the seed is passed to generation. See [Coordinates, tiles, and chunks](@/external-api/world-storage.md) for storage types, and [Actors](@/external-api/actors.md) and [Materials](@/external-api/materials.md) for the owned data.

## Accessors

- `actor_registry_mutable()` gives mutable access to the `ActorRegistry`.
- `material_table()` returns the compiled material table.
- `gravity()` returns downward acceleration in tiles per second squared. `set_gravity(f32)` sets it; the source default is `50.0`. The setter does not validate; negative values produce upward acceleration.
- `set_ambient_gas_pressure(f32)` sets the pressure of the phantom air filling empty cells, in atmospheres. Values must be finite and in `[0, 256)`; otherwise it returns `InvalidAmbientGasPressure`. The default is `1.0`; zero leaves empty cells a vacuum. Tiles without stored gas start with it.
- `ambient_gas_temperature()` returns the configured phantom-air temperature in kelvin. `set_ambient_gas_temperature(f32)` accepts finite nonnegative values and controls newly initialized gas temperatures and ambient relaxation.
- `set_ambient_gas_heat_capacity(f32)` sets heat per atmosphere per kelvin; it must be finite and positive.
- `set_ambient_gas_conduction(f32)` sets ambient gas conduction; it must be finite and nonnegative.
- `set_ambient_gas_mass(f32)` sets the mass of one atmosphere of phantom air, compared with gas material masses. Values must be finite and positive; otherwise it returns `InvalidAmbientGasMass`. The default is `29.0`.
- `edit(Edit)` queues an edit and returns `EditError` on failure. See [Collisions and edits](@/external-api/collisions-and-edits.md).
- `save()` requests persistence; it is a no-op before streaming is initialized. Save requests are nonblocking; drive the normal `stream`/`simulate` lifecycle until `is_saved()` is true. [Particle persistence](@/internals/world-streaming/fluid-streaming.md#resident-saves) describes the pending phases.
- `is_saved()` reports whether requested saves have completed.
- `active_cellular_tile_region()` returns the active region, or `None` before tiles become resident.
- `raycast(origin: ScenePosition, direction: [f64; 2], maximum_distance: f64, filter: u32, exclude: Option<Actor>) -> Option<SceneRayHit>` queries the existing collision broad phase and returns the nearest filtered hit. Direction may have any nonzero length; distance is in tiles. `filter` is a collision-group mask, and `exclude` skips the specified actor. It returns `None` before collision state exists or when the ray has invalid inputs or no hit. A ray starting inside a solid reports a zero-distance hit. `SceneRayHit` fields are `position: ScenePosition`, outward `normal: [f32; 2]`, `distance: f64` in tiles, and `target: SceneRayTarget`; targets are `CellularTile { cell }`, `RigidBody { rigid_body }`, or `Actor { actor }`.
- `pawn_velocity(actor: Actor) -> Option<[f32; 2]>`, `pawn_impact_speed(actor: Actor) -> Option<f32>`, and `pawn_is_grounded(actor: Actor) -> Option<bool>` read a simulated pawn's movement state. Velocity and impact speed are in tiles per second; impact speed is the largest speed lost along a solid surface normal in the last movement step. Grounded reports the last movement step. See [pawn movement](@/internals/actors/pawn-movement.md#pawn-impact-speed) for what counts as an impact.
- `add_pawn_velocity(actor, velocity)` adds velocity in tiles per second at the pawn's next movement step. `shove_pawn(actor, velocity)` also temporarily reduces the pawn's control over its acceleration. Both return whether the actor is a simulated pawn.
- `terrain() -> Option<SceneTerrain>` returns the last simulated region's static-cell occupancy. `SceneTerrain` exposes its tile origin and size, each tile's solid-cell bitset, and `is_solid(cell)`; positions beyond the region are not solid.
- `set_actor_sensing(configuration: SceneActorSensingConfiguration) -> Result<(), SceneError>` chooses up to eight material tags, contact and breathing margins, and gas thresholds; `actor_sensing(actor: Actor) -> Option<SceneActorSensing>` reads tag and breathing counts plus maximum temperature (kelvin) and absolute gauge pressure (atmospheres), one or more simulation steps old. `gas_tag_amount` defaults to `0.05` atmospheres. See [Actor sensing](@/internals/simulation/actor-sensing.md).

`SceneActorSensingConfiguration` fields are `tags: Vec<String>` (empty), `contact_margin: f32` (1.5 cells), `breathing_margin: f32` (2.5 cells), `breathable_gas_amount: f32` (0.5 atmospheres), and `gas_tag_amount: f32` (0.05 atmospheres). Use `..Default::default()` to keep defaults when setting selected fields. `SceneActorSensing` returns `tag_cell_counts`, `breathable_cell_count`, `breathing_cell_count`, `maximum_temperature`, and `maximum_pressure`.
- `set_pawn_tether(actor: Actor, tether: Option<ScenePawnTether<'_>>) -> bool` sets or clears the tether for an eligible simulated pawn's next movement step; set it again each update. `ScenePawnTether` has `direction: [f32; 2]`, `stretch: f32` in tiles, `bodies: &[ScenePawnTetherBody]`, and `damping: f32` for speed loss across the tether per second. Direction points from the pawn toward the nearest rope point; stretch is excess rope length. Each body entry has `rigid_body: SceneRigidBody`, `point: ScenePosition`, and `gradient: [f32; 2]`. Movement shares its impulse between pawn and held dynamic bodies. See [pawn movement](@/internals/actors/pawn-movement.md).
- Soft-body methods spawn, reshape, cut, remove, and tear bodies; query particles, paths, anchors, sleep state, and sampled environment; target or push particles; and configure environment tags. `soft_body_particles` returns positions, velocities, terrain contact normals, triangle indices, outline particle pairs, and masses. See [soft bodies](@/internals/simulation/soft-bodies.md) for the canonical API and behavior details.
- `set_decoration_atlas(Arc<SpriteSheet>)` supplies the atlas used by chunk background decorations. `ChunkBackground.decorations` contains generated pieces; see [chunk decorations](@/internals/rendering/decorations.md).
- `rigid_body_local_point(rigid_body: SceneRigidBody, position: ScenePosition) -> Option<[f32; 2]>` converts a scene position into a live cellular rigid body's local frame in tiles. `rigid_body_scene_position(rigid_body: SceneRigidBody, local: [f32; 2]) -> Option<ScenePosition>` converts back. `SceneRigidBody` identifiers are never reused and are stored in the Rapier body's user data for constant-time hit resolution; a ray hit supplies the identifier. Removed bodies no longer resolve.

## Lifecycle

Call `stream` and `simulate` each fixed update, and `graphics` when drawing. `GameInstance` already does this for games launched with `Engine::launch`; see [Frame and update lifecycle](@/architecture/frame-lifecycle.md) for the order and internal steps.

- `stream(&Accelerator, &Processor, camera_actor) -> Result<(), SceneError>` moves the active region toward a region centered on the camera, twice its unzoomed width and height. It does not wait for I/O. The actor needs camera and kinematics-position components, and the processor needs a `chunk_io` pool ([Host types](@/external-api/host.md)).
- `simulate(&Accelerator) -> Result<(), SceneError>` returns immediately before the active region exists. Otherwise it simulates dynamic cells against actors and rigid bodies, performs collisions, advances fluid particles, then pressure. When reactions are registered, chemistry selection and reactions are recorded inside thermal's compute pass between thermal prepare and step. Fluid heat is applied afterward, then actor sensing reads prior completed results and dispatches the current sample asynchronously. Collision processing may wait for GPU readbacks when it reads settled occupancies or changed cells; actor sensing does not wait for its result readback. See [actor sensing](@/internals/simulation/actor-sensing.md).
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
- `generate_background(coordinates, world_seed)` may return a transient background for generated and loaded chunks. It runs on a streaming worker, is never saved, and should be deterministic. See [chunk backgrounds](@/external-api/world-storage.md#chunk-backgrounds).
- It runs on the `chunk_io` worker pool.
- The returned chunk must use the requested coordinates; mismatches are rejected as generation errors.
- A panic in `generate_chunk` is caught and reported as `SceneError::ChunkGeneration`.
- Generated cell colors are resolved against material albedo by default. `Chunk::set_albedo_resolved(true)` makes supplied colors final; cells left white still take their material albedo. See [Materials](@/external-api/materials.md).

`Scene::set_material_edges(bool)` turns surface art, its color shift, and contact shadows on or off; it is off by default. `Scene::set_background_sprites(Arc<SpriteSheet>)` sets the background sprite sheet. Each `ChunkBackgroundSprite` has a `crop` of its frame, which should stay tight around what the frame shows, because its quad's pixels all cost GPU time. `Scene::set_material_surfaces(&Accelerator, &[MaterialSurface])` uploads optional surface art; see [material surfaces](@/external-api/materials.md#material-surfaces).

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
| Gas configuration | `InvalidAmbientGasPressure`, `InvalidAmbientGasMass`, `InvalidAmbientGasTemperature`, `InvalidAmbientGasHeatCapacity`, `InvalidAmbientGasConduction` |
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

> **Limitation:** Parallax light spill uses a depth-dependent blur and attenuation approximation. Graphics records retain physical depth separately from integer sprite order for lighting.

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

## Lighting

Scenes expose ambient-light and light-penetration settings. Defaults, validation, and units are listed on [Lighting](@/external-api/lighting.md#scene-and-actor-controls). The renderer consumes them with resident scene graphics.

## Acoustic observations

`sample_acoustics` exposes coarse GPU activity without blocking simulation; `acoustic_impacts` reads significant existing CPU rigid-body contacts. `stored_material_at` and `rigid_body_material` provide advisory material hints. These methods leave playback and game-specific thresholds to the caller. See [simulation acoustics](@/external-api/acoustics.md).
