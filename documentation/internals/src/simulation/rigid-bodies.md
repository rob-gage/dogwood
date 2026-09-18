# Rigid Cellular Bodies

Rigid cellular bodies preserve a connected collection of material cells while
giving that collection a transform and CPU rigid-body motion. They bridge
cellular composition and ordinary collision physics. They are not terrain,
not granular particles, and not generic Rapier bodies with a decorative
texture.

## Why Rigid Cellular Bodies Exist

Static terrain is world-anchored and represented by resident cell fields.
Dynamic cells can move cell by cell and exchange neighboring locations. A
generic Rapier body only owns shape, transform, and velocity. A rigid cellular
body additionally preserves which material cells it contains, their appearance,
integrity, amount, temperature, and reaction/phase behavior. This allows a
rock or machine made from several materials to move as one body while still
participating in cellular pressure, thermal, and material systems.

## Sources Of Rigid Bodies

Bodies can be authored or placed explicitly through scene editing. They can
also arise when static terrain loses support: the scene gathers the detached
component, checks connectivity and the configured minimum size, and promotes a
large enough component to a body. Small components are discarded or converted
through debris metadata instead of creating a body that cannot participate
meaningfully in physics.

### Relevant Implementation

- `engine_physics/src/simulation_rigid_bodies/rigid_cellular_body.rs` — rigid
  body identity, transform, and body-local state.
- `engine_physics/src/simulation_rigid_bodies/rigid_cellular_body_cell.rs` —
  body-local material-cell data.
- `engine_physics/src/scenes/scene_rigid_detachment.rs` — static-component
  detachment and body creation.
- `engine_physics/src/scene_editing/scene_edit.rs` — explicit rigid placement
  edit representation.

## Authoritative Representation

The CPU body-local structure owns the stable body identifier, transform,
velocity, and collection of local cells. Each local cell keeps local
coordinates, material identifier, appearance, amount, integrity, and
temperature. This representation is the source for persistence, body-local
connectivity, splitting, and final mutation decisions.

The body-local coordinate system lets the body move without rewriting world
tile data. World coordinates are derived from transform plus local cell
coordinates when the body is rasterized or collides with another system.

## Rapier Representation

The CPU physics world owns a Rapier rigid body and colliders for integration,
contacts, actor interaction, and terrain collision. Rapier owns the integrated
transform and velocity for that physics step. Dogwood owns the mapping back to
the stable body and its body-local cellular state. A Rapier collider is not a
replacement for the body-local material cells.

Terrain collision comes from accepted cellular occupancy snapshots. Actors and
rigid bodies share collision groups and are synchronized with the scene's
actor registry after stepping.

### Relevant Implementation

- `engine_physics/src/simulation_rigid_bodies/scene_physics_world.rs` — CPU
  Rapier world and rigid-body ownership.
- `engine_physics/src/simulation_rigid_bodies/scene_physics_world_terrain.rs` —
  extracted terrain collision patches.
- `engine_physics/src/simulation_rigid_bodies/
  scene_physics_world_actor_movement.rs` —
  actor/physics movement synchronization.
- `engine_physics/src/simulation_rigid_bodies/
  scene_physics_world_collision_groups.rs` —
  collision filtering.

## Cellular Double Representation

Each active body is rasterized into the resident cell domain each tick. The
rasterized cells publish material, amount, integrity, temperature, and
occupancy at world cell locations. Cellular pressure and granular contact can
then interact with the body as if it occupied cells, while thermal and reaction
passes can observe the same material semantics. Fluids use this proxy as a
solid boundary, and rendering can sample the body through the scene view.

The rasterization is derived. It is not authoritative for body membership,
transform, persistence, or stable identity. Multiple body-local cells can map
to one raster location, and a raster slot can be reused when the ring moves.

## Upload And Rasterization

The scene assigns active bodies to bounded GPU slots. Upload copies body-local
cell records and transform information into those slots. A GPU pass projects
the local cells through the current transform and ring origin into the common
cell domain. Cellular stages consume the proxy until the next validated upload
or body movement changes it.

### Relevant Implementation

- `engine_physics/src/simulation_rigid_bodies/rigid_cell_state_upload.rs` —
  body-local-to-GPU slot upload.
- `engine_physics/src/simulation_rigid_bodies/rigid_cell_state_upload.wgsl` —
  rasterization into resident cell space.
- `engine_physics/src/simulation_cellulars/cellular_physics_body_proxy.rs` —
  shared proxy resources and slot mapping.

## Feedback From Cellular Simulation

Pressure, thermal, reactions, and phase transitions can alter a rigid cell's
material properties. Those systems work against the rasterized double for
parallel evaluation, then produce bounded readback records that identify the
body slot, local cell, and revision. The scene gathers those records and
applies changes to the CPU body-local cells. A GPU proxy therefore provides
feedback, but never becomes the owner of the resulting topology.

## Identity And Revision Safety

