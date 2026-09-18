// Copyright Rob Gage 2026

use rapier2d::prelude::Vector;

use crate::actors::ActorCollisionShape;
use crate::actors::ActorPawnSwimmingConfiguration;
use crate::actors::ActorPawnSwimmingState;
use crate::scenes::ScenePosition;
use crate::scenes::SceneVelocity;
use crate::simulation::ScenePhysicsWorld;
use crate::simulation_actors::scene_simulation_position::integrate_actor_position;

/// Advances one swimmer relative to its asynchronously sampled surrounding fluid
pub(super) fn simulate_actor_pawn_swimming(
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
    let Some(collision_shape) = collision_shape else {
        return;
    };
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
    let immersion: f32 = state.immersion.clamp(0.0, 1.0);
    let buoyancy_ratio: f32 = immersion * state.fluid_density / configuration.density;
    velocity.x += gravity[0] * (1.0 - buoyancy_ratio) * delta_time;
    velocity.y += gravity[1] * (1.0 - buoyancy_ratio) * delta_time;

    let fluid_velocity: Vector = Vector::new(state.fluid_velocity[0], state.fluid_velocity[1]);
    let mut relative_velocity: Vector = Vector::new(velocity.x, velocity.y) - fluid_velocity;
    let swimming_drag_factor: f32 =
        (-state.fluid_viscosity * configuration.drag * immersion * delta_time).exp();
    relative_velocity *= swimming_drag_factor;
    let mut movement_input: Vector = gravity_tangent_direction * control.locomotion_x
        + gravity_up_direction * control.locomotion_y;
    let movement_input_magnitude: f32 = movement_input.length();
    if movement_input_magnitude > 1.0 {
        movement_input /= movement_input_magnitude;
    }
    relative_velocity += movement_input * configuration.acceleration * delta_time;
    let relative_speed: f32 = relative_velocity.length();
    if relative_speed > configuration.maximum_speed {
        relative_velocity *= configuration.maximum_speed / relative_speed;
    }
    let resolved_velocity: Vector = fluid_velocity + relative_velocity;
    velocity.x = resolved_velocity.x;
    velocity.y = resolved_velocity.y;

    let world_x: f32 = position.tile_coordinates.x as f32 + position.x_offset;
    let world_y: f32 = position.tile_coordinates.y as f32 + position.y_offset;
    let (movement, _) = physics_world.move_actor(
        collision_shape,
        Vector::new(world_x, world_y),
        Vector::new(velocity.x, velocity.y) * delta_time,
        gravity_up_direction,
        1.0,
        0.0,
        &mut |normal| {
            let normal_speed: f32 = velocity.x * normal.x + velocity.y * normal.y;
            if normal_speed < 0.0 {
                velocity.x -= normal.x * normal_speed;
                velocity.y -= normal.y * normal_speed;
            }
        },
    );
    integrate_actor_position(
        position,
        &SceneVelocity {
            x: movement.x,
            y: movement.y,
        },
        1.0,
    );
}
