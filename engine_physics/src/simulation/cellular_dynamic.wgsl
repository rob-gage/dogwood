// Copyright Rob Gage 2026

struct Parameters {
    buffered_origin: vec2<i32>,
    buffered_tile_size: vec2<u32>,
    ring_offset: vec2<u32>,
    gravity: vec2<f32>,
    delta_time: f32,
    epoch: u32,
    buffered_cell_count: u32,
    maximum_movement_cells: u32,
    padding: vec4<u32>,
}

@group(0) @binding(0) var<storage, read_write> cellular_material_identifiers: array<u32>;
@group(0) @binding(1) var<storage, read_write> cellular_appearances: array<u32>;
@group(0) @binding(2) var<storage, read_write> cellular_kinematics: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read_write> processed_epochs: array<u32>;
@group(0) @binding(4) var<storage, read_write> active_tiles: array<atomic<u32>>;
@group(0) @binding(5) var<storage, read> external_body_occupancy: array<u32>;
@group(0) @binding(6) var<uniform> parameters: Parameters;
@group(0) @binding(7) var<storage, read_write> active_tile_indices: array<u32>;
@group(1) @binding(0) var<storage, read_write> indirect_dispatch: array<atomic<u32>>;

var<workgroup> neighborhood_materials: array<u32, 256>;
var<workgroup> neighborhood_external_occupancy: array<u32, 256>;
var<workgroup> neighborhood_claims: array<atomic<u32>, 256>;
var<workgroup> workgroup_moved: atomic<u32>;
var<workgroup> workgroup_has_potential_motion: atomic<u32>;

const EMPTY: u32 = 0u;
const CELLULAR_DYNAMIC_FORM: u32 = 2u;
const INVALID_INDEX: u32 = 0xffffffffu;
const CELLS_PER_TILE: i32 = 8;
const NEIGHBORHOOD_SIZE: i32 = 16;
const HALO_SIZE: i32 = 4;
const ACTIVE_COUNTDOWN: u32 = 8u;
const WAKE_NEIGHBORS: u32 = 0x80000000u;

// Produces four dense parity lists and their indirect workgroup counts.
@compute @workgroup_size(64)
fn compact_active_cellular_dynamic_tiles(
    @builtin(global_invocation_id) invocation: vec3<u32>,
) {
    let tile_count: u32 = parameters.buffered_cell_count / 64u;
    if invocation.x == 0u {
        for (var group: u32 = 0u; group < 4u; group++) {
            atomicStore(&indirect_dispatch[group * 3u + 1u], 1u);
            atomicStore(&indirect_dispatch[group * 3u + 2u], 1u);
        }
    }
    let logical_tile_index: u32 = invocation.x;
    if logical_tile_index >= tile_count { return; }
    let logical_tile: vec2<u32> = vec2<u32>(
        logical_tile_index % parameters.buffered_tile_size.x,
        logical_tile_index / parameters.buffered_tile_size.x,
    );
    let physical_tile: u32 = physical_tile_index(logical_tile);
    if atomicLoad(&active_tiles[physical_tile]) == 0u { return; }
    let world_tile: vec2<i32> = parameters.buffered_origin + vec2<i32>(logical_tile);
    let group: u32 = (u32(world_tile.x) & 1u) | ((u32(world_tile.y) & 1u) << 1u);
    let slot: u32 = atomicAdd(&indirect_dispatch[group * 3u], 1u);
    active_tile_indices[group * tile_count + slot] = logical_tile_index;
}

@compute @workgroup_size(64)
fn move_cellular_dynamic_group_a(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    move_cellular_dynamic_tile(workgroup.x, local_index, 0u);
}

@compute @workgroup_size(64)
fn move_cellular_dynamic_group_b(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    move_cellular_dynamic_tile(workgroup.x, local_index, 1u);
}

@compute @workgroup_size(64)
fn move_cellular_dynamic_group_c(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    move_cellular_dynamic_tile(workgroup.x, local_index, 2u);
}

@compute @workgroup_size(64)
fn move_cellular_dynamic_group_d(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    move_cellular_dynamic_tile(workgroup.x, local_index, 3u);
}

