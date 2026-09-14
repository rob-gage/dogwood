// Copyright Rob Gage 2026

struct Parameters {
    buffered_origin: vec2<i32>,
    buffered_tile_size: vec2<u32>,
    ring_offset: vec2<u32>,
    gravity: vec2<f32>,
    delta_time: f32,
    buffered_cell_count: u32,
    gas_count: u32,
    streaming_cell_count: u32,
    streaming_origin: vec2<i32>,
    streaming_tile_size: vec2<u32>,
    vorticity_confinement: f32,
    buoyancy_coefficient: f32,
    maximum_speed: f32,
    fluid_obstacle_coverage: f32,
    ambient_density: f32,
    padding_0: u32,
    padding_1: vec2<u32>,
}

@group(0) @binding(0) var<storage, read_write> velocity: array<vec2<f32>>;
@group(0) @binding(1) var<storage, read_write> velocity_scratch: array<vec2<f32>>;
@group(0) @binding(2) var<storage, read_write> concentrations: array<f32>;
@group(0) @binding(3) var<storage, read_write> concentration_scratch: array<f32>;
@group(0) @binding(4) var<storage, read_write> divergence: array<f32>;
@group(0) @binding(5) var<storage, read_write> pressure_a: array<f32>;
@group(0) @binding(6) var<storage, read_write> pressure_b: array<f32>;
@group(0) @binding(7) var<storage, read_write> curl: array<f32>;
@group(0) @binding(8) var<storage, read> cellular_material_identifiers: array<u32>;
@group(0) @binding(9) var<storage, read> external_body_occupancy: array<u32>;
@group(0) @binding(10) var<storage, read> fluid_coverage: array<f32>;
@group(0) @binding(11) var<storage, read> gas_properties: array<vec4<f32>>;
@group(0) @binding(12) var<storage, read_write> streaming_data: array<u32>;
@group(0) @binding(13) var<uniform> parameters: Parameters;

const INVALID_INDEX: u32 = 0xffffffffu;
const CELLS_PER_TILE: f32 = 8.0;

// Semi-Lagrangian backtracing reads the immutable current field and writes separate scratch.
@compute @workgroup_size(64)
fn advect_gas_velocity(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let logical_index: u32 = invocation.x;
    if logical_index >= parameters.buffered_cell_count { return; }
    let cell: vec2<i32> = world_cell_from_logical_index(logical_index);
    let index: u32 = physical_cell_index(cell);
    if is_obstacle(cell) {
        velocity_scratch[index] = vec2<f32>(0.0);
        return;
    }
    let center: vec2<f32> = vec2<f32>(cell) + vec2<f32>(0.5);
    let backtraced: vec2<f32> = center - velocity[index] * parameters.delta_time;
    velocity_scratch[index] = sample_velocity(backtraced);
}

@compute @workgroup_size(64)
fn calculate_gas_curl(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let logical_index: u32 = invocation.x;
    if logical_index >= parameters.buffered_cell_count { return; }
    let cell: vec2<i32> = world_cell_from_logical_index(logical_index);
    let index: u32 = physical_cell_index(cell);
    if is_obstacle(cell) {
        curl[index] = 0.0;
        return;
    }
    let left: vec2<f32> = scratch_velocity_at(cell + vec2<i32>(-1, 0));
    let right: vec2<f32> = scratch_velocity_at(cell + vec2<i32>(1, 0));
    let bottom: vec2<f32> = scratch_velocity_at(cell + vec2<i32>(0, -1));
    let top: vec2<f32> = scratch_velocity_at(cell + vec2<i32>(0, 1));
    curl[index] = 0.5 * ((right.y - left.y) - (top.x - bottom.x));
}

