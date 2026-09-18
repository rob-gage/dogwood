# Cross-System Flows

These flows connect the subsystem pages. They are intentionally prose rather
than pseudocode: GPU passes, bounded readbacks, and scene queues are the
implementation details behind the ownership changes described here.

## One Simulation Tick

`GameApplication` translates input and calls `Scene::update`. The scene first
applies completed streaming and readback work, moves the resident ring if its
target changed, and applies queued edits. Each fixed tick synchronizes actor
and rigid proxies, steps CPU physics, advances cellular, fluid, and gas stages,
resolves material reactions and pressure, couples thermal state and phase
transitions, and schedules collision extraction. Completed GPU work is applied
only after validation on a later safe boundary. Rendering then samples the
resident scene view and UI is composited over it.

The important dependency is that proxies and ring coordinates must be current
before interaction passes, while readbacks must be validated before they alter
CPU ownership.

## A Material In The World

Game or template code authors `Material`, registers it with
`MaterialRegistryBuilder`, and compiles a `MaterialRegistry`. The scene builds
dense simulation metadata and render appearance tables. Placement writes the
identifier and appearance into `TileData` or an edit request. Resident upload
copies the tile fields into GPU cell storage, where pressure, thermal,
reaction, and movement stages read the compiled identifier. Rendering samples
the same identifier and appearance through `SceneGraphics`; it never becomes
the owner of the material.

## A Thermal Phase Transition

Each form contributes temperature to thermal interaction. Conduction resolves
neighbor transfer using conductivity and heat capacity. Scatter sends the
resolved temperature back to cellular cells, fluid particles, gas cells, and
rigid body-local cells. Phase evaluation compares the authoritative result to
hot/cold thresholds and creates a bounded candidate with target, yield, and
latent energy. Mutation application consumes the source and creates the target
form, using rigid readback and identity checks when a body-local cell is
involved.

## Static Terrain Under Pressure

Contacts and reaction pressure enter the cellular pressure field. Directional
transmission and retained pressure reduce static integrity when the load is
eligible. Failed cells can emit dynamic debris. The remaining static graph is
checked for connected components; a sufficiently large detached component is
gathered into a CPU rigid body, while a small component is discarded or
reduced through debris metadata. The new body is uploaded as a proxy on a
later tick and begins participating in Rapier and cellular contact.

## Rigid Body Interacting With Sand

Rapier integrates the body's transform. Upload/rasterization publishes its
body-local cells as occupied proxy cells. Dynamic granular movement sees those
cells as contacts and settles or redirects around them. Pressure and damage
may produce validated body-cell feedback, but the body transform stays in
Rapier and sand remains authoritative in dynamic cellular state.

## Rigid Body Interacting With Water

Water particles predict motion and solve density, then project away from the
rigid proxy. The resulting displacement reconstructs particle velocity and the
fluid contact field can apply body push/drag effects. Swimming samples the
derived fluid raster for pawns. The body-local structure remains the rigid
authority and the particle population remains the fluid authority.

## Fire Consuming Material And Producing Heat/Smoke

A reaction candidate matches fire and its reactant selector plus temperature,
pressure, or air gates. Reservation prevents another candidate from consuming
the same source. Application consumes the reactant, adds thermal energy and
pressure, and creates smoke as a gas product through gas mutation routing.
Thermal conduction and scatter then update nearby forms, while rendering reads
the resulting smoke concentration and material appearance.

## Water Heating Into Vapor

Water particle temperature is gathered into the thermal solve. Conduction and
reaction energy bring it across the hot transition threshold. A phase candidate
requests a gas target; mutation removes the source amount from fluid authority
and inserts concentration into the gas field. The next gas transport pass
advects that concentration in the shared velocity field.

## Vapor Cooling Into Water

Gas temperature is resolved and scattered through the gas authority. Cooling
below the material's cold threshold produces a fluid target. Mutation removes
gas concentration and allocates fluid particles with the resulting material,
position, amount, and temperature. The particle solver then owns motion.

## Rigid Material Melting Into Fluid

Thermal evaluation identifies a hot rigid body-local cell. A validated rigid
readback identifies the exact body and local cell before mutation removes it
from the CPU body-local structure. The product is allocated as fluid particles
at the corresponding world position. If the remaining body disconnects,
component splitting runs after the removal.

## Scene Streaming

When the active target moves, the scene identifies outgoing strips and waits
for tile, fluid, and gas downloads. Tile fields become persistent chunk state;
particles and gas cells become sparse dormant records; rigid bodies are
gathered into stable owner files. Only then can physical ring slots be reused.
Incoming chunks are loaded or generated, fields and dormant records are
uploaded, and rigid bodies are reconstructed. Collision snapshots and body
activation wait for the new ring origin and matching generation so old GPU
results cannot affect new occupants.

### Relevant Implementation

- `engine_physics/src/scenes/scene_update.rs` — fixed-tick and completion order.
- `engine_physics/src/scenes/scene_chunk_navigation.rs` — resident-ring
  movement.
- `engine_physics/src/scenes/scene_rigid_detachment.rs` — pressure-to-body flow.
- `engine_physics/src/scenes/scene_rigid_cell_mutation.rs` — rigid cross-form
  mutation.
- `engine_physics/src/simulation_materials/` — reaction discovery,
  reservation, and application.
- `engine_physics/src/simulation_thermal/` — thermal gathering, conduction,
  scatter, and phase candidates.
- `engine_physics/src/scenes_streaming/` — asynchronous transfer boundaries.
