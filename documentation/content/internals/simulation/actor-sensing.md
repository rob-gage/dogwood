+++
title = "Actor sensing"
description = "Cell and breathable-gas counts sampled around cellular tile actors."
draft = false
[extra]
sources = ["engine_world/src/scenes/actor_sensing", "engine_world/src/scenes/scene.rs", "engine_world/src/scenes/cellular_tiles/cellular_tile_pass.rs", "engine_world/src/scenes/fluids/fluid_particle.wgsl", "engine_world/src/scenes/pressure/mod.rs"]
+++

`Scene` samples cells around actors after each active-region simulation step. Configure the sample with `SceneActorSensingConfiguration`; read the latest completed sample with `Scene::actor_sensing`. A missing result means no completed sample exists for that actor.

## Counts

The pass uses each actor's analytic circle, rectangle, or capsule collider and scans cell centers inside the active region.

- A cell touches the actor when its signed distance from the collider is at most `contact_margin` cells. This includes cells inside the collider.
- A cell belongs to the breathing band when its center is outside the collider and no farther than `breathing_margin` cells from it.
- `tag_cell_counts` counts touching cells for each configured tag, in configuration order. Cellular static, cellular dynamic, and fluid material tags are eligible. One cell contributes at most once to each matching tag.
- `breathing_cell_count` counts every in-region cell in the breathing band. `breathable_cell_count` counts those without matter, rigid-body cells, or fluid coverage at or above 0.5, and whose total species-gas amount is no greater than `breathable_gas_amount`. Empty cells with ambient phantom air are breathable because phantom air is not included in the species sum.

The default configuration uses no tags, a 1.5-cell contact margin, a 2.5-cell breathing margin, and a 0.5-atmosphere species-gas limit. Fluid coverage is raster mass divided by the selected fluid's rest density. Contact and drowning both use a 0.5 coverage threshold, representing at least one particle: each particle rasterizes its fluid's rest density into a cell.

## Tags and actor selection

At most eight tags can be configured. Each material's `u32` mask uses bit `i` for configured tag `i`. The mask array is laid out as cellular static indices, then cellular dynamic indices, then fluid indices; gas materials have no tag-mask entries. Rigid-body cells are excluded from tag counts.

Actor records come from the cellular tile pass's actor buffer, which is limited to `CELLULAR_TILE_ACTOR_COUNT_MAXIMUM` (32). The pass samples the actor records also selected for fluid coupling. Actors without a record in that buffer are not sampled and have no result.

## Readback and limits

The pass writes a result each simulation step and stores up to three asynchronous readbacks. It polls without waiting for the GPU; if all slots are in flight, that step's result copy is skipped. A result is therefore at least one step old and can lag further while the GPU is busy. The readback replaces the stored map with the actors included in that completed sample.

> **Limitation:** Rigid-body cells count toward no material tag and are never breathable. Gas species are not counted toward material tags. All returned values describe an earlier simulation step, not the current live scene state.

See [scene order](@/architecture/scene-pipeline.md#simulation-order), [synchronization](@/architecture/concurrency-and-gpu-sync.md#gpu-readbacks), and the [actor sensing shader](@/internals/shaders/actor-sensing-shader.md).
