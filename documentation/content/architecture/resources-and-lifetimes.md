+++
title = "Resources and lifetimes"
description = "How a scene creates, shares, saves, and releases its world resources."
weight = 50
[extra]
sources = ["engine_world/src/scenes/fluids", "engine_world/src/scenes/pressure", "engine_world/src/scenes/collisions/collision.rs", "engine_world/src/scenes/scene.rs", "engine_world/src/scenes/chunks/chunk_streaming_state.rs", "engine_world/src/scenes/cellular_tiles/cellular_tile_streaming_state.rs", "engine_world/src/scenes/mod.rs", "engine_world/src/scenes/collisions/mod.rs", "engine_world/src/scenes/cellular_tiles/cellular_tile.wgsl", "engine_host/src/acceleration/accelerator.rs"]
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
└── CellularTiles
```

### Lazy initialization

`Scene::new` leaves chunk streaming, tile buffers, tile streaming, collision, and kinematics absent. The first `Scene::stream` creates chunk streaming and the GPU tile ring, sized to twice the first camera's unzoomed width and height. Later requested regions must keep those dimensions.

Collision and kinematics are created on the first `simulate` call with an active region; pressure simulation is created on that call after fluids. Tile buffers retain a cloned `Accelerator`; its clone holds handles to the same WGPU instance, adapter, device, and queue, so teardown can use that context. See the [scene pipeline](@/architecture/scene-pipeline.md) for call order.

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

| `COLLISION_RIGID_BODY_COUNT_MAXIMUM` | 1024 | Rigid bodies simulated at once |
| `COLLISION_RIGID_BODY_CELLULAR_TILE_COUNT_MAXIMUM` | 16384 | Tiles held across simulated rigid bodies |

### Defaults and reserved collision groups

| Constant | Value | Meaning |
| --- | ---: | --- |
| `SCENE_GRAVITY_DEFAULT` | 50.0 | Default downward acceleration, in tiles per second squared |
| `SCENE_AMBIENT_GAS_PRESSURE_DEFAULT` | 1.0 | Default phantom air pressure, in atmospheres |
| `SCENE_AMBIENT_GAS_MASS_DEFAULT` | 29.0 | Default mass of one atmosphere of phantom air |
| `SCENE_CHUNK_THREAD_POOL_NAME` | `"chunk_io"` | Pool for background chunk loads, generation, and saves |
| `COLLISION_GROUP_CELLULAR_DYNAMIC` | `1 << 27` | Settled dynamic cells collision group |
| `COLLISION_GROUP_ACTOR` | `1 << 28` | Actor colliders collision group |
| `COLLISION_GROUP_CELLULAR_TILE` | `1 << 29` | Solid cells in active cellular tiles |
| `COLLISION_GROUP_RIGID_BODY` | `1 << 30` | Cellular rigid body collision group |
| `COLLISION_GROUP_EDIT` | `1 << 31` | Colliders skipped by edit insertion |

Among the collision constants above, only `COLLISION_RIGID_BODY_COUNT_MAXIMUM` and the five `COLLISION_GROUP_*` constants are public; see [collision setup](@/examples/collision-setup.md). The other constants are private.

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
| `FLUID_COHESION_SCALE` | 0.5 |
| `FLUID_COHESION_NORMALIZATION_2D` | 54.584815 |
| `FLUID_VELOCITY_MAXIMUM` | 4 cells/tick (vector-length cap in the particle solver) |
| `FLUID_RASTER_SCALE` | 4096 |
| `FLUID_WORKGROUP_SIZE` | 128 |
| `FLUID_SCAN_WORKGROUP_SIZE` | 256 |
| `FLUID_TIME_STEP` | 1/60 second |

The private `FLUID_ACTOR_CONTACT_SCALE` is `1024`. Count, actor, and import readbacks each have three staging slots; the resident save has one separate snapshot slot. [GPU layouts](@/internals/shaders/fluid-shader.md) document record sizes and region/pool scaling. The collision-side `RIGID_BODY_FLUID_CURRENT_COUPLING` is `1.0`.

## Pressure resources

Pressure simulation allocates a scratch gas buffer the size of the tile ring's gas buffer, an eight-byte open-volume record per ring cell, per-tile control storage, and the published pressure buffer. Its first `simulate` creates them; they live until the scene drops. Public constants in `engine_world/src/scenes/pressure/mod.rs`:

| Constant | Value |
| --- | --- |
| `GAS_AMOUNT_PER_ATMOSPHERE` | 4096 |
| `GAS_AMOUNT_MAXIMUM` | `0xff_ffff` (about 256 atmospheres in one species slot) |
| `PRESSURE_MAXIMUM` | 4095 atm per cell |
| `PRESSURE_BROKEN_CELLULAR_TILE_COUNT_MAXIMUM` | 1024 tiles before full occupancy refresh |
| `PRESSURE_BROKEN_RIGID_BODY_CELL_COUNT_MAXIMUM` | 4096 cells reported per tick |

Pressure uses a 1/60-second tick and 12 substeps; per-second coefficients are listed on the [pressure simulation](@/internals/simulation/pressure.md#flows-volume-and-sleeping) page. See the [pressure shader](@/internals/shaders/pressure-shader.md) for its tuning constants.
