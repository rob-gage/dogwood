// Copyright Rob Gage 2026

//! Declarative material set shared by the template project and tests.

extern crate dogwood_engine as engine;

use engine::{
    graphics::{Color, MaterialAppearance},
    physics::materials::{
        Material, MaterialIdentifier, MaterialReaction, MaterialReactionProduct,
        MaterialReactionReactant, MaterialReference, MaterialRegistry, MaterialRegistryBuilder,
        MaterialThermalProperties, MaterialThermalTransition,
    },
};

pub struct TemplateMaterials {
    pub registry: MaterialRegistry,
    pub stone_debris: MaterialIdentifier,
    pub sand: MaterialIdentifier,
    pub stone: MaterialIdentifier,
    pub water: MaterialIdentifier,
    pub water_vapor: MaterialIdentifier,
    pub smoke: MaterialIdentifier,
    pub fire: MaterialIdentifier,
    pub slush: MaterialIdentifier,
    pub ice: MaterialIdentifier,
    pub lava: MaterialIdentifier,
    pub molten_glass: MaterialIdentifier,
    pub broken_glass: MaterialIdentifier,
    pub glass: MaterialIdentifier,
    pub coal: MaterialIdentifier,
    pub oil: MaterialIdentifier,
    pub natural_gas: MaterialIdentifier,
    pub blasting_powder: MaterialIdentifier,
    pub acid: MaterialIdentifier,
    pub acid_gas: MaterialIdentifier,
    pub acid_sludge: MaterialIdentifier,
    pub stone_variation: [f32; 4],
    pub sand_variation: [f32; 4],
}

