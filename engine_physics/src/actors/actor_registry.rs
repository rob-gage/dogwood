// Copyright Rob Gage 2026

use super::{
    Actor,
    ActorControlState,
    ActorPawn,
    ActorPawnWalkingConfiguration,
    ActorPossessable,
    ActorPawnWalkingState,
    ActorPreviousPosition,
};
use crate::{
    scenes::{
        Scene,
        ScenePosition,
        SceneVelocity
    },
    simulation::{
        ScenePhysicsWorld,
        SceneSimulation
    }
};

/// Owns the ECS world and provides the engine's actor-facing API
pub struct ActorRegistry {
    world: bevy_ecs::world::World,
}

impl ActorRegistry {

    /// Creates an empty `ActorRegistry`
    pub fn new() -> Self { Self { world: bevy_ecs::world::World::new() } }

    /// Creates an actor with a `ScenePosition`
    pub fn spawn(&mut self, position: ScenePosition) -> Actor {
        Actor::from_bevy_entity(self.world.spawn((
            ActorPreviousPosition(position),
            position,
        )).id())
    }

    /// Creates a pawn with a position and velocity
    pub fn spawn_pawn(
        &mut self,
        pawn: ActorPawn,
        position: ScenePosition,
        velocity: SceneVelocity,
    ) -> Actor {
        Actor::from_bevy_entity(self.world.spawn((
            pawn,
            ActorControlState::default(),
            ActorPawnWalkingState::default(),
            ActorPreviousPosition(position),
            position,
            velocity,
        )).id())
    }

    /// Creates a pawn that is eligible for possession
    pub fn spawn_possessable_pawn(
        &mut self,
        pawn: ActorPawn,
        position: ScenePosition,
        velocity: SceneVelocity,
    ) -> Actor {
        Actor::from_bevy_entity(self.world.spawn((
            pawn,
            ActorPossessable,
            ActorControlState::default(),
            ActorPawnWalkingState::default(),
            ActorPreviousPosition(position),
            position,
            velocity,
        )).id())
    }

    /// Removes an actor from the registry, returning true if successful
    pub fn despawn(&mut self, identifier: Actor) -> bool {
        self.world.despawn(identifier.bevy_entity())
    }

    /// Returns `true` if this `ActorRegistry` contains this `Actor`
    pub fn contains(&self, identifier: Actor) -> bool {
        self.world.get_entity(identifier.bevy_entity()).is_ok()
    }

    /// Returns an actor's position
    pub fn get_position(&self, identifier: Actor) -> Option<&ScenePosition> {
        self.world.get::<ScenePosition>(identifier.bevy_entity())
    }

    /// Returns an actor's velocity
    pub fn get_velocity(&self, identifier: Actor) -> Option<&SceneVelocity> {
        self.world.get::<SceneVelocity>(identifier.bevy_entity())
    }

    /// Returns an actor's pawn configuration
    pub fn get_pawn(&self, identifier: Actor) -> Option<&ActorPawn> {
        self.world.get::<ActorPawn>(identifier.bevy_entity())
    }

    /// Returns an actor position interpolated between its latest fixed ticks
    pub(crate) fn get_render_position(
        &self,
        identifier: Actor,
        interpolation: f32,
    ) -> Option<ScenePosition> {
        let position: ScenePosition = *self.get_position(identifier)?;
        let previous: ScenePosition = self.world.get::<ActorPreviousPosition>(
            identifier.bevy_entity(),
        ).map_or(position, |previous| previous.0);
        Some(position.interpolated(previous, interpolation))
    }

    /// Returns the first walking pawn's position and collider dimensions for scene rendering
    pub fn first_walking_pawn_graphics(
        &self,
        interpolation: f32,
    ) -> Option<([f32; 2], [f32; 2])> {
        self.world.iter_entities().find_map(|entity| {
            let pawn: &ActorPawn = entity.get::<ActorPawn>()?;
            let walking: ActorPawnWalkingConfiguration = pawn.walking?;
            let position: ScenePosition = *entity.get::<ScenePosition>()?;
            let previous: ScenePosition = entity.get::<ActorPreviousPosition>()
                .map_or(position, |previous| previous.0);
            let position: ScenePosition = position.interpolated(previous, interpolation);
            Some((
                [
                    position.tile_coordinates.x as f32 + position.x_offset,
                    position.tile_coordinates.y as f32 + position.y_offset,
                ],
                [walking.collider_width, walking.collider_height],
            ))
        })
    }

