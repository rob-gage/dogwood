// Copyright Rob Gage 2026

use super::{
    Actor,
    ActorControlState,
    ActorPawn,
    ActorPawnMovement,
    ActorPawnNoclipConfiguration,
    ActorPossessable,
};
use crate::scenes::{
    ScenePosition,
    SceneVelocity,
};

/// Owns the ECS world and provides the engine's actor-facing API
pub struct ActorRegistry {
    world: bevy_ecs::world::World,
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::{
        actors::ActorPawnNoclipConfiguration,
        tiles::TileCoordinates,
    };

    /// Creates a noclip pawn configuration for movement tests
    fn noclip_pawn(simulate_when_paused: bool) -> ActorPawn {
        ActorPawn {
            walking: None,
            flying: None,
            swimming: None,
            noclip: Some(ActorPawnNoclipConfiguration { speed: 6.0 }),
            movement: Some(ActorPawnMovement::Noclip),
            simulate_when_paused,
        }
    }

    #[test]
    fn noclip_uses_controls_and_can_run_while_paused() {
        let mut registry: ActorRegistry = ActorRegistry::new();
        let actor: Actor = registry.spawn_pawn(
            noclip_pawn(true),
            ScenePosition {
                tile_coordinates: TileCoordinates { x: 0, y: 0 },
                x_offset: 0.5,
                y_offset: 0.5,
            },
            SceneVelocity { x: 0.0, y: 0.0 },
        );
        registry.set_control_state(actor, ActorControlState(engine_input::ControlState {
            locomotion_x: 1.0,
            locomotion_y: -0.5,
        }));
        registry.simulate_pawns(0.5, false);
        let position: ScenePosition = *registry.get_position(actor).unwrap();
        let velocity: SceneVelocity = *registry.get_velocity(actor).unwrap();
        assert_eq!(position.tile_coordinates.x, 3);
        assert_eq!(position.tile_coordinates.y, -1);
        assert_eq!(position.x_offset, 0.5);
        assert_eq!(position.y_offset, 0.0);
        assert_eq!(velocity.x, 6.0);
        assert_eq!(velocity.y, -3.0);

        registry.clear_control_state(actor);
        registry.simulate_pawns(0.5, false);
        let stopped: ScenePosition = *registry.get_position(actor).unwrap();
        assert_eq!(stopped.tile_coordinates.x, 3);
        assert_eq!(stopped.tile_coordinates.y, -1);
        assert_eq!(stopped.x_offset, 0.5);
        assert_eq!(stopped.y_offset, 0.0);

        let paused_actor: Actor = registry.spawn_pawn(
            noclip_pawn(false),
            ScenePosition {
                tile_coordinates: TileCoordinates { x: 0, y: 0 },
                x_offset: 0.5,
                y_offset: 0.5,
            },
            SceneVelocity { x: 0.0, y: 0.0 },
        );
        registry.set_control_state(paused_actor, ActorControlState(engine_input::ControlState {
            locomotion_x: 1.0,
            locomotion_y: 0.0,
        }));
        registry.simulate_pawns(1.0, false);
        let paused_position: ScenePosition = *registry.get_position(paused_actor).unwrap();
        assert_eq!(paused_position.tile_coordinates.x, 0);
        assert_eq!(paused_position.x_offset, 0.5);
    }

}

impl ActorRegistry {

    /// Creates an empty `ActorRegistry`
    pub fn new() -> Self { Self { world: bevy_ecs::world::World::new() } }

    /// Creates an actor with a `ScenePosition`
    pub fn spawn(&mut self, position: ScenePosition) -> Actor {
        Actor::from_bevy_entity(self.world.spawn(position).id())
    }

    /// Creates a pawn with a position and velocity
    pub fn spawn_pawn(
        &mut self,
        pawn: ActorPawn,
        position: ScenePosition,
        velocity: SceneVelocity,
    ) -> Actor {
        Actor::from_bevy_entity(
            self.world.spawn((
                pawn,
                ActorControlState::default(),
                position,
                velocity,
            )).id()
        )
    }

    /// Creates a pawn that is eligible for possession
    pub fn spawn_possessable_pawn(
        &mut self,
        pawn: ActorPawn,
        position: ScenePosition,
        velocity: SceneVelocity,
    ) -> Actor {
        Actor::from_bevy_entity(
            self.world.spawn((
                pawn,
                ActorPossessable,
                ActorControlState::default(),
                position,
                velocity,
            )).id()
        )
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

    /// Sets an actor's position
    pub fn set_position(
        &mut self,
        identifier: Actor,
        position: ScenePosition,
    ) -> bool {
        self.world.get_mut::<ScenePosition>(identifier.bevy_entity())
            .map(|mut current| *current = position)
            .is_some()
    }

    /// Sets an actor's velocity
    pub fn set_velocity(
        &mut self,
        identifier: Actor,
        velocity: SceneVelocity,
    ) -> bool {
        self.world.get_mut::<SceneVelocity>(identifier.bevy_entity())
            .map(|mut current| *current = velocity)
            .is_some()
    }

    /// Passes universal controls to a pawn
    pub fn set_control_state(
        &mut self,
        identifier: Actor,
        control_state: ActorControlState,
    ) -> bool {
        if self.world.get::<ActorPawn>(identifier.bevy_entity()).is_none() {
            return false;
        }
        self.world.entity_mut(identifier.bevy_entity())
            .insert(control_state);
        true
    }

    /// Clears the universal controls assigned to a pawn
    pub fn clear_control_state(&mut self, identifier: Actor) -> bool {
        self.set_control_state(identifier, ActorControlState::default())
    }

    /// Advances configured pawn movement by one fixed simulation step
    pub fn simulate_pawns(&mut self, delta_time: f32, is_simulation_active: bool) {
        let mut query: bevy_ecs::query::QueryState<(
            &ActorPawn,
            &ActorControlState,
            &mut ScenePosition,
            &mut SceneVelocity,
        )> = self.world.query();
        for (pawn, control, mut position, mut velocity) in query.iter_mut(&mut self.world) {
            if !is_simulation_active && !pawn.simulate_when_paused { continue; }
            let Some(ActorPawnMovement::Noclip): Option<ActorPawnMovement> = pawn.movement
                else { continue; };
            let Some(configuration): Option<ActorPawnNoclipConfiguration> = pawn.noclip
                else { continue; };
            velocity.x = control.0.locomotion_x * configuration.speed;
            velocity.y = control.0.locomotion_y * configuration.speed;
            Self::move_position(&mut position, &velocity, delta_time);
        }
    }

    /// Applies continuous velocity while keeping a position normalized to its containing tile
    fn move_position(
        position: &mut ScenePosition,
        velocity: &SceneVelocity,
        delta_time: f32,
    ) {
        let x: f32 = position.tile_coordinates.x as f32 + position.x_offset +
            velocity.x * delta_time;
        let y: f32 = position.tile_coordinates.y as f32 + position.y_offset +
            velocity.y * delta_time;
        let tile_x: f32 = x.floor();
        let tile_y: f32 = y.floor();
        position.tile_coordinates.x = tile_x as i32;
        position.tile_coordinates.y = tile_y as i32;
        position.x_offset = x - tile_x;
        position.y_offset = y - tile_y;
    }

    /// Returns whether an actor is eligible for possession
    pub fn is_possessable(&self, identifier: Actor) -> bool {
        self.world.get::<ActorPossessable>(identifier.bevy_entity()).is_some()
    }

}
