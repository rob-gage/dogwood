// Copyright Rob Gage 2026

#define_import_path compute::rigid_granular_contact

#import utility::cell_coordinates::{
    CELL_COUNT_PER_TILE,
    CELLS_PER_TILE_FLOAT,
    world_cell_from_logical_tile_major_index,
}
#import utility::material_identifier::{
    CELLULAR_DYNAMIC_MATERIAL_FORM,
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
    padding: u32,
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

struct ContactGeometry {
    owner: u32,
    channel: u32,
    normal: vec2<f32>,
    point: vec2<f32>,
}

@group(0) @binding(0) var<storage, read> cellular_material_identifiers: array<u32>;
@group(0) @binding(1) var<storage, read_write> cellular_kinematics: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read> contact_velocity_snapshot: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read> dynamic_properties: array<vec4<f32>>;
@group(0) @binding(4) var<storage, read> static_properties: array<StaticProperties>;
@group(0) @binding(5) var<storage, read> rigid_owners: array<u32>;
@group(0) @binding(6) var<storage, read> rigid_material_identifiers: array<u32>;
@group(0) @binding(7) var<storage, read> rigid_transforms: array<vec4<f32>>;
@group(0) @binding(8) var<storage, read_write> contact_statistics: array<ContactStatistics>;
@group(0) @binding(9) var<storage, read_write> reactions: array<Reaction>;
@group(0) @binding(10) var<storage, read> active_tile_indices: array<u32>;
@group(0) @binding(11) var<uniform> parameters: Parameters;

const CONTACT_EPSILON: f32 = 0.0001;
const LINEAR_FIXED_SCALE: f32 = 256.0;
const ANGULAR_FIXED_SCALE: f32 = 64.0;
const MAXIMUM_FIXED_VALUE: f32 = 2147483520.0;

@compute @workgroup_size(64)
fn clear_rigid_granular_contact_statistics(
    @builtin(global_invocation_id) invocation: vec3<u32>,
) {
    let body: u32 = invocation.x;
    if body >= parameters.body_count { return; }
    atomicStore(&contact_statistics[body].contact_count, 0u);
    atomicStore(&contact_statistics[body].padding_0, 0u);
    for (var channel: u32 = 0u; channel < 4u; channel++) {
        atomicStore(&contact_statistics[body].geometric_support[channel], 0u);
        atomicStore(&contact_statistics[body].motion_support[channel], 0u);
    }
}

@compute @workgroup_size(64)
fn gather_rigid_granular_contact_statistics(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    let logical_index: u32 = active_tile_indices[workgroup.x] * CELL_COUNT_PER_TILE + local_index;
    if logical_index >= parameters.buffered_cell_count { return; }
    let cell: vec2<i32> = world_cell_from_logical_tile_major_index(
        logical_index, parameters.buffered_origin, parameters.buffered_tile_size,
    );
    let index: u32 = contact_physical_index(cell);
    if !is_dynamic_grain(index) { return; }
    let grain_velocity: vec2<f32> = contact_velocity_snapshot[index].xy;
    for (var channel: u32 = 0u; channel < 4u; channel++) {
        let contact: ContactGeometry = contact_geometry(cell, index, channel);
        if contact.owner == 0u || contact.owner > parameters.body_count { continue; }
        let body: u32 = contact.owner - 1u;
        let transform: vec4<f32> = rigid_transforms[body * 3u + 1u];
        let center_of_mass: vec2<f32> = rigid_transforms[body * 3u + 2u].xy;
        let radius: vec2<f32> = contact.point - center_of_mass;
        let body_velocity: vec2<f32> = transform.xy +
            transform.z * vec2<f32>(-radius.y, radius.x);
        let grain_normal: f32 = dot(grain_velocity, contact.normal);
        let body_normal: f32 = dot(body_velocity, contact.normal);
        let relative_normal: f32 = grain_normal - body_normal;
        let grain_toward: f32 = max(0.0, grain_normal);
        let body_toward: f32 = max(0.0, -body_normal);
        let toward_sum: f32 = grain_toward + body_toward;
        var body_approach: f32 = 0.0;
        if toward_sum > CONTACT_EPSILON {
            body_approach = max(0.0, relative_normal) * body_toward / toward_sum;
        }
        atomicAdd(&contact_statistics[body].contact_count, 1u);
        if relative_normal >= -CONTACT_EPSILON {
            atomicAdd(&contact_statistics[body].geometric_support[contact.channel], 1u);
        }
        if body_approach > CONTACT_EPSILON {
            atomicAdd(&contact_statistics[body].motion_support[contact.channel], 1u);
        }
    }
}

