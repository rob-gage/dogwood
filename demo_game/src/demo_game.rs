// Copyright Rob Gage 2026

use super::DemoSceneGenerator;
use engine::{
    Game,
    compute::Accelerator,
    graphics::{
        Camera,
        Color,
        MaterialAppearance,
    },
    physics::{
        actors::{
            ActorPawn,
            ActorPawnMovement,
            ActorPawnWalkingConfiguration,
        },
        materials::{
            Material,
            MaterialIdentifier,
            MaterialRegistry,
        },
        scenes::{
            Scene,
            SceneData,
            ScenePosition,
            SceneVelocity,
        },
        simulation::SceneSimulationConfiguration,
        tiles::TileCoordinates,
    },
    user_interface::UserInterfaceContext,
};

use std::{
    error::Error,
    sync::Arc,
};

/// A minimal game used to test the engine
pub struct DemoGame {
    user_interface_context: UserInterfaceContext,
    scene: Option<Scene>,
}

impl DemoGame {

    /// Creates a `DemoGame` with a GPU-backed scene
    pub fn new(accelerator: &Arc<Accelerator>) -> Result<Self, Box<dyn Error>> {
        let mut materials: MaterialRegistry = MaterialRegistry::new();
        let stone: MaterialIdentifier = materials.register(Material::CellularStatic {
            name: "Stone".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(128, 128, 128)),
        });
        let data: SceneData = SceneData::new_temporary(materials)?;
        let mut scene: Scene = Scene::load_with_generator(
            accelerator,
            SceneSimulationConfiguration {
                // Reverse the sign to test walking and jumping on the ceiling platform.
                gravity: [0.0, -18.0],
                width: 16,
                height: 9,
                buffer_size: 4,
                streaming_batch_size: 1,
            },
            data,
            DemoSceneGenerator { stone },
        )?;
        let mut pawn_configuration: ActorPawn = ActorPawn::new();
        pawn_configuration.walking = Some(ActorPawnWalkingConfiguration {
            speed: 4.0,
            acceleration: 24.0,
            jump_velocity: 7.0,
            collider_width: 0.75,
            collider_height: 0.75,
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
            width: 16.0,
            height: 9.0,
            zoom: 1.0,
            follow_acceleration: 0.0,
            follow_speed: 0.0,
            follow_distance_maximum: 0.0,
        }
    }

    fn is_paused(&self) -> bool { false }

    fn scene(&self) -> Option<&Scene> { self.scene.as_ref() }

    fn scene_mutable(&mut self) -> Option<&mut Scene> { self.scene.as_mut() }

    fn user_interface_context(&mut self) -> &mut UserInterfaceContext {
        &mut self.user_interface_context
    }

}
