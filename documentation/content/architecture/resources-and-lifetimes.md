+++
title = "Resources and lifetimes"
description = "How a scene creates, shares, saves, and releases its world resources."
weight = 50
[extra]
sources = ["engine_world/src/scenes/fluids", "engine_world/src/scenes/pressure", "engine_world/src/scenes/thermal", "engine_world/src/scenes/collisions/collision.rs", "engine_world/src/scenes/scene.rs", "engine_world/src/scenes/actor_sensing", "engine_world/src/scenes/soft_bodies", "engine_world/src/scenes/chunks/chunk_streaming_state.rs", "engine_world/src/scenes/scene_backgrounds.rs", "engine_world/src/scenes/scene_material_surfaces.rs", "engine_world/src/scenes/cellular_tiles/cellular_tile_streaming_state.rs", "engine_world/src/scenes/mod.rs", "engine_world/src/scenes/collisions/mod.rs", "engine_world/src/scenes/cellular_tiles/cellular_tile.wgsl", "engine_host/src/acceleration/accelerator.rs", "engine_graphics/src/renders/scene_renderer_lighting_pass.rs", "engine_graphics/src/renders/scene_renderer_lighting_clear.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_cache.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_blocks.wgsl", "engine_graphics/src/renders/scene_renderer_lighting.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_cascades.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_mips.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_optical.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_spread.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_sprites.wgsl", "engine_world/src/scenes/scene_material_surfaces.rs", "engine_graphics/src/renders/scene_renderer_surface_pass.rs", "engine_graphics/src/renders/scene_renderer_surface.wgsl", "engine_graphics/src/world/materials/material_radiance.rs", "engine_world/src/materials/material_table.rs", "engine_world/src/materials/material_temperature_color.rs", "engine_world/src/scenes/chemistry", "engine_graphics/src/world/scenes/lighting_quality.rs", "engine_graphics/src/renders/scene_renderer_decorations_pass.rs", "engine_graphics/src/renders/scene_renderer_decoration_field.wgsl", "engine_graphics/src/renders/scene_renderer_decorations.wgsl", "engine_graphics/src/world/scenes/scene_graphics_decorations.rs", "engine_world/src/scenes/chunks/chunk_decoration.rs", "engine_world/src/scenes/chunks/chunk_background.rs", "engine_world/src/scenes/scene_backgrounds.rs"]
+++

This page describes what a `Scene` owns, when its GPU and streaming state is created, the fixed capacity constants, and what happens on save and drop. It is for engine contributors.

## Ownership

{{ <rustdoc path="engine/scenes/struct.Scene.html" text="Scene" /> }} owns its `ChunkData`, generator, actor registry, edit state, and optional streaming and simulation states. Its compiled `MaterialTable` and generator are held through `Arc`. Chunk streaming gets a cloned material-table handle and a cloned `ChunkData`.

Background chunk load, generation, and save jobs use the `chunk_io` pool, shared through `Arc<rayon::ThreadPool>`. Final saves during `ChunkStreamingState::drop` run synchronously.

```text
Scene
├── ChunkData ── clone ──> ChunkStreamingState
├── Arc<MaterialTable> ── clone ──> ChunkStreamingState
├── Arc<dyn SceneGenerator>
├── ActorRegistry + EditStreamingState
├── CellularTileBuffers ── cloned Accelerator + GPU buffers
├── CellularTileStreamingState
├── Collision
├── Thermal ── fluid heat, collision change lists, conversion list
├── Chemistry ── reacting-tile list and indirect dispatch (when reactions exist)
├── CellularTiles
├── ActorSensingPass + three asynchronous result readbacks
├── SoftBodySamplingPass + asynchronous sample readbacks (after simulation)
└── SceneRenderer owns decoration pass, atlas batches, and motion-field textures while drawing
```

### Lazy initialization

`Scene::new` leaves chunk streaming, tile buffers, tile streaming, collision, and kinematics absent. The first `Scene::stream` creates chunk streaming and the GPU tile ring, sized to twice the first camera's unzoomed width and height. Later requested regions must keep those dimensions.

