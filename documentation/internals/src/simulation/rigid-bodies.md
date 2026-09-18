# Rigid cellular bodies

Rigid cellular bodies arise from authored rigid placement or detached connected
static components. Their authoritative representation is a CPU body-local cell
set plus transform, velocity, material state, integrity, amount, temperature,
and stable body identity. The CPU `ScenePhysicsWorld` owns the Rapier body and
colliders that integrate that transform with terrain and actors.

The body is also published into the cellular domain. Each fixed tick, the scene
gets the current body transform/state from the CPU physics world and
`CellularPhysicsBodyProxy` rasterizes body-local cells into transient
ring-aligned material, appearance, and occupancy buffers. Pressure, thermal,
reaction, and cellular contact stages can therefore treat a rigid as nearby
cellular matter without making the GPU raster authoritative.

GPU stages can modify body-local state indirectly. Rigid cell gather/readback
captures changed integrity, amount, temperature, pressure/contact results, and
reaction/phase candidates into bounded slots. The scene validates body identity,
topology revision, and state slot before applying results. A stale readback is
discarded rather than applied to a reused body slot.

Pressure damage and static detachment can remove cells. Connected components
below a material’s minimum size are destroyed or converted to debris; surviving
components become separate rigid bodies with new stable IDs. Phase transitions
and reactions use their authored transition/debris semantics, not a generic
fracture shortcut.

Bodies outside the resident ring enter dormancy. Their body-local records are
gathered, written to one owner chunk file, and removed from active physics and
GPU proxies. Loading reverses the process, but activation waits until the
matching terrain collision snapshot has been installed. This prevents a body
from colliding against an old ring mapping.

### Relevant implementation

- `engine_physics/src/simulation_rigid_bodies/rigid_cellular_body.rs` — body
  identity, transform, and body-local cell ownership.
- `engine_physics/src/simulation_rigid_bodies/scene_physics_world.rs` — CPU
  Rapier world and rigid integration.
- `engine_physics/src/simulation_rigid_bodies/rigid_cell_state_upload.rs` —
  publishes authoritative body state to GPU slots.
- `engine_physics/src/simulation_rigid_bodies/rigid_cell_state_gather.rs` —
  readback of modified body-local state.
- `engine_physics/src/simulation_cellulars/cellular_physics_body_proxy.rs` —
  rasterized rigid/actor cellular double.
- `engine_physics/src/scenes/scene_rigid_cell_mutation.rs` — phase/reaction
  changes, splitting, debris, and cell removal.
- `engine_physics/src/scenes/scene_rigid_detachment.rs` — static connected
  component detection and promotion.
- `engine_physics/src/scenes/scene_rigid_dormancy.rs` — gather and restore
  around streaming.
- `engine_physics/src/scenes/scene_rigid_persistence.rs` — owner-file jobs and
  persistence responses.
