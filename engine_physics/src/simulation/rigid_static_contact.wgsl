// Copyright Rob Gage 2026

#define_import_path compute::rigid_static_contact

#import utility::material_identifier::{
    CELLULAR_STATIC_MATERIAL_FORM,
    material_form_from_identifier,
    material_index_from_identifier,
}
#import utility::tile_ring::{INVALID_PHYSICAL_CELL_INDEX, physical_cell_index_from_world_cell}

struct Parameters {
    buffered_origin: vec2<i32>,
    buffered_tile_size: vec2<u32>,
    ring_offset: vec2<u32>,
    gravity: vec2<f32>,
    delta_time: f32,
    body_count: u32,
    buffered_cell_count: u32,
    rigid_cell_count: u32,
}

struct StaticProperties {
    pressure_ignore_threshold: f32,
    pressure_transmission: f32,
    debris: u32,
    debris_yield_rate: f32,
    friction: f32,
    restitution: f32,
    padding: vec2<f32>,
}

struct ContactStatistics {
    contact_count: atomic<u32>,
    padding_0: atomic<u32>,
    padding_1: atomic<u32>,
    padding_2: atomic<u32>,
    geometric_support: array<atomic<u32>, 4>,
    motion_support: array<atomic<u32>, 4>,
}

struct Reaction {
    impulse_x: atomic<i32>,
    impulse_y: atomic<i32>,
    angular_impulse: atomic<i32>,
    overflow: atomic<i32>,
}

struct Contact {
    found: u32,
    body: u32,
    material: u32,
    channel: u32,
    normal: vec2<f32>,
    point: vec2<f32>,
    penetration: f32,
}

@group(0) @binding(0) var<storage, read> cellular_material_identifiers: array<u32>;
@group(0) @binding(4) var<storage, read> static_properties: array<StaticProperties>;
@group(0) @binding(7) var<storage, read> rigid_transforms: array<vec4<f32>>;
@group(0) @binding(8) var<storage, read_write> contact_statistics: array<ContactStatistics>;
@group(0) @binding(9) var<storage, read_write> reactions: array<Reaction>;
@group(0) @binding(11) var<uniform> parameters: Parameters;
@group(0) @binding(12) var<storage, read> rigid_cells: array<vec4<u32>>;

const CELL_SIZE: f32 = 0.125;
const CELL_HALF: f32 = 0.0625;
const CELL_RADIUS: f32 = 0.08838835;
const CONTACT_EPSILON: f32 = 0.0001;
const LINEAR_FIXED_SCALE: f32 = 256.0;
const ANGULAR_FIXED_SCALE: f32 = 64.0;
const MAXIMUM_FIXED_VALUE: f32 = 2147483520.0;

@compute @workgroup_size(64)
fn gather_rigid_static_contact_statistics(
    @builtin(global_invocation_id) invocation: vec3<u32>,
) {
    let contact: Contact = find_contact(invocation.x);
    if contact.found == 0u { return; }
    let motion: vec4<f32> = rigid_transforms[contact.body * 3u + 1u];
    let center_of_mass: vec2<f32> = rigid_transforms[contact.body * 3u + 2u].xy;
    let radius: vec2<f32> = contact.point - center_of_mass;
    let velocity: vec2<f32> = motion.xy + motion.z * vec2<f32>(-radius.y, radius.x);
    let approach: f32 = max(0.0, -dot(velocity, contact.normal));
    atomicAdd(&contact_statistics[contact.body].contact_count, 1u);
    atomicAdd(&contact_statistics[contact.body].padding_0, 1u);
    atomicAdd(&contact_statistics[contact.body].geometric_support[contact.channel], 1u);
    if approach > CONTACT_EPSILON || contact.penetration > CONTACT_EPSILON {
        atomicAdd(&contact_statistics[contact.body].motion_support[contact.channel], 1u);
    }
}

