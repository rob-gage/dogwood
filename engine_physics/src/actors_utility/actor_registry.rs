// Copyright Rob Gage 2026

use super::{
    actor_cellular_proxy_state::ActorCellularProxyState,
    actor_collision_shape::ActorCollisionShape, actor_physics_proxy_state::ActorPhysicsProxyState,
};
use crate::{
    actors::{
        Actor, ActorControlState, ActorPawn, ActorPawnMovement, ActorPawnSwimmingConfiguration,
        ActorPawnSwimmingState, ActorPawnWalkingState, ActorPossessable, ActorPreviousPosition,
    },
    scenes::{Scene, ScenePosition, SceneVelocity},
    simulation::{ScenePhysicsWorld, SceneSimulation},
};
/// Owns the ECS world and provides the engine's actor-facing API.
pub struct ActorRegistry {
    /// ECS entities and their actor components.
    world: bevy_ecs::world::World,
}

impl Default for ActorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ActorRegistry {
    /// Creates an empty `ActorRegistry`
    pub fn new() -> Self {
        Self {
            world: bevy_ecs::world::World::new(),
        }
    }

    /// Creates an actor with a `ScenePosition`
    pub fn spawn(&mut self, position: ScenePosition) -> Actor {
        Actor::from_bevy_entity(
            self.world
                .spawn((ActorPreviousPosition(position), position))
                .id(),
        )
    }

    /// Creates a pawn with a position and velocity
    pub fn spawn_pawn(
        &mut self,
        pawn: ActorPawn,
        position: ScenePosition,
        velocity: SceneVelocity,
    ) -> Actor {
        Self::validate_pawn(&pawn);
        Actor::from_bevy_entity(
            self.world
                .spawn((
                    pawn,
                    ActorControlState::default(),
                    ActorPawnSwimmingState::default(),
                    ActorPawnWalkingState::default(),
                    ActorPreviousPosition(position),
                    position,
                    velocity,
                ))
                .id(),
        )
    }

