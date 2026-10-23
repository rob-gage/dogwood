+++
title = "Gas shader"
description = "Reference for gas compute entry points, bindings, storage records, and simulation tuning constants."
weight = 36
[extra]
sources = ["engine_world/src/scenes/gases/gas_cellular_tile.wgsl", "engine_world/src/scenes/gases/gas_cellular_tile_pass.rs", "engine_world/src/scenes/gases/mod.rs", "engine_world/src/scenes/cellular_tiles/cellular_tile_buffers.rs", "engine_world/src/materials/material_table.rs"]
+++

`gas_cellular_tile.wgsl` is driven by `GasCellularTilePass`. This page is the GPU layout and dispatch reference; [gas simulation](@/internals/simulation/gases.md) describes the numerical model. The [tile ring](@/internals/world-streaming/tile-ring.md) page covers ring indexing and related cellular buffers.

## Entry points and dispatch

The pass dispatches these entry points in one compute pass:

| Order | Entry point | Dispatch |
| --- | --- | --- |
| 1 | `volume` | One 64-thread workgroup per physical tile, across the whole ring |
| 2 | `collect` | 64 threads per workgroup, over logical region tiles |
| 3 | `clear` | 64 threads per workgroup, over logical region tiles |
| 4 | `prepare` | One invocation, writes indirect arguments |
| 5 | `step` | Indirect, one 64-thread workgroup per active tile, four substeps |

Each `step` invocation handles one cell of an 8×8 tile. The four substeps alternate the input and output gas bindings between the cellular tile buffer and scratch buffer. The even count leaves final state in the cellular tile buffer. `collect` lists awake tiles and their neighbors; the active count drives the indirect dispatch.

## Uniforms and bindings

`GasUniforms` is 48 bytes, with twelve 32-bit fields in this order:

| Offset | Field | Units or meaning |
| ---: | --- | --- |
| 0 | `region_width` | Tiles |
| 4 | `region_height` | Tiles |
| 8 | `ring_offset_x` | Physical tile offset |
| 12 | `ring_offset_y` | Physical tile offset |
| 16 | `ambient_amount` | Gas amount units per cell |
| 20 | `ambient_mass` | Mass per atmosphere of phantom air |
| 24 | `gravity` | Cells per substep squared, after scaling |
| 28 | `fluid_raster_scale` | Fixed-point scale used by the fluid raster |
| 32 | `actor_count` | Number of actor records to read |
| 36 | `actor_velocity_scale` | Converts actor velocity to cells per substep |
| 40 | `padding_1` | Unused |
| 44 | `padding_2` | Unused |

The bind groups contain these resources:

| Group | Binding | Resource | Access |
| ---: | ---: | --- | --- |
| 0 | 0 | `GasUniforms` | Uniform |
| 0 | 1 | Cellular material identifiers | Read |
| 0 | 2 | Input gas records | Read |
| 0 | 3 | Output gas records | Read/write |
| 0 | 4 | Per-tile `gas_awake` flags | Read/write atomics |
| 0 | 5 | Per-cell open volume and packed drag | Read/write |
| 0 | 6 | Active-tile `GasControl` record | Read/write atomics |
| 0 | 7 | Gas material records | Read |
| 0 | 8 | Fluid raster | Read |
| 0 | 9 | Fluid material records | Read |
| 0 | 10 | Kinematics actor records | Read |
| 1 | 0 | Indirect dispatch record | Read/write; `prepare` writes it |

The actor record matches the kinematics actor record layout.

## Storage records

`gas_buffer` stores eight `u32` words per cell, or 32 bytes:

| Words | Contents |
| --- | --- |
| 0–3 | Four species slots; gas index in the high 8 bits, amount in the low 24 bits |
| 4 | Phantom air amount |
| 5 | Right-face flow as `f32` bits |
| 6 | Top-face flow as `f32` bits |
| 7 | Padding |

