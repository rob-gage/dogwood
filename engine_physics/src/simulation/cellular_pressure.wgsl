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
    damage_rate: f32,
    padding: vec2<u32>,
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

@group(0) @binding(0) var<storage, read_write> cellular_material_identifiers: array<u32>;
@group(0) @binding(1) var<storage, read_write> cellular_appearances: array<u32>;
@group(0) @binding(2) var<storage, read_write> cellular_integrities: array<f32>;
@group(0) @binding(3) var<storage, read_write> cellular_kinematics: array<vec4<f32>>;
@group(0) @binding(4) var<storage, read> cellular_static_properties: array<StaticProperties>;
@group(0) @binding(5) var<storage, read> cellular_dynamic_properties: array<vec4<f32>>;
@group(0) @binding(6) var<storage, read_write> pending_pressure: array<vec4<f32>>;
@group(0) @binding(7) var<storage, read_write> pressure_a: array<vec4<f32>>;
@group(0) @binding(8) var<storage, read_write> pressure_b: array<vec4<f32>>;
@group(0) @binding(9) var<storage, read_write> retained_pressure: array<vec4<f32>>;
@group(0) @binding(10) var<storage, read> external_body_occupancy: array<u32>;
@group(0) @binding(11) var<storage, read> external_body_velocity: array<vec4<f32>>;
@group(0) @binding(12) var<storage, read> external_body_count: array<atomic<u32>>;
@group(0) @binding(13) var<uniform> parameters: Parameters;
@group(0) @binding(14) var<storage, read_write> active_pressure_tiles: array<atomic<u32>>;

const EMPTY: u32 = 0u;
const CELLULAR_STATIC_FORM: u32 = 1u;
const CELLULAR_DYNAMIC_FORM: u32 = 2u;
const MATERIAL_INDEX_MASK: u32 = 0x3fffffffu;
const INVALID_INDEX: u32 = 0xffffffffu;
const IMMOVABLE_CONTACT_MASS: f32 = 1000000.0;
const CONTACT_PRESSURE_TRANSFER: f32 = 0.02;

// Clears the transient coarse mask before current pressure sources are discovered
@compute @workgroup_size(64)
fn clear_active_pressure_tiles(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let tile_count: u32 = parameters.buffered_tile_size.x * parameters.buffered_tile_size.y;
    if invocation.x < tile_count { atomicStore(&active_pressure_tiles[invocation.x], 0u); }
}

// Marks source tiles and the one-tile halo reachable by the fixed six-pass stencil
@compute @workgroup_size(64)
fn mark_active_pressure_tiles(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let logical_tile_index: u32 = invocation.x;
    let tile_count: u32 = parameters.buffered_tile_size.x * parameters.buffered_tile_size.y;
    if logical_tile_index >= tile_count { return; }
    let logical_tile: vec2<u32> = vec2<u32>(
        logical_tile_index % parameters.buffered_tile_size.x,
        logical_tile_index / parameters.buffered_tile_size.x,
    );
    let physical_tile: vec2<u32> =
        (logical_tile + parameters.ring_offset) % parameters.buffered_tile_size;
    let cell_start: u32 =
        (physical_tile.y * parameters.buffered_tile_size.x + physical_tile.x) * 64u;
    var has_source: bool = false;
    for (var local: u32 = 0u; local < 64u; local++) {
        let index: u32 = cell_start + local;
        if any(pending_pressure[index] != vec4<f32>(0.0)) ||
                external_body_occupancy[index] != 0u {
            has_source = true;
            break;
        }
        let material: u32 = cellular_material_identifiers[index];
        if material >> 30u == CELLULAR_DYNAMIC_FORM &&
                any(cellular_kinematics[index].xy != vec2<f32>(0.0)) {
            has_source = true;
            break;
        }
    }
    if !has_source { return; }
    for (var offset_y: i32 = -1; offset_y <= 1; offset_y++) {
        for (var offset_x: i32 = -1; offset_x <= 1; offset_x++) {
            let active_tile: vec2<i32> = vec2<i32>(logical_tile) +
                vec2<i32>(offset_x, offset_y);
            if any(active_tile < vec2<i32>(0)) ||
                    active_tile.x >= i32(parameters.buffered_tile_size.x) ||
                    active_tile.y >= i32(parameters.buffered_tile_size.y) { continue; }
            atomicStore(&active_pressure_tiles[
                u32(active_tile.y) * parameters.buffered_tile_size.x + u32(active_tile.x)
            ], 1u);
        }
    }
}

