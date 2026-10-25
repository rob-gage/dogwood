+++
title = "Runtime internals"
description = "How Engine connects host callbacks to a mutable GameInstance."
weight = 20
[extra]
sources = ["engine/src/runtime", "engine/src/games", "engine_user_interface/src/user_interface_renderer.rs"]
+++

This page explains how `Engine` bridges host callbacks to a `GameInstance`, and what each update and render step does. It is for contributors working on the runtime; for the public contracts see [Engine and Game](@/external-api/engine-and-games.md).

## Launch wiring

`Engine` owns an `Accelerator` and a `Processor` (see [Host internals](@/internal-api/host.md)). `Engine::launch` performs these steps:

- Asks the `Game` for its `WindowConfiguration`.
- Creates a `RefCell<GameInstance<G>>`, where `Game` and `GameInstance` are described in [Engine and Game](@/external-api/engine-and-games.md).
- Clones the instance's `UserInterfaceContext`.
- Builds mutable callback closures that borrow the instance for each event, update, or render.
- Passes those callbacks and the shared resources to `Window::open`.

This bridge lets the host event loop operate on one game instance without owning the game. The full timeline is in [Frame and update lifecycle](@/architecture/frame-lifecycle.md).

The runtime sets updates to 60 per second and caps accumulated time at six update intervals. The host callback receives one interval as `elapsed_time` for each fixed update, and `GameInstance::update` calls `Game::update` within that step.

## GameInstance state and update

`GameInstance<G>` stores:

- the game,
- the held physical key codes,
- an optional camera override,
- a scene renderer with a selected `SceneRendererViewMode`;
- host overlays appended to scene overlays during rendering,
- the game's UI context, and
- an optional UI renderer.

The UI renderer is created on the first rendered frame using that frame's texture format and retained. Later render targets must use the same format.

The editor depends on `game_mutable`, `set_camera_actor`, `set_view_mode`, and `set_overlays` (see [Editor internals](@/internal-api/editor.md)).

### Update steps

`GameInstance::update` runs in this order:

1. Queries `Game::scene()` and, if it returns a scene, inserts held-key controls on actors with identity and possession components. A/Left moves left, D/Right moves right, W/Up jumps, and S/Down crouches.
2. Calls `Game::update(elapsed_time)`.
3. Queries `Game::scene()` again. This can return a different scene; if it returns one, streams and simulates it.

Streaming and simulation use `camera_actor.unwrap_or(game_camera_actor)`, so a host can temporarily substitute the camera returned by the game. Pawn-control insertion, streaming, and simulation errors panic.

### Stream-only step

`GameInstance::stream` performs only the scene streaming step, using the same camera selection. Streaming errors in `GameInstance::stream` panic.

### Edge cases

- If `Game::scene()` returns `None` after `Game::update`, `GameInstance::update` skips streaming and simulation but still calls `Game::update`.
- `GameInstance::stream` skips streaming when its scene query returns `None`.

## Input and rendering

Keyboard events add or remove a key only when the physical key is `PhysicalKey::Code`; the event is then forwarded to the game. `FocusReleased` clears the held-key set before forwarding the window event. Mouse events are forwarded unchanged.

Rendering has these steps:

1. Draws the selected scene into the supplied texture view, or clears it to black if the game has no scene. Scene graphics use the same camera selection as update; lookup errors panic.
2. Runs the game's UI composition through `UserInterfaceContext`.
3. Creates or reuses `UserInterfaceRenderer` for the texture format.
4. Submits the callback command buffers, followed by the command buffer returned by `command_encoder.finish()`.

The returned `egui::PlatformOutput` goes back to the host so it can apply cursor, clipboard, and text-input effects. Lower-level details live in [Host internals](@/internal-api/host.md), [Graphics internals](@/internal-api/graphics.md), and [UI internals](@/internal-api/user-interface.md).