@compute @workgroup_size(64)
fn resolve_rigid_granular_contacts(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    let logical_index: u32 = active_tile_indices[workgroup.x] * CELL_COUNT_PER_TILE + local_index;
    if logical_index >= parameters.buffered_cell_count { return; }
    let cell: vec2<i32> = world_cell_from_logical_tile_major_index(
        logical_index, parameters.buffered_origin, parameters.buffered_tile_size,
    );
    let index: u32 = contact_physical_index(cell);
    if !is_dynamic_grain(index) { return; }
    let grain_material: u32 = cellular_material_identifiers[index];
    let grain_properties: vec4<f32> =
        dynamic_properties[material_index_from_identifier(grain_material)];
    let grain_mass: f32 = grain_properties.x;
    if grain_mass != grain_mass || abs(grain_mass) > 3.402823e+38 ||
            grain_mass <= CONTACT_EPSILON { return; }
    let grain_start_velocity: vec2<f32> = contact_velocity_snapshot[index].xy;
    var grain_velocity: vec2<f32> = cellular_kinematics[index].xy;
    for (var channel: u32 = 0u; channel < 4u; channel++) {
        let contact: ContactGeometry = contact_geometry(cell, index, channel);
        if contact.owner == 0u || contact.owner > parameters.body_count { continue; }
        let body: u32 = contact.owner - 1u;
        let motion: vec4<f32> = rigid_transforms[body * 3u + 1u];
        let mass_record: vec4<f32> = rigid_transforms[body * 3u + 2u];
        let radius: vec2<f32> = contact.point - mass_record.xy;
        let body_velocity: vec2<f32> = motion.xy +
            motion.z * vec2<f32>(-radius.y, radius.x);
        let relative_velocity: vec2<f32> = grain_start_velocity - body_velocity;
        let grain_normal: f32 = dot(grain_start_velocity, contact.normal);
        let body_normal: f32 = dot(body_velocity, contact.normal);
        let relative_approach: f32 = max(0.0, grain_normal - body_normal);
        let grain_toward: f32 = max(0.0, grain_normal);
        let body_toward: f32 = max(0.0, -body_normal);
        let toward_sum: f32 = grain_toward + body_toward;
        var grain_approach: f32 = 0.0;
        var body_approach: f32 = 0.0;
        if toward_sum > CONTACT_EPSILON {
            grain_approach = relative_approach * grain_toward / toward_sum;
            body_approach = relative_approach * body_toward / toward_sum;
        }
        let rotational_arm: f32 = radius.x * contact.normal.y -
            radius.y * contact.normal.x;
        let inverse_effective_mass: f32 = mass_record.z +
            mass_record.w * rotational_arm * rotational_arm;
        var grain_impulse: f32 = 0.0;
        if grain_approach > 0.0 && inverse_effective_mass >= 0.0 {
            let rigid_material: u32 = rigid_material_identifiers[
                contact_physical_index(vec2<i32>(floor(contact.point * CELLS_PER_TILE_FLOAT +
                    contact.normal * 0.5)))
            ];
            let rigid_restitution: f32 = rigid_material_restitution(rigid_material);
            grain_impulse = (1.0 + min(grain_properties.w, rigid_restitution)) * grain_approach /
                (1.0 / grain_mass + inverse_effective_mass);
        }
        var motion_impulse: f32 = 0.0;
        let motion_count: u32 = atomicLoad(
            &contact_statistics[body].motion_support[contact.channel],
        );
        if body_approach > CONTACT_EPSILON && inverse_effective_mass > CONTACT_EPSILON &&
                motion_count != 0u {
            motion_impulse = body_approach / inverse_effective_mass / f32(motion_count);
        }
        var gravity_impulse: f32 = 0.0;
        let geometric_count: u32 = atomicLoad(
            &contact_statistics[body].geometric_support[contact.channel],
        );
        if geometric_count != 0u && mass_record.z > CONTACT_EPSILON &&
                grain_normal - body_normal >= -CONTACT_EPSILON {
            let body_mass: f32 = 1.0 / mass_record.z;
            gravity_impulse = max(
                0.0,
                dot(-body_mass * parameters.gravity * parameters.delta_time, contact.normal),
            ) / f32(geometric_count);
        }
        let normal_impulse: vec2<f32> = contact.normal *
            (grain_impulse + motion_impulse + gravity_impulse);
        grain_velocity -= normal_impulse / grain_mass;
        accumulate_rigid_reaction(body, normal_impulse, radius);

        let rigid_material_index: u32 = contact_physical_index(vec2<i32>(floor(
            contact.point * CELLS_PER_TILE_FLOAT + contact.normal * 0.5,
        )));
        let rigid_friction: f32 = rigid_material_friction(
            rigid_material_identifiers[rigid_material_index],
        );
        let tangent_velocity: vec2<f32> = relative_velocity -
            contact.normal * dot(relative_velocity, contact.normal);
        let friction_factor: f32 = min(
            1.0,
            min(grain_properties.z, rigid_friction) * parameters.delta_time *
                CELLS_PER_TILE_FLOAT,
        );
        let grain_tangent_impulse: vec2<f32> =
            grain_mass * (-tangent_velocity * friction_factor);
        grain_velocity += grain_tangent_impulse / grain_mass;
        accumulate_rigid_reaction(body, -grain_tangent_impulse, radius);
    }
    cellular_kinematics[index].x = grain_velocity.x;
    cellular_kinematics[index].y = grain_velocity.y;
}

