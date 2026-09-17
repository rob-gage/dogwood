fn is_hard_external_body(occupancy: u32) -> bool {
    return occupancy == HARD_EXTERNAL_BODY_OCCUPANCY || occupancy == RIGID_EXTERNAL_BODY_OCCUPANCY;
}

// Removes every authoritative particle whose current world cell was edited
@compute @workgroup_size(64)
fn remove_edited_fluid_particles(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let particle_index: u32 = invocation.x;
    if
        particle_index >= parameters.particle_capacity
            || particles[particle_index].material_identifier == EMPTY_MATERIAL_IDENTIFIER
    {
        return;
    }
    let cell: vec2<i32> =
        vec2<i32>(floor(particles[particle_index].position * CELLS_PER_TILE_FLOAT));
    let cell_index: u32 = fluid_physical_cell_index_from_world_cell(cell);
    if
        cell_index == INVALID_PHYSICAL_CELL_INDEX || edit_cells[cell_index] == EMPTY_MATERIAL_IDENTIFIER
    {
        return;
    }
    release_fluid_particle_index(particle_index);
}

// Creates at most one particle at each edited world-cell center
@compute @workgroup_size(64)
fn spawn_edited_fluid_particles(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let logical_index: u32 = invocation.x;
    if logical_index >= parameters.buffered_cell_count {
        return;
    }
    let cell: vec2<i32> = world_cell_from_fluid_logical_index(logical_index);
    let cell_index: u32 = fluid_physical_cell_index_from_world_cell(cell);
    let material_identifier: u32 = edit_cells[cell_index];
    if
        material_form_from_identifier(
            material_identifier,
        ) != FLUID_MATERIAL_FORM || cellular_material_identifiers[cell_index] != EMPTY_MATERIAL_IDENTIFIER
    {
        return;
    }
    let particle_index: u32 = claim_free_fluid_particle_index();
    if particle_index == INVALID_FLUID_PARTICLE_INDEX {
        return;
    }
    particles[particle_index] =
        Particle(
            material_identifier,
            0u,
            (vec2<f32>(cell) + vec2<f32>(0.5)) / CELLS_PER_TILE_FLOAT,
            vec2<f32>(0.0),
            vec2<f32>(0.0),
            edit_amounts[cell_index],
            edit_temperatures[cell_index],
        );
}

// Clears transient fluid edit requests after remove and spawn passes consume them
@compute @workgroup_size(64)
fn clear_fluid_edits(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if invocation.x < parameters.buffered_cell_count {
        edit_cells[invocation.x] = EMPTY_MATERIAL_IDENTIFIER;
        edit_amounts[invocation.x] = 0.0;
        edit_temperatures[invocation.x] = 0.0;
    }
}

// Produces zero-work indirect records unless another Accelerator subsystem wrote a fluid edit.
@compute @workgroup_size(1)
fn prepare_accelerator_fluid_edits(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if invocation.x != 0u {
        return;
    }
    let pending: u32 = atomicExchange(&accelerator_edits_pending[0], 0u);
    let particle_workgroups: u32 =
        select(0u, (parameters.particle_capacity + 63u) / 64u, pending != 0u);
    let cell_workgroups: u32 =
        select(0u, (parameters.buffered_cell_count + 63u) / 64u, pending != 0u);
    let bucket_workgroups: u32 = select(0u, (parameters.bucket_count + 63u) / 64u, pending != 0u);
    accelerator_edit_dispatch[0] = particle_workgroups;
    accelerator_edit_dispatch[1] = 1u;
    accelerator_edit_dispatch[2] = 1u;
    accelerator_edit_dispatch[3] = cell_workgroups;
    accelerator_edit_dispatch[4] = 1u;
    accelerator_edit_dispatch[5] = 1u;
    accelerator_edit_dispatch[6] = cell_workgroups;
    accelerator_edit_dispatch[7] = 1u;
    accelerator_edit_dispatch[8] = 1u;
    accelerator_edit_dispatch[9] = bucket_workgroups;
    accelerator_edit_dispatch[10] = 1u;
    accelerator_edit_dispatch[11] = 1u;
    accelerator_edit_dispatch[12] = particle_workgroups;
    accelerator_edit_dispatch[13] = 1u;
    accelerator_edit_dispatch[14] = 1u;
    accelerator_edit_dispatch[15] = cell_workgroups;
    accelerator_edit_dispatch[16] = 1u;
    accelerator_edit_dispatch[17] = 1u;
}

