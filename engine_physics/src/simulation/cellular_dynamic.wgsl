// Copyright Rob Gage 2026

struct Parameters {
    buffered_origin: vec2<i32>,
    buffered_tile_size: vec2<u32>,
    active_origin: vec2<i32>,
    active_tile_size: vec2<u32>,
    ring_offset: vec2<u32>,
    gravity: vec2<f32>,
    delta_time: f32,
    tick: u32,
    buffered_cell_count: u32,
    maximum_movement_cells: u32,
    padding: array<vec4<u32>, 2>,
}

struct Proposal {
    destination: u32,
    padding_0: u32,
    padding_1: u32,
    padding_2: u32,
    kinematics: vec4<f32>,
}

@group(0) @binding(0) var<storage, read> cellular_material_identifiers: array<u32>;
@group(0) @binding(1) var<storage, read> cellular_appearances: array<u32>;
@group(0) @binding(2) var<storage, read_write> cellular_kinematics: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read_write> material_identifiers_output: array<u32>;
@group(0) @binding(4) var<storage, read_write> appearances_output: array<u32>;
@group(0) @binding(5) var<storage, read_write> destination_claims: array<atomic<u32>>;
@group(0) @binding(6) var<storage, read_write> proposals: array<Proposal>;
@group(0) @binding(7) var<uniform> parameters: Parameters;
@group(0) @binding(8) var<storage, read> external_body_occupancy: array<u32>;

const INVALID_INDEX: u32 = 0xffffffffu;
const CELLS_PER_TILE: i32 = 8;
const CELLULAR_DYNAMIC_FORM: u32 = 2u;

// Reset transient atomic contention state
@compute @workgroup_size(64)
fn clear_cellular_dynamic_destination_claims(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let index: u32 = invocation.x;
    if index >= parameters.buffered_cell_count { return; }
    atomicStore(&destination_claims[index], INVALID_INDEX);
}

// Calculate immutable-input movement proposals and destination claims
@compute @workgroup_size(64)
fn calculate_cellular_dynamic_movement_proposals(@builtin(global_invocation_id) invocation: vec3<u32>) {
    // map the logical invocation through the physical tile ring
    let logical_index: u32 = invocation.x;
    if logical_index >= parameters.buffered_cell_count { return; }
    let source_cell: vec2<i32> = cellular_dynamic_world_cell_from_logical_index(logical_index);
    let source_index: u32 = cellular_dynamic_physical_cell_index(source_cell);
    let material_identifier: u32 = cellular_material_identifiers[source_index];
    if material_identifier >> 30u != CELLULAR_DYNAMIC_FORM { return; }

    // integrate arbitrary gravity and enforce the per-tick transport limit
    var velocity: vec2<f32> = cellular_kinematics[source_index].xy +
        parameters.gravity * parameters.delta_time;
    let maximum_velocity: f32 = f32(parameters.maximum_movement_cells) /
        (f32(CELLS_PER_TILE) * parameters.delta_time);
    let velocity_magnitude: f32 = length(velocity);
    if velocity_magnitude > maximum_velocity {
        velocity *= maximum_velocity / velocity_magnitude;
    }
    var accumulated: vec2<f32> = cellular_kinematics[source_index].zw +
        velocity * f32(CELLS_PER_TILE) * parameters.delta_time;
    let accumulated_distance: f32 = length(accumulated);
    if accumulated_distance > f32(parameters.maximum_movement_cells) {
        accumulated *= f32(parameters.maximum_movement_cells) / accumulated_distance;
    }
    let source_inside_body = external_body_occupancy[source_index] != 0u;
    // separate whole-cell displacement from retained subcell residual
    let displacement: vec2<i32> = vec2<i32>(accumulated);
    var residual: vec2<f32> = accumulated - vec2<f32>(displacement);
    var destination_cell: vec2<i32> = source_cell;
    if any(displacement != vec2<i32>(0)) {
        // trace crossed cells and retain the farthest immutable-empty destination
        let absolute: vec2<i32> = abs(displacement);
        let direction: vec2<i32> = sign(displacement);
        var error: i32 = absolute.x - absolute.y;
        var current: vec2<i32> = source_cell;
        var direct_blocked: bool = false;
        for (var step: u32 = 0u; step < parameters.maximum_movement_cells; step++) {
            if all(current == source_cell + displacement) { break; }
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
            let next_index: u32 = cellular_dynamic_physical_cell_index(next);
            if next_index == INVALID_INDEX || cellular_material_identifiers[next_index] != 0u ||
                    external_body_occupancy[next_index] != 0u {
                direct_blocked = true;
                break;
            }
            destination_cell = next;
            current = next;
        }
        // try minimal gravity-relative sliding only when the first step is blocked
        if direct_blocked && all(destination_cell == source_cell) {
            destination_cell = choose_cellular_dynamic_slide_destination(source_cell);
            if all(destination_cell == source_cell) {
                let gravity_length: f32 = length(parameters.gravity);
                if gravity_length > 0.0 {
                    let gravity_direction: vec2<f32> = parameters.gravity / gravity_length;
                    let into_support: f32 = dot(velocity, gravity_direction);
                    if into_support > 0.0 {
                        velocity -= gravity_direction * into_support;
                    }
                }
                residual = clamp(accumulated, vec2<f32>(-0.999), vec2<f32>(0.999));
            }
        }
    }
    if source_inside_body && all(destination_cell == source_cell) {
        destination_cell = choose_external_body_exit_destination(source_cell);
    }
    // publish integrated matter state before atomically competing for its destination
    let destination_index: u32 = cellular_dynamic_physical_cell_index(destination_cell);
    proposals[source_index] = Proposal(
        destination_index,
        0u,
        0u,
        0u,
        vec4<f32>(velocity, residual),
    );
    if destination_index != source_index {
        atomicMin(
            &destination_claims[destination_index],
            cellular_dynamic_destination_claim_ticket(source_index, destination_cell),
        );
    }
}

