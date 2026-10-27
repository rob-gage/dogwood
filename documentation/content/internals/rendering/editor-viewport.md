+++
title = "Editor viewport"
description = "How the editor renders a game into an offscreen texture and maps viewport coordinates to the scene."
weight = 30
[extra]
sources = ["editor/src/editor_viewport.rs", "editor/src/editor.rs", "engine_world/src/scenes/scene.rs", "engine/src/games/game_instance.rs"]
+++

## Offscreen game rendering

This page is for editor contributors. It covers how the editor draws a running game into an egui image, maps viewport coordinates to the scene, and implements the free camera.

### Texture and creation

The editor creates an `EditorViewport` lazily when the viewport tab is drawn. It owns a texture view and the egui texture ID registered for that view.

- The texture is a single-sample 2D `Rgba8UnormSrgb` target with render-attachment and texture-binding usage.
- Each dimension is clamped to at least one physical pixel.
- The egui renderer registers it with nearest filtering.

On first visibility, `render_viewport` has no texture to paint. `Editor::render` creates and renders it after composing that UI frame, so the image appears on the next frame.

### Sizing sequence

Each editor frame handles the viewport in this order:

1. `render_viewport` records the tab's available egui rectangle.
2. The editor composes the UI using the current texture dimensions, so overlay coordinates use the existing texture size.
3. The editor converts the tab's point dimensions to rounded physical pixels with the current `pixels_per_point`, and creates or resizes the texture if needed. Resizing replaces the view and updates the existing egui texture registration.
4. `GameInstance::render` draws the game into the texture.
5. The editor paints the registered texture into the tab rectangle with full-range UVs.

The [scene renderer page](@/internals/rendering/scene-renderer.md) describes the game's own camera framing and render passes. The [UI renderer page](@/internals/rendering/user-interface.md) covers the editor's egui frame.

## Camera mapping and overlays

### Scale and letterboxing

The camera provides visible width and height in tiles. `EditorViewport::pixels_per_tile` divides texture width by visible width and texture height by visible height, then uses the smaller ratio. This fits the camera view into the texture without stretching; unused extent is letterboxed. If either visible dimension is non-positive, the scale is zero.

### Position mapping

- Scene to texture: take the offset from the camera position, scale it by pixels per tile, add it to the texture center, and flip the vertical axis.
- Texture to scene: `scene_position` applies the inverse mapping. If the scale is zero, it returns the camera position.

Pointer positions in letterboxed areas map beyond the camera's visible bounds; the conversion does not clip them to the displayed scene.

### Overlays and pointer

Editor grid and brush overlays use this same mapping. The editor converts the pointer position from egui points to texture pixels, accounting for the current texture-to-tab scale. `EditorViewport::scene_position` maps that pixel to a world position, and the editor derives the cursor cell from it. Overlay positions are mapped back through `pixel_position`. These methods work in scene positions and tiles; cell selection is a later conversion in the editor.

## Free camera and game input

### Free camera lifecycle

Switching to the free camera copies the game's camera position and camera component into a new actor with `ActorPersistenceComponent`, then selects it as the `GameInstance` camera. The free camera copies the game camera's width, height and current zoom.

Switching back despawns that actor, restores the game camera, and clears the editor's free-camera held-key set.

The requested region keeps the same size as the game camera's request and is centered on the free camera. The active region shifts after incoming tiles are resident. Zoom changes the visible area, not the requested region's size.

### Free camera controls

While the free camera is selected:

- Held `D`/Right and `A`/Left move horizontally; `W`/Up and `S`/Down move vertically.
- Each movement axis advances at one visible camera width per second. Diagonal movement combines both axes without normalization.
- Scrolling over the viewport updates zoom as `clamp(zoom * (1 - smooth_scroll_delta.y * 0.002), 0.25, 2.0)`.

### Input coordinates

Focus and native/egui event routing are described in [editor internals](@/internal-api/editor.md). For viewport-relative coordinates, native cursor movement subtracts the viewport's physical-pixel origin, while egui pointer coordinates subtract its egui-point origin.

## View modes

The editor can draw the scene in its normal graphics mode, pressure mode, or temperature mode. Temperature mode tints cells relative to `Scene::ambient_gas_temperature()` and uses a 6000 K upper display reference. Its legend uses the same ambient value.

## API reference

- {{ <rustdoc path="editor/editor_viewport/struct.EditorViewport.html" text="EditorViewport" private={true} /> }}
