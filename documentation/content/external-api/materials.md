+++
title = "Materials"
description = "Material forms, identifiers, registration, compilation, and available material table lookups."
weight = 70
[extra]
sources = ["engine_world/src/materials", "engine_world/src/scenes/scene_material_surfaces.rs", "engine_graphics/src/world/materials", "engine_graphics/src/world/scenes/scene_graphics_material_surfaces.rs", "engine_graphics/src/sprites", "engine/src/lib.rs"]
+++

Materials are registered in a `MaterialRegistry`, compiled into an immutable `MaterialTable`, and referenced by `MaterialIdentifier`. This page is the API reference for defining and querying materials; see [defining materials](@/examples/materials-and-sprites.md) for construction examples. The facade exports the API as `engine::materials`.

## Forms and properties

Games register materials with a name and one form-specific properties struct. References such as
static debris and transition products use names; compilation resolves them after all definitions
have been registered.

| Registration method | Properties type | Additional properties |
|---|---|---|
| `register_cellular_static(name, properties)` | `MaterialPropertiesCellularStatic` | `mass`, `friction`, `restitution`, `damage_threshold`, `damage_coefficient`, `absorption`, `debris: Option<String>`, `debris_chance` |
| `register_cellular_dynamic(name, properties)` | `MaterialPropertiesCellularDynamic` | `mass`, `friction`, `restitution`, `damage_threshold`, `damage_coefficient` |
| `register_fluid(name, properties)` | `MaterialPropertiesFluid` | `density`, `viscosity`, `cohesion`, `artificial_pressure`, `drag`, `friction`, `restitution`, `edit_color`, `edit_radiance`, `graphics` |
| `register_gas(name, properties)` | `MaterialPropertiesGas` | `mass`, `diffusivity`, `cohesion`, `graphics` (`color`, `opacity`, `radiance`) |

Every form also has `chemical: MaterialPropertiesChemical`, with reaction-selection `tags` and visual-only `amount_colors`; gases ignore amount colors. Every form also has `thermal: MaterialPropertiesThermal`, with `heat_capacity`, `conduction`,
`edit_temperature`, `hot_transition`, `cold_transition`, and render-time `temperature_colors`. Defaults are heat capacity `1.0`,
conduction `0.0`, edit temperature `293.0`, and no transitions. Gas heat capacity is per
atmosphere. `MaterialTransition::new(temperature, latent_heat, product)` takes a product name,
then `with_amount`, `with_color`, and `with_radiance` set optional values; defaults are amount `1.0`
and identity color and radiance transformations.

Registration rejects empty trimmed names and duplicate exact names. Names are case-sensitive and
are not trimmed before lookup. Compilation rejects unknown references, debris that does not name a
cellular dynamic material, and invalid transitions. The compilation validation limits are listed
in [the material table page](@/internals/simulation/material-table.md).

### Validation at compile time

- Cellular mass must be finite and positive; friction finite and nonnegative; restitution in
  `[0, 1]`.
- Cellular damage thresholds must be positive and not NaN (positive infinity makes the material
  unbreakable); damage coefficients must be finite and nonnegative. Static absorption must be
  finite in `[0, 1]` and damps pressure waves only. Static debris chance must be finite and in
  `[0, 1]`; it is ignored when `debris` is `None`.
- Fluid density must be finite and positive; viscosity and restitution in `[0, 1]`; cohesion,
  artificial pressure, drag, and friction finite and nonnegative. Viscosity is a nonlinear control
  for per-substep XSPH relaxation toward the neighbor-weighted mean velocity. Fluid friction also
  controls dynamic-cell contacts and, with viscosity, tangential damping against solid boundaries;
  the boundary response clamps friction to one.
- Gas mass must be finite and positive; diffusivity in `[0, 0.4]`; cohesion in `[0, 1]`; at most
  255 gas materials may be registered.
- Thermal heat capacity must be finite and positive; conduction and edit temperature finite and
  nonnegative. Transition temperatures and latent heat must be nonnegative, amount at least
  1/4096, and hot transition temperature above cold transition temperature. Color transformation values must
  be finite, and color variances nonnegative. Temperature color keys require finite, nonnegative, strictly
  increasing temperatures, finite transforms, and zero variances.
