// Copyright Rob Gage 2026

use crate::template_scene_generator::TemplateSceneGenerator;
use engine::{
    actors::{
        components::{
            ActorCameraComponent,
            ActorCollisionComponent,
            ActorKinematicsPositionComponent,
            ActorParallaxComponent,
            ParallaxRepeat,
            ActorPawnMovementComponent,
            ActorPawnPossessionComponent,
            ActorPersistenceComponent,
            ActorSpritesComponent,
        },
        Actor,
    },
    games::Game,
    materials::{
        MaterialAmountColor,
        MaterialIdentifier,
        MaterialPropertiesCellularDynamic,
        MaterialPropertiesCellularStatic,
        MaterialPropertiesChemical,
        MaterialPropertiesFluid,
        MaterialPropertiesGas,
        MaterialPropertiesThermal,
        MaterialReactant,
        MaterialReaction,
        MaterialReactionProduct,
        MaterialRegistry,
        MaterialTable,
        MaterialTag,
        MaterialTemperatureColor,
        MaterialTransition,
        MaterialValue,
    },
    scenes::{
        cellular_tiles::CELLULAR_TILE_SIZE,
        chunks::ChunkData,
        collisions::{
            Collider,
            ColliderShape,
            CollisionBody,
            COLLISION_GROUP_ACTOR,
            COLLISION_GROUP_CELLULAR_DYNAMIC,
            COLLISION_GROUP_EDIT,
        },
        Scene,
        ScenePosition,
    },
    runtime::EngineError,
    system_events::{
        KeyCode,
        PhysicalKey,
        SystemKeyboardEvent,
    },
    Accelerator,
    MaterialColorTransformation,
    MaterialGraphicsCellularDynamic,
    MaterialGraphicsCellularStatic,
    MaterialGraphicsGas,
    SpriteSheet,
    pack_radiance,
};
use std::{
    path::PathBuf,
    sync::Arc,
};

/// Flat stone world with gravel to place, explored by a white square player
pub struct TemplateGame {
    /// Scene holding the world and player
    scene: Scene,
    /// Keyboard-controlled pawn whose camera displays the scene
    player: Actor,
}

impl TemplateGame {

