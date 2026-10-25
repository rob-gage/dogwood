+++
title = "Graphics internals"
description = "How the scene renderer prepares and draws cellular content and sprites."
weight = 50
[extra]
sources = ["engine_graphics/src/renders", "engine_graphics/src/world/scenes"]
+++

## Renderer structure

This page is for engine contributors working on `engine_graphics/src/renders`. It explains how `SceneRenderer` prepares and draws a scene's cellular content and sprites.

`SceneRenderer` owns seven passes and draws them in this order, so the first sprite group is behind cells and the second in front:

```text
SceneGraphics -> SceneRenderer:
  [sprites pass, depth < 0] -> [fluid pass] -> [gas pass] -> [cellular pass]
    -> [sprites pass, depth >= 0] -> [pressure pass, optional] -> [overlay pass]
  (one render pass, cleared to black)
```

Sprite selection and cellular content come from `SceneGraphics`, which borrows the scene's buffers. The cellular pass retains cloned handles to the material and color buffers while its bind group is cached. See [graphics API](@/external-api/graphics.md) for the graphics-facing types and [world internals](@/internal-api/world.md) for scene ownership.

{{ <rustdoc path="engine_graphics/renders/struct.SceneRenderer.html" text="SceneRenderer" /> }}

## Prepare and draw

### Viewport

`SceneRenderer::render` reads the target texture's format and dimensions.

- When both visible dimensions are positive, the uniform scale is the smaller of target width divided by visible width and target height divided by visible height. Otherwise the scale is zero.
- It rounds the resulting viewport dimensions, clamps them to the target, and centers the viewport.
- Cellular preparation is skipped when the viewport is empty, cellular graphics are absent, or any resident region dimension or cellular tile size is zero.

### Passes

Each applicable pass receives `prepare` before the renderer opens its render pass. Preparation ensures a pipeline exists for the target format and uploads per-view uniforms. Each pass draws only when a pipeline exists for the supplied format; `SceneRenderer::render` passes the target format to both `prepare` and `draw`.

Before drawing, two compute dispatches classify and shade visible fluid cells; see [fluid rendering](@/internals/rendering/fluid-pass.md). Drawing opens one render pass that clears the full target to opaque black. For a visible viewport, it sets the centered viewport and scissor and draws the passes in the order shown above. Pressure view mode adds a tint pass after sprites; overlays draw last. It submits the finished command buffer directly to the accelerator queue; this renderer does not take a caller-owned encoder.

## Cellular pass resources

`SceneRendererCellularPass` owns one shader module, pipeline layout, and uniform buffer, plus a `HashMap<TextureFormat, RenderPipeline>`. Pipelines use triangle-list topology, one sample, no depth/stencil, and premultiplied-alpha blending. Its 64-byte uniform buffer is rewritten for each prepared view with viewport origin and size, pixels per tile, camera-relative region origin, region dimensions, ring offsets, and cellular tile size.

The cellular bind group contains the uniform buffer and the material-identifier and color storage buffers. The pass caches the two input buffer handles and rebuilds the bind group only when either differs from the current `SceneGraphicsCellular` buffers (or when no bind group exists). The pipeline and bind group are separate caches: changing target format creates or selects a pipeline without itself rebuilding the data bindings.

## Sprite pass resources

There are two `SceneRendererSpritesPass` instances with disjoint inclusive depth ranges: `i32::MIN..=-1` and `0..=i32::MAX`. Each owns its own format-keyed pipeline map, uniform buffer, instance buffer, and texture bind-group cache. Sprite pipelines use triangle strips, one sample, no depth/stencil, and premultiplied-alpha blending. A nearest-filter sampler clamps texture coordinates to each texture edge.

### Preparation

Preparation filters sprites by the pass's depth range, sorts selected indices in ascending depth order (the stable sort preserves their input order at equal depth), packs per-instance values, and groups adjacent instances that use the same texture view. The instance buffer begins with capacity for 256 instances at 56 bytes each and grows to the next power of two when needed. Preparation creates missing texture bind groups for current batches and removes stale entries.

### Drawing

Drawing binds each batch texture and draws four vertices for its instance range.

> **Limitation:** Pipelines are cached per target format and never evicted.

For the frame-level order that calls scene and UI rendering, see [frame lifecycle](@/architecture/frame-lifecycle.md). `SceneRenderer` is exposed by `engine_graphics::renders`; its pass types are private, and `SceneRendererViewMode`, `SceneGraphicsOverlay`, and `SceneGraphicsOverlayShape` are re-exported by the `engine` facade.
