+++
title = "Fluid boundaries and sleeping"
description = "Solid density samples, moving-wall projection, and tile sleep/wake rules."
weight = 76
[extra]
sources = ["engine_world/src/scenes/fluids/fluid_particle.wgsl", "engine_world/src/scenes/fluids/fluid_particle_pass.rs"]
+++

The [fluid solver](@/internals/simulation/fluids.md) combines sampled solid density with geometric projection. These serve different purposes: density samples enter the pressure constraint, while projection keeps particle centers out of solid geometry. Actors use analytic projection rather than the solid-cell density map.

## Sparse solid samples

Each tick marks tiles containing particles and expands them to their eight neighbors. This one-tile margin covers the kernel and motion margin. Only listed tiles rebuild solid occupancy, boundary weights, and node samples; old listed tiles are cleared through the previous list. Solid occupancy is any nonzero cellular material identifier, including dynamic grains and rasterized rigid bodies.

Each solid cell's Akinci-style weight is `psi = rho0 / sum W(solid-center distances)`, over a 5×5 neighborhood within kernel support. Each cell-corner node accumulates weighted density, weighted spiky gradient, and the dot product of boundary velocity with that gradient. Particles bilinearly interpolate these node records instead of searching solid neighbors during every constraint iteration.

Boundary velocity comes from packed cellular velocity for a dynamic grain, linear plus angular velocity for a cellular rigid body, and zero for ordinary static matter. Rigid-body positions use collision-origin cells; the uniforms carry the active region's offset in that space. Invalid or unlisted node ownership returns zero rather than old map contents.

## Solid projection

`project_from_solids` clamps centers to an active-region border inset of `0.4501` cell. It makes at most two passes over the surrounding 3×3 solid squares. Near a square, it uses the closest point and outward normal; inside a square it prefers the nearest face opening onto free space, falling back to the nearest face if all sides are solid.

Prediction sweeps each substep in pieces no longer than approximately half a cell, capped to eight pieces. On contact it removes relative velocity into the wall and retains `0.95` of tangential relative speed, using the sampled wall velocity. Constraint corrections project again after each iteration.

> **Limitation:** This particle-wall projection uses the fixed `FLUID_BOUNDARY_FRICTION = 0.05`, not material contact friction or restitution. Those material coefficients are used by the separate [cell/raster contact solver](@/internals/simulation/fluid-coupling.md#dynamic-cell-contacts). Active-region edges act as walls until streaming rebases the pool; particles do not freely flow into unloaded space.

## Actor projection

The shared actor list contains eligible cellular-collision actors, limited to `KINEMATICS_ACTOR_COUNT_MAXIMUM`. It supports rotated circles, rectangles, and vertical capsules. Projection expands the shape by approximately half a cell: the implementation uses `0.4501` for circles and rectangle corners, and `0.5001` for capsule radii and rectangle interior-face exits. A `0.002` contact tolerance retains normals just outside the surface.

Each substep linearly extrapolates the uploaded actor using its inferred velocity and sweeps that translation in at most eight half-cell pieces. Projection runs after prediction and each pressure correction, then projects against solids again. It accumulates the opposite particle momentum change into the actor's feedback record. After velocity commit, another pass restores any missing actor-normal velocity and records its reaction. Rotation is sampled, not swept through an angular trajectory.

See [actor feedback](@/internals/simulation/fluid-coupling.md#actors) for selection, latency, and CPU response.

## Sleeping and waking

Sleeping is per logical 8×8 tile, not per particle. A tile can sleep after 30 consecutive calm ticks, with maximum net particle displacement below `0.02` cell/tick and positive density-constraint error below `0.02`. Net displacement is measured from tick-start position, so temporary gravity motion canceled by pressure does not itself prevent sleep. A tick in which the tile was woken resets the calm counter.

Sleeping particles remain in neighbor bins and the mass raster. They contribute density, but prediction, pressure correction, and commit keep their positions fixed; commit zeros their velocity. Sleep decisions use workgroup-local hash/reduction slots before updating global per-tile maxima.

Wake sources include:

- new spawns, a first valid particle tile, and particle erasures;
- active-region rebasing;
- changes in a hash of solid occupancy, including adjacent changed tiles;
- a nonzero moving-boundary gradient sampled at the tile's nodes;
- an actor's bounds expanded by its motion and a one-cell margin;
- an awake neighboring particle tile whose previous tick's displacement exceeds `0.1` cell;
- an entering particle predicted into a sleeping tile.

Entry wakes are deferred until the first prediction finishes reading sleep state. If any tile wakes, an indirect second prediction runs over the particles; already-awake predictions are recomputed from unchanged committed state, while newly awake particles receive prediction before sorting. This avoids modifying sleep decisions while the first prediction is using them.

> **Limitation:** A sleeping pool still incurs binning, rasterization, and tile-management work; sleeping is not a complete suspension of all GPU dispatches. Solid wake detection uses occupancy hashes, not an exact history of geometry changes.
