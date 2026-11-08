+++
title = "Particles"
description = "A pooled CPU particle system drawn through one sprite-only actor."
weight = 55
[extra]
sources = ["engine_world/src/particles", "engine/src/lib.rs", "engine/src/runtime/engine_error.rs"]
+++

`engine::particles` provides short-lived cosmetic particles. A `ParticleSystem` owns one persistent, sprite-only actor with an `ActorSpritesComponent` and rewrites its sprites in `update`. Particles are one-cell white pixels tinted per particle, so they stay aligned to the cell grid; with a nonzero `glow` they also emit radiance in their tint. See [actor components](@/external-api/actor-components.md) for the sprite component.

- `ParticleEffect`: a data-driven preset (counts, speed, spread, lifetime, gravity, drag, size, colors, fade, glow, depth, jitter, inherited velocity, stretch). Build presets with `..ParticleEffect::DEFAULT`; it is `const`-constructible.
- `ParticleSystem::new(scene, accelerator, capacity)`: spawns the actor. Fails with `ParticleError`, which converts into `EngineError`.
- `emit` and `emit_scaled`: burst at a `ScenePosition` with a direction, an emitter velocity, and (for `emit_scaled`) a multiplier on the count range.
- `update(scene, elapsed)`: simulates and publishes sprites; call once per game update. Cost is linear in live particles and the sprite buffer is reused.
- `ParticleEmitter`: rate-based emission that carries fractions of a particle between calls.
- `clear`, `len`, `set_seed` (deterministic splitmix64 generator), and `remove(scene)`, which despawns the actor.

The simulation lives in `ParticlePool`, which has no GPU or scene dependency. Its y axis points up; `gravity` pulls toward negative y. When the pool is full, new particles are dropped. Every particle draws in whole, unrotated cells snapped to the scene's cell grid, so particle pixels match terrain and actor cells: sizes round to whole cells (minimum one), and a nonzero `stretch` draws a streak of up to 12 single cells trailing back along the velocity, fading toward the tail.

```rust
use engine::particles::{ParticleEffect, ParticleEmitter, ParticleSystem};
use engine::scenes::{Scene, ScenePosition};
use engine::Accelerator;

const SPARKS: ParticleEffect = ParticleEffect {
    count: [8, 14],
    speed: [4.0, 9.0],
    spread: 0.6,
    lifetime: [0.2, 0.5],
    gravity: 12.0,
    drag: 1.0,
    colors: [[1.0, 0.8, 0.3, 1.0], [1.0, 0.4, 0.1, 1.0]],
    color_end: [1.0, 0.1, 0.0, 0.0],
    glow: 2.0,
    stretch: 0.15,
    ..ParticleEffect::DEFAULT
};

const SMOKE: ParticleEffect = ParticleEffect {
    speed: [0.5, 1.5],
    lifetime: [1.0, 2.0],
    gravity: -2.0,
    size: [2.0, 3.0],
    size_end: 2.0,
    colors: [[0.3, 0.3, 0.3, 0.5]; 2],
    color_end: [0.1, 0.1, 0.1, 0.0],
    fade_in: 0.2,
    ..ParticleEffect::DEFAULT
};

fn example(
    scene: &mut Scene,
    accelerator: &Accelerator,
    hit: ScenePosition,
) -> Result<(), engine::runtime::EngineError> {
    let mut particles = ParticleSystem::new(scene, accelerator, 2048)?;
    // a burst of sparks flying up from an impact
    particles.emit(&SPARKS, hit, [0.0, 1.0], [0.0, 0.0]);
    // continuous smoke at 20 particles per second
    let mut smoke = ParticleEmitter::new(20.0);
    smoke.emit(&mut particles, &SMOKE, hit, [0.0, 1.0], [0.0, 0.0], 1.0 / 60.0);
    particles.update(scene, 1.0 / 60.0);
    Ok(())
}
```

> **Not implemented:** Reaction-generated particle notifications are not exposed. Emission today is driven by explicit game events.