// Queues editor impulse pressure without bypassing material transmission or mass response
@compute @workgroup_size(64)
fn queue_cellular_radial_impulse(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let logical_index: u32 = invocation.x;
    if logical_index >= parameters.buffered_cell_count { return; }
    let cell: vec2<i32> = world_cell(logical_index);
    let index: u32 = physical_index(cell);
    if index == INVALID_INDEX || cellular_material_identifiers[index] == EMPTY { return; }
    let delta: vec2<f32> = vec2<f32>(cell) + vec2<f32>(0.5) - parameters.impulse_center;
    let distance: f32 = length(delta);
    if distance > parameters.impulse_radius { return; }
    let direction: vec2<f32> = normalize(select(
        vec2<f32>(1.0, 0.0),
        delta,
        distance > 0.0001,
    ));
    let falloff: f32 = 1.0 - distance / max(parameters.impulse_radius, 0.0001);
    let impulse: vec2<f32> = direction * parameters.impulse_strength * falloff;
    pending_pressure[index] += encode_directional_pressure(impulse);
}

// Snapshots tick-start velocities into pressure scratch before contact resolution mutates them
@compute @workgroup_size(64)
fn copy_contact_velocity(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let logical_index: u32 = invocation.x;
    if logical_index >= parameters.buffered_cell_count { return; }
    if !pressure_tile_is_active(logical_index) { return; }
    let index: u32 = physical_index(world_cell(logical_index));
    if index == INVALID_INDEX { return; }
    pressure_b[index] = vec4<f32>(cellular_kinematics[index].xy, 0.0, 0.0);
}

// Resolves pairwise contact velocity and gathers its small pressure-transfer component
@compute @workgroup_size(64)
fn resolve_cellular_contacts(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let logical_index: u32 = invocation.x;
    if logical_index >= parameters.buffered_cell_count { return; }
    if !pressure_tile_is_active(logical_index) { return; }
    let cell: vec2<i32> = world_cell(logical_index);
    let index: u32 = physical_index(cell);
    if index == INVALID_INDEX { return; }
    let material: u32 = cellular_material_identifiers[index];
    if material >> 30u == CELLULAR_DYNAMIC_FORM {
        let velocity: vec2<f32> = resolve_dynamic_contact_velocity(cell, index);
        cellular_kinematics[index].x = velocity.x;
        cellular_kinematics[index].y = velocity.y;
    }
    pending_pressure[index] += gather_contact_pressure(cell, index);
}

// Converts queued sources into the first directional pressure field
@compute @workgroup_size(64)
fn seed_cellular_pressure(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let logical_index: u32 = invocation.x;
    if logical_index >= parameters.buffered_cell_count { return; }
    if !pressure_tile_is_active(logical_index) { return; }
    let index: u32 = physical_index(world_cell(logical_index));
    if index == INVALID_INDEX { return; }
    retained_pressure[index] = vec4<f32>(0.0);
    pressure_b[index] = vec4<f32>(0.0);
    var source: vec4<f32> = pending_pressure[index];
    pending_pressure[index] = vec4<f32>(0.0);
    // Divide one authoritative body impulse across its rasterized proxy cells
    if external_body_occupancy[index] != 0u {
        let occupied_count: u32 = max(1u, atomicLoad(&external_body_count[0]));
        source += encode_directional_pressure(
            external_body_velocity[index].zw / f32(occupied_count),
        );
    }
    pressure_a[index] = source;
}

