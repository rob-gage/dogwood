+++
title = "Kinematics shader"
description = "Reference for the cellular kinematics WGSL entry points, bindings, packed records, and asynchronous readbacks."
weight = 30
[extra]
sources = ["engine_world/src/scenes/kinematics/kinematics_fluids.wgsl", "engine_world/src/scenes/kinematics/kinematics_cellular_tile.wgsl", "engine_world/src/scenes/kinematics/kinematics_pass.rs"]
+++

This page is the reference for `kinematics_cellular_tile.wgsl` and `kinematics_fluids.wgsl`, the GPU stages driven by `KinematicsPass`, for engine contributors. It covers entry points, bindings, packed records, dispatch, and readbacks. The movement algorithm is in [cellular kinematics](@/internals/simulation/cellular-kinematics.md).

## Entry points and dispatch order

Each tick runs these entry points in order:

1. `collect`, `clear`, `prepare`, `integrate`.
2. `prepare` again, to size the substep dispatches.
3. For each substep: `request`, `pick`, `resolve_begin`, alternating `resolve_even` and `resolve_odd`, then `stage` and `commit`.
4. `contact`, last.

All entry points except `prepare` use 64-thread workgroups; `prepare` uses one invocation. A cell-processing workgroup covers one 8x8 cellular tile, and indirect counts limit work to active tiles. The pass uses a 256-byte uniform stride for dynamic offsets.

`collect` marks a tile active when any tile in its 3x3 neighborhood is awake or disturbed, or when the tile's bounds expanded by one tile overlap an actor's bounds. It appends active tiles' logical indices to the indirect-dispatch list.

There is no checkerboard or Margolus partition. Neighboring cells issue requests, a target picks one claimant, and the resolve stages propagate move status.

### Dispatch sizes

- `collect` and `clear` dispatch over the tile count, 64 invocations per workgroup.
- `prepare` dispatches one invocation, twice per tick.
- `integrate` and later stages use generated indirect dispatches. The pass sets dynamic uniform offsets for each substep and resolve pass.
- The dispatch buffer holds nine 12-byte indirect records: one whole-tick dispatch and eight substep dispatches.

### Adaptive substeps and resolve passes

Each tick encodes `substep_count` substeps (1-8) and `resolve_count` resolve passes (even, 2-8; `KINEMATICS_RESOLVE_COUNT` is 8). The pass starts with the maximum counts and updates them from the newest control readback when one arrives; otherwise, it retains the previous counts:

- `integrate` records the largest per-cell step count in `step_count` with `atomicMax`, capped at 8. Rust sets `substep_count = step_count + 1`, clamped to 1-8.
- Each resolve pass sets its bit in `pending_resolves` while move chains are still unresolved. Rust derives an even `resolve_count` from the highest pending-resolve bit. If a pending bit reaches the last pass used for that readback, it selects the maximum count of 8.
- Substeps beyond `step_count` get zero workgroups from `prepare`.
- The uniform buffer holds 8 x 8 slots of 256 bytes, indexed by substep and resolve pass.

## Bindings

Group 0 bindings are declared in WGSL and matched by the Rust layout:

| Binding | WGSL resource | Use |
|---:|---|---|
| 0 | `KinematicsUniforms` | Region dimensions and ring offsets, tick/substep, actor count, gravity, resolve selector, region origin |
| 1-4 | `material_identifiers`, `colors`, `kinematics`, `awake` | Mutable cell and tile state |
| 5 | `materials: array<vec4<f32>>` | Read-only material properties |
| 6-7 | `motions`, `staged` | Per-cell motion/status and temporary incoming cell records |
| 8 | `KinematicsControl` | Atomic active-tile, step and pending-resolve counts plus the active tile list |
| 9 | `actors: array<KinematicsActor>` | Read-only actor records |
| 10 | `contacts: array<atomic<i32>>` | Actor contact and impulse accumulators |
| 11-14 | `rigid_body_cells`, `rigid_body_cellular_tile_cells`, `rigid_body_cellular_tiles`, `rigid_bodies` | Read-only rigid-body raster and motion data |
| 15 | `rigid_body_contacts: array<atomic<u32>>` | Rigid-body impulse, holding-cell, and fluid sample accumulators |
| 16 | `fluid_raster` | Fluid mass, momentum, material, and contact data |
| 17 | `fluid_materials` | Fluid property records |

