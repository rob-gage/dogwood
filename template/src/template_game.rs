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
        MaterialPropertiesCellularDynamic,
        MaterialPropertiesCellularStatic,
        MaterialPropertiesFluid,
        MaterialRegistry,
    },
    scenes::{
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
    MaterialGraphicsCellularDynamic,
    MaterialGraphicsCellularStatic,
    MaterialGraphicsFluid,
    SpriteSheet,
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
        // cellular dynamic debris that piles at its angle of repose and that players wade through
        let gravel: MaterialIdentifier = material_registry.register(Material::CellularDynamic {
            name: String::from("gravel"),
            properties: MaterialPropertiesCellularDynamic {
                graphics: MaterialGraphicsCellularDynamic {
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
                mass: 1.0,
                friction: 0.8,
                restitution: 0.1,
            },
        })?;
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
                mass: 2.0,
                friction: 0.6,
                restitution: 0.1,
                debris: Some(gravel),
            },
        })?;
        let _water: MaterialIdentifier = material_registry.register(Material::Fluid {
            name: String::from("water"),
            properties: MaterialPropertiesFluid {
                density: 0.6,
                viscosity: 0.05,
                cohesion: 0.2,
                artificial_pressure: 0.001,
                drag: 1.0,
                friction: 0.0,
                restitution: 0.0,
                graphics: MaterialGraphicsFluid {
                    color: u32::from_le_bytes([40, 110, 200, 200]),
                },
            },
        })?;
        // the template starts from its generated world on every launch
        let world_path: PathBuf = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("world");
        let _ = std::fs::remove_dir_all(&world_path);
        let mut scene: Scene = Scene::new(
            ChunkData::new(world_path),
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
