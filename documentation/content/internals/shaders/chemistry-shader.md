+++
title = "Chemistry shader"
description = "Reaction selection, indirect dispatch, ownership, and shader buffers."
weight = 75
[extra]
sources = ["engine_world/src/scenes/chemistry", "engine_world/src/scenes/thermal/thermal_transition.wgsl", "engine_world/src/scenes/thermal/thermal_conversion.wgsl", "engine_world/src/scenes/thermal/thermal_cellular_tile_pass.rs", "engine_world/src/materials/material_table.rs", "engine_world/src/scenes/fluids/fluid_particle.wgsl"]
+++

`ChemistryCellularTilePass` embeds `chemistry_cellular_tile.wgsl` with the shared thermal transition and conversion helpers. Thermal prepares reaction uniforms, then records the chemistry stages in its compute pass before its own `step` stage. See [chemistry simulation](@/internals/simulation/chemistry.md) for behavior and limits.

## Dispatch stages

| Entry point | Work |
|---|---|
| `select_tiles` | Reads thermal-listed tiles and checks a 10×10 cell workgroup region for unary or paired reaction flags. Candidate tiles are appended to the reacting-tile list. |
| `prepare_reactions` | Resets the reacting count and writes indirect dispatch arguments. |
| `react_tiles` | Runs one workgroup per selected tile, processes owned 2×2 blocks and writes mutations. |

Selection checks precompiled reaction flags and pair lookup entries before the reaction kernel reads full cell state. The pair lookup stores sorted reaction lists and their lengths; the selection flag words follow the unary headers. `MaterialTable` compiles these tables with the reaction records and product transformation vectors.

## Bindings and mutation

The main bind group contains uniforms, tile matter/gas/color/kinematics, awake flags, reacting tiles, compiled material and reaction tables, fluid heat/species/spawn data, pressure, and rigid-body collision buffers. The selection bind group reads the listed tiles and the minimum cell/reaction lookup state. A second bind group holds the reacting-tile list and indirect dispatch record.

Fluid species occupy an aligned four-word section appended to the fluid raster buffer: two packed material indices and two amounts per physical cell. The chemistry pass writes remaining amounts there. Fluid heat consumes the amounts to scale particle amount; the kill count is read back asynchronously and requests compaction. Fluid products use eight-word records in a spawn list with capacity 4096, which the next fluid simulation consumes.

Reaction mutations reuse thermal conversion helpers for cellular products, gas, color/radiance transforms, heat, and collision occupancy reporting. Writes are gated by `TILE_LISTED`, ensuring thermal's apply step consumes the changes. `HEAT_CAPACITY_MINIMUM` is `0.01`; heat that cannot be assigned to cellular matter or gas is passed through the fluid heat raster where fluid is present.
