// Copyright Rob Gage 2026

use super::{
    actor_cellular_proxy_state::ActorCellularProxyState,
    actor_collision_shape::ActorCollisionShape, actor_physical::ActorPhysical,
    actor_physical_proxy_state::ActorPhysicalProxyState,
    actor_physics_proxy_state::ActorPhysicsProxyState,
};
use crate::{
    actors::{
        Actor, ActorControlState, ActorPawn, ActorPawnMovement, ActorPawnSwimmingConfiguration,
        ActorPawnSwimmingState, ActorPawnWalkingState, ActorPhysicalConfiguration,
        ActorPossessable, ActorPreviousPosition,
    },
    scenes::{Scene, ScenePosition, SceneVelocity},
    simulation::ScenePhysicsWorld,
    simulation_actors::SceneSimulation,
};
use engine_graphics::SceneActorGraphics;
use std::collections::HashMap;
/// Owns the ECS world and provides the engine's actor-facing API.
pub struct ActorRegistry {
    /// ECS entities and their actor components.
    world: bevy_ecs::world::World,
    entities: HashMap<Actor, bevy_ecs::entity::Entity>,
    next_identifier: u64,
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
            entities: HashMap::new(),
            next_identifier: 0,
        }
    }

    fn bevy_entity(&self, actor: Actor) -> bevy_ecs::entity::Entity {
        self.entities[&actor]
    }

    fn spawn_components(&mut self, components: impl bevy_ecs::bundle::Bundle) -> Actor {
        let entity: bevy_ecs::entity::Entity = self.world.spawn(components).id();
        let actor: Actor = Actor::new(self.next_identifier);
        self.next_identifier = self.next_identifier.wrapping_add(1);
        self.entities.insert(actor, entity);
        actor
    }

    /// Creates an actor with a `ScenePosition`
    pub fn spawn(&mut self, position: ScenePosition) -> Actor {
        self.spawn_components((ActorPreviousPosition(position), position))
    }

    /// Creates a pawn with a position and velocity
    pub fn spawn_pawn(
        &mut self,
        pawn: ActorPawn,
        position: ScenePosition,
        velocity: SceneVelocity,
    ) -> Actor {
        Self::validate_pawn(&pawn);
        self.spawn_components((
            pawn,
            ActorControlState::default(),
            ActorPawnSwimmingState::default(),
            ActorPawnWalkingState::default(),
            ActorPreviousPosition(position),
            position,
            velocity,
        ))
    }

    /// Creates a dynamic physical actor with the supplied initial state.
    pub fn spawn_physical_actor(
        &mut self,
        configuration: ActorPhysicalConfiguration,
        position: ScenePosition,
        velocity: SceneVelocity,
    ) -> Actor {
        assert!(configuration.collision_shape.is_valid());
        assert!(configuration.mass.is_finite() && configuration.mass > 0.0);
        self.spawn_components((
            ActorPhysical(configuration),
            ActorPreviousPosition(position),
            position,
            velocity,
        ))
    }

    /// Creates a pawn that is eligible for possession
    pub fn spawn_possessable_pawn(
        &mut self,
        pawn: ActorPawn,
        position: ScenePosition,
        velocity: SceneVelocity,
    ) -> Actor {
        Self::validate_pawn(&pawn);
        self.spawn_components((
            pawn,
            ActorPossessable,
            ActorControlState::default(),
            ActorPawnSwimmingState::default(),
            ActorPawnWalkingState::default(),
            ActorPreviousPosition(position),
            position,
            velocity,
        ))
    }

    /// Removes an actor from the registry, returning true if successful
    pub fn despawn(&mut self, identifier: Actor) -> bool {
        let Some(entity) = self.entities.remove(&identifier) else {
            return false;
        };
        self.world.despawn(entity)
    }

    /// Returns `true` if this `ActorRegistry` contains this `Actor`
    pub fn contains(&self, identifier: Actor) -> bool {
        self.entities
            .get(&identifier)
            .is_some_and(|entity| self.world.get_entity(*entity).is_ok())
    }

    /// Returns an actor's position
    pub fn get_position(&self, identifier: Actor) -> Option<&ScenePosition> {
        self.world
            .get::<ScenePosition>(self.bevy_entity(identifier))
    }

    /// Returns an actor's velocity
    pub fn get_velocity(&self, identifier: Actor) -> Option<&SceneVelocity> {
        self.world
            .get::<SceneVelocity>(self.bevy_entity(identifier))
    }

    /// Returns an actor's pawn configuration
    pub fn get_pawn(&self, identifier: Actor) -> Option<&ActorPawn> {
        self.world.get::<ActorPawn>(self.bevy_entity(identifier))
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
            .get::<ActorPreviousPosition>(self.bevy_entity(identifier))
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
                    actor: self
                        .entities
                        .iter()
                        .find_map(|(actor, current)| (*current == entity.id()).then_some(*actor))
                        .unwrap(),
                    center: [
                        position.tile_coordinates.x as f32 + position.x_offset,
                        position.tile_coordinates.y as f32 + position.y_offset,
                    ],
                    shape,
                })
            })
            .collect()
    }

    pub(crate) fn physical_proxy_states(&self) -> Vec<ActorPhysicalProxyState> {
        self.world
            .iter_entities()
            .filter_map(|entity| {
                let actor_physical: &ActorPhysical = entity.get::<ActorPhysical>()?;
                let actor_scene_position: ScenePosition = *entity.get::<ScenePosition>()?;
                let actor_scene_velocity: SceneVelocity = *entity.get::<SceneVelocity>()?;
                let actor_identifier: Actor = self
                    .entities
                    .iter()
                    .find_map(|(actor, current)| (*current == entity.id()).then_some(*actor))?;
                Some(ActorPhysicalProxyState {
                    actor: actor_identifier,
                    center: [
                        actor_scene_position.tile_coordinates.x as f32
                            + actor_scene_position.x_offset,
                        actor_scene_position.tile_coordinates.y as f32
                            + actor_scene_position.y_offset,
                    ],
                    velocity: [actor_scene_velocity.x, actor_scene_velocity.y],
                    shape: actor_physical.0.collision_shape,
                    mass: actor_physical.0.mass,
                    friction: actor_physical.0.friction,
                    restitution: actor_physical.0.restitution,
                })
            })
            .collect()
    }

    pub(crate) fn apply_physical_proxy_states(&mut self, states: &[(Actor, [f32; 2], [f32; 2])]) {
        for (actor, center, velocity) in states {
            let Some(entity) = self.entities.get(actor).copied() else {
                continue;
            };
            if let Some(mut position) = self.world.get_mut::<ScenePosition>(entity) {
                position.tile_coordinates.x = center[0].floor() as i32;
                position.tile_coordinates.y = center[1].floor() as i32;
                position.x_offset = center[0] - center[0].floor();
                position.y_offset = center[1] - center[1].floor();
            }
            if let Some(mut current) = self.world.get_mut::<SceneVelocity>(entity) {
                *current = SceneVelocity {
                    x: velocity[0],
                    y: velocity[1],
                };
            }
        }
    }

    /// Returns the possessed swimming-capable pawn shape to sample on the Accelerator
    pub fn swimming_pawn_sample(
        &self,
        identifier: Actor,
    ) -> Option<([f32; 2], ActorCollisionShape)> {
        let entity: bevy_ecs::world::EntityRef<'_> =
            self.world.get_entity(self.bevy_entity(identifier)).ok()?;
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
        let entity: bevy_ecs::entity::Entity = self.bevy_entity(identifier);
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
            .get_mut::<ActorPawn>(self.bevy_entity(identifier))
            .map(|mut pawn| pawn.movement = Some(movement))
            .is_some()
    }

    /// Sets an actor's position
    pub fn set_position(&mut self, identifier: Actor, position: ScenePosition) -> bool {
        let entity: bevy_ecs::entity::Entity = self.bevy_entity(identifier);
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
            .get_mut::<SceneVelocity>(self.bevy_entity(identifier))
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
            .get::<ActorPawn>(self.bevy_entity(identifier))
            .is_none()
        {
            return false;
        }
        self.world
            .entity_mut(self.bevy_entity(identifier))
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
        Scene::simulate_actors_pawns(
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
            .get::<ActorPossessable>(self.bevy_entity(identifier))
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
