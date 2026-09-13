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
        let was_grounded: bool = state.grounded;
        velocity.x += tangent.x * change;
        velocity.y += tangent.y * change;
        velocity.x += gravity[0] * delta_time;
        velocity.y += gravity[1] * delta_time;
        let jump_requested: bool = control.locomotion_y > 0.0 && state.grounded;
        if was_grounded && !jump_requested {
            let into_ground: f32 = velocity.x * up.x + velocity.y * up.y;
            if into_ground < 0.0 {
                velocity.x -= up.x * into_ground;
                velocity.y -= up.y * into_ground;
            }
        }
        if jump_requested {
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
        let mut up_velocity: f32 = velocity.x * up.x + velocity.y * up.y;
        let mut tangent_velocity: f32 = velocity.x * tangent.x + velocity.y * tangent.y;
        if was_grounded && control.locomotion_x == 0.0 && tangent_velocity.abs() < 1e-4 {
            tangent_velocity = 0.0;
        }
        let requested_tangent_velocity: f32 = tangent_velocity;
        let mut contacted_wall: bool = false;
        let mut contacted_walkable_surface: bool = false;
        let walkable_normal: f32 = configuration.maximum_slope_angle.cos();
        let mut horizontal_movement = physics_world.move_character(
            &controller,
            delta_time,
            &character_shape,
            &Pose::translation(world_x, world_y),
            tangent * tangent_velocity * delta_time,
            |collision| {
                let normal_up: f32 = collision.hit.normal1.dot(up);
                if normal_up >= walkable_normal {
                    contacted_walkable_surface = true;
                } else if normal_up > -walkable_normal {
                    contacted_wall = true;
                    let normal_tangent: f32 = collision.hit.normal1.dot(tangent);
                    if normal_tangent * tangent_velocity < 0.0 {
                        tangent_velocity = 0.0;
                    }
                }
            },
        );
        // sample a short gravity-relative ramp instead of judging only the immediate wall cell
        if was_grounded && contacted_wall {
            let lookahead: f32 = 2.0 / 8.0;
            let maximum_rise: f32 = lookahead * configuration.maximum_slope_angle.tan();
            let direction: f32 = requested_tangent_velocity.signum();
            let probe_up = physics_world.move_character(
                &controller,
                delta_time,
                &character_shape,
                &Pose::translation(world_x, world_y),
                up * maximum_rise,
                |_| { },
            );
            if maximum_rise.is_finite() && maximum_rise > 0.0 &&
                    probe_up.translation.dot(up) >= maximum_rise - 1.0 / 1024.0 {
                let probe_forward = physics_world.move_character(
                    &controller,
                    delta_time,
                    &character_shape,
                    &Pose::translation(
                        world_x + probe_up.translation.x,
                        world_y + probe_up.translation.y,
                    ),
                    tangent * direction * lookahead,
                    |_| { },
                );
                if probe_forward.translation.dot(tangent) * direction >=
                        lookahead - 1.0 / 1024.0 {
                    let mut landing_is_walkable: bool = false;
                    let probe_down = physics_world.move_character(
                        &controller,
                        delta_time,
                        &character_shape,
                        &Pose::translation(
                            world_x + probe_up.translation.x + probe_forward.translation.x,
                            world_y + probe_up.translation.y + probe_forward.translation.y,
                        ),
                        -up * maximum_rise,
                        |collision| {
                            if collision.hit.normal1.dot(up) >= walkable_normal {
                                landing_is_walkable = true;
                            }
                        },
                    );
                    let rise: f32 = (
                        probe_up.translation + probe_forward.translation + probe_down.translation
                    ).dot(up);
                    if landing_is_walkable && rise > 0.0 &&
                            rise.atan2(lookahead) <= configuration.maximum_slope_angle {
                        let climb_forward = physics_world.move_character(
                            &controller,
                            delta_time,
                            &character_shape,
                            &Pose::translation(
                                world_x + up.x * rise,
                                world_y + up.y * rise,
                            ),
                            tangent * requested_tangent_velocity * delta_time,
                            |_| { },
                        );
                        if climb_forward.translation.dot(tangent).abs() >
                                horizontal_movement.translation.dot(tangent).abs() +
                                    1.0 / 1024.0 {
                            horizontal_movement.translation = up * rise +
                                climb_forward.translation;
                            tangent_velocity = requested_tangent_velocity;
                        }
                    }
                }
            }
        }
        let vertical_movement = physics_world.move_character(
            &controller,
            delta_time,
            &character_shape,
            &Pose::translation(
                world_x + horizontal_movement.translation.x,
                world_y + horizontal_movement.translation.y,
            ),
            up * up_velocity * delta_time,
            |collision| {
                let normal_up: f32 = collision.hit.normal1.dot(up);
                if normal_up >= walkable_normal {
                    contacted_walkable_surface = true;
                } else if normal_up > -walkable_normal {
                    contacted_wall = true;
                } else if up_velocity > 0.0 {
                    up_velocity = 0.0;
                }
            },
        );
        Self::integrate_actor_position(
            position,
            &SceneVelocity {
                x: vertical_movement.translation.x + horizontal_movement.translation.x,
                y: vertical_movement.translation.y + horizontal_movement.translation.y,
            },
            1.0,
        );
        let rapier_grounded: bool = vertical_movement.grounded || horizontal_movement.grounded;
        // wall seams can produce tiny upward normals that Rapier reports as grounded
        state.grounded = contacted_walkable_surface ||
            (rapier_grounded && !contacted_wall);
        velocity.x = up.x * up_velocity + tangent.x * tangent_velocity;
        velocity.y = up.y * up_velocity + tangent.y * tangent_velocity;
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
