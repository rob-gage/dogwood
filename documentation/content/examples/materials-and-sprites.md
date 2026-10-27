+++
title = "Defining materials"
description = "Register named materials, resolve their dependencies, and compile them for a scene."
weight = 10
[extra]
sources = ["engine_world/src/materials", "engine_graphics/src/world/materials", "engine_graphics/src/sprites", "template/src/template_game.rs", "engine/src/lib.rs"]
+++

## Register cellular materials

A cellular material defines what a cell is made of: its sprite-sheet look and physical properties.
This example registers gravel and stone, then uses the compiled table to obtain identifiers for
world edits and generators. Dependency names can refer to materials registered later.

```rust
use engine::{
    materials::{
        MaterialPropertiesCellularDynamic, MaterialPropertiesCellularStatic,
        MaterialPropertiesThermal, MaterialRegistry,
    },
    runtime::EngineError,
    Accelerator, MaterialGraphicsCellularDynamic, MaterialGraphicsCellularStatic, SpriteSheet,
};
use std::sync::Arc;

fn register_stone(
    accelerator: &Accelerator,
    registry: &mut MaterialRegistry,
) -> Result<(), EngineError> {
    let gravel = MaterialGraphicsCellularDynamic {
        sprite_sheet: Arc::new(SpriteSheet::from_png(
            accelerator, include_bytes!("../assets/sprites/gravel.png"), 1, 1,
        )?),
        frame: 0,
        width: 1.0,
        height: 1.0,
    };
    registry.register_cellular_dynamic("gravel", MaterialPropertiesCellularDynamic {
        edit_color: gravel,
        mass: 1.0,
        friction: 0.8,
        restitution: 0.1,
        thermal: MaterialPropertiesThermal::default(),
    })?;
    registry.register_cellular_static("stone", MaterialPropertiesCellularStatic {
        edit_color: MaterialGraphicsCellularStatic {
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
        strength: 10.0,
        absorption: 0.1,
        debris: Some(String::from("gravel")),
        thermal: MaterialPropertiesThermal::default(),
    })?;
    Ok(())
}
```

Replace the asset paths with files in your project. `debris: None` makes broken-off cells vanish.
Property meanings and validation limits are in [the materials API](@/external-api/materials.md).

### Load sprite sheets

- `SpriteSheet::from_png` and `from_tga`: an `&Accelerator`, encoded bytes, grid columns, grid rows.
- `SpriteSheet::from_rgba`: an `&Accelerator`, straight-alpha sRGB RGBA8 bytes, pixel width, pixel
  height, grid columns, grid rows.

Loading fails unless image dimensions and grid counts are nonzero, the image fits the device limit,
the image divides evenly by the grid counts, and RGBA byte length equals `width * height * 4`.

## Resolve identifiers

Register all definitions, compile the registry, then look up identifiers once for hot paths:

```rust
let material_table = registry.compile(accelerator)?;
let stone = material_table.lookup("stone").expect("registered stone");
let gravel = material_table.lookup("gravel").expect("registered gravel");
```

Pass the table to `Scene::new`, then use these identifiers with edits, chunks, editor tools, and
scene generators. Registered static materials start at index one because static index zero is the
empty cell. See [editing and saving cells](@/examples/editing-cells.md) for world edits.

## Register fluid and gas

Fluid and gas definitions use the same named registration methods. Their thermal settings live in
`MaterialPropertiesThermal`; transitions name their products:

```rust
use engine::{
    materials::{
        MaterialError, MaterialPropertiesFluid, MaterialPropertiesThermal, MaterialRegistry,
    },
};

fn register_water(registry: &mut MaterialRegistry) -> Result<(), MaterialError> {
    registry.register_fluid("water", MaterialPropertiesFluid {
        density: 0.6,
        viscosity: 0.05,
        cohesion: 0.2,
        artificial_pressure: 0.001,
        drag: 1.0,
        friction: 0.0,
        restitution: 0.0,
        edit_color: u32::from_le_bytes([40, 110, 200, 200]),
        thermal: MaterialPropertiesThermal::default(),
    })
}
```

Fluid edit colors use packed sRGB RGBA, whose alpha is the opacity of a full cell. Gas graphics
use packed sRGB RGBA plus an opacity multiplier; alpha sets opacity per atmosphere. Gas heat
capacity is per atmosphere as well. Use the compiled
identifier in world edits to fill empty cells with gas or spawn fluid particles. A fluid identifier
written directly into a cellular material array does not create particles. See [fluid simulation](@/internals/simulation/fluids.md) and [pressure simulation](@/internals/simulation/pressure.md) for the implemented models.

{{ <rustdoc path="engine/materials/struct.MaterialRegistry.html" text="MaterialRegistry" /> }} · {{ <rustdoc path="engine/materials/struct.MaterialTable.html" text="MaterialTable" /> }} · {{ <rustdoc path="engine/struct.SpriteSheet.html" text="SpriteSheet" /> }}
