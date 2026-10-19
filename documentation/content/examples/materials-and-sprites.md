+++
title = "Defining materials"
description = "Register cellular materials with sprite-sheet albedo and compile them for a scene."
weight = 10
[extra]
sources = ["engine_world/src/materials", "engine_graphics/src/world/materials", "engine_graphics/src/sprites", "template/src/template_game.rs", "engine/src/lib.rs"]
+++

## Register cellular materials

A cellular material defines what a cell is made of: its sprite-sheet look and physical properties. This guide registers a dynamic material (gravel) and a static material (stone) that breaks into gravel, then compiles them into a `MaterialTable` for `Scene::new`.

Each cellular graphics struct holds an `Arc<SpriteSheet>`, a frame index, and horizontal and vertical repetition scales. `width` and `height` are cells per pixel: `1.0` maps each source pixel to one cell. Frames are indexed row-major from the top-left. This example follows the template's PNG loading and registration pattern:

```rust
use engine::{
    materials::{
        Material, MaterialIdentifier, MaterialPropertiesCellularDynamic,
        MaterialPropertiesCellularStatic, MaterialRegistry,
    },
    runtime::EngineError,
    Accelerator, MaterialGraphicsCellularDynamic, MaterialGraphicsCellularStatic, SpriteSheet,
};
use std::sync::Arc;

fn register_gravel(
    accelerator: &Accelerator,
    registry: &mut MaterialRegistry,
) -> Result<(MaterialIdentifier, MaterialIdentifier), EngineError> {
    let gravel = registry.register(Material::CellularDynamic {
        name: String::from("gravel"),
        properties: MaterialPropertiesCellularDynamic {
            graphics: MaterialGraphicsCellularDynamic {
                sprite_sheet: Arc::new(SpriteSheet::from_png(
                    accelerator, include_bytes!("../assets/sprites/gravel.png"), 1, 1,
                )?),
                frame: 0,
                width: 1.0,
                height: 1.0,
            },
            mass: 1.0,
            friction: 0.8,
            restitution: 0.1,
        },
    })?;
    let stone = registry.register(Material::CellularStatic {
        name: String::from("stone"),
        properties: MaterialPropertiesCellularStatic {
            graphics: MaterialGraphicsCellularStatic {
                sprite_sheet: Arc::new(SpriteSheet::from_png(
                    accelerator, include_bytes!("../assets/sprites/stone.png"), 1, 1,
                )?),
                frame: 0,
                width: 1.0,
                height: 1.0,
            },
            mass: 2.0,
            friction: 0.6,
            restitution: 0.1,
            debris: Some(gravel),
        },
    })?;
    Ok((gravel, stone))
}
```

Replace the asset paths with files in your project. Register gravel first because stone's `debris` field needs gravel's `MaterialIdentifier`; `debris: None` makes broken-off cells vanish. The template does the same in its [game setup](@/getting-started/template-walkthrough.md). Property meanings and the full list of material forms are in [the materials API](@/external-api/materials.md#forms-and-properties).

### Load sprite sheets

- `SpriteSheet::from_png` and `from_tga`: an `&Accelerator`, encoded bytes, grid columns, grid rows.
- `SpriteSheet::from_rgba`: an `&Accelerator`, straight-alpha sRGB RGBA8 bytes, pixel width, pixel height, grid columns, grid rows.

Loading fails unless all of these hold:

- Image dimensions and grid counts are nonzero.
- The image fits the device limit.
- The image divides evenly by the grid counts.
- For `from_rgba`, the pixel byte length equals `width * height * 4`.

### Compilation requirements

Compilation checks each cellular material:

- `mass` is finite and positive.
- `friction` is finite and nonnegative.
- `restitution` is in `0.0..=1.0`.
- Repetition scales (`width`, `height`) are finite and positive.
- The sprite frame is valid.
- A non-`None` `debris` identifies a registered dynamic material.

### Errors

- Registration can return `MaterialError::EmptyName`, `CapacityExceeded`, or `IdentifierIndexOutOfRange`.
- Compilation failures are returned as `MaterialError::Compilation`.
- Sprite loading returns `SpriteSheetError::Decode` for image decoding failures, or `InvalidDimensions` for invalid image or frame dimensions or pixel data.

## Compile for a scene

After registration, consume the registry with `registry.compile(accelerator)?` and pass the resulting `MaterialTable` to `Scene::new`. Compilation reads the cellular albedo frames once; `MaterialTable::cellular_color` uses them when the engine creates colored cells. Static index zero is reserved for empty cells. See [editing and saving cells](@/examples/editing-cells.md) for using registered identifiers in world edits and [the materials API](@/external-api/materials.md) for all material forms and errors.

{{ <rustdoc path="engine/materials/struct.MaterialRegistry.html" text="MaterialRegistry" /> }} · {{ <rustdoc path="engine/materials/struct.MaterialTable.html" text="MaterialTable" /> }} · {{ <rustdoc path="engine/struct.SpriteSheet.html" text="SpriteSheet" /> }}
