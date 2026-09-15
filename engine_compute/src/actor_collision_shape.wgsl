// Copyright Rob Gage 2026

#define_import_path utility::actor_collision_shape

const ACTOR_SHAPE_CIRCLE: u32 = 0u;
const ACTOR_SHAPE_CAPSULE: u32 = 1u;
const ACTOR_SHAPE_RECTANGLE: u32 = 2u;

struct ActorShape {
    center: vec2<f32>,
    up: vec2<f32>,
    tangent: vec2<f32>,
    parameters: vec2<f32>,
    kind: u32,
}

fn actor_shape_from_parameters(center: vec2<f32>, gravity: vec2<f32>,
        parameters: vec2<f32>, kind: u32) -> ActorShape {
    let gravity_length: f32 = length(gravity);
    let up: vec2<f32> = select(vec2<f32>(0.0, 1.0),
        -gravity / max(gravity_length, 0.000001), gravity_length > 0.0);
    return ActorShape(center, up, vec2<f32>(up.y, -up.x), parameters, kind);
}

fn actor_shape_world_extent(shape: ActorShape) -> vec2<f32> {
    if shape.kind == ACTOR_SHAPE_CIRCLE {
        return vec2<f32>(shape.parameters.x);
    }
    if shape.kind == ACTOR_SHAPE_CAPSULE {
        return abs(shape.tangent) * shape.parameters.x +
            abs(shape.up) * (shape.parameters.x + shape.parameters.y);
    }
    return abs(shape.tangent) * shape.parameters.x + abs(shape.up) * shape.parameters.y;
}

fn world_position_is_inside_actor_shape(position: vec2<f32>, shape: ActorShape) -> bool {
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
