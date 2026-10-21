+++
title = "Resources and lifetimes"
description = "How a scene creates, shares, saves, and releases its world resources."
weight = 50
[extra]
sources = ["engine_world/src/scenes/scene.rs", "engine_world/src/scenes/chunks/chunk_streaming_state.rs", "engine_world/src/scenes/cellular_tiles/cellular_tile_streaming_state.rs", "engine_world/src/scenes/mod.rs", "engine_world/src/scenes/collisions/mod.rs", "engine_world/src/scenes/kinematics/kinematics_cellular_tile.wgsl", "engine_host/src/acceleration/accelerator.rs"]
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
└── Kinematics
```

### Lazy initialization

`Scene::new` leaves chunk streaming, tile buffers, tile streaming, collision, and kinematics absent. The first `Scene::stream` creates chunk streaming and the GPU tile ring, sized to twice the first camera's unzoomed width and height. Later requested regions must keep those dimensions.

Collision and kinematics are created on the first `simulate` call with an active region. Tile buffers retain a cloned `Accelerator`; its clone holds handles to the same WGPU instance, adapter, device, and queue, so teardown can use that context. See the [scene pipeline](@/architecture/scene-pipeline.md) for call order.

## Saving and drop behavior

### Saving

`Scene::save` requests persistence. If tile streaming exists, it first requests return of resident tiles to their chunks; otherwise it asks existing chunk streaming to save. `Scene::is_saved` reports whether the tile and chunk save requests have cleared. These requests progress during later streaming calls rather than blocking.

### Drop

Dropping a `Scene` blocks in two steps:

1. If the scene has chunk streaming, tile buffers, and tile streaming state, `Scene::drop` applies queued edits and returns resident tiles to chunks. Failure in either step panics with `Scene cellular tiles could not be returned`.
2. `ChunkStreamingState::drop` waits for outstanding normal and copy saves, then synchronously saves the dirty chunks it still owns. It makes up to three attempts per dirty chunk, including the first, and panics with `Scene chunks could not be saved` if any still fail.

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
| `COLLISION_RIGID_BODY_HOLDING_ITERATION_COUNT` | 4 | Passes holding rigid bodies against dynamic cells |
| `COLLISION_RIGID_BODY_COUNT_MAXIMUM` | 1024 | Rigid bodies simulated at once |
| `COLLISION_RIGID_BODY_CELLULAR_TILE_COUNT_MAXIMUM` | 16384 | Tiles held across simulated rigid bodies |

### Defaults and reserved collision groups

| Constant | Value | Meaning |
| --- | ---: | --- |
| `SCENE_GRAVITY_DEFAULT` | 50.0 | Default downward acceleration, in tiles per second squared |
| `SCENE_CHUNK_THREAD_POOL_NAME` | `"chunk_io"` | Pool for background chunk loads, generation, and saves |
| `COLLISION_GROUP_CELLULAR_DYNAMIC` | `1 << 27` | Settled dynamic cells collision group |
| `COLLISION_GROUP_ACTOR` | `1 << 28` | Actor colliders collision group |
| `COLLISION_GROUP_CELLULAR_TILE` | `1 << 29` | Solid cells in active cellular tiles |
| `COLLISION_GROUP_RIGID_BODY` | `1 << 30` | Cellular rigid body collision group |
| `COLLISION_GROUP_EDIT` | `1 << 31` | Colliders skipped by edit insertion |

Only `COLLISION_RIGID_BODY_COUNT_MAXIMUM` and the five `COLLISION_GROUP_*` constants are public; see [collision setup](@/examples/collision-setup.md). The other constants are private.