impl TemplateMaterials {
    pub fn new() -> Result<Self, String> {
        let mut materials = MaterialRegistryBuilder::new();
        let stone_debris_graphics: MaterialAppearance =
            MaterialAppearance::from_color(Color::new_rgb(148, 148, 148))
                .with_variation([0.5, 0.5, 0.5, 0.0])
                .with_color_influence([0.25, 0.25, 0.25, 0.0]);
        let stone_debris: MaterialIdentifier = materials.register(Material::CellularDynamic {
            name: "Stone Debris".into(),
            graphics: stone_debris_graphics,
            mass: 3.0,
            pressure_transmission: 0.55,
            friction: 0.45,
            restitution: 0.15,
        });
        let sand_graphics: MaterialAppearance =
            MaterialAppearance::from_color(Color::new_rgb(194, 178, 128))
                .with_variation([0.5, 0.5, 0.5, 0.0])
                .with_color_influence([0.20, 0.18, 0.12, 0.0]);
        let sand: MaterialIdentifier = materials.register(Material::CellularDynamic {
            name: "Sand".into(),
            graphics: sand_graphics,
            mass: 1.0,
            pressure_transmission: 0.35,
            friction: 0.65,
            restitution: 0.05,
        });
        let stone_graphics: MaterialAppearance =
            MaterialAppearance::from_color(Color::new_rgb(108, 108, 108))
                .with_variation([0.5, 0.5, 0.5, 0.0])
                .with_color_influence([0.25, 0.25, 0.25, 0.0]);
        let stone: MaterialIdentifier = materials.register(Material::CellularStatic {
            name: "Stone".into(),
            graphics: stone_graphics,
            mass: 1.0,
            pressure_ignore_threshold: 8.0,
            default_integrity: 20.0,
            minimum_rigid_body_cell_count: 12,
            debris_material: Some(stone_debris),
            debris_yield_rate: 0.8,
            pressure_transmission: 0.8,
            friction: 0.8,
            restitution: 0.02,
        });
        let water = materials.register(Material::Fluid {
            name: "Water".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(45, 125, 210)),
            pressure_transmission: 0.95,
            friction: 0.05,
            restitution: 0.0,
            rest_density: 1.0,
            artificial_pressure: 0.02,
            xsph_smoothing: 0.08,
            body_push_speed: 4.0,
            density: 1.0,
            viscosity: 2.0,
        });
        let water_vapor: MaterialIdentifier = materials.register(Material::Gas {
            name: "Water Vapor".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(176, 205, 220)),
            density: 0.622,
            diffusivity: 0.8,
            extinction: 0.75,
            dissipation: 0.0,
            compressibility: 0.05,
        });
        let smoke: MaterialIdentifier = materials.register(Material::Gas {
            name: "Smoke".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(68, 72, 76)),
            density: 0.85,
            diffusivity: 0.5,
            extinction: 2.5,
            dissipation: 0.0001,
            compressibility: 0.1,
        });
        let fire: MaterialIdentifier = materials.register(Material::Gas {
            name: "Fire".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(255, 145, 24)),
            density: 0.12,
            diffusivity: 0.85,
            extinction: 0.2,
            dissipation: 0.002,
            compressibility: 0.08,
        });
        let slush = materials.register(Material::CellularDynamic {
            name: "Slush".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(130, 185, 215)),
            mass: 1.0,
            pressure_transmission: 0.4,
            friction: 0.35,
            restitution: 0.0,
        });
        let ice = materials.register(Material::CellularStatic {
            name: "Ice".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(180, 220, 245)),
            mass: 1.0,
            pressure_ignore_threshold: 1.0,
            default_integrity: 1.0,
            minimum_rigid_body_cell_count: 32,
            debris_material: Some(slush),
            debris_yield_rate: 0.75,
            pressure_transmission: 0.5,
            friction: 0.2,
            restitution: 0.0,
        });
        let lava = materials.register(Material::Fluid {
            name: "Lava".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(220, 70, 20)),
            pressure_transmission: 0.92,
            friction: 0.18,
            restitution: 0.0,
            rest_density: 1.15,
            artificial_pressure: 0.02,
            xsph_smoothing: 0.06,
            body_push_speed: 4.0,
            density: 2.4,
            viscosity: 12.0,
        });
        let molten_glass = materials.register(Material::Fluid {
            name: "Molten Glass".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(245, 125, 40)),
            pressure_transmission: 0.9,
            friction: 0.2,
            restitution: 0.0,
            rest_density: 1.2,
            artificial_pressure: 0.02,
            xsph_smoothing: 0.05,
            body_push_speed: 3.0,
            density: 2.2,
            viscosity: 18.0,
        });
        let broken_glass = materials.register(Material::CellularDynamic {
            name: "Broken Glass".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(170, 200, 215)),
            mass: 2.0,
            pressure_transmission: 0.45,
            friction: 0.5,
            restitution: 0.08,
        });
        let glass = materials.register(Material::CellularStatic {
            name: "Glass".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(190, 215, 225)),
            mass: 2.0,
            pressure_ignore_threshold: 5.0,
            default_integrity: 12.0,
            minimum_rigid_body_cell_count: 24,
            debris_material: Some(broken_glass),
            debris_yield_rate: 0.65,
            pressure_transmission: 0.65,
            friction: 0.35,
            restitution: 0.04,
        });
        let coal = materials.register(Material::CellularDynamic {
            name: "Coal".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(42, 35, 32)),
            mass: 1.4,
            pressure_transmission: 0.4,
            friction: 0.7,
            restitution: 0.02,
        });
        let oil = materials.register(Material::Fluid {
            name: "Oil".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(88, 62, 24)),
            pressure_transmission: 0.9,
            friction: 0.08,
            restitution: 0.0,
            rest_density: 0.9,
            artificial_pressure: 0.02,
            xsph_smoothing: 0.08,
            body_push_speed: 4.0,
            density: 0.82,
            viscosity: 5.0,
        });
        let natural_gas = materials.register(Material::Gas {
            name: "Natural Gas".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(215, 188, 105)),
            density: 0.2,
            diffusivity: 0.9,
            extinction: 0.15,
            dissipation: 0.0,
            compressibility: 0.08,
        });
        let blasting_powder = materials.register(Material::CellularDynamic {
            name: "Blasting Powder".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(150, 118, 82)),
            mass: 1.1,
            pressure_transmission: 0.3,
            friction: 0.65,
            restitution: 0.02,
        });
        let acid = materials.register(Material::Fluid {
            name: "Acid".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(85, 220, 72)),
            pressure_transmission: 0.9,
            friction: 0.08,
            restitution: 0.0,
            rest_density: 1.0,
            artificial_pressure: 0.02,
            xsph_smoothing: 0.08,
            body_push_speed: 4.0,
            density: 1.05,
            viscosity: 2.5,
        });
        let acid_gas = materials.register(Material::Gas {
            name: "Acid Gas".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(180, 235, 92)),
            density: 0.9,
            diffusivity: 0.75,
            extinction: 0.35,
            dissipation: 0.0,
            compressibility: 0.06,
        });
        let acid_sludge = materials.register(Material::Fluid {
            name: "Acid Sludge".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(112, 150, 48)),
            pressure_transmission: 0.9,
            friction: 0.1,
            restitution: 0.0,
            rest_density: 1.05,
            artificial_pressure: 0.02,
            xsph_smoothing: 0.08,
            body_push_speed: 4.0,
            density: 1.15,
            viscosity: 6.0,
        });
        let exact = |material: MaterialIdentifier, amount: f32| {
            Some(MaterialReactionReactant {
                selector: MaterialReference::Material(material),
                amount,
            })
        };
        let tag = |name: &str, amount: f32| {
            Some(MaterialReactionReactant {
                selector: MaterialReference::Tag(name.into()),
                amount,
            })
        };
        let product = |material: MaterialIdentifier, amount: f32| {
            Some(MaterialReactionProduct { material, amount })
        };
        materials.register_reaction(MaterialReaction {
            reactants: [exact(fire, 0.08), tag("flammable", 1.0)],
            products: [product(fire, 0.08), product(smoke, 0.12)],
            minimum_air: Some(0.02),
            maximum_extent_per_tick: 0.08,
            thermal_energy: 18.0,
            priority: 80,
            ..Default::default()
        });
        materials.register_reaction(MaterialReaction {
            reactants: [exact(natural_gas, 1.0), None],
            products: [product(fire, 0.18), product(smoke, 0.25)],
            minimum_temperature: Some(430.0),
            minimum_air: Some(0.02),
            maximum_extent_per_tick: 0.35,
            thermal_energy: 95.0,
            pressure_output: 3.0,
            priority: 90,
            ..Default::default()
        });
        materials.register_reaction(MaterialReaction {
            reactants: [exact(blasting_powder, 1.0), None],
            products: [product(fire, 0.2), product(smoke, 0.2)],
            minimum_temperature: Some(420.0),
            maximum_extent_per_tick: 0.8,
            thermal_energy: 160.0,
            pressure_output: 80.0,
            priority: 100,
            ..Default::default()
        });
        materials.register_reaction(MaterialReaction {
            reactants: [exact(blasting_powder, 1.0), None],
            products: [product(fire, 0.2), product(smoke, 0.2)],
            minimum_pressure: Some(12.0),
            maximum_extent_per_tick: 0.8,
            thermal_energy: 180.0,
            pressure_output: 100.0,
            priority: 101,
            ..Default::default()
        });
        materials.register_reaction(MaterialReaction {
            reactants: [exact(acid, 0.2), tag("corrodable", 1.0)],
            products: [None, None],
            maximum_extent_per_tick: 0.15,
            thermal_energy: 0.01,
            priority: 20,
            ..Default::default()
        });
        materials.register_reaction(MaterialReaction {
            reactants: [exact(acid_gas, 0.2), tag("corrodable", 1.0)],
            products: [None, None],
            maximum_extent_per_tick: 0.008,
            thermal_energy: 0.002,
            priority: 20,
            ..Default::default()
        });
        materials.register_reaction(MaterialReaction {
            reactants: [exact(acid, 1.0), exact(water, 1.0)],
            products: [product(acid_sludge, 1.0), None],
            maximum_extent_per_tick: 0.12,
            priority: 30,
            ..Default::default()
        });
        materials.register_reaction(MaterialReaction {
            reactants: [exact(sand, 1.0), exact(acid_sludge, 1.0)],
            products: [product(blasting_powder, 1.0), None],
            minimum_temperature: Some(500.0),
            maximum_temperature: Some(1500.0),
            maximum_extent_per_tick: 1.0,
            priority: 30,
            ..Default::default()
        });
        for (material, name) in [
            (coal, "flammable"),
            (oil, "flammable"),
            (natural_gas, "flammable"),
            (blasting_powder, "flammable"),
            (stone, "corrodable"),
            (stone_debris, "corrodable"),
            (sand, "corrodable"),
            (glass, "corrodable"),
            (broken_glass, "corrodable"),
            (coal, "corrodable"),
            (blasting_powder, "corrodable"),
        ] {
            materials
                .tag(material, name)
                .map_err(|error| error.to_string())?;
        }
        // All identifiers now exist, so transition metadata can be compiled
        // without relying on registration order.
        materials
            .set_thermal(
                ice,
                MaterialThermalProperties {
                    conductivity: 2.2,
                    specific_heat_capacity: 2.1,
                    default_temperature: Some(263.15),
                    cold_transition: None,
                    hot_transition: Some(MaterialThermalTransition {
                        threshold_temperature: 273.15,
                        target: water,
                        yield_rate: 1.0,
                        latent_energy: 20.0,
                    }),
                },
            )
            .map_err(|error| error.to_string())?;
        materials
            .set_thermal(
                slush,
                MaterialThermalProperties {
                    conductivity: 1.6,
                    specific_heat_capacity: 2.1,
                    default_temperature: Some(268.15),
                    cold_transition: None,
                    hot_transition: Some(MaterialThermalTransition {
                        threshold_temperature: 273.15,
                        target: water,
                        yield_rate: 1.0,
                        latent_energy: 20.0,
                    }),
                },
            )
            .map_err(|error| error.to_string())?;
        materials
            .set_thermal(
                stone,
                MaterialThermalProperties {
                    conductivity: 1.4,
                    specific_heat_capacity: 0.88,
                    default_temperature: Some(293.15),
                    cold_transition: None,
                    hot_transition: Some(MaterialThermalTransition {
                        threshold_temperature: 1473.15,
                        target: lava,
                        yield_rate: 1.0,
                        latent_energy: 120.0,
                    }),
                },
            )
            .map_err(|error| error.to_string())?;
        materials
            .set_thermal(
                stone_debris,
                MaterialThermalProperties {
                    conductivity: 1.2,
                    specific_heat_capacity: 0.88,
                    default_temperature: Some(293.15),
                    cold_transition: None,
                    hot_transition: Some(MaterialThermalTransition {
                        threshold_temperature: 1473.15,
                        target: lava,
                        yield_rate: 1.0,
                        latent_energy: 120.0,
                    }),
                },
            )
            .map_err(|error| error.to_string())?;
        materials
            .set_thermal(
                lava,
                MaterialThermalProperties {
                    conductivity: 1.0,
                    specific_heat_capacity: 1.1,
                    default_temperature: Some(1573.15),
                    cold_transition: Some(MaterialThermalTransition {
                        threshold_temperature: 1473.15,
                        target: stone,
                        yield_rate: 1.0,
                        latent_energy: 120.0,
                    }),
                    hot_transition: None,
                },
            )
            .map_err(|error| error.to_string())?;
        materials
            .set_thermal(
                sand,
                MaterialThermalProperties {
                    conductivity: 0.8,
                    specific_heat_capacity: 0.83,
                    default_temperature: Some(293.15),
                    cold_transition: None,
                    hot_transition: Some(MaterialThermalTransition {
                        threshold_temperature: 1700.0,
                        target: molten_glass,
                        yield_rate: 1.0,
                        latent_energy: 80.0,
                    }),
                },
            )
            .map_err(|error| error.to_string())?;
        materials
            .set_thermal(
                molten_glass,
                MaterialThermalProperties {
                    conductivity: 0.7,
                    specific_heat_capacity: 1.0,
                    default_temperature: Some(1800.0),
                    cold_transition: Some(MaterialThermalTransition {
                        threshold_temperature: 1400.0,
                        target: glass,
                        yield_rate: 1.0,
                        latent_energy: 80.0,
                    }),
                    hot_transition: None,
                },
            )
            .map_err(|error| error.to_string())?;
        materials
            .set_thermal(
                glass,
                MaterialThermalProperties {
                    conductivity: 0.9,
                    specific_heat_capacity: 0.84,
                    default_temperature: Some(293.15),
                    cold_transition: None,
                    hot_transition: Some(MaterialThermalTransition {
                        threshold_temperature: 1400.0,
                        target: molten_glass,
                        yield_rate: 1.0,
                        latent_energy: 80.0,
                    }),
                },
            )
            .map_err(|error| error.to_string())?;
        materials
            .set_thermal(
                broken_glass,
                MaterialThermalProperties {
                    conductivity: 0.8,
                    specific_heat_capacity: 0.84,
                    default_temperature: Some(293.15),
                    cold_transition: None,
                    hot_transition: Some(MaterialThermalTransition {
                        threshold_temperature: 1400.0,
                        target: molten_glass,
                        yield_rate: 1.0,
                        latent_energy: 80.0,
                    }),
                },
            )
            .map_err(|error| error.to_string())?;
        materials
            .set_thermal(
                water,
                MaterialThermalProperties {
                    conductivity: 0.6,
                    specific_heat_capacity: 4.18,
                    default_temperature: Some(293.15),
                    cold_transition: Some(MaterialThermalTransition {
                        threshold_temperature: 273.15,
                        target: ice,
                        yield_rate: 1.0,
                        latent_energy: 20.0,
                    }),
                    hot_transition: Some(MaterialThermalTransition {
                        threshold_temperature: 373.15,
                        target: water_vapor,
                        yield_rate: 1.0,
                        latent_energy: 50.0,
                    }),
                },
            )
            .map_err(|error| error.to_string())?;
        materials
            .set_thermal(
                water_vapor,
                MaterialThermalProperties {
                    conductivity: 0.025,
                    specific_heat_capacity: 2.0,
                    default_temperature: Some(393.15),
                    cold_transition: Some(MaterialThermalTransition {
                        threshold_temperature: 373.15,
                        target: water,
                        yield_rate: 1.0,
                        latent_energy: 50.0,
                    }),
                    hot_transition: None,
                },
            )
            .map_err(|error| error.to_string())?;
        materials
            .set_thermal(
                acid,
                MaterialThermalProperties {
                    conductivity: 0.55,
                    specific_heat_capacity: 3.2,
                    default_temperature: Some(293.15),
                    cold_transition: None,
                    hot_transition: Some(MaterialThermalTransition {
                        threshold_temperature: 450.0,
                        target: acid_gas,
                        yield_rate: 1.0,
                        latent_energy: 35.0,
                    }),
                },
            )
            .map_err(|error| error.to_string())?;
        materials
            .set_thermal(
                acid_gas,
                MaterialThermalProperties {
                    conductivity: 0.03,
                    specific_heat_capacity: 1.6,
                    default_temperature: Some(500.0),
                    cold_transition: Some(MaterialThermalTransition {
                        threshold_temperature: 450.0,
                        target: acid,
                        yield_rate: 1.0,
                        latent_energy: 35.0,
                    }),
                    hot_transition: None,
                },
            )
            .map_err(|error| error.to_string())?;
        materials
            .set_thermal(
                fire,
                MaterialThermalProperties {
                    conductivity: 0.04,
                    specific_heat_capacity: 1.2,
                    default_temperature: Some(1050.0),
                    cold_transition: None,
                    hot_transition: None,
                },
            )
            .map_err(|error| error.to_string())?;
        for (material, conductivity, specific_heat_capacity, default_temperature) in [
            (coal, 0.35, 1.5, 293.15),
            (oil, 0.12, 2.0, 293.15),
            (natural_gas, 0.08, 2.2, 293.15),
            (blasting_powder, 0.25, 1.3, 293.15),
            (smoke, 0.05, 1.1, 500.0),
        ] {
            materials
                .set_thermal(
                    material,
                    MaterialThermalProperties {
                        conductivity,
                        specific_heat_capacity,
                        default_temperature: Some(default_temperature),
                        cold_transition: None,
                        hot_transition: None,
                    },
                )
                .map_err(|error| error.to_string())?;
        }

        Ok(Self {
            registry: materials.compile().map_err(|error| error.to_string())?,
            stone_debris,
            sand,
            stone,
            water,
            water_vapor,
            smoke,
            fire,
            slush,
            ice,
            lava,
            molten_glass,
            broken_glass,
            glass,
            coal,
            oil,
            natural_gas,
            blasting_powder,
            acid,
            acid_gas,
            acid_sludge,
            stone_variation: stone_graphics.variation(),
            sand_variation: sand_graphics.variation(),
        })
    }
}
