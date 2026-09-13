// Copyright Rob Gage 2026

struct Particle {
    material_identifier: u32,
    is_active: u32,
    position: vec2<f32>,
    velocity: vec2<f32>,
    padding_2: vec2<u32>,
}

struct Parameters {
    buffered_origin: vec2<i32>,
    buffered_tile_size: vec2<u32>,
    active_origin: vec2<i32>,
    active_tile_size: vec2<u32>,
    ring_offset: vec2<u32>,
    bucket_dimensions: vec2<u32>,
    streaming_origin: vec2<i32>,
    streaming_tile_size: vec2<u32>,
    gravity: vec2<f32>,
    delta_time: f32,
    particle_capacity: u32,
    buffered_cell_count: u32,
    bucket_count: u32,
    support_radius_cells: f32,
    particle_radius_cells: f32,
    maximum_movement_cells: u32,
    padding_0: u32,
    padding_1: vec2<u32>,
}

@group(0) @binding(0) var<storage, read_write> particles: array<Particle>;
@group(0) @binding(1) var<storage, read_write> free_indices: array<u32>;
@group(0) @binding(2) var<storage, read_write> free_count: array<atomic<u32>>;
@group(0) @binding(3) var<storage, read_write> edit_cells: array<u32>;
@group(0) @binding(4) var<storage, read_write> bucket_heads: array<atomic<u32>>;
@group(0) @binding(5) var<storage, read_write> next_particle: array<u32>;
@group(0) @binding(6) var<storage, read_write> derived_material_identifiers: array<u32>;
@group(0) @binding(7) var<storage, read_write> derived_coverage: array<f32>;
@group(0) @binding(8) var<storage, read_write> derived_velocity: array<vec4<f32>>;
@group(0) @binding(9) var<storage, read> cellular_material_identifiers: array<u32>;
@group(0) @binding(10) var<storage, read> external_body_occupancy: array<u32>;
@group(0) @binding(11) var<storage, read> external_body_velocity: array<vec4<f32>>;
@group(0) @binding(12) var<uniform> parameters: Parameters;
@group(0) @binding(13) var<storage, read_write> predicted_positions: array<vec2<f32>>;
@group(0) @binding(14) var<storage, read_write> lambdas: array<f32>;
@group(0) @binding(15) var<storage, read_write> position_corrections: array<vec2<f32>>;
@group(0) @binding(16) var<storage, read> fluid_material_properties: array<vec4<f32>>;
@group(0) @binding(17) var<storage, read_write> streaming_particles: array<Particle>;
@group(0) @binding(18) var<storage, read_write> streaming_count: array<atomic<u32>>;
@group(0) @binding(19) var<storage, read_write> streaming_results: array<u32>;

const EMPTY: u32 = 0u;
const INVALID_INDEX: u32 = 0xffffffffu;
const FLUID_FORM: u32 = 3u;
const CELLS_PER_TILE: f32 = 8.0;
const PI: f32 = 3.141592653589793;
const PBF_SUBSTEP_COUNT: f32 = 2.0;
const PBF_CONSTRAINT_ITERATION_COUNT: f32 = 4.0;
const CONSTRAINT_EPSILON: f32 = 0.01;
const ARTIFICIAL_PRESSURE_DELTA_Q_RATIO: f32 = 0.3;
const MAXIMUM_CORRECTION_CELLS: f32 = 0.25;

// Removes every authoritative particle whose current world cell was edited
@compute @workgroup_size(64)
fn remove_edited_fluid_particles(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let particle_index: u32 = invocation.x;
    if particle_index >= parameters.particle_capacity ||
            particles[particle_index].material_identifier == EMPTY { return; }
    let cell: vec2<i32> = vec2<i32>(floor(particles[particle_index].position * CELLS_PER_TILE));
    let cell_index: u32 = physical_cell_index(cell);
    if cell_index == INVALID_INDEX || edit_cells[cell_index] == EMPTY { return; }
    release_particle(particle_index);
}