// Gather winners into complete resolved output state
@compute @workgroup_size(64)
fn resolve_cellular_dynamic_movement_proposals(@builtin(global_invocation_id) invocation: vec3<u32>) {
    // map the logical output cell through the physical tile ring
    let logical_index: u32 = invocation.x;
    if logical_index >= parameters.buffered_cell_count { return; }
    let cell: vec2<i32> = cellular_dynamic_world_cell_from_logical_index(logical_index);
    let index: u32 = cellular_dynamic_physical_cell_index(cell);
    let claim: u32 = atomicLoad(&destination_claims[index]);
    // copy every matter-attached field from an incoming winning source
    if claim != INVALID_INDEX {
        let source_index: u32 = cellular_dynamic_source_index_from_claim_ticket(claim, cell);
        material_identifiers_output[index] = cellular_material_identifiers[source_index];
        appearances_output[index] = cellular_appearances[source_index];
        cellular_kinematics[index] = proposals[source_index].kinematics;
        return;
    }
    // clear accepted sources while preserving losing or stationary dynamic cells
    let material_identifier: u32 = cellular_material_identifiers[index];
    if material_identifier >> 30u == CELLULAR_DYNAMIC_FORM {
        let proposal: Proposal = proposals[index];
        if proposal.destination != index &&
                atomicLoad(&destination_claims[proposal.destination]) ==
                    cellular_dynamic_destination_claim_ticket(index, cellular_dynamic_world_cell_from_physical_index(proposal.destination)) {
            material_identifiers_output[index] = 0u;
            appearances_output[index] = 0u;
            cellular_kinematics[index] = vec4<f32>(0.0);
            return;
        }
        material_identifiers_output[index] = material_identifier;
        appearances_output[index] = cellular_appearances[index];
        cellular_kinematics[index] = proposal.kinematics;
        return;
    }
    // preserve static, empty, and inactive-buffered cells unchanged
    material_identifiers_output[index] = material_identifier;
    appearances_output[index] = cellular_appearances[index];
}

// Choose one immutable-empty gravity-relative slide destination
fn choose_cellular_dynamic_slide_destination(source: vec2<i32>) -> vec2<i32> {
    if all(parameters.gravity == vec2<f32>(0.0)) { return source; }
    let tangent: vec2<f32> = vec2<f32>(-parameters.gravity.y, parameters.gravity.x);
    let first_sign: f32 = select(-1.0, 1.0, (hash_cellular_dynamic_claim_priority(source, parameters.tick) & 1u) == 0u);
    for (var side: u32 = 0u; side < 2u; side++) {
        let tangent_sign: f32 = select(first_sign, -first_sign, side == 1u);
        let direction_f: vec2<f32> = parameters.gravity + tangent * tangent_sign;
        let direction: vec2<i32> = vec2<i32>(sign(direction_f));
        let candidate: vec2<i32> = source + direction;
        let index: u32 = cellular_dynamic_physical_cell_index(candidate);
        if any(direction != vec2<i32>(0)) && index != INVALID_INDEX &&
                cellular_material_identifiers[index] == 0u && external_body_occupancy[index] == 0u {
            return candidate;
        }
    }
    return source;
}