Collision, kinematics, and thermal simulation are created on the first `simulate` call with an active region; thermal storage is available before fluid simulation so conversions can consume the prior thermal tick's list. Pressure simulation is created after fluids. The actor-sensing pass is created after simulation and keeps three asynchronous result readbacks. Tile buffers retain a cloned `Accelerator`; its clone holds handles to the same WGPU instance, adapter, device, and queue, so teardown can use that context. See the [scene pipeline](@/architecture/scene-pipeline.md) for call order.

## Saving and drop behavior

### Saving

`Scene::save` requests persistence. If tile streaming exists, it first requests return of resident tiles to their chunks; otherwise it asks existing chunk streaming to save. `Scene::is_saved` reports whether tile, particle snapshot, and chunk save requests have cleared. These requests progress during later streaming calls rather than blocking.

### Drop

Dropping a streamed `Scene` blocks in these stages:

1. If the scene has chunk streaming, tile buffers, and tile streaming state, `Scene::drop` applies queued edits and returns resident tiles to chunks. Failure in either step panics with `Scene cellular tiles could not be returned`.
2. `Scene::drop` waits for particle import results, restores unspawned particles, and reads live particles into resident chunk snapshots. Failure panics with `Scene fluid particles could not be returned`; see [fluid teardown](@/internals/world-streaming/fluid-streaming.md#teardown).
3. `ChunkStreamingState::drop` waits for outstanding normal and copy saves, then synchronously saves the dirty chunks it still owns. It makes up to three attempts per dirty chunk, including the first, and panics with `Scene chunks could not be saved` if any still fail.

Drop does not wait for load or generation jobs. See [panics at runtime boundaries](@/architecture/errors.md#panics-at-runtime-boundaries) for the failure policy.

## Capacity limits

## Rendering and generation constants

| Constant | Value | Meaning |
| --- | ---: | --- |
| `MATERIAL_SURFACE_SIZE` | 256 | Width and height in cells of each material surface art tile |
| `SCENE_BACKGROUND_UPLOADS_PER_STREAM` | 4 | Maximum chunk backgrounds uploaded during one stream call |
| `SCENE_BACKGROUND_OVERHANG` | 32 tiles | Extra culling margin around the visible rectangle for chunk sprites |
| `SURFACE_FIELD_MARGIN` | 8 cells | Field area added around the visible cell rectangle |

Only `MATERIAL_SURFACE_SIZE` is public; the remaining implementation constants are private. The parallax batch uniform stride is 256 bytes. See [material surfaces](@/external-api/materials.md#material-surfaces), [chunk backgrounds](@/examples/scene-generators.md#chunk-backgrounds), and [scene rendering](@/internals/rendering/scene-renderer.md).

These constants are fixed implementation values rather than runtime settings.

### Streaming and storage limits

| Constant | Value | Meaning |
| --- | ---: | --- |
| `SCENE_CHUNK_REGION_COUNT_MAXIMUM` | 32 | Retained chunk-region requests per scene |
| `SCENE_CHUNK_REGION_SIZE_MAXIMUM` | 32 | Maximum width or height of a chunk region |
| `SCENE_CHUNK_RESIDENT_COUNT_MAXIMUM` | 256 | Admission limit for retained coordinates and dirty chunks counted by the check |
| `SCENE_CHUNK_SAVE_ATTEMPT_COUNT_MAXIMUM` | 3 | Synchronous save attempts per dirty chunk during chunk-state drop, including the first attempt |
| `SCENE_CELLULAR_TILE_STREAMING_REQUEST_COUNT_MAXIMUM` | 4 | Defer a region shift when 4 readbacks are outstanding; save and teardown readbacks can exceed this count |

> **Limitation:** `SCENE_CHUNK_RESIDENT_COUNT_MAXIMUM` is not a strict memory bound: the 256-coordinate admission check counts retained regions and dirty chunks still in the chunk map. A dirty chunk whose ownership has moved into an in-flight save job is not counted, so chunk payloads held in memory can temporarily exceed it.

### Collision limits and tuning

| Constant | Value | Meaning |
| --- | ---: | --- |
| `COLLISION_DAMPING` | 0.5 | Scales averaged material restitution in cellular collision response |
| `COLLISION_ORIGIN_DISTANCE_MAXIMUM` | 1024 | Recenter when either axis differs from the active-region center by more than 1024 tiles |
| `COLLISION_CONTACT_ITERATION_COUNT` | 4 | Constraint solver passes per substep |
| `COLLISION_STATIC_CONTACT_FREQUENCY` | 600.0 Hz | Natural frequency for fixed contacts |
| `COLLISION_RIGID_BODY_CELL_COUNT_MINIMUM` | 12 | Smaller detached groups become debris |
| `COLLISION_RIGID_BODY_CELL_COUNT_MAXIMUM` | 4096 | Largest floating static-cell group formed into a rigid body |
| `COLLISION_RIGID_BODY_CCD_PREDICTION` | 1.0 tile | Prediction distance for moving rigid bodies |
| `COLLISION_RIGID_BODY_SOLVER_ITERATION_COUNT` | 8 | Extra solver iterations for rigid-body contact islands |

| `COLLISION_RIGID_BODY_COUNT_MAXIMUM` | 8192 | Rigid bodies and soft-body segments simulated at once |
| `COLLISION_RIGID_BODY_CELLULAR_TILE_COUNT_MAXIMUM` | 16384 | Tiles held across simulated rigid bodies |

### Defaults and reserved collision groups

| Constant | Value | Meaning |
| --- | ---: | --- |
| `SCENE_GRAVITY_DEFAULT` | 50.0 | Default downward acceleration, in tiles per second squared |
| `SCENE_AMBIENT_GAS_PRESSURE_DEFAULT` | 1.0 | Default phantom air pressure, in atmospheres |
| `SCENE_AMBIENT_GAS_MASS_DEFAULT` | 29.0 | Default mass of one atmosphere of phantom air |
| `SCENE_AMBIENT_GAS_TEMPERATURE_DEFAULT` | 293.0 K | Default phantom-air temperature |
| `SCENE_AMBIENT_GAS_HEAT_CAPACITY_DEFAULT` | 0.01 | Heat capacity per atmosphere of phantom air |
| `SCENE_AMBIENT_GAS_CONDUCTION_DEFAULT` | 0.05 | Heat conduction between phantom-air cells and toward surroundings |
| `SCENE_CHUNK_THREAD_POOL_NAME` | `"chunk_io"` | Pool for background chunk loads, generation, and saves |
| `COLLISION_GROUP_CELLULAR_DYNAMIC` | `1 << 27` | Settled dynamic cells collision group |
| `COLLISION_GROUP_ACTOR` | `1 << 28` | Actor colliders collision group |
| `COLLISION_GROUP_CELLULAR_TILE` | `1 << 29` | Solid cells in active cellular tiles |
| `COLLISION_GROUP_RIGID_BODY` | `1 << 30` | Cellular rigid body collision group |
| `COLLISION_GROUP_EDIT` | `1 << 31` | Colliders skipped by edit insertion |
| `COLLISION_GROUP_SOFT_BODY_SOLID` | `1 << 25` | Soft bodies that collide with actors |
| `COLLISION_GROUP_SOFT_BODY` | `1 << 26` | Soft bodies that do not collide with actors |

`COLLISION_RIGID_BODY_COUNT_MAXIMUM` and all seven `COLLISION_GROUP_*` constants are public. The other collision tuning constants are private; see [collision setup](@/examples/collision-setup.md). Soft-body behavior is documented in [soft bodies](@/internals/simulation/soft-bodies.md).

## Fluid resources

The first stream allocates the fixed pool and region-sized scratch maps even if no fluid material has been registered. Pool capacity cannot change afterward. `SCENE_FLUID_CAPACITY_FACTOR_DEFAULT` is `0.25`; `SCENE_FLUID_IMPORT_COUNT_MAXIMUM_PER_TICK` targets `4096` imported particles, allowing a larger first whole-tile group. [Fluid streaming](@/internals/world-streaming/fluid-streaming.md) describes save retention, import reservations, and teardown.

Public solver constants in `engine_world/src/scenes/fluids/mod.rs`:

| Constant | Value |
| --- | --- |
| `FLUID_SUBSTEP_COUNT` | 4 |
| `FLUID_ITERATION_COUNT` | 2 |
| `FLUID_KERNEL_RADIUS` | 2.5 cells |
| `FLUID_BIN_SIZE` | 3 cells |
| `FLUID_RELAXATION` | 0.01 |
| `FLUID_POSITION_RELAXATION` | 0.5 total correction budget across Jacobi iterations per substep |
| `FLUID_COHESION_SCALE` | 0.5 |
| `FLUID_COHESION_NORMALIZATION_2D` | 54.584815 |
| `FLUID_VELOCITY_MAXIMUM` | 4 cells/tick (vector-length cap in the particle solver) |
| `FLUID_RASTER_SCALE` | 4096 |
| `FLUID_WORKGROUP_SIZE` | 128 |
| `FLUID_SCAN_WORKGROUP_SIZE` | 256 |
| `FLUID_TIME_STEP` | 1/60 second |

The private `FLUID_ACTOR_CONTACT_SCALE` is `1024`. Count, actor, and import readbacks each have three staging slots; the resident save has one separate snapshot slot. [GPU layouts](@/internals/shaders/fluid-shader.md) document record sizes and region/pool scaling. The collision-side `RIGID_BODY_FLUID_CURRENT_COUPLING` is `1.0`.

## Actor sensing resources

`ActorSensingPass` allocates a 64-byte uniform buffer, a tag-mask buffer, 64-byte result records for up to 32 cellular tile actor records, and three asynchronous result readbacks. The masks contain one `u32` per cellular static, cellular dynamic, or fluid material index. `SCENE_ACTOR_SENSING_TAG_COUNT_MAXIMUM` is 8 configured tags; private thresholds `CONTACT_COVERAGE` and `DROWNING_COVERAGE` are both 0.5 fluid coverage. `RESULT_BYTE_COUNT` is 64, `UNIFORM_BYTE_COUNT` is 64, and `READBACK_COUNT` is 3. See [actor sensing](@/internals/simulation/actor-sensing.md) and the [shader reference](@/internals/shaders/actor-sensing-shader.md).

## Chemistry resources

Chemistry allocates one 80-byte uniform record, one reacting-tile index per tile slot, a four-byte reacting count, and a 12-byte indirect dispatch record. Material compilation supplies reaction, lookup, product-transform, and amount-color buffers; the fluid raster carries 16 additional bytes per cell for two species indices and amounts. The fluid spawn list holds 4096 eight-word product records (128 KiB plus its four-byte count). Reactions are limited to 64, with at most 255 entries per lookup list. `HEAT_CAPACITY_MINIMUM` is 0.01. See [chemistry simulation](@/internals/simulation/chemistry.md) and [chemistry shader](@/internals/shaders/chemistry-shader.md).

## Thermal resources

Thermal simulation is created before the first fluid simulation so it can provide the prior tick's
conversion list. It shares the cellular matter, gas, awake, and collision change-list buffers, and
reads the fluid heat raster. The conversion list holds up to `THERMAL_CONVERSION_COUNT_MAXIMUM` (16,384) cell or gas entries.
Public fixed-point scales in `engine_world/src/scenes/thermal/mod.rs` are:

| Constant | Value | Meaning |
| --- | ---: | --- |
| `THERMAL_FLUID_CAPACITY_SCALE` | 1024 | Units per heat-capacity unit in fluid heat records |
| `THERMAL_FLUID_ENERGY_SCALE` | 64 | Units per heat unit in fluid heat records |
| `THERMAL_FLUID_CONDUCTION_SCALE` | 1024 | Units per heat-capacity and conduction product in fluid heat records |
| `THERMAL_HEAT_SCALE` | 64 | Units per heat unit added to cells by matter changes |

Thermal uses the `TILE_LISTED` bit (`4`) to mark tiles included in its current dispatch; chemistry writes only to listed tiles. `HEAT_CAPACITY_MINIMUM` (`0.01`) is the minimum cell heat capacity that accepts reaction heat.

## Pressure resources

Pressure simulation allocates two pressure cell buffers, each a quarter of the tile ring's gas buffer, an eight-byte open-volume record per ring cell, 268 bytes of control storage per tile after a 16-byte header, uniforms for cells and for pressure cells, and the published pressure buffer. The device requests `max_storage_buffers_per_shader_stage = 24`. Its first `simulate` creates them; they live until the scene drops. Public shared collision limits in `engine_world/src/scenes/collisions/mod.rs`:

| Constant | Value |
| --- | --- |
| `GAS_AMOUNT_PER_ATMOSPHERE` | 4096 |
| `GAS_AMOUNT_MAXIMUM` | `0xff_ffff` (about 256 atmospheres in one species slot) |
| `PRESSURE_MAXIMUM` | 4095 atm per cell |
| `COLLISION_CHANGED_CELLULAR_TILE_COUNT_MAXIMUM` | 1024 changed tiles reported per tick |
| `COLLISION_CHANGED_RIGID_BODY_CELL_COUNT_MAXIMUM` | 4096 changed rigid-body cells reported per tick |

Pressure uses a 1/60-second tick and 3 substeps on pressure cells of 2×2 cells; per-second coefficients are listed on the [pressure simulation](@/internals/simulation/pressure.md#flows-volume-and-sleeping) page. See the [pressure shader](@/internals/shaders/pressure-shader.md) for its tuning constants.

## Lighting and radiance resources

`MATERIAL_RADIANCE_MAXIMUM` is `127`; packed radiance stores emission mantissas and shared exponent in bits 0–24 and occlusion in bits 25–31. The lighting pass uses `LIGHTING_INTERVAL = 2`, `LIGHTING_DIRECTIONS = 4`, `LIGHTING_BACKLIGHT_CELLS = 4`, and at most 16 cascade levels. The finest cascade scales its direction count by cells per lighting texel, keeping ray density per world distance constant across qualities. Backlight always uses one texel per 4 × 4 world cells. These lighting constants are private to the pass. `LightingQuality` selects 4 × 4, 2 × 2, or 1 × 1 cells per lighting texel, with High as the default. See [Lighting pass](@/internals/rendering/lighting-pass.md) for GPU layouts and [radiance API](@/external-api/lighting.md) for the public data contract.

## Optional acoustic observations

The first `Scene::sample_acoustics` call with resident tiles allocates a scene-owned reduction pipeline, material tag masks, a 64-byte uniform buffer, a 6,144-byte result buffer, and three 6,144-byte asynchronous staging buffers. The pass is independent of fixed-step scheduling and never waits for its GPU results. The game controls observation frequency and source lifetime; see [simulation acoustics](@/external-api/acoustics.md).

## Game resource catalogue

`engine::resources::Resources` owns a loose catalogue or an in-memory package index. Keep one instance for the game lifetime. Each `ResourceReader` owns an independent bounded file handle or a decompressed byte cursor and may outlive the catalogue. Music workers own their readers; dropping a music stream cancels and joins its decoder outside the audio callback. See [Resources and distributions](@/external-api/resources-build.md).

GPU shader modules, layouts and compute/render pipelines created through the host factories
are cached for the lifetime of their `Accelerator` device, independently of scene lifetimes.
World buffers and textures remain scene-owned. See [GPU accelerator](@/internals/runtime/accelerator.md#pipeline-creation-diagnostics)
for cache identity, ownership, and compilation counters.
