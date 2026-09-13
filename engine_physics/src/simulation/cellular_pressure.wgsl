// Copyright Rob Gage 2026

struct Parameters {
    buffered_origin: vec2<i32>,
    buffered_tile_size: vec2<u32>,
    ring_offset: vec2<u32>,
    impulse_center: vec2<f32>,
    impulse_radius: f32,
    impulse_strength: f32,
    delta_time: f32,
    tick: u32,
    buffered_cell_count: u32,
    impulse_to_pressure: f32,
    damage_rate: f32,
    padding: u32,
}

@group(0) @binding(0) var<storage, read_write> material_ids: array<u32>;
@group(0) @binding(1) var<storage, read_write> appearances: array<u32>;
@group(0) @binding(2) var<storage, read_write> integrities: array<f32>;
@group(0) @binding(3) var<storage, read_write> kinematics: array<vec4<f32>>;
@group(0) @binding(4) var<storage, read> static_properties: array<vec4<u32>>;
@group(0) @binding(5) var<storage, read_write> pending_impulses: array<vec4<f32>>;
@group(0) @binding(6) var<storage, read_write> contact_loads: array<atomic<u32>>;
@group(0) @binding(7) var<storage, read_write> pressure_a: array<f32>;
@group(0) @binding(8) var<uniform> parameters: Parameters;

const EMPTY: u32 = 0u;
const STATIC_FORM: u32 = 1u;
const DYNAMIC_FORM: u32 = 2u;
const INVALID: u32 = 0xffffffffu;
const CONTACT_LOAD_SCALE: f32 = 1024.0;

@compute @workgroup_size(64)
fn apply_cellular_radial_impulse(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let index = invocation.x;
    if index >= parameters.buffered_cell_count { return; }
    let cell = world_cell(index);
    let delta = vec2<f32>(cell) + vec2<f32>(0.5) - parameters.impulse_center;
    let distance = length(delta);
    if distance > parameters.impulse_radius { return; }
    var direction = delta / max(distance, 0.0001);
    if distance < 0.0001 { direction = hash_direction(cell, parameters.tick); }
    let impulse = direction * parameters.impulse_strength *
        (1.0 - distance / max(parameters.impulse_radius, 0.0001));
    let physical = physical_index(cell);
    if physical == INVALID { return; }
    let material = material_ids[physical];
    if material >> 30u == DYNAMIC_FORM {
        var state = kinematics[physical];
        state.x += impulse.x;
        state.y += impulse.y;
        kinematics[physical] = state;
    } else if material >> 30u == STATIC_FORM {
        var pending = pending_impulses[physical];
        pending.x += impulse.x;
        pending.y += impulse.y;
        pending_impulses[physical] = pending;
    }
}

@compute @workgroup_size(64)
fn seed_cellular_pressure(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let index = invocation.x;
    if index >= parameters.buffered_cell_count { return; }
    let cell = world_cell(index);
    let physical = physical_index(cell);
    if physical == INVALID || material_ids[physical] >> 30u != STATIC_FORM {
        if physical != INVALID { pressure_a[physical] = 0.0; }
        return;
    }
    let impulse = pending_impulses[physical];
    let contact = f32(atomicLoad(&contact_loads[physical])) / CONTACT_LOAD_SCALE;
    pressure_a[physical] = length(impulse.xy) * parameters.impulse_to_pressure + contact;
}

@compute @workgroup_size(64)
fn accumulate_cellular_contacts(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let index = invocation.x;
    if index >= parameters.buffered_cell_count { return; }
    let cell = world_cell(index);
    let physical = physical_index(cell);
    if physical == INVALID || material_ids[physical] >> 30u != DYNAMIC_FORM { return; }
    let velocity = kinematics[physical].xy;
    for (var direction = 0u; direction < 4u; direction++) {
        let offset = select(select(vec2<i32>(0, -1), vec2<i32>(0, 1), direction == 1u),
            select(vec2<i32>(-1, 0), vec2<i32>(1, 0), direction == 3u), direction >= 2u);
        let neighbor = physical_index(cell + offset);
        if neighbor == INVALID || material_ids[neighbor] >> 30u != STATIC_FORM { continue; }
        let normal = normalize(vec2<f32>(offset));
        let approach = max(0.0, dot(velocity, normal));
        atomicAdd(&contact_loads[neighbor], u32(approach * CONTACT_LOAD_SCALE));
    }
}

@compute @workgroup_size(64)
fn clear_cellular_contact_loads(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if invocation.x < parameters.buffered_cell_count {
        atomicStore(&contact_loads[invocation.x], 0u);
    }
}

