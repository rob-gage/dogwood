+++
title = "Architecture"
description = "How Dogwood's crates and runtime objects cooperate across a frame, a fixed update, and a scene's lifetime."
weight = 5
sort_by = "weight"
template = "section.html"
[extra]
sources = ["Cargo.toml", "engine/Cargo.toml", "engine/src/runtime/engine.rs", "engine/src/games/game.rs", "engine/src/games/game_instance.rs", "engine_host/Cargo.toml", "engine_world/Cargo.toml", "engine_graphics/Cargo.toml", "engine_user_interface/Cargo.toml", "engine_audio/Cargo.toml", "editor/Cargo.toml", "cli/Cargo.toml"]
+++

This section explains how Dogwood's crates cooperate at runtime. Read it before changing code in `engine_world` or `engine_host`; game programmers can start with the [core concepts](@/getting-started/core-concepts.md) page instead.

## Crates

The `engine` crate is the game-facing facade. It re-exports scene, actor, material, and graphics-facing APIs and composes the other crates. The table lists direct internal workspace dependencies.

| Crate | Owns | Depends on |
| --- | --- | --- |
| `engine` | Facade, `Engine`, `GameInstance`, the `Game` trait | `engine_world`, `engine_graphics`, `engine_host`, `engine_user_interface` |
| `engine_world` | Actors, scenes, chunk storage, tile streaming, edits, kinematics, collisions, fluids, gases | `engine_graphics`, `engine_host` |
| `engine_graphics` | Drawing scene output | `engine_host` |
| `engine_host` | Native window, GPU accelerator, processor pools, system input events | `engine_user_interface` |
| `engine_user_interface` | egui integration and game UI | none |

`engine_audio` declares no dependencies; `editor` is a library crate and `cli` provides the `dogwood` binary. See [crate and module boundaries](@/internal-api/crates.md) for the full workspace layout.

```text
engine ──> engine_world
  ├──────> engine_graphics
  ├──────> engine_host
  └──────> engine_user_interface
engine_world ──> engine_graphics ──> engine_host ──> engine_user_interface
     └────────> engine_host

Engine ──creates──> GameInstance ──calls Game::scene() to access──> Scene
```

The arrows show direct workspace dependencies.

## Runtime objects

| Object | Role |
| --- | --- |
| `Engine` | Holds the `Accelerator` (GPU device and queue) and the `Processor` (CPU worker pools); launches a game. |
| `Game` | Trait implemented by the game. `Game::scene` supplies the scene and camera actor. |
| `GameInstance` | Wraps the game at launch; forwards updates, then advances the scene, and composes scene and UI during rendering. |
| `Window` | Host window that schedules fixed updates and redraws. |
| `Scene` | Owns the `ActorRegistry`, the material table, the world generator, and chunk storage. |

Streaming lazily creates chunk and [tile-ring state](@/architecture/glossary.md#world-terms). Simulation lazily creates kinematics, collision, and gas state once an [active tile region](@/architecture/glossary.md#world-terms) exists. See the [frame and update lifecycle](@/architecture/frame-lifecycle.md), the [scene update pipeline](@/architecture/scene-pipeline.md), and [resources and lifetimes](@/architecture/resources-and-lifetimes.md). Rustdoc: {{ <rustdoc path="engine/runtime/struct.Engine.html" text="Engine" /> }}.

## Reading order

1. [Frame and update lifecycle](@/architecture/frame-lifecycle.md) — host events, fixed updates, redraws, and presentation.
2. [Scene update pipeline](@/architecture/scene-pipeline.md) — the ordered streaming and simulation work in a scene.
3. [Data flow end to end](@/architecture/data-flow.md) — how world cells and input reach rendered output.
4. [Concurrency and GPU synchronization](@/architecture/concurrency-and-gpu-sync.md) — worker jobs, GPU submissions, and readbacks.
5. [Resources and lifetimes](@/architecture/resources-and-lifetimes.md) — what a scene owns, fixed limits, save and drop behavior.
6. [Error handling](@/architecture/errors.md) — error types and where the runtime turns errors into panics.
7. [Glossary and coordinate spaces](@/architecture/glossary.md) — reference for terms and coordinate conversions; consult it while reading the pages above.

Looking for a constant (chunk limits, collision groups)? See the capacity tables in [resources and lifetimes](@/architecture/resources-and-lifetimes.md#capacity-limits).