fn contact_geometry(cell: vec2<i32>, index: u32, channel: u32) -> ContactGeometry {
    let direction: vec2<i32> = cardinal_direction(channel);
    let owner_here: u32 = rigid_owners[index];
    if owner_here == 0u {
        let neighbor: u32 = contact_physical_index(cell + direction);
        if neighbor == INVALID_PHYSICAL_CELL_INDEX || rigid_owners[neighbor] == 0u {
            return ContactGeometry(0u, channel, vec2<f32>(0.0), vec2<f32>(0.0));
        }
        let normal: vec2<f32> = vec2<f32>(direction);
        return ContactGeometry(
            rigid_owners[neighbor], channel, normal,
            (vec2<f32>(cell) + vec2<f32>(0.5) + normal * 0.5) / CELLS_PER_TILE_FLOAT,
        );
    }
    let outside: u32 = contact_physical_index(cell + direction);
    if outside != INVALID_PHYSICAL_CELL_INDEX && rigid_owners[outside] == owner_here {
        return ContactGeometry(0u, channel, vec2<f32>(0.0), vec2<f32>(0.0));
    }
    let outward: vec2<f32> = vec2<f32>(direction);
    let normal: vec2<f32> = -outward;
    return ContactGeometry(
        owner_here, opposite_channel(channel), normal,
        (vec2<f32>(cell) + vec2<f32>(0.5) + outward * 0.5) / CELLS_PER_TILE_FLOAT,
    );
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

fn is_dynamic_grain(index: u32) -> bool {
    return index != INVALID_PHYSICAL_CELL_INDEX &&
        material_form_from_identifier(cellular_material_identifiers[index]) ==
            CELLULAR_DYNAMIC_MATERIAL_FORM;
}

fn rigid_material_friction(material: u32) -> f32 {
    if material_form_from_identifier(material) != CELLULAR_STATIC_MATERIAL_FORM { return 0.0; }
    return static_properties[material_index_from_identifier(material)].friction;
}

fn rigid_material_restitution(material: u32) -> f32 {
    if material_form_from_identifier(material) != CELLULAR_STATIC_MATERIAL_FORM { return 0.0; }
    return static_properties[material_index_from_identifier(material)].restitution;
}

fn cardinal_direction(channel: u32) -> vec2<i32> {
    if channel == 0u { return vec2<i32>(1, 0); }
    if channel == 1u { return vec2<i32>(-1, 0); }
    if channel == 2u { return vec2<i32>(0, 1); }
    return vec2<i32>(0, -1);
}

fn opposite_channel(channel: u32) -> u32 {
    if channel == 0u { return 1u; }
    if channel == 1u { return 0u; }
    if channel == 2u { return 3u; }
    return 2u;
}

fn contact_physical_index(cell: vec2<i32>) -> u32 {
    return physical_cell_index_from_world_cell(
        cell, parameters.buffered_origin, parameters.buffered_tile_size, parameters.ring_offset,
    );
}
