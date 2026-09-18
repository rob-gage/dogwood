// Copyright Rob Gage 2026

//! Declarative material set shared by the template project and tests.

extern crate dogwood_engine as engine;

mod reactions;

use engine::{
    graphics::{Color, MaterialAppearance},
    physics::materials::{
        Material, MaterialIdentifier, MaterialRegistry, MaterialRegistryBuilder,
        MaterialThermalProperties, MaterialThermalTransition,
    },
};

/// The complete declarative material set used by the runnable template.
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
    /// Builds and validates the template material registry.
    pub fn new() -> Result<Self, String> {
        let mut materials = MaterialRegistryBuilder::new();

        let stone_debris_graphics = MaterialAppearance::from_color(Color::new_rgb(148, 148, 148))
            .with_extinction(5.0)
            .with_variation([0.5, 0.5, 0.5, 0.0])
            .with_color_influence([0.25, 0.25, 0.25, 0.0]);
        let stone_debris = materials.register(Material::CellularDynamic {
            name: "Stone Debris".into(),
            graphics: stone_debris_graphics,
            mass: 3.0,
            pressure_transmission: 0.55,
            friction: 0.45,
            restitution: 0.15,
        });
        let mut stone_debris_thermal = thermal(1.2, 0.88, 293.15);
        stone_debris_thermal.hot_transition = Some(transition(1473.15, 120.0));

        let sand_graphics = MaterialAppearance::from_color(Color::new_rgb(194, 178, 128))
            .with_extinction(6.0)
            .with_variation([0.5, 0.5, 0.5, 0.0])
            .with_color_influence([0.20, 0.18, 0.12, 0.0]);
        let sand = materials.register(Material::CellularDynamic {
            name: "Sand".into(),
            graphics: sand_graphics,
            mass: 1.0,
            pressure_transmission: 0.35,
            friction: 0.65,
            restitution: 0.05,
        });
        let mut sand_thermal = thermal(0.8, 0.83, 293.15);
        sand_thermal.hot_transition = Some(transition(1700.0, 80.0));

        let stone_graphics = MaterialAppearance::from_color(Color::new_rgb(108, 108, 108))
            .with_extinction(12.0)
            .with_variation([0.5, 0.5, 0.5, 0.0])
            .with_color_influence([0.25, 0.25, 0.25, 0.0]);
        let stone = materials.register(Material::CellularStatic {
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
        let mut stone_thermal = thermal(1.4, 0.88, 293.15);
        stone_thermal.hot_transition = Some(transition(1473.15, 120.0));

        let water = materials.register(Material::Fluid {
            name: "Water".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(45, 125, 210))
                .with_extinction(0.8),
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
        let mut water_thermal = thermal(0.6, 4.18, 293.15);
        water_thermal.cold_transition = Some(transition(273.15, 20.0));
        water_thermal.hot_transition = Some(transition(373.15, 50.0));

        let water_vapor = materials.register(Material::Gas {
            name: "Water Vapor".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(176, 205, 220))
                .with_extinction(0.75),
            density: 0.622,
            diffusivity: 0.8,
            extinction: 0.75,
            dissipation: 0.0,
            compressibility: 0.05,
        });
        set_thermal(
            &mut materials,
            water_vapor,
            thermal_with_cold(0.025, 2.0, 393.15, 373.15, 50.0, water),
        )?;

        let smoke = materials.register(Material::Gas {
            name: "Smoke".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(68, 72, 76))
                .with_extinction(2.5),
            density: 0.85,
            diffusivity: 0.5,
            extinction: 2.5,
            dissipation: 0.0001,
            compressibility: 0.1,
        });
        set_thermal(&mut materials, smoke, thermal(0.05, 1.1, 500.0))?;

        let fire = materials.register(Material::Gas {
            name: "Fire".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(255, 145, 24))
                .with_radiance(Color::new_rgb(255, 100, 15))
                .with_extinction(0.2),
            density: 0.12,
            diffusivity: 0.85,
            extinction: 0.2,
            dissipation: 0.002,
            compressibility: 0.08,
        });
        set_thermal(&mut materials, fire, thermal(0.04, 1.2, 1050.0))?;

        let slush = materials.register(Material::CellularDynamic {
            name: "Slush".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(130, 185, 215))
                .with_extinction(0.9),
            mass: 1.0,
            pressure_transmission: 0.4,
            friction: 0.35,
            restitution: 0.0,
        });
        set_thermal(
            &mut materials,
            slush,
            thermal_with_hot(1.6, 2.1, 268.15, 273.15, 20.0, water),
        )?;

        let ice = materials.register(Material::CellularStatic {
            name: "Ice".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(180, 220, 245))
                .with_extinction(1.2),
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
        set_thermal(
            &mut materials,
            ice,
            thermal_with_hot(2.2, 2.1, 263.15, 273.15, 20.0, water),
        )?;

        let lava = materials.register(Material::Fluid {
            name: "Lava".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(220, 70, 20))
                .with_radiance(Color::new_rgb(255, 70, 12))
                .with_extinction(0.55),
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
        set_thermal(
            &mut materials,
            lava,
            thermal_with_cold(1.0, 1.1, 1573.15, 1473.15, 120.0, stone),
        )?;

        let molten_glass = materials.register(Material::Fluid {
            name: "Molten Glass".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(245, 125, 40))
                .with_radiance(Color::new_rgb(255, 90, 20))
                .with_extinction(0.65),
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
        let mut molten_glass_thermal = thermal(0.7, 1.0, 1800.0);
        molten_glass_thermal.cold_transition = Some(transition(1400.0, 80.0));

        let broken_glass = materials.register(Material::CellularDynamic {
            name: "Broken Glass".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(170, 200, 215))
                .with_extinction(2.0),
            mass: 2.0,
            pressure_transmission: 0.45,
            friction: 0.5,
            restitution: 0.08,
        });
        set_thermal(
            &mut materials,
            broken_glass,
            thermal_with_hot(0.8, 0.84, 293.15, 1400.0, 80.0, molten_glass),
        )?;

        let glass = materials.register(Material::CellularStatic {
            name: "Glass".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(190, 215, 225))
                .with_extinction(0.18),
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
        set_thermal(
            &mut materials,
            glass,
            thermal_with_hot(0.9, 0.84, 293.15, 1400.0, 80.0, molten_glass),
        )?;

        let coal = materials.register(Material::CellularDynamic {
            name: "Coal".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(42, 35, 32))
                .with_extinction(7.0),
            mass: 1.4,
            pressure_transmission: 0.4,
            friction: 0.7,
            restitution: 0.02,
        });
        set_thermal(&mut materials, coal, thermal(0.35, 1.5, 293.15))?;

        let oil = materials.register(Material::Fluid {
            name: "Oil".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(88, 62, 24))
                .with_extinction(2.0),
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
        set_thermal(&mut materials, oil, thermal(0.12, 2.0, 293.15))?;

        let natural_gas = materials.register(Material::Gas {
            name: "Natural Gas".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(215, 188, 105))
                .with_extinction(0.15),
            density: 0.2,
            diffusivity: 0.9,
            extinction: 0.15,
            dissipation: 0.0,
            compressibility: 0.08,
        });
        set_thermal(&mut materials, natural_gas, thermal(0.08, 2.2, 293.15))?;

        let blasting_powder = materials.register(Material::CellularDynamic {
            name: "Blasting Powder".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(150, 118, 82))
                .with_extinction(5.0),
            mass: 1.1,
            pressure_transmission: 0.3,
            friction: 0.65,
            restitution: 0.02,
        });
        set_thermal(&mut materials, blasting_powder, thermal(0.25, 1.3, 293.15))?;

        let acid = materials.register(Material::Fluid {
            name: "Acid".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(85, 220, 72))
                .with_extinction(1.2),
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
        let mut acid_thermal = thermal(0.55, 3.2, 293.15);
        acid_thermal.hot_transition = Some(transition(450.0, 35.0));

        let acid_gas = materials.register(Material::Gas {
            name: "Acid Gas".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(180, 235, 92))
                .with_extinction(0.35),
            density: 0.9,
            diffusivity: 0.75,
            extinction: 0.35,
            dissipation: 0.0,
            compressibility: 0.06,
        });
        set_thermal(
            &mut materials,
            acid_gas,
            thermal_with_cold(0.03, 1.6, 500.0, 450.0, 35.0, acid),
        )?;

        let acid_sludge = materials.register(Material::Fluid {
            name: "Acid Sludge".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(112, 150, 48))
                .with_extinction(2.0),
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

        // Resolve only the forward references; their complete thermal values
        // remain beside the material registrations above.
        stone_debris_thermal.hot_transition.as_mut().unwrap().target = lava;
        sand_thermal.hot_transition.as_mut().unwrap().target = molten_glass;
        stone_thermal.hot_transition.as_mut().unwrap().target = lava;
        water_thermal.cold_transition.as_mut().unwrap().target = ice;
        water_thermal.hot_transition.as_mut().unwrap().target = water_vapor;
        molten_glass_thermal
            .cold_transition
            .as_mut()
            .unwrap()
            .target = glass;
        acid_thermal.hot_transition.as_mut().unwrap().target = acid_gas;
        for (material, properties) in [
            (stone_debris, stone_debris_thermal),
            (sand, sand_thermal),
            (stone, stone_thermal),
            (water, water_thermal),
            (molten_glass, molten_glass_thermal),
            (acid, acid_thermal),
        ] {
            set_thermal(&mut materials, material, properties)?;
        }

        reactions::register_reactions(
            &mut materials,
            fire,
            smoke,
            natural_gas,
            blasting_powder,
            acid,
            acid_gas,
            water,
            acid_sludge,
            sand,
            stone,
            stone_debris,
            oil,
            glass,
            broken_glass,
            coal,
        )?;

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

fn thermal(
    conductivity: f32,
    specific_heat_capacity: f32,
    default_temperature: f32,
) -> MaterialThermalProperties {
    MaterialThermalProperties {
        conductivity,
        specific_heat_capacity,
        default_temperature: Some(default_temperature),
        ..Default::default()
    }
}

fn transition(threshold_temperature: f32, latent_energy: f32) -> MaterialThermalTransition {
    MaterialThermalTransition {
        threshold_temperature,
        target: MaterialIdentifier::NULL,
        yield_rate: 1.0,
        latent_energy,
    }
}

fn thermal_with_hot(
    conductivity: f32,
    specific_heat_capacity: f32,
    default_temperature: f32,
    threshold: f32,
    latent_energy: f32,
    target: MaterialIdentifier,
) -> MaterialThermalProperties {
    let mut properties = thermal(conductivity, specific_heat_capacity, default_temperature);
    properties.hot_transition = Some(MaterialThermalTransition {
        target,
        ..transition(threshold, latent_energy)
    });
    properties
}

fn thermal_with_cold(
    conductivity: f32,
    specific_heat_capacity: f32,
    default_temperature: f32,
    threshold: f32,
    latent_energy: f32,
    target: MaterialIdentifier,
) -> MaterialThermalProperties {
    let mut properties = thermal(conductivity, specific_heat_capacity, default_temperature);
    properties.cold_transition = Some(MaterialThermalTransition {
        target,
        ..transition(threshold, latent_energy)
    });
    properties
}

fn set_thermal(
    materials: &mut MaterialRegistryBuilder,
    material: MaterialIdentifier,
    properties: MaterialThermalProperties,
) -> Result<(), String> {
    materials
        .set_thermal(material, properties)
        .map_err(|error| error.to_string())
}
