// Copyright Rob Gage 2026

#define_import_path utility::capsule_collision

struct GravityRelativeCapsule {
    center: vec2<f32>,
    up: vec2<f32>,
    tangent: vec2<f32>,
    radius: f32,
    half_segment: f32,
}

// Builds the world-space geometry of a gravity-relative capsule
fn gravity_relative_capsule_from_parameters(
    center: vec2<f32>,
    gravity: vec2<f32>,
    radius: f32,
    half_segment: f32,
) -> GravityRelativeCapsule {
    let gravity_length: f32 = length(gravity);
    let up: vec2<f32> = select(
        vec2<f32>(0.0, 1.0),
        -gravity / max(gravity_length, 0.000001),
        gravity_length > 0.0,
    );
    return GravityRelativeCapsule(
        center,
        up,
        vec2<f32>(up.y, -up.x),
        radius,
        half_segment,
    );
}

// Tests a world position against a gravity-relative capsule
fn world_position_is_inside_gravity_relative_capsule(
    world_position: vec2<f32>,
    capsule: GravityRelativeCapsule,
) -> bool {
    let relative: vec2<f32> = world_position - capsule.center;
    let nearest: f32 = clamp(
        dot(relative, capsule.up), -capsule.half_segment, capsule.half_segment,
    );
    return length(vec2<f32>(
        dot(relative, capsule.tangent),
        dot(relative, capsule.up) - nearest,
    )) <= capsule.radius;
}

// Returns the axis-aligned world extent of a gravity-relative capsule
fn gravity_relative_capsule_world_extent(capsule: GravityRelativeCapsule) -> vec2<f32> {
    return abs(capsule.tangent) * capsule.radius +
        abs(capsule.up) * (capsule.half_segment + capsule.radius);
}
