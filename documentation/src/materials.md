# Materials

Materials are registered once, then referenced by opaque `MaterialIdentifier`s. The four forms are `CellularStatic`, `CellularDynamic`, `Fluid`, and `Gas`.

```rust
let mut builder = MaterialRegistryBuilder::new();
let stone = builder.register(Material::CellularStatic {
    name: "Stone".into(), graphics: MaterialAppearance::from_color(Color::new_rgb(120, 120, 120)),
    mass: 1.0, pressure_ignore_threshold: 0.1, default_integrity: 20.0,
    minimum_rigid_body_cell_count: 12, debris_material: None, debris_yield_rate: 0.0,
    pressure_transmission: 0.2, friction: 0.8, restitution: 0.0,
});
builder.set_thermal(stone, MaterialThermalProperties { conductivity: 1.0,
    specific_heat_capacity: 1.0, default_temperature: Some(293.15),
    cold_transition: None, hot_transition: None })?;
let registry = builder.compile()?;
```

Thermal metadata controls conductivity, heat capacity, initial temperature, and optional cold/hot transitions. Reactions and tags are also compiled by `MaterialRegistryBuilder`. The scene exposes the read-only registry as `scene.materials()`; use `get`, `iter`, `thermal_properties`, and `tag_members` for gameplay inspection.

Graphics are part of each material through `graphics::MaterialAppearance`; this is the rendering-facing hook game code configures.
