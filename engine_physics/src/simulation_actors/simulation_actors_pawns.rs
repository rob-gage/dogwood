// Copyright Rob Gage 2026

use rapier2d::prelude::Vector;

use super::scene_simulation_position::integrate_actor_position;
use super::scene_simulation_swimming::simulate_actor_pawn_swimming;
use crate::actors::ActorCollisionShape;
use crate::actors::ActorControlState;
use crate::actors::ActorPawn;
use crate::actors::ActorPawnMovement;
use crate::actors::ActorPawnNoclipConfiguration;
use crate::actors::ActorPawnSwimmingConfiguration;
use crate::actors::ActorPawnSwimmingState;
use crate::actors::ActorPawnWalkingConfiguration;
use crate::actors::ActorPawnWalkingState;
use crate::actors::ActorPreviousPosition;
use crate::scenes::Scene;
use crate::scenes::ScenePosition;
use crate::scenes::SceneVelocity;
use crate::simulation::ScenePhysicsWorld;

const CELLULAR_DRIVE_TRANSFER: f32 = 0.08;

/// Implements the fixed-rate systems that advance a `Scene`
pub trait SceneSimulation {
    /// Advances every configured Dogwood actor pawn by one fixed simulation step
    fn simulate_actors_pawns(
        world: &mut bevy_ecs::world::World,
        delta_time: f32,
        is_simulation_active: bool,
        gravity: [f32; 2],
        physics_world: &ScenePhysicsWorld,
    ) {
        let mut query: bevy_ecs::query::QueryState<(
            &ActorPawn,
            &ActorControlState,
            &mut ActorPawnSwimmingState,
            &mut ActorPawnWalkingState,
            &mut ActorPreviousPosition,
            &mut ScenePosition,
            &mut SceneVelocity,
        )> = world.query();
        for (
            pawn,
            control,
            swimming_state,
            mut walking_state,
            mut previous,
            mut position,
            mut velocity,
        ) in query.iter_mut(world)
        {
            previous.0 = *position;
            if !is_simulation_active && !pawn.simulate_when_paused {
                continue;
            }
            if !matches!(pawn.movement, Some(ActorPawnMovement::Walking)) {
                walking_state.cellular_drive_impulse = [0.0; 2];
                walking_state.grounded = false;
            }
            match pawn.movement {
                Some(ActorPawnMovement::Noclip) => {
                    let Some(configuration): Option<ActorPawnNoclipConfiguration> = pawn.noclip
                    else {
                        continue;
                    };
                    velocity.x = control.0.locomotion_x * configuration.speed;
                    velocity.y = control.0.locomotion_y * configuration.speed;
                    integrate_actor_position(&mut position, &velocity, delta_time);
                }
                Some(ActorPawnMovement::Walking) => {
                    let Some(configuration): Option<ActorPawnWalkingConfiguration> = pawn.walking
                    else {
                        continue;
                    };
                    Self::simulate_actor_pawn_walking(
                        &control.0,
                        &configuration,
                        pawn.collision_shape,
                        &mut walking_state,
                        &mut position,
                        &mut velocity,
                        gravity,
                        physics_world,
                        delta_time,
                    );
                }
                Some(ActorPawnMovement::Swimming) => {
                    let Some(configuration): Option<ActorPawnSwimmingConfiguration> = pawn.swimming
                    else {
                        continue;
                    };
                    simulate_actor_pawn_swimming(
                        &control.0,
                        &configuration,
                        &swimming_state,
                        pawn.collision_shape,
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

    /// Resolves one walking pawn's desired motion with direct scene collision queries
    fn simulate_actor_pawn_walking(
        control: &engine_input::ControlState,
        configuration: &ActorPawnWalkingConfiguration,
        collision_shape: Option<ActorCollisionShape>,
        state: &mut ActorPawnWalkingState,
        position: &mut ScenePosition,
        velocity: &mut SceneVelocity,
        gravity: [f32; 2],
        physics_world: &ScenePhysicsWorld,
        delta_time: f32,
    ) {
        let Some(collision_shape) = collision_shape else {
            return;
        };
        if !configuration.mass.is_finite() || configuration.mass <= 0.0 {
            return;
        }
        let gravity_magnitude: f32 = gravity[0].hypot(gravity[1]);
        let gravity_up_direction: Vector = if gravity_magnitude > 0.0 {
            Vector::new(
                -gravity[0] / gravity_magnitude,
                -gravity[1] / gravity_magnitude,
            )
        } else {
            Vector::Y
        };
        let gravity_tangent_direction: Vector =
            Vector::new(gravity_up_direction.y, -gravity_up_direction.x);
        let was_grounded: bool = state.grounded;
        let jump_requested: bool = control.locomotion_y > 0.0 && state.grounded;
        let world_x: f32 = position.tile_coordinates.x as f32 + position.x_offset;
        let world_y: f32 = position.tile_coordinates.y as f32 + position.y_offset;
        let previous_up_velocity: f32 =
            velocity.x * gravity_up_direction.x + velocity.y * gravity_up_direction.y;
        let landing_impulse: f32 =
            (-previous_up_velocity).max(0.0) * configuration.mass * CELLULAR_DRIVE_TRANSFER;
        let current_tangent_speed: f32 =
            velocity.x * gravity_tangent_direction.x + velocity.y * gravity_tangent_direction.y;
        let travel_direction: f32 = if control.locomotion_x.abs() > 1e-4 {
            control.locomotion_x.signum()
        } else if current_tangent_speed.abs() > 1e-4 {
            current_tangent_speed.signum()
        } else {
            0.0
        };
        let surface_direction: Option<Vector> =
            if was_grounded && !jump_requested && travel_direction != 0.0 {
                Self::probe_actor_pawn_walking_surface(
                    physics_world,
                    collision_shape,
                    Vector::new(world_x, world_y),
                    gravity_tangent_direction,
                    gravity_up_direction,
                    travel_direction,
                    configuration.maximum_slope_angle,
                    delta_time,
                )
            } else {
                None
            };
        let stationary_support: bool = was_grounded
            && !jump_requested
            && travel_direction == 0.0
            && (Self::probe_actor_pawn_walking_surface(
                physics_world,
                collision_shape,
                Vector::new(world_x, world_y),
                gravity_tangent_direction,
                gravity_up_direction,
                1.0,
                configuration.maximum_slope_angle,
                delta_time,
            )
            .is_some()
                || Self::probe_actor_pawn_walking_surface(
                    physics_world,
                    collision_shape,
                    Vector::new(world_x, world_y),
                    gravity_tangent_direction,
                    gravity_up_direction,
                    -1.0,
                    configuration.maximum_slope_angle,
                    delta_time,
                )
                .is_some()
                || {
                    let mut supported: bool = false;
                    let (_, grounded) = physics_world.move_actor(
                        collision_shape,
                        Vector::new(world_x, world_y),
                        -gravity_up_direction * (1.0 / 8.0),
                        gravity_up_direction,
                        configuration.maximum_slope_angle.cos(),
                        0.0,
                        &mut |normal| {
                            supported |= normal.dot(gravity_up_direction) > 1e-4;
                        },
                    );
                    supported || grounded
                });
        let locomotion_direction: Vector =
            surface_direction.unwrap_or(gravity_tangent_direction * travel_direction);
        let mut desired_velocity: Vector;
        if was_grounded && !jump_requested {
            let current_surface_speed: f32 = if travel_direction == 0.0 {
                0.0
            } else {
                Vector::new(velocity.x, velocity.y)
                    .dot(locomotion_direction)
                    .max(0.0)
            };
            let target_surface_speed: f32 = control.locomotion_x.abs() * configuration.speed;
            let surface_speed_change: f32 = (target_surface_speed - current_surface_speed).clamp(
                -configuration.acceleration * delta_time,
                configuration.acceleration * delta_time,
            );
            desired_velocity =
                locomotion_direction * (current_surface_speed + surface_speed_change).max(0.0);
        } else {
            desired_velocity = Vector::new(velocity.x, velocity.y);
            let target_tangent_speed: f32 = control.locomotion_x * configuration.speed;
            let current_tangent_speed: f32 = desired_velocity.dot(gravity_tangent_direction);
            let tangent_speed_change: f32 = (target_tangent_speed - current_tangent_speed).clamp(
                -configuration.acceleration * delta_time,
                configuration.acceleration * delta_time,
            );
            desired_velocity += gravity_tangent_direction * tangent_speed_change
                + Vector::new(gravity[0], gravity[1]) * delta_time;
        }
        state.cellular_drive_impulse = [
            locomotion_direction.x
                * control.locomotion_x.abs()
                * configuration.acceleration
                * configuration.mass
                * delta_time
                * CELLULAR_DRIVE_TRANSFER,
            locomotion_direction.y
                * control.locomotion_x.abs()
                * configuration.acceleration
                * configuration.mass
                * delta_time
                * CELLULAR_DRIVE_TRANSFER,
        ];
        if jump_requested {
            let gravity_up_velocity: f32 = desired_velocity.dot(gravity_up_direction);
            let jump_impulse: f32 = (configuration.jump_velocity - gravity_up_velocity)
                * configuration.mass
                * CELLULAR_DRIVE_TRANSFER;
            state.cellular_drive_impulse[0] += gravity_up_direction.x * jump_impulse;
            state.cellular_drive_impulse[1] += gravity_up_direction.y * jump_impulse;
            desired_velocity +=
                gravity_up_direction * (configuration.jump_velocity - gravity_up_velocity);
        }
        let mut contacted_wall: bool = false;
        let mut contacted_walkable_surface: bool = false;
        let walkable_normal: f32 = configuration.maximum_slope_angle.cos();
        let desired_translation: Vector = desired_velocity * delta_time;
        let virtual_surface: bool = was_grounded && !jump_requested && surface_direction.is_some();
        let mut collisions: &mut dyn FnMut(Vector) = &mut |normal: Vector| {
            let normal_gravity_up_dot_product: f32 = normal.dot(gravity_up_direction);
            if normal_gravity_up_dot_product >= walkable_normal {
                contacted_walkable_surface = true;
            } else if normal_gravity_up_dot_product > -walkable_normal {
                contacted_wall = true;
            }
        };
        let (resolved_translation, collision_grounded) = if virtual_surface {
            Self::traverse_actor_pawn_virtual_surface(
                physics_world,
                collision_shape,
                Vector::new(world_x, world_y),
                desired_translation,
                gravity_tangent_direction,
                gravity_up_direction,
                walkable_normal,
                &mut collisions,
            )
        } else {
            physics_world.move_actor(
                collision_shape,
                Vector::new(world_x, world_y),
                desired_translation,
                gravity_up_direction,
                walkable_normal,
                1.0 / 8.0,
                &mut collisions,
            )
        };
        let virtual_surface_complete: bool = virtual_surface
            && (resolved_translation - desired_translation).length_squared()
                <= 4.0 / (1024.0 * 1024.0);
        integrate_actor_position(
            position,
            &SceneVelocity {
                x: resolved_translation.x,
                y: resolved_translation.y,
            },
            1.0,
        );
        // wall seams can produce tiny upward normals that Rapier reports as grounded
        state.grounded = stationary_support
            || virtual_surface_complete
            || contacted_walkable_surface
            || (collision_grounded && !contacted_wall);
        if state.grounded && !jump_requested {
            let requested_distance: f32 = desired_velocity.length() * delta_time;
            let resolved_locomotion_distance: f32 = if requested_distance > 0.0 {
                resolved_translation
                    .dot(locomotion_direction)
                    .clamp(0.0, requested_distance)
            } else {
                0.0
            };
            let resolved_surface_speed: f32 = if virtual_surface_complete {
                desired_velocity.length()
            } else {
                resolved_locomotion_distance / delta_time
            };
            velocity.x = locomotion_direction.x * resolved_surface_speed;
            velocity.y = locomotion_direction.y * resolved_surface_speed;
        } else {
            velocity.x = resolved_translation.x / delta_time;
            velocity.y = resolved_translation.y / delta_time;
        }
        if state.grounded && !was_grounded && landing_impulse > 0.0 {
            state.cellular_drive_impulse[0] -= gravity_up_direction.x * landing_impulse;
            state.cellular_drive_impulse[1] -= gravity_up_direction.y * landing_impulse;
        }
    }

    /// Estimates a continuous walkable direction from one- and two-cell support samples
    fn probe_actor_pawn_walking_surface(
        physics_world: &ScenePhysicsWorld,
        collision_shape: ActorCollisionShape,
        position: Vector,
        gravity_tangent_direction: Vector,
        gravity_up_direction: Vector,
        direction: f32,
        maximum_slope_angle: f32,
        delta_time: f32,
    ) -> Option<Vector> {
        let midpoint_run: f32 = 1.0 / 8.0;
        let lookahead_run: f32 = 2.0 / 8.0;
        let midpoint_rise: f32 = Self::probe_actor_pawn_support_rise(
            physics_world,
            collision_shape,
            position,
            gravity_tangent_direction,
            gravity_up_direction,
            direction,
            midpoint_run,
            maximum_slope_angle,
            delta_time,
        )?;
        let lookahead_rise: f32 = Self::probe_actor_pawn_support_rise(
            physics_world,
            collision_shape,
            position,
            gravity_tangent_direction,
            gravity_up_direction,
            direction,
            lookahead_run,
            maximum_slope_angle,
            delta_time,
        )?;
        let maximum_midpoint_rise: f32 = midpoint_run * maximum_slope_angle.tan();
        if midpoint_rise.abs() > maximum_midpoint_rise + 1.0 / 1024.0
            || (lookahead_rise - midpoint_rise).abs() > maximum_midpoint_rise + 1.0 / 1024.0
            || lookahead_rise.atan2(lookahead_run).abs() > maximum_slope_angle
        {
            return None;
        }
        Some(
            (gravity_tangent_direction * direction * lookahead_run
                + gravity_up_direction * lookahead_rise)
                .normalize(),
        )
    }

    /// Traverses a validated cellular staircase above its raw risers, then settles on support.
    fn traverse_actor_pawn_virtual_surface(
        physics_world: &ScenePhysicsWorld,
        collision_shape: ActorCollisionShape,
        position: Vector,
        desired: Vector,
        gravity_tangent_direction: Vector,
        gravity_up_direction: Vector,
        walkable_normal: f32,
        collisions: &mut impl FnMut(Vector),
    ) -> (Vector, bool) {
        let requested_rise: f32 = desired.dot(gravity_up_direction);
        // a canonical cell is the maximum discrete riser accepted by the validated probe.
        let clearance: f32 = 1.0 / 8.0 + requested_rise.max(0.0);
        let (raised, _) = physics_world.move_actor(
            collision_shape,
            position,
            gravity_up_direction * clearance,
            gravity_up_direction,
            walkable_normal,
            0.0,
            collisions,
        );
        if raised.dot(gravity_up_direction) < clearance - 1.0 / 1024.0 {
            return (raised, false);
        }
        let (forward, _) = physics_world.move_actor(
            collision_shape,
            position + raised,
            gravity_tangent_direction * desired.dot(gravity_tangent_direction),
            gravity_up_direction,
            walkable_normal,
            0.0,
            collisions,
        );
        let (settled, grounded) = physics_world.resolve_actor_support(
            collision_shape,
            position + raised + forward,
            clearance - requested_rise,
            gravity_up_direction,
        );
        (raised + forward + settled, grounded)
    }

    /// Measures signed support-height change at one gravity-relative tangent distance
    fn probe_actor_pawn_support_rise(
        physics_world: &ScenePhysicsWorld,
        collision_shape: ActorCollisionShape,
        position: Vector,
        gravity_tangent_direction: Vector,
        gravity_up_direction: Vector,
        direction: f32,
        run: f32,
        maximum_slope_angle: f32,
        _delta_time: f32,
    ) -> Option<f32> {
        let maximum_rise: f32 = run * maximum_slope_angle.tan();
        if !maximum_rise.is_finite() || maximum_rise <= 0.0 {
            return None;
        }
        let rise_clearance: f32 = maximum_rise + 1.0 / 8.0;
        let (probe_up, _) = physics_world.move_actor(
            collision_shape,
            position,
            gravity_up_direction * rise_clearance,
            gravity_up_direction,
            maximum_slope_angle.cos(),
            0.0,
            &mut |_| {},
        );
        if probe_up.dot(gravity_up_direction) < rise_clearance - 1.0 / 1024.0 {
            return None;
        }
        let raised_position: Vector = position + probe_up;
        let (probe_forward, _) = physics_world.move_actor(
            collision_shape,
            raised_position,
            gravity_tangent_direction * direction * run,
            gravity_up_direction,
            maximum_slope_angle.cos(),
            0.0,
            &mut |_| {},
        );
        if probe_forward.dot(gravity_tangent_direction) * direction < run - 1.0 / 1024.0 {
            return None;
        }
        let forward_position: Vector = raised_position + probe_forward;
        let (probe_down, grounded) = physics_world.resolve_actor_support(
            collision_shape,
            forward_position,
            rise_clearance + maximum_rise + 1.0 / 8.0,
            gravity_up_direction,
        );
        if !grounded {
            return None;
        }
        Some((probe_up + probe_forward + probe_down).dot(gravity_up_direction))
    }
}

impl SceneSimulation for Scene {}