@compute @workgroup_size(64)
fn resolve_rigid_static_contacts(
    @builtin(global_invocation_id) invocation: vec3<u32>,
) {
    let contact: Contact = find_contact(invocation.x);
    if contact.found == 0u { return; }
    let motion: vec4<f32> = rigid_transforms[contact.body * 3u + 1u];
    let mass: vec4<f32> = rigid_transforms[contact.body * 3u + 2u];
    let radius: vec2<f32> = contact.point - mass.xy;
    let velocity: vec2<f32> = motion.xy + motion.z * vec2<f32>(-radius.y, radius.x);
    let approach: f32 = max(0.0, -dot(velocity, contact.normal));
    let arm: f32 = radius.x * contact.normal.y - radius.y * contact.normal.x;
    let inverse_effective_mass: f32 = mass.z + mass.w * arm * arm;
    if inverse_effective_mass <= CONTACT_EPSILON { return; }
    let properties: StaticProperties =
        static_properties[material_index_from_identifier(contact.material)];
    let rigid_material: u32 = rigid_cells[invocation.x * 2u].w;
    let rigid_properties: StaticProperties =
        static_properties[material_index_from_identifier(rigid_material)];
    let motion_count: u32 = atomicLoad(
        &contact_statistics[contact.body].motion_support[contact.channel],
    );
    var normal_magnitude: f32 = 0.0;
    if motion_count != 0u {
        let restitution: f32 = min(properties.restitution, rigid_properties.restitution);
        let bias_velocity: f32 = min(
            CELL_SIZE / parameters.delta_time,
            max(0.0, contact.penetration - CELL_SIZE * 0.02) * 0.2 / parameters.delta_time,
        );
        normal_magnitude += ((1.0 + restitution) * approach + bias_velocity) /
            inverse_effective_mass / f32(motion_count);
    }
    let geometric_count: u32 = atomicLoad(
        &contact_statistics[contact.body].geometric_support[contact.channel],
    );
    if geometric_count != 0u && mass.z > CONTACT_EPSILON {
        normal_magnitude += max(
            0.0,
            dot(-(parameters.gravity * parameters.delta_time) / mass.z, contact.normal),
        ) / f32(geometric_count);
    }
    let normal_impulse: vec2<f32> = contact.normal * normal_magnitude;
    accumulate_rigid_reaction(contact.body, normal_impulse, radius);

    let tangent: vec2<f32> = vec2<f32>(-contact.normal.y, contact.normal.x);
    let tangent_arm: f32 = radius.x * tangent.y - radius.y * tangent.x;
    let tangent_inverse_mass: f32 = mass.z + mass.w * tangent_arm * tangent_arm;
    if tangent_inverse_mass > CONTACT_EPSILON {
        let requested: f32 = -dot(velocity, tangent) / tangent_inverse_mass;
        let friction: f32 = min(properties.friction, rigid_properties.friction);
        let friction_impulse: f32 = clamp(
            requested, -friction * normal_magnitude, friction * normal_magnitude,
        );
        accumulate_rigid_reaction(contact.body, tangent * friction_impulse, radius);
    }
}

