+++
title = "Configuring the engine"
description = "Build an engine with custom worker pools, set the window frame rate, and set scene gravity."
weight = 70
[extra]
sources = [
    "engine_host/src/processors",
    "engine_host/src/windows/window.rs",
    "engine_host/src/windows/window_configuration.rs",
    "engine/src/runtime/engine.rs",
    "engine/src/games/game.rs",
    "engine/src/runtime/mod.rs",
    "engine/src/runtime/engine_error.rs",
    "engine/src/lib.rs",
    "engine_host/src/lib.rs",
    "engine_host/src/windows/mod.rs",
    "engine_world/src/scenes/mod.rs",
    "engine_world/src/scenes/scene.rs",
    "engine_graphics/src/renders/scene_renderer_lighting_pass.rs",
    "engine_graphics/src/renders/scene_renderer_lighting.wgsl",
    "engine_graphics/src/renders/scene_renderer_lighting_cascades.wgsl",
    "engine_graphics/src/renders/scene_renderer_lighting_mips.wgsl",
    "engine_graphics/src/renders/scene_renderer_lighting_optical.wgsl",
    "engine_graphics/src/renders/scene_renderer_lighting_spread.wgsl",
    "engine_graphics/src/renders/scene_renderer_lighting_sprites.wgsl",
    "engine_graphics/src/world/scenes/scene_graphics_lighting.rs",
    "engine_world/src/materials/material_temperature_color.rs",
]
+++

An `Engine` bundles the GPU `Accelerator` and the worker-thread `Processor`. Use `Engine::default()` for the default accelerator and processor. This page covers the settings you can change: worker pools, window frame rate, scene gravity, ambient gas, and scene lighting.

## Build an engine

`Engine::default()` creates the default graphics accelerator and processor. It panics if either resource cannot be initialized. Use `Engine::new(accelerator, processor)` when supplying configured resources; it takes ownership of both.

```rust
use engine::{
    runtime::{Engine, EngineError},
    Accelerator, Processor, ProcessorConfiguration,
};

fn make_engine() -> Result<Engine, EngineError> {
    let mut configuration = ProcessorConfiguration::default();
    configuration.add_cpu_bound_pool("simulation", 3);
    configuration.add_dedicated_worker_pool("chunk_io", 2);
    let processor = Processor::new(configuration)?;
    Ok(Engine::new(Accelerator::default(), processor))
}
```

Pass the engine to the runtime as shown in the [template walkthrough](@/getting-started/template-walkthrough.md).

For pool configuration and defaults, see [Host types](@/external-api/host.md) and [CPU worker pools](@/internals/runtime/processor-pools.md).

### Errors and required pools

- Invalid capacities, duplicate or empty names, zero weights or worker counts, and CPU-bound allocations beyond available workers return `ProcessorError`. Pool creation can also return `ProcessorError::RayonThreadPoolBuild`.
- On the first call to `Scene::stream`, Dogwood looks up the `chunk_io` pool and returns `SceneError::ChunkThreadPoolUnavailable` if it is missing.

## Window frame rate

Add `engine_host` to `Cargo.toml` to name `WindowConfiguration`; it lives in `engine_host::windows` and is not re-exported by `engine`.

`WindowConfiguration` currently has one field, `fps_limit`, the maximum render frames per second. Set it through the `Game::window_configuration` callback.

- The default is uncapped (`u32::MAX`).
- Finite limits are clamped to at least 1 FPS, so `0` means 1 FPS.

```rust
use engine::games::Game;
use engine_host::windows::WindowConfiguration;

struct MyGame;

impl Game for MyGame {
    fn window_configuration(&self) -> WindowConfiguration {
        WindowConfiguration { fps_limit: 120 }
    }
}
```

See [host types](@/external-api/host.md) for the host API.

## Scene gravity

Scenes start with downward gravity of `50.0` tiles per second squared. Call `Scene::set_gravity` to replace it; the value is used by dynamic bodies, pawns, and gas buoyancy. See [scenes](@/external-api/scenes.md) for scene setup and [engine and Game](@/external-api/engine-and-games.md) for runtime behavior.

## Ambient gas

Empty cells hold phantom air at `1.0` atmosphere with a mass of `29.0` per atmosphere. Call `Scene::set_ambient_gas_pressure` and `Scene::set_ambient_gas_mass` to replace them; gases lighter than the ambient mass rise and heavier ones sink, and a pressure of zero makes empty cells a vacuum. See [scenes](@/external-api/scenes.md) for validation.

## Scene lighting

`Scene` exposes runtime setters for ambient light and light penetration. The template uses daylight ambient `[0.75, 0.78, 0.85]`; see [lighting](@/external-api/lighting.md) for defaults, validation, and radiance behavior.

{{ <rustdoc path="engine/runtime/struct.Engine.html" text="Engine" /> }}
