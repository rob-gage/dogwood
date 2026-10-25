+++
title = "Collisions and edits"
description = "Public collider and edit data types, reserved collision groups, and engine-managed simulation APIs."
weight = 80
[extra]
sources = ["engine_world/src/scenes/collisions/mod.rs", "engine_world/src/scenes/collisions/collider.rs", "engine_world/src/scenes/collisions/collider_shape.rs", "engine_world/src/scenes/collisions/collision_body.rs", "engine_world/src/scenes/collisions/collision.rs", "engine_world/src/actors/components/actor_collision.rs", "engine_world/src/actors/components/actor_contact.rs", "engine_world/src/scenes/edits/edit.rs", "engine_world/src/scenes/edits/edit_mode.rs", "engine_world/src/scenes/edits/edit_error.rs", "engine_world/src/scenes/edits/edit_streaming_state.rs", "engine_world/src/scenes/edits/mod.rs", "engine_world/src/scenes/cellular_tiles/mod.rs", "engine_world/src/scenes/scene.rs", "engine_world/src/scenes/collisions/collision_cellular_tile.rs", "engine_world/src/scenes/collisions/collision_rigid_body_pass.rs", "engine_world/src/scenes/cellular_tiles/cellular_tiles.rs", "engine/src/lib.rs"]
+++

This page covers the colliders actors attach to the physics world and the `Edit` API for changing cells. It also lists the public collision and cellular tile simulation types that games do not manage. The collision types are under `engine::scenes::collisions`.

## Collider data

A `Collider` combines a `ColliderShape` with `offset_x`, `offset_y`, and `rotation`. Offsets are in tiles relative to the owning actor, and rotation is counterclockwise radians.

| Shape | Dimensions (tiles) |
|---|---|
| `Circle { radius }` | `radius` |
| `Rectangle { width, height }` | Axis-aligned before `Collider.rotation` is applied |
| `Capsule { width, height }` | Vertical; `width` cannot exceed `height` |

Every dimension must be finite and positive, and offsets and rotation must be finite. Invalid colliders produce `SceneError::InvalidCollider` when checked.

### Collision bodies

Actors supply a `CollisionBody` through `ActorCollisionComponent`; contact-only actors use `ActorContactComponent`. See [actor components](@/external-api/actor-components.md).

| Body | Behavior |
|---|---|
| `Fixed` | Does not move under physics; the scene relocates it when its owner's position changes. |
| `Kinematic` | Moves when its owner's position is set; pawn movement can also move it. |
| `Dynamic` | Moves under simulated forces and collisions. |

### Groups, memberships, and filters

Both component types carry a `memberships` bitmask and a `filter` bitmask. For `ActorCollisionComponent`, the filter selects groups it collides with; for `ActorContactComponent`, it selects groups it contacts. In either case, both filters must accept the other component's memberships.

For example, an actor with memberships `1 << 0` and filter `COLLISION_GROUP_CELLULAR_TILE | 1 << 0` collides with solid terrain and with other actors in group 0. See [collision setup](@/examples/collision-setup.md) for a worked configuration.

The high five group bits are reserved by the engine:

| Constant | Bit | Reserved use |
|---|---:|---|
| `COLLISION_GROUP_CELLULAR_DYNAMIC` | 27 | Settled dynamic cells; they interact only with cellular rigid bodies, and actor filters do not enable collisions with this group |
| `COLLISION_GROUP_ACTOR` | 28 | Actor colliders |
| `COLLISION_GROUP_CELLULAR_TILE` | 29 | Solid cells in active cellular tiles |
| `COLLISION_GROUP_RIGID_BODY` | 30 | Cellular rigid bodies |
| `COLLISION_GROUP_EDIT` | 31 | Colliders excluded by `EditMode::Insert` |

`COLLISION_RIGID_BODY_COUNT_MAXIMUM` limits detached cellular rigid bodies to 1024; actor rigid bodies do not use this pool. Other capacity values are private implementation constants.

## Edits

An `Edit` exposes its `mode`, `cellular_tiles` fills, and `pressures` map of per-cell pressure deltas. Request one with `Scene::edit`; see [editing and saving the world](@/examples/editing-cells.md) for the request flow.

### Building an edit

- `Edit::new(mode)` starts an empty edit.
- `fill_cell(cell: CellCoordinates, material_identifier: MaterialIdentifier, color: u32)` records a cell and replaces any earlier fill of that cell in the edit. Pack `color` as RGBA with red in the least-significant byte: `u32::from_le_bytes([r, g, b, a])`.
- `fill_segment(start: ScenePosition, end: ScenePosition, radius: f64, material_identifier: MaterialIdentifier, color: u32) -> Result<(), EditError>` fills cells whose centers fall within `radius` of the segment; `radius` is in tiles.
- `add_pressure_cell` / `remove_pressure_cell` change one cell by an atmosphere amount. `add_pressure_segment` / `remove_pressure_segment` apply the change within a segment radius in tiles.

