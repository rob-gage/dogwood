// Copyright Rob Gage 2026

#define_import_path compute::fluids

#import utility::capsule_collision::{
    GravityRelativeCapsule,
    gravity_relative_capsule_from_parameters,
    gravity_relative_capsule_world_extent,
    world_position_is_inside_gravity_relative_capsule,
}
#import utility::cell_coordinates::{
    CELLS_PER_TILE_FLOAT,
    world_cell_from_logical_tile_major_index,
}
#import utility::material_identifier::{
    EMPTY_MATERIAL_IDENTIFIER,
    FLUID_MATERIAL_FORM,
    material_form_from_identifier,
    material_index_from_identifier,
}
#import utility::tile_ring::{INVALID_PHYSICAL_CELL_INDEX, physical_cell_index_from_world_cell}

struct Particle {
    material_identifier: u32,
    is_active: u32,
    position: vec2<f32>,
    velocity: vec2<f32>,
    prediction_collision_displacement: vec2<f32>,
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
    sample_center: vec2<f32>,
    sample_collider: vec2<f32>,
    padding_1: vec2<u32>,
}

struct DerivedFluidCellSample {
    material_identifier: u32,
    coverage: f32,
    velocity: vec2<f32>,
}

struct DerivedFluidCapsuleSample {
    capsule_cell_count: f32,
    coverage_sum: f32,
    velocity_sum: vec2<f32>,
    density_sum: f32,
    viscosity_sum: f32,
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
@group(0) @binding(20) var<storage, read_write> sample_output: array<vec4<f32>>;

const INVALID_FLUID_PARTICLE_INDEX: u32 = 0xffffffffu;
const INVALID_FLUID_BUCKET_INDEX: u32 = 0xffffffffu;
const PI: f32 = 3.141592653589793;
const PBF_SUBSTEP_COUNT: f32 = 2.0;
const PBF_CONSTRAINT_ITERATION_COUNT: f32 = 4.0;
const CONSTRAINT_EPSILON: f32 = 0.01;
const ARTIFICIAL_PRESSURE_DELTA_Q_RATIO: f32 = 0.3;
const MAXIMUM_CORRECTION_CELLS: f32 = 0.25;
const HARD_EXTERNAL_BODY_OCCUPANCY: u32 = 1u;
const SWIMMER_EXTERNAL_BODY_OCCUPANCY: u32 = 2u;

// Removes every authoritative particle whose current world cell was edited
@compute @workgroup_size(64)
fn remove_edited_fluid_particles(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let particle_index: u32 = invocation.x;
    if particle_index >= parameters.particle_capacity ||
            particles[particle_index].material_identifier == EMPTY_MATERIAL_IDENTIFIER { return; }
    let cell: vec2<i32> = vec2<i32>(floor(
        particles[particle_index].position * CELLS_PER_TILE_FLOAT,
    ));
    let cell_index: u32 = fluid_physical_cell_index_from_world_cell(cell);
    if cell_index == INVALID_PHYSICAL_CELL_INDEX ||
            edit_cells[cell_index] == EMPTY_MATERIAL_IDENTIFIER { return; }
    release_fluid_particle_index(particle_index);
}

// Creates at most one particle at each edited world-cell center
@compute @workgroup_size(64)
fn spawn_edited_fluid_particles(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let logical_index: u32 = invocation.x;
    if logical_index >= parameters.buffered_cell_count { return; }
    let cell: vec2<i32> = world_cell_from_fluid_logical_index(logical_index);
    let cell_index: u32 = fluid_physical_cell_index_from_world_cell(cell);
    let material_identifier: u32 = edit_cells[cell_index];
    if material_form_from_identifier(material_identifier) != FLUID_MATERIAL_FORM ||
            cellular_material_identifiers[cell_index] != EMPTY_MATERIAL_IDENTIFIER { return; }
    let particle_index: u32 = claim_free_fluid_particle_index();
    if particle_index == INVALID_FLUID_PARTICLE_INDEX { return; }
    particles[particle_index] = Particle(
        material_identifier,
        0u,
        (vec2<f32>(cell) + vec2<f32>(0.5)) / CELLS_PER_TILE_FLOAT,
        vec2<f32>(0.0),
        vec2<f32>(0.0),
    );
}

// Clears transient fluid edit requests after remove and spawn passes consume them
@compute @workgroup_size(64)
fn clear_fluid_edits(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if invocation.x < parameters.buffered_cell_count {
        edit_cells[invocation.x] = EMPTY_MATERIAL_IDENTIFIER;
    }
}

// Freezes active-area membership for all substeps in this fixed tick
@compute @workgroup_size(64)
fn classify_active_fluid_particles(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let particle_index: u32 = invocation.x;
    if particle_index >= parameters.particle_capacity ||
            particles[particle_index].material_identifier == EMPTY_MATERIAL_IDENTIFIER { return; }
    particles[particle_index].is_active = select(
        0u,
        1u,
        fluid_position_is_inside_active_region(particles[particle_index].position),
    );
}

// Predicts one PBF substep while retaining the previous authoritative position for velocity reconstruction
@compute @workgroup_size(64)
fn predict_fluid_particles(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let particle_index: u32 = invocation.x;
    if particle_index >= parameters.particle_capacity ||
            particles[particle_index].material_identifier == EMPTY_MATERIAL_IDENTIFIER { return; }
    var particle: Particle = particles[particle_index];
    if particle.is_active == 0u {
        if fluid_position_supports_active_neighbor_buckets(particle.position) {
            predicted_positions[particle_index] = particle.position;
        }
        return;
    }
    let substep_delta_time: f32 = parameters.delta_time / PBF_SUBSTEP_COUNT;
    particle.velocity += parameters.gravity * substep_delta_time;
    particle.prediction_collision_displacement = vec2<f32>(0.0);
    let maximum_speed: f32 = f32(parameters.maximum_movement_cells) /
        (CELLS_PER_TILE_FLOAT * parameters.delta_time);
    let speed: f32 = length(particle.velocity);
    if speed > maximum_speed { particle.velocity *= maximum_speed / speed; }
    let movement_cells: f32 =
        length(particle.velocity) * substep_delta_time * CELLS_PER_TILE_FLOAT;
    let step_count: u32 = clamp(u32(ceil(movement_cells * 2.0)), 1u,
        parameters.maximum_movement_cells * 2u);
    var position: vec2<f32> = particle.position;
    for (var movement_step: u32 = 0u; movement_step < step_count; movement_step++) {
        position += particle.velocity * substep_delta_time / f32(step_count);
        let resolved: vec2<f32> = project_fluid_particle_out_of_cellular_collision(position);
        particle.prediction_collision_displacement += position - resolved;
        position = resolved;
    }
    particles[particle_index] = particle;
    predicted_positions[particle_index] = position;
}

// Clears support-radius spatial bucket heads before rebuilding linked lists
@compute @workgroup_size(64)
fn clear_fluid_buckets(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if invocation.x < parameters.bucket_count {
        atomicStore(&bucket_heads[invocation.x], INVALID_FLUID_PARTICLE_INDEX);
    }
}

// Inserts committed authoritative particles into support-radius spatial buckets
@compute @workgroup_size(64)
fn insert_committed_fluid_particles_into_spatial_buckets(
    @builtin(global_invocation_id) invocation: vec3<u32>,
) {
    let particle_index: u32 = invocation.x;
    if particle_index >= parameters.particle_capacity ||
            particles[particle_index].material_identifier == EMPTY_MATERIAL_IDENTIFIER { return; }
    let bucket_index: u32 =
        fluid_spatial_bucket_index_from_position(particles[particle_index].position);
    if bucket_index == INVALID_FLUID_BUCKET_INDEX { return; }
    next_particle[particle_index] = atomicExchange(
        &bucket_heads[bucket_index], particle_index,
    );
}

// Builds the same linked-list grid from predicted rather than committed positions
@compute @workgroup_size(64)
fn insert_predicted_fluid_particles_into_spatial_buckets(
    @builtin(global_invocation_id) invocation: vec3<u32>,
) {
    let particle_index: u32 = invocation.x;
    if particle_index >= parameters.particle_capacity ||
            particles[particle_index].material_identifier == EMPTY_MATERIAL_IDENTIFIER { return; }
    if !fluid_position_supports_active_neighbor_buckets(particles[particle_index].position) { return; }
    let bucket_index: u32 =
        fluid_spatial_bucket_index_from_position(predicted_positions[particle_index]);
    if bucket_index == INVALID_FLUID_BUCKET_INDEX { return; }
    next_particle[particle_index] = atomicExchange(
        &bucket_heads[bucket_index], particle_index,
    );
}

// Calculates the standard PBF density constraint and lambda from immutable predicted positions
@compute @workgroup_size(64)
fn calculate_fluid_density_constraint_lambdas(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let particle_index: u32 = invocation.x;
    if particle_index >= parameters.particle_capacity ||
            particles[particle_index].material_identifier == EMPTY_MATERIAL_IDENTIFIER { return; }
    let position: vec2<f32> = predicted_positions[particle_index];
    if !fluid_position_supports_active_lambdas(position) { return; }
    lambdas[particle_index] = calculate_fluid_density_constraint_lambda(
        particle_index,
        position,
        fluid_constraint_properties_from_identifier(
            particles[particle_index].material_identifier,
        ).x,
    );
}

// Gathers correction separately so every lambda and predicted position is immutable during this pass
@compute @workgroup_size(64)
fn calculate_fluid_position_corrections(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let particle_index: u32 = invocation.x;
    if particle_index >= parameters.particle_capacity ||
            particles[particle_index].material_identifier == EMPTY_MATERIAL_IDENTIFIER { return; }
    if particles[particle_index].is_active == 0u { return; }
    let position: vec2<f32> = predicted_positions[particle_index];
    var correction_cells: vec2<f32> = calculate_fluid_particle_position_correction_cells(
        particle_index, position,
    );
    let correction_length: f32 = length(correction_cells);
    if correction_length > MAXIMUM_CORRECTION_CELLS {
        correction_cells *= MAXIMUM_CORRECTION_CELLS / correction_length;
    }
    position_corrections[particle_index] = correction_cells / CELLS_PER_TILE_FLOAT;
}

// Applies race-free corrections and reprojects against the same concrete solid boundary
@compute @workgroup_size(64)
fn apply_fluid_position_corrections(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let particle_index: u32 = invocation.x;
    if particle_index >= parameters.particle_capacity ||
            particles[particle_index].material_identifier == EMPTY_MATERIAL_IDENTIFIER { return; }
    if particles[particle_index].is_active == 0u { return; }
    let corrected: vec2<f32> = predicted_positions[particle_index] +
        position_corrections[particle_index];
    predicted_positions[particle_index] = project_fluid_particle_out_of_cellular_collision(corrected);
}

// Commits the corrected position and reconstructs velocity from the retained authoritative position
@compute @workgroup_size(64)
fn commit_fluid_particles(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let particle_index: u32 = invocation.x;
    if particle_index >= parameters.particle_capacity ||
            particles[particle_index].material_identifier == EMPTY_MATERIAL_IDENTIFIER { return; }
    if particles[particle_index].is_active == 0u { return; }
    let position: vec2<f32> = predicted_positions[particle_index];
    if !fluid_position_is_inside_buffered_region(position) { return; }
    let substep_delta_time: f32 = parameters.delta_time / PBF_SUBSTEP_COUNT;
    var velocity: vec2<f32> = (position + particles[particle_index].prediction_collision_displacement -
        particles[particle_index].position) / substep_delta_time;
    let maximum_speed: f32 = f32(parameters.maximum_movement_cells) /
        (CELLS_PER_TILE_FLOAT * parameters.delta_time);
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
            particles[particle_index].material_identifier == EMPTY_MATERIAL_IDENTIFIER { return; }
    let particle: Particle = particles[particle_index];
    if particle.is_active == 0u { return; }
    position_corrections[particle_index] =
        calculate_fluid_particle_velocity_smoothing(particle_index, particle);
}

// Applies the race-free XSPH correction calculated from committed neighbors
@compute @workgroup_size(64)
fn apply_fluid_velocity_smoothing(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let particle_index: u32 = invocation.x;
    if particle_index >= parameters.particle_capacity ||
            particles[particle_index].material_identifier == EMPTY_MATERIAL_IDENTIFIER { return; }
    if particles[particle_index].is_active == 0u { return; }
    particles[particle_index].velocity += position_corrections[particle_index];
}

// Resolves hard contact and soft swimmer entrainment after final XSPH smoothing
@compute @workgroup_size(64)
fn resolve_fluid_cellular_contact_velocity(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let logical_index: u32 = invocation.x;
    if logical_index >= parameters.buffered_cell_count { return; }
    let cell: vec2<i32> = world_cell_from_fluid_logical_index(logical_index);
    let index: u32 = fluid_physical_cell_index_from_world_cell(cell);
    let material_identifier: u32 = derived_material_identifiers[index];
    let initial_velocity: vec2<f32> = derived_velocity[index].xy;
    if material_identifier == EMPTY_MATERIAL_IDENTIFIER {
        derived_velocity[index] = vec4<f32>(initial_velocity, vec2<f32>(0.0));
        return;
    }
    var velocity: vec2<f32> = resolve_fluid_velocity_against_neighboring_cellular_contacts(
        cell, initial_velocity, material_identifier,
    );
    velocity = apply_fluid_swimmer_velocity_entrainment(
        index, velocity, material_identifier,
    );
    derived_velocity[index] = vec4<f32>(initial_velocity, velocity - initial_velocity);
}

// Resolves one derived fluid cell against neighboring cellular and hard-body faces
fn resolve_fluid_velocity_against_neighboring_cellular_contacts(
    world_cell: vec2<i32>,
    initial_velocity: vec2<f32>,
    material_identifier: u32,
) -> vec2<f32> {
    var resolved_velocity: vec2<f32> = initial_velocity;
    let offsets: array<vec2<i32>, 4> = array<vec2<i32>, 4>(
        vec2<i32>(-1, 0),
        vec2<i32>(1, 0),
        vec2<i32>(0, -1),
        vec2<i32>(0, 1),
    );
    for (var neighbor_number: u32 = 0u; neighbor_number < 4u; neighbor_number++) {
        let offset: vec2<i32> = offsets[neighbor_number];
        let neighbor_index: u32 = fluid_physical_cell_index_from_world_cell(
            world_cell + offset,
        );
        if neighbor_index == INVALID_PHYSICAL_CELL_INDEX ||
                (cellular_material_identifiers[neighbor_index] == EMPTY_MATERIAL_IDENTIFIER &&
                    external_body_occupancy[neighbor_index] != HARD_EXTERNAL_BODY_OCCUPANCY) {
            continue;
        }
        var boundary_velocity: vec2<f32> = select(
            vec2<f32>(0.0),
            external_body_velocity[neighbor_index].xy,
            external_body_occupancy[neighbor_index] == HARD_EXTERNAL_BODY_OCCUPANCY,
        );
        let boundary_speed: f32 = length(boundary_velocity) * CELLS_PER_TILE_FLOAT;
        let body_push_speed: f32 = fluid_constraint_properties_from_identifier(material_identifier).w;
        if boundary_speed > body_push_speed {
            boundary_velocity *= body_push_speed / boundary_speed;
        }
        resolved_velocity = resolve_fluid_velocity_against_cellular_contact(
            resolved_velocity,
            boundary_velocity,
            -vec2<f32>(offset),
            material_identifier,
        );
    }
    return resolved_velocity;
}

// Couples one swimmer proxy cell toward its requested external-body velocity
fn apply_fluid_swimmer_velocity_entrainment(
    physical_cell_index: u32,
    fluid_velocity: vec2<f32>,
    material_identifier: u32,
) -> vec2<f32> {
    if external_body_occupancy[physical_cell_index] != SWIMMER_EXTERNAL_BODY_OCCUPANCY {
        return fluid_velocity;
    }
    let viscosity: f32 = max(
        0.0, fluid_physical_properties_from_identifier(material_identifier).y,
    );
    let coupling: f32 = clamp(1.0 - exp(
        -viscosity * derived_coverage[physical_cell_index] * parameters.delta_time,
    ), 0.0, 1.0);
    return fluid_velocity +
        (external_body_velocity[physical_cell_index].xy - fluid_velocity) * coupling;
}

// Each authoritative particle consumes at most its center cell's one bulk correction
@compute @workgroup_size(64)
fn apply_fluid_cellular_contact_velocity_to_particles(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let particle_index: u32 = invocation.x;
    if particle_index >= parameters.particle_capacity ||
            particles[particle_index].material_identifier == EMPTY_MATERIAL_IDENTIFIER { return; }
    if particles[particle_index].is_active == 0u { return; }
    let cell: vec2<i32> = vec2<i32>(floor(
        particles[particle_index].position * CELLS_PER_TILE_FLOAT,
    ));
    let index: u32 = fluid_physical_cell_index_from_world_cell(cell);
    if index == INVALID_PHYSICAL_CELL_INDEX || derived_material_identifiers[index] !=
            particles[particle_index].material_identifier { return; }
    particles[particle_index].velocity += derived_velocity[index].zw;
}

// Transfers exact authoritative records out before their world tiles leave residency
@compute @workgroup_size(64)
fn export_fluid_particles(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let particle_index: u32 = invocation.x;
    if particle_index >= parameters.particle_capacity ||
            particles[particle_index].material_identifier == EMPTY_MATERIAL_IDENTIFIER { return; }
    let tile: vec2<i32> = vec2<i32>(floor(particles[particle_index].position));
    let relative: vec2<i32> = tile - parameters.streaming_origin;
    if any(relative < vec2<i32>(0)) || relative.x >= i32(parameters.streaming_tile_size.x) ||
            relative.y >= i32(parameters.streaming_tile_size.y) { return; }
    let output_index: u32 = atomicAdd(&streaming_count[0], 1u);
    streaming_particles[output_index] = particles[particle_index];
    release_fluid_particle_index(particle_index);
}

// Reclaims authoritative GPU slots and records each claim result for asynchronous ownership transfer
@compute @workgroup_size(64)
fn import_fluid_particles(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let input_index: u32 = invocation.x;
    if input_index >= atomicLoad(&streaming_count[0]) { return; }
    let particle_index: u32 = claim_free_fluid_particle_index();
    if particle_index == INVALID_FLUID_PARTICLE_INDEX {
        streaming_results[input_index] = 0u;
        return;
    }
    particles[particle_index] = streaming_particles[input_index];
    streaming_results[input_index] = 1u;
}

// Each cell gathers its own result, so particles never contend for derived-cell writes
@compute @workgroup_size(64)
fn rasterize_fluid_particle_coverage_into_cells(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let logical_index: u32 = invocation.x;
    if logical_index >= parameters.buffered_cell_count { return; }
    let cell: vec2<i32> = world_cell_from_fluid_logical_index(logical_index);
    let physical_index: u32 = fluid_physical_cell_index_from_world_cell(cell);
    let center: vec2<f32> = vec2<f32>(cell) + vec2<f32>(0.5);
    let sample: DerivedFluidCellSample = gather_fluid_particle_sample_for_cell(center);
    derived_material_identifiers[physical_index] = sample.material_identifier;
    derived_coverage[physical_index] = sample.coverage;
    derived_velocity[physical_index] = vec4<f32>(sample.velocity, 0.0, 0.0);
}

// Gathers nearby authoritative particles for one derived cellular sample
fn gather_fluid_particle_sample_for_cell(center: vec2<f32>) -> DerivedFluidCellSample {
    let base_bucket_coordinates: vec2<i32> = fluid_spatial_bucket_coordinates_from_position(
        center / CELLS_PER_TILE_FLOAT,
    );
    var weight_sum: f32 = 0.0;
    var velocity_sum: vec2<f32> = vec2<f32>(0.0);
    var strongest_weight: f32 = 0.0;
    var material_identifier: u32 = EMPTY_MATERIAL_IDENTIFIER;
    for (var bucket_y: i32 = -1; bucket_y <= 1; bucket_y++) {
        for (var bucket_x: i32 = -1; bucket_x <= 1; bucket_x++) {
            let bucket_index: u32 = fluid_spatial_bucket_index_from_coordinates(
                base_bucket_coordinates + vec2<i32>(bucket_x, bucket_y),
            );
            if bucket_index == INVALID_FLUID_BUCKET_INDEX { continue; }
            var particle_index: u32 = atomicLoad(&bucket_heads[bucket_index]);
            for (var chain_length: u32 = 0u;
                    particle_index != INVALID_FLUID_PARTICLE_INDEX &&
                        chain_length < parameters.particle_capacity;
                    chain_length++) {
                let particle: Particle = particles[particle_index];
                let distance_cells: f32 = length(
                    particle.position * CELLS_PER_TILE_FLOAT - center,
                );
                // Support radius is for PBF; one particle renders as about one cell
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
    return DerivedFluidCellSample(
        material_identifier,
        min(weight_sum, 1.0),
        select(
            vec2<f32>(0.0), velocity_sum / max(weight_sum, 0.000001), weight_sum > 0.0,
        ),
    );
}

// Reduces the possessed pawn capsule against the final derived fluid representation
@compute @workgroup_size(1)
fn sample_fluid_state_inside_pawn_capsule(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if invocation.x != 0u { return; }
    let radius: f32 = min(parameters.sample_collider.x, parameters.sample_collider.y) * 0.5;
    let half_segment: f32 = max(0.0, (parameters.sample_collider.y - radius * 2.0) * 0.5);
    let capsule: GravityRelativeCapsule = gravity_relative_capsule_from_parameters(
        parameters.sample_center, parameters.gravity, radius, half_segment,
    );
    let extent: vec2<f32> = gravity_relative_capsule_world_extent(capsule);
    let minimum: vec2<i32> = vec2<i32>(floor(
        (parameters.sample_center - extent) * CELLS_PER_TILE_FLOAT,
    ));
    let maximum: vec2<i32> = vec2<i32>(floor(
        (parameters.sample_center + extent) * CELLS_PER_TILE_FLOAT,
    ));
    let sample: DerivedFluidCapsuleSample = gather_derived_fluid_sample_inside_capsule(
        capsule, minimum, maximum,
    );
    let divisor: f32 = max(sample.coverage_sum, 0.000001);
    sample_output[0] = vec4<f32>(
        sample.coverage_sum / max(sample.capsule_cell_count, 1.0),
        sample.velocity_sum.x / divisor,
        sample.velocity_sum.y / divisor,
        sample.density_sum / divisor,
    );
    sample_output[1] = vec4<f32>(sample.viscosity_sum / divisor, 0.0, 0.0, 0.0);
}

// Gathers derived coverage and physical properties beneath one pawn capsule
fn gather_derived_fluid_sample_inside_capsule(
    capsule: GravityRelativeCapsule,
    minimum: vec2<i32>,
    maximum: vec2<i32>,
) -> DerivedFluidCapsuleSample {
    var capsule_cell_count: f32 = 0.0;
    var coverage_sum: f32 = 0.0;
    var velocity_sum: vec2<f32> = vec2<f32>(0.0);
    var density_sum: f32 = 0.0;
    var viscosity_sum: f32 = 0.0;
    for (var y: i32 = minimum.y; y <= maximum.y; y++) {
        for (var x: i32 = minimum.x; x <= maximum.x; x++) {
            let cell: vec2<i32> = vec2<i32>(x, y);
            let world_position: vec2<f32> =
                (vec2<f32>(cell) + vec2<f32>(0.5)) / CELLS_PER_TILE_FLOAT;
            if !world_position_is_inside_gravity_relative_capsule(
                    world_position, capsule) {
                continue;
            }
            capsule_cell_count += 1.0;
            let index: u32 = fluid_physical_cell_index_from_world_cell(cell);
            if index == INVALID_PHYSICAL_CELL_INDEX ||
                    derived_material_identifiers[index] == EMPTY_MATERIAL_IDENTIFIER {
                continue;
            }
            let coverage: f32 = clamp(derived_coverage[index], 0.0, 1.0);
            let properties: vec2<f32> = fluid_physical_properties_from_identifier(
                derived_material_identifiers[index],
            );
            coverage_sum += coverage;
            velocity_sum += derived_velocity[index].xy * coverage;
            density_sum += properties.x * coverage;
            viscosity_sum += properties.y * coverage;
        }
    }
    return DerivedFluidCapsuleSample(
        capsule_cell_count,
        coverage_sum,
        velocity_sum,
        density_sum,
        viscosity_sum,
    );
}

// Calculates one XSPH velocity correction from committed neighbors
fn calculate_fluid_particle_velocity_smoothing(
    particle_index: u32,
    particle: Particle,
) -> vec2<f32> {
    let base_bucket_coordinates: vec2<i32> =
        fluid_spatial_bucket_coordinates_from_position(particle.position);
    var difference_sum: vec2<f32> = vec2<f32>(0.0);
    var weight_sum: f32 = 0.0;
    for (var bucket_y: i32 = -1; bucket_y <= 1; bucket_y++) {
        for (var bucket_x: i32 = -1; bucket_x <= 1; bucket_x++) {
            let bucket_index: u32 = fluid_spatial_bucket_index_from_coordinates(
                base_bucket_coordinates + vec2<i32>(bucket_x, bucket_y),
            );
            if bucket_index == INVALID_FLUID_BUCKET_INDEX { continue; }
            var neighbor_index: u32 = atomicLoad(&bucket_heads[bucket_index]);
            for (var chain_length: u32 = 0u;
                    neighbor_index != INVALID_FLUID_PARTICLE_INDEX &&
                        chain_length < parameters.particle_capacity;
                    chain_length++) {
                if neighbor_index != particle_index {
                    let distance: f32 = length(
                        (particle.position - particles[neighbor_index].position) *
                            CELLS_PER_TILE_FLOAT,
                    );
                    let weight: f32 = fluid_poly6_kernel_weight(distance);
                    difference_sum +=
                        (particles[neighbor_index].velocity - particle.velocity) * weight;
                    weight_sum += weight;
                }
                neighbor_index = next_particle[neighbor_index];
            }
        }
    }
    return fluid_constraint_properties_from_identifier(particle.material_identifier).z *
        difference_sum / max(weight_sum, 0.000001);
}

// Calculates one PBF density constraint multiplier from predicted neighbors
fn calculate_fluid_density_constraint_lambda(
    particle_index: u32,
    position: vec2<f32>,
    rest_density: f32,
) -> f32 {
    let base_bucket_coordinates: vec2<i32> =
        fluid_spatial_bucket_coordinates_from_position(position);
    var density: f32 = fluid_poly6_kernel_weight(0.0);
    var self_gradient: vec2<f32> = vec2<f32>(0.0);
    var gradient_squared_sum: f32 = 0.0;
    for (var bucket_y: i32 = -1; bucket_y <= 1; bucket_y++) {
        for (var bucket_x: i32 = -1; bucket_x <= 1; bucket_x++) {
            let bucket_index: u32 = fluid_spatial_bucket_index_from_coordinates(
                base_bucket_coordinates + vec2<i32>(bucket_x, bucket_y),
            );
            if bucket_index == INVALID_FLUID_BUCKET_INDEX { continue; }
            var neighbor_index: u32 = atomicLoad(&bucket_heads[bucket_index]);
            for (var chain_length: u32 = 0u;
                    neighbor_index != INVALID_FLUID_PARTICLE_INDEX &&
                        chain_length < parameters.particle_capacity;
                    chain_length++) {
                if neighbor_index != particle_index {
                    let separation: vec2<f32> =
                        (position - predicted_positions[neighbor_index]) *
                            CELLS_PER_TILE_FLOAT;
                    let distance: f32 = length(separation);
                    if distance < parameters.support_radius_cells {
                        density += fluid_poly6_kernel_weight(distance);
                        let gradient: vec2<f32> = fluid_spiky_kernel_gradient(
                            separation, distance,
                        ) / rest_density;
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
    return -constraint / (gradient_squared_sum + CONSTRAINT_EPSILON);
}

// Gathers one race-free PBF position correction from immutable neighbor state
fn calculate_fluid_particle_position_correction_cells(
    particle_index: u32,
    position: vec2<f32>,
) -> vec2<f32> {
    let material_identifier: u32 = particles[particle_index].material_identifier;
    let rest_density: f32 =
        fluid_constraint_properties_from_identifier(material_identifier).x;
    let base_bucket_coordinates: vec2<i32> =
        fluid_spatial_bucket_coordinates_from_position(position);
    var correction_cells: vec2<f32> = vec2<f32>(0.0);
    for (var bucket_y: i32 = -1; bucket_y <= 1; bucket_y++) {
        for (var bucket_x: i32 = -1; bucket_x <= 1; bucket_x++) {
            let bucket_index: u32 = fluid_spatial_bucket_index_from_coordinates(
                base_bucket_coordinates + vec2<i32>(bucket_x, bucket_y),
            );
            if bucket_index == INVALID_FLUID_BUCKET_INDEX { continue; }
            var neighbor_index: u32 = atomicLoad(&bucket_heads[bucket_index]);
            for (var chain_length: u32 = 0u;
                    neighbor_index != INVALID_FLUID_PARTICLE_INDEX &&
                        chain_length < parameters.particle_capacity;
                    chain_length++) {
                if neighbor_index != particle_index {
                    let separation: vec2<f32> =
                        (position - predicted_positions[neighbor_index]) *
                            CELLS_PER_TILE_FLOAT;
                    let distance: f32 = length(separation);
                    if distance > 0.000001 && distance < parameters.support_radius_cells {
                        correction_cells += (
                            lambdas[particle_index] + lambdas[neighbor_index] +
                            calculate_fluid_artificial_pressure(distance, material_identifier)
                        ) * fluid_spiky_kernel_gradient(separation, distance) / rest_density;
                    }
                }
                neighbor_index = next_particle[neighbor_index];
            }
        }
    }
    return correction_cells;
}

// Evaluates the two-dimensional PBF poly6 density kernel
fn fluid_poly6_kernel_weight(distance: f32) -> f32 {
    let h: f32 = parameters.support_radius_cells;
    if distance >= h { return 0.0; }
    let difference: f32 = h * h - distance * distance;
    return 4.0 * difference * difference * difference /
        (PI * h * h * h * h * h * h * h * h);
}

// Evaluates the two-dimensional PBF spiky-kernel gradient
fn fluid_spiky_kernel_gradient(separation: vec2<f32>, distance: f32) -> vec2<f32> {
    let h: f32 = parameters.support_radius_cells;
    if distance <= 0.000001 || distance >= h { return vec2<f32>(0.0); }
    let remaining: f32 = h - distance;
    return -30.0 * remaining * remaining / (PI * h * h * h * h * h) *
        separation / distance;
}

// Calculates tensile-instability correction for one neighboring particle
fn calculate_fluid_artificial_pressure(distance: f32, material_identifier: u32) -> f32 {
    let reference: f32 = fluid_poly6_kernel_weight(
        ARTIFICIAL_PRESSURE_DELTA_Q_RATIO * parameters.support_radius_cells,
    );
    let ratio: f32 = fluid_poly6_kernel_weight(distance) / max(reference, 0.000001);
    let squared: f32 = ratio * ratio;
    return -fluid_constraint_properties_from_identifier(material_identifier).y * squared * squared;
}

// Reads density, artificial pressure, smoothing, and body-push properties
fn fluid_constraint_properties_from_identifier(material_identifier: u32) -> vec4<f32> {
    return fluid_material_properties[material_index_from_identifier(material_identifier) * 2u];
}

// Reads friction and restitution for fluid-cellular contact
fn fluid_contact_properties_from_identifier(material_identifier: u32) -> vec2<f32> {
    return fluid_material_properties[
        material_index_from_identifier(material_identifier) * 2u + 1u
    ].xy;
}

// Reads density and viscosity for derived fluid interaction
fn fluid_physical_properties_from_identifier(material_identifier: u32) -> vec2<f32> {
    return fluid_material_properties[
        material_index_from_identifier(material_identifier) * 2u + 1u
    ].zw;
}

// Projects a fluid particle out of cellular or hard-body geometry
fn project_fluid_particle_out_of_cellular_collision(initial_position: vec2<f32>) -> vec2<f32> {
    var position: vec2<f32> = initial_position;
    let radius: f32 = parameters.particle_radius_cells / CELLS_PER_TILE_FLOAT;
    for (var iteration: u32 = 0u; iteration < 2u; iteration++) {
        let center_cell: vec2<i32> = vec2<i32>(floor(position * CELLS_PER_TILE_FLOAT));
        var resolved: bool = false;
        for (var offset_y: i32 = -1; offset_y <= 1 && !resolved; offset_y++) {
            for (var offset_x: i32 = -1; offset_x <= 1; offset_x++) {
                let cell: vec2<i32> = center_cell + vec2<i32>(offset_x, offset_y);
                let index: u32 = fluid_physical_cell_index_from_world_cell(cell);
                if index == INVALID_PHYSICAL_CELL_INDEX ||
                        (cellular_material_identifiers[index] == EMPTY_MATERIAL_IDENTIFIER &&
                            external_body_occupancy[index] != HARD_EXTERNAL_BODY_OCCUPANCY) {
                    continue;
                }
                let minimum: vec2<f32> = vec2<f32>(cell) / CELLS_PER_TILE_FLOAT;
                let maximum: vec2<f32> =
                    vec2<f32>(cell + vec2<i32>(1)) / CELLS_PER_TILE_FLOAT;
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
                resolved = true;
                break;
            }
        }
        if !resolved { break; }
    }
    return position;
}

// Resolves normal restitution and tangential friction against one boundary
fn resolve_fluid_velocity_against_cellular_contact(
    velocity: vec2<f32>,
    boundary_velocity: vec2<f32>,
    normal: vec2<f32>,
    material_identifier: u32,
) -> vec2<f32> {
    let properties: vec2<f32> = clamp(
        fluid_contact_properties_from_identifier(material_identifier),
        vec2<f32>(0.0),
        vec2<f32>(1.0),
    );
    var relative_velocity: vec2<f32> = velocity - boundary_velocity;
    let inward_speed: f32 = dot(relative_velocity, normal);
    if inward_speed < 0.0 {
        relative_velocity -= normal * inward_speed * (1.0 + properties.y);
    }
    let normal_velocity: vec2<f32> = normal * dot(relative_velocity, normal);
    relative_velocity -= (relative_velocity - normal_velocity) * properties.x;
    return boundary_velocity + relative_velocity;
}

// Atomically pops one reusable authoritative fluid-particle slot
fn claim_free_fluid_particle_index() -> u32 {
    var available: u32 = atomicLoad(&free_count[0]);
    loop {
        if available == 0u { return INVALID_FLUID_PARTICLE_INDEX; }
        let result = atomicCompareExchangeWeak(&free_count[0], available, available - 1u);
        if result.exchanged { return free_indices[available - 1u]; }
        available = result.old_value;
    }
    return INVALID_FLUID_PARTICLE_INDEX;
}

// Atomically returns one authoritative fluid-particle slot to the free stack
fn release_fluid_particle_index(particle_index: u32) {
    particles[particle_index].material_identifier = EMPTY_MATERIAL_IDENTIFIER;
    let free_index: u32 = atomicAdd(&free_count[0], 1u);
    free_indices[free_index] = particle_index;
}

// Tests continuous tile-space position against buffered residency
fn fluid_position_is_inside_buffered_region(position: vec2<f32>) -> bool {
    let relative: vec2<f32> = position - vec2<f32>(parameters.buffered_origin);
    return all(relative >= vec2<f32>(0.0)) &&
        relative.x < f32(parameters.buffered_tile_size.x) &&
        relative.y < f32(parameters.buffered_tile_size.y);
}

// Tests continuous tile-space position against active simulation bounds
fn fluid_position_is_inside_active_region(position: vec2<f32>) -> bool {
    let relative: vec2<f32> = position - vec2<f32>(parameters.active_origin);
    return all(relative >= vec2<f32>(0.0)) &&
        relative.x < f32(parameters.active_tile_size.x) &&
        relative.y < f32(parameters.active_tile_size.y);
}

// Includes neighbors needed by active particles and their lambda neighbors
fn fluid_position_supports_active_neighbor_buckets(position: vec2<f32>) -> bool {
    let predicted_movement_cells: f32 = f32(parameters.maximum_movement_cells) /
        PBF_SUBSTEP_COUNT + MAXIMUM_CORRECTION_CELLS * PBF_CONSTRAINT_ITERATION_COUNT;
    return fluid_position_is_inside_active_padding(
        (parameters.support_radius_cells * 2.0 + predicted_movement_cells) /
            CELLS_PER_TILE_FLOAT,
        position,
    );
}

// Includes particles whose lambdas directly support the active fluid region
fn fluid_position_supports_active_lambdas(position: vec2<f32>) -> bool {
    let predicted_movement_cells: f32 = f32(parameters.maximum_movement_cells) /
        PBF_SUBSTEP_COUNT + MAXIMUM_CORRECTION_CELLS * PBF_CONSTRAINT_ITERATION_COUNT;
    return fluid_position_is_inside_active_padding(
        (parameters.support_radius_cells + predicted_movement_cells) / CELLS_PER_TILE_FLOAT,
        position,
    );
}

// Tests continuous tile-space position against padded active bounds
fn fluid_position_is_inside_active_padding(padding: f32, position: vec2<f32>) -> bool {
    let minimum: vec2<f32> = vec2<f32>(parameters.active_origin) - vec2<f32>(padding);
    let maximum: vec2<f32> = vec2<f32>(parameters.active_origin) +
        vec2<f32>(parameters.active_tile_size) + vec2<f32>(padding);
    return all(position >= minimum) && all(position < maximum);
}

// Converts continuous tile-space position into fluid spatial-bucket coordinates
fn fluid_spatial_bucket_coordinates_from_position(position: vec2<f32>) -> vec2<i32> {
    let origin: vec2<f32> = vec2<f32>(parameters.buffered_origin);
    let bucket_size: f32 = parameters.support_radius_cells / CELLS_PER_TILE_FLOAT;
    return vec2<i32>(floor((position - origin) / bucket_size));
}

// Converts continuous tile-space position into a fluid spatial-bucket index
fn fluid_spatial_bucket_index_from_position(position: vec2<f32>) -> u32 {
    return fluid_spatial_bucket_index_from_coordinates(
        fluid_spatial_bucket_coordinates_from_position(position),
    );
}

// Converts bounded fluid bucket coordinates into row-major storage
fn fluid_spatial_bucket_index_from_coordinates(bucket_coordinates: vec2<i32>) -> u32 {
    if any(bucket_coordinates < vec2<i32>(0)) ||
            bucket_coordinates.x >= i32(parameters.bucket_dimensions.x) ||
            bucket_coordinates.y >= i32(parameters.bucket_dimensions.y) {
        return INVALID_FLUID_BUCKET_INDEX;
    }
    return u32(bucket_coordinates.y) * parameters.bucket_dimensions.x +
        u32(bucket_coordinates.x);
}

// Maps a world cell through the fluid solver's current physical tile ring
fn fluid_physical_cell_index_from_world_cell(world_cell: vec2<i32>) -> u32 {
    return physical_cell_index_from_world_cell(
        world_cell, parameters.buffered_origin, parameters.buffered_tile_size,
        parameters.ring_offset,
    );
}

// Converts fluid cell dispatch order into a signed buffered world cell
fn world_cell_from_fluid_logical_index(logical_index: u32) -> vec2<i32> {
    return world_cell_from_logical_tile_major_index(
        logical_index, parameters.buffered_origin, parameters.buffered_tile_size,
    );
}