    /// Sets an actor's position
    pub fn set_position(&mut self, identifier: Actor, position: ScenePosition) -> bool {
        let entity: bevy_ecs::entity::Entity = identifier.bevy_entity();
        let is_set: bool = self.world.get_mut::<ScenePosition>(entity)
            .map(|mut current| *current = position).is_some();
        if is_set && let Some(mut previous) = self.world.get_mut::<ActorPreviousPosition>(entity) {
            previous.0 = position;
        }
        is_set
    }

    /// Sets an actor's velocity
    pub fn set_velocity(&mut self, identifier: Actor, velocity: SceneVelocity) -> bool {
        self.world.get_mut::<SceneVelocity>(identifier.bevy_entity())
            .map(|mut current| *current = velocity).is_some()
    }

    /// Passes universal controls to a pawn
    pub fn set_control_state(
        &mut self,
        identifier: Actor,
        control_state: ActorControlState,
    ) -> bool {
        if self.world.get::<ActorPawn>(identifier.bevy_entity()).is_none() { return false; }
        self.world.entity_mut(identifier.bevy_entity()).insert(control_state);
        true
    }

    /// Clears the universal controls assigned to a pawn
    pub fn clear_control_state(&mut self, identifier: Actor) -> bool {
        self.set_control_state(identifier, ActorControlState::default())
    }

    /// Advances configured pawn movement by one fixed simulation step
    pub fn simulate_actor_pawns(
        &mut self,
        delta_time: f32,
        is_simulation_active: bool,
        gravity: [f32; 2],
        physics_world: &ScenePhysicsWorld,
    ) {
        Scene::simulate_actor_pawns(
            &mut self.world,
            delta_time,
            is_simulation_active,
            gravity,
            physics_world,
        );
    }

    /// Returns whether an actor is eligible for possession
    pub fn is_possessable(&self, identifier: Actor) -> bool {
        self.world.get::<ActorPossessable>(identifier.bevy_entity()).is_some()
    }

}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::actors::{ActorPawnMovement, ActorPawnNoclipConfiguration};
    use crate::tiles::TileCoordinates;

    #[test]
    fn interpolates_fixed_tick_movement_and_does_not_smear_teleports() {
        let mut registry: ActorRegistry = ActorRegistry::new();
        let mut pawn: ActorPawn = ActorPawn::new();
        pawn.noclip = Some(ActorPawnNoclipConfiguration { speed: 6.0 });
        pawn.movement = Some(ActorPawnMovement::Noclip);
        let actor: Actor = registry.spawn_pawn(
            pawn,
            ScenePosition {
                tile_coordinates: TileCoordinates { x: 0, y: 0 },
                x_offset: 0.0,
                y_offset: 0.0,
            },
            SceneVelocity { x: 0.0, y: 0.0 },
        );
        registry.set_control_state(actor, ActorControlState(engine_input::ControlState {
            locomotion_x: 1.0,
            locomotion_y: 0.0,
        }));
        registry.simulate_actor_pawns(
            1.0 / 60.0,
            true,
            [0.0, 0.0],
            &ScenePhysicsWorld::new(),
        );

        let position: ScenePosition = registry.get_render_position(actor, 0.5).unwrap();
        assert!((position.x_offset - 0.05).abs() < f32::EPSILON * 4.0);

        let teleported: ScenePosition = ScenePosition {
            tile_coordinates: TileCoordinates { x: 10, y: 0 },
            x_offset: 0.25,
            y_offset: 0.5,
        };
        assert!(registry.set_position(actor, teleported));
        let position: ScenePosition = registry.get_render_position(actor, 0.5).unwrap();
        assert!(position.tile_coordinates == teleported.tile_coordinates);
        assert!(position.x_offset == teleported.x_offset);
        assert!(position.y_offset == teleported.y_offset);
    }

}