// Alternating entry points preserve explicit pressure ping-pong ordering on the CPU
@compute @workgroup_size(64)
fn propagate_pressure_a(@builtin(global_invocation_id) invocation: vec3<u32>) {
    propagate_pressure(invocation.x, true);
}

// Runs the second ping-pong direction of one pressure propagation step
@compute @workgroup_size(64)
fn propagate_pressure_b(@builtin(global_invocation_id) invocation: vec3<u32>) {
    propagate_pressure(invocation.x, false);
}

// Applies all retained pressure after the fixed propagation budget has been consumed
@compute @workgroup_size(64)
fn apply_retained_pressure(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let logical_index: u32 = invocation.x;
    if logical_index >= parameters.buffered_cell_count { return; }
    if !pressure_tile_is_active(logical_index) { return; }
    let cell: vec2<i32> = world_cell(logical_index);
    let index: u32 = physical_index(cell);
    if index == INVALID_INDEX { return; }
    // Six propagation passes finish in A; the remaining in-flight pressure is retained here
    let load: vec4<f32> = retained_pressure[index] + pressure_a[index];
    let material: u32 = cellular_material_identifiers[index];
    let form: u32 = material >> 30u;
    if form == CELLULAR_DYNAMIC_FORM {
        let mass: f32 = cellular_dynamic_properties[material & MATERIAL_INDEX_MASK].x;
        let impulse: vec2<f32> = vec2<f32>(
            load.x - load.y,
            load.z - load.w,
        ) / mass;
        cellular_kinematics[index].x += impulse.x;
        cellular_kinematics[index].y += impulse.y;
    } else if form == CELLULAR_STATIC_FORM {
        apply_static_pressure(cell, index, material, load);
    }
    pressure_a[index] = vec4<f32>(0.0);
    pressure_b[index] = vec4<f32>(0.0);
    retained_pressure[index] = vec4<f32>(0.0);
}

// Applies conservative normal response and friction using immutable tick-start velocities
fn resolve_dynamic_contact_velocity(cell: vec2<i32>, index: u32) -> vec2<f32> {
    let mass: f32 = contact_mass(index);
    let old_velocity: vec2<f32> = contact_velocity(index);
    var velocity: vec2<f32> = old_velocity;
    for (var channel: u32 = 0u; channel < 4u; channel++) {
        let offset: vec2<i32> = pressure_direction(channel);
        let normal: vec2<f32> = vec2<f32>(offset);
        let neighbor: u32 = physical_index(cell + offset);
        if neighbor == INVALID_INDEX || pressure_transmission(neighbor) <= 0.0 { continue; }
        let relative_velocity: vec2<f32> = old_velocity - contact_velocity(neighbor);
        let approach_speed: f32 = max(0.0, dot(relative_velocity, normal));
        if approach_speed <= 0.0 { continue; }
        let neighbor_is_dynamic: bool =
            cellular_material_identifiers[neighbor] >> 30u == CELLULAR_DYNAMIC_FORM &&
            external_body_occupancy[neighbor] == 0u;
        let inverse_neighbor_mass: f32 = select(
            0.0,
            1.0 / contact_mass(neighbor),
            neighbor_is_dynamic,
        );
        let impulse: f32 =
            (1.0 + contact_restitution(index, neighbor)) * approach_speed /
            (1.0 / mass + inverse_neighbor_mass);
        velocity -= normal * (impulse / mass);
        let tangent_velocity: vec2<f32> =
            relative_velocity - normal * dot(relative_velocity, normal);
        let friction_factor: f32 = min(
            1.0,
            min(contact_friction(index), contact_friction(neighbor)) *
                parameters.delta_time * 8.0,
        );
        velocity -= tangent_velocity * friction_factor;
    }
    return velocity;
}

