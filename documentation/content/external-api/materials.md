+++
title = "Materials"
description = "Material forms, identifiers, registration, compilation, and available material table lookups."
weight = 70
[extra]
sources = ["engine_world/src/materials", "engine_graphics/src/world/materials", "engine/src/lib.rs"]
+++

Materials are registered in a `MaterialRegistry`, compiled into an immutable `MaterialTable`, and referenced by `MaterialIdentifier`. This page is the API reference for defining and querying materials; see [defining materials](@/examples/materials-and-sprites.md) for construction examples. The facade exports the API as `engine::materials`.

## Forms and properties

A `Material` is one of four variants, one per `MaterialForm`: `CellularStatic`, `CellularDynamic`, `Fluid`, or `Gas`. Each variant carries a `String` name and its properties struct; `Material::name()` and `Material::form()` return those values.

| Properties type | Fields | Meaning |
|---|---|---|
| `MaterialPropertiesCellularStatic` | `graphics`; `mass`, `friction`, `restitution: f32`; `debris: Option<MaterialIdentifier>` | `mass` uses dynamic cellular material mass units. `friction` is the Coulomb coefficient for rigid bodies. `restitution` is the fraction of speed kept, reversed, on collision. `debris` selects the dynamic material used when cells break off, or `None` to vanish. |
| `MaterialPropertiesCellularDynamic` | `graphics`; `mass`, `friction`, `restitution: f32` | `mass` uses dynamic cellular material mass units. `friction` is the tangent of the steepest slope a resting cell holds. `restitution` is the fraction of speed kept, reversed, when a cell hits something it cannot enter. |
| `MaterialPropertiesFluid` | `graphics` | Properties for fluid registration. |
| `MaterialPropertiesGas` | `graphics` | Properties for gas registration. |

### Validation at compile time

- Cellular `mass` must be finite and positive.
- `friction` must be finite and nonnegative.
- `restitution` must be between 0 and 1 inclusive.
- Static `debris` must identify a registered cellular dynamic material.
- Graphics `width` and `height` repetition scales must be finite and positive, and the sprite-sheet `frame` must be valid.

### Graphics properties

`MaterialGraphicsCellularStatic` and `MaterialGraphicsCellularDynamic` share the fields `sprite_sheet: Arc<SpriteSheet>`, `frame` (frame index), and `width` and `height` (repetition scales in cells per pixel). The facade re-exports them as `engine::MaterialGraphicsCellularStatic` and `engine::MaterialGraphicsCellularDynamic`; they are defined in `engine_graphics::world::materials`.

`MaterialGraphicsFluid` and `MaterialGraphicsGas` are empty structs in the same module and are not re-exported by the facade.

> **Limitation:** Fluid and gas materials can be registered and queried, but have no simulation rules, form-specific rendering, or GPU property buffers, and edits cannot place them. If a fluid or gas identifier appears in a chunk, the generic cellular renderer still draws it from its stored color. Static cellular GPU property buffers are also absent; dynamic cellular properties are uploaded.

## Identifiers

`MaterialIdentifier` wraps a `u32`:

- Bits 31-30 encode the `MaterialForm`.
- Bits 29-0 encode a form-specific index.
- `new(form, index)` rejects indices above `0x3fff_ffff`.
- `as_u32()` and `from_u32()` convert to and from the raw value; `from_u32()` does not check that the identifier is registered.
- `form()` returns the encoded `MaterialForm`, and `index()` returns the form-specific index.
- `EMPTY` is raw zero, the reserved cellular-static index zero.

Registered static materials start at index 1. Dynamic, fluid, and gas indices start at zero.

## MaterialRegistry

- `MaterialRegistry::new()` creates an empty registry.
- `register(material)` checks that the trimmed name is nonempty, assigns the next index for that form, and returns the identifier.
- `get`, `material_count`, and the form-specific property methods query registered entries. `material_count()` counts registrations across all forms.
- `cellular_static_graphics()` yields `None` for the empty identifier, then each registered static material's graphics in identifier order.
- `compile(self, accelerator)` consumes the registry and returns an immutable `MaterialTable`. Compilation validates the properties above and reads sprite-sheet albedo frames.

`register` and `compile` return `MaterialError`, which has four variants: `EmptyName`, `CapacityExceeded`, `IdentifierIndexOutOfRange`, and `Compilation(String)`.

## MaterialTable

`MaterialTable` provides `get`, `material_count`, `cellular_static_properties`, `cellular_dynamic_properties`, `fluid_properties`, `gas_properties`, and `cellular_color(identifier, cell, color)`. Typed lookups return `None` for an unregistered identifier or one of another form.

`cellular_color` returns the supplied color for non-cellular materials or missing albedo data. For cellular materials with albedo, it samples the albedo at the cell center. The pattern repeats from the scene origin, with image rows flipped. It applies the packed sRGB tint, multiplying RGB in linear light and alpha separately. Colors use RGBA bytes, with red in the least-significant byte: `u32::from_le_bytes([r, g, b, a])`.

### Advanced: GPU buffers

The accessors `cellular_static_properties_buffer`, `cellular_dynamic_properties_buffer`, `fluid_properties_buffer`, and `gas_properties_buffer` expose GPU buffers; the dynamic accessor returns a buffer and the others return optional buffers. Only the dynamic buffer holds records. Compilation waits for cellular albedo-frame texture readback, so it completes synchronously from the caller's perspective. See [material table compilation](@/internals/simulation/material-table.md) for buffer layout and tint details.

## Rustdoc

Core: {{ <rustdoc path="engine/materials/enum.Material.html" text="Material" /> }} · {{ <rustdoc path="engine/materials/enum.MaterialForm.html" text="MaterialForm" /> }} · {{ <rustdoc path="engine/materials/struct.MaterialIdentifier.html" text="MaterialIdentifier" /> }} · {{ <rustdoc path="engine/materials/struct.MaterialRegistry.html" text="MaterialRegistry" /> }} · {{ <rustdoc path="engine/materials/struct.MaterialTable.html" text="MaterialTable" /> }} · {{ <rustdoc path="engine/materials/enum.MaterialError.html" text="MaterialError" /> }}

Properties: {{ <rustdoc path="engine/materials/struct.MaterialPropertiesCellularStatic.html" text="MaterialPropertiesCellularStatic" /> }} · {{ <rustdoc path="engine/materials/struct.MaterialPropertiesCellularDynamic.html" text="MaterialPropertiesCellularDynamic" /> }} · {{ <rustdoc path="engine/materials/struct.MaterialPropertiesFluid.html" text="MaterialPropertiesFluid" /> }} · {{ <rustdoc path="engine/materials/struct.MaterialPropertiesGas.html" text="MaterialPropertiesGas" /> }}

Graphics: {{ <rustdoc path="engine/struct.MaterialGraphicsCellularStatic.html" text="MaterialGraphicsCellularStatic" /> }} · {{ <rustdoc path="engine/struct.MaterialGraphicsCellularDynamic.html" text="MaterialGraphicsCellularDynamic" /> }} · {{ <rustdoc path="engine_graphics/world/materials/struct.MaterialGraphicsFluid.html" text="MaterialGraphicsFluid" /> }} · {{ <rustdoc path="engine_graphics/world/materials/struct.MaterialGraphicsGas.html" text="MaterialGraphicsGas" /> }}
