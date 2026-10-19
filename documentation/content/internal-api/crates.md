+++
title = "Crates and modules"
description = "Workspace crate roles, module boundaries, and dependency relationships."
weight = 10
[extra]
sources = ["Cargo.toml", "Cargo.lock", "engine/src/lib.rs", "engine_host/src/lib.rs", "engine_world/src/lib.rs", "engine_graphics/src/lib.rs", "engine_graphics/src/world/mod.rs", "engine_user_interface/src/lib.rs", "editor/src/lib.rs", "engine/Cargo.toml", "engine_host/Cargo.toml", "engine_graphics/Cargo.toml", "engine_world/Cargo.toml", "engine_user_interface/Cargo.toml", "engine_audio", "editor/Cargo.toml", "cli/Cargo.toml", "cli/src/main.rs", "cli/src/commands", "cli/src/template", "cli/src/utility", "template/Cargo.toml"]
+++

This page maps the Dogwood workspace for engine contributors: what each crate is responsible for, how the crates depend on each other, and which items they re-export. The workspace manifest lists eight members.

## Internal dependencies

Each line lists the crates that the first crate depends on by path. `engine_user_interface`, `cli`, and `engine_audio` have no internal dependencies.

```text
engine          -> engine_world, engine_graphics, engine_host, engine_user_interface
engine_world    -> engine_graphics, engine_host
engine_graphics -> engine_host
engine_host     -> engine_user_interface
editor          -> engine, engine_host
```

## Workspace map

| Crate | Responsibility and module tree | Key external crates |
|---|---|---|
| `engine` | Game-facing facade; `games`, `runtime`. Re-exports selected host, world, and graphics items. | `wgpu` |
| `engine_host` | Platform services; `acceleration`, `processors`, `system_events`, `windows`. | `pollster`, `rayon`, `winit`, `wgpu` (per platform) |
| `engine_world` | World state and simulation; `actors`, `materials`, `scenes`. | `bevy_ecs`, `rayon`, `rapier2d`, `wgpu` |
| `engine_graphics` | Rendering, sprites, and graphics-side world types; `renders`, `sprites`, `world::{materials, scenes}`. | `image`, `wgpu` |
| `engine_user_interface` | Immediate-mode UI, widgets, game UI, and rendering integration; `advanced`, `components`, `game_ui`, plus context, renderer, theme, and window-state modules. | `taffy`, `egui`, `egui-winit`, `egui-wgpu`, `egui_extras`, `egui_dock`, `winit`, `wgpu` |
| `editor` | Editor application and its tools, tabs, brushes, and viewport. Its modules are private; `Editor` is re-exported. | None |
| `cli` | `dogwood` binary; commands, template handling, and utility modules. | `clap`, `home`, `include_dir`, `semver`, `serde_json`, `which` |
| `engine_audio` | Empty crate stub: its library file contains only the copyright comment. | None |

Engine crates pin external versions with `=version`; `cli` and `engine_host`'s `winit` use ranges. `wgpu` is enabled with platform-specific features in `engine_host` and `engine_user_interface`. Resolved versions in `Cargo.lock`:

| Group | Crates and versions |
|---|---|
| GPU and windowing | `wgpu` 30.0.0, `winit` 0.30.13, `pollster` 0.4.0 |
| Simulation | `bevy_ecs` 0.20.0, `rapier2d` 0.35.3, `rayon` 1.12.0 |
| Graphics and UI | `image` 0.25.10, `taffy` 0.14.0, `egui`, `egui-winit`, `egui-wgpu`, and `egui_extras` 0.36.2; `egui_dock` 0.21.1 |
| CLI | `clap` 4.6.7, `home` 0.5.12, `include_dir` 0.7.4, `semver` 1.0.28, `serde_json` 1.0.151, `which` 8.0.6 |

## Visibility and re-exports

Implementation crates commonly declare modules privately and selectively `pub use` types from their module root.

- `engine_world::scenes` has public `chunks`, `cellular_tiles`, `collisions`, `edits`, and `kinematics` namespaces. Within them, submodules are private and chosen types are re-exported.
- `actors` exposes the public `components` namespace.
- `materials` re-exports its types and keeps its implementation modules private.

The `engine` crate exposes `games` and `runtime`, re-exports selected graphics, host, and world items at the crate root, and exposes the UI crate as `engine::user_interface`. It does not mirror every implementation namespace; host windows and graphics renders are not root re-exports.

For the game-facing map, see [Public API](@/external-api/_index.md). For implementation entry points, see [Runtime internals](@/internal-api/runtime.md), [Host internals](@/internal-api/host.md), and [World internals](@/internal-api/world.md).

## Project template

`template/` is a project template rather than a workspace member. The CLI renders its manifest with dependency values substituted.

- `engine` and optional `editor` dependencies are injected through `{% raw %}{{engine_dependency}}{% endraw %}` and `{% raw %}{{editor_dependency}}{% endraw %}`.
- Enabling the template's `editor` feature activates `dep:editor`.
- The package name and `default-run` value contain `{% raw %}{{project_name}}{% endraw %}`; the explicit editor binary is named `editor`.
