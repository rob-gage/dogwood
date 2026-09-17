// Copyright Rob Gage 2026

//! Declarative material set shared by the template project and tests.

extern crate dogwood_engine as engine;

mod template_materials_reactions;
mod template_materials_thermal;

use engine::{
    graphics::{Color, MaterialAppearance},
    physics::materials::{Material, MaterialIdentifier, MaterialRegistry, MaterialRegistryBuilder},
};

/// The complete declarative material set used by the runnable template.
pub struct TemplateMaterials {
    /// Registered materials and their compiled derived properties.
    pub registry: MaterialRegistry,
    /// Material identifier for stone debris.
    pub stone_debris: MaterialIdentifier,
    /// Material identifier for sand.
    pub sand: MaterialIdentifier,
    /// Material identifier for stone.
    pub stone: MaterialIdentifier,
    /// Material identifier for water.
    pub water: MaterialIdentifier,
    /// Material identifier for water vapor.
    pub water_vapor: MaterialIdentifier,
    /// Material identifier for smoke.
    pub smoke: MaterialIdentifier,
    /// Material identifier for fire.
    pub fire: MaterialIdentifier,
    /// Material identifier for slush.
    pub slush: MaterialIdentifier,
    /// Material identifier for ice.
    pub ice: MaterialIdentifier,
    /// Material identifier for lava.
    pub lava: MaterialIdentifier,
    /// Material identifier for molten glass.
    pub molten_glass: MaterialIdentifier,
    /// Material identifier for broken glass.
    pub broken_glass: MaterialIdentifier,
    /// Material identifier for glass.
    pub glass: MaterialIdentifier,
    /// Material identifier for coal.
    pub coal: MaterialIdentifier,
    /// Material identifier for oil.
    pub oil: MaterialIdentifier,
    /// Material identifier for natural gas.
    pub natural_gas: MaterialIdentifier,
    /// Material identifier for blasting powder.
    pub blasting_powder: MaterialIdentifier,
    /// Material identifier for acid.
    pub acid: MaterialIdentifier,
    /// Material identifier for acid gas.
    pub acid_gas: MaterialIdentifier,
    /// Material identifier for acid sludge.
    pub acid_sludge: MaterialIdentifier,
    /// Per-channel variation applied when rendering stone.
    pub stone_variation: [f32; 4],
    /// Per-channel variation applied when rendering sand.
    pub sand_variation: [f32; 4],
}

impl TemplateMaterials {
    /// Builds and validates the template material registry.
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
        template_materials_reactions::register_reactions(
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
        template_materials_thermal::register_thermal_properties(
            &mut materials,
            ice,
            water,
            slush,
            stone,
            stone_debris,
            lava,
            sand,
            molten_glass,
            glass,
            broken_glass,
            water_vapor,
            acid,
            acid_gas,
            fire,
            coal,
            oil,
            natural_gas,
            blasting_powder,
            smoke,
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
