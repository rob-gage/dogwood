+++
title = "Cellular rigid bodies"
description = "How floating static-cell groups become Rapier bodies, split after edits, and return to tiles."
weight = 70
[extra]
sources = ["engine_world/src/scenes/collisions/collision.rs", "engine_world/src/scenes/collisions/collision_rigid_body.rs", "engine_world/src/scenes/collisions/collision_rigid_body_pass.rs", "engine_world/src/scenes/collisions/collision_rigid_body.wgsl", "engine_world/src/scenes/collisions/mod.rs", "engine_world/src/scenes/scene.rs", "engine_world/src/scenes/edits/edit_streaming_state.rs", "engine_world/src/materials/material_table.rs", "engine_world/src/materials/material_properties_cellular_static.rs"]
+++
A cellular rigid body is a group of static cells that is no longer connected to the rest of the world. Collision detects it, simulates it as a Rapier body, and rasterizes it back into the tile buffers. This page is for engine contributors; it covers detection, creation, splitting, rasterization, and release.

## Limits

| Limit | Value |
| --- | --- |
| Flood-fill bound for one group | 4096 cells |
| Debris threshold (groups below this become debris) | 12 cells |
| Changed body cells processed per read | 65,536 |

Other rigid-body capacities are listed in [resources and lifetimes](@/architecture/resources-and-lifetimes.md).

## Detection

Detection starts after the static-cell occupancy readback changes; see [static cell colliders](@/internals/simulation/static-cell-colliders.md) for how occupancy is built and refreshed.

- The detector is seeded from occupancy changes in the previous snapshot. Occupancy in newly entering tiles does not seed a flood, so this is not a full-region scan.
- Collision checks the changed cells and their four side-neighbors.
- For each unvisited static cell in those seed neighborhoods, it flood-fills side-connected static cells.
- A group stays attached when the search reaches the active region's edge, exceeds 4096 cells, or reaches a cell already assigned to another flood.

These checks avoid detaching groups that may continue beyond the available region or exceed the search bound.

## Creation

Detached groups are read from GPU cell storage to retain each cell's material identifier, color, stored pressure, and integrity. The read is copied at the end of the collision tick and applied in the next one, and a group whose cells are no longer all static by then is dropped.

- Groups with fewer than 12 cells become debris. Each cell is replaced by its static material's configured debris material (a registered cellular dynamic material), or by empty if none is configured. Cells converted to a debris material keep their color; cells replaced by empty have their color cleared.
- Groups of 12 or more cells become candidate rigid bodies.
- If body or tile capacity prevents insertion, the group remains static.

Each body stores an 8x8-cell occupancy mask per cellular tile, plus per-cell material identifiers and colors. Rapier receives a dynamic body and a voxel collider. The GPU pass receives tile records and a body record used to rasterize the cells. Each body cell also carries stored pressure and integrity in a GPU state buffer. That state follows cells through release, rasterization, body splits, and detachment. The body tracks the last rasterized pose. The [Rapier collision world](@/internals/simulation/collision-world.md) covers the broader collision step.

### Mass properties

`CollisionRigidBody::mass_properties` visits every occupied cell and reads its static material properties. Cells whose material has no static properties are skipped.

- Each cell contributes its material mass at the cell center. Total mass is the sum, and the center of mass is the mass-weighted mean of cell centers.
- Inertia starts as the sum over cells of mass times squared distance from the origin, plus a term for each square cell's own spin (`cell_size^2 / 6` times its mass). The parallel-axis shift then subtracts total mass times the squared distance of the center of mass, giving inertia about the center.
- Friction and restitution are the mass-weighted averages of the cells' values.
- A body with no mass gets zero for every value.

## Edits and splitting

When an edit writes a different material into a rigid-body cell, the next collision update reads that cell. A rigid-body cell is detached when its grid identifier differs from the body's stored identifier and the new identifier is not dynamic, including when it is empty. The cell is removed from the body's occupancy.

`connected_cells` then groups the remaining occupied cells by shared sides, largest group first:

- The largest group keeps the original body if it has at least 12 cells.
- Other groups of at least 12 cells become new bodies at the parent's pose. They keep its angular velocity, and their linear velocity is adjusted so each new center of mass keeps the parent's velocity field.
- Smaller groups, and groups that cannot be inserted, become debris.
- If no occupied cells remain, the body is removed.

This readback and split path is why an edit requests fresh collision cell data; see the [edit pipeline](@/internals/simulation/edit-pipeline.md).

## Rasterization

The collision update writes records when a body's pose, per-tick linear or angular movement values, or cells change. The pass submits pending cell replacements first, then returns departing tiles, clears old coverage for moved bodies, and rasterizes tile records at the new pose.

Dynamic cells under a rigid body stay associated while its current rasterization covers them. When the body moves, old coverage markers are cleared; dynamic material, color, and kinematics at uncovered old cells remain in place. Unchanged, resting bodies keep their current rasterized cells.

Edit changed-cell scans wait for GPU staging-buffer mappings. Pressure-broken cell and tile lists are copied asynchronously and applied on the next collision tick. Detached group material reads are asynchronous; bodies are created on the following tick after the read is verified. Rasterization writes are queued without waiting.

## Release

Before the tile ring moves, collision checks each body's occupied tile corners against the requested region. If any corner of any body tile lies outside, the body is released:

- its current record and tile records are sent to the GPU release pass;
- its Rapier body is removed;
- the pass returns currently static rasterized cells at rest, while dynamic cells under the body remain dynamic as their body-cell markers are cleared;
- collision requests a fresh occupancy read.

Each collision simulation also checks bodies against the current active region and releases any that have moved outside it.

Scene drop similarly returns active tiles through tile streaming so changed cells can be saved; see [tile streaming](@/internals/world-streaming/tile-streaming.md).

## Changed-cell read limit

Each read processes at most 65,536 changed body cells. Remaining cells stay attached until a later read.
