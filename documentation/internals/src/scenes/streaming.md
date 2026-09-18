# Streaming and persistence

Streaming keeps a bounded GPU world around the active area. The scene follows
the target in batch-sized tile shifts. Before reusing outgoing ring slots it
downloads tile, fluid, and gas state into the owning CPU chunks; only after
those transfers are applied can incoming chunks be uploaded. Chunk loading,
generation, and saving happen through `ChunkEntry` states rather than blocking
the fixed simulation on filesystem work.

The transfer sequence for a shift is conceptually:

1. identify outgoing strips and incoming chunk-aligned area;
2. begin GPU downloads for tiles, particles, and gas cells;
3. apply completed downloads to CPU `Chunk` records;
4. save dirty chunks and dormant rigid owner files as needed;
5. remap the ring and load or generate incoming chunks;
6. upload tile fields and reconstruct fluid/gas/rigid state;
7. clear or rebuild collision and derived views for the new origin.

`SceneData` stores a serialized material registry, chunk files, and separate
rigid owner files. Fluid particles and gas cells that leave residency are
serialized as sparse dormant records in chunks. A rigid body has one owner
chunk even when its footprint overlaps other chunks; atomic owner-file writes
prevent partial persistence.

Streaming is asynchronous at the GPU and filesystem boundaries, but the scene
uses explicit pending queues and applies completions at safe update points.
Generation IDs and rigid identities prevent stale completions from mutating a
new occupant of a reused slot. The ring is a residency cache, not a second
world; non-resident coordinates cannot be queried from active buffers.

### Relevant implementation

- `engine_physics/src/scenes/scene_chunk_navigation.rs` — region movement and
  transfer ordering.
- `engine_physics/src/scenes/scene_chunk_streaming.rs` — chunk fetch, generate,
  save, and active-entry state transitions.
- `engine_physics/src/scenes_streaming/` — tile, fluid, and gas transfer jobs.
- `engine_physics/src/scene_data/scene_data_store.rs` — filesystem scene data
  and atomic rigid owner persistence.
- `engine_physics/src/chunks/` — serialized terrain and dormant fluid/gas data.
- `engine_physics/src/scenes/scene_rigid_dormancy.rs` — rigid gather and restore
  around residency changes.
