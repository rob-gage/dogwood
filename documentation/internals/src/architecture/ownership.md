# Ownership, Coordinates, And CPU/GPU Split

## Coordinates And Storage Units

World positions use tile units. A tile is 8 by 8 cells. A chunk is 64 by 64
tiles, so it is 512 by 512 cells. `TileCoordinates` and `CellCoordinates` are
integer world coordinates; `ScenePosition` stores a tile coordinate plus a
normalized offset, and `SceneVelocity` is tiles per simulation second.

The resident GPU area is a rectangular tile ring around the active simulation
area. Ring offsets let the scene move the logical origin without copying the
whole buffer. `TileArea` is the shared rectangle type for active, buffered,
streaming, and download/upload regions.

## Authoritative Representations

- `SceneData` and `Chunk` own inactive persistent world state on the CPU.
- `Scene` owns active CPU metadata, actors, rigid bodies, and transfer queues.
- Resident static/dynamic cell fields, pressure, thermal fields, gas fields,
  and fluid particles are Accelerator buffers while active.
- Fluids own authoritative resident particle records on the GPU and derive a
  ring-aligned cellular view for collision and rendering.
- Gases own authoritative Eulerian velocity, concentration, and temperature
  fields on the GPU.
- Rigid bodies own body-local cells and transforms on the CPU physics side;
  rasterized cells are a GPU-visible double used by cellular stages.

CPU code must not read a GPU result before a submission has completed and a
readback has been mapped. The scene uses `Accelerator::poll`, staging buffers,
generation/identity checks, and pending queues to enforce this. A moving ring
must download outgoing state before its physical slots are reused for incoming
world coordinates.

### Relevant Implementation

- `engine_physics/src/scene_geometry/scene_position.rs` — continuous tile-space
  actor positions and render interpolation.
- `engine_physics/src/tiles/` — cells, tiles, areas, and ring coordinates.
- `engine_physics/src/chunks/chunk.rs` — persistent 64-by-64-tile chunk state.
- `engine_physics/src/scenes/scene.rs` — resident buffers and ownership fields.
- `engine_physics/src/scenes/scene_chunk_navigation.rs` — active/buffered ring
  movement and transfer ordering.
