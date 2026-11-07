+++
title = "Scene renderer"
description = "Scene graphics data, letterboxed viewport calculation, and render-pass ordering."
weight = 20
[extra]
sources = ["engine_graphics/src/renders/scene_renderer_fluid.wgsl", "engine_graphics/src/renders/scene_renderer_gas_pass.rs", "engine_graphics/src/renders/scene_renderer_fluid_pass.rs", "engine_graphics/src/renders/scene_renderer.rs", "engine_graphics/src/renders/scene_renderer_shaded_pass.rs", "engine_graphics/src/renders/scene_renderer_shaded.wgsl", "engine_graphics/src/renders/scene_renderer_decorations_pass.rs", "engine_graphics/src/renders/scene_renderer_decoration_field.wgsl", "engine_graphics/src/renders/scene_renderer_decorations.wgsl", "engine_graphics/src/renders/scene_renderer_pressure_pass.rs", "engine_graphics/src/renders/scene_renderer_temperature_pass.rs", "engine_graphics/src/renders/scene_renderer_overlay_pass.rs", "engine_graphics/src/renders/scene_renderer_pressure.wgsl", "engine_graphics/src/renders/scene_renderer_temperature.wgsl", "engine_graphics/src/renders/scene_renderer_overlay.wgsl", "engine_graphics/src/renders/scene_renderer_sprites_pass.rs", "engine_graphics/src/renders/scene_renderer_cellular_pass.rs", "engine_graphics/src/renders/scene_renderer_parallax_batches_pass.rs", "engine_graphics/src/renders/scene_renderer_surface_pass.rs", "engine_graphics/src/renders/scene_renderer_surface.wgsl", "engine_graphics/src/world/scenes", "engine_world/src/scenes/chunks/chunk_decoration.rs", "engine_world/src/scenes/chunks/chunk_background.rs", "engine_world/src/scenes/chunks/chunk_streaming_state.rs", "engine_world/src/scenes/scene.rs", "engine_world/src/scenes/scene_backgrounds.rs", "engine_world/src/scenes/scene_material_surfaces.rs", "engine/src/lib.rs", "engine_world/src/scenes/cellular_tiles/mod.rs", "engine_graphics/src/renders/scene_renderer_lighting_pass.rs", "engine_graphics/src/renders/scene_renderer_lighting_clear.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_cache.wgsl", "engine_graphics/src/renders/scene_renderer_lighting.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_cascades.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_mips.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_optical.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_spread.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_sprites.wgsl", "engine_world/src/actors/components/actor_shader.rs"]
+++

## Scene graphics input

This page is for engine contributors. It covers how a scene becomes graphics data, how the renderer computes its viewport, and the order of the render passes.

`Scene::graphics` returns a borrowed `SceneGraphics` for a camera actor, or an `ActorError` if the camera components cannot be read. It does the following:

- Derives the visible width and height from the camera.
- Collects actor sprites relative to the camera position.
- Includes visible chunk backgrounds as static parallax batches, and optional material-surface art.
- Projects sprites on actors with `ActorParallaxComponent`, retaining their physical depth in `SceneGraphicsParallax`.
- Includes cellular, fluid, gas, pressure, and temperature buffers only when a streamed active tile region is available; it also includes actor overlays.

Sprite frames outside a sheet are skipped. Sheet pixel dimensions are converted to tile units using `CELLULAR_TILE_SIZE`. Each valid sprite record applies the component's offsets and scales and carries its rotation, depth, and color.

`SceneGraphics` contains the visible dimensions in tiles, optional `SceneGraphicsCellular`, `SceneGraphicsFluid`, and `SceneGraphicsGas` records, and a `SceneGraphicsSprites` list. The cellular data holds:

- the GPU material-ID and packed-color buffers;
- the active region's camera-relative origin and dimensions in tiles;
- the ring offsets and cells per tile.

The fluid record borrows the mass/momentum raster, material properties, and particle color sums with the same region/ring placement; see [fluid rendering](@/internals/rendering/fluid-pass.md). The gas record borrows the gas cells, material identifiers, and gas material properties with the same placement; see [gas rendering](@/internals/rendering/gas-pass.md). Pressure and temperature records borrow their cell buffers with the same placement and their respective ambient values.