// Gathers dynamic impact pressure at its destination without float atomics or write conflicts
fn gather_contact_pressure(cell: vec2<i32>, index: u32) -> vec4<f32> {
    if pressure_transmission(index) <= 0.0 { return vec4<f32>(0.0); }
    let destination_velocity: vec2<f32> = contact_velocity(index);
    var gathered: vec4<f32> = vec4<f32>(0.0);
    for (var channel: u32 = 0u; channel < 4u; channel++) {
        let offset: vec2<i32> = pressure_direction(channel);
        let neighbor: u32 = physical_index(cell + offset);
        if neighbor == INVALID_INDEX ||
                cellular_material_identifiers[neighbor] >> 30u != CELLULAR_DYNAMIC_FORM {
            continue;
        }
        let toward_destination: vec2<f32> = -vec2<f32>(offset);
        let source_velocity: vec2<f32> = contact_velocity(neighbor);
        let approach_speed: f32 = max(
            0.0,
            dot(source_velocity - destination_velocity, toward_destination),
        );
        if approach_speed <= 0.0 { continue; }
        let source_mass: f32 = contact_mass(neighbor);
        let destination_is_dynamic: bool =
            cellular_material_identifiers[index] >> 30u == CELLULAR_DYNAMIC_FORM &&
            external_body_occupancy[index] == 0u;
        let inverse_destination_mass: f32 = select(
            0.0,
            1.0 / contact_mass(index),
            destination_is_dynamic,
        );
        let impulse: f32 =
            (1.0 + contact_restitution(neighbor, index)) * approach_speed /
            (1.0 / source_mass + inverse_destination_mass);
        gathered += encode_directional_pressure(
            toward_destination * impulse * CONTACT_PRESSURE_TRANSFER,
        );
    }
    return gathered;
}

// Damages or fractures one static cell using scalar compression from all four channels
fn apply_static_pressure(
    cell: vec2<i32>,
    index: u32,
    material: u32,
    load: vec4<f32>,
) {
    let properties: StaticProperties =
        cellular_static_properties[material & MATERIAL_INDEX_MASK];
    let compression: f32 = load.x + load.y + load.z + load.w;
    let overload: f32 = compression - properties.pressure_ignore_threshold;
    if overload <= 0.0 { return; }
    cellular_integrities[index] -= overload * parameters.delta_time * parameters.damage_rate;
    if cellular_integrities[index] > 0.0 { return; }
    if properties.debris != EMPTY &&
            hash_float(cell, parameters.tick) < properties.debris_yield_rate {
        cellular_material_identifiers[index] = properties.debris;
        cellular_kinematics[index] = vec4<f32>(
            load.x - load.y,
            load.z - load.w,
            0.0,
            0.0,
        );
    } else {
        cellular_material_identifiers[index] = EMPTY;
        cellular_appearances[index] = EMPTY;
        cellular_kinematics[index] = vec4<f32>(0.0);
    }
    cellular_integrities[index] = 0.0;
}

// Retains pressure that the source material or blocked stencil routes cannot transmit
fn propagate_pressure(logical_index: u32, from_a: bool) {
    if logical_index >= parameters.buffered_cell_count { return; }
    if !pressure_tile_is_active(logical_index) { return; }
    let cell: vec2<i32> = world_cell(logical_index);
    let index: u32 = physical_index(cell);
    if index == INVALID_INDEX { return; }
    let current: vec4<f32> = select(pressure_b[index], pressure_a[index], from_a);
    var local_retained: vec4<f32> = vec4<f32>(0.0);
    for (var channel: u32 = 0u; channel < 4u; channel++) {
        local_retained[channel] = current[channel] -
            outgoing_pressure(cell, channel, current[channel]);
    }
    retained_pressure[index] += local_retained;
    var gathered: vec4<f32> = vec4<f32>(0.0);
    for (var channel: u32 = 0u; channel < 4u; channel++) {
        gathered[channel] = incoming_pressure(cell, channel, from_a);
    }
    if from_a {
        pressure_b[index] = gathered;
    } else {
        pressure_a[index] = gathered;
    }
}

