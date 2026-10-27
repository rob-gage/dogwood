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
        MaterialIdentifier,
        MaterialPropertiesCellularDynamic,
        MaterialPropertiesCellularStatic,
        MaterialPropertiesFluid,
        MaterialPropertiesGas,
        MaterialPropertiesThermal,
        MaterialRegistry,
        MaterialTable,
        MaterialTransition,
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
        let melting = |product: &str| -> Option<MaterialTransition> {
            Some(MaterialTransition::new(1400.0, 300.0, product)
                .with_color(blend(0.2, lava_color, 0.0)))
        };
        let thawing = |product: &str| -> Option<MaterialTransition> {
            Some(MaterialTransition::new(273.0, 330.0, product)
                .with_color(blend(0.0, water_color, 0.0)))
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
            mass: 1.0,
            friction: 0.8,
            restitution: 0.1,
            thermal: MaterialPropertiesThermal {
                heat_capacity: 1.0,
                conduction: 0.3,
                edit_temperature: 293.0,
                hot_transition: melting("lava"),
                ..Default::default()
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
                ..Default::default()
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
            thermal: MaterialPropertiesThermal {
                heat_capacity: 4.0,
                conduction: 0.6,
                edit_temperature: 293.0,
                // each particle boils into an atmosphere of vapor in its cell
                hot_transition: Some(MaterialTransition::new(373.0, 1000.0, "water vapor")),
                cold_transition: Some(
                    MaterialTransition::new(273.0, 330.0, "ice")
                        .with_color(blend(0.1, ice_color, 0.03)),
                ),
            },
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
                        }),
                ),
                ..Default::default()
            },
        })?;
        material_registry.register_gas("water vapor", MaterialPropertiesGas {
            mass: 18.0,
            diffusivity: 0.04,
            cohesion: 0.1,
            graphics: MaterialGraphicsGas {
                color: u32::from_le_bytes([225, 230, 240, 150]),
                opacity: 1.0,
            },
            thermal: MaterialPropertiesThermal {
                heat_capacity: 1.0,
                conduction: 0.05,
                edit_temperature: 400.0,
                // an atmosphere of vapor in one cell condenses into one particle
                cold_transition: Some(
                    MaterialTransition::new(373.0, 1000.0, "water")
                        .with_color(blend(0.0, water_color, 0.0)),
                ),
                ..Default::default()
            },
        })?;
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
        // Each background is a normal sprite actor with an independent physical depth.
        let sky = Arc::new(SpriteSheet::from_color(accelerator, 131, 188, 218, 255)?);
        scene.actor_registry_mutable().spawn((
            ActorKinematicsPositionComponent::default(),
            ActorParallaxComponent {
                // A separate finite sky plane, reserved for ambient radiance later.
                depth: 10_000.0, ..Default::default()
            },
            ActorSpritesComponent {
                sprites: vec![(sky, 0, 0.0, 0.0, 16_000_000.0, 16_000_000.0,
                    0.0, [1.0; 4], 0)],
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
                    sprites: vec![(sheet, 0, 0.0, 0.0, scale, scale, 0.0, [1.0; 4], 0)],
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
