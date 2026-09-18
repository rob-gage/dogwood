// Copyright Rob Gage 2026

use engine_graphics::SceneActorGraphics;

use super::actor_physical::ActorPhysical;
use super::actor_registry::ActorRegistry;
use crate::actors::Actor;
use crate::actors::ActorPhysicalConfiguration;
use crate::actors::ActorPhysicalSnapshot;
use crate::actors::ActorPreviousPosition;
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
        self.spawn_components_with_actor(
            snapshot.actor,
            (
                ActorPhysical(snapshot.configuration),
                ActorPreviousPosition(snapshot.position),
                snapshot.position,
                snapshot.velocity,
            ),
        )
    }

    pub(crate) fn actor_graphics(&self, interpolation: f32) -> Vec<SceneActorGraphics> {
        self.world
            .iter_entities()
            .filter_map(|entity| {
                let physical: &ActorPhysical = entity.get::<ActorPhysical>()?;
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
}
