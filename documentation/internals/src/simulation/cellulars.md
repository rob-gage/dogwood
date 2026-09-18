# Cellular Simulation

Cellular simulation is the dense, cell-space part of the world model. A tile
contains an 8 by 8 array of cells. Resident tiles are packed into a ring of
GPU buffers so neighboring cells can be processed in parallel. A cell carries
its material identifier, appearance seed/value, amount, temperature, and
integrity; the exact set of fields differs slightly between static and dynamic
paths.

The persistent CPU form is `TileData` inside a `Chunk`. The resident GPU copy
is the simulation working set. It is allowed to change while a region is
resident, but the scene must download it before a ring slot is reused or a
chunk is saved. This makes GPU state authoritative for active cellular motion
while keeping chunk data authoritative for inactive persistence.

## Cellular Representation

The ring stores cells in tile-local order and uses a ring origin to translate
world coordinates into physical buffer coordinates. Material identifiers are
form-local packed values, while appearance is independent of material identity
so procedural variation survives movement and persistence. Amount represents
occupancy or quantity for a cell; integrity represents remaining structural
strength. Temperature is shared with the thermal system.

Static cells also participate in occupancy extraction. Dynamic cells use the
same coordinate domain but have movement and contact state that lets them
replace an available neighboring cell. Rigid bodies and actors are published
as temporary cell-space proxies. Those proxies allow common interaction
algorithms without making them persistent terrain.

### Relevant Implementation

- `engine_physics/src/tiles/tile_data.rs` — persistent per-tile cell fields and
  serialization.
- `engine_physics/src/tiles/tile_area.rs` — rectangular tile and ring regions.
- `engine_physics/src/simulation_cellulars/cellular_static_state.rs` — resident
  static-cell buffers and their shared cell layout.
- `engine_physics/src/simulation_cellulars/cellular_dynamic.rs` — dynamic-cell
  resources and movement dispatch.
- `engine_physics/src/simulation_utility/cell_coordinates.wgsl` — GPU world,
  tile, and cell coordinate conversion.

## Static Cellular Materials

Static cellular matter is terrain-like. It remains in a cell until an edit,
phase transition, reaction, or structural failure changes it. Its integrity is
the local resistance to damage; pressure transmission controls how much load
continues through it, and the low-pressure ignore threshold prevents numerical
noise from damaging every connected cell.

Pressure first records contacts and directional load, then transmits eligible
load through neighboring static cells. Damage reduces integrity. When a cell
fails, its material may produce a configured debris material and yield rather
than simply disappearing. The surviving static graph is checked for detached
components. A component can remain terrain, be converted into a rigid
cellular body, or be reduced to debris when it is below the configured minimum
body size.

Static occupancy is also the source for terrain collision extraction. The
physics world does not inspect the GPU cell buffer directly; it consumes a
delayed compact occupancy snapshot and rebuilds collision patches from it.

### Relevant Implementation

- `engine_physics/src/simulation_cellulars/cellular_static_state_gather.rs` —
  gathers static state needed by CPU-side scene operations.
- `engine_physics/src/simulation_cellulars/cellular_pressure_simulation.rs` —
  coordinates pressure/contact and damage passes.
- `engine_physics/src/scenes/scene_rigid_detachment.rs` — checks detached
  components and creates rigid bodies.
- `engine_physics/src/simulation_rigid_bodies/scene_physics_world_terrain.rs` —
  turns collision snapshots into CPU terrain patches.

## Dynamic Cellular Materials

Dynamic cellular materials are granular or otherwise mobile cell matter. Each
step evaluates gravity and the availability of neighboring cells, while
respecting contacts and pressure state. A movable cell can exchange places or
move into an available neighboring location; an occupied location becomes a
contact rather than an unconditional overwrite. Static cells therefore act as
support or obstacles, while dynamic cells can settle into spaces opened by
edits or destruction.

The dynamic pass operates in the GPU resident representation. The scene does
not attempt to mirror every intermediate move into `TileData`. On download,
the resulting material, amount, appearance, integrity, and temperature fields
are folded back into persistent tile representation. Dynamic material also
shares proxy and contact data with fluid and rigid-cellular paths, so a rigid
proxy can push granular matter without becoming a dynamic cell itself.

