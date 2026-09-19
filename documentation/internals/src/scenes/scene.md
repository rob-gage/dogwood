# Scene Model

`Scene` is the runtime world coordinator. Construction validates
`SceneSimulationConfiguration`, loads or creates `SceneData`, allocates the
resident buffers, builds material-derived GPU tables, and initializes actors,
physics, fluid, gas, reaction, thermal, collision, and transfer resources.

The world hierarchy is:

```text
Scene
  active tile area (simulation)
    8x8 cells per tile
  buffered tile ring (GPU residency and transfer margin)
    64x64 tiles per persistent chunk
```

`Chunk` is authoritative for inactive terrain and dormant fluid/gas records.
`TileData` stores persistent per-cell material, appearance, integrity, amount,
and temperature. Active cell arrays are packed into Accelerator buffers in the
same logical row-major tile/cell layout, with ring offsets translating logical
world coordinates to physical slots.

`SceneGenerator` supplies missing chunks. `ChunkGenerationRegion` gives exact
absolute tile/cell bounds and `ChunkInitializationWriter` writes directly into
the new `Chunk`; this bypasses `SceneEditBatch` and per-cell simulation work.
Generated chunks become normal active or persistent chunks; generation is not a
separate simulation authority. Persisted chunks, including empty ones, always
take precedence over generation.
`SceneEditBatch` is the common ingress for gameplay/editor placements, erases,
destruction, thermal edits, and rigid-body placement. The scene applies batches
at update boundaries so edits are ordered relative to GPU stages.

`Scene` also owns possession and actor registries, collision snapshots, contact
event accumulation, render views, and the active region target. A possessed
actor normally drives region following, while `request_area_around` can request
an explicit recenter.

### Relevant Implementation

- `engine_physics/src/scenes/scene.rs` — central state and subsystem fields.
- `engine_physics/src/scenes/scene_construction.rs` — resource construction and
  initial uploads.
- `engine_physics/src/scenes/scene_construction_entrypoints.rs` — public scene
  construction entry points.
- `engine_physics/src/scenes/scene_generator.rs` — missing-chunk generation
  contract.
- `engine_physics/src/scene_editing/` — queued edit representation.
- `engine_physics/src/scenes/scene_cell_editing.rs` — applying edits to active
  cellular, fluid, and rigid representations.
