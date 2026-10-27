+++
title = "Fluid simulation"
description = "The particle solver, its numerical model, and where to find coupling and persistence details."
weight = 75
[extra]
sources = ["engine_world/src/scenes/fluids", "engine_world/src/scenes/thermal"]
+++

Fluids are GPU particles, separate from the cellular tile arrays. A particle represents one cell's fluid mass but can move continuously and share a cell with other particles. `Fluids` owns edits, imports, count reservations, and actor feedback; `FluidParticlePass` owns storage and compute submission.

This page describes the implemented numerical model. See [boundaries and sleeping](@/internals/simulation/fluid-boundaries.md), [system interactions](@/internals/simulation/fluid-coupling.md), [streaming and persistence](@/internals/world-streaming/fluid-streaming.md), and [GPU records and dispatch](@/internals/shaders/fluid-shader.md) for the other parts.

## One fixed tick

```text
prepare / convert due particles and listed cells -> compact / rebase / erase
  -> spawn edits and imports -> prepare boundaries and sleeping
  -> apply accumulated fluid contact velocities
  -> four substeps:
       predict -> project actors -> bin and scatter particles
       -> two rounds of density lambda / position correction / actor projection
       -> commit position and velocity -> restore actor-normal velocity
       -> XSPH velocity smoothing
  -> velocity decay -> finish sleeping
  -> rasterize mass and momentum -> sample actors -> queue readbacks / exports
```

A tick is skipped when nothing could change. That requires all of the following and no recent
thermal sample listing a conversion into fluid:

- A count readback newer than every particle edit, erasure, import, and spawn reports an empty pool.
- No edits, erasures, or imports are queued, including rejected imports.
- No export is requested.
- The active region has not moved.

The skipped tick submits no work, so the fluid raster stays as the last tick left it, empty.

The tick is 1/60 second. Positions use cells relative to the active region; velocities use cells per tick, including during substeps. Gravity in tiles/s² converts to a substep velocity increment as `gravity * CELLULAR_TILE_SIZE * dt² / FLUID_SUBSTEP_COUNT`. Prediction divides velocity by the substep count to obtain displacement.

When a material transition starts or ends in a fluid, the pass converts due live particles and
then consumes the preceding thermal tick's list of cells or gas slots due to become fluid.
Compaction runs for a recent nonzero particle-transition sample, a nonzero thermal fluid-conversion
list, or an edit, rebase, or export that requires it. Particle-to-cell conversions wait for an empty,
unowned cell; cell and gas conversions wait for pool capacity. The thermal list holds at most
`THERMAL_CONVERSION_COUNT_MAXIMUM` entries, and asynchronous count readback keeps the pool awake
briefly after a nonzero list.

## Neighbors

The solver uses dense spatial bins, three cells wide. Each substep clears bin counts, counts particles, computes exclusive prefix sums, and scatters particle state and predicted positions into contiguous bin ranges. The hierarchical scan uses workgroup-local scans, scans of block totals, and a final prefix addition; it does not require GPU subgroup operations. Chunk totals are scanned in one 256-thread workgroup when they fit; larger chunk counts fall back to a serial loop in lane zero.

Neighbor queries scan contiguous spans of row-major bins intersecting a search circle. They then reject particles beyond the actual kernel radius. The search radius is `2.5 + 2 * 0.25 = 3` cells, allowing for two capped position corrections after sorting. The current two-iteration configuration sorts once per substep. Code for more iterations inserts another sort after the second correction.

Scatter also caches each neighbor's old position, cohesion, artificial pressure, and sleeping flag. Position corrections use separate input/output arrays, so a Jacobi iteration reads a consistent previous result rather than partially updated neighbors. Particle ordering within a bin depends on atomic scatter order.

## Density constraints

This is a two-dimensional position-based fluids (PBF) solver with poly6 density weights and spiky gradients. For support radius `h = 2.5`:

```text
W(r)       = 4 / (pi * h^8) * (h² - r²)^3                  for r < h
 grad W(r) = -30 / (pi * h^5) * (h - r)^2 * direction(r)   for 0 < r < h
```