// One parity group owns disjoint [tile * 8 - 4, tile * 8 + 11] neighborhoods.
fn move_cellular_dynamic_tile(
    active_tile_index: u32,
    local_index: u32,
    group: u32,
) {
    let tile_count: u32 = parameters.buffered_cell_count / 64u;
    let logical_tile_index: u32 = active_tile_indices[
        group * tile_count + active_tile_index
    ];
    let logical_tile: vec2<u32> = vec2<u32>(
        logical_tile_index % parameters.buffered_tile_size.x,
        logical_tile_index / parameters.buffered_tile_size.x,
    );
    let physical_tile: u32 = physical_tile_index(logical_tile);
    let world_tile: vec2<i32> = parameters.buffered_origin + vec2<i32>(logical_tile);
    let neighborhood_origin: vec2<i32> = world_tile * CELLS_PER_TILE -
        vec2<i32>(HALO_SIZE);
    if local_index == 0u {
        atomicStore(&workgroup_moved, 0u);
        atomicStore(&workgroup_has_potential_motion, 0u);
    }
    for (var load: u32 = local_index; load < 256u; load += 64u) {
        let neighborhood_cell: vec2<i32> = vec2<i32>(
            i32(load % 16u),
            i32(load / 16u),
        );
        let index: u32 = physical_cell_index(neighborhood_origin + neighborhood_cell);
        if index == INVALID_INDEX {
            neighborhood_materials[load] = INVALID_INDEX;
            neighborhood_external_occupancy[load] = 1u;
        } else {
            neighborhood_materials[load] = cellular_material_identifiers[index];
            neighborhood_external_occupancy[load] = external_body_occupancy[index];
        }
        atomicStore(&neighborhood_claims[load], INVALID_INDEX);
    }
    workgroupBarrier();

    let local_cell: vec2<i32> = vec2<i32>(
        i32(local_index % 8u),
        i32(local_index / 8u),
    );
    let source_cell: vec2<i32> = world_tile * CELLS_PER_TILE + local_cell;
    let source_index: u32 = physical_cell_index(source_cell);
    let source_neighborhood: vec2<i32> = local_cell + vec2<i32>(HALO_SIZE);
    let source_neighborhood_index: u32 = neighborhood_index(source_neighborhood);
    let material: u32 = neighborhood_materials[source_neighborhood_index];
    var destination_neighborhood: vec2<i32> = source_neighborhood;
    var updated_kinematics: vec4<f32> = cellular_kinematics[source_index];
    var claim_ticket: u32 = INVALID_INDEX;
    var processes_dynamic: bool = false;
    var proposes_movement: bool = false;

    if material >> 30u == CELLULAR_DYNAMIC_FORM &&
            processed_epochs[source_index] != parameters.epoch {
        processes_dynamic = true;
        var velocity: vec2<f32> = updated_kinematics.xy +
            parameters.gravity * parameters.delta_time;
        let maximum_velocity: f32 = f32(parameters.maximum_movement_cells) /
            (f32(CELLS_PER_TILE) * parameters.delta_time);
        let velocity_magnitude: f32 = length(velocity);
        if velocity_magnitude > maximum_velocity {
            velocity *= maximum_velocity / velocity_magnitude;
        }
        var accumulated: vec2<f32> = updated_kinematics.zw +
            velocity * f32(CELLS_PER_TILE) * parameters.delta_time;
        let accumulated_distance: f32 = length(accumulated);
        if accumulated_distance > f32(parameters.maximum_movement_cells) {
            accumulated *= f32(parameters.maximum_movement_cells) / accumulated_distance;
        }
        let displacement: vec2<i32> = vec2<i32>(accumulated);
        var residual: vec2<f32> = accumulated - vec2<f32>(displacement);
        var direct_blocked: bool = false;
        if any(displacement != vec2<i32>(0)) {
            let absolute: vec2<i32> = abs(displacement);
            let direction: vec2<i32> = sign(displacement);
            var error: i32 = absolute.x - absolute.y;
            var current: vec2<i32> = source_neighborhood;
            for (var step: u32 = 0u; step < parameters.maximum_movement_cells; step++) {
                if all(current == source_neighborhood + displacement) { break; }
                let doubled_error: i32 = error * 2;
                var next: vec2<i32> = current;
                if doubled_error > -absolute.y {
                    error -= absolute.y;
                    next.x += direction.x;
                }
                if doubled_error < absolute.x {
                    error += absolute.x;
                    next.y += direction.y;
                }
                if neighborhood_is_occupied(next) {
                    direct_blocked = true;
                    break;
                }
                destination_neighborhood = next;
                current = next;
            }
        }
        if direct_blocked && all(destination_neighborhood == source_neighborhood) {
            destination_neighborhood = choose_slide_destination(
                source_neighborhood,
                source_cell,
            );
            if all(destination_neighborhood == source_neighborhood) {
                let gravity_length: f32 = length(parameters.gravity);
                if gravity_length > 0.0 {
                    let gravity_direction: vec2<f32> = parameters.gravity / gravity_length;
                    let into_support: f32 = dot(velocity, gravity_direction);
                    if into_support > 0.0 {
                        velocity -= gravity_direction * into_support;
                    }
                    let residual_into_support: f32 = dot(residual, gravity_direction);
                    if residual_into_support > 0.0 {
                        residual -= gravity_direction * residual_into_support;
                    }
                }
            }
        }
        if all(destination_neighborhood == source_neighborhood) &&
                cell_is_stably_supported(source_neighborhood) {
            let gravity_direction: vec2<f32> = normalize(parameters.gravity);
            let into_support: f32 = dot(velocity, gravity_direction);
            if into_support > 0.0 {
                velocity -= gravity_direction * into_support;
            }
            let residual_into_support: f32 = dot(residual, gravity_direction);
            if residual_into_support > 0.0 {
                residual -= gravity_direction * residual_into_support;
            }
        }
        if neighborhood_external_occupancy[source_neighborhood_index] != 0u &&
                all(destination_neighborhood == source_neighborhood) {
            destination_neighborhood = choose_body_exit_destination(
                source_neighborhood,
                source_cell,
            );
        }
        updated_kinematics = vec4<f32>(velocity, residual);
        proposes_movement = any(destination_neighborhood != source_neighborhood);
        if proposes_movement {
            claim_ticket = (hash_cell(source_cell, parameters.epoch) & 0xffffffc0u) |
                local_index;
            atomicMin(
                &neighborhood_claims[neighborhood_index(destination_neighborhood)],
                claim_ticket,
            );
            atomicStore(&workgroup_has_potential_motion, 1u);
        } else if cell_has_potential_motion(
                source_neighborhood,
                velocity,
                residual,
            ) {
            atomicStore(&workgroup_has_potential_motion, 1u);
        }
    }
    workgroupBarrier();

    if processes_dynamic {
        let wins: bool = proposes_movement && atomicLoad(
            &neighborhood_claims[neighborhood_index(destination_neighborhood)]
        ) == claim_ticket;
        if wins {
            let destination_cell: vec2<i32> = neighborhood_origin + destination_neighborhood;
            let destination_index: u32 = physical_cell_index(destination_cell);
            cellular_material_identifiers[destination_index] = material;
            cellular_appearances[destination_index] = cellular_appearances[source_index];
            cellular_kinematics[destination_index] = updated_kinematics;
            processed_epochs[destination_index] = parameters.epoch;
            cellular_material_identifiers[source_index] = EMPTY;
            cellular_appearances[source_index] = EMPTY;
            cellular_kinematics[source_index] = vec4<f32>(0.0);
            processed_epochs[source_index] = 0u;
            atomicStore(&workgroup_moved, 1u);
        } else {
            cellular_kinematics[source_index] = updated_kinematics;
            processed_epochs[source_index] = parameters.epoch;
        }
    }
    workgroupBarrier();

    if local_index == 0u {
        let moved: bool = atomicLoad(&workgroup_moved) != 0u;
        let has_potential_motion: bool =
            atomicLoad(&workgroup_has_potential_motion) != 0u;
        let wake_neighbors: bool =
            (atomicLoad(&active_tiles[physical_tile]) & WAKE_NEIGHBORS) != 0u;
        if moved || wake_neighbors {
            wake_tile_neighborhood(logical_tile);
            atomicStore(&active_tiles[physical_tile], ACTIVE_COUNTDOWN);
        } else if has_potential_motion {
            atomicStore(&active_tiles[physical_tile], ACTIVE_COUNTDOWN);
        } else {
            let activity: u32 = atomicLoad(&active_tiles[physical_tile]);
            atomicStore(&active_tiles[physical_tile], activity - min(activity, 1u));
        }
    }
}