// Freezes active-area membership for all substeps in this fixed tick
@compute @workgroup_size(64)
fn classify_active_fluid_particles(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let particle_index: u32 = invocation.x;
    if
        particle_index >= parameters.particle_capacity
            || particles[particle_index].material_identifier == EMPTY_MATERIAL_IDENTIFIER
    {
        return;
    }
    particles[particle_index].is_active =
        select(0u, 1u, fluid_position_is_inside_active_region(particles[particle_index].position));
}

// Predicts one particle-fluid substep while retaining the previous authoritative position for velocity reconstruction
@compute @workgroup_size(64)
fn predict_fluid_particles(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let particle_index: u32 = invocation.x;
    if
        particle_index >= parameters.particle_capacity
            || particles[particle_index].material_identifier == EMPTY_MATERIAL_IDENTIFIER
    {
        return;
    }
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
    let maximum_speed: f32 =
        f32(parameters.maximum_movement_cells) / (CELLS_PER_TILE_FLOAT * parameters.delta_time);
    let speed: f32 = length(particle.velocity);
    if speed > maximum_speed {
        particle.velocity *= maximum_speed / speed;
    }
    let movement_cells: f32 = length(particle.velocity) * substep_delta_time * CELLS_PER_TILE_FLOAT;
    let step_count: u32 =
        clamp(u32(ceil(movement_cells * 2.0)), 1u, parameters.maximum_movement_cells * 2u);
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
    if
        particle_index >= parameters.particle_capacity
            || particles[particle_index].material_identifier == EMPTY_MATERIAL_IDENTIFIER
    {
        return;
    }
    let bucket_index: u32 =
        fluid_spatial_bucket_index_from_position(particles[particle_index].position);
    if bucket_index == INVALID_FLUID_BUCKET_INDEX {
        return;
    }
    next_particle[particle_index] = atomicExchange(&bucket_heads[bucket_index], particle_index);
}

// Builds the same linked-list grid from predicted rather than committed positions
@compute @workgroup_size(64)
fn insert_predicted_fluid_particles_into_spatial_buckets(
    @builtin(global_invocation_id) invocation: vec3<u32>,
) {
    let particle_index: u32 = invocation.x;
    if
        particle_index >= parameters.particle_capacity
            || particles[particle_index].material_identifier == EMPTY_MATERIAL_IDENTIFIER
    {
        return;
    }
    if !fluid_position_supports_active_neighbor_buckets(particles[particle_index].position) {
        return;
    }
    let bucket_index: u32 =
        fluid_spatial_bucket_index_from_position(predicted_positions[particle_index]);
    if bucket_index == INVALID_FLUID_BUCKET_INDEX {
        return;
    }
    next_particle[particle_index] = atomicExchange(&bucket_heads[bucket_index], particle_index);
}