    /// Creates the stone world, its gravel, and a player falling onto it from above
    pub fn new(accelerator: &Accelerator) -> Result<Self, EngineError> {
        let mut material_registry: MaterialRegistry = MaterialRegistry::new();
        // colors carried through transitions, as linear red, green, blue, and alpha
        let water_color: [f32; 4] = [0.021, 0.147, 0.578, 0.78];
        let lava_color: [f32; 4] = [1.0, 0.127, 0.007, 1.0];
        let ice_color: [f32; 4] = [0.491, 0.716, 0.888, 1.0];
        // radiances as linear emission and occlusion per tile
        let stone_radiance: [f32; 4] = [0.0, 0.0, 0.0, 0.9];
        let water_radiance: [f32; 4] = [0.0, 0.0, 0.0, 0.1];
        let lava_radiance: [f32; 4] = [1.2, 0.35, 0.05, 0.95];
        let ice_radiance: [f32; 4] = [0.0, 0.0, 0.0, 0.15];
        let acid_color: [f32; 4] = [0.153, 0.716, 0.032, 0.82];
        let acid_radiance: [f32; 4] = [0.0, 0.15, 0.0, 0.02];
        let sludge_color: [f32; 4] = [0.114, 0.144, 0.021, 1.0];
        // a transformation keeping a fraction of the carried color and adding the rest of another
        let blend = |kept: f32, color: [f32; 4], channel_variance: f32|
            -> MaterialColorTransformation {
            MaterialColorTransformation {
                matrix: [
                    [kept, 0.0, 0.0, 0.0],
                    [0.0, kept, 0.0, 0.0],
                    [0.0, 0.0, kept, 0.0],
                    [0.0, 0.0, 0.0, kept],
                ],
                offset: color.map(|channel| channel * (1.0 - kept)),
                intensity_variance: 0.0,
                channel_variance: [channel_variance, channel_variance, channel_variance, 0.0],
            }
        };
        // a transformation replacing the carried radiance with a product's
        let radiance = |product_radiance: [f32; 4]| -> MaterialColorTransformation {
            MaterialColorTransformation {
                matrix: [[0.0; 4]; 4],
                offset: product_radiance,
                intensity_variance: 0.0,
                channel_variance: [0.0; 4],
            }
        };
        let melting = |product: &str| -> Option<MaterialTransition> {
            Some(MaterialTransition::new(1400.0, 300.0, product)
                .with_color(blend(0.2, lava_color, 0.0))
                .with_radiance(radiance(lava_radiance)))
        };
        let thawing = |product: &str| -> Option<MaterialTransition> {
            Some(MaterialTransition::new(273.0, 330.0, product)
                .with_color(blend(0.0, water_color, 0.0))
                .with_radiance(radiance(water_radiance)))
        };
        // rock darkens to a red glow while hot, fading as it cools
        let glowing = || -> Vec<MaterialTemperatureColor> {
            vec![
                MaterialTemperatureColor::new(800.0),
                MaterialTemperatureColor::new(1300.0)
                    .with_color(MaterialColorTransformation {
                        matrix: [
                            [0.25, 0.0, 0.0, 0.0],
                            [0.0, 0.25, 0.0, 0.0],
                            [0.0, 0.0, 0.25, 0.0],
                            [0.0, 0.0, 0.0, 1.0],
                        ],
                        offset: [0.18, 0.035, 0.006, 0.0],
                        intensity_variance: 0.0,
                        channel_variance: [0.0; 4],
                    })
                    .with_radiance(MaterialColorTransformation {
                        offset: [1.0, 0.25, 0.03, 0.0],
                        ..MaterialColorTransformation::IDENTITY
                    }),
            ]
        };
        // wood darkens as it chars and glows orange as it burns
        let wood_temperature_colors = || -> Vec<MaterialTemperatureColor> {
            vec![
                MaterialTemperatureColor::new(550.0),
                MaterialTemperatureColor::new(700.0)
                    .with_color(MaterialColorTransformation {
                        matrix: [
                            [0.28, 0.0, 0.0, 0.0],
                            [0.0, 0.08, 0.0, 0.0],
                            [0.0, 0.0, 0.04, 0.0],
                            [0.0, 0.0, 0.0, 1.0],
                        ],
                        offset: [0.12, 0.012, 0.002, 0.0],
                        intensity_variance: 0.0,
                        channel_variance: [0.0; 4],
                    })
                    .with_radiance(radiance([0.18, 0.025, 0.002, 0.0])),
                MaterialTemperatureColor::new(1000.0)
                    .with_color(MaterialColorTransformation {
                        matrix: [[0.35, 0.0, 0.0, 0.0], [0.0, 0.18, 0.0, 0.0],
                            [0.0, 0.0, 0.04, 0.0], [0.0, 0.0, 0.0, 1.0]],
                        offset: [0.35, 0.12, 0.008, 0.0],
                        intensity_variance: 0.0,
                        channel_variance: [0.0; 4],
                    })
                    .with_radiance(radiance([1.3, 0.32, 0.025, 0.0])),
            ]
        };
        // wood blackens as it burns away, with the same keys on its debris
        let charring = |kept: f32| -> MaterialColorTransformation {
            MaterialColorTransformation {
                matrix: [
                    [kept, 0.0, 0.0, 0.0],
                    [0.0, kept, 0.0, 0.0],
                    [0.0, 0.0, kept, 0.0],
                    [0.0, 0.0, 0.0, 1.0],
                ],
                ..MaterialColorTransformation::IDENTITY
            }
        };
        let wood_amount_colors = || -> Vec<MaterialAmountColor> {
            vec![
                MaterialAmountColor::new(0.15).with_color(charring(0.08)),
                MaterialAmountColor::new(0.5).with_color(charring(0.44)),
                MaterialAmountColor::new(1.0),
            ]
        };
        // cellular dynamic debris that piles at its angle of repose and that players wade through
        material_registry.register_cellular_dynamic("gravel", MaterialPropertiesCellularDynamic {
            edit_color: MaterialGraphicsCellularDynamic {
                sprite_sheet: Arc::new(SpriteSheet::from_png(
                    accelerator,
                    include_bytes!("../assets/sprites/gravel.png"),
                    1,
                    1,
                )?),
                frame: 0,
                width: 1.0,
                height: 1.0,
            },
            edit_radiance: MaterialGraphicsCellularDynamic {
                sprite_sheet: Arc::new(SpriteSheet::from_color(accelerator, 0, 0, 0, 217)?),
                frame: 0,
                width: 1.0,
                height: 1.0,
            },
            edit_radiance_intensity: 1.0,
            mass: 1.0,
            friction: 0.8,
            restitution: 0.1,
            thermal: MaterialPropertiesThermal {
                heat_capacity: 1.0,
                conduction: 0.3,
                edit_temperature: 293.0,
                hot_transition: melting("lava"),
                temperature_colors: glowing(),
                ..Default::default()
            },
            chemical: MaterialPropertiesChemical {
                tags: vec![MaterialTag::new("corrodible")], amount_colors: Vec::new(),
            },
        })?;
        // loose ice that breaks off frozen water and melts back into it
        material_registry.register_cellular_dynamic("snow", MaterialPropertiesCellularDynamic {
            edit_color: MaterialGraphicsCellularDynamic {
                sprite_sheet: Arc::new(SpriteSheet::from_png(
                    accelerator,
                    include_bytes!("../assets/sprites/snow.png"),
                    1,
                    1,
                )?),
                frame: 0,
                width: 1.0,
                height: 1.0,
            },
            edit_radiance: MaterialGraphicsCellularDynamic {
                sprite_sheet: Arc::new(SpriteSheet::from_color(accelerator, 0, 0, 0, 128)?),
                frame: 0,
                width: 1.0,
                height: 1.0,
            },
            edit_radiance_intensity: 1.0,
            mass: 0.5,
            friction: 0.9,
            restitution: 0.0,
            thermal: MaterialPropertiesThermal {
                heat_capacity: 2.0,
                conduction: 0.2,
                edit_temperature: 263.0,
                hot_transition: thawing("water"),
                ..Default::default()
            },
            chemical: MaterialPropertiesChemical::default(),
        })?;
        material_registry.register_cellular_static("stone", MaterialPropertiesCellularStatic {
            edit_color: MaterialGraphicsCellularStatic {
                sprite_sheet: Arc::new(SpriteSheet::from_png(
                    accelerator,
                    include_bytes!("../assets/sprites/stone.png"),
                    1,
                    1,
                )?),
                frame: 0,
                width: 1.0,
                height: 1.0,
            },
            edit_radiance: MaterialGraphicsCellularStatic {
                sprite_sheet: Arc::new(SpriteSheet::from_color(accelerator, 0, 0, 0, 230)?),
                frame: 0,
                width: 1.0,
                height: 1.0,
            },
            edit_radiance_intensity: 1.0,
            mass: 2.0,
            friction: 0.6,
            restitution: 0.1,
            strength: 10.0,
            absorption: 0.1,
            debris: Some(String::from("gravel")),
            thermal: MaterialPropertiesThermal {
                heat_capacity: 1.0,
                conduction: 0.5,
                edit_temperature: 293.0,
                hot_transition: melting("lava"),
                temperature_colors: glowing(),
                ..Default::default()
            },
            chemical: MaterialPropertiesChemical {
                tags: vec![MaterialTag::new("corrodible")], amount_colors: Vec::new(),
            },
        })?;
        material_registry.register_cellular_static("ice", MaterialPropertiesCellularStatic {
            edit_color: MaterialGraphicsCellularStatic {
                sprite_sheet: Arc::new(SpriteSheet::from_png(
                    accelerator,
                    include_bytes!("../assets/sprites/ice.png"),
                    1,
                    1,
                )?),
                frame: 0,
                width: 1.0,
                height: 1.0,
            },
            edit_radiance: MaterialGraphicsCellularStatic {
                sprite_sheet: Arc::new(SpriteSheet::from_color(accelerator, 0, 0, 0, 38)?),
                frame: 0,
                width: 1.0,
                height: 1.0,
            },
            edit_radiance_intensity: 1.0,
            mass: 0.9,
            friction: 0.05,
            restitution: 0.1,
            strength: 3.0,
            absorption: 0.2,
            debris: Some(String::from("snow")),
            thermal: MaterialPropertiesThermal {
                heat_capacity: 2.0,
                conduction: 1.0,
                edit_temperature: 263.0,
                hot_transition: thawing("water"),
                ..Default::default()
            },
            chemical: MaterialPropertiesChemical::default(),
        })?;
        material_registry.register_fluid("water", MaterialPropertiesFluid {
            density: 0.6,
            viscosity: 0.05,
            cohesion: 0.2,
            artificial_pressure: 0.001,
            drag: 1.0,
            friction: 0.0,
            restitution: 0.0,
            edit_color: u32::from_le_bytes([40, 110, 200, 200]),
            edit_radiance: pack_radiance(water_radiance),
            thermal: MaterialPropertiesThermal {
                heat_capacity: 4.0,
                conduction: 0.6,
                edit_temperature: 293.0,
                // each particle boils into an atmosphere of vapor in its cell
                hot_transition: Some(MaterialTransition::new(373.0, 1000.0, "water vapor")),
                cold_transition: Some(
                    MaterialTransition::new(273.0, 330.0, "ice")
                        .with_color(blend(0.1, ice_color, 0.03))
                        .with_radiance(radiance(ice_radiance)),
                ),
                temperature_colors: Vec::new(),
            },
            chemical: MaterialPropertiesChemical::default(),
        })?;
        // molten rock that cools into stone, or into gravel where too little of it freezes
        // together to stay put
        material_registry.register_fluid("lava", MaterialPropertiesFluid {
            density: 1.5,
            viscosity: 0.95,
            cohesion: 1.0,
            artificial_pressure: 0.001,
            drag: 12.0,
            friction: 0.8,
            restitution: 0.0,
            edit_color: u32::from_le_bytes([255, 100, 20, 255]),
            edit_radiance: pack_radiance(lava_radiance),
            thermal: MaterialPropertiesThermal {
                heat_capacity: 1.0,
                conduction: 0.4,
                edit_temperature: 1600.0,
                cold_transition: Some(
                    MaterialTransition::new(1300.0, 300.0, "stone")
                        .with_color(MaterialColorTransformation {
                            matrix: [
                                [0.03, 0.03, 0.03, 0.0],
                                [0.03, 0.03, 0.03, 0.0],
                                [0.03, 0.03, 0.03, 0.0],
                                [0.0, 0.0, 0.0, 1.0],
                            ],
                            offset: [0.06, 0.055, 0.05, 0.0],
                            intensity_variance: 0.2,
                            channel_variance: [0.0; 4],
                        })
                        .with_radiance(radiance(stone_radiance)),
                ),
                ..Default::default()
            },
            chemical: MaterialPropertiesChemical::default(),
        })?;
        material_registry.register_gas("water vapor", MaterialPropertiesGas {
            mass: 18.0,
            diffusivity: 0.04,
            cohesion: 0.1,
            graphics: MaterialGraphicsGas {
                color: u32::from_le_bytes([225, 230, 240, 150]),
                radiance: pack_radiance([0.0, 0.0, 0.0, 0.02]),
                opacity: 1.0,
            },
            thermal: MaterialPropertiesThermal {
                heat_capacity: 1.0,
                conduction: 0.05,
                edit_temperature: 400.0,
                // an atmosphere of vapor in one cell condenses into one particle
                cold_transition: Some(
                    MaterialTransition::new(373.0, 1000.0, "water")
                        .with_color(blend(0.0, water_color, 0.0))
                        .with_radiance(radiance(water_radiance)),
                ),
                ..Default::default()
            },
            chemical: MaterialPropertiesChemical::default(),
        })?;
        // static wood burns into chips when broken
        material_registry.register_cellular_static("wood", MaterialPropertiesCellularStatic {
            edit_color: MaterialGraphicsCellularStatic {
                sprite_sheet: Arc::new(SpriteSheet::from_png(
                    accelerator,
                    include_bytes!("../assets/sprites/wood.png"),
                    1,
                    1,
                )?),
                frame: 0,
                width: 1.0,
                height: 1.0,
            },
            edit_radiance: MaterialGraphicsCellularStatic {
                sprite_sheet: Arc::new(SpriteSheet::from_color(accelerator, 0, 0, 0, 230)?),
                frame: 0,
                width: 1.0,
                height: 1.0,
            },
            edit_radiance_intensity: 1.0,
            mass: 0.7,
            friction: 0.7,
            restitution: 0.1,
            strength: 6.0,
            absorption: 0.3,
            debris: Some(String::from("wood chips")),
            thermal: MaterialPropertiesThermal {
                heat_capacity: 1.5,
                conduction: 0.5,
                edit_temperature: 293.0,
                temperature_colors: wood_temperature_colors(),
                ..Default::default()
            },
            chemical: MaterialPropertiesChemical {
                tags: vec![
                    MaterialTag::new("corrodible"),
                    MaterialTag::new("combustion")
                        .with_parameter("ignition", 500.0)
                        .with_parameter("flame", 1300.0)
                        .with_parameter("rate", 2.0)
                        .with_parameter("amount", 0.02)
                        .with_parameter("fire", 0.3)
                        .with_parameter("smoke", 0.1)
                        .with_parameter("heat", 150.0)
                        .with_parameter("pressure", 0.0),
                ],
                amount_colors: wood_amount_colors(),
            },
        })?;
        // loose wood flakes that pile at their angle of repose
        material_registry.register_cellular_dynamic(
            "wood chips",
            MaterialPropertiesCellularDynamic {
                edit_color: MaterialGraphicsCellularDynamic {
                    sprite_sheet: Arc::new(SpriteSheet::from_png(
                        accelerator,
                        include_bytes!("../assets/sprites/wood_chips.png"),
                        1,
                        1,
                    )?),
                    frame: 0,
                    width: 1.0,
                    height: 1.0,
                },
                edit_radiance: MaterialGraphicsCellularDynamic {
                    sprite_sheet: Arc::new(SpriteSheet::from_color(accelerator, 0, 0, 0, 230)?),
                    frame: 0,
                    width: 1.0,
                    height: 1.0,
                },
                edit_radiance_intensity: 1.0,
                mass: 0.4,
                friction: 0.8,
                restitution: 0.0,
                thermal: MaterialPropertiesThermal {
                    heat_capacity: 1.5,
                    conduction: 0.5,
                    edit_temperature: 293.0,
                    temperature_colors: wood_temperature_colors(),
                    ..Default::default()
                },
                chemical: MaterialPropertiesChemical {
                    tags: vec![
                        MaterialTag::new("corrodible"),
                        MaterialTag::new("combustion")
                            .with_parameter("ignition", 450.0)
                            .with_parameter("flame", 1300.0)
                            .with_parameter("rate", 4.0)
                            .with_parameter("amount", 0.02)
                            .with_parameter("fire", 0.3)
                            .with_parameter("smoke", 0.1)
                            .with_parameter("heat", 150.0)
                            .with_parameter("pressure", 0.0),
                    ],
                    amount_colors: wood_amount_colors(),
                },
            },
        )?;
        // dark grains that flash into hot fire and a burst of pressure
        material_registry.register_cellular_dynamic(
            "gunpowder",
            MaterialPropertiesCellularDynamic {
                edit_color: MaterialGraphicsCellularDynamic {
                    sprite_sheet: Arc::new(SpriteSheet::from_color(accelerator, 45, 42, 40, 255)?),
                    frame: 0,
                    width: 1.0,
                    height: 1.0,
                },
                edit_radiance: MaterialGraphicsCellularDynamic {
                    sprite_sheet: Arc::new(SpriteSheet::from_color(accelerator, 0, 0, 0, 240)?),
                    frame: 0,
                    width: 1.0,
                    height: 1.0,
                },
                edit_radiance_intensity: 1.0,
                mass: 0.9,
                friction: 0.7,
                restitution: 0.0,
                thermal: MaterialPropertiesThermal {
                    heat_capacity: 1.0,
                    conduction: 4.0,
                    ..Default::default()
                },
                chemical: MaterialPropertiesChemical {
                    tags: vec![
                        MaterialTag::new("combustion")
                            .with_parameter("ignition", 400.0)
                            .with_parameter("flame", f32::INFINITY)
                            .with_parameter("rate", 60.0)
                            .with_parameter("amount", 1.0)
                            .with_parameter("fire", 1.0)
                            .with_parameter("smoke", 0.5)
                            .with_parameter("heat", 4000.0)
                            .with_parameter("pressure", 40.0),
                    ],
                    amount_colors: Vec::new(),
                },
            },
        )?;
        // bright green liquid acid that evaporates into a heavier gas
        material_registry.register_fluid("acid", MaterialPropertiesFluid {
            density: 0.65,
            viscosity: 0.03,
            cohesion: 0.2,
            artificial_pressure: 0.001,
            drag: 1.0,
            friction: 0.0,
            restitution: 0.0,
            edit_color: u32::from_le_bytes([110, 220, 50, 210]),
            edit_radiance: pack_radiance(acid_radiance),
            thermal: MaterialPropertiesThermal {
                heat_capacity: 3.0,
                conduction: 0.5,
                edit_temperature: 293.0,
                hot_transition: Some(MaterialTransition::new(390.0, 600.0, "acid gas")),
                ..Default::default()
            },
            chemical: MaterialPropertiesChemical {
                tags: vec![
                    MaterialTag::new("combustion")
                        .with_parameter("ignition", 380.0)
                        .with_parameter("flame", f32::INFINITY)
                        .with_parameter("rate", 1.5)
                        .with_parameter("amount", 0.04)
                        .with_parameter("fire", 0.3)
                        .with_parameter("smoke", 0.03)
                        .with_parameter("heat", 500.0)
                        .with_parameter("pressure", 0.0),
                ],
                amount_colors: Vec::new(),
            },
        })?;
        // dense green vapor that condenses back into acid
        material_registry.register_gas("acid gas", MaterialPropertiesGas {
            mass: 40.0,
            diffusivity: 0.15,
            cohesion: 0.0,
            graphics: MaterialGraphicsGas {
                color: u32::from_le_bytes([170, 230, 60, 90]),
                radiance: pack_radiance([0.0, 0.12, 0.0, 0.01]),
                opacity: 1.0,
            },
            thermal: MaterialPropertiesThermal {
                heat_capacity: 1.0,
                conduction: 0.05,
                edit_temperature: 400.0,
                cold_transition: Some(
                    MaterialTransition::new(385.0, 600.0, "acid")
                        .with_color(blend(0.0, acid_color, 0.0))
                        .with_radiance(radiance(acid_radiance)),
                ),
                ..Default::default()
            },
            chemical: MaterialPropertiesChemical {
                tags: vec![
                    MaterialTag::new("combustion")
                        .with_parameter("ignition", 500.0)
                        .with_parameter("flame", f32::INFINITY)
                        .with_parameter("rate", 15.0)
                        .with_parameter("amount", 1.0)
                        .with_parameter("fire", 2.0)
                        .with_parameter("smoke", 0.05)
                        .with_parameter("heat", 700.0)
                        .with_parameter("pressure", 0.0),
                ],
                amount_colors: Vec::new(),
            },
        })?;
        // inert olive sludge formed when acid mixes with water
        material_registry.register_cellular_dynamic(
            "acid sludge",
            MaterialPropertiesCellularDynamic {
                edit_color: MaterialGraphicsCellularDynamic {
                    sprite_sheet: Arc::new(SpriteSheet::from_color(accelerator, 95, 105, 40, 255)?),
                    frame: 0,
                    width: 1.0,
                    height: 1.0,
                },
                edit_radiance: MaterialGraphicsCellularDynamic {
                    sprite_sheet: Arc::new(SpriteSheet::from_color(accelerator, 0, 0, 0, 220)?),
                    frame: 0,
                    width: 1.0,
                    height: 1.0,
                },
                edit_radiance_intensity: 1.0,
                mass: 1.2,
                friction: 0.9,
                restitution: 0.0,
                thermal: MaterialPropertiesThermal {
                    heat_capacity: 2.0,
                    conduction: 0.3,
                    ..Default::default()
                },
                chemical: MaterialPropertiesChemical::default(),
            },
        )?;
        // hot fire gas releases light and eventually fades into smoke
        material_registry.register_gas("fire", MaterialPropertiesGas {
            mass: 20.0,
            diffusivity: 0.3,
            cohesion: 0.0,
            graphics: MaterialGraphicsGas {
                color: u32::from_le_bytes([255, 170, 60, 150]),
                radiance: pack_radiance([3.0, 1.2, 0.2, 0.01]),
                opacity: 1.0,
            },
            thermal: MaterialPropertiesThermal {
                heat_capacity: 1.0,
                conduction: 5.0,
                edit_temperature: 1300.0,
                cold_transition: Some(
                    MaterialTransition::new(600.0, 0.0, "smoke")
                        .with_amount(0.3),
                ),
                // dull red where fire cools toward smoke, orange, then yellow-white when hottest
                temperature_colors: vec![
                    MaterialTemperatureColor::new(650.0)
                        .with_color(MaterialColorTransformation {
                            matrix: [[0.6, 0.0, 0.0, 0.0], [0.0, 0.12, 0.0, 0.0],
                                [0.0, 0.0, 0.05, 0.0], [0.0, 0.0, 0.0, 0.7]],
                            ..MaterialColorTransformation::IDENTITY
                        })
                        .with_radiance(radiance([0.4, 0.05, 0.0, 0.0])),
                    MaterialTemperatureColor::new(1000.0)
                        .with_radiance(radiance([2.5, 0.9, 0.15, 0.0])),
                    MaterialTemperatureColor::new(1400.0)
                        .with_color(MaterialColorTransformation {
                            offset: [0.0, 0.35, 0.3, 0.1],
                            ..MaterialColorTransformation::IDENTITY
                        })
                        .with_radiance(radiance([4.0, 3.0, 1.2, 0.0])),
                ],
                ..Default::default()
            },
            chemical: MaterialPropertiesChemical::default(),
        })?;
        // dim gas left behind when fire cools or matter burns
        material_registry.register_gas("smoke", MaterialPropertiesGas {
            mass: 30.0,
            diffusivity: 0.2,
            cohesion: 0.05,
            graphics: MaterialGraphicsGas {
                color: u32::from_le_bytes([55, 55, 60, 110]),
                radiance: pack_radiance([0.0, 0.0, 0.0, 0.0]),
                opacity: 1.0,
            },
            thermal: MaterialPropertiesThermal {
                heat_capacity: 1.0,
                conduction: 0.05,
                edit_temperature: 350.0,
                ..Default::default()
            },
            chemical: MaterialPropertiesChemical::default(),
        })?;
        // glowing crystal of each color, its dimmer debris, and its brighter gas, which melts
        // well below rock and condenses back as easily
        for (color_name, color, stone_png, gravel_png) in [
            (
                "red",
                [1.0, 0.0, 0.0],
                include_bytes!("../assets/sprites/glowstone_red.png").as_slice(),
                include_bytes!("../assets/sprites/glowstone_gravel_red.png").as_slice(),
            ),
            (
                "green",
                [0.0, 1.0, 0.0],
                include_bytes!("../assets/sprites/glowstone_green.png").as_slice(),
                include_bytes!("../assets/sprites/glowstone_gravel_green.png").as_slice(),
            ),
            (
                "blue",
                [0.0, 0.0, 1.0],
                include_bytes!("../assets/sprites/glowstone_blue.png").as_slice(),
                include_bytes!("../assets/sprites/glowstone_gravel_blue.png").as_slice(),
            ),
            (
                "cyan",
                [0.0, 1.0, 1.0],
                include_bytes!("../assets/sprites/glowstone_cyan.png").as_slice(),
                include_bytes!("../assets/sprites/glowstone_gravel_cyan.png").as_slice(),
            ),
            (
                "magenta",
                [1.0, 0.0, 1.0],
                include_bytes!("../assets/sprites/glowstone_magenta.png").as_slice(),
                include_bytes!("../assets/sprites/glowstone_gravel_magenta.png").as_slice(),
            ),
            (
                "yellow",
                [1.0, 1.0, 0.0],
                include_bytes!("../assets/sprites/glowstone_yellow.png").as_slice(),
                include_bytes!("../assets/sprites/glowstone_gravel_yellow.png").as_slice(),
            ),
        ] {
            let glowstone: String = format!("glowstone_{color_name}");
            let glowstone_gravel: String = format!("glowstone_gravel_{color_name}");
            let glowstone_gas: String = format!("glowstone_gas_{color_name}");
            let glowstone_radiance: [f32; 4] = [2.0 * color[0], 2.0 * color[1], 2.0 * color[2], 0.4];
            let radiance_color: [u8; 3] = color.map(|channel: f32| (channel * 255.0).round() as u8);
            let gas_radiance: [f32; 4] = [2.5 * color[0], 2.5 * color[1], 2.5 * color[2], 0.01];
            let gas_color: [u8; 3] = color.map(|channel: f32| if channel > 0.5 { 255 } else { 48 });
            material_registry.register_cellular_static(&glowstone, MaterialPropertiesCellularStatic {
                edit_color: MaterialGraphicsCellularStatic {
                    sprite_sheet: Arc::new(SpriteSheet::from_png(accelerator, stone_png, 1, 1)?),
                    frame: 0,
                    width: 1.0,
                    height: 1.0,
                },
                edit_radiance: MaterialGraphicsCellularStatic {
                    sprite_sheet: Arc::new(SpriteSheet::from_color(
                        accelerator, radiance_color[0], radiance_color[1], radiance_color[2], 102,
                    )?),
                    frame: 0,
                    width: 1.0,
                    height: 1.0,
                },
                edit_radiance_intensity: 2.0,
                mass: 2.0,
                friction: 0.6,
                restitution: 0.1,
                strength: 10.0,
                absorption: 0.1,
                debris: Some(glowstone_gravel.clone()),
                thermal: MaterialPropertiesThermal {
                    heat_capacity: 1.0,
                    conduction: 0.5,
                    edit_temperature: 293.0,
                    hot_transition: Some(MaterialTransition::new(1100.0, 200.0, &glowstone_gas)),
                    ..Default::default()
                },
                chemical: MaterialPropertiesChemical {
                    tags: vec![MaterialTag::new("corrodible")], amount_colors: Vec::new(),
                },
            })?;
            material_registry.register_cellular_dynamic(
                &glowstone_gravel,
                MaterialPropertiesCellularDynamic {
                    edit_color: MaterialGraphicsCellularDynamic {
                        sprite_sheet: Arc::new(SpriteSheet::from_png(accelerator, gravel_png, 1, 1)?),
                        frame: 0,
                        width: 1.0,
                        height: 1.0,
                    },
                    edit_radiance: MaterialGraphicsCellularDynamic {
                        sprite_sheet: Arc::new(SpriteSheet::from_color(
                            accelerator, radiance_color[0], radiance_color[1], radiance_color[2], 102,
                        )?),
                        frame: 0,
                        width: 1.0,
                        height: 1.0,
                    },
                    edit_radiance_intensity: 0.5,
                    mass: 1.0,
                    friction: 0.8,
                    restitution: 0.1,
                    thermal: MaterialPropertiesThermal {
                        heat_capacity: 1.0,
                        conduction: 0.3,
                        edit_temperature: 293.0,
                        hot_transition: Some(MaterialTransition::new(1100.0, 200.0, &glowstone_gas)),
                        ..Default::default()
                    },
                    chemical: MaterialPropertiesChemical {
                        tags: vec![MaterialTag::new("corrodible")], amount_colors: Vec::new(),
                    },
                },
            )?;
            material_registry.register_gas(&glowstone_gas, MaterialPropertiesGas {
                mass: 18.0,
                diffusivity: 0.04,
                cohesion: 0.1,
                graphics: MaterialGraphicsGas {
                    color: u32::from_le_bytes([gas_color[0], gas_color[1], gas_color[2], 72]),
                    radiance: pack_radiance(gas_radiance),
                    opacity: 1.0,
                },
                thermal: MaterialPropertiesThermal {
                    heat_capacity: 1.0,
                    conduction: 0.05,
                    edit_temperature: 1150.0,
                    cold_transition: Some(
                        MaterialTransition::new(1050.0, 200.0, &glowstone)
                            .with_radiance(radiance(glowstone_radiance)),
                    ),
                    ..Default::default()
                },
                chemical: MaterialPropertiesChemical {
                    tags: vec![MaterialTag::new("corrodible")], amount_colors: Vec::new(),
                },
            })?;
        }
        // matter tagged for combustion burns by its own values once it is hot enough: fire and
        // smoke rise from its surfaces, and it stays below its flame temperature so a fire
        // settles instead of heating without bound
        let combustion = |parameter: &str| {
            MaterialValue::parameter(format!("combustion.{parameter}"))
        };
        material_registry.register_reaction(
            MaterialReaction::new(
                vec![MaterialReactant::tag("combustion", combustion("amount"))],
                vec![
                    MaterialReactionProduct::new("fire", combustion("fire")),
                    MaterialReactionProduct::new("smoke", combustion("smoke")),
                ],
            )
                .with_temperature_range(combustion("ignition"), combustion("flame"))
                .with_rate(combustion("rate"))
                .with_heat(combustion("heat"))
                .with_pressure(combustion("pressure")),
        );
        material_registry.register_reaction(
            MaterialReaction::new(
                vec![
                    MaterialReactant::material("acid", 0.5),
                    MaterialReactant::material("water", 0.5),
                ],
                vec![MaterialReactionProduct::new("acid sludge", 1.0)
                    .with_color(MaterialColorTransformation {
                        matrix: [[0.0; 4]; 4],
                        offset: sludge_color,
                        intensity_variance: 0.0,
                        channel_variance: [0.01, 0.01, 0.01, 0.0],
                    })],
            )
                .with_heat(50.0)
                .with_rate(4.0),
        );
        material_registry.register_reaction(
            MaterialReaction::new(
                vec![
                    MaterialReactant::material("acid", 0.05),
                    MaterialReactant::tag("corrodible", 0.34),
                ],
                Vec::new(),
            )
                .with_heat(10.0)
                .with_rate(8.0),
        );
        material_registry.register_reaction(
            MaterialReaction::new(
                vec![MaterialReactant::material("fire", 0.5)],
                vec![MaterialReactionProduct::new("smoke", 0.2)],
            )
                .with_rate(2.0),
        );
        material_registry.register_reaction(
            MaterialReaction::new(vec![MaterialReactant::material("smoke", 0.25)], Vec::new())
                .with_rate(0.5),
        );
        // the template starts from its generated world on every launch
        let world_path: PathBuf = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("world");
        let _ = std::fs::remove_dir_all(&world_path);
        let material_table: MaterialTable = material_registry.compile(accelerator)?;
        let stone: MaterialIdentifier = material_table.lookup("stone")
            .expect("registered stone material should be present");
        let mut scene: Scene = Scene::new(
            ChunkData::new(world_path),
            material_table,
            Arc::new(TemplateSceneGenerator { stone }),
            0,
        );
        // gentle daylight everywhere, which emitted light adds to
        scene.set_ambient_light([0.75, 0.78, 0.85])
            .expect("template ambient light should be valid");
        let black_radiance: Arc<SpriteSheet> =
            Arc::new(SpriteSheet::from_color(accelerator, 0, 0, 0, 0)?);
        // Each background is a normal sprite actor with an independent physical depth.
        let sky = Arc::new(SpriteSheet::from_color(accelerator, 131, 188, 218, 255)?);
        scene.actor_registry_mutable().spawn((
            ActorKinematicsPositionComponent::default(),
            ActorParallaxComponent {
                // A separate finite sky plane, reserved for ambient radiance later.
                depth: 10_000.0, ..Default::default()
            },
            ActorSpritesComponent {
                sprites: vec![(sky, Arc::clone(&black_radiance), 0, 0.0, 0.0,
                    16_000_000.0, 16_000_000.0, 0.0, [1.0; 4], 1.0, 0)],
            },
        ))?;
        for (pixels, depth, elevation, apparent_width, vertical_scroll) in [
            (include_bytes!("../assets/sprites/parallax_far_slopes.png").as_slice(), 24.0, 2.0, 120.0, 0.1),
            (include_bytes!("../assets/sprites/parallax_middle_slopes.png").as_slice(), 8.0, 1.0, 112.0, 0.25),
            (include_bytes!("../assets/sprites/parallax_near_slopes.png").as_slice(), 2.0, 0.5, 80.0, 0.5),
        ] {
            let sheet = Arc::new(SpriteSheet::from_png(accelerator, pixels, 1, 1)?);
            let distance = 1.0 + depth;
            // Taller textures put their surface near the center and continue stone below it.
            // Author apparent widths independently of physical distance.
            let width = apparent_width * distance;
            let scale = width * CELLULAR_TILE_SIZE as f32 / sheet.width() as f32;
            scene.actor_registry_mutable().spawn((
                ActorKinematicsPositionComponent::default(),
                ActorParallaxComponent {
                    depth, offset: [0.0, elevation * distance],
                    repeat: [ParallaxRepeat::Mirror, ParallaxRepeat::None],
                    scroll_factor: [1.0, vertical_scroll], ..Default::default()
                },
                ActorSpritesComponent {
                    sprites: vec![(sheet, Arc::clone(&black_radiance), 0, 0.0, 0.0,
                        scale, scale, 0.0, [1.0; 4], 1.0, 0)],
                },
            ))?;
        }
        // a one-tile white square with a matching circular body
        let player: Actor = scene.actor_registry_mutable().spawn((
            ActorKinematicsPositionComponent {
                position: ScenePosition { tile_x: 0, tile_y: 4, offset_x: 0, offset_y: 0 },
            },
            ActorSpritesComponent {
                sprites: vec![(
                    Arc::new(SpriteSheet::from_rgba(accelerator, &[255; 8 * 8 * 4], 8, 8, 1, 1)?),
                    Arc::clone(&black_radiance),
                    0,
                    0.0,
                    0.0,
                    1.0,
                    1.0,
                    0.0,
                    [1.0; 4],
                    1.0,
                    0,
                )],
            },
            ActorCollisionComponent {
                body: CollisionBody::Kinematic,
                collider: Collider {
                    shape: ColliderShape::Circle { radius: 0.5 },
                    offset_x: 0.0,
                    offset_y: 0.0,
                    rotation: 0.0,
                },
                memberships: COLLISION_GROUP_ACTOR | COLLISION_GROUP_EDIT,
                filter: u32::MAX & !COLLISION_GROUP_CELLULAR_DYNAMIC,
                mass: 40.0,
            },
            ActorPawnMovementComponent::default(),
            ActorPawnPossessionComponent,
            ActorCameraComponent { width: 80, height: 45, zoom: 1.0 },
            ActorPersistenceComponent,
        ))?;
        Ok(Self { scene, player })
    }

}

impl Game for TemplateGame {

    fn keyboard_event(&mut self, event: SystemKeyboardEvent) {
        if !event.pressed { return; }
        let zoom_factor: f32 = match event.physical_key {
            PhysicalKey::Code(KeyCode::Equal | KeyCode::NumpadAdd) => 1.0 / 1.1,
            PhysicalKey::Code(KeyCode::Minus | KeyCode::NumpadSubtract) => 1.1,
            _ => return,
        };
        let camera: &mut ActorCameraComponent = self.scene.actor_registry_mutable()
            .component_mutable(self.player)
            .expect("Template player must have a camera");
        // Zoom changes only the view; width and height keep the simulation area fixed.
        // The widest view fits within the streamed region, twice the unzoomed view.
        camera.zoom = (camera.zoom * zoom_factor).clamp(0.25, 2.0);
    }

    fn scene(&mut self) -> Option<(&mut Scene, Actor)> {
        Some((&mut self.scene, self.player))
    }

}
