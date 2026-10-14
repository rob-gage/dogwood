// Copyright Rob Gage 2026

use crate::template_scene_generator::TemplateSceneGenerator;
use engine::{
    actors::{
        components::{
            ActorCameraComponent,
            ActorCollisionComponent,
            ActorKinematicsPositionComponent,
            ActorPawnMovementComponent,
            ActorPawnPossessionComponent,
            ActorPersistenceComponent,
            ActorSpritesComponent,
        },
        Actor,
    },
    games::Game,
    materials::{
        Material,
        MaterialIdentifier,
        MaterialPropertiesCellularStatic,
        MaterialRegistry,
    },
    scenes::{
        chunks::ChunkData,
        collisions::{
            Collider,
            ColliderShape,
            CollisionBody,
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
    MaterialGraphicsCellularStatic,
    SpriteSheet,
};
use std::{
    path::PathBuf,
    sync::Arc,
};

/// Flat stone world explored by a white square player
pub struct TemplateGame {
    /// Scene holding the world and player
    scene: Scene,
    /// Keyboard-controlled pawn whose camera displays the scene
    player: Actor,
}

impl TemplateGame {

    /// Creates the stone world and a player falling onto it from above
    pub fn new(accelerator: &Accelerator) -> Result<Self, EngineError> {
        let mut material_registry: MaterialRegistry = MaterialRegistry::new();
        let stone: MaterialIdentifier = material_registry.register(Material::CellularStatic {
            name: String::from("stone"),
            properties: MaterialPropertiesCellularStatic {
                graphics: MaterialGraphicsCellularStatic {
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
            },
        })?;
        let mut scene: Scene = Scene::new(
            ChunkData::new(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("world")),
            material_registry.compile(accelerator)?,
            Arc::new(TemplateSceneGenerator { stone }),
            0,
        );
        // a one-tile white square with a matching circular body
        let player: Actor = scene.actor_registry_mutable().spawn((
            ActorKinematicsPositionComponent {
                position: ScenePosition { tile_x: 0, tile_y: 4, offset_x: 0, offset_y: 0 },
            },
            ActorSpritesComponent {
                sprites: vec![(
                    Arc::new(SpriteSheet::from_rgba(accelerator, &[255; 8 * 8 * 4], 8, 8, 1, 1)?),
                    0,
                    0.0,
                    0.0,
                    1.0,
                    1.0,
                    0.0,
                    [1.0; 4],
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
                memberships: u32::MAX,
                filter: u32::MAX,
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
