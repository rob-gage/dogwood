+++
title = "Rendering and UI"
description = "How scene, cellular, sprite, and user interface rendering fit together."
weight = 50
template = "section.html"
sort_by = "weight"
[extra]
sources = ["engine_graphics/src/renders/scene_renderer.rs", "engine/src/games/game_instance.rs", "engine_user_interface/src/user_interface_renderer.rs"]
+++

## Frame composition

This section is for engine contributors. It explains how Dogwood renders a scene and overlays the user interface.

Dogwood renders a scene into a caller-provided texture, then can overlay egui output on that target. The scene renderer clears the target and draws the two sprite depth ranges around resident fluid, gas, and cellular content. The UI renderer uses a load operation, so it preserves the scene image beneath the interface.

```text
Scene::graphics(camera)
        |
        v
SceneRenderer::render(target)
  clear target to opaque black
  viewport: letterboxed to visible tile dimensions
  +-----------------------------------------+
  | negative-depth sprites                  |
  | fluid raster (if resident)              |
  | gas cells (if resident)                 |
  | cellular tiles (if resident)            |
  | zero-and-positive-depth sprites         |
  +-----------------------------------------+
        |
        v
UserInterfaceContext::run(input, compose)
        |
        v
UserInterfaceRenderer::render(target, output)
  egui overlay; preserves scene target
```

## Pages

- [Scene renderer](@/internals/rendering/scene-renderer.md): scene graphics input, viewport, and pass order.
- [Fluid render pass](@/internals/rendering/fluid-pass.md): raster smoothing, depth shading, and blending.
- [Gas render pass](@/internals/rendering/gas-pass.md): cell-aligned cloud edges, slot opacity, and blending.
- [Cellular render pass](@/internals/rendering/cellular-pass.md): uniforms, bindings, and pipeline cache.
- [Sprite render pass](@/internals/rendering/sprite-pass.md): instances, batching, and sprite sheets.
- [User interface rendering](@/internals/rendering/user-interface.md): egui context, input state, overlay renderer, and themes.
- [Game UI layout and appearance](@/internals/rendering/game-ui-layout.md): element tree, Taffy layout, and painting.
- [Editor viewport](@/internals/rendering/editor-viewport.md): offscreen game texture, coordinate mapping, and input routing.

WGSL for scene rendering is in [Render shaders](@/internals/shaders/render-shaders.md).
