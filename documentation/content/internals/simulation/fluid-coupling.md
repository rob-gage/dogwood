+++
title = "Fluid interactions with cells and bodies"
description = "The raster bridge, contact response, buoyancy, actor feedback, and swimming."
weight = 77
[extra]
sources = ["engine_world/src/scenes/fluids", "engine_world/src/scenes/cellular_tiles", "engine_world/src/scenes/collisions/collision.rs", "engine_world/src/scenes/scene.rs"]
+++

Fluids have no separate Rapier collider. They interact through a shared cell raster and delayed CPU feedback. The particle solver lives in `scenes/fluids`; cellular-side helpers and rigid-body kernels live in `scenes/cellular_tiles/cellular_tile_fluids.wgsl`, composed into `CellularTilePass`. [Scene ordering](@/architecture/scene-pipeline.md#simulation-order) determines which state each side sees.

## Raster bridge

Each particle contributes its material density as mass and mass times velocity as momentum to the physical ring cell containing its center. Accumulators use signed fixed-point values at scale `4096` and saturating integer addition. Cell material selection uses the largest identifier present, not a mass-weighted winner. A sparse physical-tile list tracks cells to clear on the next tick; [record layout](@/internals/shaders/fluid-shader.md#raster-and-control-storage) describes the appended contact data.

Queue order makes the prior fluid submission's raster available to kinematics, which computes solid/fluid velocity changes and writes changes into a separate per-cell accumulator. The particle pass applies those changes to particles in the cell before prediction, then clears them. Its final raster is available for the next kinematics submission and current rendering. Idle-swimmer current changes written after rasterization remain for the next particle step.

> **Limitation:** A cell can contain multiple fluid materials, but the raster retains only total mass/momentum and one selected identifier. Downstream density, friction, drag, and color therefore approximate mixed cells using that identifier. Rasterized light accumulates emission and occlusion from all particles in the cell.

## Dynamic-cell contacts

For a dynamic cell not covered by an actor or rigid body, kinematics samples its four neighboring fluid cells. Immersion is clamped average mass divided by the density of the sampled material with the largest summed mass. Buoyancy adds `gravity_per_tick * immersion * fluid_density / cell_mass` to vertical velocity.

Every wet face also solves a contact using relative normal speed and effective inverse mass. Fluid cell mass is shared across its neighboring solid faces. Only approaching contacts above `0.0001` cell/tick produce an impulse. Normal impulse uses restitution; tangential impulse is limited by Coulomb friction. Dynamic-cell contacts use the smaller of cellular and fluid friction/restitution coefficients. The cell receives its velocity change immediately. The opposite impulse is divided by the full fluid cell mass before its velocity change is queued, because every particle in that raster cell receives the change; the face share limits the solved impulse only.

Fluid adjacency or current can activate resting cellular tiles. Cellular sleep and particle sleep are separate systems; a live fluid raster can keep cellular work active even when fluid particles themselves are sleeping.

## Cellular rigid bodies

After ordinary cellular contacts, `rigid_fluid_faces` counts wet faces across the full physical region, including sleeping tiles. `rigid_fluid` then computes buoyancy and wet-face current moments. Each tile reduces contributions for its lowest-index body in workgroup memory; additional bodies sharing that tile accumulate directly.

Buoyancy searches left and right from each rigid cell, traversing at most 32 cells. After leaving the body it searches up to three cells for fluid with raster mass at least half its material density. It uses the larger sampled density to add upward impulse and its torque arm. This side sampling differs from counting water above a body.

Face contacts queue opposite velocity changes to fluid. The body's CPU response uses sampled wet-face mass, momentum, angular moment, squared-arm moment, and mass-weighted arm, rather than applying a delayed face-velocity drag impulse directly. On returned contact data, collision relaxes the current body velocity toward the sampled current with fraction `1 - exp(-k * fluid_mass / body_mass)`, where `k = 1.0`. It similarly relaxes angular velocity using the arm moment and inertia, subtracting the translational contribution to current spin. The rigid-body response does not use material `drag`.

[Contact exchange](@/internals/simulation/contact-exchange.md) owns tuple layouts, units, accumulation, and readback timing. Buoyancy and face samples are cell-based approximations, not integration over an exact submerged polygon.

## Actors

`CellularTiles` builds the shared actor list from positioned physical colliders whose filter includes `COLLISION_GROUP_CELLULAR_TILE`, within a four-tile region margin. Fixed actors receive a large effective mass; invalid masses fail validation. The list is capped at 32. Contact-only sensors are not fluid obstacles. Fluid actor projection uses these uploaded records, not actor positions uploaded again after the Rapier step.

The particle solver reports projection reaction impulse plus immersion, average current, dominant density, and drag. Sampling tests cell centers against an expanded rotated shape with a `2.5`-cell margin, skipping solid or rigid-body cells. Immersion is average sampled fluid mass divided by dominant density, clamped to `[0, 1]`. Dominance is selected by summing mass attributed to each raster material. Average current is total momentum divided by total mass.

Readbacks carry actor identity, collision settings, and submission tick. Samples are rejected when the actor disappears or its collision settings change. The newest matching sample replaces the cache; otherwise the previous sample remains.

Dynamic actors receive the projection impulse and upward impulse from `actor_mass * gravity * immersion * fluid_density / actor_density * dt`. Actor density is mass divided by analytic collider area in cells. Fixed actors receive no CPU fluid response. Generic dynamic actors do not use the sampled material `drag` for an additional drag law.

> **Limitation:** The cached fluid actor sample retains its impulse as well as its immersion/current. A dynamic actor can therefore receive the same projection impulse on multiple CPU steps while a newer readback is unavailable. This differs from the once-only cached cellular velocity changes.

## Pawns and currents

Kinematic pawns use immersion/current/density/drag for [swimming](@/internals/actors/pawn-movement.md#swimming), rather than applying the fluid projection-impulse field directly. Swimming uses threshold hysteresis, buoyancy, exponential relative-velocity drag, and directional thrust. Walking remains active below the thresholds; shallow immersion does not independently enable the swimming drag law.

Only swimmers with no horizontal, jump, or crouch intent set the GPU swimming flag. Those idle swimmers draw nearby water velocity toward actor velocity with `1 - exp(-material_drag * coverage * dt)`. Stroking swimmers still displace particles by projection, but do not add this current-following term. This prevents swim-speed feedback through the current used by the controller.

> **Limitation:** Body coupling is staggered: particle geometry uses uploaded actor state and rasterized rigid bodies, while CPU reactions use asynchronous earlier samples. It is not one simultaneous fluid/Rapier constraint solve.


Fluid particles read published gauge pressure in the first `predict` substep and apply the shared pressure impulse when the magnitude is at least 0.02 atm. The publication is from the preceding tick; see [pressure simulation](@/internals/simulation/pressure.md#published-pressure-and-consumers).