- Cellular albedo scales must be finite and positive, and each sprite frame must be valid.

### Graphics properties

`MaterialGraphicsCellularStatic` and `MaterialGraphicsCellularDynamic` share the fields `sprite_sheet: Arc<SpriteSheet>`, `frame` (frame index), and `width` and `height` (repetition scales in cells per pixel). Their radiance graphics use the same graphics type in `edit_radiance`, with an independent frame and repetition pattern; `edit_radiance_intensity` scales its decoded RGB emission. The facade re-exports both graphics types as `engine::MaterialGraphicsCellularStatic` and `engine::MaterialGraphicsCellularDynamic`; they are defined in `engine_graphics::world::materials`.

`MaterialPropertiesFluid::edit_color` is packed sRGB RGBA; alpha specifies the opacity of a full cell when edits create fluid particles. `edit_radiance` supplies packed emission and occlusion for new fluid particles. `MaterialPropertiesFluid::graphics` is a `MaterialGraphicsFluid`, re-exported as `engine::MaterialGraphicsFluid`: `absorption` is the linear RGB fraction of light absorbed per cell of depth below the fluid's surface, as an exponential rate; `depth_opacity` is the rate per cell of depth at which transparency fades toward opaque; `surface_brightness` multiplies the lit color within the surface band; `surface_sheen` is the light falling on the surface, added within the band and scaled by the fluid's opacity; `surface_band` is the band's height in cells along an open surface's top cell, from 0 to 1, where zero means no band; `shimmer` is the strength of the noise varying the lit color's brightness, averaging out; `texture_scale` is the texture's feature size in cells; `texture_speed` is how fast the texture churns; `crust` is the strength of the dark crust and bright seams on the emission, averaging out. All are finite and nonnegative, `texture_scale` is positive, and `surface_band` is at most 1. `Default` gives absorption `0.0356` per channel, depth opacity `0.0`, brightness `1.08`, sheen `0.0`, band `1.0`, shimmer `0.0`, texture scale `3.0`, texture speed `1.0`, and crust `0.0`, which reproduces the earlier look. `MaterialGraphicsGas` has packed `color: u32`, material-wide radiance emission per atmosphere, and an `opacity` multiplier; color alpha is the opacity of one atmosphere before that multiplier. It also has `texture`, the strength of the noise varying the gas's density, averaging out; `texture_scale`, the texture's feature size in cells; `drift`, the texture's upward drift in cells per second; and `flicker`, the strength of the noise varying the gas's emission, averaging out. Texture and flicker are finite and nonnegative, `texture_scale` is positive, and drift is any finite value. There is no `Default`; a texture of `0.0` and flicker of `0.0` with scale `3.0` and drift `0.0` mean no texturing. It is re-exported as `engine::MaterialGraphicsGas`.

Radiance is linear HDR emission RGB plus occlusion, the fraction of light blocked by one tile of thickness. Emission is per cell along the optical path: a full-thickness cell shows its emission at the surface, while thin gas or sprites emit along the path. `pack_radiance` clamps emission channels to 0–127 and occlusion to 0–1; its packed `u32` uses three 7-bit emission mantissas in bits 0–20, a shared 4-bit exponent in bits 21–24, and 7-bit occlusion in bits 25–31.

`MaterialTemperatureColor::new(temperature)` creates an identity key; `with_color` and `with_radiance` set its two transforms. `MaterialPropertiesThermal::temperature_colors` stores these keys, which interpolate color and radiance transforms at render time. They change appearance without modifying stored color or radiance; temperature keys require finite, nonnegative, strictly increasing temperatures, finite transforms, and zero variances. With no keys, the transforms are identity. See [thermal simulation](@/internals/simulation/thermal.md) for the render-time behavior.

`damage_threshold` is the load in atmospheres borne indefinitely, for both pressure (static only)
and scripted damage (static and dynamic). `damage_coefficient` is the number of whole cells of
amount lost per second per atmosphere above the threshold. Both pressure and scripted damage spend
the cell's existing amount, which chemistry also consumes. `absorption` only damps pressure waves.
For static materials, `debris_chance` controls whether a broken cell becomes its configured debris
material; `1.0` preserves the guaranteed debris behavior.

