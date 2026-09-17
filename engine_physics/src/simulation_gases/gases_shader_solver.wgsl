@compute @workgroup_size(64)
fn advect_gas_velocity(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let logical_index: u32 = invocation.x;
    if logical_index >= parameters.buffered_cell_count {
        return;
    }
    let cell: vec2<i32> = world_cell_from_gas_logical_index(logical_index);
    let index: u32 = gas_physical_cell_index_from_world_cell(cell);
    if world_cell_is_gas_obstacle(cell) {
        velocity_scratch[index] = vec2<f32>(0.0);
        return;
    }
    let center: vec2<f32> = vec2<f32>(cell) + vec2<f32>(0.5);
    let backtraced: vec2<f32> = center - velocity[index] * parameters.delta_time;
    velocity_scratch[index] = sample_gas_velocity_bilinear_at_cell_position(backtraced);
}

// Calculates scalar curl from the advected scratch velocity field
@compute @workgroup_size(64)
fn calculate_gas_curl(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let logical_index: u32 = invocation.x;
    if logical_index >= parameters.buffered_cell_count {
        return;
    }
    let cell: vec2<i32> = world_cell_from_gas_logical_index(logical_index);
    let index: u32 = gas_physical_cell_index_from_world_cell(cell);
    if world_cell_is_gas_obstacle(cell) {
        curl[index] = 0.0;
        return;
    }
    let left: vec2<f32> = gas_scratch_velocity_at_world_cell(cell + vec2<i32>(-1, 0));
    let right: vec2<f32> = gas_scratch_velocity_at_world_cell(cell + vec2<i32>(1, 0));
    let bottom: vec2<f32> = gas_scratch_velocity_at_world_cell(cell + vec2<i32>(0, -1));
    let top: vec2<f32> = gas_scratch_velocity_at_world_cell(cell + vec2<i32>(0, 1));
    curl[index] = 0.5 * ((right.y - left.y) - (top.x - bottom.x));
}

// Applies buoyancy and vorticity confinement to advected gas velocity
@compute @workgroup_size(64)
fn apply_gas_forces(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let logical_index: u32 = invocation.x;
    if logical_index >= parameters.buffered_cell_count {
        return;
    }
    let cell: vec2<i32> = world_cell_from_gas_logical_index(logical_index);
    let index: u32 = gas_physical_cell_index_from_world_cell(cell);
    if world_cell_is_gas_obstacle(cell) {
        velocity[index] = vec2<f32>(0.0);
        return;
    }
    let buoyancy: vec2<f32> =
        parameters.gravity
            * CELLS_PER_TILE_FLOAT
            * gas_density_offset_from_ambient(index)
            * parameters.buoyancy_coefficient;
    let confinement: vec2<f32> = calculate_gas_vorticity_confinement(cell, index);
    var next_velocity: vec2<f32> =
        velocity_scratch[index] + (buoyancy + confinement) * parameters.delta_time;
    let speed: f32 = length(next_velocity);
    if speed > parameters.maximum_speed {
        next_velocity *= parameters.maximum_speed / speed;
    }
    velocity[index] = next_velocity;
}

// Calculates velocity divergence for the incompressibility projection
@compute @workgroup_size(64)
fn calculate_gas_divergence(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let logical_index: u32 = invocation.x;
    if logical_index >= parameters.buffered_cell_count {
        return;
    }
    let cell: vec2<i32> = world_cell_from_gas_logical_index(logical_index);
    let index: u32 = gas_physical_cell_index_from_world_cell(cell);
    if world_cell_is_gas_obstacle(cell) {
        divergence[index] = 0.0;
        return;
    }
    divergence[index] =
        0.5 * (gas_velocity_at_world_cell(cell + vec2<i32>(1, 0)).x
            - gas_velocity_at_world_cell(cell + vec2<i32>(-1, 0)).x
            + gas_velocity_at_world_cell(cell + vec2<i32>(0, 1)).y
            - gas_velocity_at_world_cell(cell + vec2<i32>(0, -1)).y);
}

// Clears both Jacobi pressure fields before projection
@compute @workgroup_size(64)
fn clear_gas_pressure(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if invocation.x >= parameters.buffered_cell_count {
        return;
    }
    pressure_a[invocation.x] = 0.0;
    pressure_b[invocation.x] = 0.0;
}

// Solves one Jacobi iteration from pressure B into pressure A
@compute @workgroup_size(64)
fn solve_gas_pressure_a(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let logical_index: u32 = invocation.x;
    if logical_index >= parameters.buffered_cell_count {
        return;
    }
    let cell: vec2<i32> = world_cell_from_gas_logical_index(logical_index);
    let index: u32 = gas_physical_cell_index_from_world_cell(cell);
    if world_cell_is_gas_obstacle(cell) {
        pressure_a[index] = 0.0;
        return;
    }
    pressure_a[index] = calculate_gas_jacobi_pressure(cell, index, false);
}

// Solves one Jacobi iteration from pressure A into pressure B
@compute @workgroup_size(64)
fn solve_gas_pressure_b(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let logical_index: u32 = invocation.x;
    if logical_index >= parameters.buffered_cell_count {
        return;
    }
    let cell: vec2<i32> = world_cell_from_gas_logical_index(logical_index);
    let index: u32 = gas_physical_cell_index_from_world_cell(cell);
    if world_cell_is_gas_obstacle(cell) {
        pressure_b[index] = 0.0;
        return;
    }
    pressure_b[index] = calculate_gas_jacobi_pressure(cell, index, true);
}

