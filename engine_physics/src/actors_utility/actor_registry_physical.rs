// Copyright Rob Gage 2026

use engine_graphics::SceneActorGraphics;
use engine_graphics::SceneSpriteGraphics;
use engine_graphics::SceneSpriteSheetGraphics;

use super::actor_physical::ActorPhysical;
use super::actor_registry::ActorRegistry;
use crate::actors::Actor;
use crate::actors::ActorPhysicalConfiguration;
use crate::actors::ActorPhysicalSnapshot;
use crate::actors::ActorPreviousPosition;
use crate::actors::ActorSprites;
use crate::scenes::ScenePosition;
use crate::scenes::SceneVelocity;

impl ActorRegistry {
    /// Captures a loaded generic physical actor without ECS or Rapier state.
    pub(crate) fn physical_snapshot(&self, identifier: Actor) -> Option<ActorPhysicalSnapshot> {
        let entity: bevy_ecs::entity::Entity = self.bevy_entity(identifier)?;
        let physical: &ActorPhysical = self.world.get::<ActorPhysical>(entity)?;
        Some(ActorPhysicalSnapshot {
            actor: identifier,
            position: *self.world.get::<ScenePosition>(entity)?,
            velocity: *self.world.get::<SceneVelocity>(entity)?,
            configuration: physical.0,
            sprites: self.world.get::<ActorSprites>(entity).cloned(),
        })
    }

    pub(crate) fn physical_snapshots(&self) -> Vec<ActorPhysicalSnapshot> {
        self.reverse_entities
            .values()
            .filter_map(|actor| self.physical_snapshot(*actor))
            .collect()
    }

    pub fn physical_configuration(&self, identifier: Actor) -> Option<ActorPhysicalConfiguration> {
        let entity: bevy_ecs::entity::Entity = self.bevy_entity(identifier)?;
        self.world.get::<ActorPhysical>(entity).map(|value| value.0)
    }

    /// Restores a previously unloaded generic actor with its original identity.
    pub(crate) fn restore_physical_snapshot(&mut self, snapshot: ActorPhysicalSnapshot) -> bool {
        let restored: bool = self.spawn_components_with_actor(
            snapshot.actor,
            (
                ActorPhysical(snapshot.configuration),
                ActorPreviousPosition(snapshot.position),
                snapshot.position,
                snapshot.velocity,
            ),
        );
        if restored && let Some(sprites) = snapshot.sprites {
            self.set_sprites(snapshot.actor, sprites);
        }
        restored
    }

    pub(crate) fn actor_graphics(&self, interpolation: f32) -> Vec<SceneActorGraphics> {
        self.world
            .iter_entities()
            .filter_map(|entity| {
                let physical: &ActorPhysical = entity.get::<ActorPhysical>()?;
                if entity.get::<ActorSprites>().is_some() {
                    return None;
                }
                let position: ScenePosition = *entity.get::<ScenePosition>()?;
                let previous: ScenePosition = entity
                    .get::<ActorPreviousPosition>()
                    .map_or(position, |value| value.0);
                let interpolated_position: ScenePosition =
                    position.interpolated(previous, interpolation);
                Some(SceneActorGraphics {
                    position: [
                        interpolated_position.tile_coordinates.x as f32
                            + interpolated_position.x_offset,
                        interpolated_position.tile_coordinates.y as f32
                            + interpolated_position.y_offset,
                    ],
                    size: physical.0.collision_shape.nominal_dimensions(),
                    color: physical.0.color,
                })
            })
            .collect()
    }

    pub(crate) fn sprite_graphics(&self, interpolation: f32) -> Vec<SceneSpriteGraphics> {
        self.world
            .iter_entities()
            .filter_map(|entity| {
                let sprites: &ActorSprites = entity.get::<ActorSprites>()?;
                let identifier = sprites.current_animation_identifier()?;
                let animation = sprites.animation(identifier)?;
                let position: ScenePosition = *entity.get::<ScenePosition>()?;
                let previous: ScenePosition = entity
                    .get::<ActorPreviousPosition>()
                    .map_or(position, |value| value.0);
                let interpolated_position: ScenePosition =
                    position.interpolated(previous, interpolation);
                let frame_x: u32 = animation.frame_width().checked_mul(sprites.frame_index())?;
                let texture_sheet_width: f32 = animation.sprite_sheet().width() as f32;
                let texture_sheet_height: f32 = animation.sprite_sheet().height() as f32;
                let texture_coordinates: [f32; 4] = [
                    (frame_x as f32 + 0.5) / texture_sheet_width,
                    0.5 / texture_sheet_height,
                    (frame_x + animation.frame_width()) as f32 - 0.5,
                    animation.frame_height() as f32 - 0.5,
                ];
                let texture_coordinates: [f32; 4] = [
                    texture_coordinates[0],
                    texture_coordinates[1],
                    texture_coordinates[2] / texture_sheet_width,
                    texture_coordinates[3] / texture_sheet_height,
                ];
                let sprite_sheet = animation.sprite_sheet();
                let radiance_sprite_sheet =
                    animation
                        .radiance_sprite_sheet()
                        .map(|sheet| SceneSpriteSheetGraphics {
                            width: sheet.width(),
                            height: sheet.height(),
                            rgba_data: sheet.rgba_data_shared(),
                        });
                Some(SceneSpriteGraphics {
                    position: [
                        interpolated_position.tile_coordinates.x as f32
                            + interpolated_position.x_offset,
                        interpolated_position.tile_coordinates.y as f32
                            + interpolated_position.y_offset,
                    ],
                    world_size: sprites.world_size(),
                    world_offset: sprites.world_offset(),
                    texture_coordinates,
                    sprite_sheet: SceneSpriteSheetGraphics {
                        width: sprite_sheet.width(),
                        height: sprite_sheet.height(),
                        rgba_data: sprite_sheet.rgba_data_shared(),
                    },
                    radiance_sprite_sheet,
                })
            })
            .collect()
    }
}