`fill_segment` edge cases:

- It returns `EditError::CapacityReached` if the segment's bounding rectangle spans more than 4096 cellular tiles (the private tile limit).
- A non-finite or negative radius, or non-finite relative endpoints, produces no fill and returns `Ok(())`.

Pending edits are applied in request order before the next stream moves their resident tiles.

### Edit modes

| Mode | Behavior |
|---|---|
| `EditMode::Insert` | Fills empty cells not overlapped by physical `ActorCollisionComponent` colliders whose memberships include `COLLISION_GROUP_EDIT`; the collider filter is not checked. Contact sensors do not block inserts. |
| `EditMode::Replace` | Fills every requested cell and ignores colliders. |

In `Replace` mode, filling with `MaterialIdentifier::EMPTY` clears a cell; `Insert` skips fills with `EMPTY`. Filling with a gas material leaves the cell empty and fills it with that gas at scene ambient gas pressure, or one atmosphere when ambient pressure is lower; emptying leaves phantom air at ambient pressure. Pressure edits add to stored pressure in static cells; other cells add phantom air or reduce all gas proportionally.

### Edit errors

`Scene::edit` returns an `EditError`:

- `CellularTileInactive { coordinates }`: a tile of the edit is outside the active region.
- `MaterialUnregistered { material_identifier }`: the material is not in the scene's material table.
- `InvalidPressure`: a segment pressure is negative or not finite.
- `CapacityReached`: see below.

`Scene::edit` also returns `CapacityReached` when the tile entries in edits awaiting the next stream would exceed 4096. Tile entries repeated across edits count separately.

## Engine-managed public types

`Collision`, `CollisionCellularTile`, `CollisionRigidBodyPass`, `CellularTiles`, and `EditStreamingState` are public Rust types, but games do not construct or manage them. For engine-managed cell collider behavior, see [Static cell colliders](@/internals/simulation/static-cell-colliders.md); use `Scene::edit` to change cells.

{{ <rustdoc path="engine/scenes/collisions/struct.Collider.html" text="Collider" /> }} ·
{{ <rustdoc path="engine/scenes/collisions/enum.ColliderShape.html" text="ColliderShape" /> }} ·
{{ <rustdoc path="engine/scenes/collisions/enum.CollisionBody.html" text="CollisionBody" /> }} ·
{{ <rustdoc path="engine/scenes/collisions/struct.Collision.html" text="Collision" /> }} ·
{{ <rustdoc path="engine/scenes/collisions/struct.CollisionCellularTile.html" text="CollisionCellularTile" /> }} ·
{{ <rustdoc path="engine/scenes/collisions/struct.CollisionRigidBodyPass.html" text="CollisionRigidBodyPass" /> }} ·
{{ <rustdoc path="engine/scenes/cellular_tiles/struct.CellularTiles.html" text="`CellularTiles`" /> }} ·
{{ <rustdoc path="engine/scenes/collisions/constant.COLLISION_GROUP_CELLULAR_DYNAMIC.html" text="COLLISION_GROUP_CELLULAR_DYNAMIC" /> }} ·
{{ <rustdoc path="engine/scenes/collisions/constant.COLLISION_GROUP_ACTOR.html" text="COLLISION_GROUP_ACTOR" /> }} ·
{{ <rustdoc path="engine/scenes/collisions/constant.COLLISION_GROUP_CELLULAR_TILE.html" text="COLLISION_GROUP_CELLULAR_TILE" /> }} ·
{{ <rustdoc path="engine/scenes/collisions/constant.COLLISION_GROUP_RIGID_BODY.html" text="COLLISION_GROUP_RIGID_BODY" /> }} ·
{{ <rustdoc path="engine/scenes/collisions/constant.COLLISION_GROUP_EDIT.html" text="COLLISION_GROUP_EDIT" /> }} ·
{{ <rustdoc path="engine/scenes/collisions/constant.COLLISION_RIGID_BODY_COUNT_MAXIMUM.html" text="COLLISION_RIGID_BODY_COUNT_MAXIMUM" /> }} ·
{{ <rustdoc path="engine/scenes/edits/struct.EditStreamingState.html" text="EditStreamingState" /> }} ·
{{ <rustdoc path="engine/scenes/edits/struct.Edit.html" text="Edit" /> }} ·
{{ <rustdoc path="engine/scenes/edits/enum.EditMode.html" text="EditMode" /> }} ·
{{ <rustdoc path="engine/scenes/edits/enum.EditError.html" text="EditError" /> }}

## Fluid edits

Edits accept registered fluid materials and queue particle spawns for the next simulation. Fluid `Insert` spawns skip occupied cellular/particle cells. Replacing or erasing cells queues particle erasure; `CapacityReached` additionally covers reserved fluid-particle capacity. Particle actions and cellular Insert masks are queued separately; see [fluid edit behavior and limitations](@/internals/world-streaming/fluid-streaming.md#editing-fluid).