// Density below the implicit ambient atmosphere reverses arbitrary scene gravity.
@compute @workgroup_size(64)
fn apply_gas_forces(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let logical_index: u32 = invocation.x;
    if logical_index >= parameters.buffered_cell_count { return; }
    let cell: vec2<i32> = world_cell_from_logical_index(logical_index);
    let index: u32 = physical_cell_index(cell);
    if is_obstacle(cell) {
        velocity[index] = vec2<f32>(0.0);
        return;
    }
    var density_offset: f32 = 0.0;
    for (var species: u32 = 0u; species < parameters.gas_count; species++) {
        density_offset += concentrations[concentration_index(species, index)] *
            (gas_properties[species].x - parameters.ambient_density);
    }
    let curl_gradient: vec2<f32> = 0.5 * vec2<f32>(
        abs(curl_at(cell + vec2<i32>(1, 0))) - abs(curl_at(cell + vec2<i32>(-1, 0))),
        abs(curl_at(cell + vec2<i32>(0, 1))) - abs(curl_at(cell + vec2<i32>(0, -1))),
    );
    let gradient_length: f32 = length(curl_gradient);
    let normal: vec2<f32> = select(
        vec2<f32>(0.0),
        curl_gradient / max(gradient_length, 0.000001),
        gradient_length > 0.000001,
    );
    let confinement: vec2<f32> = parameters.vorticity_confinement *
        vec2<f32>(normal.y, -normal.x) * curl[index];
    let buoyancy: vec2<f32> = parameters.gravity * CELLS_PER_TILE * density_offset *
        parameters.buoyancy_coefficient;
    var next_velocity: vec2<f32> = velocity_scratch[index] +
        (buoyancy + confinement) * parameters.delta_time;
    let speed: f32 = length(next_velocity);
    if speed > parameters.maximum_speed { next_velocity *= parameters.maximum_speed / speed; }
    velocity[index] = next_velocity;
}

@compute @workgroup_size(64)
fn calculate_gas_divergence(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let logical_index: u32 = invocation.x;
    if logical_index >= parameters.buffered_cell_count { return; }
    let cell: vec2<i32> = world_cell_from_logical_index(logical_index);
    let index: u32 = physical_cell_index(cell);
    if is_obstacle(cell) {
        divergence[index] = 0.0;
        return;
    }
    divergence[index] = 0.5 * (
        velocity_at(cell + vec2<i32>(1, 0)).x - velocity_at(cell + vec2<i32>(-1, 0)).x +
        velocity_at(cell + vec2<i32>(0, 1)).y - velocity_at(cell + vec2<i32>(0, -1)).y
    );
}

@compute @workgroup_size(64)
fn clear_gas_pressure(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if invocation.x >= parameters.buffered_cell_count { return; }
    pressure_a[invocation.x] = 0.0;
    pressure_b[invocation.x] = 0.0;
}

@compute @workgroup_size(64)
fn solve_gas_pressure_a(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let logical_index: u32 = invocation.x;
    if logical_index >= parameters.buffered_cell_count { return; }
    let cell: vec2<i32> = world_cell_from_logical_index(logical_index);
    let index: u32 = physical_cell_index(cell);
    if is_obstacle(cell) {
        pressure_a[index] = 0.0;
        return;
    }
    let center: f32 = pressure_b[index];
    pressure_a[index] = (pressure_b_at(cell + vec2<i32>(-1, 0), center) +
        pressure_b_at(cell + vec2<i32>(1, 0), center) +
        pressure_b_at(cell + vec2<i32>(0, -1), center) +
        pressure_b_at(cell + vec2<i32>(0, 1), center) - divergence[index]) * 0.25;
}

@compute @workgroup_size(64)
fn solve_gas_pressure_b(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let logical_index: u32 = invocation.x;
    if logical_index >= parameters.buffered_cell_count { return; }
    let cell: vec2<i32> = world_cell_from_logical_index(logical_index);
    let index: u32 = physical_cell_index(cell);
    if is_obstacle(cell) {
        pressure_b[index] = 0.0;
        return;
    }
    let center: f32 = pressure_a[index];
    pressure_b[index] = (pressure_a_at(cell + vec2<i32>(-1, 0), center) +
        pressure_a_at(cell + vec2<i32>(1, 0), center) +
        pressure_a_at(cell + vec2<i32>(0, -1), center) +
        pressure_a_at(cell + vec2<i32>(0, 1), center) - divergence[index]) * 0.25;
}

// Ten Jacobi iterations end in pressure B, which is the sole projection input.
@compute @workgroup_size(64)
fn project_gas_velocity(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let logical_index: u32 = invocation.x;
    if logical_index >= parameters.buffered_cell_count { return; }
    let cell: vec2<i32> = world_cell_from_logical_index(logical_index);
    let index: u32 = physical_cell_index(cell);
    if is_obstacle(cell) {
        velocity[index] = vec2<f32>(0.0);
        return;
    }
    let center: f32 = pressure_b[index];
    var projected: vec2<f32> = velocity[index] - 0.5 * vec2<f32>(
        pressure_b_at(cell + vec2<i32>(1, 0), center) -
            pressure_b_at(cell + vec2<i32>(-1, 0), center),
        pressure_b_at(cell + vec2<i32>(0, 1), center) -
            pressure_b_at(cell + vec2<i32>(0, -1), center),
    );
    if is_obstacle(cell + vec2<i32>(-1, 0)) && projected.x < 0.0 { projected.x = 0.0; }
    if is_obstacle(cell + vec2<i32>(1, 0)) && projected.x > 0.0 { projected.x = 0.0; }
    if is_obstacle(cell + vec2<i32>(0, -1)) && projected.y < 0.0 { projected.y = 0.0; }
    if is_obstacle(cell + vec2<i32>(0, 1)) && projected.y > 0.0 { projected.y = 0.0; }
    velocity[index] = projected;
}

