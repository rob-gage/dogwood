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
| 4 | `collect` | 64 threads per workgroup, lists awake tiles and their neighbors as active |
| 5 | `clear` | 64 threads per workgroup, consumes awake flags |
| 6 | `collect_blocks` | 64 threads per workgroup, lists blocks of 2×2 tiles holding an active tile and flags the tiles their halos reach |
| 7 | `prepare` | One invocation, writes the active-tile and block indirect arguments |
| 8 | `coarsen` | One 16-thread workgroup per region tile, rebuilds flagged tiles' pressure cells |
| 9 | `step` | Per substep, indirect, one 104-thread workgroup per block; simulates blocks whose halo holds no gas and lists the others |
| 10 | `prepare_gas` | Per substep, one invocation, writes the gas-block indirect arguments |
| 11 | `step_gas` | Per substep, indirect, one 104-thread workgroup per listed gas block |
| 12 | `refine` | Indirect, one 256-thread workgroup per block, writes pressure cells back over active tiles' cells |
| 13 | `publish` | Indirect, one workgroup per active tile, writes gauge atmosphere pressure and wakes pushing neighbors |
| 14 | `damage` | Indirect, one workgroup per active tile, breaks exhausted solid cells and records topology changes |

Stages 9–11 repeat for each of the three substeps. Each `step` workgroup loads a 14×14 halo of pressure cells around its 8×8 pressure tile. Its first 64 invocations own the pressure tile's cells, and 40 more help load the halo and compute the flows and transfers of the rings around it. Each workgroup recomputes those rings itself from the previous substep, so blocks need no synchronization within a substep. `step` and `step_gas` share one substep function; `step` compiles it without species-slot work. Step pipelines skip workgroup memory zero-initialization.

The workgroup halo needs `max_compute_workgroup_storage_size = 32768`, which [accelerator setup](@/internals/runtime/accelerator.md) requests. The step keeps phantom air, flows, and open volume, solid words, and density per halo cell, and it derives the rest when read.

## Uniforms and bindings

`PressureUniforms` has 28 32-bit words (112 bytes): region width and height; ring offsets; ambient amount and mass; gravity and fluid raster scale; actor count; actor velocity scale; squared gas speed; per-substep flow damping, bulk viscosity, flow minimum, ambient diffusivity, substep count, actor drag, solid response, solid speed, solid relaxation, trapped expulsion, and damage rate; a volume-list flag; the diffusion scale; gas relaxation fraction and minimum; and two padding words. The pass writes two copies: one measured in cells, and one in pressure cells for `coarsen`, the steps, `refine`, and `publish`. The pressure cell copy scales amounts by the cell area, diffusion and viscosity terms by its inverse, and gravity and actor velocity by the inverse cell size.

Group 0 bindings 0–19 are: uniforms; cellular material identifiers; input gas state; output gas state; gas awake flags; open-volume and actor-drag records; pressure control and active-tile list; gas material records; fluid raster; fluid material records; actor records; static cellular material records; rigid-body tile records; rigid-body cell records; published pressure; dynamic-cell awake flags; kinematics; fluid raster and materials; actor records; static cellular materials; rigid-body records; published pressure; dynamic-cell awake flags; kinematics; gas thermal properties and matter; color. Group 1 binding 0 is the indirect dispatch record written by `prepare`, `prepare_volume`, and `prepare_gas`, at byte offsets 0 (active tiles), 12 (volume tiles), 24 (gas blocks), and 36 (blocks).

The input and output gas bindings select the direction of each stage:

- Volume and collection stages read the cellular gas.
- `coarsen` reads the cellular gas and writes the first pressure cell buffer.
- Substeps alternate between the two pressure cell buffers.
- `refine` reads the newest pressure cell buffer and writes the cellular gas.
- `publish` reads the newest pressure cell buffer and the refined stored pressure of static cells.
- `damage` reads and writes the cellular gas.

## Storage records

The cellular gas state is eight `u32` words per cell (32 bytes):

| Word | Contents |
| ---: | --- |
| 0–3 | Four species slots: 8-bit gas index in high bits and 24-bit amount |
| 4 | Phantom-air amount |
| 5 | Right-face flow as `f32` bits |
| 6 | Top-face flow as `f32` bits |
| 7 | Signed stored pressure in amount units for solid cells; solver state for other cell kinds |

The two pressure cell buffers use the same eight-word record, one per pressure cell, each a quarter of the cellular gas buffer. They are indexed by region position rather than ring position.

The control buffer holds four counts (active tiles, volume tiles, gas blocks, blocks), then 268 bytes per region tile:

- the active-tile list;
- the measured-tile list, which the block list replaces once volumes are measured;
- the gas-block list;
- four words per pressure cell that stay fixed through a tick's substeps: its volume word, packed drag, solid word, and density bits.

A pressure cell's volume word carries the dragged flag and `VOLUME_PARTIAL` for an open pressure cell holding static cells. Tile flags are `TILE_AWAKE` (1), `TILE_ACTIVE` (4), `TILE_BROKEN` (8), `TILE_VOLUME_DEPENDENT` (16), and `TILE_HALO` (32), which marks tiles `coarsen` rebuilds.

Open-volume records use two words per cell. The first stores volume at 65,536 units per whole volume and a high-bit dragged flag; the second stores packed right/top actor drag velocities. `pressure_buffer` stores published gauge pressure. Solid material records are four `f32` values (mass, absorption, strength, padding); gas records are three `f32` values (mass, diffusivity, cohesion) and packed color. See [material table compilation](@/internals/simulation/material-table.md).

## Shader constants

| Constant | Value | Meaning |
| --- | ---: | --- |
| `PRESSURE_IMPULSE` | 0.1111 | Consumer impulse per atmosphere and face, scaled by mass·cells/tick |
| `PRESSURE_PUSH_MINIMUM` | 0.02 atm | Minimum pressure magnitude for consumer pushes |
| `RELAXATION_TOLERANCE` | 0.01 atm | Gas pressure difference from ambient below which relaxation stops |
| `SLEEP_PHANTOM_CHANGE_MAXIMUM` | 64 | Largest phantom-air change per substep that lets a tile sleep |
| `SLEEP_FLOW_MINIMUM` | 16 | Smallest face flow that keeps a tile awake |
| `PRESSURE_CELLS_PER_TILE` | 4 | Pressure cells across a cellular tile |
| `AMOUNT_PER_ATMOSPHERE` | 4096 | Gas amount units per atmosphere |
| `AMOUNT_MAXIMUM` | 16,773,120 | Largest amount leaving room under the 4095-atmosphere pressure cap |
| `TRANSFER_SCALE` | 65,536 | Integer units per whole transfer fraction |
| `VOLUME_SCALE` | 65,536 | Integer units per whole open volume |
| `VOLUME_MINIMUM` | 0.25 | Minimum flow-evaluation volume |
| `FLUID_OCCUPANCY_CLOSED` | 0.5 | Smoothed fluid occupancy above which the open volume follows the unoccupied fraction |
| `FLUID_GAS_PERMEABILITY` | 0.5 | Minimum open volume retained by a fluid-occupied cell |
| `ADVECTION_SPEED_MAXIMUM` | 0.9 | Maximum flow advection speed per substep |
| `BULK_FRACTION_MAXIMUM` | 0.5 | Maximum total amount transferred per substep |

The CPU-side per-second coefficients and fixed 3-substep tick are listed on [pressure simulation](@/internals/simulation/pressure.md#flows-volume-and-sleeping).
