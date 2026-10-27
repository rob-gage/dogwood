+++
title = "Thermal shader"
description = "Thermal compute stages, bindings, and heat records."
weight = 70
[extra]
sources = ["engine_world/src/scenes/thermal", "engine_world/src/scenes/cellular_tiles/cellular_tile_buffers.rs"]
+++

`ThermalCellularTilePass` runs the thermal compute pipeline over awake tiles. Its bindings include scene uniforms, cellular material and matter buffers, tile awake flags, the compiled thermal material table, gas and fluid heat state, and thermal work records. Thermal material records retain transition properties, but transitions are not simulated.

Matter records are two `u32` words per cell: temperature bits followed by amount. Thermal work updates these records and manages thermal awake flags. Collision-owned changed-cell records are copied after thermal and fluid particle heat are applied, then collision consumes the changes on the following tick.