// Projects velocity with pressure B after the fixed twelve Jacobi iterations
@compute @workgroup_size(64)
fn project_gas_velocity(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let logical_index: u32 = invocation.x;
    if logical_index >= parameters.buffered_cell_count {
        return;
    }
    let cell: vec2<i32> = world_cell_from_gas_logical_index(logical_index);
    let index: u32 = gas_physical_cell_index_from_world_cell(cell);
    if world_cell_is_gas_obstacle(cell) {
        velocity[index] = vec2<f32>(0.0);
        return;
    }
    let center_pressure: f32 = pressure_b[index];
    var projected: vec2<f32> =
        velocity[index] - 0.5 * vec2<f32>(
            gas_pressure_at_world_cell(
                cell + vec2<i32>(1, 0),
                center_pressure,
                false,
            ) - gas_pressure_at_world_cell(cell + vec2<i32>(-1, 0), center_pressure, false),
            gas_pressure_at_world_cell(
                cell + vec2<i32>(0, 1),
                center_pressure,
                false,
            ) - gas_pressure_at_world_cell(cell + vec2<i32>(0, -1), center_pressure, false),
        );
    velocity[index] = clamp_projected_gas_velocity_against_obstacle_faces(cell, projected);
}

// Advects, diffuses, and dissipates every gas-species concentration
@compute @workgroup_size(64)
fn advect_gas_concentrations(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let field_index: u32 = invocation.x;
    let field_count: u32 = parameters.buffered_cell_count * parameters.gas_count;
    if field_index >= field_count {
        return;
    }
    let species: u32 = field_index / parameters.buffered_cell_count;
    let logical_index: u32 = field_index % parameters.buffered_cell_count;
    let cell: vec2<i32> = world_cell_from_gas_logical_index(logical_index);
    let index: u32 = gas_physical_cell_index_from_world_cell(cell);
    let output_index: u32 =
        gas_concentration_storage_index_from_species_and_physical_cell(species, index);
    if world_cell_is_gas_obstacle(cell) {
        let remaining: f32 = exp(-max(gas_properties[species * 2u].w, 0.0) * parameters.delta_time);
        concentration_scratch[output_index] =
            select(
                concentrations[output_index] * remaining,
                0.0,
                gas_open_neighbor_count(cell) > 0u,
            );
        return;
    }
    let current: f32 = concentrations[output_index];
    let transported: f32 =
        current - parameters.delta_time * (gas_concentration_flux_across_positive_x_face(
            species,
            cell,
        )
            - gas_concentration_flux_across_positive_x_face(species, cell + vec2<i32>(-1, 0))
            + gas_concentration_flux_across_positive_y_face(species, cell)
            - gas_concentration_flux_across_positive_y_face(species, cell + vec2<i32>(0, -1)));
    let properties: vec4<f32> = gas_properties[species * 2u];
    let compressibility: f32 = clamp(gas_properties[species * 2u + 1u].x, 0.0, 1.0);
    let mixing: f32 =
        min(
            (max(
                properties.y,
                0.0,
            ) + (1.0 - compressibility) * INCOMPRESSIBILITY_MIXING) * parameters.delta_time,
            0.24,
        );
    let mixed: f32 =
        transported
            + mixing * (gas_concentration_at_world_cell(species, cell + vec2<i32>(-1, 0), current)
                + gas_concentration_at_world_cell(species, cell + vec2<i32>(1, 0), current)
                + gas_concentration_at_world_cell(species, cell + vec2<i32>(0, -1), current)
                + gas_concentration_at_world_cell(species, cell + vec2<i32>(0, 1), current)
                - 4.0 * current)
            + gas_displaced_into_open_cell(species, cell);
    let remaining: f32 = exp(-max(properties.w, 0.0) * parameters.delta_time);
    concentration_scratch[output_index] = max(0.0, mixed) * remaining;
}

fn gas_open_neighbor_count(cell: vec2<i32>) -> u32 {
    var count = 0u;
    for (var direction = 0u; direction < 4u; direction += 1u) {
        let offset =
            select(
                select(vec2<i32>(0, -1), vec2<i32>(0, 1), direction == 3u),
                select(vec2<i32>(-1, 0), vec2<i32>(1, 0), direction == 1u),
                direction < 2u,
            );
        if !world_cell_is_gas_obstacle(cell + offset) {
            count += 1u;
        }
    }
    return count;
}

fn gas_displaced_into_open_cell(species: u32, cell: vec2<i32>) -> f32 {
    var displaced = 0.0;
    for (var direction = 0u; direction < 4u; direction += 1u) {
        let offset =
            select(
                select(vec2<i32>(0, -1), vec2<i32>(0, 1), direction == 3u),
                select(vec2<i32>(-1, 0), vec2<i32>(1, 0), direction == 1u),
                direction < 2u,
            );
        let source = cell + offset;
        if !world_cell_is_gas_obstacle(source) {
            continue;
        }
        let source_index = gas_physical_cell_index_from_world_cell(source);
        let open_neighbors = gas_open_neighbor_count(source);
        if source_index != INVALID_PHYSICAL_CELL_INDEX && open_neighbors > 0u {
            displaced +=
                concentrations[gas_concentration_storage_index_from_species_and_physical_cell(
                    species,
                    source_index,
                )] / f32(open_neighbors);
        }
    }
    return displaced;
}

// Clears authoritative gas state after a physical ring slot is reassigned
