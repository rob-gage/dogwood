// Copyright Rob Gage 2026

use crate::template_scene_generator::TemplateSceneGenerator;
use engine::{
    actor_components::{
        ActorCameraComponent,
        ActorKinematicsPositionComponent,
    },
    actors::Actor,
    chunks::ChunkData,
    games::Game,
    materials::{
        Material,
        MaterialForm,
        MaterialIdentifier,
        MaterialRegistry,
    },
    scenes::Scene,
};
use std::{
    path::PathBuf,
    sync::Arc,
};

/// Flat stone world viewed by an invisible camera
pub struct TemplateGame {
    /// Scene holding the world and camera
    scene: Scene,
    /// Invisible actor whose camera displays the scene
    camera_actor: Actor,
}

impl TemplateGame {

    /// Creates the scene and its camera actor at the world origin
    pub fn new() -> Self {
        let mut material_registry: MaterialRegistry = MaterialRegistry::new();
        let stone: MaterialIdentifier = material_registry
            .register(Material { name: String::from("stone"), form: MaterialForm::CellularStatic })
            .unwrap();
        let mut scene: Scene = Scene::new(
            ChunkData::new(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("world")),
            material_registry,
            Arc::new(TemplateSceneGenerator { stone }),
            0,
        );
        let camera_actor: Actor = scene.actor_registry_mutable()
            .spawn((
                ActorCameraComponent { width: 80, height: 45, zoom: 1.0 },
                ActorKinematicsPositionComponent::default(),
            ))
            .unwrap();
        Self { scene, camera_actor }
    }

}

impl Game for TemplateGame {

    fn scene(&mut self) -> Option<(&mut Scene, Actor)> {
        Some((&mut self.scene, self.camera_actor))
    }

}