@compute @workgroup_size(64)
fn advect_gas_concentrations(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let field_index: u32 = invocation.x;
    let field_count: u32 = parameters.buffered_cell_count * parameters.gas_count;
    if field_index >= field_count { return; }
    let species: u32 = field_index / parameters.buffered_cell_count;
    let logical_index: u32 = field_index % parameters.buffered_cell_count;
    let cell: vec2<i32> = world_cell_from_logical_index(logical_index);
    let index: u32 = physical_cell_index(cell);
    let output_index: u32 = concentration_index(species, index);
    if is_obstacle(cell) {
        concentration_scratch[output_index] = 0.0;
        return;
    }
    let center: vec2<f32> = vec2<f32>(cell) + vec2<f32>(0.5);
    let backtraced: vec2<f32> = center - velocity[index] * parameters.delta_time;
    let advected: f32 = sample_concentration(species, backtraced);
    let neighborhood: f32 = 0.25 * (
        sample_concentration(species, backtraced + vec2<f32>(-1.0, 0.0)) +
        sample_concentration(species, backtraced + vec2<f32>(1.0, 0.0)) +
        sample_concentration(species, backtraced + vec2<f32>(0.0, -1.0)) +
        sample_concentration(species, backtraced + vec2<f32>(0.0, 1.0))
    );
    let mixing: f32 = 1.0 - exp(-max(gas_properties[species].y, 0.0) * parameters.delta_time);
    let remaining: f32 = exp(-max(gas_properties[species].w, 0.0) * parameters.delta_time);
    concentration_scratch[output_index] = max(0.0, mix(advected, neighborhood, mixing)) * remaining;
}

@compute @workgroup_size(64)
fn clear_gas_area(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if invocation.x >= parameters.streaming_cell_count { return; }
    let cell: vec2<i32> = world_cell_from_streaming_index(invocation.x);
    let index: u32 = physical_cell_index(cell);
    if index == INVALID_INDEX { return; }
    velocity[index] = vec2<f32>(0.0);
    for (var species: u32 = 0u; species < parameters.gas_count; species++) {
        concentrations[concentration_index(species, index)] = 0.0;
    }
}

// Each invocation owns one fixed output record and clears the same outgoing physical cell.
@compute @workgroup_size(64)
fn export_gas_area(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let output_index: u32 = invocation.x;
    if output_index >= parameters.streaming_cell_count { return; }
    let cell: vec2<i32> = world_cell_from_streaming_index(output_index);
    let index: u32 = physical_cell_index(cell);
    let stride: u32 = 4u + parameters.gas_count;
    let start: u32 = output_index * stride;
    streaming_data[start] = bitcast<u32>(cell.x);
    streaming_data[start + 1u] = bitcast<u32>(cell.y);
    streaming_data[start + 2u] = bitcast<u32>(velocity[index].x);
    streaming_data[start + 3u] = bitcast<u32>(velocity[index].y);
    velocity[index] = vec2<f32>(0.0);
    for (var species: u32 = 0u; species < parameters.gas_count; species++) {
        let concentration: u32 = concentration_index(species, index);
        streaming_data[start + 4u + species] = bitcast<u32>(concentrations[concentration]);
        concentrations[concentration] = 0.0;
    }
}

fn sample_velocity(position: vec2<f32>) -> vec2<f32> {
    let shifted: vec2<f32> = position - vec2<f32>(0.5);
    let base: vec2<i32> = vec2<i32>(floor(shifted));
    let fraction: vec2<f32> = fract(shifted);
    let bottom: vec2<f32> = mix(
        velocity_at(base), velocity_at(base + vec2<i32>(1, 0)), fraction.x,
    );
    let top: vec2<f32> = mix(
        velocity_at(base + vec2<i32>(0, 1)),
        velocity_at(base + vec2<i32>(1, 1)), fraction.x,
    );
    return mix(bottom, top, fraction.y);
}