// Creates at most one particle at each edited world-cell center
@compute @workgroup_size(64)
fn spawn_edited_fluid_particles(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let logical_index: u32 = invocation.x;
    if logical_index >= parameters.buffered_cell_count { return; }
    let cell: vec2<i32> = world_cell_from_logical_index(logical_index);
    let cell_index: u32 = physical_cell_index(cell);
    let material_identifier: u32 = edit_cells[cell_index];
    if material_identifier >> 30u != FLUID_FORM ||
            cellular_material_identifiers[cell_index] != EMPTY { return; }
    let particle_index: u32 = claim_free_particle();
    if particle_index == INVALID_INDEX { return; }
    particles[particle_index] = Particle(
        material_identifier,
        0u,
        (vec2<f32>(cell) + vec2<f32>(0.5)) / CELLS_PER_TILE,
        vec2<f32>(0.0),
        vec2<u32>(0u),
    );
}

@compute @workgroup_size(64)
fn clear_fluid_edits(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if invocation.x < parameters.buffered_cell_count { edit_cells[invocation.x] = EMPTY; }
}

// Freezes active-area membership for all substeps in this fixed tick
@compute @workgroup_size(64)
fn classify_active_fluid_particles(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let particle_index: u32 = invocation.x;
    if particle_index >= parameters.particle_capacity ||
            particles[particle_index].material_identifier == EMPTY { return; }
    particles[particle_index].is_active = select(
        0u,
        1u,
        position_is_active(particles[particle_index].position),
    );
}

// Predicts one PBF substep while retaining the previous authoritative position for velocity reconstruction
@compute @workgroup_size(64)
fn predict_fluid_particles(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let particle_index: u32 = invocation.x;
    if particle_index >= parameters.particle_capacity ||
            particles[particle_index].material_identifier == EMPTY { return; }
    var particle: Particle = particles[particle_index];
    if particle.is_active == 0u {
        if position_supports_active_buckets(particle.position) {
            predicted_positions[particle_index] = particle.position;
        }
        return;
    }
    let substep_dt: f32 = parameters.delta_time / PBF_SUBSTEP_COUNT;
    particle.velocity += parameters.gravity * substep_dt;
    let maximum_speed: f32 = f32(parameters.maximum_movement_cells) /
        (CELLS_PER_TILE * parameters.delta_time);
    let speed: f32 = length(particle.velocity);
    if speed > maximum_speed { particle.velocity *= maximum_speed / speed; }
    let movement_cells: f32 = length(particle.velocity) * substep_dt * CELLS_PER_TILE;
    let step_count: u32 = clamp(u32(ceil(movement_cells * 2.0)), 1u,
        parameters.maximum_movement_cells * 2u);
    var position: vec2<f32> = particle.position;
    for (var movement_step: u32 = 0u; movement_step < step_count; movement_step++) {
        position += particle.velocity * substep_dt / f32(step_count);
        let resolved: vec4<f32> = resolve_particle_collisions(
            position,
            particle.velocity,
            particle.material_identifier,
        );
        position = resolved.xy;
        particle.velocity = resolved.zw;
    }
    particles[particle_index] = particle;
    predicted_positions[particle_index] = position;
}

@compute @workgroup_size(64)
fn clear_fluid_buckets(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if invocation.x < parameters.bucket_count {
        atomicStore(&bucket_heads[invocation.x], INVALID_INDEX);
    }
}

@compute @workgroup_size(64)
fn insert_fluid_particles(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let particle_index: u32 = invocation.x;
    if particle_index >= parameters.particle_capacity ||
            particles[particle_index].material_identifier == EMPTY { return; }
    let bucket: u32 = bucket_index(particles[particle_index].position);
    if bucket == INVALID_INDEX { return; }
    next_particle[particle_index] = atomicExchange(&bucket_heads[bucket], particle_index);
}

// Builds the same linked-list grid from predicted rather than committed positions
@compute @workgroup_size(64)
fn insert_predicted_fluid_particles(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let particle_index: u32 = invocation.x;
    if particle_index >= parameters.particle_capacity ||
            particles[particle_index].material_identifier == EMPTY { return; }
    if !position_supports_active_buckets(particles[particle_index].position) { return; }
    let bucket: u32 = bucket_index(predicted_positions[particle_index]);
    if bucket == INVALID_INDEX { return; }
    next_particle[particle_index] = atomicExchange(&bucket_heads[bucket], particle_index);
}

