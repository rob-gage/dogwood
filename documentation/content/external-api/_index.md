+++
title = "Public API"
description = "The game-facing API of the engine facade, editor, and command-line tool."
weight = 3
sort_by = "weight"
template = "section.html"
[extra]
sources = ["engine/src/lib.rs", "engine/src/games", "engine_world/src/materials/mod.rs", "engine_world/src/particles/mod.rs", "engine_world/src/scenes/mod.rs", "engine_world/src/scenes/actor_sensing/mod.rs", "engine_world/src/scenes/soft_bodies/mod.rs", "engine_world/src/scenes/chunks/mod.rs", "engine_world/src/scenes/chunks/chunk_decoration.rs", "engine_world/src/scenes/chunks/chunk_background.rs", "engine_world/src/scenes/scene.rs", "engine_host/src/lib.rs", "engine_graphics/src/lib.rs", "template/Cargo.toml.template", "documentation/site.sh", "engine_graphics/src/world/materials/material_radiance.rs", "engine_audio/src/lib.rs"]
+++

The `engine` crate is the game-facing facade, and this section documents it together with the `editor` crate and the `dogwood` command-line tool. It is for game programmers who write a `Game` and need exact type, method, and module names. New readers should start with [Engine and Game](@/external-api/engine-and-games.md).

The facade re-exports selected types from the host, graphics, world, and user-interface crates, and defines its own `games` and `runtime` modules. The template depends on `engine`; its optional `editor` feature also depends on the separate `editor` crate.

## Pages in this section

- [Engine and Game](@/external-api/engine-and-games.md): implement `Game` and launch the `Engine`.
- [Host types](@/external-api/host.md): GPU, CPU pools, and input events.
- [Scenes](@/external-api/scenes.md): `Scene`, generators, and positions.
- [Coordinates, tiles, and chunks](@/external-api/world-storage.md): coordinate types and chunk storage.
- [Actors](@/external-api/actors.md): actor handles and the registry.
- [Actor components](@/external-api/actor-components.md): every component.
- [Particles](@/external-api/particles.md): pooled CPU particle effects.
- [Materials](@/external-api/materials.md): material definitions and tables.
- [Collisions and edits](@/external-api/collisions-and-edits.md): colliders and world edits.
- [Graphics](@/external-api/graphics.md): `SpriteSheet` and the `engine_graphics` renderer.
- [User interface](@/external-api/user-interface.md): in-game UI.
- [Audio](@/external-api/audio.md): stereo playback, music/SFX buses, panning, and persistent voices.
- [Simulation acoustics](@/external-api/acoustics.md): coarse GPU activity buckets, CPU contacts, and advisory material hints.
- [Editor crate](@/external-api/editor.md): `Editor::launch`.
- [Resources and distributions](@/external-api/resources-build.md): catalogue, resource readers, packaging and native builds.
- [CLI reference](@/external-api/cli.md): the `dogwood` command.

## Facade map

| Facade path | Contents | Guide |
| --- | --- | --- |
| `engine::games` | `Game`, `GameEventQueue`, `GameInstance` | [Engine and Game](@/external-api/engine-and-games.md) |
| `engine::runtime` | `Engine`, `EngineError` | [Engine and Game](@/external-api/engine-and-games.md) |
| `engine::actors` | World actor module and its selected exports | [Actors](@/external-api/actors.md), [Actor components](@/external-api/actor-components.md) |
| `engine::particles` | `ParticleEffect`, `ParticleSystem`, `ParticleEmitter`, `ParticlePool`, `ParticleError` | [Particles](@/external-api/particles.md) |
| `engine::materials` | World material module and its selected exports, including chemical material properties and reaction types | [Materials](@/external-api/materials.md) |
| `engine::scenes` | `Scene`, `SceneError`, `SceneGenerator`, `ScenePosition`, `SceneRayHit`, `SceneRayTarget`, `SceneRigidBody`, `ScenePawnTether`, `ScenePawnTetherBody`, `SceneActorSensing`, `SceneActorSensingConfiguration`, `SCENE_ACTOR_SENSING_TAG_COUNT_MAXIMUM`, and scene submodules including `soft_bodies` | [Scenes](@/external-api/scenes.md), [Coordinates, tiles, and chunks](@/external-api/world-storage.md), [Collisions and edits](@/external-api/collisions-and-edits.md), [Soft bodies](@/internals/simulation/soft-bodies.md) |
| `engine::system_events` | Keyboard, mouse, and window events; `KeyCode`, `PhysicalKey` | [Host types](@/external-api/host.md) |
| `engine::Accelerator`, `engine::AcceleratorError` | GPU context and initialization error | [Host types](@/external-api/host.md) |
| `engine::Processor`, `engine::ProcessorConfiguration`, `engine::ProcessorError` | Named CPU pools and configuration | [Host types](@/external-api/host.md) |
| `engine::SpriteSheet` | Sprite sheet type | [Graphics](@/external-api/graphics.md) |
| `engine::SceneRenderer`, `engine::SceneRendererViewMode`, `engine::SceneGraphicsShaded`, `engine::SceneGraphicsOverlay`, `engine::SceneGraphicsOverlayShape`, `engine::SceneGraphicsTemperature` | Scene renderer, its view modes, and cell-aligned overlays | [Graphics](@/external-api/graphics.md) |
| `engine::MaterialGraphicsCellularDynamic`, `engine::MaterialGraphicsCellularStatic`, `engine::MaterialGraphicsFluid`, `engine::MaterialGraphicsGas` | Graphics properties for cellular, fluid, and gas materials | [Materials](@/external-api/materials.md) |
| `engine::resources` | Re-export of the `engine_resources` crate | [Resources and distributions](@/external-api/resources-build.md) |
| `engine::audio` | Re-export of the `engine_audio` crate | [Audio](@/external-api/audio.md) |
| `engine::user_interface` | Re-export of the `engine_user_interface` crate | [User interface](@/external-api/user-interface.md) |

### Not in the facade

- **Not re-exported:** `engine_host::windows` and `engine_graphics::renders`. Depend on those crates directly if you need them; see [Host types](@/external-api/host.md) for `engine_host::windows` and [Graphics](@/external-api/graphics.md) for `engine_graphics::renders`.
- **Graphics modules:** `engine_graphics` also has `sprites` and `world` modules beyond the facade's `SpriteSheet` and material graphics re-exports.
- **Scene submodules:** `engine::scenes` also exposes `cellular_tiles`, `chunks`, `collisions`, `edits`, `kinematics`, and `soft_bodies`. Some of their public types are engine-internal.

## Rustdoc

Browse the {{ <rustdoc path="engine/index.html" text="public engine API" /> }}. The output includes workspace crates such as `engine_host`, `engine_world`, `engine_graphics`, and `engine_user_interface`; prefer the `engine` paths above when writing game code.

The site's references come from `./documentation/site.sh build`, which runs rustdoc twice: a public reference of the library crates (`cargo doc --workspace --exclude cli --no-deps`) and a private one of every crate, including the `dogwood` CLI (`--document-private-items`).

The `engine::scenes::fluids` module exports engine-managed `Fluids` and solver constants. `engine::scenes::chunks::ChunkFluidParticle` represents dormant particles. See [fluid simulation](@/internals/simulation/fluids.md). The `engine::scenes::pressure` module exports engine-managed `Pressure` and pressure constants; see [pressure simulation](@/internals/simulation/pressure.md).

- [Lighting](@/external-api/lighting.md): radiance sheets, scene controls, and actor lights.

The facade also re-exports `pack_radiance`; see [Lighting](@/external-api/lighting.md).
