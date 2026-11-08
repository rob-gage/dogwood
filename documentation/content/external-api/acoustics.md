+++
title = "Simulation acoustics"
description = "Bounded GPU activity buckets and existing CPU contacts for game-defined audio."
weight = 115
[extra]
sources = ["engine_world/src/scenes/acoustics.rs", "engine_world/src/scenes/acoustics.wgsl", "engine_world/src/scenes/scene.rs", "engine_world/src/scenes/mod.rs", "engine_world/src/scenes/collisions/collision.rs", "engine_world/src/scenes/actor_sensing/mod.rs", "engine_world/src/scenes/cellular_tiles/cellular_tile_buffers.rs"]
+++

`Scene::sample_acoustics(accelerator, center, half, tags)` returns completed `AcousticBucket` observations and submits the next GPU observation. `center` is a `ScenePosition`; `half` gives the observed half-width and half-height in tiles. Call at a low frequency such as eight times per second, after simulation has published its buffers. The engine does not play sounds or select game assets.

`tags: &[String; 4]` identifies, in order, burning gas, corrosive fluid, hot fluid, and corrodible solids. Keep the tag configuration fixed for a scene; it is compiled on the first call. Before scene tiles and fluids are resident the method returns an empty vector.

## Measurements

`AcousticBucket` has `position: ScenePosition` and `activity: [f32; 6]`:

| Index | Activity |
| --- | --- |
| 0 | Ordinary fluid momentum above the movement threshold |
| 1 | Amount of gas with the burning tag |
| 2 | Corrosive fluid contacting corrodible solid in the positive horizontal/vertical neighboring cells |
| 3 | Hot fluid momentum above the movement threshold |
| 4 | Absolute gauge pressure above the pressure threshold |
| 5 | Moving granular matter above the movement threshold |

These are coarse sums, not normalized volume controls or individual events. The game chooses thresholds, gains, cooldowns, source clustering, panning, and asset mappings. Burning gas and acid contact are activity proxies; exact reaction extent, steam generation, and thermal cracking are not reported.

## Reduction and readback

Observation is clipped to the active region and extends four tiles beyond the requested rectangle. Buckets start at 64 by 64 cells and grow to keep at most 16 by 16 buckets. Large rectangles use a coarser sampling stride targeting roughly 300,000 sampled cells; totals scale by stride area. One 64-thread workgroup reduces six floats per bucket in shared memory, without per-cell global atomics or any writes to simulation buffers.

At most 6,144 result bytes are copied through three asynchronous staging slots. Polling never waits, a busy ring skips its copy, and results retain their original positions when the camera moves. Mapping/poll errors are logged and the observation is skipped rather than failing scene simulation. The pass is lazy, scene-owned, and runs only when the game calls it; it adds no step to `Scene::simulate` itself. See [GPU synchronization](@/architecture/concurrency-and-gpu-sync.md).

> **Limitation:** Small features can be missed at coarse strides, and short activity can fall between observations. No occlusion, sound propagation, or GPU timestamp profiling is performed.

## CPU contacts and material hints

`Scene::acoustic_impacts()` reads existing Rapier contacts directly and returns at most sixteen strongest `(ScenePosition, normalized_impulse, MaterialIdentifier)` tuples, sorted strongest first. Only cellular rigid-body contacts with impulse divided by body mass of at least 10 are returned. Repeated calls may report the same contact; games apply cooldowns. The material is a representative occupied cell of the body, not necessarily its contact cell. No GPU readback is needed.

`Scene::rigid_body_material(body)` exposes that representative material. `Scene::stored_material_at(cell)` reads the resident saved/generated chunk snapshot without GPU transfer; active simulation edits can be newer. Use a live raycast first to confirm a surface exists, and treat the material as advisory.
