// Copyright Rob Gage 2026

use super::ScenePhysicsWorld;
use crate::{
    actors::{
        ActorControlState,
        ActorCollisionShape,
        ActorPawn,
        ActorPawnMovement,
        ActorPawnNoclipConfiguration,
        ActorPawnSwimmingConfiguration,
        ActorPawnSwimmingState,
        ActorPawnWalkingConfiguration,
        ActorPawnWalkingState,
        ActorPreviousPosition,
    },
    scenes::{Scene, ScenePosition, SceneVelocity},
};
use rapier2d::prelude::Vector;

const CELLULAR_DRIVE_TRANSFER: f32 = 0.08;

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
            &mut ActorPawnSwimmingState,
            &mut ActorPawnWalkingState,
            &mut ActorPreviousPosition,
            &mut ScenePosition,
            &mut SceneVelocity,
        )> = world.query();
        for (pawn, control, swimming_state, mut walking_state, mut previous,
                mut position, mut velocity) in
                query.iter_mut(world) {
            previous.0 = *position;
            if !is_simulation_active && !pawn.simulate_when_paused { continue; }
            if !matches!(pawn.movement, Some(ActorPawnMovement::Walking)) {
                walking_state.cellular_drive_impulse = [0.0; 2];
                walking_state.grounded = false;
            }
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
                        else { continue; };
                    Self::simulate_actor_pawn_swimming(
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
        let Some(collision_shape) = collision_shape else { return; };
        if !configuration.mass.is_finite() || configuration.mass <= 0.0 { return; }
        let gravity_magnitude: f32 = gravity[0].hypot(gravity[1]);
        let up: Vector = if gravity_magnitude > 0.0 {
            Vector::new(-gravity[0] / gravity_magnitude, -gravity[1] / gravity_magnitude)
        } else {
            Vector::Y
        };
        let tangent: Vector = Vector::new(up.y, -up.x);
        let was_grounded: bool = state.grounded;
        let jump_requested: bool = control.locomotion_y > 0.0 && state.grounded;
        let world_x: f32 = position.tile_coordinates.x as f32 + position.x_offset;
        let world_y: f32 = position.tile_coordinates.y as f32 + position.y_offset;
        let previous_up_velocity: f32 = velocity.x * up.x + velocity.y * up.y;
        let landing_impulse: f32 = (-previous_up_velocity).max(0.0) * configuration.mass *
            CELLULAR_DRIVE_TRANSFER;
        let current_tangent_speed: f32 = velocity.x * tangent.x + velocity.y * tangent.y;
        let travel_direction: f32 = if control.locomotion_x.abs() > 1e-4 {
            control.locomotion_x.signum()
        } else if current_tangent_speed.abs() > 1e-4 {
            current_tangent_speed.signum()
        } else { 0.0 };
        let surface_direction: Option<Vector> = if was_grounded && !jump_requested &&
                travel_direction != 0.0 {
            Self::probe_actor_pawn_walking_surface(
                physics_world,
                collision_shape,
                Vector::new(world_x, world_y),
                tangent,
                up,
                travel_direction,
                configuration.maximum_slope_angle,
                delta_time,
            )
        } else { None };
        let stationary_support: bool = was_grounded && !jump_requested &&
            travel_direction == 0.0 && (
                Self::probe_actor_pawn_walking_surface(
                    physics_world, collision_shape,
                    Vector::new(world_x, world_y), tangent, up, 1.0,
                    configuration.maximum_slope_angle, delta_time,
                ).is_some() ||
                Self::probe_actor_pawn_walking_surface(
                    physics_world, collision_shape,
                    Vector::new(world_x, world_y), tangent, up, -1.0,
                    configuration.maximum_slope_angle, delta_time,
                ).is_some() || {
                    let mut supported = false;
                    let (_, grounded) = physics_world.move_actor(collision_shape,
                        Vector::new(world_x, world_y), -up * (1.0 / 8.0), up,
                        configuration.maximum_slope_angle.cos(), 0.0, None,
                        &mut |normal| { supported |= normal.dot(up) > 1e-4; });
                    supported || grounded
                }
            );
        let locomotion_direction: Vector = surface_direction.unwrap_or(tangent * travel_direction);
        let mut desired_velocity: Vector;
        if was_grounded && !jump_requested {
            let current_surface_speed: f32 = if travel_direction == 0.0 { 0.0 } else {
                Vector::new(velocity.x, velocity.y).dot(locomotion_direction).max(0.0)
            };
            let target_surface_speed: f32 = control.locomotion_x.abs() * configuration.speed;
            let change: f32 = (target_surface_speed - current_surface_speed).clamp(
                -configuration.acceleration * delta_time,
                configuration.acceleration * delta_time,
            );
            desired_velocity = locomotion_direction * (current_surface_speed + change).max(0.0);
        } else {
            desired_velocity = Vector::new(velocity.x, velocity.y);
            let target_tangent_speed: f32 = control.locomotion_x * configuration.speed;
            let tangent_speed: f32 = desired_velocity.dot(tangent);
            let change: f32 = (target_tangent_speed - tangent_speed).clamp(
                -configuration.acceleration * delta_time,
                configuration.acceleration * delta_time,
            );
            desired_velocity += tangent * change + Vector::new(gravity[0], gravity[1]) * delta_time;
        }
        state.cellular_drive_impulse = [
            locomotion_direction.x * control.locomotion_x.abs() * configuration.acceleration *
                configuration.mass * delta_time * CELLULAR_DRIVE_TRANSFER,
            locomotion_direction.y * control.locomotion_x.abs() * configuration.acceleration *
                configuration.mass * delta_time * CELLULAR_DRIVE_TRANSFER,
        ];
        if jump_requested {
            let up_velocity: f32 = desired_velocity.dot(up);
            let jump_impulse: f32 = (configuration.jump_velocity - up_velocity) *
                configuration.mass * CELLULAR_DRIVE_TRANSFER;
            state.cellular_drive_impulse[0] += up.x * jump_impulse;
            state.cellular_drive_impulse[1] += up.y * jump_impulse;
            desired_velocity += up * (configuration.jump_velocity - up_velocity);
        }
        let mut contacted_wall: bool = false;
        let mut contacted_walkable_surface: bool = false;
        let walkable_normal: f32 = configuration.maximum_slope_angle.cos();
        let desired_translation: Vector = desired_velocity * delta_time;
        let virtual_surface = was_grounded && !jump_requested && surface_direction.is_some();
        let mut collisions = |normal: Vector| {
                let normal_up: f32 = normal.dot(up);
                if normal_up >= walkable_normal { contacted_walkable_surface = true; }
                else if normal_up > -walkable_normal { contacted_wall = true; }
            };
        let (resolved_translation, collision_grounded) = if virtual_surface {
            Self::traverse_actor_pawn_virtual_surface(physics_world, collision_shape,
                Vector::new(world_x, world_y), desired_translation, tangent, up,
                walkable_normal, &mut collisions)
        } else {
            physics_world.move_actor(collision_shape, Vector::new(world_x, world_y),
                desired_translation, up, walkable_normal, 1.0 / 8.0, None, &mut collisions)
        };
        let virtual_surface_complete = virtual_surface && (resolved_translation - desired_translation)
            .length_squared() <= 4.0 / (1024.0 * 1024.0);
        Self::integrate_actor_position(
            position,
            &SceneVelocity {
                x: resolved_translation.x,
                y: resolved_translation.y,
            },
            1.0,
        );
        // wall seams can produce tiny upward normals that Rapier reports as grounded
        state.grounded = stationary_support || virtual_surface_complete ||
            contacted_walkable_surface || (collision_grounded && !contacted_wall);
        if state.grounded && !jump_requested {
            let requested_distance: f32 = desired_velocity.length() * delta_time;
            let resolved_locomotion_distance: f32 = if requested_distance > 0.0 {
                resolved_translation.dot(locomotion_direction).clamp(0.0, requested_distance)
            } else { 0.0 };
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
            state.cellular_drive_impulse[0] -= up.x * landing_impulse;
            state.cellular_drive_impulse[1] -= up.y * landing_impulse;
        }
    }

    /// Estimates a continuous walkable direction from one- and two-cell support samples
    fn probe_actor_pawn_walking_surface(
        physics_world: &ScenePhysicsWorld,
        collision_shape: ActorCollisionShape,
        position: Vector,
        tangent: Vector,
        up: Vector,
        direction: f32,
        maximum_slope_angle: f32,
        delta_time: f32,
    ) -> Option<Vector> {
        let midpoint_run: f32 = 1.0 / 8.0;
        let lookahead_run: f32 = 2.0 / 8.0;
        let midpoint_rise: f32 = Self::probe_actor_pawn_support_rise(
            physics_world, collision_shape, position, tangent, up, direction,
            midpoint_run, maximum_slope_angle, delta_time,
        )?;
        let lookahead_rise: f32 = Self::probe_actor_pawn_support_rise(
            physics_world, collision_shape, position, tangent, up, direction,
            lookahead_run, maximum_slope_angle, delta_time,
        )?;
        let maximum_midpoint_rise: f32 = midpoint_run * maximum_slope_angle.tan();
        if midpoint_rise.abs() > maximum_midpoint_rise + 1.0 / 1024.0 ||
                (lookahead_rise - midpoint_rise).abs() >
                    maximum_midpoint_rise + 1.0 / 1024.0 ||
                lookahead_rise.atan2(lookahead_run).abs() > maximum_slope_angle {
            return None;
        }
        Some((tangent * direction * lookahead_run + up * lookahead_rise).normalize())
    }

    /// Traverses a validated cellular staircase above its raw risers, then settles on support.
    fn traverse_actor_pawn_virtual_surface(
        physics_world: &ScenePhysicsWorld,
        collision_shape: ActorCollisionShape,
        position: Vector,
        desired: Vector,
        tangent: Vector,
        up: Vector,
        walkable_normal: f32,
        collisions: &mut impl FnMut(Vector),
    ) -> (Vector, bool) {
        let requested_rise = desired.dot(up);
        // A canonical cell is the maximum discrete riser accepted by the validated probe.
        let clearance = 1.0 / 8.0 + requested_rise.max(0.0);
        let (raised, _) = physics_world.move_actor(collision_shape, position, up * clearance,
            up, walkable_normal, 0.0, None, collisions);
        if raised.dot(up) < clearance - 1.0 / 1024.0 { return (raised, false); }
        let (forward, _) = physics_world.move_actor(collision_shape, position + raised,
            tangent * desired.dot(tangent), up, walkable_normal, 0.0, None, collisions);
        let (settled, grounded) = physics_world.resolve_actor_support(collision_shape,
            position + raised + forward, clearance - requested_rise, up);
        (raised + forward + settled, grounded)
    }

    /// Measures signed support-height change at one gravity-relative tangent distance
    fn probe_actor_pawn_support_rise(
        physics_world: &ScenePhysicsWorld,
        collision_shape: ActorCollisionShape,
        position: Vector,
        tangent: Vector,
        up: Vector,
        direction: f32,
        run: f32,
        maximum_slope_angle: f32,
        _delta_time: f32,
    ) -> Option<f32> {
        let maximum_rise: f32 = run * maximum_slope_angle.tan();
        if !maximum_rise.is_finite() || maximum_rise <= 0.0 { return None; }
        let rise_clearance: f32 = maximum_rise + 1.0 / 8.0;
        let (probe_up, _) = physics_world.move_actor(collision_shape, position,
            up * rise_clearance, up, maximum_slope_angle.cos(), 0.0, None, &mut |_| { });
        if probe_up.dot(up) < rise_clearance - 1.0 / 1024.0 { return None; }
        let raised_position: Vector = position + probe_up;
        let (probe_forward, _) = physics_world.move_actor(collision_shape, raised_position,
            tangent * direction * run, up, maximum_slope_angle.cos(), 0.0, None, &mut |_| { });
        if probe_forward.dot(tangent) * direction < run - 1.0 / 1024.0 {
            return None;
        }
        let forward_position: Vector = raised_position + probe_forward;
        let (probe_down, grounded) = physics_world.resolve_actor_support(collision_shape,
            forward_position, rise_clearance + maximum_rise + 1.0 / 8.0, up);
        if !grounded { return None; }
        Some((probe_up + probe_forward + probe_down).dot(up))
    }

    /// Advances one swimmer relative to its asynchronously sampled surrounding fluid
    fn simulate_actor_pawn_swimming(
        control: &engine_input::ControlState,
        configuration: &ActorPawnSwimmingConfiguration,
        state: &ActorPawnSwimmingState,
        collision_shape: Option<ActorCollisionShape>,
        position: &mut ScenePosition,
        velocity: &mut SceneVelocity,
        gravity: [f32; 2],
        physics_world: &ScenePhysicsWorld,
        delta_time: f32,
    ) {
        let Some(collision_shape) = collision_shape else { return; };
        let gravity_magnitude: f32 = gravity[0].hypot(gravity[1]);
        let up: Vector = if gravity_magnitude > 0.0 {
            Vector::new(-gravity[0] / gravity_magnitude, -gravity[1] / gravity_magnitude)
        } else {
            Vector::Y
        };
        let tangent: Vector = Vector::new(up.y, -up.x);
        let immersion: f32 = state.immersion.clamp(0.0, 1.0);
        let buoyancy_ratio: f32 = immersion * state.fluid_density / configuration.density;
        velocity.x += gravity[0] * (1.0 - buoyancy_ratio) * delta_time;
        velocity.y += gravity[1] * (1.0 - buoyancy_ratio) * delta_time;

        let fluid_velocity: Vector = Vector::new(
            state.fluid_velocity[0],
            state.fluid_velocity[1],
        );
        let mut relative_velocity: Vector = Vector::new(velocity.x, velocity.y) - fluid_velocity;
        let drag: f32 = (-state.fluid_viscosity * configuration.drag * immersion *
            delta_time).exp();
        relative_velocity *= drag;
        let mut input: Vector = tangent * control.locomotion_x + up * control.locomotion_y;
        let input_magnitude: f32 = input.length();
        if input_magnitude > 1.0 { input /= input_magnitude; }
        relative_velocity += input * configuration.acceleration * delta_time;
        let relative_speed: f32 = relative_velocity.length();
        if relative_speed > configuration.maximum_speed {
            relative_velocity *= configuration.maximum_speed / relative_speed;
        }
        let resolved_velocity: Vector = fluid_velocity + relative_velocity;
        velocity.x = resolved_velocity.x;
        velocity.y = resolved_velocity.y;

        let world_x: f32 = position.tile_coordinates.x as f32 + position.x_offset;
        let world_y: f32 = position.tile_coordinates.y as f32 + position.y_offset;
        let (movement, _) = physics_world.move_actor(collision_shape,
            Vector::new(world_x, world_y), Vector::new(velocity.x, velocity.y) * delta_time,
            up, 1.0, 0.0, None, &mut |normal| {
                let normal_speed: f32 = velocity.x * normal.x +
                    velocity.y * normal.y;
                if normal_speed < 0.0 {
                    velocity.x -= normal.x * normal_speed;
                    velocity.y -= normal.y * normal_speed;
                }
            });
        Self::integrate_actor_position(
            position,
            &SceneVelocity {
                x: movement.x,
                y: movement.y,
            },
            1.0,
        );
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

    fn staircase_snapshot(height: impl Fn(i32) -> i32) -> CollisionOccupancySnapshot {
        let origin = TileCoordinates { x: -1, y: -1 };
        let width = 3u16;
        let height_tiles = 3u16;
        let mut dynamic_masks = vec![[0u32; 2]; usize::from(width * height_tiles)];
        for x in -8..16 {
            for y in -8..height(x) {
                let tile_x = x.div_euclid(8) - origin.x;
                let tile_y = y.div_euclid(8) - origin.y;
                if tile_x < 0 || tile_y < 0 || tile_x >= i32::from(width) ||
                        tile_y >= i32::from(height_tiles) { continue; }
                let tile = tile_y as usize * usize::from(width) + tile_x as usize;
                let cell = y.rem_euclid(8) as usize * 8 + x.rem_euclid(8) as usize;
                dynamic_masks[tile][cell / 32] |= 1 << (cell % 32);
            }
        }
        CollisionOccupancySnapshot {
            sequence: 0,
            origin,
            width,
            height: height_tiles,
            static_masks: vec![[0; 2]; usize::from(width * height_tiles)].into_boxed_slice(),
            dynamic_masks: dynamic_masks.into_boxed_slice(),
        }
    }

    fn probe_staircase(height: impl Fn(i32) -> i32) -> Option<Vector> {
        let mut physics = ScenePhysicsWorld::new();
        physics.update_cellular_terrain(staircase_snapshot(height));
        Scene::probe_actor_pawn_walking_surface(
            &physics,
            ActorCollisionShape::Rectangle { width: 0.05, height: 0.5 },
            Vector::new(0.0625, 0.251),
            Vector::X,
            Vector::Y,
            1.0,
            50.0_f32.to_radians(),
            1.0 / 60.0,
        )
    }

    #[test]
    fn two_cell_probe_derives_normalized_surface_speed_without_applying_rise() {
        let forty_five = probe_staircase(|x| x).expect("45 degree staircase");
        assert!((forty_five.x * 4.0 - 2.828).abs() < 0.08);
        assert!((forty_five.y * 4.0 - 2.828).abs() < 0.08);
        assert!((forty_five.length() * 4.0 - 4.0).abs() < 0.001);
        assert!(forty_five.y * 4.0 / 60.0 < 0.125);

        let shallow = probe_staircase(|x| x.div_euclid(2)).expect("shallow staircase");
        assert!((shallow.x * 4.0 - 3.578).abs() < 0.08);
        assert!((shallow.y * 4.0 - 1.789).abs() < 0.08);
        assert!((shallow.length() * 4.0 - 4.0).abs() < 0.001);
    }

    #[test]
    fn midpoint_probe_rejects_locally_too_steep_staircase() {
        assert!(probe_staircase(|x| x * 2).is_none());
    }

    #[test]
    fn virtual_surface_traversal_clears_a_riser_without_losing_surface_distance() {
        let mut physics = ScenePhysicsWorld::new();
        physics.update_cellular_terrain(staircase_snapshot(|x| x));
        let surface = Scene::probe_actor_pawn_walking_surface(&physics,
            ActorCollisionShape::Rectangle { width: 0.05, height: 0.5 },
            Vector::new(0.0625, 0.251),
            Vector::X, Vector::Y, 1.0, 50.0_f32.to_radians(), 1.0 / 60.0)
            .expect("45 degree staircase");
        let position = Vector::new(0.1, 0.2885);
        let desired = surface * 0.1;
        let (movement, _) = Scene::traverse_actor_pawn_virtual_surface(&physics,
            ActorCollisionShape::Rectangle { width: 0.05, height: 0.5 }, position, desired,
            Vector::X, Vector::Y, 50.0_f32.to_radians().cos(), &mut |_| { });
        assert!((movement - desired).length() < 1.0 / 1024.0);
    }

}
