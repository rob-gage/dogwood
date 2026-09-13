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
        let stone_debris_graphics: MaterialAppearance = MaterialAppearance::from_color(
            Color::new_rgb(148, 148, 148),
        ).with_variation([0.5, 0.5, 0.5, 0.0])
            .with_color_influence([0.25, 0.25, 0.25, 0.0]);
        let stone_debris: MaterialIdentifier = materials.register(Material::CellularDynamic {
            name: "Stone Debris".into(),
            graphics: stone_debris_graphics,
            mass: 3.0,
            pressure_transmission: 0.55,
            friction: 0.45,
            restitution: 0.15,
        });
        let sand_graphics: MaterialAppearance = MaterialAppearance::from_color(
            Color::new_rgb(194, 178, 128),
        ).with_variation([0.5, 0.5, 0.5, 0.0])
            .with_color_influence([0.20, 0.18, 0.12, 0.0]);
        let sand: MaterialIdentifier = materials.register(Material::CellularDynamic {
            name: "Sand".into(),
            graphics: sand_graphics,
            mass: 1.0,
            pressure_transmission: 0.35,
            friction: 0.65,
            restitution: 0.05,
        });
        let stone_graphics: MaterialAppearance = MaterialAppearance::from_color(
            Color::new_rgb(108, 108, 108),
        ).with_variation([0.5, 0.5, 0.5, 0.0])
            .with_color_influence([0.25, 0.25, 0.25, 0.0]);
        let stone: MaterialIdentifier = materials.register(Material::CellularStatic {
            name: "Stone".into(),
            graphics: stone_graphics,
            pressure_ignore_threshold: 8.0,
            default_integrity: 20.0,
            debris_material: Some(stone_debris),
            debris_yield_rate: 0.8,
            pressure_transmission: 0.8,
            friction: 0.8,
            restitution: 0.02,
        });
        materials.register(Material::Fluid {
            name: "Water".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(45, 125, 210)),
            pressure_transmission: 0.95,
            friction: 0.05,
            restitution: 0.0,
            rest_density: 1.0,
            artificial_pressure: 0.02,
            xsph_smoothing: 0.08,
            body_push_speed: 4.0,
        });
        let data: SceneData = SceneData::new_temporary(materials)?;
        let mut scene: Scene = Scene::load_with_generator(
            accelerator,
            SceneSimulationConfiguration {
                gravity: [0.0, -18.0],
                width: 16,
                height: 9,
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
        let mut pawn_configuration: ActorPawn = ActorPawn::new();
        pawn_configuration.walking = Some(ActorPawnWalkingConfiguration {
            speed: 4.0,
            acceleration: 24.0,
            mass: 8.0,
            jump_velocity: 7.0,
            maximum_slope_angle: 50.0_f32.to_radians(),
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