fn sample_concentration(species: u32, position: vec2<f32>) -> f32 {
    let shifted: vec2<f32> = position - vec2<f32>(0.5);
    let base: vec2<i32> = vec2<i32>(floor(shifted));
    let fraction: vec2<f32> = fract(shifted);
    let bottom: f32 = mix(
        concentration_at(species, base),
        concentration_at(species, base + vec2<i32>(1, 0)), fraction.x,
    );
    let top: f32 = mix(
        concentration_at(species, base + vec2<i32>(0, 1)),
        concentration_at(species, base + vec2<i32>(1, 1)), fraction.x,
    );
    return mix(bottom, top, fraction.y);
}

fn concentration_at(species: u32, cell: vec2<i32>) -> f32 {
    if is_obstacle(cell) { return 0.0; }
    let index: u32 = physical_cell_index(cell);
    if index == INVALID_INDEX { return 0.0; }
    return concentrations[concentration_index(species, index)];
}

fn concentration_index(species: u32, physical_index: u32) -> u32 {
    return species * parameters.buffered_cell_count + physical_index;
}

fn velocity_at(cell: vec2<i32>) -> vec2<f32> {
    if is_obstacle(cell) { return vec2<f32>(0.0); }
    let index: u32 = physical_cell_index(cell);
    if index == INVALID_INDEX { return vec2<f32>(0.0); }
    return velocity[index];
}

fn scratch_velocity_at(cell: vec2<i32>) -> vec2<f32> {
    if is_obstacle(cell) { return vec2<f32>(0.0); }
    let index: u32 = physical_cell_index(cell);
    if index == INVALID_INDEX { return vec2<f32>(0.0); }
    return velocity_scratch[index];
}

fn curl_at(cell: vec2<i32>) -> f32 {
    if is_obstacle(cell) { return 0.0; }
    let index: u32 = physical_cell_index(cell);
    if index == INVALID_INDEX { return 0.0; }
    return curl[index];
}

fn pressure_a_at(cell: vec2<i32>, boundary: f32) -> f32 {
    if is_obstacle(cell) { return boundary; }
    return pressure_a[physical_cell_index(cell)];
}

fn pressure_b_at(cell: vec2<i32>, boundary: f32) -> f32 {
    if is_obstacle(cell) { return boundary; }
    return pressure_b[physical_cell_index(cell)];
}

fn is_obstacle(cell: vec2<i32>) -> bool {
    let index: u32 = physical_cell_index(cell);
    return index == INVALID_INDEX || cellular_material_identifiers[index] != 0u ||
        external_body_occupancy[index] != 0u ||
        fluid_coverage[index] >= parameters.fluid_obstacle_coverage;
}

fn physical_cell_index(cell: vec2<i32>) -> u32 {
    let tile: vec2<i32> = vec2<i32>(floor_divide(cell.x, 8), floor_divide(cell.y, 8));
    let relative: vec2<i32> = tile - parameters.buffered_origin;
    if any(relative < vec2<i32>(0)) || relative.x >= i32(parameters.buffered_tile_size.x) ||
            relative.y >= i32(parameters.buffered_tile_size.y) { return INVALID_INDEX; }
    let physical: vec2<u32> =
        (vec2<u32>(relative) + parameters.ring_offset) % parameters.buffered_tile_size;
    let local: vec2<u32> = vec2<u32>(cell - tile * 8);
    return (physical.y * parameters.buffered_tile_size.x + physical.x) * 64u +
        local.y * 8u + local.x;
}

fn world_cell_from_logical_index(index: u32) -> vec2<i32> {
    let tile_index: u32 = index / 64u;
    let local_index: u32 = index % 64u;
    return (parameters.buffered_origin + vec2<i32>(
        i32(tile_index % parameters.buffered_tile_size.x),
        i32(tile_index / parameters.buffered_tile_size.x),
    )) * 8 + vec2<i32>(i32(local_index % 8u), i32(local_index / 8u));
}

fn world_cell_from_streaming_index(index: u32) -> vec2<i32> {
    let width: u32 = parameters.streaming_tile_size.x * 8u;
    return parameters.streaming_origin * 8 + vec2<i32>(
        i32(index % width), i32(index / width),
    );
}

fn floor_divide(value: i32, divisor: i32) -> i32 {
    if value < 0 { return (value - divisor + 1) / divisor; }
    return value / divisor;
}
