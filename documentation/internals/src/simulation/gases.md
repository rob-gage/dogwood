# Gases

Gases use an Eulerian field over resident physical cells. One shared velocity
field is paired with species-major concentration fields and one shared gas
temperature field. Each registered gas material is a species; its metadata
controls density, diffusivity, optical extinction, dissipation, and
compressibility.

The tick splits gas work around material coupling. Pre-coupling advects the
velocity field, calculates curl, and applies gravity/buoyancy and vorticity
confinement. Material reactions and cellular/fluid stages can then write gas
or thermal effects. Post-coupling computes divergence, solves pressure by
Jacobi iterations, projects velocity, advects/diffuses concentrations, and
commits the concentration scratch field.

Gas concentration is the authoritative active representation. Rendering reads
the concentration buffer and material graphics to produce species appearance;
the renderer does not reconstruct gas from chunk records. Gas cells outside the
resident ring are sparse `ChunkGasCell` records containing enough state to
restore concentration and temperature later.

Material mutation supports cross-form requests, including gas/fluid
condensation. Phase/reaction code writes requests with an explicit authority
locator; the mutation stage resolves them against the correct cell, fluid
particle, or gas species rather than assuming all matter is cellular.

### Relevant implementation

- `engine_physics/src/simulation_gases/gases.rs` — Eulerian fields and solver
  resources.
- `engine_physics/src/simulation_gases/gases_operations.rs` — pre/post coupling
  dispatch, projection, transport, and area clearing.
- `engine_physics/src/simulation_gases/gases_streaming.rs` — gas export/import
  packing.
- `engine_physics/src/chunks/chunk_gas_cell.rs` — dormant gas serialization.
- `engine_physics/src/scenes/scene_gas_streaming.rs` — scene transfer boundary.
- `engine/src/renders/scene_renderer.rs` — binds gas concentration for drawing.
