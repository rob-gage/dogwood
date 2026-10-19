+++
title = "User interface rendering"
description = "The egui context, native input state, theme application, and wgpu overlay lifecycle."
weight = 50
[extra]
sources = ["engine_user_interface/src/user_interface_renderer.rs", "engine_user_interface/src/user_interface_context.rs", "engine_user_interface/src/user_interface_window_state.rs", "engine_user_interface/src/user_interface_theme.rs", "engine_user_interface/src/components", "engine_user_interface/src/advanced"]
+++

## Context and composition

This page is for engine contributors. It covers how the egui context, native input, and wgpu overlay renderer fit into a frame.

{{ <rustdoc path="engine_user_interface/struct.UserInterfaceContext.html" text="UserInterfaceContext" /> }} owns persistent egui state. Construction applies the default dark {{ <rustdoc path="engine_user_interface/struct.UserInterfaceTheme.html" text="UserInterfaceTheme" /> }}. `run(input, compose)` runs the egui frame and returns `egui::FullOutput`; egui may call the composition closure again for a discarded multipass layout.

The context can expose its egui context for native integration, apply another theme, or request repaint. Repaint requests use an egui callback when one is installed. Without a repaint callback, egui's `repaint_delay` remains in `FullOutput`; Dogwood's host redraws from its update/FPS loop and does not use that delay.

The game instance and editor each compose their UI through this context, separate `platform_output` from the frame output, then pass the remaining output to {{ <rustdoc path="engine_user_interface/struct.UserInterfaceRenderer.html" text="UserInterfaceRenderer" /> }}. The game calls its `user_interface` hook; the editor composes its own toolbar, status, dock, and inspector components.

The game-facing element tree is covered in [Game UI layout and appearance](@/internals/rendering/game-ui-layout.md); the editor's embedded game target is in [Editor viewport](@/internals/rendering/editor-viewport.md).

## Native input and platform output

{{ <rustdoc path="engine_user_interface/struct.UserInterfaceWindowState.html" text="UserInterfaceWindowState" /> }} binds egui-winit state to one native window and the root viewport. The host forwards each window event to it, then independently translates and dispatches engine events. The returned consumed/repaint flags are advisory: UI consumption does not suppress game event delivery, lifecycle events are forced unconsumed, and redraw events do not request another repaint.

On redraw, the host takes accumulated egui input (including current viewport and scale information), invokes its render handler, applies the returned platform output to the window, and presents. Platform output carries egui requests such as clipboard, cursor, URL, and IME changes. In an editor, game input is additionally filtered and translated to the embedded viewport; details are in [Editor viewport](@/internals/rendering/editor-viewport.md).

## Overlay rendering

### Renderer and target format

`UserInterfaceRenderer::new` creates a single-sample egui-wgpu renderer for a target format. A target view must use that format; the renderer has no depth or stencil attachment.

The source documentation prefers `Rgba8Unorm` or `Bgra8Unorm` gamma-space targets for egui. The host picks the first sRGB surface format the surface offers, and the editor viewport uses `Rgba8UnormSrgb`. `renderer()` and `renderer_mut()` borrow the inner egui-wgpu renderer; `renderer_mut` is used to register native textures and callback resources.

If the target format changes, recreate the renderer and egui context together; the old context will not replay consumed texture deltas.

### Render and submission

`render` consumes the `FullOutput`, applies texture updates, tessellates shapes with the output's pixels-per-point value, updates egui-wgpu buffers, and paints jobs in an overlay pass using `Load` and `Store`. This preserves previously rendered scene contents. A zero-width or zero-height target skips tessellation and painting but still frees texture deltas scheduled for deletion.

`render` returns callback command buffers and does not submit them. Callers submit those buffers before the finished command encoder buffer. See [Scene renderer](@/internals/rendering/scene-renderer.md) for the scene renderer's own queue submission.

## Themes and editor components

`UserInterfaceTheme::dark` and `light` start from egui's corresponding default style, apply Dogwood spacing, widget visuals, and text sizes, and set a blue accent by default.

- `with_accent` updates links and selection colors, chooses a readable selection foreground, and sets active/open widget border strokes to the accent color.
- `apply` installs the theme and style while preserving egui interaction memory.

The UI crate also provides editor-oriented components and advanced helpers used by the editor, including dock workspace state, inspector sections, settings rows, and notifications. This page covers their place in the composition and rendering path; component-specific behavior belongs with the [UI API documentation](@/external-api/user-interface.md).
