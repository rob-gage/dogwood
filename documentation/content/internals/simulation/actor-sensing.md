+++
title = "Actor sensing"
description = "Environmental cells, gas tags, temperature, and pressure sampled around actors."
draft = false
[extra]
sources = ["engine_world/src/scenes/actor_sensing", "engine_world/src/scenes/scene.rs", "engine_world/src/scenes/cellular_tiles/cellular_tile_pass.rs", "engine_world/src/scenes/fluids/fluid_particle.wgsl", "engine_world/src/scenes/thermal/thermal_cellular_tile.wgsl", "engine_world/src/scenes/pressure/mod.rs"]
+++

`Scene` samples cells around actors after each active-region simulation step. Configure the sample with `SceneActorSensingConfiguration`; read the latest completed sample with `Scene::actor_sensing`. A missing result means no completed sample exists for that actor.

## Counts

The pass uses each actor's analytic circle, rectangle, or capsule collider and scans cell centers inside the active region.

- A cell touches the actor when its signed distance from the collider is at most `contact_margin` cells. This includes cells inside the collider.
- A cell belongs to the breathing band when its center is outside the collider and no farther than `breathing_margin` cells from it.
- `tag_cell_counts` counts touching cells for each configured tag, in configuration order. Cellular static, cellular dynamic, fluid, and gas material tags are eligible. A gas species contributes when its amount reaches `gas_tag_amount`, including in cells inside the collider. One cell contributes at most once to each matching tag, regardless of its material mix.
- `breathing_cell_count` counts every in-region cell in the breathing band. `breathable_cell_count` counts those without matter, rigid-body cells, or fluid coverage at or above 0.5, and whose total species-gas amount is no greater than `breathable_gas_amount`. Empty cells with ambient phantom air are breathable because phantom air is not included in the species sum.
- `maximum_temperature` and `maximum_pressure` are maxima over cells touching the collider or in the breathing band. Temperature is in kelvin from shared per-cell thermal data; pressure is the absolute value of the published gauge pressure in atmospheres. Invalid temperature or pressure values are skipped. The thermal step includes heat exchanged with rasterized fluid particles in the shared cell temperature.

The default configuration uses no tags, a 1.5-cell contact margin, a 2.5-cell breathing margin, a 0.5-atmosphere species-gas breathability limit, and a 0.05-atmosphere minimum gas amount for tag counts. Fluid coverage is raster mass divided by the selected fluid's rest density. Contact and drowning both use a 0.5 coverage threshold, representing at least one particle: each particle rasterizes its fluid's rest density into a cell.

## Tags and actor selection

At most eight tags can be configured. Each material's `u32` mask uses bit `i` for configured tag `i`. The mask array is laid out as cellular static indices, cellular dynamic indices, fluid indices, then gas indices. Static, dynamic, and fluid counting semantics are unchanged; gas species masks are ORed into the cell mask, with per-species thresholding, so each configured tag counts once per cell. Rigid-body cells do not suppress gas tags.

Actor records come from the cellular tile pass's actor buffer, which is limited to `CELLULAR_TILE_ACTOR_COUNT_MAXIMUM` (32). The pass samples the actor records also selected for fluid coupling. Actors without a record in that buffer are not sampled and have no result.

## Readback and limits

The pass writes a result each simulation step and stores up to three asynchronous readbacks. It polls without waiting for the GPU; if all slots are in flight, that step's result copy is skipped. A result is therefore at least one step old and can lag further while the GPU is busy. The readback replaces the stored map with the actors included in that completed sample.

> **Limitation:** Rigid-body cells count toward no cellular or fluid material tag and are never breathable. All returned values describe an earlier simulation step, not the current live scene state.

See [scene order](@/architecture/scene-pipeline.md#simulation-order), [synchronization](@/architecture/concurrency-and-gpu-sync.md#gpu-readbacks), and the [actor sensing shader](@/internals/shaders/actor-sensing-shader.md).
