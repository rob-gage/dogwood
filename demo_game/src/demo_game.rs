// Copyright Rob Gage 2026

use super::DemoSceneGenerator;
use engine::{
    Game,
    compute::Accelerator,
    graphics::{Camera, Color, MaterialAppearance},
    physics::{
        actors::{
            ActorCollisionShape, ActorPawn, ActorPawnMovement, ActorPawnSwimmingConfiguration,
            ActorPawnWalkingConfiguration,
        },
        materials::{
            Material, MaterialIdentifier, MaterialRegistryBuilder, MaterialThermalProperties,
            MaterialThermalTransition,
        },
        scenes::{Scene, SceneData, SceneEditBatch, ScenePosition, SceneVelocity},
        simulation::SceneSimulationConfiguration,
        tiles::{CellCoordinates, CellularAppearance, TileCoordinates},
    },
    user_interface::UserInterfaceContext,
};

use std::{error::Error, sync::Arc};

/// A minimal game used to test the engine
pub struct DemoGame {
    user_interface_context: UserInterfaceContext,
    scene: Option<Scene>,
}

impl DemoGame {
    /// Creates a `DemoGame` with a GPU-backed scene
    pub fn new(accelerator: &Arc<Accelerator>) -> Result<Self, Box<dyn Error>> {
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
            .map_err(std::io::Error::other)?;
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
            .map_err(std::io::Error::other)?;
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
            .map_err(std::io::Error::other)?;
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
            .map_err(std::io::Error::other)?;
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
            .map_err(std::io::Error::other)?;
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
            .map_err(std::io::Error::other)?;
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
            .map_err(std::io::Error::other)?;
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
            .map_err(std::io::Error::other)?;
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
            .map_err(std::io::Error::other)?;
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
            .map_err(std::io::Error::other)?;
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
            .map_err(std::io::Error::other)?;
        let data: SceneData =
            SceneData::new_temporary(materials.compile().map_err(std::io::Error::other)?)?;
        let mut scene: Scene = Scene::load_with_generator(
            accelerator,
            SceneSimulationConfiguration {
                gravity: [0.0, -18.0],
                ambient_temperature: 293.15,
                empty_space_thermal_conductivity: 0.0,
                empty_space_heat_capacity: 1.0,
                maximum_gas_concentration: 4.0,
                width: 48,
                height: 27,
                buffer_size: 12,
                streaming_batch_size: 4,
            },
            data,
            DemoSceneGenerator {
                stone,
                stone_variation: stone_graphics.variation(),
                sand,
                sand_variation: sand_graphics.variation(),
                stone_integrity: 20.0,
            },
        )?;
        let mut gas_edits: SceneEditBatch = SceneEditBatch::new();
        gas_edits.place_material(
            water_vapor,
            CellularAppearance::NEUTRAL,
            (8..20)
                .flat_map(|y| (-56..-40).map(move |x| CellCoordinates { x, y }))
                .collect(),
        );
        gas_edits.place_material(
            smoke,
            CellularAppearance::NEUTRAL,
            (8..20)
                .flat_map(|y| (-44..-28).map(move |x| CellCoordinates { x, y }))
                .collect(),
        );
        scene.queue_edits(gas_edits);
        let mut pawn_configuration: ActorPawn = ActorPawn::new();
        pawn_configuration.collision_shape = Some(ActorCollisionShape::Circle { radius: 0.375 });
        pawn_configuration.walking = Some(ActorPawnWalkingConfiguration {
            speed: 4.0,
            acceleration: 24.0,
            mass: 8.0,
            jump_velocity: 7.0,
            maximum_slope_angle: 50.0_f32.to_radians(),
        });
        pawn_configuration.swimming = Some(ActorPawnSwimmingConfiguration {
            maximum_speed: 3.5,
            acceleration: 12.0,
            density: 0.95,
            drag: 1.0,
            enter_immersion: 0.55,
            exit_immersion: 0.35,
        });
        pawn_configuration.movement = Some(ActorPawnMovement::Walking);
        let pawn: engine::physics::actors::Actor =
            scene.actor_registry_mutable().spawn_possessable_pawn(
                pawn_configuration,
                ScenePosition {
                    tile_coordinates: TileCoordinates { x: 0, y: 1 },
                    x_offset: 0.5,
                    y_offset: 0.75,
                },
                SceneVelocity { x: 0.0, y: 0.0 },
            );
        scene.possess_actor(pawn);
        Ok(Self {
            user_interface_context: UserInterfaceContext::new(),
            scene: Some(scene),
        })
    }
}