`SpriteSheet` albedo uploads generate a complete CPU box-filtered mip chain with RGB averaged in
linear light and linear alpha. See [lighting](@/external-api/lighting.md) for sprite radiance sheets.

Fluid materials use [particle simulation](@/internals/simulation/fluids.md) and [raster-based rendering](@/internals/rendering/fluid-pass.md). Material density supplies physical mass; the pressure solver uses a shared rest spacing.

Gas species use the shared [pressure substrate](@/internals/simulation/pressure.md) in the cellular tiles and [cell-aligned rendering](@/internals/rendering/gas-pass.md).

Placing a fluid identifier directly in a cellular array does not create a particle; use edits or chunk particle records.

## Reaction properties and validation

`MaterialPropertiesChemical::tags` lists `MaterialTag` entries that reaction reactants can select by name. `MaterialTag::new(name)` creates a tag, and `with_parameter(name, value)` sets a named number on it, replacing an earlier value of that name; reactions read it as `tag.parameter`, such as `combustion.ignition`. `amount_colors` is a render-time list of `MaterialAmountColor` keys for cellular and fluid material; each key selects a color and radiance transformation at an amount per full cell or particle. Amount colors are applied before temperature colors. They do not alter stored simulation color or radiance.

`MaterialReaction` takes one or two `MaterialReactant` entries and zero to three `MaterialReactionProduct` entries. Reactants select an exact material name or tag and specify consumed amount per event; amount zero marks a catalyst. Products specify material name and amount, with optional color and radiance transforms. Builder methods set event rate, inclusive temperature and pressure ranges, heat per event, and pressure added per event. Use `with_temperature_range`, `with_pressure_range`, `with_heat`, and `with_pressure`.

Every number of a reaction, its reactant and product amounts included, is a `MaterialValue`: a constant, which an `f32` converts into, or `MaterialValue::parameter("tag.parameter")`. A parameter is read from the first reactant whose material carries that tag with that parameter, so one reaction selecting a tag can react each tagged material by that material's own values.

`MaterialRegistry::register_reaction` appends a reaction; registration order determines priority for a matching contact. Use `with_rate`, `with_temperature_range`, `with_pressure_range`, `with_heat`, and `with_pressure` to set its conditions and outputs. Compilation supports at most 64 registered reactions and 255 entries in each reaction lookup list. It reads a reaction's values for every material, or pair of materials, its reactants select, and compiles one record for each distinct set of values. It rejects unknown reactant materials (`UnknownReactantMaterial`), tags carried by no material (`UnknownTag`), parameters no reactant's material sets (`UnknownParameter`), unknown products (`UnknownProduct`), and invalid counts, amounts (consumed amounts below 1/4096), ranges, rates, pressure, or transformations (`InvalidReaction`), checking values after reading them. A reaction requires at least one consumed reactant. A single-reactant catalyst is invalid.

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
  `UnknownMaterial`, `InvalidDebris`, `InvalidTransition`, `UnknownReactantMaterial`, `UnknownTag`,
  `UnknownParameter`, `UnknownProduct`, `InvalidReaction`, capacity errors, or `Compilation(String)`.

## MaterialTable

`lookup(name)` returns the compiled identifier for an exact, case-sensitive name.
`materials()` iterates `(MaterialIdentifier, name, MaterialForm)` in identifier order.
`get` and the typed property methods return the compiled definitions, including their dependency
names. `debris(identifier)` returns the resolved dynamic debris identifier. `material_count()`
counts all forms.

The table provides `amount_color_buffer()` for render-time amount keys, along with reaction records, reaction product color transforms, and reaction lookup buffers. It also provides `cellular_color(identifier, cell, color)`, `cellular_radiance(identifier, cell, radiance)`, and GPU buffer accessors. Cellular radiance sampling uses the same frame and repetition math as albedo sampling; the explicit radiance value passes through unchanged. For cellular materials with albedo, `cellular_color` samples the frame at the cell center. The pattern
repeats from the scene origin with image rows flipped. It applies the packed sRGB tint, multiplying
RGB in linear light and alpha separately. Colors use RGBA bytes, with red in the least-significant
byte: `u32::from_le_bytes([r, g, b, a])`.