@compute @workgroup_size(64)
fn propagate_cellular_pressure(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let index = invocation.x;
    if index >= parameters.buffered_cell_count { return; }
    let cell = world_cell(index);
    let physical = physical_index(cell);
    if physical == INVALID || material_ids[physical] >> 30u != STATIC_FORM { return; }
    var pressure = pressure_a[physical];
    for (var dy = -1; dy <= 1; dy++) {
        for (var dx = -1; dx <= 1; dx++) {
            if dx == 0 && dy == 0 { continue; }
            let neighbor = physical_index(cell + vec2<i32>(dx, dy));
            if neighbor == INVALID || material_ids[neighbor] >> 30u != STATIC_FORM { continue; }
            let weight = select(0.60, 0.85, dx == 0 || dy == 0);
            pressure = max(pressure, pressure_a[neighbor] * weight);
        }
    }
    pending_impulses[physical].z = pressure;
}

@compute @workgroup_size(64)
fn commit_cellular_pressure(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let index = invocation.x;
    if index >= parameters.buffered_cell_count { return; }
    pressure_a[index] = pending_impulses[index].z;
}

@compute @workgroup_size(64)
fn damage_cellular_integrity(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let index = invocation.x;
    if index >= parameters.buffered_cell_count { return; }
    let cell = world_cell(index);
    let physical = physical_index(cell);
    if physical == INVALID { return; }
    let material = material_ids[physical];
    if material >> 30u == STATIC_FORM {
        let properties = static_properties[material & 0x3fffffffu];
        let overload = pressure_a[physical] - bitcast<f32>(properties.x);
        if overload > 0.0 {
            integrities[physical] -= overload * parameters.delta_time * parameters.damage_rate;
            if integrities[physical] <= 0.0 {
                let debris = properties.z;
                if debris != EMPTY && hash_float(cell, parameters.tick) < bitcast<f32>(properties.w) {
                    material_ids[physical] = debris;
                    kinematics[physical] = vec4<f32>(pending_impulses[physical].xy, 0.0, 0.0);
                    integrities[physical] = 0.0;
                } else {
                    material_ids[physical] = EMPTY;
                    appearances[physical] = 0u;
                    kinematics[physical] = vec4<f32>(0.0);
                    integrities[physical] = 0.0;
                }
            }
        }
    }
    pending_impulses[physical] = vec4<f32>(0.0);
    atomicStore(&contact_loads[physical], 0u);
}

fn world_cell(index: u32) -> vec2<i32> {
    let tile = index / 64u;
    return (parameters.buffered_origin + vec2<i32>(
        i32(tile % parameters.buffered_tile_size.x),
        i32(tile / parameters.buffered_tile_size.x),
    )) * 8 + vec2<i32>(i32(index % 8u), i32((index % 64u) / 8u));
}

fn physical_index(cell: vec2<i32>) -> u32 {
    let tile = vec2<i32>(floor_div(cell.x, 8), floor_div(cell.y, 8));
    let relative = tile - parameters.buffered_origin;
    if any(relative < vec2<i32>(0)) || relative.x >= i32(parameters.buffered_tile_size.x) ||
            relative.y >= i32(parameters.buffered_tile_size.y) { return INVALID; }
    let physical = (vec2<u32>(relative) + parameters.ring_offset) % parameters.buffered_tile_size;
    let local = vec2<u32>(cell - tile * 8);
    return (physical.y * parameters.buffered_tile_size.x + physical.x) * 64u + local.y * 8u + local.x;
}

fn floor_div(value: i32, divisor: i32) -> i32 {
    if value < 0 { return (value - divisor + 1) / divisor; }
    return value / divisor;
}

fn hash_direction(cell: vec2<i32>, tick: u32) -> vec2<f32> {
    let value = hash(cell, tick);
    return normalize(vec2<f32>(select(-1.0, 1.0, (value & 1u) == 0u),
        select(-1.0, 1.0, (value & 2u) == 0u)));
}

fn hash_float(cell: vec2<i32>, tick: u32) -> f32 {
    return f32(hash(cell, tick)) / 4294967295.0;
}

fn hash(cell: vec2<i32>, tick: u32) -> u32 {
    var value = u32(cell.x) * 0x9e3779b9u ^ u32(cell.y) * 0x85ebca6bu ^ tick * 0xc2b2ae35u;
    value ^= value >> 16u;
    value *= 0x7feb352du;
    value ^= value >> 15u;
    return value;
}