Each body has a stable identity and a current topology revision. GPU slots are
temporary and can be reused after a body leaves or changes residency. A result
that arrives after a split, removal, ring move, or slot reuse is stale. Gather
code checks body identity, slot ownership, and topology revision before
applying it; invalid results are discarded.

### Relevant Implementation

- `engine_physics/src/simulation_rigid_bodies/rigid_granular_readback_slot.rs` —
  bounded body-cell readback slot.
- `engine_physics/src/simulation_rigid_bodies/rigid_cell_state_gather.rs` —
  validates and gathers GPU feedback.
- `engine_physics/src/simulation_rigid_bodies/
  rigid_granular_readback_status.rs` —
  asynchronous completion state.

## Pressure And Damage

Rigid cells participate in the same pressure contacts as static cellular cells,
but damage is applied against body-local integrity. Pressure can remove a cell,
produce configured debris, or make the remaining body split. The body proxy
lets neighboring static and dynamic cells exert load on it, while the CPU
physics body supplies transform/contact information for the proxy.

## Cell Removal And Splitting

Cells disappear when a validated mutation, reaction, phase change, pressure
failure, or explicit destruction removes them. After removal, the scene checks
body-local connectivity. The component containing the body's retained identity
survives as the original body when possible; other connected components become
new bodies or debris according to size and material rules. A body with no
remaining cells is removed from CPU physics.

### Relevant Implementation

- `engine_physics/src/scenes/scene_rigid_cell_mutation.rs` — applies body-cell
  material and phase changes.
- `engine_physics/src/scenes/scene_rigid_detachment.rs` — component gathering
  and new-body creation.
- `engine_physics/src/scenes/scene_rigid_cell_removal_cause.rs` — records why a
  body-local cell was removed.
- `engine_physics/src/simulation_rigid_bodies/
  rigid_granular_reaction_batch.rs` —
  batches reaction/removal consequences.

## Debris

Static and rigid material definitions can name a debris material and a yield
rate. Destruction consumes failed cell material and uses that metadata to
create dynamic/granular debris or another configured result. Debris is a
material mutation, not an implicit renderer effect, so it participates in the
same authority and streaming rules as ordinary material creation.

## Phase Transitions And Reactions

Rigid cells use the same registry identifiers, thermal properties, selectors,
and reaction tables as other material forms. A hot rigid cell can become a
fluid or gas product, or change to another cellular material. The mutation
request names the body-local source, and validated gather/application updates
the CPU cell before the next rasterization. A GPU proxy cannot create a body
or persist itself without this CPU transition.

## Collision With Cellular Terrain

The Rapier body collides with the CPU terrain patches built from delayed
cellular occupancy snapshots. Body motion is integrated in Rapier, while its
cellular proxy is rebuilt from the resulting transform for GPU interaction.
The two views are consequently not instantaneous copies: a terrain snapshot
may be one or more GPU readbacks old, and activation waits for matching ring
and collision state.

## Dynamic And Granular Contacts

Dynamic cellular matter sees the rasterized body as occupied proxy cells. Its
movement solver can settle against or be displaced by that proxy. Contact and
pressure results can feed body-cell damage, but the body transform remains
owned by Rapier and body-cell membership remains owned by the CPU structure.

## Dormancy And Streaming

When a body leaves the active buffered region, the scene waits for pending
cellular feedback, gathers body-local state, removes its live Rapier body and
GPU slot, and assigns the stable record to an owner chunk/file. It is not
serialized from the rasterized view. Restoration reconstructs the body-local
cells and transform, waits for compatible terrain/collision residency, then
creates the active physics and GPU proxy again.

### Relevant Implementation

- `engine_physics/src/scenes/scene_rigid_dormancy.rs` — body removal and
  restoration coordination.
- `engine_physics/src/scenes/scene_rigid_persistence.rs` — stable body data and
  owner-file persistence.
- `engine_physics/src/scene_data/scene_data_dormant_rigid.rs` — dormant body
  serialization.
- `engine_physics/src/scenes/scene_rigid_body_streaming_response.rs` —
  streaming completion and activation checks.

## Complete Rigid-Body Lifecycle

The lifecycle is: static terrain or authored placement produces a CPU
body-local rigid object; Rapier integrates its transform and velocity; upload
and rasterization publish a GPU cellular proxy; pressure, thermal, reaction,
fluid, and granular stages interact with that proxy; validated feedback changes
body-local cells; damage can split or destroy the body; leaving the resident
ring gathers and persists the local representation; and restoration rebuilds
the CPU object and waits for matching terrain before reactivating it.

### Relevant Implementation

- `engine_physics/src/scenes/scene.rs` — scene-level body ownership and stage
  coordination.
- `engine_physics/src/scenes/scene_update.rs` — per-tick placement of body
  steps, uploads, simulation, and readbacks.
- `engine_physics/src/simulation_rigid_bodies/mod.rs` — rigid subsystem module
  boundary.
