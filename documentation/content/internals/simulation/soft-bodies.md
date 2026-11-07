+++
title = "Soft bodies"
description = "How particle bodies carry, rasterize, and lose static cells."
weight = 75
[extra]
sources = ["engine_world/src/scenes/soft_bodies", "engine_world/src/scenes/collisions/collision.rs", "engine_world/src/scenes/collisions/collision_rigid_body_pass.rs", "engine_world/src/scenes/collisions/collision_rigid_body.wgsl", "engine_world/src/scenes/collisions/mod.rs", "engine_world/src/scenes/scene.rs", "engine_world/src/actors/components/actor_soft_body.rs", "engine_world/src/actors/components/actor_shader.rs", "vendor/rapier2d"]
+++
Soft bodies combine Rapier particles and edges with static cellular matter carried in segments between neighboring particles. A descriptor also supplies bend edges, optional anchors, collision surface edges, and material, motion, and environment-response settings. Optional counterclockwise `triangles` with `elasticity` add deformable triangular cells; `SoftBodyElasticity` sets `young_modulus`, `poisson_ratio`, and `damping_ratio`. `initial_state` can place and give velocity to every particle, and `surface_coordinates` supplies a coordinate per particle for shaders. Both per-particle arrays are used when their lengths match the particle count. `touches_actors` controls actor contact and defaults to true; `blocks_actors` controls whether that contact blocks actors.

## Cells and rasterization

Each segment stores its cells as rows along the path between its endpoint particles. Those cells remain in the cell grid and are rasterized by the same GPU rigid-body pass used for cellular rigid bodies. Segment tiles use stamped rasterization, placing cells at their transformed centers so thin strands remain connected; tile occupancy and per-cell material, color, radiance, temperature, and amount are written through the pass's cell records. After the Rapier step, particles settle against the static terrain and moving rigid bodies, and segment cells are prepared for rasterization. The segment is represented by its pose and movement between particles.

The collision-owned changed-cell report identifies segment cells that were changed by simulation or edits. Such cells are removed from the segment. If the remaining cells no longer form a connected path between the segment ends, the segment tears and its Rapier edge is removed. On that tear, cells hanging off the path are converted to their material's configured debris, or to empty when there is no debris material.

After cell changes, a body whose total remaining segment cells fall below its `minimum_cell_count` is removed. Its cells become configured debris or nothing. The minimum is checked for bodies marked changed by a cell report; an empty body is removed when that check finds its count below the minimum.

## Actor collision and terrain

`blocks_actors` selects the soft-body collision group that collides with actor colliders. When false, actors can pass through the soft body; the body still collides with other eligible bodies. When true, actors are included in its collision filter.

The active region's static-cell occupancy is sampled as terrain. Particles are kept out of solid cells and pushed toward nearby open cells when terrain appears around them; particle results include a contact normal, zero when free. Separation between bodies moves particles by a bounded distance per step and sweeps the movement against terrain. A GPU sampling pass also reads nearby cell temperature, pressure and pressure gradient, gas flow, fluid coverage and velocity, and configured material tags. Completed samples are applied asynchronously and drive configured gravity, pressure, gas, fluid drag, and buoyancy responses.

The public module is `engine::scenes::soft_bodies`; `Scene` methods spawn, remove, reshape, cut, tear, inspect, target, and push bodies and configure environment tags. `soft_body_particles` returns positions, velocities, terrain contact normals, triangles, outline edges, and masses. `terrain` exposes the last simulated region's static-cell occupancy. Samples are dispatched after actor sensing and read asynchronously on a later step. See [Scenes](@/external-api/scenes.md) for the public API summary.

Dogwood uses its vendored Rapier source under `vendor/rapier2d`; its soft-body voxel contact behavior includes the repository's contact patch. Collision and origin handling are described in [Rapier collision world](@/internals/simulation/collision-world.md).

A particle can be let through solid cells with `Scene::set_soft_body_particle_passes_terrain`, for one caught on rock; it then skips the terrain sweep and any terrain limit on its separation from other bodies until it is set back.
