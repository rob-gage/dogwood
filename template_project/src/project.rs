// Copyright Rob Gage 2026

use super::scene::TemplateSceneGenerator;
use engine::{
    Game,
    compute::Accelerator,
    graphics::Camera,
    physics::{
        actors::{
            ActorCollisionShape, ActorPawn, ActorPawnMovement, ActorPawnSwimmingConfiguration,
            ActorPawnWalkingConfiguration,
        },
        scenes::{
            MaterialExtraction, MaterialExtractionRequest, MaterialFilter, Scene, SceneData,
            ScenePosition, SceneRegion, SceneVelocity,
        },
        simulation::SceneSimulationConfiguration,
        tiles::TileCoordinates,
    },
    user_interface::UserInterfaceContext,
};

use std::{error::Error, sync::Arc};
use template_materials::TemplateMaterials;

#[cfg(test)]
use engine::physics::materials::{Material, MaterialIdentifier};

/// Runnable Dogwood template project with the shared template material set.
///
/// A project owns its UI context and the currently loaded scene. The scene is
/// created from temporary scene data and uses the caller-provided Accelerator.
pub struct TemplateProject {
    user_interface_context: UserInterfaceContext,
    scene: Option<Scene>,
    pawn: engine::physics::actors::Actor,
    collected: u32,
    extraction_demo_request: MaterialExtractionRequest,
    extraction_demo_amount: f32,
}

impl TemplateProject {
    /// Creates a template project with an Accelerator-backed scene.
    pub fn new(accelerator: &Arc<Accelerator>) -> Result<Self, Box<dyn Error>> {
        let TemplateMaterials {
            registry,
            stone,
            stone_variation,
            ..
        } = TemplateMaterials::new().map_err(std::io::Error::other)?;
        let data: SceneData = SceneData::new_temporary(registry)?;
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
            TemplateSceneGenerator {
                stone,
                stone_variation,
                stone_integrity: 20.0,
            },
        )?;
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
        let extraction_demo_request = scene.extract_materials(MaterialExtraction {
            region: SceneRegion::Circle {
                center: ScenePosition {
                    tile_coordinates: TileCoordinates { x: 0, y: -1 },
                    x_offset: 0.5,
                    y_offset: 0.5,
                },
                radius: 0.45,
            },
            filter: MaterialFilter::Material(stone),
        })?;
        Ok(Self {
            user_interface_context: UserInterfaceContext::new(),
            scene: Some(scene),
            pawn,
            collected: 0,
            extraction_demo_request,
            extraction_demo_amount: 0.0,
        })
    }
}

impl Game for TemplateProject {
    const TITLE: &'static str = "Demo Game";

    fn actor_contacts(&mut self, contacts: &[engine::physics::actors::ActorContactEvent]) {
        if let Some(scene) = self.scene.as_mut() {
            self.collected += crate::gameplay::collect_contacts(scene, self.pawn, contacts);
        }
    }

    fn compose_user_interface(&mut self) {
        crate::ui::draw_counter(&self.user_interface_context, self.collected);
        crate::ui::draw_extraction_demo(&self.user_interface_context, self.extraction_demo_amount);
    }

    fn material_extractions(
        &mut self,
        results: &[engine::physics::scenes::MaterialExtractionResult],
    ) {
        for result in results {
            if result.request == self.extraction_demo_request {
                self.extraction_demo_amount = result
                    .materials
                    .iter()
                    .map(|material| material.amount)
                    .sum();
            }
        }
    }

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

    fn material_identifier(scene: &Scene, name: &str) -> MaterialIdentifier {
        scene
            .materials()
            .iter()
            .find_map(|(material_identifier, material)| {
                (material.name() == name).then_some(material_identifier)
            })
            .unwrap_or_else(|| panic!("missing demo material {name}"))
    }

    fn transition_target(scene: &Scene, source: &str, hot: bool) -> Option<String> {
        let properties: &engine::physics::materials::MaterialThermalProperties = scene
            .materials()
            .thermal_properties(material_identifier(scene, source))
            .unwrap();
        let transition: &engine::physics::materials::MaterialThermalTransition = if hot {
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
    fn test_demo_material_graph_and_rigid_thresholds_are_declarative() {
        let accelerator: Arc<Accelerator> = Arc::new(Accelerator::new().unwrap());
        let game: TemplateProject = TemplateProject::new(&accelerator).unwrap();
        let scene: &Scene = game.scene.as_ref().unwrap();
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
            match scene
                .materials()
                .get(material_identifier(scene, source))
                .unwrap()
            {
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
            match scene
                .materials()
                .get(material_identifier(scene, source))
                .unwrap()
            {
                Material::CellularStatic {
                    debris_material, ..
                } => {
                    let debris: Option<&Material> = debris_material
                        .and_then(|material_identifier| scene.materials().get(material_identifier));
                    assert_eq!(debris.map(Material::name), Some(target));
                }
                _ => panic!("{source} is not static"),
            }
        }
    }

    #[test]
    fn extraction_demo_completes_asynchronously() {
        let accelerator: Arc<Accelerator> = Arc::new(Accelerator::new().unwrap());
        let mut game: TemplateProject = TemplateProject::new(&accelerator).unwrap();
        for _ in 0..8 {
            let results = {
                let scene = game.scene.as_mut().unwrap();
                scene
                    .update(std::time::Duration::from_millis(20), false)
                    .unwrap();
                scene.take_material_extraction_results()
            };
            if let Some(result) = results
                .into_iter()
                .find(|result| result.request == game.extraction_demo_request)
            {
                assert!(
                    result
                        .materials
                        .iter()
                        .any(|material| material.amount > 0.0)
                );
                return;
            }
        }
        panic!("extraction demo did not complete");
    }
}
