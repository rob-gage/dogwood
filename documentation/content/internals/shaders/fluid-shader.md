+++
title = "Fluid particle shader"
description = "Compute stages, bind groups, packed records, raster layout, and asynchronous readbacks."
weight = 35
[extra]
sources = ["engine_world/src/scenes/fluids", "engine_world/src/scenes/thermal/thermal_transition.wgsl"]
+++

`fluid_particle.wgsl` is driven by `FluidParticlePass`. This page owns the GPU layout and dispatch reference; [fluid simulation](@/internals/simulation/fluids.md) explains the numerical algorithm. Rust substitutes `FLUID_BIN_SIZE` for the WGSL source placeholder when creating the module.

## Dispatch sequence

The pass records the following work in one command encoder, using one compute pass with pipeline and bind-group changes:

| Phase | Entry points |
| --- | --- |
| Previous raster cleanup | `prepare_raster`, `clear_fluid_raster_tiles`, `reset_fluid_raster_tile_count` |
| Optional thermal conversion | `prepare`, `convert_particles`, `convert_cells` |
| Optional erase/rebase/export | `prepare`, `compact`, `finish_compaction` |
| Optional Insert/import occupancy | `begin_occupancy`, `prepare`, `mark_occupancy` |
| Spawn edits and imports | `spawn`, `prepare`, `spawn_imported`, when needed |
| Sparse solid map | `prepare_boundary`, `clear_boundary_tiles`, `reset_boundary_tile_count`, `prepare`, `mark_boundary_tiles`, `dilate_boundary_tiles`, `prepare_boundary`, `boundary`, `boundary_weights`, `prepare_boundary`, `boundary_motion`, `boundary_nodes` |
| Sleep preparation | `prepare_sleep`, `wake_actor_neighbors`, `wake_solid_neighbors` |
| Raster momentum cleanup | `prepare_boundary`, `clear_fluid_current_tiles` |
| Contact consumption | `prepare`, `apply_fluid_contacts`, `clear_fluid_contacts` |
| Each substep | `predict`, optional first-substep wake/re-prediction, optional `project_actors`, neighbor sort, two `lambda`/`correct` rounds with optional actor projection, `commit`, optional `commit_actor_velocity` |
| After substeps | `xsph`, `apply_xsph`, `finish_sleep`, `prepare`, `rasterize_fluid`, optional `actor_contacts` |

The neighbor sort is `clear_bins`, `count_bins`, `scan_bins`, `scan_blocks`, `scan_block_prefix`, `add_block_prefix`, `scatter`. First-substep wake handling uses `reset_wake_count`, `apply_pending_wakes`, and `prepare_wake_dispatch` around prediction. Copies for count, import status, actor feedback, and outgoing exports follow compute before submission.

Particle workgroups contain 128 invocations; scans use 256; raster clearing and boundary cells/nodes use 64. Tile-management and indirect-preparation kernels use one invocation per workgroup. `dilate_boundary_tiles` uses 64-thread workgroups; its Rust dispatch currently divides tile count by `FLUID_WORKGROUP_SIZE` (128), so this is not a full-coverage 64-thread dispatch for every region size.

> **Limitation:** The boundary dilation dispatch divisor differs from the shader workgroup size. The documentation describes the current implementation; full intended tile coverage is not guaranteed by that dispatch.

## Bind groups

Group 0 binding 0 is a dynamic uniform buffer. The main solver layout has nineteen storage bindings; specialized layouts select smaller subsets for preparation, boundary motion, actor projection, actor sampling, and actor velocity commit. Group 1 binding 0 is writable indirect arguments, supplied only to preparation pipelines.

| Group 0 binding | Resource |
| --- | --- |
| 1 / 2 | Cellular identifiers / fluid material records |
| 3–4 | Two particle-state arrays |
| 5–6 | Two predicted-position arrays |
| 7 | Lambdas |
| 8 | Solid-cell weight and boundary motion |
| 9 / 10 | Dense-bin scan storage / control, occupancy stamps, sleep state |
| 11 / 12 | Spawn commands / erasure masks |
| 13–14 | Sparse boundary tile data / boundary corner-node map |
| 15 / 16 | Compact neighbor records / fluid raster and contacts |
| 18–20 | Rigid-body cell lookup, tile records, and body motion records |
| 22 | Packed cellular kinematics |
| 23–24 | Shared-layout actor records / actor feedback |
| 25–27 | Import commands, rejection flags, and particle exports |

The main solver layout uses its existing storage bindings. The conversion layout adds bindings
40–53 for writable cellular material, color, kinematics, matter, gas, conversion lists, rigid-body
cells, changed rigid-body cells, awake flags, temperature heat changes, transition colors, and gas
properties. Thermal materials and awake flags reuse bindings 29 and 30; static material debris
records use binding 54. Only conversion entry points use this reduced layout, which stays within
the per-stage storage binding limit.

## Packed records

All words are serialized little-endian by Rust. Sizes below are record strides, not native Rust struct layouts.