// Calculates the standard PBF density constraint and lambda from immutable predicted positions
@compute @workgroup_size(64)
fn calculate_fluid_lambdas(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let particle_index: u32 = invocation.x;
    if particle_index >= parameters.particle_capacity ||
            particles[particle_index].material_identifier == EMPTY { return; }
    let position: vec2<f32> = predicted_positions[particle_index];
    if !position_supports_active_lambdas(position) { return; }
    let rest_density: f32 = fluid_properties_for(particles[particle_index].material_identifier).x;
    let base_bucket: vec2<i32> = bucket_coordinates(position);
    var density: f32 = poly6_kernel(0.0);
    var self_gradient: vec2<f32> = vec2<f32>(0.0);
    var gradient_squared_sum: f32 = 0.0;
    for (var bucket_y: i32 = -1; bucket_y <= 1; bucket_y++) {
        for (var bucket_x: i32 = -1; bucket_x <= 1; bucket_x++) {
            let bucket: u32 = bucket_index_from_coordinates(
                base_bucket + vec2<i32>(bucket_x, bucket_y),
            );
            if bucket == INVALID_INDEX { continue; }
            var neighbor_index: u32 = atomicLoad(&bucket_heads[bucket]);
            for (var chain_length: u32 = 0u;
                    neighbor_index != INVALID_INDEX && chain_length < parameters.particle_capacity;
                    chain_length++) {
                if neighbor_index != particle_index {
                    let separation: vec2<f32> =
                        (position - predicted_positions[neighbor_index]) * CELLS_PER_TILE;
                    let distance: f32 = length(separation);
                    if distance < parameters.support_radius_cells {
                        density += poly6_kernel(distance);
                        let gradient: vec2<f32> = spiky_gradient(separation, distance) /
                            rest_density;
                        self_gradient += gradient;
                        gradient_squared_sum += dot(gradient, gradient);
                    }
                }
                neighbor_index = next_particle[neighbor_index];
            }
        }
    }
    gradient_squared_sum += dot(self_gradient, self_gradient);
    let constraint: f32 = density / rest_density - 1.0;
    lambdas[particle_index] = -constraint /
        (gradient_squared_sum + CONSTRAINT_EPSILON);
}

// Gathers correction separately so every lambda and predicted position is immutable during this pass
@compute @workgroup_size(64)
fn calculate_fluid_position_corrections(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let particle_index: u32 = invocation.x;
    if particle_index >= parameters.particle_capacity ||
            particles[particle_index].material_identifier == EMPTY { return; }
    if particles[particle_index].is_active == 0u { return; }
    let position: vec2<f32> = predicted_positions[particle_index];
    let base_bucket: vec2<i32> = bucket_coordinates(position);
    var correction_cells: vec2<f32> = vec2<f32>(0.0);
    for (var bucket_y: i32 = -1; bucket_y <= 1; bucket_y++) {
        for (var bucket_x: i32 = -1; bucket_x <= 1; bucket_x++) {
            let bucket: u32 = bucket_index_from_coordinates(
                base_bucket + vec2<i32>(bucket_x, bucket_y),
            );
            if bucket == INVALID_INDEX { continue; }
            var neighbor_index: u32 = atomicLoad(&bucket_heads[bucket]);
            for (var chain_length: u32 = 0u;
                    neighbor_index != INVALID_INDEX && chain_length < parameters.particle_capacity;
                    chain_length++) {
                if neighbor_index != particle_index {
                    let separation: vec2<f32> =
                        (position - predicted_positions[neighbor_index]) * CELLS_PER_TILE;
                    let distance: f32 = length(separation);
                    if distance > 0.000001 && distance < parameters.support_radius_cells {
                        correction_cells += (lambdas[particle_index] + lambdas[neighbor_index] +
                            artificial_pressure(distance, particles[particle_index].material_identifier)) * spiky_gradient(separation, distance) /
                            fluid_properties_for(particles[particle_index].material_identifier).x;
                    }
                }
                neighbor_index = next_particle[neighbor_index];
            }
        }
    }
    let correction_length: f32 = length(correction_cells);
    if correction_length > MAXIMUM_CORRECTION_CELLS {
        correction_cells *= MAXIMUM_CORRECTION_CELLS / correction_length;
    }
    position_corrections[particle_index] = correction_cells / CELLS_PER_TILE;
}