impl Game for DemoGame {
    const TITLE: &'static str = "Demo Game";

    fn camera(&self) -> Camera {
        Camera {
            width: 48.0,
            height: 27.0,
            zoom: 1.0,
            follow_acceleration: 0.0,
            follow_speed: 0.0,
            follow_distance_maximum: 0.0,
        }
    }

    fn is_paused(&self) -> bool {
        false
    }

    fn scene(&self) -> Option<&Scene> {
        self.scene.as_ref()
    }

    fn scene_mutable(&mut self) -> Option<&mut Scene> {
        self.scene.as_mut()
    }

    fn user_interface_context(&mut self) -> &mut UserInterfaceContext {
        &mut self.user_interface_context
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn material_id(scene: &Scene, name: &str) -> MaterialIdentifier {
        scene
            .materials()
            .iter()
            .find_map(|(id, material)| (material.name() == name).then_some(id))
            .unwrap_or_else(|| panic!("missing demo material {name}"))
    }

    fn transition_target(scene: &Scene, source: &str, hot: bool) -> Option<String> {
        let properties = scene
            .materials()
            .thermal_properties(material_id(scene, source))
            .unwrap();
        let transition = if hot {
            properties.hot_transition.as_ref()
        } else {
            properties.cold_transition.as_ref()
        }?;
        Some(
            scene
                .materials()
                .get(transition.target)
                .unwrap()
                .name()
                .to_owned(),
        )
    }

    #[test]
    fn demo_material_graph_and_rigid_thresholds_are_declarative() {
        let accelerator = Arc::new(Accelerator::new().unwrap());
        let game = DemoGame::new(&accelerator).unwrap();
        let scene = game.scene.as_ref().unwrap();
        for (source, target) in [
            ("Ice", "Water"),
            ("Slush", "Water"),
            ("Stone", "Lava"),
            ("Stone Debris", "Lava"),
            ("Sand", "Molten Glass"),
            ("Glass", "Molten Glass"),
            ("Broken Glass", "Molten Glass"),
        ] {
            assert_eq!(
                transition_target(scene, source, true).as_deref(),
                Some(target)
            );
        }
        for (source, target) in [("Lava", "Stone"), ("Molten Glass", "Glass")] {
            assert_eq!(
                transition_target(scene, source, false).as_deref(),
                Some(target)
            );
        }
        for source in ["Slush", "Stone Debris", "Broken Glass"] {
            assert_eq!(transition_target(scene, source, false), None);
        }
        for source in ["Lava", "Molten Glass"] {
            assert_eq!(transition_target(scene, source, true), None);
        }
        for (source, minimum) in [("Stone", 12), ("Ice", 32), ("Glass", 24)] {
            match scene.materials().get(material_id(scene, source)).unwrap() {
                Material::CellularStatic {
                    minimum_rigid_body_cell_count,
                    ..
                } => {
                    assert_eq!(*minimum_rigid_body_cell_count, minimum)
                }
                _ => panic!("{source} is not static"),
            }
        }
        for (source, target) in [
            ("Stone", "Stone Debris"),
            ("Ice", "Slush"),
            ("Glass", "Broken Glass"),
        ] {
            match scene.materials().get(material_id(scene, source)).unwrap() {
                Material::CellularStatic {
                    debris_material, ..
                } => {
                    let debris = debris_material.and_then(|id| scene.materials().get(id));
                    assert_eq!(debris.map(Material::name), Some(target));
                }
                _ => panic!("{source} is not static"),
            }
        }
    }
}
