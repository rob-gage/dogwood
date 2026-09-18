# Cellular simulation

Cellular state is a dense 8-by-8-per-tile representation. Parallel resident
buffers hold material identifiers, appearances, integrity, normalized amount,
and temperature. Static cellular materials are terrain-like and retain
structural integrity; dynamic cellular materials are granular/mobile matter.

The dynamic solver updates cellular movement from gravity and pressure-related
state. It does not replace the CPU `TileData` authority while a tile is active;
the scene downloads the resulting fields when residency changes. Static cells
participate in collision extraction and can be detached into rigid bodies when
connected support is lost.

Pressure propagates directional load through material transmission. It combines
cell contacts, actor/rigid proxy geometry, retained pressure, integrity damage,
and fracture decisions. Materials can ignore low loads, transmit only a
fraction of pressure, and emit a debris material when integrity fails. The
minimum connected-component size determines whether a detached component is
discarded/debrised or promoted to a rigid cellular body.

The cellular collision subsystem extracts a compact occupancy snapshot from
the GPU. The CPU physics world consumes that snapshot to build terrain
colliders. It is therefore a delayed collision view, not a direct per-cell
Rapier query. Snapshot origin and age are checked so a stale extraction cannot
be applied to a remapped ring.

Actor pawns and rigid bodies enter the cellular domain through transient proxy
rasterization. Actor drive and rigid transforms are inputs; their proxy cells
are not persistent terrain. Fluid particles also produce a derived cellular
coverage/material view for contact and rendering.

### Relevant implementation

- `engine_physics/src/simulation_cellulars/cellular_dynamic.rs` — dynamic
  cellular GPU resource and movement dispatch.
- `engine_physics/src/simulation_cellulars/cellular_pressure.rs` — pressure,
  damage, contact, and reaction readback resources.
- `engine_physics/src/simulation_cellulars/cellular_collision.rs` — GPU
  occupancy extraction and collision readback.
- `engine_physics/src/simulation_cellulars/cellular_physics_body_proxy.rs` —
  actor/rigid rasterized cellular double.
- `engine_physics/src/scenes/scene_rigid_detachment.rs` — static component
  gather and rigid formation.
- `engine_physics/src/scenes/scene_edit_application.rs` — edit-to-cellular and
  edit-to-fluid/rigid routing.