`reference_density()` sums the density kernel on a unit square lattice over offsets -2 through 2. This common `rho0` describes particle spacing. It is distinct from material `density`, which supplies physical mass for rasterization and body reactions. Different fluid materials participate in the same neighbor solve without mass-weighted density constraints or separation by material.

For each awake particle, density includes its self kernel, neighboring particle kernels, and sampled boundary density. The constraint is `C = density / rho0 - 1`. Lambda is `-C / (|sum gradients|² + sum neighbor gradient norms² + relaxation)`, with `relaxation = 0.01`. Boundary gradients contribute to the summed gradient. Sleeping neighbors contribute density but no movable-neighbor gradient norm and have lambda zero.

Each Jacobi pass computes one complete correction for a particle by summing its pairwise pressure and cohesion terms and its boundary-gradient term. Before the length cap and solid projection, the solver scales this accumulated displacement by `FLUID_POSITION_RELAXATION / FLUID_ITERATION_COUNT`. `FLUID_POSITION_RELAXATION = 0.5` is the total correction budget across one substep, divided evenly across its Jacobi passes. This global under-relaxation suits the already accumulated per-particle PBF constraint; dividing by neighbor count would count pair contributions again and change the relative effect of the density solve and cohesion. `FLUID_RELAXATION` remains the separate epsilon in the lambda denominator.

The implementation uses signed `C`: it does not clamp under-density to zero. The sleep reduction records only positive density error.

## Pressure correction and cohesion

Each correction combines:

- pressure from the particle and its neighbors' lambdas;
- artificial pressure `s_corr = -artificial_pressure_i * (W(r) / W(0.3h))^4`;
- the boundary-gradient correction;
- an area-normalized, signed two-dimensional Akinci cohesion spline.

Artificial pressure uses the receiving particle's coefficient. Cohesion uses the average of the pair's coefficients, multiplied by `FLUID_COHESION_SCALE`. For `p = r³ * (h - r)³ / h^6`, the spline uses `2p - 1/64` through `h/2` and `p` above it, scaled by `FLUID_COHESION_NORMALIZATION_2D / h²`. It is zero at effectively coincident points and outside kernel support. These controls are distinct: one is the short-range artificial-pressure term, the other the cohesion term.

Nonfinite corrections become zero. After under-relaxation, each correction vector is capped to length `0.25` cell, then projected out of solid squares and actors. [Boundary handling](@/internals/simulation/fluid-boundaries.md) describes these projections.

## Velocity and viscosity

Commit reconstructs velocity as `(corrected_position - old_position) * substep_count`. It caps vector length to four cells/tick and replaces nonfinite values with a valid prior position or zero velocity. Actor-normal velocity can subsequently be restored to keep a moving actor from sweeping through particles after that cap.

After each substep's position and velocity commit, XSPH smooths velocities toward the kernel-weighted neighbor mean. A separate apply dispatch prevents readers seeing partially smoothed neighbors. `viscosity` is a nonlinear control in `[0, 1]`: its per-tick relaxation fraction is `1 - (1 - viscosity)^(1 + 4 * viscosity^3)`, then divided evenly across substeps in exponential form. This keeps the same material control's per-tick effect stable if the substep count changes. Low values remain close to their former linear meaning, while high values approach rigid local motion. The pass caps speed again and multiplies velocity by `0.998` after all substeps, before [sleep testing](@/internals/simulation/fluid-boundaries.md#sleeping-and-waking).

Material `drag` is a separate swimming/current-coupling control; it is not the coefficient of the XSPH calculation. Fluid `friction` and viscosity also increase tangential damping when a particle contacts a solid boundary; see [boundary handling](@/internals/simulation/fluid-boundaries.md#solid-projection).

> **Limitation:** This solver uses fixed iteration counts, bounded corrections, velocity decay, approximate boundaries, and delayed body feedback. It does not enforce exact incompressibility or exact momentum conservation across all systems. There is no material-specific rest spacing or immiscible-fluid separation rule.

Public solver constants are defined in `engine_world/src/scenes/fluids/mod.rs`; [resource limits](@/architecture/resources-and-lifetimes.md#fluid-resources) collect their current values.