fn find_contact(source: u32) -> Contact {
    if source >= parameters.rigid_cell_count { return empty_contact(); }
    let cell: vec4<u32> = rigid_cells[source * 2u];
    let body: u32 = cell.z;
    if body >= parameters.body_count { return empty_contact(); }
    let pose: vec4<f32> = rigid_transforms[body * 3u];
    let motion: vec4<f32> = rigid_transforms[body * 3u + 1u];
    let local_center: vec2<f32> =
        (vec2<f32>(bitcast<vec2<i32>>(cell.xy)) + vec2<f32>(0.5)) * CELL_SIZE;
    let axis_x: vec2<f32> = pose.zw;
    let axis_y: vec2<f32> = vec2<f32>(-axis_x.y, axis_x.x);
    let current: vec2<f32> = pose.xy + axis_x * local_center.x + axis_y * local_center.y;
    let angle_step: f32 = clamp(motion.z * parameters.delta_time, -3.14159265, 3.14159265);
    let previous_axis_x: vec2<f32> = vec2<f32>(
        axis_x.x * cos(angle_step) + axis_x.y * sin(angle_step),
        -axis_x.x * sin(angle_step) + axis_x.y * cos(angle_step),
    );
    let previous_axis_y: vec2<f32> = vec2<f32>(-previous_axis_x.y, previous_axis_x.x);
    let previous_unbounded: vec2<f32> = pose.xy - motion.xy * parameters.delta_time +
        previous_axis_x * local_center.x + previous_axis_y * local_center.y;
    let sweep: vec2<f32> = current - previous_unbounded;
    // ponytail: the fixed 16-cell sweep bounds per-source work; raise it if gameplay exceeds 120 tiles/s.
    let sweep_scale: f32 = min(1.0, 2.0 / max(length(sweep), CONTACT_EPSILON));
    let previous: vec2<f32> = current - sweep * sweep_scale;
    let rotational_expansion: f32 = min(
        CELL_SIZE * 2.0,
        abs(angle_step) * length(local_center),
    );
    let extent: f32 = CELL_RADIUS + rotational_expansion;
    let minimum: vec2<i32> = vec2<i32>(floor((min(previous, current) - vec2<f32>(extent)) * 8.0));
    let maximum: vec2<i32> = vec2<i32>(floor((max(previous, current) + vec2<f32>(extent)) * 8.0));
    var best: Contact = empty_contact();
    var best_time: f32 = 2.0;
    var y: i32 = minimum.y;
    loop {
        if y > maximum.y { break; }
        var x: i32 = minimum.x;
        loop {
            if x > maximum.x { break; }
            let world_cell: vec2<i32> = vec2<i32>(x, y);
            let index: u32 = physical_cell_index_from_world_cell(
                world_cell, parameters.buffered_origin, parameters.buffered_tile_size,
                parameters.ring_offset,
            );
            if index != INVALID_PHYSICAL_CELL_INDEX {
                let material: u32 = cellular_material_identifiers[index];
                if material_form_from_identifier(material) == CELLULAR_STATIC_MATERIAL_FORM {
                    let static_center: vec2<f32> = (vec2<f32>(world_cell) + vec2<f32>(0.5)) * CELL_SIZE;
                    let overlap: vec4<f32> = overlap_contact(
                        current, axis_x, axis_y, static_center,
                    );
                    var hit: vec4<f32> = overlap;
                    var time: f32 = 0.0;
                    if overlap.w < 0.0 {
                        hit = swept_contact(previous, current, static_center, extent);
                        time = hit.z;
                    }
                    if hit.w >= 0.0 && time < best_time {
                        best_time = time;
                        let normal: vec2<f32> = hit.xy;
                        let point_center: vec2<f32> = mix(previous, current, time);
                        best = Contact(1u, body, material, normal_channel(normal), normal,
                            point_center - normal * CELL_RADIUS, hit.w);
                    }
                }
            }
            x += 1;
        }
        y += 1;
    }
    return best;
}

fn overlap_contact(
    center: vec2<f32>, axis_x: vec2<f32>, axis_y: vec2<f32>, static_center: vec2<f32>,
) -> vec4<f32> {
    let difference: vec2<f32> = center - static_center;
    var best_axis: vec2<f32> = vec2<f32>(0.0);
    var best_penetration: f32 = 3.402823e+38;
    let axes: array<vec2<f32>, 4> = array<vec2<f32>, 4>(
        vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0), axis_x, axis_y,
    );
    for (var index: u32 = 0u; index < 4u; index++) {
        let axis: vec2<f32> = axes[index];
        let rigid_radius: f32 = CELL_HALF * (abs(dot(axis_x, axis)) + abs(dot(axis_y, axis)));
        let static_radius: f32 = CELL_HALF * (abs(axis.x) + abs(axis.y));
        let penetration: f32 = rigid_radius + static_radius - abs(dot(difference, axis));
        if penetration < 0.0 { return vec4<f32>(0.0, 0.0, 0.0, -1.0); }
        if penetration < best_penetration {
            best_penetration = penetration;
            best_axis = select(-axis, axis, dot(difference, axis) >= 0.0);
        }
    }
    return vec4<f32>(best_axis, 0.0, best_penetration);
}