`volumes` is one `vec2<u32>` per cell. Word 0 stores open-volume units at scale 65,536 in its lower bits and `VOLUME_DRAGGED` in its high bit. Word 1 stores the packed signed-normalized right- and top-face drag velocities. The velocity pair is encoded relative to `ACTOR_VELOCITY_MAXIMUM`.

`gas_awake` has one `u32` per tile. `TILE_AWAKE` (`1`) marks changed gas or open volume; `TILE_ACTIVE` (`4`) marks a tile scheduled this tick. `GasControl` contains an atomic active count followed by logical tile indices. The indirect dispatch buffer contains three `u32` workgroup counts (x, y, z), written by `prepare`.

Gas material records are `vec4<f32>` with a 16-byte stride: mass, diffusivity, cohesion, then packed color bits. Fluid records are `vec4<f32>` pairs with 32-byte stride; gas occupancy reads density from the first record's first component. See [material table compilation](@/internals/simulation/material-table.md).

## Shared-memory halo

Each active-tile workgroup loads a 14×14 cell halo: its 8×8 tile plus three cell rings. The halo retains species slots, phantom air, prior face flows, open volume, actor drag, and derived pressure/density. It computes new flows for a 12×12 region, then transfers and admission records for a 10×10 region, covering every face that can affect the tile.

Neighboring workgroups recompute overlapping flows, transfers, and admissions from the same previous substep. Transfer fractions use the same integer arithmetic and round-down `transfer()` calculation on both sides of a face, so the duplicated calculations produce the same conserved transfer without cross-workgroup synchronization.

## Tuning constants

| Constant | Value | Meaning |
| --- | ---: | --- |
| `GAS_AMOUNT_PER_ATMOSPHERE` / `AMOUNT_PER_ATMOSPHERE` | 65,536 | Amount units per atmosphere |
| `GAS_AMOUNT_MAXIMUM` | `0xff_ffff` | Maximum amount in one species slot |
| `GAS_TIME_STEP` | 1/60 s | Fixed simulation tick |
| `GAS_SUBSTEP_COUNT` | 4 | Flow and transfer substeps per tick |
| `GAS_BUOYANCY_SCALE` | 0.5 | Fraction of scene gravity applied to gas |
| `TRANSFER_SCALE` | 65,536 | Integer units per whole transfer fraction |
| `VOLUME_SCALE` | 65,536 | Integer units per whole open volume |
| `ACTOR_DRAG_RANGE` | 6.0 | Actor drag reach in actor radii |
| `ACTOR_WAKE` | 1.0 | Wake stream strength factor |
| `ACTOR_WAKE_OFFSET` | 3.0 | Wake center offset behind actor, in radii |
| `ACTOR_WAKE_LENGTH` | 3.0 | Wake length along actor path, in radii |
| `ACTOR_DRAG` | 0.2 | Face-flow blend toward actor drag per substep |
| `ACTOR_VELOCITY_MAXIMUM` | 8.0 | Actor velocity scale in cells per tick |
| `BULK_VISCOSITY` | 0.1 | Flow response to divergence difference |
| `FLUID_OCCUPANCY_CLOSED` | 0.5 | Smoothed fluid occupancy that closes a cell |
| `VOLUME_MINIMUM` | 0.25 | Minimum volume used for pressure |
| `PRESSURE_RESPONSE` | 0.2 | Flow response to pressure difference |
| `FLOW_DAMPING` | 0.004 | Fraction of face flow removed per substep |
| `FLOW_MINIMUM` | 32.0 | Smaller flow is set to zero |
| `ADVECTION_SPEED_MAXIMUM` | 0.9 | Maximum flow advection speed per substep |
| `BULK_FRACTION_MAXIMUM` | 0.5 | Maximum total cell amount transferred per substep |
| `AMBIENT_DIFFUSIVITY` | 0.02 | Phantom-air diffusion fraction per neighboring cell |