### Relevant Implementation

- `engine_physics/src/simulation_cellulars/cellular_dynamic.rs` — owns the
  dynamic buffers and dispatches movement.
- `engine_physics/src/simulation_cellulars/cellular_dynamic.wgsl` — implements
  resident dynamic-cell movement rules.
- `engine_physics/src/simulation_cellulars/cellular_physics_body_proxy.rs` —
  exposes actor and rigid shapes to cell-space contact passes.

## Pressure And Structural Damage

Pressure enters through contacts, configured edits, actor/rigid proxies, and
material reactions. The solver propagates directional pressure through cells
according to material transmission. A cell may retain part of the result so a
later stage can account for load that was not immediately transmitted. Contact
records distinguish solid-solid, proxy-solid, and other support relationships.

The damage decision combines transmitted pressure, the material's ignore
threshold, contact support, and integrity. This ordering matters: pressure is
not an immediate delete operation. It first becomes a load field, then a
material-specific integrity change, and only then a fracture/debris or
detachment request. Readback and scene application turn those requests into
CPU rigid bodies or queued edits at a safe boundary.

### Relevant Implementation

- `engine_physics/src/simulation_cellulars/cellular_pressure.rs` — pressure
  resource ownership and readback slots.
- `engine_physics/src/simulation_cellulars/cellular_pressure_operations.rs` —
  pressure pass setup and result handling.
- `engine_physics/src/simulation_cellulars/cellular_pressure_runtime.rs` —
  runtime pressure dispatch and synchronization.
- `engine_physics/src/simulation_cellulars/
  cellular_pressure_shader_propagation.wgsl` —
  directional pressure propagation.
- `engine_physics/src/simulation_cellulars/
  cellular_pressure_shader_damage.wgsl` —
  integrity and fracture evaluation.

## Cellular Collision Extraction

Rapier owns CPU collision objects, while active cellular occupancy lives in GPU
buffers. Direct per-cell Rapier queries would cross the device boundary and
create an unbounded number of CPU colliders. Instead, the GPU extracts
occupancy into a compact snapshot. The scene consumes completed snapshots to
update terrain collision patches.

Snapshots are delayed. They carry the ring origin and validation information
needed to prove that their cells still refer to the current physical slots.
When the active area moves, a snapshot for the old origin is rejected rather
than being applied to a new world location. This delay is a deliberate
performance and correctness boundary: gameplay and actors see the latest
accepted CPU collision view, not an instantaneous GPU query.

### Relevant Implementation

- `engine_physics/src/simulation_cellulars/cellular_collision.rs` — occupancy
  extraction resources and dispatch.
- `engine_physics/src/simulation_cellulars/collision_occupancy_snapshot.rs` —
  snapshot data and origin validation.
- `engine_physics/src/simulation_cellulars/collision_readback_slot.rs` —
  bounded asynchronous GPU-to-CPU readback.
- `engine_physics/src/simulation_rigid_bodies/
  static_terrain_collision_patch.rs` —
  CPU collision patch representation.

## Cellular Proxies And Other Material Forms

Actors and rigid bodies retain CPU physics ownership, then publish rasterized
occupancy into the cellular domain for contact and pressure. Fluid particles
publish a derived coverage/material view for solid collision, sampling, and
rendering. Gas fields participate through their own Eulerian buffers and
material mutation/thermal interfaces rather than becoming cellular cells.

These are doubles: they provide a common interaction surface, but a proxy
cannot be saved as terrain or used to reconstruct the authoritative actor,
fluid, gas, or rigid state. Upload and readback stages explicitly identify
which owner a result belongs to before applying it.

### Relevant Implementation

- `engine_physics/src/simulation_cellulars/
  cellular_physics_body_proxy_construction.rs` —
  builds proxy resources for actors and rigid bodies.
- `engine_physics/src/simulation_cellulars/cellular_physics_body_proxy.wgsl` —
  rasterizes proxy shapes into cell space.
- `engine_physics/src/simulation_rigid_bodies/rigid_cell_state_upload.rs` —
  publishes rigid body-local cells into the cellular domain.
- `engine_physics/src/simulation_fluids/fluids_operations.rs` — builds fluid
  coverage and contact fields consumed by neighboring systems.