// Calculates the fraction that actually leaves one source through its fixed stencil
fn outgoing_pressure(cell: vec2<i32>, channel: u32, value: f32) -> f32 {
    let source_transmission: f32 = pressure_transmission(physical_index(cell));
    if source_transmission <= 0.0 { return 0.0; }
    var transmitted: f32 = 0.0;
    for (var side: i32 = -1; side <= 1; side++) {
        let destination_cell: vec2<i32> = cell + pressure_direction(channel) +
            pressure_side_offset(channel, side);
        let destination: u32 = physical_index(destination_cell);
        if destination != INVALID_INDEX && pressure_transmission(destination) > 0.0 {
            let weight: f32 = select(0.2, 0.6, side == 0);
            transmitted += value * source_transmission * weight;
        }
    }
    return transmitted;
}

// Gathers only valid source-routed pressure into one destination cell
fn incoming_pressure(cell: vec2<i32>, channel: u32, from_a: bool) -> f32 {
    if pressure_transmission(physical_index(cell)) <= 0.0 { return 0.0; }
    var gathered: f32 = 0.0;
    for (var side: i32 = -1; side <= 1; side++) {
        let source_cell: vec2<i32> = cell - pressure_direction(channel) -
            pressure_side_offset(channel, side);
        let source: u32 = physical_index(source_cell);
        if source == INVALID_INDEX { continue; }
        let source_transmission: f32 = pressure_transmission(source);
        if source_transmission <= 0.0 { continue; }
        let source_pressure: f32 = select(
            pressure_b[source][channel],
            pressure_a[source][channel],
            from_a,
        );
        let weight: f32 = select(0.2, 0.6, side == 0);
        gathered += source_pressure * source_transmission * weight;
    }
    return gathered;
}

// Returns source-material transmission, with transient body cells effectively perfect
fn pressure_transmission(index: u32) -> f32 {
    if index == INVALID_INDEX { return 0.0; }
    if external_body_occupancy[index] != 0u { return 1.0; }
    let material: u32 = cellular_material_identifiers[index];
    let form: u32 = material >> 30u;
    if form == CELLULAR_STATIC_FORM {
        return cellular_static_properties[material & MATERIAL_INDEX_MASK].pressure_transmission;
    }
    if form == CELLULAR_DYNAMIC_FORM {
        return cellular_dynamic_properties[material & MATERIAL_INDEX_MASK].y;
    }
    return 0.0;
}

// Returns the friction coefficient for one cellular contact medium
fn contact_friction(index: u32) -> f32 {
    if index == INVALID_INDEX || external_body_occupancy[index] != 0u { return 0.0; }
    let material: u32 = cellular_material_identifiers[index];
    let form: u32 = material >> 30u;
    if form == CELLULAR_STATIC_FORM {
        return cellular_static_properties[material & MATERIAL_INDEX_MASK].friction;
    }
    if form == CELLULAR_DYNAMIC_FORM {
        return cellular_dynamic_properties[material & MATERIAL_INDEX_MASK].z;
    }
    return 0.0;
}

// Returns the restitution coefficient for one cellular contact medium
fn material_restitution(index: u32) -> f32 {
    if index == INVALID_INDEX || external_body_occupancy[index] != 0u { return 0.0; }
    let material: u32 = cellular_material_identifiers[index];
    let form: u32 = material >> 30u;
    if form == CELLULAR_STATIC_FORM {
        return cellular_static_properties[material & MATERIAL_INDEX_MASK].restitution;
    }
    if form == CELLULAR_DYNAMIC_FORM {
        return cellular_dynamic_properties[material & MATERIAL_INDEX_MASK].w;
    }
    return 0.0;
}

// An immovable body uses the cellular material's restitution rather than suppressing bounce
fn contact_restitution(first: u32, second: u32) -> f32 {
    if external_body_occupancy[first] != 0u { return material_restitution(second); }
    if external_body_occupancy[second] != 0u { return material_restitution(first); }
    return min(material_restitution(first), material_restitution(second));
}