See {{ <rustdoc path="engine_graphics/world/scenes/struct.SceneGraphics.html" text="the scene graphics API" /> }} and the {{ <rustdoc path="engine/scenes/struct.Scene.html" text="Scene API" /> }}.

## Viewport

{{ <rustdoc path="engine_graphics/renders/struct.SceneRenderer.html" text="SceneRenderer" /> }} reads the target texture's dimensions and format in `render`. When both visible dimensions are positive, it computes `pixels_per_tile` as the smaller of target width / visible width and target height / visible height.

It rounds the resulting viewport dimensions, caps them at the texture dimensions, and centers the viewport. The unused bars remain black because the render pass clears the full target to opaque black.

## Pass order

The renderer prepares passes only when the viewport has nonzero dimensions. Cellular rendering also requires a present cellular record with nonzero region width, region height, and tile size. Fluid, gas, pressure, and temperature rendering require the same nonzero viewport and region dimensions. Pressure and temperature rendering are selected by `SceneRendererViewMode::Pressure` and `Temperature`. The renderer classifies and shades fluid cells in two compute dispatches before opening one render pass with no depth attachment and draws in this order:

0. Compute lighting (including shaded-body and glowing-decoration optics), then the decoration motion field, then the surface field for resident cellular data.
1. Parallax planes, sorted farthest first by physical depth, then by sprite integer order.
2. Static `SceneGraphicsParallaxBatch` instances using the shared sprite shader and surface field.
3. Sprites with depth `i32::MIN..=-1`.
4. Fluid raster, when resident fluid data is available.
5. Gas cells, when resident gas data is available.
6. Cellular tiles, compositing material surface art where its encoded reach covers the surface field.
7. Shaded actor soft bodies, when present.
8. Sprites with depth `0..=i32::MAX`.
9. Pressure or temperature tint, when its view mode is selected.
10. Cell-aligned overlays, including actor and host cursor shapes.

Each pass uses premultiplied-alpha blending. `SceneRenderer` finishes its command encoder and submits it to the accelerator queue itself. This is a scene-only render: the UI overlay is a separate stage described in [User interface rendering](@/internals/rendering/user-interface.md).

## Related pages

- [Cellular render pass](@/internals/rendering/cellular-pass.md): cellular buffer layout and draw setup.
- [Render shaders](@/internals/shaders/render-shaders.md): pixel-to-cell mapping and sprite vertex math.
- [Sprite render pass](@/internals/rendering/sprite-pass.md): depth sorting, instance packing, and texture batching.

- [Fluid render pass](@/internals/rendering/fluid-pass.md): cell classification, shading, and blending.

Parallax preparation expands visible copy ranges according to each plane's per-axis `ParallaxRepeat`. Layer offsets set the original placement for finite and repeated sprites. Copy placement rotates with each sprite; candidate ranges cover the inverse-rotated viewport. Patterns exceeding 16,384 copies per sprite are skipped to bound CPU expansion.

Lighting and decoration motion are computed before fluid shading and the main render pass. Decorations use the motion field computed by `SceneRendererDecorationsPass`; see [chunk decorations](@/internals/rendering/decorations.md) for its mapping and inputs. Lighting is consumed by appearance passes. See the [lighting pass](@/internals/rendering/lighting-pass.md) for transport stages and limits.

`SceneRendererParallaxBatchesPass` draws static sprite instances positioned by a per-batch camera-relative origin. `SceneRendererSurfacePass` computes the cellular exposure field consumed by batches and the cellular shader. Field channels and bindings are documented in [cellular rendering](@/internals/rendering/cellular-pass.md#material-surfaces).

`SceneRendererShadedPass` draws soft bodies paired with `ActorShaderComponent` after cellular tiles and before upper sprites. It tests fragments against the body's smoothed outline and passes distance, depth, and outward direction to the actor's shader; `is_cell_snapped` aligns appearance shading samples to world cells. During the lighting pass, each shaded body's world cells are shaded and averaged into lighting texels as emission and occlusion, so `shade` must not use derivatives. The same lighting optics render pass draws glowing decoration pieces.
