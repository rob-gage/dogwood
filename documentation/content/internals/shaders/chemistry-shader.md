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
| `prepare_reactions` | Pads the reacting-tile list to a multiple of four with tiles of no reactive blocks, writes indirect dispatch arguments, and resets the reacting count. |
| `react_tiles` | Runs a 64-invocation workgroup per four selected tiles, one invocation per owned 2×2 block, and writes mutations. |

Selection checks precompiled reaction flags and conservative partner-class masks before the reaction kernel reads full cell state. The pair lookup stores ordered reaction lists and their lengths; the selection flag words follow the unary headers. The legacy headers, flags, and 32-class masks retain their offsets; four additional `u32` mask words per thermal material follow the legacy lookup data, encoding 128 classes without changing reaction-list offsets. `MaterialTable` compiles these tables with the reaction records and product transformation vectors.

## Bindings and mutation

The main bind group contains uniforms, tile matter/gas/color/kinematics, awake flags, reacting tiles, compiled material and reaction tables, fluid heat/species/spawn data, pressure, and rigid-body collision buffers. The selection bind group reads the listed tiles and the minimum cell/reaction lookup state. A second bind group holds the reacting-tile list and indirect dispatch record.

Fluid species occupy an aligned four-word section appended to the fluid raster buffer: two packed material indices and two amounts per physical cell. The chemistry pass writes remaining amounts there. Fluid heat consumes the amounts to scale particle amount; the kill count is read back asynchronously and requests compaction. Fluid products use eight-word records in a spawn list with capacity 4096, which the next fluid simulation consumes.

Reaction mutations reuse thermal conversion helpers for cellular products, gas, color/radiance transforms, heat, and collision occupancy reporting. Before contact calls, the traversal can skip a unary row with no unary flag or a whole paired-contact group when current unit classes cannot match. Paired groups are rechecked at each boundary and rows are rechecked after every actual contact, so products created by earlier contacts retain their original traversal order and same-tick opportunities. Skipped calls have no side effects. Writes are gated by `TILE_LISTED`, ensuring thermal's apply step consumes the changes. `HEAT_CAPACITY_MINIMUM` is `0.01`; heat that cannot be assigned to cellular matter or gas is passed through the fluid heat raster where fluid is present.

The 96-byte uniform record includes fixed runtime traversal bounds: seven material units, four
internal faces, four owned cells, twelve loaded cells, two neighboring axes, three heat destinations,
and four gas slots. Runtime bounds keep driver compilation from expanding the reaction solver.
One ordered contact loop shares the solver across unary, same-cell, internal-face, and neighboring
catalyst checks. Internal faces retain order `(0, 1)`, `(2, 3)`, `(0, 2)`, `(1, 3)`.

Paired-contact traversal caches each loaded cell's paired-reactant unit mask in
invocation-private storage. Empty masks reject contact groups before rebuilding
partner classes, which include only units eligible for paired reactions. A reaction
that passes its deterministic event roll invalidates every cached mask before any
product or consumption write, including writes to other owned cells. Later contacts
therefore see new reactants in the original order. Failed rolls and range checks
leave the cache valid because they cannot change unit types.

`DOGWOOD_CHEMISTRY_NO_PAIR_CACHE=1` selects the uncached reference at pipeline
creation. GPU regressions compare stateful contact traces and complete solver
output with the cache enabled and disabled, including new paired products,
catalysts, ring offsets, block offsets, and material class aliases above 128.
