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

- Chunk streaming, GPU tile buffers, tile streaming, collision, and kinematics start absent.
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
├── EditStreamingState            created in Scene::new
│   └── Option<EditCellularTilePass>
│       created on first streamed edit with tiles to write
├── Option<Collision>             first Scene::simulate with active region
│   ├── CollisionCellularTilePass
│   └── CollisionRigidBodyPass
└── Option<Kinematics>            first Scene::simulate with active region
    └── KinematicsCellularTilePass
```

The pass objects are owned by their parent systems:

- `Collision` manages actor and rigid-body Rapier state plus cellular occupancy.
- `Kinematics` manages GPU dynamic-cell simulation and contact readback.

`Scene` passes shared tile buffers to both and, when creating kinematics, the collision system's rigid-body pass. For ordering and resource lifetime, see [Scene update pipeline](@/architecture/scene-pipeline.md) and [Resources and lifetimes](@/architecture/resources-and-lifetimes.md).

## Initialization and call flow

### Streaming

`Scene::stream` derives a camera-centered requested region and coordinates chunk residency with the GPU tile ring; see the [Scene update pipeline](@/architecture/scene-pipeline.md) for call order and [Tile streaming](@/internals/world-streaming/tile-streaming.md) for region movement. Both streaming advances are nonblocking; they incorporate completed work and schedule or advance remaining transfers. Chunk state is covered in [Chunk residency](@/internals/world-streaming/chunk-residency.md).

### Simulation

`Scene::simulate` returns early until an active region exists, then lazily creates `Collision` and `Kinematics`. It first calls `Kinematics::simulate_kinematics`, which returns cellular and rigid-body contact maps. It passes those maps to `Collision::simulate_collisions`.

[Collision world](@/internals/simulation/collision-world.md), [Cellular kinematics](@/internals/simulation/cellular-kinematics.md), and [Edit pipeline](@/internals/simulation/edit-pipeline.md) describe those systems in depth.

### Drop

`Scene::drop` applies pending edits and returns active tiles to chunks when all three streaming resources exist. Errors in this path panic.

## Internal entry points

The generated private docs expose each type's API and fields. The `Scene` definition is in `engine_world/src/scenes/scene.rs`; child module files own their subsystem state.

{{ <rustdoc path="engine_world/scenes/struct.Scene.html" text="Scene" private={true} /> }} · {{ <rustdoc path="engine_world/scenes/chunks/struct.ChunkStreamingState.html" text="ChunkStreamingState" private={true} /> }} · {{ <rustdoc path="engine_world/scenes/cellular_tiles/struct.CellularTileStreamingState.html" text="CellularTileStreamingState" private={true} /> }} · {{ <rustdoc path="engine_world/scenes/edits/edit_streaming_state/struct.EditStreamingState.html" text="EditStreamingState" private={true} /> }} · {{ <rustdoc path="engine_world/scenes/kinematics/kinematics/struct.Kinematics.html" text="Kinematics" private={true} /> }} · {{ <rustdoc path="engine_world/scenes/collisions/collision/struct.Collision.html" text="Collision" private={true} /> }}

### Streaming signatures

```text
ChunkStreamingState::stream_chunks(
    &mut self, generator: &Arc<dyn SceneGenerator>, world_seed: u64
) -> Result<(), SceneError>

CellularTileStreamingState::stream_cellular_tiles(
    &mut self, accelerator: &Accelerator,
    cellular_tile_buffers: &CellularTileBuffers,
    chunk_streaming_state: &mut ChunkStreamingState
) -> Result<(), SceneError>

EditStreamingState::stream_edits(
    &mut self, accelerator: &Accelerator,
    cellular_tile_buffers: &CellularTileBuffers,
    cellular_tile_streaming_state: &CellularTileStreamingState,
    actor_registry: &ActorRegistry, collision: Option<&mut Collision>
) -> Result<(), SceneError>
```

### Simulation signatures

```text
Kinematics::simulate_kinematics(
    &mut self, accelerator: &Accelerator,
    active_region: CellularTileRegion, ring_offset: (u32, u32),
    region_origin: (i32, i32), gravity: f32,
    actor_registry: &ActorRegistry, rigid_bodies: &[(u32, u64)]
) -> Result<(
    HashMap<Actor, ([f32; 2], [f32; 2], f32)>,
    HashMap<u64, ([f32; 2], f32, [[f32; 5]; 2])>
), SceneError>

Collision::simulate_collisions(
    &mut self, accelerator: &Accelerator,
    active_region: CellularTileRegion, ring_offset: (u32, u32),
    gravity: f32, actor_registry: &mut ActorRegistry,
    cellular_contacts: &HashMap<Actor, ([f32; 2], [f32; 2], f32)>,
    rigid_body_contacts: &HashMap<u64, ([f32; 2], f32, [[f32; 5]; 2])>,
    material_table: &MaterialTable
) -> Result<(), SceneError>
```

The first map is keyed by `Actor`; its values contain velocity change, contact normal, and contact depth. The second is keyed by rigid-body ID; its values contain impulse, angular impulse, and holding-cell data. See [Cell and body contact exchange](@/internals/simulation/contact-exchange.md) for tuple layouts, units, and timing.

Each fallible call propagates its `SceneError`. See [Scenes](@/external-api/scenes.md) and [Collisions and edits](@/external-api/collisions-and-edits.md) for public contracts.