// Applies race-free corrections and reprojects against the same concrete solid boundary
@compute @workgroup_size(64)
fn apply_fluid_position_corrections(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let particle_index: u32 = invocation.x;
    if particle_index >= parameters.particle_capacity ||
            particles[particle_index].material_identifier == EMPTY { return; }
    if particles[particle_index].is_active == 0u { return; }
    let corrected: vec2<f32> = predicted_positions[particle_index] +
        position_corrections[particle_index];
    predicted_positions[particle_index] = resolve_particle_collisions(
        corrected,
        particles[particle_index].velocity,
        particles[particle_index].material_identifier,
    ).xy;
}

// Commits the corrected position and reconstructs velocity from the retained authoritative position
@compute @workgroup_size(64)
fn commit_fluid_particles(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let particle_index: u32 = invocation.x;
    if particle_index >= parameters.particle_capacity ||
            particles[particle_index].material_identifier == EMPTY { return; }
    if particles[particle_index].is_active == 0u { return; }
    let position: vec2<f32> = predicted_positions[particle_index];
    if !position_is_resident(position) { return; }
    let substep_dt: f32 = parameters.delta_time / PBF_SUBSTEP_COUNT;
    var velocity: vec2<f32> = (position - particles[particle_index].position) / substep_dt;
    let maximum_speed: f32 = f32(parameters.maximum_movement_cells) /
        (CELLS_PER_TILE * parameters.delta_time);
    let speed: f32 = length(velocity);
    if speed > maximum_speed { velocity *= maximum_speed / speed; }
    particles[particle_index].position = position;
    particles[particle_index].velocity = velocity;
}

// Calculates one final XSPH correction from committed neighbors to quiet constraint noise
@compute @workgroup_size(64)
fn calculate_fluid_velocity_smoothing(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let particle_index: u32 = invocation.x;
    if particle_index >= parameters.particle_capacity ||
            particles[particle_index].material_identifier == EMPTY { return; }
    let particle: Particle = particles[particle_index];
    if particle.is_active == 0u { return; }
    let base_bucket: vec2<i32> = bucket_coordinates(particle.position);
    var difference_sum: vec2<f32> = vec2<f32>(0.0);
    var weight_sum: f32 = 0.0;
    for (var bucket_y: i32 = -1; bucket_y <= 1; bucket_y++) {
        for (var bucket_x: i32 = -1; bucket_x <= 1; bucket_x++) {
            let bucket: u32 = bucket_index_from_coordinates(
                base_bucket + vec2<i32>(bucket_x, bucket_y),
            );
            if bucket == INVALID_INDEX { continue; }
            var neighbor_index: u32 = atomicLoad(&bucket_heads[bucket]);
            for (var chain_length: u32 = 0u;
                    neighbor_index != INVALID_INDEX && chain_length < parameters.particle_capacity;
                    chain_length++) {
                if neighbor_index != particle_index {
                    let distance: f32 = length(
                        (particle.position - particles[neighbor_index].position) * CELLS_PER_TILE,
                    );
                    let weight: f32 = poly6_kernel(distance);
                    difference_sum +=
                        (particles[neighbor_index].velocity - particle.velocity) * weight;
                    weight_sum += weight;
                }
                neighbor_index = next_particle[neighbor_index];
            }
        }
    }
    position_corrections[particle_index] = fluid_properties_for(particle.material_identifier).z * difference_sum /
        max(weight_sum, 0.000001);
}

@compute @workgroup_size(64)
fn apply_fluid_velocity_smoothing(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let particle_index: u32 = invocation.x;
    if particle_index >= parameters.particle_capacity ||
            particles[particle_index].material_identifier == EMPTY { return; }
    if particles[particle_index].is_active == 0u { return; }
    particles[particle_index].velocity += position_corrections[particle_index];
}

