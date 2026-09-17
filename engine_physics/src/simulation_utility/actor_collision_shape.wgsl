// Copyright Rob Gage 2026

#define_import_path utility::actor_collision_shape
#import utility::simulation_constants::{
    CELLS_PER_TILE,
    CELLS_PER_TILE_FLOAT,
    CELL_COUNT_PER_TILE,
    EMPTY_MATERIAL_IDENTIFIER,
    GAS_MATERIAL_FORM,
    CELLULAR_STATIC_MATERIAL_FORM,
    CELLULAR_DYNAMIC_MATERIAL_FORM,
    FLUID_MATERIAL_FORM,
    MATERIAL_IDENTIFIER_INDEX_MASK,
    INVALID_MATERIAL_DENSE_INDEX,
    FLUID_EDIT_ERASE,
    INVALID_PHYSICAL_CELL_INDEX,
    INVALID_CELLULAR_DYNAMIC_CLAIM_INDEX,
    INVALID_FLUID_PARTICLE_INDEX,
    INVALID_FLUID_BUCKET_INDEX,
    ACTOR_SHAPE_CIRCLE,
    ACTOR_SHAPE_CAPSULE,
    ACTOR_SHAPE_RECTANGLE,
    PI,
    PBF_SUBSTEP_COUNT,
    PBF_CONSTRAINT_ITERATION_COUNT,
    CONSTRAINT_EPSILON,
    ARTIFICIAL_PRESSURE_DELTA_Q_RATIO,
    MAXIMUM_CORRECTION_CELLS,
    HARD_EXTERNAL_BODY_OCCUPANCY,
    SWIMMER_EXTERNAL_BODY_OCCUPANCY,
    RIGID_EXTERNAL_BODY_OCCUPANCY,
    IMMOVABLE_CONTACT_MASS,
    CONTACT_PRESSURE_TRANSFER,
    LINEAR_FIXED_SCALE,
    ANGULAR_FIXED_SCALE,
    CELL_SIZE,
    CELL_HALF,
    CELL_RADIUS,
    INCOMPRESSIBILITY_MIXING,
    RESERVATION_SCALE,
    RESERVATION_SCALE_U32
}

struct ActorShape {
    center: vec2<f32>,
    up: vec2<f32>,
    tangent: vec2<f32>,
    parameters: vec2<f32>,
    kind: u32,
}

fn actor_shape_from_parameters(
    center: vec2<f32>,
    gravity: vec2<f32>,
    parameters: vec2<f32>,
    kind: u32
) -> ActorShape {
    let gravity_length: f32 = length(gravity);
    let up: vec2<f32> = select(
      vec2<f32>(0.0, 1.0),
      -gravity / max(gravity_length, 0.000001),
      gravity_length > 0.0);
    return ActorShape(center, up, vec2<f32>(up.y, -up.x), parameters, kind);
}

fn actor_shape_world_extent(shape: ActorShape) -> vec2<f32> {
    if shape.kind == ACTOR_SHAPE_CIRCLE {
        return vec2<f32>(shape.parameters.x);
    }
    if shape.kind == ACTOR_SHAPE_CAPSULE {
        return
      abs(shape.tangent) * shape.parameters.x + abs(
        shape.up) * (shape.parameters.x + shape.parameters.y);
    }
    return
    abs(shape.tangent) * shape.parameters.x + abs(
      shape.up) * shape.parameters.y;
}

fn world_position_is_inside_actor_shape(
    position: vec2<f32>,
    shape: ActorShape
) -> bool {
    let relative: vec2<f32> = position - shape.center;
    let tangent: f32 = dot(relative, shape.tangent);
    let up: f32 = dot(relative, shape.up);
    if shape.kind == ACTOR_SHAPE_CIRCLE {
        return dot(relative, relative) <= shape.parameters.x * shape.parameters.x;
    }
    if shape.kind == ACTOR_SHAPE_CAPSULE {
        let segment: f32 = clamp(up, -shape.parameters.y, shape.parameters.y);
        return length(vec2<f32>(tangent, up - segment)) <= shape.parameters.x;
    }
    return abs(tangent) <= shape.parameters.x && abs(up) <= shape.parameters.y;
}

fn actor_shape_intersects_axis_aligned_cell(
    cell_center: vec2<f32>,
    cell_half_extent: f32,
    shape: ActorShape,
) -> bool {
    let relative: vec2<f32> = cell_center - shape.center;
    if shape.kind == ACTOR_SHAPE_CIRCLE {
        let closest: vec2<f32> = max(abs(relative) - vec2<f32>(cell_half_extent), vec2<f32>(0.0));
        return dot(closest, closest) <= shape.parameters.x * shape.parameters.x;
    }
    let tangent: f32 = dot(relative, shape.tangent);
    let up: f32 = dot(relative, shape.up);
    if shape.kind == ACTOR_SHAPE_CAPSULE {
        let segment: f32 = clamp(up, -shape.parameters.y, shape.parameters.y);
        let conservative_radius: f32 = shape.parameters.x + cell_half_extent * sqrt(2.0);
        return
      dot(
        vec2<f32>(tangent, up - segment),
        vec2<f32>(tangent, up - segment)) <= conservative_radius * conservative_radius;
    }
    let cell_radius_on_tangent: f32 = cell_half_extent * (abs(shape.tangent.x) + abs(shape.tangent.y));
    let cell_radius_on_up: f32 = cell_half_extent * (abs(shape.up.x) + abs(shape.up.y));
    if abs(tangent) > shape.parameters.x + cell_radius_on_tangent || abs(
      up) > shape.parameters.y + cell_radius_on_up {
        return false;
    }
    let actor_radius_x: f32 = shape.parameters.x * abs(shape.tangent.x) + shape.parameters.y * abs(
      shape.up.x);
    let actor_radius_y: f32 = shape.parameters.x * abs(shape.tangent.y) + shape.parameters.y * abs(
      shape.up.y);
    return
    abs(relative.x) <= actor_radius_x + cell_half_extent && abs(
      relative.y) <= actor_radius_y + cell_half_extent;
}
