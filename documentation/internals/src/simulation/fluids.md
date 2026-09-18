# Fluids

Fluids are represented as particles while resident. The particle population
is fixed-capacity GPU storage; a free-index structure distinguishes live and
available entries. A particle records material, position, velocity, amount,
and temperature. Particle state is authoritative for fluid motion. The
cellular raster derived from those particles is an interaction and rendering
view, not a second authority.

## Fluid Authority

The solver updates particle positions and velocities directly. This preserves
sub-cell motion and avoids treating an 8 by 8 raster as if it could represent
all fluid movement. Cell placement, material reactions, phase changes, and
streaming can create or remove particles, but once a particle is live its
position and velocity come from the particle solver.

## Particle Storage And Allocation

Particle storage is preallocated for the scene's configured capacity. An entry
is either free or live; allocation takes a free index and erasing returns the
index to the pool. Scene edits and cross-form mutations become bounded
spawn/erase requests. Each request carries material, amount, temperature, and
a position or source cell, so the allocator can reconstruct the same
information after streaming.

### Relevant Implementation

- `engine_physics/src/simulation_fluids/fluids.rs` — particle buffers, free
  entries, derived fields, and solver resources.
- `engine_physics/src/simulation_fluids/fluids_construction.rs` — capacity and
  buffer initialization.
- `engine_physics/src/simulation_fluids/fluids_operations.rs` — spawn/erase,
  simulation, sampling, and rasterization dispatch.

## Spatial Neighborhood Search

PBF-style constraints need nearby particles, but a full pairwise search would
grow quadratically with particle count. Dogwood groups particles into spatial
bucket data using the current support region. A particle's bucket identifies
the neighboring buckets that can contain relevant particles, limiting density,
collision, and smoothing work to a local neighborhood.

The structure is rebuilt after predicted positions change enough to affect
neighborhood membership. It is a solver workspace, not persistent fluid data;
streaming and persistence serialize particles, not the buckets.

### Relevant Implementation

- `engine_physics/src/simulation_fluids/fluid_authority_view.rs` — internal
  authority view and neighborhood resources.
- `engine_physics/src/simulation_utility/fluid_spatial.wgsl` — shared spatial
  bucket and support-radius helpers.
- `engine_physics/src/simulation_fluids/
  fluids_shader_particle_operations.wgsl` —
  particle-neighborhood operations.

## Predicted Motion

Each fluid substep first predicts positions from velocity, gravity, and body
forces. Prediction separates external acceleration from the density solve: the
constraint stage can correct overlap and restore volume without losing the
effect of gravity. The neighborhood structure is then rebuilt from predicted
positions before density constraints are evaluated.

## Density Constraint Solve

The current solver follows a position-based-fluid process. It estimates local
density from neighboring particles, evaluates how far each particle is from
the configured rest-density constraint, and calculates a correction shared by
the participants. It applies the correction, repeats the constraint work as
configured, and uses corrected positions as the substep result.

The important architectural point is that the solve is local and iterative: it
maintains liquid volume by removing excessive compression rather than
integrating a pressure scalar into a permanent cell field. The implementation
uses GPU scratch buffers and synchronization between passes; callers should
not assume one dispatch produces final positions.

### Relevant Implementation

- `engine_physics/src/simulation_fluids/fluids_operations.rs` — orders the
  predicted-position, density, correction, and commit stages.
- `engine_physics/src/simulation_fluids/
  fluids_shader_particle_operations.wgsl` —
  density and position-correction kernels.

## Solid Collision Handling

Static terrain is supplied through the extracted occupancy/collision view.
Actors and rigid bodies are supplied through cell-space proxy fields. After
position constraints, particles are projected away from solid occupancy so a
corrected position does not remain inside terrain or a body proxy. Final
velocity is reconstructed from displacement over the substep, then
neighbor-weighted smoothing is applied.

This ordering matters: collision projection must affect the displacement used
to compute velocity, otherwise particles would leave a solid visually while
retaining a velocity that immediately re-enters it.

## Velocity Smoothing

The solver applies an XSPH-style neighbor correction to reduce noisy relative
velocity between nearby particles. It is a stabilizing and visual-quality
step after position solving, not the authority for particle location. Body
contacts and actor swimming consume the resulting sampled or rasterized view.

## Fluid Cellular Representation

The fluid raster maps particles into the resident cell domain. It records the
dominant material, coverage/amount, weighted velocity, and thermal
information. Cellular pressure/contact stages use it to find fluid-solid
interaction; pawn swimming samples it; rendering uses it for coverage and
appearance. A cell may contain contributions from several particles, so this
view cannot reconstruct exact particle positions or allocation state.

### Relevant Implementation

- `engine_physics/src/simulation_fluids/fluids_operations.rs` — rasterization,
  contact response, and pawn sampling.
- `engine_physics/src/simulation_fluids/fluids_shader_sampling.wgsl` — fluid
  sampling and derived-cell operations.
- `engine_physics/src/simulation_fluids/fluids_shader_collision.wgsl` — solid
  projection and contact response.

## Interaction With Actors And Rigid Bodies

Actor and rigid transforms remain authoritative in CPU physics. Their proxy
occupancy gives particles a collision surface, while fluid contact fields can
produce push or drag information applied to fluid velocity. Pawn swimming uses
a compact fluid sample to select buoyancy/drag behavior. Neither the sample
nor the proxy replaces the actor body or the fluid particle records.

## Streaming

When particles leave the buffered region, a GPU export selects outgoing live
entries and packs dormant records into their owning chunks. The active entry is
then returned to the free pool. Import allocates new slots and reconstructs
position, velocity, amount, material, and temperature. The simulation buffer
includes a margin beyond the visible active area so support neighborhoods and
short-term motion do not immediately depend on unresident particles.

### Relevant Implementation

- `engine_physics/src/simulation_fluids/fluids_streaming.rs` — export/import
  packing and particle residency operations.
- `engine_physics/src/scenes_streaming/fluid_download.rs` — asynchronous
  outgoing-particle readback.
- `engine_physics/src/scenes_streaming/fluid_upload.rs` — incoming-particle
  upload and validation.
- `engine_physics/src/scenes/scene_fluid_streaming.rs` — scene ordering and
  completion application.
