// Copyright Rob Gage 2026

#define_import_path compute::cellular_pressure

#import utility::cell_coordinates::{
    CELL_COUNT_PER_TILE,
    CELLS_PER_TILE_FLOAT,
    world_cell_from_logical_tile_major_index,
}
#import utility::material_identifier::{
    CELLULAR_DYNAMIC_MATERIAL_FORM,
    CELLULAR_STATIC_MATERIAL_FORM,
    EMPTY_MATERIAL_IDENTIFIER,
    material_form_from_identifier,
    material_index_from_identifier,
}
#import utility::tile_ring::{
    INVALID_PHYSICAL_CELL_INDEX,
    physical_cell_index_from_world_cell,
    physical_tile_from_logical_tile,
}

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
@group(0) @binding(15) var<storage, read_write> active_pressure_tile_indices: array<u32>;
@group(1) @binding(0) var<storage, read_write> pressure_indirect_dispatch: array<atomic<u32>>;

const IMMOVABLE_CONTACT_MASS: f32 = 1000000.0;
const CONTACT_PRESSURE_TRANSFER: f32 = 0.02;

// Clears the transient coarse mask before current pressure sources are discovered
@compute @workgroup_size(64)
fn clear_active_cellular_pressure_tiles(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let tile_count: u32 = parameters.buffered_tile_size.x * parameters.buffered_tile_size.y;
    if invocation.x < tile_count { atomicStore(&active_pressure_tiles[invocation.x], 0u); }
}