// Transfers exact authoritative records out before their world tiles leave residency
@compute @workgroup_size(64)
fn export_fluid_particles(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let particle_index: u32 = invocation.x;
    if particle_index >= parameters.particle_capacity ||
            particles[particle_index].material_identifier == EMPTY { return; }
    let tile: vec2<i32> = vec2<i32>(floor(particles[particle_index].position));
    let relative: vec2<i32> = tile - parameters.streaming_origin;
    if any(relative < vec2<i32>(0)) || relative.x >= i32(parameters.streaming_tile_size.x) ||
            relative.y >= i32(parameters.streaming_tile_size.y) { return; }
    let output_index: u32 = atomicAdd(&streaming_count[0], 1u);
    streaming_particles[output_index] = particles[particle_index];
    release_particle(particle_index);
}

// Reclaims authoritative GPU slots and records each claim result for asynchronous ownership transfer
@compute @workgroup_size(64)
fn import_fluid_particles(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let input_index: u32 = invocation.x;
    if input_index >= atomicLoad(&streaming_count[0]) { return; }
    let particle_index: u32 = claim_free_particle();
    if particle_index == INVALID_INDEX {
        streaming_results[input_index] = 0u;
        return;
    }
    particles[particle_index] = streaming_particles[input_index];
    streaming_results[input_index] = 1u;
}

// Each cell gathers its own result, so particles never contend for derived-cell writes
@compute @workgroup_size(64)
fn rasterize_fluid_cells(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let logical_index: u32 = invocation.x;
    if logical_index >= parameters.buffered_cell_count { return; }
    let cell: vec2<i32> = world_cell_from_logical_index(logical_index);
    let physical_index: u32 = physical_cell_index(cell);
    let center: vec2<f32> = vec2<f32>(cell) + vec2<f32>(0.5);
    let base_bucket: vec2<i32> = bucket_coordinates(center / CELLS_PER_TILE);
    var weight_sum: f32 = 0.0;
    var velocity_sum: vec2<f32> = vec2<f32>(0.0);
    var strongest_weight: f32 = 0.0;
    var material_identifier: u32 = EMPTY;
    for (var bucket_y: i32 = -1; bucket_y <= 1; bucket_y++) {
        for (var bucket_x: i32 = -1; bucket_x <= 1; bucket_x++) {
            let bucket: u32 = bucket_index_from_coordinates(
                base_bucket + vec2<i32>(bucket_x, bucket_y),
            );
            if bucket == INVALID_INDEX { continue; }
            var particle_index: u32 = atomicLoad(&bucket_heads[bucket]);
            for (var chain_length: u32 = 0u;
                    particle_index != INVALID_INDEX && chain_length < parameters.particle_capacity;
                    chain_length++) {
                let particle: Particle = particles[particle_index];
                let distance_cells: f32 = length(particle.position * CELLS_PER_TILE - center);
                // Support radius is for PBF neighbors; one particle should render as about one cell.
                let particle_diameter: f32 = 1.2;
                let weight: f32 = max(0.0, 1.0 - distance_cells / particle_diameter);
                if weight > 0.0 {
                    weight_sum += weight;
                    velocity_sum += particle.velocity * weight;
                    if weight > strongest_weight {
                        strongest_weight = weight;
                        material_identifier = particle.material_identifier;
                    }
                }
                particle_index = next_particle[particle_index];
            }
        }
    }
    derived_material_identifiers[physical_index] = material_identifier;
    derived_coverage[physical_index] = min(weight_sum, 1.0);
    derived_velocity[physical_index] = vec4<f32>(
        select(vec2<f32>(0.0), velocity_sum / max(weight_sum, 0.000001), weight_sum > 0.0),
        0.0,
        0.0,
    );
}

fn poly6_kernel(distance: f32) -> f32 {
    let h: f32 = parameters.support_radius_cells;
    if distance >= h { return 0.0; }
    let difference: f32 = h * h - distance * distance;
    return 4.0 * difference * difference * difference /
        (PI * h * h * h * h * h * h * h * h);
}

