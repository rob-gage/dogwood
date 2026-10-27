+++
title = "Public API"
description = "The game-facing API of the engine facade, editor, and command-line tool."
weight = 3
sort_by = "weight"
template = "section.html"
[extra]
sources = ["engine/src/lib.rs", "engine_host/src/lib.rs", "engine_graphics/src/lib.rs", "template/Cargo.toml.template", "documentation/site.sh", "engine_graphics/src/world/materials/material_radiance.rs"]
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
- [Materials](@/external-api/materials.md): material definitions and tables.
- [Collisions and edits](@/external-api/collisions-and-edits.md): colliders and world edits.
- [Graphics](@/external-api/graphics.md): `SpriteSheet` and the `engine_graphics` renderer.
- [User interface](@/external-api/user-interface.md): in-game UI.
- [Editor crate](@/external-api/editor.md): `Editor::launch`.
- [CLI reference](@/external-api/cli.md): the `dogwood` command.

## Facade map

| Facade path | Contents | Guide |
| --- | --- | --- |
| `engine::games` | `Game`, `GameInstance` | [Engine and Game](@/external-api/engine-and-games.md) |
| `engine::runtime` | `Engine`, `EngineError` | [Engine and Game](@/external-api/engine-and-games.md) |
| `engine::actors` | World actor module and its selected exports | [Actors](@/external-api/actors.md), [Actor components](@/external-api/actor-components.md) |
| `engine::materials` | World material module and its selected exports | [Materials](@/external-api/materials.md) |
| `engine::scenes` | `Scene`, `SceneError`, `SceneGenerator`, `ScenePosition`, and scene submodules | [Scenes](@/external-api/scenes.md), [Coordinates, tiles, and chunks](@/external-api/world-storage.md), [Collisions and edits](@/external-api/collisions-and-edits.md) |
| `engine::system_events` | Keyboard, mouse, and window events; `KeyCode`, `PhysicalKey` | [Host types](@/external-api/host.md) |
| `engine::Accelerator`, `engine::AcceleratorError` | GPU context and initialization error | [Host types](@/external-api/host.md) |
| `engine::Processor`, `engine::ProcessorConfiguration`, `engine::ProcessorError` | Named CPU pools and configuration | [Host types](@/external-api/host.md) |
| `engine::SpriteSheet` | Sprite sheet type | [Graphics](@/external-api/graphics.md) |
| `engine::SceneRenderer`, `engine::SceneRendererViewMode`, `engine::SceneGraphicsOverlay`, `engine::SceneGraphicsOverlayShape`, `engine::SceneGraphicsTemperature` | Scene renderer, its view modes, and cell-aligned overlays | [Graphics](@/external-api/graphics.md) |
| `engine::MaterialGraphicsCellularDynamic`, `engine::MaterialGraphicsCellularStatic`, `engine::MaterialGraphicsGas` | Graphics properties for cellular, fluid, and gas materials | [Materials](@/external-api/materials.md) |
| `engine::user_interface` | Re-export of the `engine_user_interface` crate | [User interface](@/external-api/user-interface.md) |

### Not in the facade

- **Not re-exported:** `engine_host::windows` and `engine_graphics::renders`. Depend on those crates directly if you need them; see [Host types](@/external-api/host.md) for `engine_host::windows` and [Graphics](@/external-api/graphics.md) for `engine_graphics::renders`.
- **Graphics modules:** `engine_graphics` also has `sprites` and `world` modules beyond the facade's `SpriteSheet` and material graphics re-exports.
- **Scene submodules:** `engine::scenes` also exposes `cellular_tiles`, `chunks`, `collisions`, `edits`, and `kinematics`. Some of their public types are engine-internal.

## Rustdoc

Browse the {{ <rustdoc path="engine/index.html" text="public engine API" /> }}. The output includes workspace crates such as `engine_host`, `engine_world`, `engine_graphics`, and `engine_user_interface`; prefer the `engine` paths above when writing game code.

The site's references come from `./documentation/site.sh build`, which runs rustdoc twice: a public reference of the library crates (`cargo doc --workspace --exclude cli --no-deps`) and a private one of every crate, including the `dogwood` CLI (`--document-private-items`).

The `engine::scenes::fluids` module exports engine-managed `Fluids` and solver constants. `engine::scenes::chunks::ChunkFluidParticle` represents dormant particles. See [fluid simulation](@/internals/simulation/fluids.md). The `engine::scenes::pressure` module exports engine-managed `Pressure` and pressure constants; see [pressure simulation](@/internals/simulation/pressure.md).

- [Lighting](@/external-api/lighting.md): radiance sheets, scene controls, and actor lights.

The facade also re-exports `pack_radiance`; see [Lighting](@/external-api/lighting.md).
