+++
title = "Event loop and timing"
description = "How the Winit host dispatches input, advances fixed updates, and schedules redraws."
weight = 10
[extra]
sources = ["engine_host/src/windows/window.rs", "engine/src/runtime/engine.rs", "engine/src/games/game_instance.rs", "engine_host/src/windows/window_configuration.rs", "engine_host/src/system_events", "engine_user_interface/src/user_interface_context.rs", "engine_user_interface/src/user_interface_window_state.rs"]
+++

This page explains how the host's Winit event loop dispatches window events to the game and how it schedules fixed updates and redraws. It is for engine contributors; `Engine::launch` supplies the callbacks, and [frame and update lifecycle](@/architecture/frame-lifecycle.md) shows how game and scene work fits inside each one.

`Engine::launch` passes the game instance's callbacks, the game's window configuration, and the fixed-step timing values to `Window::open`. The host runs Winit's `ApplicationHandler` on the calling thread. The runtime sets updates to 60 Hz and caps retained elapsed time at six update intervals.

## Native event dispatch

For events belonging to its window, the host first gives each event to `UserInterfaceWindowState::process_window_event`. Its response is ignored, so egui consumption does not prevent recognized events from reaching the game. A source comment in `Window` notes that egui repaint requests are ignored because the host already schedules redraws.

| Winit event | Dogwood callback or host action |
| --- | --- |
| `CloseRequested` | `SystemWindowEvent::CloseRequested`, then exit the event loop |
| `Resized` | `SystemWindowEvent::Resized { width, height }`, after updating the surface configuration, or suspending it for a zero-sized window |
| `Focused(true/false)` | `FocusAcquired` / `FocusReleased` |
| `KeyboardInput` | `SystemKeyboardEvent` with physical key, pressed state, and repeat flag |
| `MouseInput` | `SystemMouseEvent::Button` |
| `CursorMoved`, `CursorEntered`, `CursorLeft` | Corresponding cursor event |
| `MouseWheel` | `WheelLines` or `WheelPixels`, according to Winit's delta variant |
| `RedrawRequested` | On a nonminimized redraw, attempt presentation; acquisition may skip the frame |

Other Winit window events have no game callback mapping, although the UI still receives them. See [host input and window event types](@/external-api/host.md) for the event payloads and units.

### Focus and held keys

The game instance clears its held-key set on focus loss before forwarding the focus event. Keyboard tracking uses physical `KeyCode`s.

### Resume and suspend

On a resume with no native window yet, the host resets `last_update_time` and `next_render_time`, clears the update accumulator, and requests an initial redraw once the window is created.

See [windows and surfaces](@/internals/runtime/surfaces.md#resize-and-suspension) for resume and suspend handling.

### Edge cases

- A resize failure exits the event loop before the resize callback is dispatched.

## Fixed updates and redraw cadence

The host does its scheduling in `about_to_wait`. Each iteration runs these steps:

1. Measure the time elapsed since the previous scheduler iteration.
2. Add it to the update accumulator with saturating arithmetic, then clamp the accumulator to `maximum_accumulated_time`.
3. While the accumulator is at least `update_interval`, call the update handler with exactly that interval.
4. Subtract one interval after each call.

With the runtime's normal settings, a delayed iteration therefore runs at most six updates; the clamp discards excess elapsed time.

When a window exists but has no surface configuration, the host schedules a wakeup for the next update and skips redraw requests.

### Frame-rate limit

`WindowConfiguration::default()` sets `fps_limit` to `u32::MAX` (unlimited). The two cases behave as follows:

| `fps_limit` | Behaviour |
| --- | --- |
| Finite (below `u32::MAX`) | Clamp the limit to at least 1. When the render deadline arrives, request a redraw and set the next deadline one frame interval later (`max(1, 1_000_000_000 / fps_limit)` nanoseconds). Wait for the earlier of the next update or render deadline. |
| `u32::MAX` | The host requests a redraw and uses `ControlFlow::Poll`. |

A native redraw event performs presentation separately from this scheduling logic. Before the render callback, the host takes accumulated egui input as `egui::RawInput`; `GameInstance::render` passes it to `UserInterfaceContext::run`.

> **Limitation:** `Window::open` does not reject a zero `update_interval`. The accumulator condition then stays true after subtracting zero, so the scheduler cannot advance past the update loop.

See [windows and surfaces](@/internals/runtime/surfaces.md) for redraw acquisition and presentation recovery, and [GPU accelerator](@/internals/runtime/accelerator.md) for the shared GPU context.
