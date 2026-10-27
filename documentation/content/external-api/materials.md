+++
title = "Materials"
description = "Material forms, identifiers, registration, compilation, and available material table lookups."
weight = 70
[extra]
sources = ["engine_world/src/materials", "engine_graphics/src/world/materials", "engine/src/lib.rs"]
+++

Materials are registered in a `MaterialRegistry`, compiled into an immutable `MaterialTable`, and referenced by `MaterialIdentifier`. This page is the API reference for defining and querying materials; see [defining materials](@/examples/materials-and-sprites.md) for construction examples. The facade exports the API as `engine::materials`.

## Forms and properties

Games register materials with a name and one form-specific properties struct. References such as
static debris and transition products use names; compilation resolves them after all definitions
have been registered.

| Registration method | Properties type | Additional properties |
|---|---|---|
| `register_cellular_static(name, properties)` | `MaterialPropertiesCellularStatic` | `mass`, `friction`, `restitution`, `strength`, `absorption`, `debris: Option<String>` |
| `register_cellular_dynamic(name, properties)` | `MaterialPropertiesCellularDynamic` | `mass`, `friction`, `restitution` |
| `register_fluid(name, properties)` | `MaterialPropertiesFluid` | `density`, `viscosity`, `cohesion`, `artificial_pressure`, `drag`, `friction`, `restitution`, `edit_color` |
| `register_gas(name, properties)` | `MaterialPropertiesGas` | `mass`, `diffusivity`, `cohesion`, `graphics` (`color`, `opacity`) |

Every form also has `thermal: MaterialPropertiesThermal`, with `heat_capacity`, `conduction`,
`edit_temperature`, `hot_transition`, and `cold_transition`. Defaults are heat capacity `1.0`,
conduction `0.0`, edit temperature `293.0`, and no transitions. Gas heat capacity is per
atmosphere. `MaterialTransition::new(temperature, latent_heat, product)` takes a product name,
then `with_amount` and `with_color` set optional values; defaults are amount `1.0` and identity
color transformation.

> **Not implemented:** Material transitions are not yet simulated.

Registration rejects empty trimmed names and duplicate exact names. Names are case-sensitive and
are not trimmed before lookup. Compilation rejects unknown references, debris that does not name a
cellular dynamic material, and invalid transitions. The compilation validation limits are listed
in [the material table page](@/internals/simulation/material-table.md).

### Validation at compile time

- Cellular mass must be finite and positive; friction finite and nonnegative; restitution in
  `[0, 1]`.
- Static strength must be positive and not NaN (positive infinity is allowed), and absorption must
  be finite in `[0, 1]`.
- Fluid density must be finite and positive; viscosity and restitution in `[0, 1]`; cohesion,
  artificial pressure, drag, and friction finite and nonnegative. Viscosity controls XSPH velocity
  smoothing. Fluid friction also controls dynamic-cell contacts.
- Gas mass must be finite and positive; diffusivity in `[0, 0.4]`; cohesion in `[0, 1]`; at most
  255 gas materials may be registered.
- Thermal heat capacity must be finite and positive; conduction and edit temperature finite and
  nonnegative. Transition temperatures and latent heat must be nonnegative, amount at least
  1/4096, and hot transition temperature above cold transition temperature. Color transformation values must
  be finite, and color variances nonnegative.
- Cellular albedo scales must be finite and positive, and each sprite frame must be valid.

### Graphics properties

`MaterialGraphicsCellularStatic` and `MaterialGraphicsCellularDynamic` share the fields `sprite_sheet: Arc<SpriteSheet>`, `frame` (frame index), and `width` and `height` (repetition scales in cells per pixel). The facade re-exports them as `engine::MaterialGraphicsCellularStatic` and `engine::MaterialGraphicsCellularDynamic`; they are defined in `engine_graphics::world::materials`.

`MaterialPropertiesFluid::edit_color` is packed sRGB RGBA; alpha specifies the opacity of a full cell when edits create fluid particles. `MaterialGraphicsGas` has a packed `color: u32` and an `opacity` multiplier; alpha specifies the opacity of one atmosphere of the gas before that multiplier. It is re-exported as `engine::MaterialGraphicsGas`.

Fluid materials use [particle simulation](@/internals/simulation/fluids.md) and [raster-based rendering](@/internals/rendering/fluid-pass.md). Material density supplies physical mass; the pressure solver uses a shared rest spacing.

