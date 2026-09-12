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
        ActorPreviousPosition,
    },
    scenes::{Scene, ScenePosition, SceneVelocity},
};
use rapier2d::{
    control::{CharacterLength, KinematicCharacterController},
    prelude::{
        Capsule,
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
            &mut ActorPreviousPosition,
            &mut ScenePosition,
            &mut SceneVelocity,
        )> = world.query();
        for (pawn, control, mut state, mut previous, mut position, mut velocity) in
                query.iter_mut(world) {
            previous.0 = *position;
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
        if !configuration.collider_width.is_finite() ||
                !configuration.collider_height.is_finite() ||
                configuration.collider_width <= 0.0 ||
                configuration.collider_height <= 0.0 {
            return;
        }
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
        velocity.x += tangent.x * change;
        velocity.y += tangent.y * change;
        if !state.grounded {
            velocity.x += gravity[0] * delta_time;
            velocity.y += gravity[1] * delta_time;
        }
        if control.locomotion_y > 0.0 && state.grounded {
            let up_velocity: f32 = velocity.x * up.x + velocity.y * up.y;
            velocity.x += up.x * (configuration.jump_velocity - up_velocity);
            velocity.y += up.y * (configuration.jump_velocity - up_velocity);
        }
        // capsule cannot be shorter along up than its diameter, so an
        // incompatible configuration safely collapses to a circular capsule.
        let radius: f32 = configuration.collider_width.min(configuration.collider_height) * 0.5;
        let half_segment_length: f32 = (configuration.collider_height - radius * 2.0) * 0.5;
        let character_shape: Capsule = Capsule::new(
            -up * half_segment_length,
            up * half_segment_length,
            radius,
        );
        let controller: KinematicCharacterController = KinematicCharacterController {
            up,
            // a cell is 1/8 tile; this is large enough for stable contact without
            // visibly separating the pawn from cellular terrain
            offset: CharacterLength::Absolute(1.0 / 1024.0),
            max_slope_climb_angle: configuration.maximum_slope_angle,
            min_slope_slide_angle: configuration.maximum_slope_angle,
            snap_to_ground: Some(CharacterLength::Absolute(1.0 / 8.0)),
            autostep: None,
            // clear the contact offset in one iteration before continuing sideways
            normal_nudge_factor: 1.0 / 1024.0,
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
        state.grounded = movement.grounded;
        if state.grounded {
            let velocity_into_ground: f32 = velocity.x * up.x + velocity.y * up.y;
            if velocity_into_ground < 0.0 {
                velocity.x -= up.x * velocity_into_ground;
                velocity.y -= up.y * velocity_into_ground;
            }
        }
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

#[cfg(test)]
mod tests {

    use super::*;
    use crate::{
        simulation::CollisionOccupancySnapshot,
        tiles::TileCoordinates,
    };

    #[test]
    fn walking_motion_remains_uniform_across_collision_tile_boundaries() {
        let mut physics_world: ScenePhysicsWorld = ScenePhysicsWorld::new();
        let mut masks: Vec<[u32; 2]> = vec![[u32::MAX; 2]; 8];
        masks.extend(vec![[0; 2]; 8]);
        physics_world.update_cellular_terrain(CollisionOccupancySnapshot {
            sequence: 0,
            origin: TileCoordinates { x: -4, y: -1 },
            width: 8,
            height: 2,
            masks: masks.into_boxed_slice(),
        });
        let configuration: ActorPawnWalkingConfiguration = ActorPawnWalkingConfiguration {
            speed: 4.0,
            acceleration: 24.0,
            jump_velocity: 7.0,
            maximum_slope_angle: 50.0_f32.to_radians(),
            collider_width: 0.75,
            collider_height: 0.75,
        };
        let control: engine_input::ControlState = engine_input::ControlState {
            locomotion_x: 1.0,
            locomotion_y: 0.0,
        };
        let mut state: ActorPawnWalkingState = ActorPawnWalkingState::default();
        let mut position: ScenePosition = ScenePosition {
            tile_coordinates: TileCoordinates { x: -3, y: 0 },
            x_offset: 0.5,
            y_offset: 0.376,
        };
        let mut velocity: SceneVelocity = SceneVelocity { x: 0.0, y: 0.0 };
        let mut previous_x: f32 = -2.5;
        for tick in 0..80 {
            physics_world.step([0.0, -18.0], 1.0 / 60.0);
            <Scene as SceneSimulation>::simulate_actor_pawn_walking(
                &control,
                &configuration,
                &mut state,
                &mut position,
                &mut velocity,
                [0.0, -18.0],
                &physics_world,
                1.0 / 60.0,
            );
            let x: f32 = position.tile_coordinates.x as f32 + position.x_offset;
            let y: f32 = position.tile_coordinates.y as f32 + position.y_offset;
            if tick >= 10 {
                assert!(
                    x - previous_x > 0.06,
                    "tick {tick}, x {x}, y {y}, velocity [{}, {}], grounded {}, delta {}",
                    velocity.x,
                    velocity.y,
                    state.grounded,
                    x - previous_x,
                );
                assert!(state.grounded, "tick {tick}, y {y}");
                assert!((y - 0.376).abs() < 0.001, "tick {tick}, y {y}");
            }
            previous_x = x;
        }
    }

}
