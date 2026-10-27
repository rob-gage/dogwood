+++
title = "Graphics"
description = "Sprite sheets, borrowed scene graphics, and the engine_graphics scene renderer API."
weight = 90
[extra]
sources = ["engine_graphics/src/lib.rs", "engine_graphics/src/sprites", "engine_graphics/src/renders/mod.rs", "engine_graphics/src/renders/scene_renderer.rs", "engine_graphics/src/world/scenes", "engine/src/lib.rs", "engine/src/games/game_instance.rs", "engine_graphics/src/sprites/sprite_sheet.rs", "engine_graphics/src/sprites/sprite_sheet_error.rs", "engine_graphics/src/renders/scene_renderer_lighting_pass.rs", "engine_graphics/src/renders/scene_renderer_lighting_blocks.wgsl", "engine_graphics/src/renders/scene_renderer_lighting.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_cascades.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_mips.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_optical.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_spread.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_sprites.wgsl"]
+++

Game code generally needs only `SpriteSheet`. Integrators providing their own render loop can use `SceneRenderer` and `SceneGraphics`; the engine also uses them internally. `SceneRenderer`, `SceneRendererViewMode`, `SceneGraphicsOverlay`, `SceneGraphicsOverlayShape`, and `SceneGraphicsTemperature` are re-exported from `engine`; borrowed scene graphics records live in `engine_graphics`.

## SpriteSheet

`SpriteSheet` is re-exported from the facade as `engine::SpriteSheet` (defined in `engine_graphics::sprites`). Constructors upload straight-alpha sRGB RGBA8 pixels into a GPU texture divided into equal frames, numbered row-major from the top-left.

| Constructor | Input |
|---|---|
| `from_png(accelerator, bytes, columns, rows)` | Encoded PNG bytes. |
| `from_tga(accelerator, bytes, columns, rows)` | Encoded TGA bytes. |
| `from_rgba(accelerator, pixels, width, height, columns, rows)` | Raw RGBA8 bytes and texture/frame grid dimensions. |
| `from_color(accelerator, red, green, blue, alpha)` | One RGBA color as a single frame. |

### Errors

`from_rgba` fails with `SpriteSheetError::InvalidDimensions` when:

- any dimension or grid count is zero;
- the texture exceeds the device limit;
- the dimensions do not divide evenly into the frame grid;
- the byte count is not exactly `width * height * 4`.

Uploads generate a complete CPU mip chain. Each level uses a box filter; RGB is averaged in
linear light and alpha linearly.

`from_png` and `from_tga` return `SpriteSheetError::Decode(image::ImageError)` for decode failures, or `InvalidDimensions` for an invalid decoded frame grid or device dimensions.

### Accessors

- `width()` and `height()` return per-frame pixel dimensions.
- `frame_count()` returns columns times rows.
- `frame_texture_region(frame)` returns normalized left, top, right, and bottom coordinates, or `None` when the index is out of range.
- `texture_view()` borrows the GPU texture view.

## Scene graphics and renderer

`SceneGraphics`, `SceneGraphicsCellular`, `SceneGraphicsFluid`, `SceneGraphicsGas`, `SceneGraphicsPressure`, `SceneGraphicsTemperature`, `SceneGraphicsOverlay`, `SceneGraphicsOverlayShape`, and `SceneGraphicsSprites`, and `SceneGraphicsParallax` (in `engine_graphics::world::scenes`) are borrowed descriptions of one frame:

- the visible tile dimensions;
- optional resident cellular buffers and ring placement;
- optional fluid raster, material-property, and per-particle color buffers with ring placement;
- optional gas cell, material identifier, and gas material-property buffers with ring placement;
- optional published pressure buffer with ambient pressure and ring placement;
- optional tile-major matter buffer with ambient temperature and ring placement;
- actor and host overlays aligned to scene cells, each a circle radius or a rectangle width and height in tiles with a packed RGBA8 color;
- sprite albedo and radiance texture views with position, size, rotation, depth, independent
  texture regions, linear color, and radiance intensity.

The actor sprite tuple that feeds the sprite data is documented on [actor components](@/external-api/actor-components.md). In `SceneGraphicsSprites`, each 12-value entry is `(albedo view, radiance view, x, y, width, height, rotation, depth, albedo region, radiance region, color, radiance intensity)`.

`SceneRenderer` is in `engine_graphics::renders`. Create it with `SceneRenderer::new(accelerator)`, then call `render(accelerator, texture_view, &graphics)` on a mutable renderer. `render` submits its command buffer to `accelerator.queue()`.

The target must be single-sample.

Each frame is drawn in this order, into a centered letterboxed viewport:

```text
[negative-depth sprites] -> [fluid] -> [gas] -> [cellular content] -> [nonnegative-depth sprites] -> [pressure or temperature tint, optional] -> [cell overlays]
```

`SceneRendererViewMode::Pressure` and `Temperature` enable their respective cell tints. `GameInstance::set_view_mode` selects the view and `set_overlays` supplies host overlays. Cellular rendering is skipped without resident data or a nonempty viewport. See [scene renderer](@/internals/rendering/scene-renderer.md) for the render passes.

{{ <rustdoc path="engine_graphics/sprites/struct.SpriteSheet.html" text="SpriteSheet" /> }} ·
{{ <rustdoc path="engine_graphics/sprites/enum.SpriteSheetError.html" text="SpriteSheetError" /> }} ·
{{ <rustdoc path="engine_graphics/world/scenes/struct.SceneGraphics.html" text="SceneGraphics" /> }} ·
{{ <rustdoc path="engine_graphics/renders/struct.SceneRenderer.html" text="SceneRenderer" /> }} ·
{{ <rustdoc path="engine_graphics/renders/enum.SceneRendererViewMode.html" text="SceneRendererViewMode" /> }}

Parallax records contain projected centers and sizes plus physical depth. `SceneRenderer` draws these planes farthest first through a separate sprite pass before all gameplay content. See [parallax backgrounds](@/external-api/scenes.md#parallax-backgrounds) for authoring and projection.

Sprite radiance uses a second `SpriteSheet`, independent of the albedo sheet. See
[Lighting](@/external-api/lighting.md) and the [lighting pass](@/internals/rendering/lighting-pass.md).