fn spiky_gradient(separation: vec2<f32>, distance: f32) -> vec2<f32> {
    let h: f32 = parameters.support_radius_cells;
    if distance <= 0.000001 || distance >= h { return vec2<f32>(0.0); }
    let remaining: f32 = h - distance;
    return -30.0 * remaining * remaining / (PI * h * h * h * h * h) *
        separation / distance;
}

fn artificial_pressure(distance: f32, material_identifier: u32) -> f32 {
    let reference: f32 = poly6_kernel(
        ARTIFICIAL_PRESSURE_DELTA_Q_RATIO * parameters.support_radius_cells,
    );
    let ratio: f32 = poly6_kernel(distance) / max(reference, 0.000001);
    let squared: f32 = ratio * ratio;
    return -fluid_properties_for(material_identifier).y * squared * squared;
}

fn fluid_properties_for(material_identifier: u32) -> vec4<f32> {
    return fluid_material_properties[(material_identifier & 0x3fffffffu) * 2u];
}

fn fluid_contact_properties_for(material_identifier: u32) -> vec2<f32> {
    return fluid_material_properties[(material_identifier & 0x3fffffffu) * 2u + 1u].xy;
}

fn resolve_particle_collisions(
    initial_position: vec2<f32>,
    initial_velocity: vec2<f32>,
    material_identifier: u32,
) -> vec4<f32> {
    var position: vec2<f32> = initial_position;
    var velocity: vec2<f32> = initial_velocity;
    let radius: f32 = parameters.particle_radius_cells / CELLS_PER_TILE;
    for (var iteration: u32 = 0u; iteration < 2u; iteration++) {
        let center_cell: vec2<i32> = vec2<i32>(floor(position * CELLS_PER_TILE));
        var resolved: bool = false;
        for (var offset_y: i32 = -1; offset_y <= 1 && !resolved; offset_y++) {
            for (var offset_x: i32 = -1; offset_x <= 1; offset_x++) {
                let cell: vec2<i32> = center_cell + vec2<i32>(offset_x, offset_y);
                let index: u32 = physical_cell_index(cell);
                if index == INVALID_INDEX || (cellular_material_identifiers[index] == EMPTY &&
                        external_body_occupancy[index] == EMPTY) { continue; }
                let minimum: vec2<f32> = vec2<f32>(cell) / CELLS_PER_TILE;
                let maximum: vec2<f32> = vec2<f32>(cell + vec2<i32>(1)) / CELLS_PER_TILE;
                let nearest: vec2<f32> = clamp(position, minimum, maximum);
                let delta: vec2<f32> = position - nearest;
                let distance: f32 = length(delta);
                if distance >= radius { continue; }
                var normal: vec2<f32>;
                var penetration: f32;
                if distance > 0.000001 {
                    normal = delta / distance;
                    penetration = radius - distance;
                } else {
                    let distances: vec4<f32> = vec4<f32>(
                        position.x - minimum.x,
                        maximum.x - position.x,
                        position.y - minimum.y,
                        maximum.y - position.y,
                    );
                    let side: f32 = min(min(distances.x, distances.y), min(distances.z, distances.w));
                    normal = select(select(vec2<f32>(-1.0, 0.0), vec2<f32>(1.0, 0.0), side == distances.y),
                        select(vec2<f32>(0.0, -1.0), vec2<f32>(0.0, 1.0), side == distances.w),
                        side == distances.z || side == distances.w);
                    penetration = radius + side;
                }
                position += normal * penetration;
                var boundary_velocity: vec2<f32> = select(
                    vec2<f32>(0.0), external_body_velocity[index].xy,
                    external_body_occupancy[index] != EMPTY,
                );
                let boundary_speed: f32 = length(boundary_velocity) * CELLS_PER_TILE;
                let body_push_speed: f32 = fluid_properties_for(material_identifier).w;
                if boundary_speed > body_push_speed {
                    boundary_velocity *= body_push_speed / boundary_speed;
                }
                var relative_velocity: vec2<f32> = velocity - boundary_velocity;
                let inward_speed: f32 = dot(relative_velocity, normal);
                if inward_speed < 0.0 {
                    relative_velocity -= normal * inward_speed *
                        (1.0 + fluid_contact_properties_for(material_identifier).y);
                }
                let normal_velocity: vec2<f32> = normal * dot(relative_velocity, normal);
                relative_velocity -= (relative_velocity - normal_velocity) *
                    fluid_contact_properties_for(material_identifier).x;
                velocity = boundary_velocity + relative_velocity;
                resolved = true;
                break;
            }
        }
        if !resolved { break; }
    }
    return vec4<f32>(position, velocity);
}

