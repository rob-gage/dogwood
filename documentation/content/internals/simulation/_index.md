+++
title = "Simulation"
description = "How GPU cell kinematics and Rapier collision simulation exchange data during a scene step."
weight = 40
sort_by = "weight"
template = "section.html"
[extra]
sources = ["engine_world/src/scenes/scene.rs", "engine_world/src/scenes/kinematics/kinematics.rs", "engine_world/src/scenes/kinematics/kinematics_cellular_tile_pass.rs", "engine_world/src/scenes/collisions/collision.rs", "engine_world/src/scenes/collisions/collision_rigid_body_pass.rs"]
+++

This section is for engine contributors. It describes the two connected simulation systems that run over a scene's active region and the pages that cover each part.

- GPU kinematics moves dynamic cells and records their contacts with actors and cellular rigid bodies.
- The Rapier collision world steps actor and rigid-body motion against voxel colliders built from static cells and, for rigid-body collisions, settled dynamic cells. It then rasterizes moving cellular rigid bodies back into the tile buffers.

```text
tile buffers + materials + actors + rigid bodies
                  |
                  v
          GPU cell kinematics
           |              ^
   cell contacts      body records
           v              |
       Rapier collision world
           |              |
           v              v
   actor registry    tile buffers
   (positions)       (body rasterization)
```

## Step order

At each fixed simulation step, `Scene::simulate` runs kinematics first. Its actor contacts and rigid-body impulses and holding contacts feed `Collision::simulate_collisions`. Collision then synchronizes actors and voxel state, advances Rapier, and queues rigid-body rasterization.

GPU kinematics submits work without waiting for its contact readbacks, so collision consumes contacts returned from earlier GPU work. See [scene update ordering](@/architecture/scene-pipeline.md) and [contact exchange](@/internals/simulation/contact-exchange.md) for the exact handoff.

## Pages

- [Material table compilation](@/internals/simulation/material-table.md) — how registered materials become GPU properties and CPU albedo lookup data.
- [Cellular kinematics](@/internals/simulation/cellular-kinematics.md) — dynamic-cell movement, sleeping tiles, and interaction with actors and rigid bodies.
- [Rapier collision world](@/internals/simulation/collision-world.md) — collision-world state, origin shifts, actor synchronization, and step order.
- [Static cell colliders](@/internals/simulation/static-cell-colliders.md) — GPU occupancy readback and voxel collider updates.
- [Cellular rigid bodies](@/internals/simulation/rigid-bodies.md) — detection of floating static groups, Rapier body creation, splitting after edits, and rasterization back into tiles.
- [Cell and body contact exchange](@/internals/simulation/contact-exchange.md) — contact record formats, readback timing, and how collision applies them.
- [Edit pipeline](@/internals/simulation/edit-pipeline.md) — edit validation, coalescing, the GPU edit pass, and interaction with collision and streaming.
- [Shaders and GPU data](@/internals/shaders/_index.md) documents the WGSL entry points and GPU record layouts.
