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
        materials::{
            Material,
            MaterialIdentifier,
            MaterialRegistry,
        },
        scenes::{
            Scene,
            SceneConfiguration,
        },
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
            name: "Stone",
            graphics: MaterialAppearance::from_color(Color::new_rgb(128, 128, 128)),
        });
        Ok(Self {
            user_interface_context: UserInterfaceContext::new(),
            scene: Some(Scene::new_with_generator(accelerator, SceneConfiguration {
                material_graphics: materials.build_material_graphics(accelerator),
                data_path: "demo_data".into(),
                simulation_width: 16,
                simulation_height: 9,
                simulation_buffer_size: 4,
                tile_streaming_batch_size: 1,
            }, DemoSceneGenerator { stone })?),
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

    fn is_paused(&self) -> bool { true }

    fn scene(&self) -> Option<&Scene> { self.scene.as_ref() }

    fn scene_mutable(&mut self) -> Option<&mut Scene> { self.scene.as_mut() }

    fn user_interface_context(&mut self) -> &mut UserInterfaceContext {
        &mut self.user_interface_context
    }

}