fn choose_slide_destination(source: vec2<i32>, world_cell: vec2<i32>) -> vec2<i32> {
    if all(parameters.gravity == vec2<f32>(0.0)) { return source; }
    let tangent: vec2<f32> = vec2<f32>(-parameters.gravity.y, parameters.gravity.x);
    let first_sign: f32 = select(
        -1.0,
        1.0,
        (hash_cell(world_cell, parameters.epoch) & 1u) == 0u,
    );
    for (var side: u32 = 0u; side < 2u; side++) {
        let tangent_sign: f32 = select(first_sign, -first_sign, side == 1u);
        let direction: vec2<i32> = vec2<i32>(sign(
            parameters.gravity + tangent * tangent_sign
        ));
        let destination: vec2<i32> = source + direction;
        if any(direction != vec2<i32>(0)) && !neighborhood_is_occupied(destination) {
            return destination;
        }
    }
    return source;
}

fn choose_body_exit_destination(source: vec2<i32>, world_cell: vec2<i32>) -> vec2<i32> {
    let first: u32 = hash_cell(world_cell, parameters.epoch) & 3u;
    for (var offset: u32 = 0u; offset < 4u; offset++) {
        let direction: u32 = (first + offset) & 3u;
        let delta: vec2<i32> = select(
            select(vec2<i32>(1, 0), vec2<i32>(-1, 0), direction == 1u),
            select(vec2<i32>(0, 1), vec2<i32>(0, -1), direction == 3u),
            direction >= 2u,
        );
        if !neighborhood_is_occupied(source + delta) { return source + delta; }
    }
    return source;
}

