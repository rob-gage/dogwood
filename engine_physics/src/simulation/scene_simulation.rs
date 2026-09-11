// Copyright Rob Gage 2026

use super::ScenePhysicsWorld;
use crate::{
    actors::{
        ActorControlState,
        ActorPawn,
        ActorPawnMovement,
        ActorPawnNoclipConfiguration,
        ActorPawnWalkingConfiguration,
        ActorPawnWalkingState,
    },
    scenes::{Scene, ScenePosition, SceneVelocity},
};
use rapier2d::{
    control::{CharacterLength, KinematicCharacterController},
    prelude::{
        Cuboid,
        Pose,
        Vector,
    },
};

/// Implements the fixed-rate systems that advance a `Scene`
pub trait SceneSimulation {

    /// Advances every configured Dogwood actor pawn by one fixed simulation step
    fn simulate_actor_pawns(
        world: &mut bevy_ecs::world::World,
        delta_time: f32,
        is_simulation_active: bool,
        gravity: [f32; 2],
        physics_world: &ScenePhysicsWorld,
    ) {
        let mut query: bevy_ecs::query::QueryState<(
            &ActorPawn,
            &ActorControlState,
            &mut ActorPawnWalkingState,
            &mut ScenePosition,
            &mut SceneVelocity,
        )> = world.query();
        for (pawn, control, mut state, mut position, mut velocity) in
                query.iter_mut(world) {
            if !is_simulation_active && !pawn.simulate_when_paused { continue; }
            match pawn.movement {
                Some(ActorPawnMovement::Noclip) => {
                    let Some(configuration): Option<ActorPawnNoclipConfiguration> = pawn.noclip
                        else { continue; };
                    velocity.x = control.0.locomotion_x * configuration.speed;
                    velocity.y = control.0.locomotion_y * configuration.speed;
                    Self::integrate_actor_position(&mut position, &velocity, delta_time);
                }
                Some(ActorPawnMovement::Walking) => {
                    let Some(configuration): Option<ActorPawnWalkingConfiguration> = pawn.walking
                        else { continue; };
                    Self::simulate_actor_pawn_walking(
                        &control.0,
                        &configuration,
                        &mut state,
                        &mut position,
                        &mut velocity,
                        gravity,
                        physics_world,
                        delta_time,
                    );
                }
                _ => (),
            }
        }
    }

    /// Resolves one walking pawn's desired motion with Rapier's character controller
    fn simulate_actor_pawn_walking(
        control: &engine_input::ControlState,
        configuration: &ActorPawnWalkingConfiguration,
        state: &mut ActorPawnWalkingState,
        position: &mut ScenePosition,
        velocity: &mut SceneVelocity,
        gravity: [f32; 2],
        physics_world: &ScenePhysicsWorld,
        delta_time: f32,
    ) {
        let gravity_magnitude: f32 = gravity[0].hypot(gravity[1]);
        let up: Vector = if gravity_magnitude > 0.0 {
            Vector::new(-gravity[0] / gravity_magnitude, -gravity[1] / gravity_magnitude)
        } else {
            Vector::Y
        };
        let tangent: Vector = Vector::new(up.y, -up.x);
        let tangent_velocity: f32 = velocity.x * tangent.x + velocity.y * tangent.y;
        let target: f32 = control.locomotion_x * configuration.speed;
        let change: f32 = (target - tangent_velocity).clamp(
            -configuration.acceleration * delta_time,
            configuration.acceleration * delta_time,
        );
        velocity.x += tangent.x * change + gravity[0] * delta_time;
        velocity.y += tangent.y * change + gravity[1] * delta_time;
        if control.locomotion_y > 0.0 && state.grounded {
            let up_velocity: f32 = velocity.x * up.x + velocity.y * up.y;
            velocity.x += up.x * (configuration.jump_velocity - up_velocity);
            velocity.y += up.y * (configuration.jump_velocity - up_velocity);
        }
        let character_shape: Cuboid = Cuboid::new(Vector::new(
            configuration.collider_width * 0.5,
            configuration.collider_height * 0.5,
        ));
        let controller: KinematicCharacterController = KinematicCharacterController {
            up,
            offset: CharacterLength::Absolute(1.0 / 4096.0),
            snap_to_ground: None,
            autostep: None,
            ..Default::default()
        };
        let world_x: f32 = position.tile_coordinates.x as f32 + position.x_offset;
        let world_y: f32 = position.tile_coordinates.y as f32 + position.y_offset;
        let movement = physics_world.move_character(
            &controller,
            delta_time,
            &character_shape,
            &Pose::translation(world_x, world_y),
            Vector::new(velocity.x, velocity.y) * delta_time,
            |_| { },
        );
        Self::integrate_actor_position(
            position,
            &SceneVelocity {
                x: movement.translation.x,
                y: movement.translation.y,
            },
            1.0,
        );
        velocity.x = movement.translation.x / delta_time;
        velocity.y = movement.translation.y / delta_time;
        state.grounded = movement.grounded;
    }

    /// Integrates an actor's continuous velocity and normalizes its tile-relative position
    fn integrate_actor_position(
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

}

impl SceneSimulation for Scene { }