fn claim_free_particle() -> u32 {
    var available: u32 = atomicLoad(&free_count[0]);
    loop {
        if available == 0u { return INVALID_INDEX; }
        let result = atomicCompareExchangeWeak(&free_count[0], available, available - 1u);
        if result.exchanged { return free_indices[available - 1u]; }
        available = result.old_value;
    }
    return INVALID_INDEX;
}

fn release_particle(particle_index: u32) {
    particles[particle_index].material_identifier = EMPTY;
    let free_index: u32 = atomicAdd(&free_count[0], 1u);
    free_indices[free_index] = particle_index;
}

fn position_is_resident(position: vec2<f32>) -> bool {
    let relative: vec2<f32> = position - vec2<f32>(parameters.buffered_origin);
    return all(relative >= vec2<f32>(0.0)) &&
        relative.x < f32(parameters.buffered_tile_size.x) &&
        relative.y < f32(parameters.buffered_tile_size.y);
}

fn position_is_active(position: vec2<f32>) -> bool {
    let relative: vec2<f32> = position - vec2<f32>(parameters.active_origin);
    return all(relative >= vec2<f32>(0.0)) &&
        relative.x < f32(parameters.active_tile_size.x) &&
        relative.y < f32(parameters.active_tile_size.y);
}

// Neighbor lambdas need one support radius beyond particles that directly support active fluid.
fn position_supports_active_buckets(position: vec2<f32>) -> bool {
    let predicted_movement_cells: f32 = f32(parameters.maximum_movement_cells) /
        PBF_SUBSTEP_COUNT + MAXIMUM_CORRECTION_CELLS * PBF_CONSTRAINT_ITERATION_COUNT;
    return position_is_within_active_padding(
        (parameters.support_radius_cells * 2.0 + predicted_movement_cells) / CELLS_PER_TILE,
        position,
    );
}

fn position_supports_active_lambdas(position: vec2<f32>) -> bool {
    let predicted_movement_cells: f32 = f32(parameters.maximum_movement_cells) /
        PBF_SUBSTEP_COUNT + MAXIMUM_CORRECTION_CELLS * PBF_CONSTRAINT_ITERATION_COUNT;
    return position_is_within_active_padding(
        (parameters.support_radius_cells + predicted_movement_cells) / CELLS_PER_TILE,
        position,
    );
}

fn position_is_within_active_padding(padding: f32, position: vec2<f32>) -> bool {
    let minimum: vec2<f32> = vec2<f32>(parameters.active_origin) - vec2<f32>(padding);
    let maximum: vec2<f32> = vec2<f32>(parameters.active_origin) +
        vec2<f32>(parameters.active_tile_size) + vec2<f32>(padding);
    return all(position >= minimum) && all(position < maximum);
}

fn bucket_coordinates(position: vec2<f32>) -> vec2<i32> {
    let origin: vec2<f32> = vec2<f32>(parameters.buffered_origin);
    let bucket_size: f32 = parameters.support_radius_cells / CELLS_PER_TILE;
    return vec2<i32>(floor((position - origin) / bucket_size));
}

fn bucket_index(position: vec2<f32>) -> u32 {
    return bucket_index_from_coordinates(bucket_coordinates(position));
}

fn bucket_index_from_coordinates(bucket: vec2<i32>) -> u32 {
    if any(bucket < vec2<i32>(0)) || bucket.x >= i32(parameters.bucket_dimensions.x) ||
            bucket.y >= i32(parameters.bucket_dimensions.y) { return INVALID_INDEX; }
    return u32(bucket.y) * parameters.bucket_dimensions.x + u32(bucket.x);
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

fn floor_divide(value: i32, divisor: i32) -> i32 {
    if value < 0 { return (value - divisor + 1) / divisor; }
    return value / divisor;
}
