+++
title = "Pressure shader"
description = "Reference for pressure compute entry points, bindings, cellular state, and shader constants."
weight = 36
[extra]
sources = ["engine_world/src/scenes/pressure/pressure_cellular_tile.wgsl", "engine_world/src/scenes/pressure/pressure_cellular_tile_pass.rs", "engine_world/src/scenes/pressure/mod.rs", "engine_world/src/scenes/cellular_tiles/cellular_tile_buffers.rs", "engine_world/src/materials/material_table.rs"]
+++

`pressure_cellular_tile.wgsl` is driven by `PressureCellularTilePass`. This page records GPU bindings, packed state, and dispatch; [pressure simulation](@/internals/simulation/pressure.md) describes the model. The [tile ring](@/internals/world-streaming/tile-ring.md) documents physical ring indexing.

## Entry points and dispatch

The pass records these stages in one compute pass:

| Order | Entry point | Dispatch |
| --- | --- | --- |
| 1 | `volume_collect` | On unchanged region/ring, scans active tiles to list volume-dependent tiles |
| 2 | `prepare_volume` | On unchanged region/ring, prepares sparse volume dispatch |
| 3 | `volume` | Full-ring dispatch after region/ring change; otherwise indirect sparse dispatch |
| 4 | `collect` | 64 threads per workgroup, over active-region tiles |
| 5 | `clear` | 64 threads per workgroup, consumes awake flags |
| 6 | `prepare` | One invocation, writes indirect arguments |
| 7 | `step` | Indirect, one workgroup per active tile, 12 substeps |
| 8 | `publish` | Indirect, writes gauge atmosphere pressure and wakes pushing neighbors |
| 9 | `damage` | Indirect, breaks exhausted solid cells and records topology changes |

Each `step` workgroup loads a halo around an 8×8 tile and writes the next pressure/gas state into the alternate buffer. Twelve is even, so the result ends in `CellularTileBuffers::gas_buffer` before publication and damage.

## Uniforms and bindings

`PressureUniforms` has 24 32-bit words (96 bytes): region width and height; ring offsets; ambient amount and mass; gravity and fluid raster scale; actor count; inverse substep count; squared gas speed; per-substep flow damping, bulk viscosity, flow minimum, ambient diffusivity, actor drag, solid response, solid speed, solid relaxation, trapped expulsion, and damage rate; a volume-list flag and padding.

Group 0 bindings 0–19 are: uniforms; cellular material identifiers; input gas state; output gas state; gas awake flags; open-volume and actor-drag records; pressure control and active-tile list; gas material records; fluid raster; fluid material records; actor records; static cellular material records; rigid-body tile records; rigid-body cell records; published pressure; dynamic-cell awake flags; kinematics; broken-tile output; color; broken-rigid-body-cell output. Group 1 binding 0 is the indirect dispatch record written by `prepare` or `prepare_volume`.

## Storage records

The cellular gas state is eight `u32` words per cell (32 bytes):

| Word | Contents |
| ---: | --- |
| 0–3 | Four species slots: 8-bit gas index in high bits and 24-bit amount |
| 4 | Phantom-air amount |
| 5 | Right-face flow as `f32` bits |
| 6 | Top-face flow as `f32` bits |
| 7 | Signed stored pressure in amount units for solid cells; solver state for other cell kinds |

Open-volume records use two words per cell. The first stores volume at 65,536 units per whole volume and a high-bit dragged flag; the second stores packed right/top actor drag velocities. `pressure_buffer` stores published gauge pressure. Solid material records are four `f32` values (mass, absorption, strength, padding); gas records are three `f32` values (mass, diffusivity, cohesion) and packed color. See [material table compilation](@/internals/simulation/material-table.md).

## Shader constants

| Constant | Value | Meaning |
| --- | ---: | --- |
| `PRESSURE_IMPULSE` | 0.1111 | Consumer impulse per atmosphere and face, scaled by mass·cells/tick |
| `PRESSURE_PUSH_MINIMUM` | 0.02 atm | Minimum pressure magnitude for consumer pushes |
| `AMOUNT_PER_ATMOSPHERE` | 4096 | Gas amount units per atmosphere |
| `AMOUNT_MAXIMUM` | 16,773,120 | Largest amount leaving room under the 4095-atmosphere pressure cap |
| `TRANSFER_SCALE` | 65,536 | Integer units per whole transfer fraction |
| `VOLUME_SCALE` | 65,536 | Integer units per whole open volume |
| `VOLUME_MINIMUM` | 0.25 | Minimum flow-evaluation volume |
| `FLUID_OCCUPANCY_CLOSED` | 0.5 | Smoothed fluid occupancy that closes a cell |
| `ADVECTION_SPEED_MAXIMUM` | 0.9 | Maximum flow advection speed per substep |
| `BULK_FRACTION_MAXIMUM` | 0.5 | Maximum total amount transferred per substep |

The CPU-side per-second coefficients and fixed 12-substep tick are listed on [pressure simulation](@/internals/simulation/pressure.md#flows-volume-and-sleeping).