    /// Creates a pawn that is eligible for possession
    pub fn spawn_possessable_pawn(
        &mut self,
        pawn: ActorPawn,
        position: ScenePosition,
        velocity: SceneVelocity,
    ) -> Actor {
        Self::validate_pawn(&pawn);
        Actor::from_bevy_entity(
            self.world
                .spawn((
                    pawn,
                    ActorPossessable,
                    ActorControlState::default(),
                    ActorPawnSwimmingState::default(),
                    ActorPawnWalkingState::default(),
                    ActorPreviousPosition(position),
                    position,
                    velocity,
                ))
                .id(),
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

    /// Returns an actor's pawn configuration
    pub fn get_pawn(&self, identifier: Actor) -> Option<&ActorPawn> {
        self.world.get::<ActorPawn>(identifier.bevy_entity())
    }

    /// Returns an actor position interpolated between its latest fixed ticks
    pub fn get_render_position(
        &self,
        identifier: Actor,
        interpolation: f32,
    ) -> Option<ScenePosition> {
        let position: ScenePosition = *self.get_position(identifier)?;
        let previous: ScenePosition = self
            .world
            .get::<ActorPreviousPosition>(identifier.bevy_entity())
            .map_or(position, |previous| previous.0);
        Some(position.interpolated(previous, interpolation))
    }

    /// Returns the first physical pawn's position and nominal dimensions for scene rendering
    pub fn first_walking_pawn_graphics(&self, interpolation: f32) -> Option<([f32; 2], [f32; 2])> {
        self.world.iter_entities().find_map(|entity| {
            let pawn: &ActorPawn = entity.get::<ActorPawn>()?;
            let shape: ActorCollisionShape = pawn.collision_shape?;
            let position: ScenePosition = *entity.get::<ScenePosition>()?;
            let previous: ScenePosition = entity
                .get::<ActorPreviousPosition>()
                .map_or(position, |previous| previous.0);
            let position: ScenePosition = position.interpolated(previous, interpolation);
            Some((
                [
                    position.tile_coordinates.x as f32 + position.x_offset,
                    position.tile_coordinates.y as f32 + position.y_offset,
                ],
                shape.nominal_dimensions(),
            ))
        })
    }

    /// Gathers all physical non-noclip pawns for batched transient proxy rasterization
    pub(crate) fn cellular_proxy_states(&self) -> Vec<ActorCellularProxyState> {
        self.world
            .iter_entities()
            .filter_map(|entity| {
                let pawn: &ActorPawn = entity.get::<ActorPawn>()?;
                let shape: ActorCollisionShape = pawn.collision_shape?;
                if matches!(pawn.movement, Some(ActorPawnMovement::Noclip)) {
                    return None;
                }
                let position: ScenePosition = *entity.get::<ScenePosition>()?;
                let velocity: SceneVelocity = *entity.get::<SceneVelocity>()?;
                let state: &ActorPawnWalkingState = entity.get::<ActorPawnWalkingState>()?;
                Some(ActorCellularProxyState {
                    center: [
                        position.tile_coordinates.x as f32 + position.x_offset,
                        position.tile_coordinates.y as f32 + position.y_offset,
                    ],
                    velocity: [velocity.x, velocity.y],
                    drive: if matches!(pawn.movement, Some(ActorPawnMovement::Walking)) {
                        state.cellular_drive_impulse
                    } else {
                        [0.0; 2]
                    },
                    shape,
                    occupancy_kind: if pawn.swimming.is_some() { 2 } else { 1 },
                    mass: pawn.walking.map_or(0.0, |configuration| configuration.mass),
                })
            })
            .collect()
    }

    pub(crate) fn physics_proxy_states(&self) -> Vec<ActorPhysicsProxyState> {
        self.world
            .iter_entities()
            .filter_map(|entity| {
                let pawn: &ActorPawn = entity.get::<ActorPawn>()?;
                let shape: ActorCollisionShape = pawn.collision_shape?;
                if matches!(pawn.movement, Some(ActorPawnMovement::Noclip)) {
                    return None;
                }
                let position: ScenePosition = *entity.get::<ScenePosition>()?;
                Some(ActorPhysicsProxyState {
                    actor: Actor::from_bevy_entity(entity.id()),
                    center: [
                        position.tile_coordinates.x as f32 + position.x_offset,
                        position.tile_coordinates.y as f32 + position.y_offset,
                    ],
                    shape,
                })
            })
            .collect()
    }

    /// Returns the possessed swimming-capable pawn shape to sample on the Accelerator
    pub fn swimming_pawn_sample(
        &self,
        identifier: Actor,
    ) -> Option<([f32; 2], ActorCollisionShape)> {
        let entity: bevy_ecs::world::EntityRef<'_> =
            self.world.get_entity(identifier.bevy_entity()).ok()?;
        let pawn: &ActorPawn = entity.get::<ActorPawn>()?;
        pawn.swimming?;
        let shape: ActorCollisionShape = pawn.collision_shape?;
        let position: ScenePosition = *entity.get::<ScenePosition>()?;
        Some((
            [
                position.tile_coordinates.x as f32 + position.x_offset,
                position.tile_coordinates.y as f32 + position.y_offset,
            ],
            shape,
        ))
    }

    /// Applies one completed derived-fluid sample and its walking/swimming hysteresis
    pub fn apply_swimming_sample(&mut self, identifier: Actor, sample: [f32; 5]) -> bool {
        let entity: bevy_ecs::entity::Entity = identifier.bevy_entity();
        let Some(pawn) = self.world.get::<ActorPawn>(entity) else {
            return false;
        };
        let Some(configuration) = pawn.swimming else {
            return false;
        };
        let movement: Option<ActorPawnMovement> = pawn.movement;
        let has_walking: bool = pawn.walking.is_some();
        {
            let Some(mut state): Option<bevy_ecs::world::Mut<'_, ActorPawnSwimmingState>> =
                self.world.get_mut::<ActorPawnSwimmingState>(entity)
            else {
                return false;
            };
            state.immersion = sample[0].clamp(0.0, 1.0);
            state.fluid_velocity = [sample[1], sample[2]];
            state.fluid_density = sample[3].max(0.0);
            state.fluid_viscosity = sample[4].max(0.0);
        }
        let movement: Option<ActorPawnMovement> = match movement {
            Some(ActorPawnMovement::Walking) if sample[0] >= configuration.enter_immersion => {
                Some(ActorPawnMovement::Swimming)
            }
            Some(ActorPawnMovement::Swimming) if sample[0] < configuration.exit_immersion => {
                has_walking
                    .then_some(ActorPawnMovement::Walking)
                    .or(Some(ActorPawnMovement::Swimming))
            }
            _ => return true,
        };
        self.world.get_mut::<ActorPawn>(entity).unwrap().movement = movement;
        true
    }

    #[cfg(test)]
    pub(crate) fn set_movement_for_test(
        &mut self,
        identifier: Actor,
        movement: ActorPawnMovement,
    ) -> bool {
        self.world
            .get_mut::<ActorPawn>(identifier.bevy_entity())
            .map(|mut pawn| pawn.movement = Some(movement))
            .is_some()
    }

    /// Sets an actor's position
    pub fn set_position(&mut self, identifier: Actor, position: ScenePosition) -> bool {
        let entity: bevy_ecs::entity::Entity = identifier.bevy_entity();
        let is_set: bool = self
            .world
            .get_mut::<ScenePosition>(entity)
            .map(|mut current| *current = position)
            .is_some();
        if is_set && let Some(mut previous) = self.world.get_mut::<ActorPreviousPosition>(entity) {
            previous.0 = position;
        }
        is_set
    }

    /// Sets an actor's velocity
    pub fn set_velocity(&mut self, identifier: Actor, velocity: SceneVelocity) -> bool {
        self.world
            .get_mut::<SceneVelocity>(identifier.bevy_entity())
            .map(|mut current| *current = velocity)
            .is_some()
    }

    /// Passes universal controls to a pawn
    pub fn set_control_state(
        &mut self,
        identifier: Actor,
        control_state: ActorControlState,
    ) -> bool {
        if self
            .world
            .get::<ActorPawn>(identifier.bevy_entity())
            .is_none()
        {
            return false;
        }
        self.world
            .entity_mut(identifier.bevy_entity())
            .insert(control_state);
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
        self.world
            .get::<ActorPossessable>(identifier.bevy_entity())
            .is_some()
    }

    /// Rejects invalid configured swimming parameters before inserting a pawn
    fn validate_pawn(pawn: &ActorPawn) {
        if let Some(shape) = pawn.collision_shape {
            assert!(shape.is_valid());
        }
        if let Some(configuration) = pawn.walking {
            assert!(configuration.mass.is_finite() && configuration.mass > 0.0);
        }
        let Some(ActorPawnSwimmingConfiguration {
            maximum_speed,
            acceleration,
            density,
            drag,
            enter_immersion,
            exit_immersion,
        }) = pawn.swimming
        else {
            return;
        };
        assert!(maximum_speed.is_finite() && maximum_speed >= 0.0);
        assert!(acceleration.is_finite() && acceleration >= 0.0);
        assert!(density.is_finite() && density > 0.0);
        assert!(drag.is_finite() && drag >= 0.0);
        assert!(enter_immersion.is_finite() && (0.0..=1.0).contains(&enter_immersion));
        assert!(exit_immersion.is_finite() && (0.0..=1.0).contains(&exit_immersion));
        assert!(exit_immersion < enter_immersion);
    }
}