// Returns dynamic mass or the immovable mass used for static and proxy cells
fn contact_mass(index: u32) -> f32 {
    if index != INVALID_INDEX &&
            cellular_material_identifiers[index] >> 30u == CELLULAR_DYNAMIC_FORM &&
            external_body_occupancy[index] == 0u {
        return cellular_dynamic_properties[
            cellular_material_identifiers[index] & MATERIAL_INDEX_MASK
        ].x;
    }
    return IMMOVABLE_CONTACT_MASS;
}

// Reads the immutable contact velocity snapshot for a cellular or proxy cell
fn contact_velocity(index: u32) -> vec2<f32> {
    if external_body_occupancy[index] != 0u {
        return external_body_velocity[index].xy;
    }
    return pressure_b[index].xy;
}

// Channel order is +X, -X, +Y, -Y so opposing compression cannot cancel
fn encode_directional_pressure(value: vec2<f32>) -> vec4<f32> {
    return vec4<f32>(
        max(value.x, 0.0),
        max(-value.x, 0.0),
        max(value.y, 0.0),
        max(-value.y, 0.0),
    );
}

// Returns the world-cell direction represented by one pressure channel
fn pressure_direction(channel: u32) -> vec2<i32> {
    if channel == 0u { return vec2<i32>(1, 0); }
    if channel == 1u { return vec2<i32>(-1, 0); }
    if channel == 2u { return vec2<i32>(0, 1); }
    return vec2<i32>(0, -1);
}

// Returns the lateral stencil offset for one directional pressure channel
fn pressure_side_offset(channel: u32, side: i32) -> vec2<i32> {
    if channel < 2u { return vec2<i32>(0, side); }
    return vec2<i32>(side, 0);
}

// Converts logical tile-major dispatch order into a signed world cell
fn world_cell(index: u32) -> vec2<i32> {
    let tile: u32 = index / 64u;
    return (parameters.buffered_origin + vec2<i32>(
        i32(tile % parameters.buffered_tile_size.x),
        i32(tile / parameters.buffered_tile_size.x),
    )) * 8 + vec2<i32>(
        i32(index % 8u),
        i32((index % 64u) / 8u),
    );
}

// Maps one signed world cell through the two-dimensional physical tile ring
fn physical_index(cell: vec2<i32>) -> u32 {
    let tile: vec2<i32> = vec2<i32>(
        floor_divide(cell.x, 8),
        floor_divide(cell.y, 8),
    );
    let relative: vec2<i32> = tile - parameters.buffered_origin;
    if any(relative < vec2<i32>(0)) ||
            relative.x >= i32(parameters.buffered_tile_size.x) ||
            relative.y >= i32(parameters.buffered_tile_size.y) {
        return INVALID_INDEX;
    }
    let physical: vec2<u32> =
        (vec2<u32>(relative) + parameters.ring_offset) % parameters.buffered_tile_size;
    let local: vec2<u32> = vec2<u32>(cell - tile * 8);
    return (physical.y * parameters.buffered_tile_size.x + physical.x) * 64u +
        local.y * 8u + local.x;
}

// Performs mathematical floor division for signed world-cell coordinates
fn floor_divide(value: i32, divisor: i32) -> i32 {
    if value < 0 { return (value - divisor + 1) / divisor; }
    return value / divisor;
}

// Returns whether this logical cell lies in the coarse pressure work region
fn pressure_tile_is_active(logical_index: u32) -> bool {
    return atomicLoad(&active_pressure_tiles[logical_index / 64u]) != 0u;
}

// Produces a deterministic per-cell random value for fracture yield decisions
fn hash_float(cell: vec2<i32>, tick: u32) -> f32 {
    var value: u32 = u32(cell.x) * 0x9e3779b9u ^
        u32(cell.y) * 0x85ebca6bu ^ tick;
    value ^= value >> 16u;
    return f32(value) / 4294967295.0;
}
