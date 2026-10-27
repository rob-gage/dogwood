+++
title = "Fluid simulation"
description = "The particle solver, its numerical model, and where to find coupling and persistence details."
weight = 75
[extra]
sources = ["engine_world/src/scenes/fluids"]
+++

Fluids are GPU particles, separate from the cellular tile arrays. A particle represents one cell's fluid mass but can move continuously and share a cell with other particles. `Fluids` owns edits, imports, count reservations, and actor feedback; `FluidParticlePass` owns storage and compute submission.

This page describes the implemented numerical model. See [boundaries and sleeping](@/internals/simulation/fluid-boundaries.md), [system interactions](@/internals/simulation/fluid-coupling.md), [streaming and persistence](@/internals/world-streaming/fluid-streaming.md), and [GPU records and dispatch](@/internals/shaders/fluid-shader.md) for the other parts.

## One fixed tick

```text
prepare -> compact / rebase / erase
  -> spawn edits and imports -> prepare boundaries and sleeping
  -> apply accumulated fluid contact velocities
  -> four substeps:
       predict -> project actors -> bin and scatter particles
       -> two rounds of density lambda / position correction / actor projection
       -> commit position and velocity -> restore actor-normal velocity
  -> XSPH velocity smoothing -> velocity decay -> finish sleeping
  -> rasterize mass and momentum -> sample actors -> queue readbacks / exports
```

A tick is skipped when nothing could change. That requires all of the following:

- A count readback newer than every particle edit, erasure, import, and spawn reports an empty pool.
- No edits, erasures, or imports are queued, including rejected imports.
- No export is requested.
- The active region has not moved.

The skipped tick submits no work, so the fluid raster stays as the last tick left it, empty.

The tick is 1/60 second. Positions use cells relative to the active region; velocities use cells per tick, including during substeps. Gravity in tiles/s² converts to a substep velocity increment as `gravity * CELLULAR_TILE_SIZE * dt² / FLUID_SUBSTEP_COUNT`. Prediction divides velocity by the substep count to obtain displacement.

Material transitions can be configured through thermal properties, but they are not simulated.

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

The implementation uses signed `C`: it does not clamp under-density to zero. The sleep reduction records only positive density error.

## Pressure correction and cohesion

Each correction combines:

- pressure from the particle and its neighbors' lambdas;
- artificial pressure `s_corr = -artificial_pressure_i * (W(r) / W(0.3h))^4`;
- the boundary-gradient correction;
- an area-normalized, signed two-dimensional Akinci cohesion spline.

Artificial pressure uses the receiving particle's coefficient. Cohesion uses the average of the pair's coefficients, multiplied by `FLUID_COHESION_SCALE`. For `p = r³ * (h - r)³ / h^6`, the spline uses `2p - 1/64` through `h/2` and `p` above it, scaled by `FLUID_COHESION_NORMALIZATION_2D / h²`. It is zero at effectively coincident points and outside kernel support. These controls are distinct: one is the short-range artificial-pressure term, the other the cohesion term.

Nonfinite corrections become zero. Each correction vector is capped to length `0.25` cell, then projected out of solid squares and actors. [Boundary handling](@/internals/simulation/fluid-boundaries.md) describes these projections.

## Velocity and viscosity

Commit reconstructs velocity as `(corrected_position - old_position) * substep_count`. It caps vector length to four cells/tick and replaces nonfinite values with a valid prior position or zero velocity. Actor-normal velocity can subsequently be restored to keep a moving actor from sweeping through particles after that cap.

After all four substeps, XSPH smooths velocity once per tick. It adds `viscosity_i` times the kernel-weighted mean of neighboring velocity differences, normalized by the neighbor weight sum. A separate apply dispatch prevents readers seeing partially smoothed neighbors. It caps speed again and multiplies velocity by `0.998` before [sleep testing](@/internals/simulation/fluid-boundaries.md#sleeping-and-waking).

`viscosity` is this smoothing fraction. Material `drag` is a separate swimming/current-coupling control; it is not the coefficient of the XSPH calculation.

> **Limitation:** This solver uses fixed iteration counts, bounded corrections, velocity decay, approximate boundaries, and delayed body feedback. It does not enforce exact incompressibility or exact momentum conservation across all systems. There is no material-specific rest spacing or immiscible-fluid separation rule.

Public solver constants are defined in `engine_world/src/scenes/fluids/mod.rs`; [resource limits](@/architecture/resources-and-lifetimes.md#fluid-resources) collect their current values.