### Advanced: GPU buffers

Compilation keeps the existing GPU record layouts. It waits for cellular albedo-frame texture
readback, so it completes synchronously from the caller's perspective. See
[material table compilation](@/internals/simulation/material-table.md) for buffer layout and tint
details.

## Material surfaces

`Scene::set_material_surfaces` uploads optional art for cells where a material meets open space. Each `MaterialSurface` names a `MaterialIdentifier` and provides `top` and `under` byte slices. Each slice must contain exactly 256×256 straight-alpha sRGB RGBA8 pixels, rows from the top; invalid sizes are skipped. Alpha stores surface reach: a texel appears when the cell's closeness to open space exceeds `1 - alpha`. Closeness starts at one on a boundary and falls toward zero across nearby cells. Art repeats across the world on a cell-aligned 256-cell square, with separate layers for top and underside facing.

The scene call takes `&Accelerator` and uploads the surfaces into an array texture. See [scene rendering](@/internals/rendering/cellular-pass.md#material-surfaces) for the field and shader bindings.

{{ <rustdoc path="engine/scenes/struct.MaterialSurface.html" text="MaterialSurface" /> }}

## Rustdoc

Core: {{ <rustdoc path="engine/materials/enum.Material.html" text="Material" /> }} · {{ <rustdoc path="engine/materials/enum.MaterialForm.html" text="MaterialForm" /> }} · {{ <rustdoc path="engine/materials/struct.MaterialIdentifier.html" text="MaterialIdentifier" /> }} · {{ <rustdoc path="engine/materials/struct.MaterialRegistry.html" text="MaterialRegistry" /> }} · {{ <rustdoc path="engine/materials/struct.MaterialTable.html" text="MaterialTable" /> }} · {{ <rustdoc path="engine/materials/enum.MaterialError.html" text="MaterialError" /> }} · {{ <rustdoc path="engine/materials/struct.MaterialTransition.html" text="MaterialTransition" /> }}

Chemistry: {{ <rustdoc path="engine/materials/struct.MaterialPropertiesChemical.html" text="MaterialPropertiesChemical" /> }} · {{ <rustdoc path="engine/materials/struct.MaterialAmountColor.html" text="MaterialAmountColor" /> }} · {{ <rustdoc path="engine/materials/struct.MaterialReaction.html" text="MaterialReaction" /> }} · {{ <rustdoc path="engine/materials/enum.MaterialReactant.html" text="MaterialReactant" /> }} · {{ <rustdoc path="engine/materials/struct.MaterialReactionProduct.html" text="MaterialReactionProduct" /> }} · {{ <rustdoc path="engine/materials/struct.MaterialRegistry.html#method.register_reaction" text="register_reaction" /> }}

Properties: {{ <rustdoc path="engine/materials/struct.MaterialPropertiesCellularStatic.html" text="MaterialPropertiesCellularStatic" /> }} · {{ <rustdoc path="engine/materials/struct.MaterialPropertiesCellularDynamic.html" text="MaterialPropertiesCellularDynamic" /> }} · {{ <rustdoc path="engine/materials/struct.MaterialPropertiesFluid.html" text="MaterialPropertiesFluid" /> }} · {{ <rustdoc path="engine/materials/struct.MaterialPropertiesGas.html" text="MaterialPropertiesGas" /> }} · {{ <rustdoc path="engine/materials/struct.MaterialPropertiesThermal.html" text="MaterialPropertiesThermal" /> }} · {{ <rustdoc path="engine/materials/struct.MaterialTemperatureColor.html" text="MaterialTemperatureColor" /> }}

Graphics: {{ <rustdoc path="engine/struct.MaterialGraphicsCellularStatic.html" text="MaterialGraphicsCellularStatic" /> }} · {{ <rustdoc path="engine/struct.MaterialGraphicsCellularDynamic.html" text="MaterialGraphicsCellularDynamic" /> }} · {{ <rustdoc path="engine/struct.MaterialGraphicsFluid.html" text="MaterialGraphicsFluid" /> }} · {{ <rustdoc path="engine/struct.MaterialGraphicsGas.html" text="MaterialGraphicsGas" /> }}