fn cell_has_potential_motion(
    source: vec2<i32>,
    velocity: vec2<f32>,
    residual: vec2<f32>,
) -> bool {
    if length(velocity) < 0.0001 { return false; }
    var motion: vec2<f32> = residual +
        velocity * f32(CELLS_PER_TILE) * parameters.delta_time;
    if length(motion) < 0.0001 { motion = velocity; }
    let direct: vec2<i32> = vec2<i32>(sign(motion));
    if any(direct != vec2<i32>(0)) && !neighborhood_is_occupied(source + direct) {
        return true;
    }
    if all(parameters.gravity == vec2<f32>(0.0)) { return false; }
    let tangent: vec2<f32> = vec2<f32>(-parameters.gravity.y, parameters.gravity.x);
    for (var side: i32 = -1; side <= 1; side += 2) {
        let slide: vec2<i32> = vec2<i32>(sign(parameters.gravity + tangent * f32(side)));
        if any(slide != vec2<i32>(0)) && !neighborhood_is_occupied(source + slide) {
            return true;
        }
    }
    return false;
}

// True rest requires direct support and no gravity-relative escape on either side.
fn cell_is_stably_supported(source: vec2<i32>) -> bool {
    if all(parameters.gravity == vec2<f32>(0.0)) { return false; }
    let gravity_step: vec2<i32> = vec2<i32>(round(normalize(parameters.gravity)));
    if !neighborhood_is_occupied(source + gravity_step) { return false; }
    let tangent: vec2<f32> = vec2<f32>(-parameters.gravity.y, parameters.gravity.x);
    for (var side: i32 = -1; side <= 1; side += 2) {
        let slide: vec2<i32> = vec2<i32>(sign(parameters.gravity + tangent * f32(side)));
        if any(slide != vec2<i32>(0)) && !neighborhood_is_occupied(source + slide) {
            return false;
        }
    }
    return true;
}

fn neighborhood_is_occupied(cell: vec2<i32>) -> bool {
    if any(cell < vec2<i32>(0)) || any(cell >= vec2<i32>(NEIGHBORHOOD_SIZE)) {
        return true;
    }
    let index: u32 = neighborhood_index(cell);
    return neighborhood_materials[index] != EMPTY ||
        neighborhood_external_occupancy[index] != 0u;
}

fn neighborhood_index(cell: vec2<i32>) -> u32 {
    return u32(cell.y * NEIGHBORHOOD_SIZE + cell.x);
}

fn wake_tile_neighborhood(center: vec2<u32>) {
    for (var y: i32 = -1; y <= 1; y++) {
        for (var x: i32 = -1; x <= 1; x++) {
            let tile: vec2<i32> = vec2<i32>(center) + vec2<i32>(x, y);
            if any(tile < vec2<i32>(0)) ||
                    tile.x >= i32(parameters.buffered_tile_size.x) ||
                    tile.y >= i32(parameters.buffered_tile_size.y) {
                continue;
            }
            atomicMax(&active_tiles[physical_tile_index(vec2<u32>(tile))], ACTIVE_COUNTDOWN);
        }
    }
}

fn physical_tile_index(logical_tile: vec2<u32>) -> u32 {
    let physical: vec2<u32> =
        (logical_tile + parameters.ring_offset) % parameters.buffered_tile_size;
    return physical.y * parameters.buffered_tile_size.x + physical.x;
}

fn physical_cell_index(cell: vec2<i32>) -> u32 {
    let tile: vec2<i32> = vec2<i32>(
        floor_divide(cell.x, CELLS_PER_TILE),
        floor_divide(cell.y, CELLS_PER_TILE),
    );
    let relative: vec2<i32> = tile - parameters.buffered_origin;
    if any(relative < vec2<i32>(0)) ||
            relative.x >= i32(parameters.buffered_tile_size.x) ||
            relative.y >= i32(parameters.buffered_tile_size.y) {
        return INVALID_INDEX;
    }
    let physical: vec2<u32> =
        (vec2<u32>(relative) + parameters.ring_offset) % parameters.buffered_tile_size;
    let local: vec2<u32> = vec2<u32>(cell - tile * CELLS_PER_TILE);
    return (physical.y * parameters.buffered_tile_size.x + physical.x) * 64u +
        local.y * 8u + local.x;
}

fn floor_divide(value: i32, divisor: i32) -> i32 {
    if value < 0 { return (value - divisor + 1) / divisor; }
    return value / divisor;
}

fn hash_cell(cell: vec2<i32>, epoch: u32) -> u32 {
    var value: u32 = u32(cell.x) * 0x9e3779b9u ^
        u32(cell.y) * 0x85ebca6bu ^ epoch * 0xc2b2ae35u;
    value ^= value >> 16u;
    value *= 0x7feb352du;
    value ^= value >> 15u;
    return value;
}
