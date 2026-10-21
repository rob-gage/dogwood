+++
title = "World internals"
description = "Scene-owned streaming and simulation state, its initialization, and its update flow."
weight = 40
[extra]
sources = ["engine_world/src/scenes", "engine_world/src/actors", "engine_world/src/materials"]
+++

This page is for engine contributors. It describes the private state a `Scene` owns, when that state is created, and the internal calls behind `Scene::stream` and `Scene::simulate`. Public contracts are in [Scenes](@/external-api/scenes.md).

## Scene ownership

`Scene` coordinates actor data, material lookup, persistence, tile residency, and simulation. Its private state is split by lifecycle:

- Chunk streaming, GPU tile buffers, tile streaming, fluids, collision, and kinematics start absent.
- Edit streaming is initialized immediately with an empty queue.
- The material table and generator are shared with chunk work through `Arc` handles.

```text
Scene
├── ActorRegistry                 created in Scene::new
├── Arc<MaterialTable>            cloned into chunk streaming; borrowed
│                                 when constructing kinematics
├── ChunkData                     persistent storage handle
├── Option<ChunkStreamingState>   first Scene::stream
├── Option<CellularTileBuffers>   first Scene::stream
├── Option<CellularTileStreamingState> first Scene::stream
├── Option<Fluids>                first Scene::stream
│   └── FluidParticlePass
├── EditStreamingState            created in Scene::new
│   └── Option<EditCellularTilePass>
│       created on first streamed edit with tiles to write
├── Option<Collision>             first Scene::simulate with active region
│   ├── CollisionCellularTilePass
│   └── CollisionRigidBodyPass
└── Option<Kinematics>            first Scene::simulate with active region
    └── KinematicsPass
```

The pass objects are owned by their parent systems:

- `Collision` manages actor and rigid-body Rapier state plus cellular occupancy.
- `Kinematics` manages dynamic-cell motion and cellular/fluid coupling through `KinematicsPass`.
- `Fluids` manages particle admission, simulation, and fluid actor feedback through `FluidParticlePass`.

`Scene` passes shared tile buffers to both and, when creating kinematics, the collision system's rigid-body pass. For ordering and resource lifetime, see [Scene update pipeline](@/architecture/scene-pipeline.md) and [Resources and lifetimes](@/architecture/resources-and-lifetimes.md).

## Initialization and call flow

### Streaming

`Scene::stream` derives a camera-centered requested region and coordinates chunk residency with the GPU tile ring; see the [Scene update pipeline](@/architecture/scene-pipeline.md) for call order and [Tile streaming](@/internals/world-streaming/tile-streaming.md) for region movement. Both streaming advances are nonblocking; they incorporate completed work and schedule or advance remaining transfers. Chunk state is covered in [Chunk residency](@/internals/world-streaming/chunk-residency.md).

### Simulation

`Scene::simulate` returns early until an active region exists, then lazily creates `Collision` and `Kinematics`. It first calls `Kinematics::simulate_kinematics`, which returns cellular and rigid-body contact maps. It polls fluid actor feedback, passes the contact maps to `Collision::simulate_collisions`, then runs the particle solver. See [scene ordering](@/architecture/scene-pipeline.md#simulation-order).

[Collision world](@/internals/simulation/collision-world.md), [Cellular kinematics](@/internals/simulation/cellular-kinematics.md), and [Edit pipeline](@/internals/simulation/edit-pipeline.md) describe those systems in depth.

### Drop

`Scene::drop` applies pending edits and returns active tiles to chunks when all three streaming resources exist. It then returns unspawned imports and live particles before chunk-state teardown. Errors in either return path panic.

## Internal entry points

The generated private docs expose each type's API and fields. The `Scene` definition is in `engine_world/src/scenes/scene.rs`; child module files own their subsystem state.

{{ <rustdoc path="engine_world/scenes/struct.Scene.html" text="Scene" private={true} /> }} · {{ <rustdoc path="engine_world/scenes/chunks/struct.ChunkStreamingState.html" text="ChunkStreamingState" private={true} /> }} · {{ <rustdoc path="engine_world/scenes/cellular_tiles/struct.CellularTileStreamingState.html" text="CellularTileStreamingState" private={true} /> }} · {{ <rustdoc path="engine_world/scenes/edits/edit_streaming_state/struct.EditStreamingState.html" text="EditStreamingState" private={true} /> }} · {{ <rustdoc path="engine_world/scenes/kinematics/kinematics/struct.Kinematics.html" text="Kinematics" private={true} /> }} · {{ <rustdoc path="engine_world/scenes/collisions/collision/struct.Collision.html" text="Collision" private={true} /> }}

### Handoffs

| Boundary | Shared data | Reference |
| --- | --- | --- |
| Scene → tile streaming | Tile arrays, retained chunk regions, fluid import/export state | [Tile streaming](@/internals/world-streaming/tile-streaming.md) |
| Kinematics → collision | Actor contacts, rigid-body impulses, holding contacts, fluid moments | [Contact exchange](@/internals/simulation/contact-exchange.md) |
| Kinematics ↔ particles | Fluid raster/contact deltas, actor records, rigid-body motion | [Fluid interactions](@/internals/simulation/fluid-coupling.md) |
| Particles ↔ chunks | Dormant imports, outgoing exports, resident-save snapshots | [Fluid persistence](@/internals/world-streaming/fluid-streaming.md) |

The implementation signatures are available in rustdoc above; these pages explain the units, ownership, and timing that signatures alone do not express. Fallible handoffs propagate `SceneError` to the caller.
