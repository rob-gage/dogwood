# Cross-System Flows

These paths connect the subsystem pages. They are deliberately schematic: the
implementation uses several asynchronous queues and GPU command buffers, but
the ownership transitions below are the important architecture.

## One simulation tick

`GameApplication` translates input and calls `Scene::update`. The scene applies
completed streaming/readback work, then consumes elapsed time into fixed ticks.
Each tick updates CPU physics and actor proxies, rasterizes actors/rigids,
advances fluids and gases, discovers/applies material mutations, solves
pressure and dynamic cells, couples thermal state and phase transitions, and
extracts a collision snapshot. The next update polls and applies delayed
readbacks. The renderer then samples the scene’s resident buffers and draws UI.

## A material in the world

Game/template code authors a `Material` and registers it. The registry assigns a
form-specific identifier, compiles dense metadata/tags/reactions, and builds
`MaterialTable`/`MaterialGraphics` buffers. A scene edit or generated chunk
places the identifier and persistent appearance in `TileData`; active state is
uploaded to resident GPU fields. Simulation reads the identifier plus packed
properties, while rendering reads the same identifier and appearance through
the scene render view.

## A thermal phase transition

Temperature begins in the authoritative form’s field. Interaction and
conduction resolve heat, scatter shares it across cellular/fluid/gas/rigid
representations, and phase evaluation compares it with the material’s cold/hot
threshold. A candidate becomes a material mutation carrying target and latent
energy. The mutation stage applies it to the correct authority; for rigid cells,
bounded readback precedes body-local changes. A cellular/fluid/gas transition
can therefore change representation while preserving thermal ordering.

## Static terrain breaking into dynamic or rigid matter

Pressure propagates through static material using transmission and contact
state. Damage reduces integrity. Failed cells use debris metadata, commonly
changing static matter into a dynamic/granular debris material; connected
surviving components are checked against minimum size and may become rigid
body-local cells. Static detachment gathers the GPU state, replaces the static
cells, inserts a CPU rigid body, and rasterizes it on later ticks. Too-small
components are destroyed or reduced to debris rather than creating a useless
body.

## Fluid interacting with an actor or rigid body

Actors/rigids are represented in CPU physics and rasterized into the cellular
proxy double. Fluid particles use spatial buckets and solid/contact fields to
project positions and compute mechanical response. The response is scattered
back to particle velocity; the derived fluid cellular view also supplies
swimming samples and render coverage. CPU actor/rigid transforms remain
authoritative for their bodies; fluid particle records remain authoritative for
fluid motion.

## Scene streaming

The active target moves, selecting outgoing and incoming strips in the ring.
GPU tile/fluid/gas state is downloaded before physical slots are remapped and
written into CPU chunks or dormant records. Rigid bodies are gathered into
owner files. New chunks are loaded/generated, then their tile fields, particles,
gas cells, and rigid bodies are uploaded/reconstructed. Collision extraction
and rigid activation wait for the new ring origin, so old snapshots cannot
interact with newly interpreted slots.

### Relevant implementation

- `engine_physics/src/scenes/scene_update.rs` — complete update/tick flow.
- `engine_physics/src/scenes/scene_chunk_navigation.rs` — ring movement and
  streaming ordering.
- `engine_physics/src/scenes/scene_rigid_cell_mutation.rs` — representation
  changes from phase/reaction results.
- `engine_physics/src/scenes/scene_rigid_detachment.rs` — static component
  detachment and rigid formation.
- `engine_physics/src/simulation_thermal/` — thermal cross-form flow.
- `engine_physics/src/simulation_fluids/` and `simulation_gases/` — fluid/gas
  authoritative fields and derived representations.
