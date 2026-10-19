+++
title = "Frame and update lifecycle"
description = "How the host schedules fixed updates, scene work, redraws, and presentation."
weight = 10
[extra]
sources = ["engine/src", "engine_host/src/windows/window.rs", "engine_host/src/windows/window_configuration.rs", "engine_graphics/src/renders/scene_renderer.rs", "engine_user_interface/src/user_interface_context.rs", "engine_user_interface/src/user_interface_renderer.rs", "engine_user_interface/src/user_interface_window_state.rs"]
+++

This page follows one pass through the native host's loop: fixed updates, redraw scheduling, rendering, and presentation. It is for contributors who need to know when `Game`, scene, and renderer code runs.

The native host schedules fixed game updates independently from redraw requests. Updates use a 60 Hz interval. Rendering runs when the window delivers `RedrawRequested`, and redraw requests follow the configured FPS cap.

A frame can render without a fixed update, and a delayed loop iteration can run several updates before the next redraw. At most six fixed updates run in one scheduler callback. With a low FPS cap, updates can run in multiple callbacks between redraws (see [Fixed updates](#fixed-updates)).

```text
window events ──> Game callbacks (window / keyboard / mouse)
                         |
event-loop iteration ──> 0..6 fixed updates
                         | pawn controls
                         | Game::update
                         | Scene::stream
                         | Scene::simulate
                         v
redraw request ──> acquire surface ──> scene ──> UI ──> present
```

## Fixed updates

`Engine::launch` sets the update interval to `1_000_000_000 / 60` nanoseconds and the maximum accumulated duration to six intervals. Each callback receives the fixed interval as its elapsed time. See [event loop and timing](@/internals/runtime/event-loop.md) for the accumulator and the six-step cap.

`GameInstance::update` runs these steps in order:

1. Apply held-key movement, jump, and crouch values to possessed pawns.
2. Call `Game::update`.
3. Stream the selected scene around its camera.
4. Simulate that scene.

Streaming and simulation errors are converted to panics at this runtime boundary. A game without a scene receives its update callback but has no scene work.

### Pawn keys

Possessed pawns receive built-in movement, jump, and crouch controls; see [pawn input and key mappings](@/internals/actors/pawn-movement.md#input).

Focus loss clears the recorded held keys. The UI processes window events independently of `Game` callbacks.

## Redraw and presentation

### Scheduling

| Condition | Redraw scheduling |
| --- | --- |
| `fps_limit == u32::MAX` (default) | Redraw requested every loop iteration, with `Poll` control flow. |
| `fps_limit` below `u32::MAX` | Capped. The cap is at least 1 FPS; the interval is integer nanoseconds per frame, minimum 1 ns. |
| Zero-sized surface | Only the next fixed update is scheduled. |

With a nonzero-sized surface and a cap, the host requests a redraw at each render deadline. The next event-loop wakeup is the earlier of the next update and render deadline.

### Rendering

On `RedrawRequested`, `GameInstance::render` draws the scene (or clears to black if the game has no scene), then runs the game UI. `SceneRenderer` submits the scene command buffer; `GameInstance` then submits the UI callback buffers and UI command buffer. The host applies UI platform output and presents the acquired surface texture.

Timeouts and occluded surfaces skip a frame. Outdated or lost surfaces are reconfigured before retrying presentation. See [window surfaces](@/internals/runtime/surfaces.md) and [scene rendering](@/internals/rendering/scene-renderer.md).

For the scene-internal order of edit application, streaming, and simulation, see [scene update pipeline](@/architecture/scene-pipeline.md). The public lifecycle hooks are {{ <rustdoc path="engine/games/trait.Game.html" text="Game" /> }} and {{ <rustdoc path="engine/runtime/struct.Engine.html" text="Engine" /> }}.