fn choose_external_body_exit_destination(source: vec2<i32>) -> vec2<i32> {
    let first = hash_cellular_dynamic_claim_priority(source, parameters.tick) & 3u;
    for (var offset = 0u; offset < 4u; offset++) {
        let direction = (first + offset) & 3u;
        let delta = select(select(vec2<i32>(1, 0), vec2<i32>(-1, 0), direction == 1u),
            select(vec2<i32>(0, 1), vec2<i32>(0, -1), direction == 3u), direction >= 2u);
        let index = cellular_dynamic_physical_cell_index(source + delta);
        if index != INVALID_INDEX && cellular_material_identifiers[index] == 0u &&
                external_body_occupancy[index] == 0u { return source + delta; }
    }
    return source;
}

// Create a unique destination-specific rotating claim ticket
fn cellular_dynamic_destination_claim_ticket(source_index: u32, destination: vec2<i32>) -> u32 {
    return (source_index + cellular_dynamic_destination_claim_offset(destination)) % parameters.buffered_cell_count;
}

// Recover the winning physical source index from its reversible ticket
fn cellular_dynamic_source_index_from_claim_ticket(ticket: u32, destination: vec2<i32>) -> u32 {
    let offset: u32 = cellular_dynamic_destination_claim_offset(destination);
    if ticket >= offset { return ticket - offset; }
    return ticket + parameters.buffered_cell_count - offset;
}

// Derive the cyclic claim ordering for one destination and tick
fn cellular_dynamic_destination_claim_offset(destination: vec2<i32>) -> u32 {
    return hash_cellular_dynamic_claim_priority(destination, parameters.tick) % parameters.buffered_cell_count;
}

// Hash stable world coordinates and the simulation tick
fn hash_cellular_dynamic_claim_priority(cell: vec2<i32>, tick: u32) -> u32 {
    var value: u32 = u32(cell.x) * 0x9e3779b9u ^
        u32(cell.y) * 0x85ebca6bu ^ tick * 0xc2b2ae35u;
    value ^= value >> 16u;
    value *= 0x7feb352du;
    value ^= value >> 15u;
    return value;
}

// Map one world cell through the tile ring to tile-major physical storage
fn cellular_dynamic_physical_cell_index(cell: vec2<i32>) -> u32 {
    // split signed world coordinates into a tile and nonnegative local cell
    let tile: vec2<i32> = vec2<i32>(
        floor_divide_cell_coordinate(cell.x, CELLS_PER_TILE),
        floor_divide_cell_coordinate(cell.y, CELLS_PER_TILE),
    );
    let relative: vec2<i32> = tile - parameters.buffered_origin;
    if any(relative < vec2<i32>(0)) ||
            relative.x >= i32(parameters.buffered_tile_size.x) ||
            relative.y >= i32(parameters.buffered_tile_size.y) {
        return INVALID_INDEX;
    }

    // wrap the logical tile into its current physical ring slot
    let physical: vec2<u32> =
        (vec2<u32>(relative) + parameters.ring_offset) % parameters.buffered_tile_size;
    let local: vec2<u32> = vec2<u32>(cell - tile * CELLS_PER_TILE);
    return (physical.y * parameters.buffered_tile_size.x + physical.x) * 64u +
        local.y * 8u + local.x;
}

// Convert logical tile-major dispatch order into a signed world cell
fn cellular_dynamic_world_cell_from_logical_index(index: u32) -> vec2<i32> {
    let tile_index: u32 = index / 64u;
    let local_index: u32 = index % 64u;
    let logical_tile: vec2<u32> = vec2<u32>(
        tile_index % parameters.buffered_tile_size.x,
        tile_index / parameters.buffered_tile_size.x,
    );
    return (parameters.buffered_origin + vec2<i32>(logical_tile)) * CELLS_PER_TILE +
        vec2<i32>(i32(local_index % 8u), i32(local_index / 8u));
}

// Convert one physical ring index back into its signed world cell
fn cellular_dynamic_world_cell_from_physical_index(index: u32) -> vec2<i32> {
    let tile_index: u32 = index / 64u;
    let local_index: u32 = index % 64u;
    let physical_tile: vec2<u32> = vec2<u32>(
        tile_index % parameters.buffered_tile_size.x,
        tile_index / parameters.buffered_tile_size.x,
    );
    let logical_tile: vec2<u32> =
        (physical_tile + parameters.buffered_tile_size - parameters.ring_offset) %
            parameters.buffered_tile_size;
    return (parameters.buffered_origin + vec2<i32>(logical_tile)) * CELLS_PER_TILE +
        vec2<i32>(i32(local_index % 8u), i32(local_index / 8u));
}

// Divide signed coordinates toward negative infinity
fn floor_divide_cell_coordinate(value: i32, divisor: i32) -> i32 {
    if value < 0 { return (value - divisor + 1) / divisor; }
    return value / divisor;
}
