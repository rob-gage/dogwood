+++
title = "Thermal shader"
description = "Thermal compute stages, bindings, and conversion records."
weight = 70
[extra]
sources = ["engine_world/src/scenes/thermal", "engine_world/src/scenes/cellular_tiles/cellular_tile_buffers.rs"]
+++

`ThermalCellularTilePass` runs the thermal compute pipeline over awake tiles. Its bindings include scene uniforms, cellular material and matter buffers, tile awake flags, the compiled thermal material table, gas and fluid state, and bounded conversion and changed-cell outputs. `thermal_transition.wgsl` supplies shared heat-capacity, transition, latent-heat, and color-transform helpers; the same helper is included by fluid conversion stages.

Matter records are two `u32` words per cell: temperature bits followed by amount. Thermal work updates these records, manages thermal awake flags, and emits fluid-conversion entries. Fluid compute consumes those entries before particle compaction. Collision-owned changed-cell records are copied after thermal and fluid particle heat are applied, then collision consumes the changes on the following tick.
