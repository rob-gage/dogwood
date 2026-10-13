// Copyright Rob Gage 2026

use crate::{
    materials,
    template_scene_generator::TemplateSceneGenerator,
};
use engine::{
    actor_components::{
        ActorCameraComponent,
        ActorCollisionComponent,
        ActorKinematicsPositionComponent,
        ActorPersistenceComponent,
        ActorSpritesComponent,
    },
    actors::Actor,
    chunks::ChunkData,
    games::Game,
    materials::{
        MaterialIdentifier,
        MaterialRegistry,
        MaterialTable,
    },
    scenes::{
        Scene,
        SceneCollider,
        SceneColliderShape,
        SceneCollisionBody,
        ScenePosition,
    },
    Accelerator,
    SpriteSheet,
};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Arc,
};

/// Flat stone world viewed by an invisible camera above a white test actor
pub struct TemplateGame {
    /// Scene holding the world and camera
    scene: Scene,
    /// Invisible actor whose camera displays the scene
    camera_actor: Actor,
}

impl TemplateGame {

    /// Creates the scene, its camera actor at the world origin, and a test actor above the stone
    pub fn new(accelerator: &Accelerator) -> Self {
        let mut material_registry: MaterialRegistry = MaterialRegistry::new();
        let mut material_identifiers: HashMap<String, MaterialIdentifier> = HashMap::new();
        for material in materials::definitions(accelerator) {
            let material_name: String = String::from(material.name());
            let identifier: MaterialIdentifier = material_registry
                .register(material)
                .unwrap_or_else(|error| {
                    panic!("failed to register template material {material_name}: {error:?}")
                });
            material_identifiers.insert(material_name, identifier);
        }
        let stone: MaterialIdentifier = material_identifiers
            .get("stone")
            .copied()
            .expect("template stone material must be registered");
        let material_table: MaterialTable = material_registry
            .compile(accelerator)
            .unwrap_or_else(|error| {
                panic!("failed to compile template materials: {error:?}")
            });
        let mut scene: Scene = Scene::new(
            ChunkData::new(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("world")),
            material_table,
            Arc::new(TemplateSceneGenerator { stone }),
            0,
        );
        let camera_actor: Actor = scene.actor_registry_mutable()
            .spawn((
                ActorCameraComponent { width: 80, height: 45, zoom: 1.0 },
                ActorKinematicsPositionComponent::default(),
                ActorPersistenceComponent,
            ))
            .unwrap();
        // a one-tile white square with a matching circular body
        let white_sprite_sheet: Arc<SpriteSheet> = Arc::new(
            SpriteSheet::from_rgba(accelerator, &[255; 8 * 8 * 4], 8, 8, 1, 1)
                .unwrap_or_else(|error| {
                    panic!("failed to create template white sprite: {error:?}")
                }),
        );
        scene.actor_registry_mutable()
            .spawn((
                ActorKinematicsPositionComponent {
                    position: ScenePosition { tile_x: 0, tile_y: 4, offset_x: 0, offset_y: 0 },
                },
                ActorSpritesComponent {
                    sprites: vec![(white_sprite_sheet, 0, 0.0, 0.0, 1.0, 1.0, 0.0, [1.0; 4], 0)],
                },
                ActorCollisionComponent {
                    body: SceneCollisionBody::Dynamic,
                    collider: SceneCollider {
                        shape: SceneColliderShape::Circle { radius: 0.5 },
                        offset_x: 0.0,
                        offset_y: 0.0,
                        rotation: 0.0,
                    },
                    memberships: u32::MAX,
                    filter: u32::MAX,
                },
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
