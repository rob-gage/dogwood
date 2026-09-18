# Fluids

`Fluids` owns fixed-capacity GPU particle records. Each active record contains
material, alive state, position, velocity, amount, and temperature. A free
index stack allocates and recycles particles; cell edits are converted into
bounded spawn/erase requests against that pool.

The current solver is a GPU particle-based fluid pass using PBF-style density
constraints. Each substep predicts positions, rebuilds support-radius spatial
buckets, computes density multipliers, applies race-free position corrections,
projects particles from solid boundaries, commits velocity, and applies a
neighbor-weighted smoothing correction. The engine intentionally exposes the
pipeline rather than its equation details.

Fluid particles are authoritative for active fluid motion. A ring-aligned
derived cellular representation records dominant material, coverage, weighted
velocity, and thermal information. It is used for rendering, cellular contact,
actor swimming/body interaction, and mechanical response; it is not the source
of particle positions.

Actor and rigid collision geometry enters through cellular/body proxies and
fluid contact fields. Mechanical response is scattered from the cellular
contact result back to particles. A possessed pawn can also request a compact
fluid sample used by swimming behavior.

When tiles leave residency, an export compacts particles by outgoing region and
serializes dormant particle records into the owning chunk. Incoming records are
reconstructed through the same free-index allocator. The active-area margin is
larger than the visible region so support neighborhoods and bounded movement do
not immediately cross an unresident boundary.

### Relevant implementation

- `engine_physics/src/simulation_fluids/fluids.rs` — particle buffers, derived
  fields, and solver resources.
- `engine_physics/src/simulation_fluids/fluids_operations.rs` — edits, PBF
  substeps, contact, rasterization, and sampling dispatch.
- `engine_physics/src/simulation_fluids/fluids_streaming.rs` — particle export
  and import.
- `engine_physics/src/simulation_fluids/fluid_authority_view.rs` — internal
  view of particle authority and spatial buckets.
- `engine_physics/src/scenes/scene_fluid_streaming.rs` — scene-side transfer
  scheduling and completion application.
