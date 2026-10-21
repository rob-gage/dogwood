+++
title = "Editor internals"
description = "How the editor owns a game instance, composes its viewport and tools, and routes input."
weight = 70
[extra]
sources = ["editor/src"]
+++

## Editor state and launch

This page is for contributors working on the `editor` crate. It explains what `Editor<G>` owns, how a frame flows through it, and how input is routed between the editor UI, the free camera, and the hosted game.

### State

`Editor<G>` owns:

- a `GameInstance<G>`, an egui context and optional renderer, and a dock workspace;
- an optional offscreen viewport texture;
- viewport focus and hover state, and keyboard capture state;
- play/pause state and free-camera state;
- editing tool and brush settings, save status, and frame/update rate counters.

The dock starts with `Viewport` and `Tools` tabs. The initial split gives the tools tab a 260-point target width, clamped to 5-50% of available width. Both tabs are non-closeable and cannot be moved into separate windows.

### Launch

`Editor::launch(engine, game)` takes the window configuration from the game and creates a `GameInstance` from the engine accelerator. It wires window, keyboard, mouse, update, and render closures through a `RefCell<Editor<G>>`, then opens the host window. Window errors convert to `EngineError`.

Defaults at launch:

- the editor is paused, in Insert mode;
- the brush is a circle of 8 cells, white;
- the selected material is the first in the picker (cellular static first, then cellular dynamic, then fluid), or the eraser if the picker is empty;
- tile and chunk grid overlays are hidden;
- the update interval is 60 Hz, with a maximum accumulated interval of six updates.

See [editor crate API](@/external-api/editor.md) and [host internals](@/internal-api/host.md).

{{ <rustdoc path="editor/editor/struct.Editor.html" text="Editor" private={true} /> }}

## Frame and callback flow

### Update

The editor moves its free camera from held WASD or arrow keys. Each axis moves by the camera's visible width multiplied by elapsed seconds; diagonal movement combines both axis displacements. When playing, it calls `GameInstance::update`; while paused, it calls `GameInstance::stream`, so the scene keeps streaming without game simulation.

### Render

- The editor derives game input from the previous frame's viewport rectangle, composes the editor UI, and takes egui platform output for the host to apply.
- It lazily creates the UI renderer for the window target format.
- If the viewport tab was shown, it creates or resizes an `Rgba8UnormSrgb` offscreen texture to the tab's physical pixel size and renders the game into it. See [Editor viewport](@/internals/rendering/editor-viewport.md).
- It then clears the host target, paints the editor overlay, and submits egui callback command buffers before the encoder's finished command buffer.

The hosted game's UI is drawn into the viewport, but its platform output is discarded; the host receives the editor UI's platform output.

### Input routing

Game input is active when the editor is playing and the free camera is off.

- **Game egui input:** When game input is active, pointer moves, presses, and wheel events require viewport hover; pointer releases also pass through outside the viewport. Keys, text, copy, cut, and paste require viewport focus. Pointer coordinates and the screen rectangle are viewport-relative. When game input is inactive, the visible game still receives an input frame, but its event list is empty; in both cases its focus flag requires both window and viewport focus.
- **Native mouse:** Events are forwarded only while game input is active and the viewport is hovered.
- **Native keyboard releases:** Always reach the game and clear held keys.
- **Native keyboard presses:** Dropped when the editor UI wants keyboard input. Otherwise they go to the free camera when it is active and the viewport is hovered or focused. If not, they reach the game only when game input is active and the viewport is focused.
- **Focus:** Viewport focus starts on click or drag and ends on Escape or a press outside the viewport.

## Tools, strokes, and save state

### Brush strokes

The tools tab lists registered cellular static, cellular dynamic, and fluid materials from the scene's `MaterialTable`, followed by the eraser. Brush size ranges from 1 to 64 cells.

- Circle uses `Edit::fill_segment` between cell centers with a radius of half the brush size.
- Square stamps a brush-sized square along interpolated stroke samples; strokes continue from the previous sample to avoid gaps.
- Material strokes use the selected insert or replace mode and the opaque selected color. Erasing uses replace mode with `MaterialIdentifier::EMPTY`.
- The generated edit is clipped to the scene's active cellular tile region before `Scene::edit`.

Edge cases:

- An empty clipped edit is not sent to `Scene::edit`, but the editor still treats the request as successful, clears the error, and marks itself edited.
- Failed edits are retained as the displayed error.

The viewport also supports tile and chunk boundary overlays.

### Free camera

When the game has a scene, enabling the free camera creates a persistent scene actor with a copy of the game camera's position and camera component, selects it for `GameInstance`, and despawns it when disabled. Zoom multiplies the current zoom by `1 - scroll_y * 0.002` and is clamped to 0.25-2.0. Scene streaming requests a region twice the camera's unzoomed dimensions around its tile coordinates.

### Saving

When the game has a scene, the save button and `COMMAND+S` call `Scene::save`, clear the edited indicator, and set a pending flag. The editor clears that flag and posts a three-second "Scene saved" notification once `Scene::is_saved()` becomes true.

> **Not implemented:** Gas painting. Fluid paint uses queued particles; [fluid edits](@/internals/world-streaming/fluid-streaming.md#editing-fluid) describe why particle changes need simulation even though streaming continues while paused.

See [editor usage](@/examples/using-the-editor.md) for the user-facing controls, and [collision and edit API](@/external-api/collisions-and-edits.md) for edit semantics.