// Calculates the standard particle-fluid density constraint and multiplier from immutable predicted positions
@compute @workgroup_size(64)
fn calculate_fluid_density_constraint_lambdas(
    @builtin(global_invocation_id) invocation: vec3<u32>,
) {
    let particle_index: u32 = invocation.x;
    if
        particle_index >= parameters.particle_capacity
            || particles[particle_index].material_identifier == EMPTY_MATERIAL_IDENTIFIER
    {
        return;
    }
    let position: vec2<f32> = predicted_positions[particle_index];
    if !fluid_position_supports_active_lambdas(position) {
        return;
    }
    lambdas[particle_index] =
        calculate_fluid_density_constraint_lambda(
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
    if
        particle_index >= parameters.particle_capacity
            || particles[particle_index].material_identifier == EMPTY_MATERIAL_IDENTIFIER
    {
        return;
    }
    if particles[particle_index].is_active == 0u {
        return;
    }
    let position: vec2<f32> = predicted_positions[particle_index];
    var correction_cells: vec2<f32> =
        calculate_fluid_particle_position_correction_cells(particle_index, position);
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
    if
        particle_index >= parameters.particle_capacity
            || particles[particle_index].material_identifier == EMPTY_MATERIAL_IDENTIFIER
    {
        return;
    }
    if particles[particle_index].is_active == 0u {
        return;
    }
    let corrected: vec2<f32> =
        predicted_positions[particle_index] + position_corrections[particle_index];
    predicted_positions[particle_index] =
        project_fluid_particle_out_of_cellular_collision(corrected);
}

// Commits the corrected position and reconstructs velocity from the retained authoritative position
@compute @workgroup_size(64)
fn commit_fluid_particles(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let particle_index: u32 = invocation.x;
    if
        particle_index >= parameters.particle_capacity
            || particles[particle_index].material_identifier == EMPTY_MATERIAL_IDENTIFIER
    {
        return;
    }
    if particles[particle_index].is_active == 0u {
        return;
    }
    let position: vec2<f32> = predicted_positions[particle_index];
    if !fluid_position_is_inside_buffered_region(position) {
        return;
    }
    let substep_delta_time: f32 = parameters.delta_time / PBF_SUBSTEP_COUNT;
    var velocity: vec2<f32> =
        (position
            + particles[particle_index].prediction_collision_displacement
            - particles[particle_index].position) / substep_delta_time;
    let maximum_speed: f32 =
        f32(parameters.maximum_movement_cells) / (CELLS_PER_TILE_FLOAT * parameters.delta_time);
    let speed: f32 = length(velocity);
    if speed > maximum_speed {
        velocity *= maximum_speed / speed;
    }
    particles[particle_index].position = position;
    particles[particle_index].velocity = velocity;
}

// Calculates one final neighbor-velocity-smoothing correction from committed neighbors to quiet constraint noise
@compute @workgroup_size(64)
fn calculate_fluid_velocity_smoothing(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let particle_index: u32 = invocation.x;
    if
        particle_index >= parameters.particle_capacity
            || particles[particle_index].material_identifier == EMPTY_MATERIAL_IDENTIFIER
    {
        return;
    }
    let particle: Particle = particles[particle_index];
    if particle.is_active == 0u {
        return;
    }
    position_corrections[particle_index] =
        calculate_fluid_particle_velocity_smoothing(particle_index, particle);
}

// Applies the race-free neighbor-velocity-smoothing correction calculated from committed neighbors
@compute @workgroup_size(64)
fn apply_fluid_velocity_smoothing(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let particle_index: u32 = invocation.x;
    if
        particle_index >= parameters.particle_capacity
            || particles[particle_index].material_identifier == EMPTY_MATERIAL_IDENTIFIER
    {
        return;
    }
    if particles[particle_index].is_active == 0u {
        return;
    }
    particles[particle_index].velocity += position_corrections[particle_index];
}

// Resolves hard contact and soft swimmer entrainment after final neighbor velocity smoothing
@compute @workgroup_size(64)
fn resolve_fluid_cellular_contact_velocity(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let logical_index: u32 = invocation.x;
    if logical_index >= parameters.buffered_cell_count {
        return;
    }
    let cell: vec2<i32> = world_cell_from_fluid_logical_index(logical_index);
    let index: u32 = fluid_physical_cell_index_from_world_cell(cell);
    let material_identifier: u32 = derived_material_identifiers[index];
    let initial_velocity: vec2<f32> = derived_velocity[index].xy;
    if material_identifier == EMPTY_MATERIAL_IDENTIFIER {
        derived_velocity[index] = vec4<f32>(initial_velocity, vec2<f32>(0.0));
        return;
    }
    let velocity: vec2<f32> =
        apply_fluid_swimmer_velocity_entrainment(index, initial_velocity, material_identifier);
    derived_velocity[index] = vec4<f32>(initial_velocity, velocity - initial_velocity);
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
    let viscosity: f32 = max(0.0, fluid_physical_properties_from_identifier(material_identifier).y);
    let coupling: f32 =
        clamp(
            1.0 - exp(-viscosity * derived_coverage[physical_cell_index] * parameters.delta_time),
            0.0,
            1.0,
        );
    return
        fluid_velocity + (external_body_velocity[physical_cell_index].xy - fluid_velocity) * coupling;
}

// Each authoritative particle consumes at most its center cell's one bulk correction
@compute @workgroup_size(64)
fn apply_fluid_cellular_contact_velocity_to_particles(
    @builtin(global_invocation_id) invocation: vec3<u32>,
) {
    let particle_index: u32 = invocation.x;
    if
        particle_index >= parameters.particle_capacity
            || particles[particle_index].material_identifier == EMPTY_MATERIAL_IDENTIFIER
    {
        return;
    }
    if particles[particle_index].is_active == 0u {
        return;
    }
    let cell: vec2<i32> =
        vec2<i32>(floor(particles[particle_index].position * CELLS_PER_TILE_FLOAT));
    let index: u32 = fluid_physical_cell_index_from_world_cell(cell);
    if
        index == INVALID_PHYSICAL_CELL_INDEX
            || derived_material_identifiers[index] != particles[particle_index].material_identifier
    {
        return;
    }
    particles[particle_index].velocity += derived_velocity[index].zw;
}

// Transfers exact authoritative records out before their world tiles leave residency
@compute @workgroup_size(64)