Group 1 binding 0 is the writable indirect dispatch array. Only the `prepare` pipeline receives this second bind group.

## Source composition

`KinematicsPass` concatenates both WGSL files at Rust compile time into one shader source. `kinematics_fluids.wgsl` owns fluid bindings, constants, sampling/contact helpers, tile wake detection, and the `rigid_fluid_faces` and `rigid_fluid` kernels. Cellular movement retains its fluid contact call sites. Both fragments share one module and the existing pipelines, bindings, and dispatch order.

## CPU/GPU records

The pass serializes these records manually, little-endian; there is no Rust mirror struct for the WGSL layouts.

### Uniforms and cell state

- `KinematicsUniforms` occupies 48 bytes: ten 32-bit scalar fields at offsets 0-36 and `region_origin: vec2<i32>` at 40. Each slot starts 256 bytes after the prior one.
- Packed cell kinematics occupy two `u32` words. `unpack_kinematics` decodes them into the WGSL `CellKinematics` struct: signed 16-bit x/y velocities scaled by 1,024 in the first word, and signed 14-bit x/y progress values scaled by 512 in the second word's low 28 bits, with four flag bits in its high bits, including resting (`CELL_RESTING`, bit 28) and supported (`CELL_SUPPORTED`, bit 29).

### Actors and rigid bodies

`KinematicsActor` is 64 bytes: center (0), velocity (8), rotation (16), size (24), bounds (32), shape tag (48), mass (52), then two padding words (56).

Rust converts units before writing:

- Tiles become cells (multiplied by `CELLULAR_TILE_SIZE`), and velocity components are clamped to +-8 cells per tick.
- Rotation is stored as cosine and sine.
- `size` holds the circle radius in `size.x`, rectangle half extents, or the capsule half segment and radius.
- `bounds` is a box of the farthest extent plus one cell.
- Shape tags: 0 circle, 1 rectangle, 2 capsule.

`KinematicsRigidBodyCellularTile` is 32 bytes:

| Field | Byte offset |
|---|---:|
| `rigid_body` | 0 |
| `padding_0` | 4 |
| `tile` | 8 |
| `cells` | 16 |
| `previous_cells` | 24 |

`KinematicsRigidBody` is 64 bytes:

| Field | Byte offset |
|---|---:|
| `position` | 0 |
| `rotation` | 8 |
| `previous_position` | 16 |
| `previous_rotation` | 24 |
| `velocity` | 32 |
| `center` | 40 |
| `angular_velocity` | 48 |
| `mass` | 52 |
| `padding` | 56 |

The collision pass writes these records and kinematics shares them.

### Contacts and control

- Actor contact records are 32 bytes (eight words). Words 0-1 are impulse, 2-3 summed normal, 4 contact count, and 5 penetration depth. All except the count are fixed-point with scale 1,024 (`CONTACT_SCALE`); words 6-7 are padding.
- Rigid-body contact records are 84 bytes (21 32-bit words): words 0–2 hold x/y and angular impulse; words 3–7 and 8–12 hold each side's contact count, summed normal x/y, and summed arm x/y; word 13 is a fluid-face mask; words 14–20 hold seven fluid sample values.
- Control storage begins with three atomic `u32` counters (active tile count, step count, pending resolves), followed by an `active_tiles` list with capacity for one `u32` per tile. Each entry is the logical index (`y * region width + x`) of an active tile.

## Readback

The pass keeps three staging buffers for each readback category: actor contacts, rigid-body contacts, and control counts. After compute, it copies available data, submits the command buffer, starts asynchronous maps, advances the wrapping tick, and returns without waiting.

When every staging slot of a category is in flight:

- Actor contacts: the copy is skipped and that tick's contacts are lost, because actor contacts are cleared each tick.
- Rigid-body contacts: contacts stay in the GPU accumulator and keep accumulating. A free slot receives a copy, after which the accumulator is cleared. The entry for a removed or replaced body is also cleared.
- Control counts: that tick's snapshot is skipped and the previous counts remain in use.

For the scene-level exchange and units, see [cell and body contact exchange](@/internals/simulation/contact-exchange.md).