Gas species use the shared [pressure substrate](@/internals/simulation/pressure.md) in the cellular tiles and [cell-aligned rendering](@/internals/rendering/gas-pass.md).

Placing a fluid identifier directly in a cellular array does not create a particle; use edits or chunk particle records.

## Identifiers

`MaterialIdentifier` is an internal simulation handle that wraps a `u32`. Games should obtain
handles from `MaterialTable::lookup` after compilation rather than predicting their indices:

- Bits 31-30 encode the `MaterialForm`.
- Bits 29-0 encode a form-specific index.
- `new(form, index)` rejects indices above `0x3fff_ffff`.
- `as_u32()` and `from_u32()` convert to and from the raw value; `from_u32()` does not check that the identifier is registered.
- `form()` returns the encoded `MaterialForm`, and `index()` returns the form-specific index.
- `EMPTY` is raw zero, the reserved cellular-static index zero.

Registered static materials start at index 1. Dynamic, fluid, and gas indices start at zero.

## MaterialRegistry

- `MaterialRegistry::new()` creates an empty registry.
- The four `register_*` methods take a name and matching properties struct and return
  `Result<(), MaterialError>`.
- `compile(self, accelerator)` assigns identifiers in registration order within each form, resolves
  name references, validates properties, and reads sprite-sheet albedo frames.
- Registration errors include `EmptyName` and `DuplicateName`; compilation can return
  `UnknownMaterial`, `InvalidDebris`, `InvalidTransition`, capacity errors, or
  `Compilation(String)`.

## MaterialTable

`lookup(name)` returns the compiled identifier for an exact, case-sensitive name.
`materials()` iterates `(MaterialIdentifier, name, MaterialForm)` in identifier order.
`get` and the typed property methods return the compiled definitions, including their dependency
names. `debris(identifier)` returns the resolved dynamic debris identifier. `material_count()`
counts all forms.

The table provides `cellular_color(identifier, cell, color)` and the GPU buffer accessors. For
cellular materials with albedo, `cellular_color` samples the frame at the cell center. The pattern
repeats from the scene origin with image rows flipped. It applies the packed sRGB tint, multiplying
RGB in linear light and alpha separately. Colors use RGBA bytes, with red in the least-significant
byte: `u32::from_le_bytes([r, g, b, a])`.

### Advanced: GPU buffers

Compilation keeps the existing GPU record layouts. It waits for cellular albedo-frame texture
readback, so it completes synchronously from the caller's perspective. See
[material table compilation](@/internals/simulation/material-table.md) for buffer layout and tint
details.

## Rustdoc

Core: {{ <rustdoc path="engine/materials/enum.Material.html" text="Material" /> }} · {{ <rustdoc path="engine/materials/enum.MaterialForm.html" text="MaterialForm" /> }} · {{ <rustdoc path="engine/materials/struct.MaterialIdentifier.html" text="MaterialIdentifier" /> }} · {{ <rustdoc path="engine/materials/struct.MaterialRegistry.html" text="MaterialRegistry" /> }} · {{ <rustdoc path="engine/materials/struct.MaterialTable.html" text="MaterialTable" /> }} · {{ <rustdoc path="engine/materials/enum.MaterialError.html" text="MaterialError" /> }} · {{ <rustdoc path="engine/materials/struct.MaterialTransition.html" text="MaterialTransition" /> }}

Properties: {{ <rustdoc path="engine/materials/struct.MaterialPropertiesCellularStatic.html" text="MaterialPropertiesCellularStatic" /> }} · {{ <rustdoc path="engine/materials/struct.MaterialPropertiesCellularDynamic.html" text="MaterialPropertiesCellularDynamic" /> }} · {{ <rustdoc path="engine/materials/struct.MaterialPropertiesFluid.html" text="MaterialPropertiesFluid" /> }} · {{ <rustdoc path="engine/materials/struct.MaterialPropertiesGas.html" text="MaterialPropertiesGas" /> }} · {{ <rustdoc path="engine/materials/struct.MaterialPropertiesThermal.html" text="MaterialPropertiesThermal" /> }}

Graphics: {{ <rustdoc path="engine/struct.MaterialGraphicsCellularStatic.html" text="MaterialGraphicsCellularStatic" /> }} · {{ <rustdoc path="engine/struct.MaterialGraphicsCellularDynamic.html" text="MaterialGraphicsCellularDynamic" /> }} · {{ <rustdoc path="engine/struct.MaterialGraphicsGas.html" text="MaterialGraphicsGas" /> }}