fn swept_contact(
    start: vec2<f32>, finish: vec2<f32>, static_center: vec2<f32>, extent: f32,
) -> vec4<f32> {
    let movement: vec2<f32> = finish - start;
    let minimum: vec2<f32> = static_center - vec2<f32>(CELL_HALF + extent);
    let maximum: vec2<f32> = static_center + vec2<f32>(CELL_HALF + extent);
    var enter: f32 = 0.0;
    var leave: f32 = 1.0;
    var normal: vec2<f32> = vec2<f32>(0.0);
    for (var axis: u32 = 0u; axis < 2u; axis++) {
        if abs(movement[axis]) <= CONTACT_EPSILON {
            if start[axis] < minimum[axis] || start[axis] > maximum[axis] {
                return vec4<f32>(0.0, 0.0, 2.0, -1.0);
            }
            continue;
        }
        let first: f32 = (minimum[axis] - start[axis]) / movement[axis];
        let second: f32 = (maximum[axis] - start[axis]) / movement[axis];
        let axis_enter: f32 = min(first, second);
        let axis_leave: f32 = max(first, second);
        if axis_enter > enter {
            enter = axis_enter;
            normal = vec2<f32>(0.0);
            normal[axis] = select(1.0, -1.0, movement[axis] > 0.0);
        }
        leave = min(leave, axis_leave);
    }
    if enter > leave || enter < 0.0 || enter > 1.0 {
        return vec4<f32>(0.0, 0.0, 2.0, -1.0);
    }
    return vec4<f32>(normal, enter, 0.0);
}

fn normal_channel(normal: vec2<f32>) -> u32 {
    if abs(normal.x) >= abs(normal.y) { return select(1u, 0u, normal.x >= 0.0); }
    return select(3u, 2u, normal.y >= 0.0);
}

fn empty_contact() -> Contact {
    return Contact(0u, 0u, 0u, 0u, vec2<f32>(0.0), vec2<f32>(0.0), -1.0);
}

fn accumulate_rigid_reaction(body: u32, impulse: vec2<f32>, radius: vec2<f32>) {
    if any(impulse != impulse) || any(abs(impulse) > vec2<f32>(3.402823e+38)) {
        atomicStore(&reactions[body].overflow, 1);
        return;
    }
    let angular_impulse: f32 = radius.x * impulse.y - radius.y * impulse.x;
    saturating_atomic_add(body, 0u, impulse.x * LINEAR_FIXED_SCALE);
    saturating_atomic_add(body, 1u, impulse.y * LINEAR_FIXED_SCALE);
    saturating_atomic_add(body, 2u, angular_impulse * ANGULAR_FIXED_SCALE);
}

fn saturating_atomic_add(body: u32, component: u32, value: f32) {
    if value != value || abs(value) > MAXIMUM_FIXED_VALUE {
        atomicStore(&reactions[body].overflow, 1);
        return;
    }
    let increment: i32 = i32(round(value));
    if component == 0u {
        var current: i32 = atomicLoad(&reactions[body].impulse_x);
        loop {
            let sum: vec2<i32> = saturating_fixed_sum(current, increment);
            let result = atomicCompareExchangeWeak(
                &reactions[body].impulse_x, current, sum.x,
            );
            if result.exchanged {
                if sum.y != 0 { atomicStore(&reactions[body].overflow, 1); }
                return;
            }
            current = result.old_value;
        }
    }
    if component == 1u {
        var current: i32 = atomicLoad(&reactions[body].impulse_y);
        loop {
            let sum: vec2<i32> = saturating_fixed_sum(current, increment);
            let result = atomicCompareExchangeWeak(
                &reactions[body].impulse_y, current, sum.x,
            );
            if result.exchanged {
                if sum.y != 0 { atomicStore(&reactions[body].overflow, 1); }
                return;
            }
            current = result.old_value;
        }
    }
    var current: i32 = atomicLoad(&reactions[body].angular_impulse);
    loop {
        let sum: vec2<i32> = saturating_fixed_sum(current, increment);
        let result = atomicCompareExchangeWeak(
            &reactions[body].angular_impulse, current, sum.x,
        );
        if result.exchanged {
            if sum.y != 0 { atomicStore(&reactions[body].overflow, 1); }
            return;
        }
        current = result.old_value;
    }
}

fn saturating_fixed_sum(current: i32, increment: i32) -> vec2<i32> {
    if increment > 0 && current > 2147483647 - increment {
        return vec2<i32>(2147483647, 1);
    }
    if increment < 0 && current < -2147483647 - increment {
        return vec2<i32>(-2147483647, 1);
    }
    return vec2<i32>(current + increment, 0);
}
