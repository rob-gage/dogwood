+++
title = "Static cell colliders"
description = "How GPU occupancy readback becomes Rapier voxel geometry for static cells and settled dynamic cells."
weight = 50
[extra]
sources = ["engine_world/src/scenes/collisions/collision.rs", "engine_world/src/scenes/collisions/collision_cellular_tile.rs", "engine_world/src/scenes/collisions/collision_cellular_tile_pass.rs", "engine_world/src/scenes/collisions/collision_cellular_tile.wgsl", "engine_world/src/scenes/collisions/mod.rs", "engine_world/src/scenes/scene_error.rs", "engine_world/src/scenes/collisions/collider_shape.rs"]
+++

This page is for engine contributors. It explains how the collision tile pass turns GPU cell state into occupancy masks, and how `Collision` turns those masks into Rapier voxel colliders.

## Occupancy readback

The collision tile pass converts GPU cell state into one 64-bit occupancy mask per 8×8 cellular tile. It reads the material-identifier, kinematics, and rigid-body-cell buffers and writes occupancies in logical active-region order. Each bit is `y * 8 + x`. Static reads map a staging buffer and wait for the GPU before updating Rapier geometry.

### Modes

The pass has two modes:

- **Static occupancy** includes nonempty static material identifiers.
- **Settled dynamic occupancy** includes dynamic identifiers whose kinematics state has the resting flag.

In both modes, cells represented by a rigid-body cell record are excluded, because their geometry belongs to the rigid body's own collider. While cellular rigid bodies exist, collision polls three asynchronous settled-dynamic staging slots and uses the newest completed snapshot matching the active region and ring offset. It releases all completed slots, including snapshots for other regions. If no matching result is ready, it retains the held snapshot; if that snapshot does not match the region and ring offset either, it reads synchronously. It then requests another snapshot, skipping the request if all three slots are in flight.

> **Limitation:** Settled dynamic geometry can lag the GPU cell state by multiple ticks when readback is delayed.

## Voxel collider updates

`Collision` stores the last occupancy vector and active region for each mode. When a new read arrives, it:

1. Compares masks for tiles that remain resident, treats departed tiles as empty, and fills tiles newly entering the region.
2. Turns changed bits into voxel keys relative to the Rapier origin. Each voxel is one cell wide (`1 / CELLULAR_TILE_SIZE` tile); its integer key is the cell coordinate relative to that origin.
3. Edits an existing `Voxels` shape in place. If no collider exists but some cells are occupied, it creates one and fills it voxel by voxel.
4. Removes the collider when the solid-cell count reaches zero.

### Collision groups

- The static collider uses the reserved static-cell membership group with `Group::ALL` as its filter.
- The settled dynamic-cell collider uses the reserved dynamic-cell membership group and only the rigid-body group as its filter. These resting cells support cellular rigid bodies without serving as general actor terrain.

`CollisionCellularTile::new` also provides a CPU conversion from 64 material identifiers to static occupancy; the GPU pass uses the same nonempty-static rule.

## Refresh triggers

Static occupancy is read when any of these holds:

- no prior snapshot exists;
- the active region or ring offset changes;
- a reread is pending.

Edits, incomplete changed-cell reports, and releasing one or more rigid bodies can set the reread flag. If a changed-cell report is incomplete, unread cells remain attached until the next read. A change in region or ring also changes which tile-buffer positions correspond to logical tiles.

When an edit may have changed cells belonging to cellular rigid bodies, those bodies' changed cells are read and removed or split before static occupancy is read, so the replaced cells can appear in the static result.

## Errors

- Occupancy extraction returns `SceneError::InvalidCellularTileRegion` if the output would exceed its allocated buffer.
- Device polling failures or a missing mapping callback return `SceneError::CollisionCellularTilePoll`.
- A buffer mapping error returns `SceneError::CollisionCellularTileResponse { error }`.
- The staging-range access uses `expect` after mapping succeeds.

See the [collision world](@/internals/simulation/collision-world.md) for step ordering and the [collision shader reference](@/internals/shaders/collision-shaders.md) for GPU bindings and packing.
