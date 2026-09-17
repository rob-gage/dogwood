fn export_fluid_particles(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let particle_index: u32 = invocation.x;
    if
        particle_index >= parameters.particle_capacity
            || particles[particle_index].material_identifier == EMPTY_MATERIAL_IDENTIFIER
    {
        return;
    }
    let tile: vec2<i32> = vec2<i32>(floor(particles[particle_index].position));
    let relative: vec2<i32> = tile - parameters.streaming_origin;
    if
        any(relative < vec2<i32>(0))
            || relative.x >= i32(parameters.streaming_tile_size.x)
            || relative.y >= i32(parameters.streaming_tile_size.y)
    {
        return;
    }
    let output_index: u32 = atomicAdd(&streaming_count[0], 1u);
    if output_index >= arrayLength(&streaming_particles) {
        return;
    }
    streaming_particles[output_index] = particles[particle_index];
    release_fluid_particle_index(particle_index);
}

// Reclaims authoritative Accelerator slots and records each claim result for asynchronous ownership transfer
@compute @workgroup_size(64)
fn import_fluid_particles(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let input_index: u32 = invocation.x;
    if input_index >= atomicLoad(&streaming_count[0]) {
        return;
    }
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
fn rasterize_fluid_particle_coverage_into_cells(
    @builtin(global_invocation_id) invocation: vec3<u32>,
) {
    let logical_index: u32 = invocation.x;
    if logical_index >= parameters.buffered_cell_count {
        return;
    }
    let cell: vec2<i32> = world_cell_from_fluid_logical_index(logical_index);
    let physical_index: u32 = fluid_physical_cell_index_from_world_cell(cell);
    let center: vec2<f32> = vec2<f32>(cell) + vec2<f32>(0.5);
    let sample: DerivedFluidCellSample = gather_fluid_particle_sample_for_cell(center);
    derived_material_identifiers[physical_index] = sample.material_identifier;
    derived_coverage[physical_index] = sample.coverage;
    derived_velocity[physical_index] = vec4<f32>(sample.velocity, 0.0, 0.0);
    mechanical_cells[physical_index] =
        MechanicalFluidCell(
            sample.mechanical_material_identifier,
            sample.mechanical_mass,
            sample.mechanical_velocity,
        );
    mechanical_original_velocity[physical_index] = sample.mechanical_velocity;
    derived_thermal[physical_index] = sample.thermal;
}

// Each authoritative particle retains its center cell's solved velocity delta.
@compute @workgroup_size(64)
fn scatter_fluid_mechanical_response(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let particle_index: u32 = invocation.x;
    if
        particle_index >= parameters.particle_capacity
            || particles[particle_index].material_identifier == EMPTY_MATERIAL_IDENTIFIER
            || particles[particle_index].is_active == 0u
    {
        return;
    }
    let cell: vec2<i32> =
        vec2<i32>(floor(particles[particle_index].position * CELLS_PER_TILE_FLOAT));
    let index: u32 = fluid_physical_cell_index_from_world_cell(cell);
    if index == INVALID_PHYSICAL_CELL_INDEX || mechanical_cells[index].mass <= 0.0 {
        return;
    }
    particles[particle_index].velocity +=
        mechanical_cells[index].velocity - mechanical_original_velocity[index];
}