| Record | Bytes | Fields |
| --- | ---: | --- |
| Particle | 32 | Position xy at 0, velocity xy at 8, material ID at 16, tick-start xy at 20, padding at 28 |
| Exact-position import | 32 | Position xy, velocity xy, material ID, three padding words |
| Spawn command | 16 | Local integer cell xy, material ID, Insert flag |
| Predicted position | 8 | Two `f32` coordinates |
| Neighbor | 20 | Old position xy, cohesion, artificial pressure, sleeping flag |
| Lambda | 4 | `f32` |
| Boundary cell/node | 16 | Cell: psi, velocity xy, body index; node: density, gradient xy, moving-gradient scalar |
| Actor | 64 | Shared [cellular tile actor layout](@/internals/shaders/cellular-tile-shader.md#actors-and-rigid-bodies); word 14 is the idle-swimmer flag |
| Actor feedback | 32 | Signed fixed-point impulse xy, immersion, current xy, density, drag, unused word |
| Import result | 4 | Zero accepted, nonzero rejected |
| Export | 40 | Whole world-cell x/y as four low/high words, fractional xy, velocity xy, material ID, padding |

Exports have a leading `u32` count. Resident snapshots instead hold a leading live count plus the packed particle array. The [material table](@/internals/simulation/material-table.md#fluid-property-buffer) owns the 32-byte material layout; [chunk format](@/internals/world-streaming/chunk-format.md#fluid-particle-section) owns the distinct 32-byte disk layout.

### Uniforms

The binding uses 152 bytes at each 256-byte dynamic stride. Four setup slots precede nine slots per substep (40 slots at current settings).

| Byte offsets | Fields |
| --- | --- |
| 0–28 | Tile dimensions, ring offsets, cell dimensions, bin dimensions |
| 32–60 | Bin count, capacity, state side, predicted side, edit/erasure counts, velocity cap, scan-block count |
| 64–96 | Relaxation, rho0, gravity increment, rebase xy, substep count, cohesion scale/normalization, raster scale |
| 100–116 | Fluid-material count, collision-relative region origin xy, substep index, import count |
| 120–132 | World origin low xy and high xy |
| 136–148 | Actor count, contact scale, actor time, tick |

Sparse contact clearing sizes the shared indirect dispatch for contact tiles. The pass restores the live-particle dispatch before prediction, sorting, and the solver substeps.

The two particle buffers alternate during compaction, scattering, and commit. Predicted positions alternate during scatter and Jacobi correction. CPU uniform slot selection tracks both sides; the WGSL is not free to select arbitrary current buffers.

## Raster and control storage

For `C` physical ring cells and `T` tiles, the fluid raster contains:

```text
4*C words     mass, momentum x/y, selected material per cell
T words       sparse physical-tile flags
T words       sparse physical-tile list
1 word        listed-tile count
2*C words     signed fixed-point contact velocity delta x/y per cell
```

Mass/momentum/contact deltas use scale `4096`; actor feedback uses scale `1024`. Mass/momentum and actor-feedback additions saturate to signed 32-bit range. Contact-delta accumulators use ordinary atomic integer addition, with individual contributions clamped before conversion.

Control begins with live count, survivor count, listed-boundary count, and occupancy generation, followed by one occupancy stamp per logical cell. Each logical tile has seven sleep words: maximum net displacement, maximum positive density error, calm ticks, solid hash, current flags, prior displacement, and prior flags. A wake-count word and a particle-transition count word follow. The transition counter is cleared each tick, incremented during fluid rasterization for particles due to convert, and copied with the live count for asynchronous CPU polling.

With `P` particle capacity, the two state arrays, two predicted arrays, neighbor records, and lambdas use `104 * P` bytes before import/export/readback storage. Boundary-cell and corner-node maps use 16 bytes per cell/node independently of particle capacity. Empty storage allocations have a 16-byte minimum.

Bin storage has counts, exclusive offsets, and scatter cursors, then block totals/prefixes and chunk totals/prefixes. Boundary tiles store flags, a compact list, and sixteen packed occupancy words per tile; each word holds four cell occupancy bytes.

The first `predict` substep reads the pressure buffer published by the prior tick and applies pressure push above the 0.02-atmosphere threshold. See [pressure simulation](@/internals/simulation/pressure.md#published-pressure-and-consumers).

## Readback policy

Count, import status, and actor feedback each have three staging slots. Count and actor copies are skipped when all slots are busy; their previous CPU state remains. Import submissions wait for a status slot so every submitted record can be accepted or returned. Particle counts retain submission tick and spawn revision; actor slots retain identities and collision settings. The particle count staging copy includes both live-particle count and due-transition count. [Fluid streaming](@/internals/world-streaming/fluid-streaming.md) explains reservations and record ownership.

One separate resident snapshot may be mapped at a time. Ordinary reads use `Poll`; import draining and final live-pool reads use `wait_indefinitely` during teardown. Mapping or polling failures propagate as [scene errors](@/architecture/errors.md#fluid-failures).
