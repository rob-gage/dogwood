+++
title = "Cellular render pass"
description = "GPU resources and draw setup for resident cellular tile colors."
weight = 30
[extra]
sources = ["engine_graphics/src/renders/scene_renderer_cellular_pass.rs", "engine_graphics/src/renders/scene_renderer_cellular.wgsl", "engine_graphics/src/world/scenes/scene_graphics_cellular.rs"]
+++

## Resources

This page is for engine contributors. It describes the GPU resources and draw setup of the pass that renders resident cellular tile colors.

`SceneRendererCellularPass` owns a 64-byte uniform buffer, a bind group layout, a WGSL module, and render pipelines cached by target `TextureFormat`. The bind group has three fragment-visible buffers:

| Binding | Resource | Access |
| --- | --- | --- |
| 0 | Per-view uniform buffer | Uniform |
| 1 | Material identifier buffer | Read-only storage |
| 2 | Packed color buffer | Read-only storage |

The two storage buffers come from the borrowed {{ <rustdoc path="engine_graphics/world/scenes/struct.SceneGraphicsCellular.html" text="SceneGraphicsCellular" /> }} input. The pass rebuilds its bind group when either storage buffer handle changes; uniforms are written for every prepared view.

## Preparation and draw

### Pipeline

`prepare` creates a pipeline on first use of each target format. The pipeline has these properties:

- `vertex` and `fragment` entry points in `scene_renderer_cellular.wgsl`.
- Triangle-list primitive without culling, one sample, and no depth state.
- Premultiplied-alpha blending.
- No vertex buffers.

### Uniform

The uniform encodes:

- viewport origin and dimensions in pixels;
- `pixels_per_tile`;
- the negated camera-relative region origin;
- active region width and height;
- tile-ring offsets and cells per tile.

The WGSL defines the matching layout and maps fragments through the tile ring; see [Render shaders](@/internals/shaders/render-shaders.md) for the mapping and bindings.

### Draw

`draw` selects the pipeline for the current target format, binds the prepared group at index 0, and draws vertices `0..3` for one instance. That range forms a full-screen triangle over the active viewport. The pass runs between sprites of negative depth and sprites of nonnegative depth; see [Scene renderer](@/internals/rendering/scene-renderer.md).