// Marks source tiles and the one-tile halo reachable by the fixed six-pass stencil
@compute @workgroup_size(64)
fn mark_active_cellular_pressure_tiles(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let logical_tile_index: u32 = invocation.x;
    let tile_count: u32 = parameters.buffered_tile_size.x * parameters.buffered_tile_size.y;
    if logical_tile_index >= tile_count { return; }
    let logical_tile: vec2<u32> = vec2<u32>(
        logical_tile_index % parameters.buffered_tile_size.x,
        logical_tile_index / parameters.buffered_tile_size.x,
    );
    let physical_tile: vec2<u32> = physical_tile_from_logical_tile(
        logical_tile, parameters.buffered_tile_size, parameters.ring_offset,
    );
    let cell_start: u32 =
        (physical_tile.y * parameters.buffered_tile_size.x + physical_tile.x) *
            CELL_COUNT_PER_TILE;
    var has_source: bool = false;
    for (var local: u32 = 0u; local < CELL_COUNT_PER_TILE; local++) {
        let index: u32 = cell_start + local;
        if any(pending_pressure[index] != vec4<f32>(0.0)) ||
                external_body_occupancy[index] != 0u {
            has_source = true;
            break;
        }
        let material: u32 = cellular_material_identifiers[index];
        if material_form_from_identifier(material) == CELLULAR_DYNAMIC_MATERIAL_FORM &&
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

// Converts the coarse pressure mask into one indirect workgroup per active tile
@compute @workgroup_size(64)
fn compact_active_cellular_pressure_tiles(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if invocation.x == 0u {
        atomicStore(&pressure_indirect_dispatch[1], 1u);
        atomicStore(&pressure_indirect_dispatch[2], 1u);
    }
    let tile_count: u32 = parameters.buffered_tile_size.x * parameters.buffered_tile_size.y;
    let logical_tile_index: u32 = invocation.x;
    if logical_tile_index >= tile_count ||
            atomicLoad(&active_pressure_tiles[logical_tile_index]) == 0u { return; }
    let slot: u32 = atomicAdd(&pressure_indirect_dispatch[0], 1u);
    active_pressure_tile_indices[slot] = logical_tile_index;
}

// Queues editor impulse pressure without bypassing material transmission or mass response
@compute @workgroup_size(64)
fn queue_cellular_radial_impulse(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let logical_index: u32 = invocation.x;
    if logical_index >= parameters.buffered_cell_count { return; }
    let cell: vec2<i32> = cellular_pressure_world_cell_from_logical_index(logical_index);
    let index: u32 = cellular_pressure_physical_cell_index_from_world_cell(cell);
    if index == INVALID_PHYSICAL_CELL_INDEX ||
            cellular_material_identifiers[index] == EMPTY_MATERIAL_IDENTIFIER { return; }
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
fn copy_cellular_contact_velocity_snapshot(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    let logical_index: u32 = logical_cell_index_from_active_pressure_workgroup(workgroup.x, local_index);
    if logical_index >= parameters.buffered_cell_count { return; }
    let index: u32 = cellular_pressure_physical_cell_index_from_world_cell(
        cellular_pressure_world_cell_from_logical_index(logical_index),
    );
    if index == INVALID_PHYSICAL_CELL_INDEX { return; }
    pressure_b[index] = vec4<f32>(cellular_kinematics[index].xy, 0.0, 0.0);
}

// Resolves pairwise contact velocity and gathers its small pressure-transfer component
@compute @workgroup_size(64)
fn resolve_cellular_contacts(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    let logical_index: u32 = logical_cell_index_from_active_pressure_workgroup(workgroup.x, local_index);
    if logical_index >= parameters.buffered_cell_count { return; }
    let cell: vec2<i32> = cellular_pressure_world_cell_from_logical_index(logical_index);
    let index: u32 = cellular_pressure_physical_cell_index_from_world_cell(cell);
    if index == INVALID_PHYSICAL_CELL_INDEX { return; }
    let material: u32 = cellular_material_identifiers[index];
    if material_form_from_identifier(material) == CELLULAR_DYNAMIC_MATERIAL_FORM {
        let velocity: vec2<f32> = resolve_cellular_dynamic_contact_velocity(cell, index);
        cellular_kinematics[index].x = velocity.x;
        cellular_kinematics[index].y = velocity.y;
    }
    pending_pressure[index] += gather_cellular_contact_pressure(cell, index);
}

// Converts queued sources into the first directional pressure field
@compute @workgroup_size(64)
fn seed_cellular_pressure(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    let logical_index: u32 = logical_cell_index_from_active_pressure_workgroup(workgroup.x, local_index);
    if logical_index >= parameters.buffered_cell_count { return; }
    let index: u32 = cellular_pressure_physical_cell_index_from_world_cell(
        cellular_pressure_world_cell_from_logical_index(logical_index),
    );
    if index == INVALID_PHYSICAL_CELL_INDEX { return; }
    retained_pressure[index] = vec4<f32>(0.0);
    pressure_b[index] = vec4<f32>(0.0);
    var source: vec4<f32> = pending_pressure[index];
    pending_pressure[index] = vec4<f32>(0.0);
    // Divide one authoritative body impulse across its rasterized proxy cells
    if external_body_occupancy[index] == 1u || external_body_occupancy[index] == 2u {
        let occupied_count: u32 = max(1u, atomicLoad(&external_body_count[0]));
        source += encode_directional_pressure(
            external_body_velocity[index].zw / f32(occupied_count),
        );
    }
    pressure_a[index] = source;
}

// Alternating entry points preserve explicit pressure ping-pong ordering on the CPU
@compute @workgroup_size(64)
fn propagate_cellular_pressure_a(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    propagate_cellular_pressure(
        logical_cell_index_from_active_pressure_workgroup(workgroup.x, local_index), true,
    );
}

// Runs the second ping-pong direction of one pressure propagation step
@compute @workgroup_size(64)
fn propagate_cellular_pressure_b(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    propagate_cellular_pressure(
        logical_cell_index_from_active_pressure_workgroup(workgroup.x, local_index), false,
    );
}

// Applies all retained pressure after the fixed propagation budget has been consumed
@compute @workgroup_size(64)
fn apply_retained_cellular_pressure(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    let logical_index: u32 = logical_cell_index_from_active_pressure_workgroup(workgroup.x, local_index);
    if logical_index >= parameters.buffered_cell_count { return; }
    let cell: vec2<i32> = cellular_pressure_world_cell_from_logical_index(logical_index);
    let index: u32 = cellular_pressure_physical_cell_index_from_world_cell(cell);
    if index == INVALID_PHYSICAL_CELL_INDEX { return; }
    // Six propagation passes finish in A; the remaining in-flight pressure is retained here
    let load: vec4<f32> = retained_pressure[index] + pressure_a[index];
    let material: u32 = cellular_material_identifiers[index];
    let form: u32 = material_form_from_identifier(material);
    if form == CELLULAR_DYNAMIC_MATERIAL_FORM {
        let mass: f32 = cellular_dynamic_properties[material_index_from_identifier(material)].x;
        let impulse: vec2<f32> = vec2<f32>(
            load.x - load.y,
            load.z - load.w,
        ) / mass;
        cellular_kinematics[index].x += impulse.x;
        cellular_kinematics[index].y += impulse.y;
    } else if form == CELLULAR_STATIC_MATERIAL_FORM {
        apply_cellular_static_pressure_damage(cell, index, material, load);
    }
    pressure_a[index] = vec4<f32>(0.0);
    pressure_b[index] = vec4<f32>(0.0);
    retained_pressure[index] = vec4<f32>(0.0);
}

// Applies conservative normal response and friction using immutable tick-start velocities
fn resolve_cellular_dynamic_contact_velocity(cell: vec2<i32>, index: u32) -> vec2<f32> {
    let mass: f32 = cellular_contact_mass_at_physical_cell_index(index);
    let tick_start_velocity: vec2<f32> =
        cellular_contact_velocity_at_physical_cell_index(index);
    var velocity: vec2<f32> = tick_start_velocity;
    if external_body_occupancy[index] == 3u { return velocity; }
    for (var channel: u32 = 0u; channel < 4u; channel++) {
        let offset: vec2<i32> = world_cell_direction_from_pressure_channel(channel);
        let normal: vec2<f32> = vec2<f32>(offset);
        let neighbor: u32 = cellular_pressure_physical_cell_index_from_world_cell(cell + offset);
        if neighbor == INVALID_PHYSICAL_CELL_INDEX ||
                external_body_occupancy[neighbor] == 3u ||
                cellular_pressure_transmission_at_physical_cell_index(neighbor) <= 0.0 {
            continue;
        }
        let relative_velocity: vec2<f32> = tick_start_velocity -
            cellular_contact_velocity_at_physical_cell_index(neighbor);
        let approach_speed: f32 = max(0.0, dot(relative_velocity, normal));
        if approach_speed <= 0.0 { continue; }
        let neighbor_is_dynamic: bool =
            material_form_from_identifier(cellular_material_identifiers[neighbor]) ==
                CELLULAR_DYNAMIC_MATERIAL_FORM &&
            external_body_occupancy[neighbor] == 0u;
        let inverse_neighbor_mass: f32 = select(
            0.0,
            1.0 / cellular_contact_mass_at_physical_cell_index(neighbor),
            neighbor_is_dynamic,
        );
        let impulse: f32 =
            (1.0 + cellular_contact_restitution(index, neighbor)) * approach_speed /
            (1.0 / mass + inverse_neighbor_mass);
        velocity -= normal * (impulse / mass);
        let tangent_velocity: vec2<f32> =
            relative_velocity - normal * dot(relative_velocity, normal);
        let friction_factor: f32 = min(
            1.0,
            min(
                cellular_contact_friction_at_physical_cell_index(index),
                cellular_contact_friction_at_physical_cell_index(neighbor),
            ) *
                parameters.delta_time * CELLS_PER_TILE_FLOAT,
        );
        velocity -= tangent_velocity * friction_factor;
    }
    return velocity;
}

// Gathers dynamic impact pressure at its destination without float atomics or write conflicts
fn gather_cellular_contact_pressure(cell: vec2<i32>, index: u32) -> vec4<f32> {
    if external_body_occupancy[index] == 3u ||
            cellular_pressure_transmission_at_physical_cell_index(index) <= 0.0 {
        return vec4<f32>(0.0);
    }
    let destination_velocity: vec2<f32> =
        cellular_contact_velocity_at_physical_cell_index(index);
    var gathered: vec4<f32> = vec4<f32>(0.0);
    for (var channel: u32 = 0u; channel < 4u; channel++) {
        let offset: vec2<i32> = world_cell_direction_from_pressure_channel(channel);
        let neighbor: u32 = cellular_pressure_physical_cell_index_from_world_cell(cell + offset);
        if neighbor == INVALID_PHYSICAL_CELL_INDEX ||
                external_body_occupancy[neighbor] == 3u ||
                material_form_from_identifier(cellular_material_identifiers[neighbor]) !=
                    CELLULAR_DYNAMIC_MATERIAL_FORM {
            continue;
        }
        let toward_destination: vec2<f32> = -vec2<f32>(offset);
        let source_velocity: vec2<f32> =
            cellular_contact_velocity_at_physical_cell_index(neighbor);
        let approach_speed: f32 = max(
            0.0,
            dot(source_velocity - destination_velocity, toward_destination),
        );
        if approach_speed <= 0.0 { continue; }
        let source_mass: f32 = cellular_contact_mass_at_physical_cell_index(neighbor);
        let destination_is_dynamic: bool =
            material_form_from_identifier(cellular_material_identifiers[index]) ==
                CELLULAR_DYNAMIC_MATERIAL_FORM &&
            external_body_occupancy[index] == 0u;
        let inverse_destination_mass: f32 = select(
            0.0,
            1.0 / cellular_contact_mass_at_physical_cell_index(index),
            destination_is_dynamic,
        );
        let impulse: f32 =
            (1.0 + cellular_contact_restitution(neighbor, index)) * approach_speed /
            (1.0 / source_mass + inverse_destination_mass);
        gathered += encode_directional_pressure(
            toward_destination * impulse * CONTACT_PRESSURE_TRANSFER,
        );
    }
    return gathered;
}

// Damages or fractures one static cell using scalar compression from all four channels
fn apply_cellular_static_pressure_damage(
    cell: vec2<i32>,
    index: u32,
    material: u32,
    load: vec4<f32>,
) {
    let properties: StaticProperties =
        cellular_static_properties[material_index_from_identifier(material)];
    let compression: f32 = load.x + load.y + load.z + load.w;
    let overload: f32 = compression - properties.pressure_ignore_threshold;
    if overload <= 0.0 { return; }
    cellular_integrities[index] -= overload * parameters.delta_time * parameters.damage_rate;
    if cellular_integrities[index] > 0.0 { return; }
    if properties.debris != EMPTY_MATERIAL_IDENTIFIER &&
            cellular_fracture_yield_random_from_world_cell(cell, parameters.tick) <
                properties.debris_yield_rate {
        cellular_material_identifiers[index] = properties.debris;
        cellular_kinematics[index] = vec4<f32>(
            load.x - load.y,
            load.z - load.w,
            0.0,
            0.0,
        );
    } else {
        cellular_material_identifiers[index] = EMPTY_MATERIAL_IDENTIFIER;
        cellular_appearances[index] = EMPTY_MATERIAL_IDENTIFIER;
        cellular_kinematics[index] = vec4<f32>(0.0);
    }
    cellular_integrities[index] = 0.0;
}

// Retains pressure that the source material or blocked stencil routes cannot transmit
fn propagate_cellular_pressure(logical_index: u32, read_pressure_a: bool) {
    if logical_index >= parameters.buffered_cell_count { return; }
    let cell: vec2<i32> = cellular_pressure_world_cell_from_logical_index(logical_index);
    let index: u32 = cellular_pressure_physical_cell_index_from_world_cell(cell);
    if index == INVALID_PHYSICAL_CELL_INDEX { return; }
    let current: vec4<f32> = select(pressure_b[index], pressure_a[index], read_pressure_a);
    var local_retained: vec4<f32> = vec4<f32>(0.0);
    for (var channel: u32 = 0u; channel < 4u; channel++) {
        local_retained[channel] = current[channel] -
            calculate_outgoing_cellular_pressure(cell, channel, current[channel]);
    }
    retained_pressure[index] += local_retained;
    var gathered: vec4<f32> = vec4<f32>(0.0);
    for (var channel: u32 = 0u; channel < 4u; channel++) {
        gathered[channel] = gather_incoming_cellular_pressure(
            cell, channel, read_pressure_a,
        );
    }
    if read_pressure_a {
        pressure_b[index] = gathered;
    } else {
        pressure_a[index] = gathered;
    }
}

// Calculates the fraction that actually leaves one source through its fixed stencil
fn calculate_outgoing_cellular_pressure(cell: vec2<i32>, channel: u32, value: f32) -> f32 {
    let source_transmission: f32 = cellular_pressure_transmission_at_physical_cell_index(
        cellular_pressure_physical_cell_index_from_world_cell(cell),
    );
    if source_transmission <= 0.0 { return 0.0; }
    var transmitted: f32 = 0.0;
    for (var side: i32 = -1; side <= 1; side++) {
        let destination_cell: vec2<i32> = cell + world_cell_direction_from_pressure_channel(channel) +
            world_cell_side_offset_from_pressure_channel(channel, side);
        let destination: u32 = cellular_pressure_physical_cell_index_from_world_cell(destination_cell);
        if destination != INVALID_PHYSICAL_CELL_INDEX &&
                cellular_pressure_transmission_at_physical_cell_index(destination) > 0.0 {
            let weight: f32 = select(0.2, 0.6, side == 0);
            transmitted += value * source_transmission * weight;
        }
    }
    return transmitted;
}

// Gathers only valid source-routed pressure into one destination cell
fn gather_incoming_cellular_pressure(
    cell: vec2<i32>,
    channel: u32,
    read_pressure_a: bool,
) -> f32 {
    if cellular_pressure_transmission_at_physical_cell_index(
            cellular_pressure_physical_cell_index_from_world_cell(cell)) <= 0.0 { return 0.0; }
    var gathered: f32 = 0.0;
    for (var side: i32 = -1; side <= 1; side++) {
        let source_cell: vec2<i32> = cell - world_cell_direction_from_pressure_channel(channel) -
            world_cell_side_offset_from_pressure_channel(channel, side);
        let source: u32 = cellular_pressure_physical_cell_index_from_world_cell(source_cell);
        if source == INVALID_PHYSICAL_CELL_INDEX { continue; }
        let source_transmission: f32 =
            cellular_pressure_transmission_at_physical_cell_index(source);
        if source_transmission <= 0.0 { continue; }
        let source_pressure: f32 = select(
            pressure_b[source][channel],
            pressure_a[source][channel],
            read_pressure_a,
        );
        let weight: f32 = select(0.2, 0.6, side == 0);
        gathered += source_pressure * source_transmission * weight;
    }
    return gathered;
}

// Returns source-material transmission, with transient body cells effectively perfect
fn cellular_pressure_transmission_at_physical_cell_index(index: u32) -> f32 {
    if index == INVALID_PHYSICAL_CELL_INDEX { return 0.0; }
    if external_body_occupancy[index] != 0u { return 1.0; }
    let material: u32 = cellular_material_identifiers[index];
    let form: u32 = material_form_from_identifier(material);
    if form == CELLULAR_STATIC_MATERIAL_FORM {
        return cellular_static_properties[
            material_index_from_identifier(material)
        ].pressure_transmission;
    }
    if form == CELLULAR_DYNAMIC_MATERIAL_FORM {
        return cellular_dynamic_properties[material_index_from_identifier(material)].y;
    }
    return 0.0;
}

// Returns the friction coefficient for one cellular contact medium
fn cellular_contact_friction_at_physical_cell_index(index: u32) -> f32 {
    if index == INVALID_PHYSICAL_CELL_INDEX || external_body_occupancy[index] != 0u { return 0.0; }
    let material: u32 = cellular_material_identifiers[index];
    let form: u32 = material_form_from_identifier(material);
    if form == CELLULAR_STATIC_MATERIAL_FORM {
        return cellular_static_properties[material_index_from_identifier(material)].friction;
    }
    if form == CELLULAR_DYNAMIC_MATERIAL_FORM {
        return cellular_dynamic_properties[material_index_from_identifier(material)].z;
    }
    return 0.0;
}

// Returns the restitution coefficient for one cellular contact medium
fn cellular_material_restitution_at_physical_cell_index(index: u32) -> f32 {
    if index == INVALID_PHYSICAL_CELL_INDEX || external_body_occupancy[index] != 0u { return 0.0; }
    let material: u32 = cellular_material_identifiers[index];
    let form: u32 = material_form_from_identifier(material);
    if form == CELLULAR_STATIC_MATERIAL_FORM {
        return cellular_static_properties[material_index_from_identifier(material)].restitution;
    }
    if form == CELLULAR_DYNAMIC_MATERIAL_FORM {
        return cellular_dynamic_properties[material_index_from_identifier(material)].w;
    }
    return 0.0;
}

// An immovable body uses the cellular material's restitution rather than suppressing bounce
fn cellular_contact_restitution(first_index: u32, second_index: u32) -> f32 {
    if external_body_occupancy[first_index] != 0u {
        return cellular_material_restitution_at_physical_cell_index(second_index);
    }
    if external_body_occupancy[second_index] != 0u {
        return cellular_material_restitution_at_physical_cell_index(first_index);
    }
    return min(
        cellular_material_restitution_at_physical_cell_index(first_index),
        cellular_material_restitution_at_physical_cell_index(second_index),
    );
}

// Returns dynamic mass or the immovable mass used for static and proxy cells
fn cellular_contact_mass_at_physical_cell_index(index: u32) -> f32 {
    if index != INVALID_PHYSICAL_CELL_INDEX &&
            material_form_from_identifier(cellular_material_identifiers[index]) ==
                CELLULAR_DYNAMIC_MATERIAL_FORM &&
            external_body_occupancy[index] == 0u {
        return cellular_dynamic_properties[
            material_index_from_identifier(cellular_material_identifiers[index])
        ].x;
    }
    return IMMOVABLE_CONTACT_MASS;
}

// Reads the immutable contact velocity snapshot for a cellular or proxy cell
fn cellular_contact_velocity_at_physical_cell_index(index: u32) -> vec2<f32> {
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
fn world_cell_direction_from_pressure_channel(channel: u32) -> vec2<i32> {
    if channel == 0u { return vec2<i32>(1, 0); }
    if channel == 1u { return vec2<i32>(-1, 0); }
    if channel == 2u { return vec2<i32>(0, 1); }
    return vec2<i32>(0, -1);
}

// Returns the lateral stencil offset for one directional pressure channel
fn world_cell_side_offset_from_pressure_channel(channel: u32, side: i32) -> vec2<i32> {
    if channel < 2u { return vec2<i32>(0, side); }
    return vec2<i32>(side, 0);
}

// Converts logical tile-major dispatch order into a signed world cell
fn cellular_pressure_world_cell_from_logical_index(logical_index: u32) -> vec2<i32> {
    return world_cell_from_logical_tile_major_index(
        logical_index, parameters.buffered_origin, parameters.buffered_tile_size,
    );
}

// Maps one signed world cell through the two-dimensional physical tile ring
fn cellular_pressure_physical_cell_index_from_world_cell(world_cell: vec2<i32>) -> u32 {
    return physical_cell_index_from_world_cell(
        world_cell, parameters.buffered_origin, parameters.buffered_tile_size,
        parameters.ring_offset,
    );
}

// Maps one compacted active tile workgroup to its tile-major logical cell
fn logical_cell_index_from_active_pressure_workgroup(workgroup: u32, local_index: u32) -> u32 {
    return active_pressure_tile_indices[workgroup] * CELL_COUNT_PER_TILE + local_index;
}

// Produces a deterministic per-cell random value for fracture yield decisions
fn cellular_fracture_yield_random_from_world_cell(cell: vec2<i32>, tick: u32) -> f32 {
    var value: u32 = u32(cell.x) * 0x9e3779b9u ^
        u32(cell.y) * 0x85ebca6bu ^ tick;
    value ^= value >> 16u;
    return f32(value) / 4294967295.0;
}
