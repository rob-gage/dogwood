# Materials

Materials are registered once and then referenced by opaque
`MaterialIdentifier` values. A material's form selects the simulation model:
`CellularStatic`, `CellularDynamic`, `Fluid`, or `Gas`. The template's
`template_materials/src/materials.rs` is the best complete example and is
shared by the runnable template and tests.

## Registry Construction

Author materials with `MaterialRegistryBuilder`. Register every material first,
then assign thermal properties and tags, register reactions, and call
`compile`. Compilation validates references and returns the immutable
`MaterialRegistry` used by a scene.

```rust
use dogwood_engine::graphics::{Color, MaterialAppearance};
use dogwood_engine::physics::materials::{
    Material,
    MaterialRegistry,
    MaterialRegistryBuilder,
    MaterialThermalProperties,
};

let mut material_registry_builder: MaterialRegistryBuilder =
    MaterialRegistryBuilder::new();
let stone_material_identifier = material_registry_builder.register(
    Material::CellularStatic {
        name: String::from("Stone"),
        graphics: MaterialAppearance::from_color(Color::new_rgb(120, 120, 120)),
        mass: 1.0,
        pressure_ignore_threshold: 8.0,
        default_integrity: 20.0,
        minimum_rigid_body_cell_count: 12,
        debris_material: None,
        debris_yield_rate: 0.0,
        pressure_transmission: 0.8,
        friction: 0.8,
        restitution: 0.0,
    },
);
material_registry_builder.set_thermal(
    stone_material_identifier,
    MaterialThermalProperties {
        conductivity: 1.0,
        specific_heat_capacity: 1.0,
        default_temperature: Some(293.15),
        ..Default::default()
    },
)?;
let material_registry: MaterialRegistry = material_registry_builder.compile()?;
```

The identifier is only meaningful with the registry that produced it. Keep
identifiers from the registry rather than manufacturing or serializing raw
indices. `scene.materials()` provides read-only inspection through `get`,
`iter`, `thermal_properties`, and `tag_members`.

Material selectors used by asynchronous scene extraction are compiled against
this registry before GPU work begins. Use `MaterialFilter::Tag` or
`MaterialFilter::Form` to match a family without doing string searches in the
shader.

## Static Cellular Materials

Static cellular matter is terrain-like. `mass` affects mechanical response;
`pressure_ignore_threshold` is the load below which structural damage is
ignored; `default_integrity` is assigned to newly placed cells; and
`pressure_transmission` controls the fraction of pressure passed onward each
solver iteration. Increasing integrity or the ignore threshold makes failure
harder. Increasing transmission passes more load through a material instead
of retaining it locally.

`minimum_rigid_body_cell_count` controls whether a detached connected
component can become a rigid cellular body. `debris_material` and
`debris_yield_rate` control what failed material produces. `friction` damps
tangential cellular contact and `restitution` retains normal bounce. These
fields interact: a strong, highly transmitting material can move damage to
another support, while a low-transmission material can absorb more local load.

## Dynamic Cellular Materials

Dynamic cellular materials are granular or mobile cells. They use `mass`,
`pressure_transmission`, `friction`, and `restitution`, but do not have static
integrity or detached-body thresholds. The solver uses neighboring occupancy,
gravity, pressure, and contact state to move them between cells. Use this form
for sand, debris, and similar matter that should settle cell by cell.

## Fluids

Fluid fields configure pressure/contact and particle behavior:

- `pressure_transmission` controls how much pressure crosses fluid contact.
- `friction` damps tangential motion against solids.
- `restitution` retains normal bounce at solid contact.
- `rest_density` is the target density used by the particle volume solve.
- `artificial_pressure` adds tensile-instability correction.
- `xsph_smoothing` controls neighbor velocity smoothing.
- `body_push_speed` caps normal speed imparted by a moving body, in cells per
  second.
- `density` is the physical density used for buoyancy.
- `viscosity` controls actor drag and swimmer entrainment.

Increasing smoothing reduces noisy relative velocity but can make motion less
locally distinct. Increasing viscosity increases drag. The fluid particle
population is authoritative; its cell raster is used for contact, swimming,
and rendering.

## Gases

Gas materials are species in a shared Eulerian flow field:

- `density` is the species density relative to the implicit ambient atmosphere.
- `diffusivity` controls mixing beyond numerical advection.
- `graphics.with_occlusion(...)` controls the fraction of light blocked by one
  full cell; gas concentration scales that occlusion. The legacy gas field remains only for
  loading older material files and is not a simulation parameter.
- `dissipation` is exponential concentration decay per second; zero preserves
  the species.
- `compressibility` controls how local flow convergence changes concentration.

Gas species share velocity and temperature but retain separate concentration
fields. Reactions and phase transitions can create or consume a species.

## Appearance And Identifiers

`MaterialAppearance` supplies base color, variation, color influence, radiance,
and first-pass optical occlusion. Set occlusion with
`with_occlusion(0.0)` for transparent material, a small value for glass, and
a larger value for opaque stone. `with_radiance` supplies emitted light; lava
and fire use it to illuminate neighboring cells. Cell appearance variation is
stored separately from the identifier, so the same material can have different
visual instances. The renderer applies these properties to cellular, rigid-body
raster, fluid-coverage, and gas-concentration samples through one optical
lookup.

The registry assigns form-local identifiers. Use the returned identifier when
placing cells or products; do not assume registration order across forms.

## Thermal Properties And Phase Transitions

`conductivity` controls heat transfer between neighbors, while
`specific_heat_capacity` controls how much energy changes temperature.
`default_temperature` is used when a new representation needs an initial
temperature. Optional cold and hot transitions specify a threshold, target
material, yield, and latent energy. Transitions can cross forms, for example
fluid water to gas vapor or static ice to fluid water.

## Tags And Reactions

Tags group identifiers for reaction selectors. Exact material selectors are
best when one source is intended; tags avoid repeating a rule for every member
of a family. Reaction environmental gates can constrain temperature, pressure,
and implicit air. Reactions also define maximum extent per tick, priority,
thermal energy, and pressure output. The registry compiles these rules before
simulation starts, so invalid references fail during setup.

## Destruction, Debris, And Rigid Bodies

Static cells can fracture under pressure. Debris settings determine how much
failed material becomes a dynamic material. Remaining connected components
can become rigid bodies when they meet the material's minimum cell count.
Rigid-body cells preserve their material and thermal state while moving as one
CPU physics object; they can later split or transition through the same
material system.

For a complete authored set, follow the template's `TemplateMaterials::new`
and its `thermal`, `transition`, and reaction helpers. For engine internals,
see [Materials And Reactions](../internals/src/simulation/materials.md).
