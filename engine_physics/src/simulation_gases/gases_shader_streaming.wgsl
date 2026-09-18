@compute @workgroup_size(64)
fn clear_gas_area(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if invocation.x >= gas_simulation_parameters.streaming_cell_count {
        return;
    }
    let cell: vec2<i32> = world_cell_from_gas_streaming_index(invocation.x);
    let gas_physical_cell_index: u32 = gas_physical_cell_index_from_world_cell(cell);
    if gas_physical_cell_index == INVALID_PHYSICAL_CELL_INDEX {
        return;
    }
    velocity[gas_physical_cell_index] = vec2<f32>(0.0);
    for (var species: u32 = 0u; species < gas_simulation_parameters.gas_count; species++) {
        concentrations[gas_concentration_storage_index_from_species_and_physical_cell(
            species,
            gas_physical_cell_index,
        )] =
            0.0;
    }
    gas_temperature[gas_physical_cell_index] = gas_simulation_parameters.ambient_temperature;
}

// Exports one fixed gas record and clears the same outgoing physical cell
@compute @workgroup_size(64)
fn export_gas_area(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let output_index: u32 = invocation.x;
    if output_index >= gas_simulation_parameters.streaming_cell_count {
        return;
    }
    let cell: vec2<i32> = world_cell_from_gas_streaming_index(output_index);
    let gas_physical_cell_index: u32 = gas_physical_cell_index_from_world_cell(cell);
    let stride: u32 = 5u + gas_simulation_parameters.gas_count;
    let start: u32 = output_index * stride;
    streaming_data[start] = bitcast<u32>(cell.x);
    streaming_data[start + 1u] = bitcast<u32>(cell.y);
    streaming_data[start + 2u] = bitcast<u32>(velocity[gas_physical_cell_index].x);
    streaming_data[start + 3u] = bitcast<u32>(velocity[gas_physical_cell_index].y);
    velocity[gas_physical_cell_index] = vec2<f32>(0.0);
    for (var species: u32 = 0u; species < gas_simulation_parameters.gas_count; species++) {
        let concentration: u32 =
            gas_concentration_storage_index_from_species_and_physical_cell(species, gas_physical_cell_index);
        streaming_data[start + 4u] = bitcast<u32>(gas_temperature[gas_physical_cell_index]);
        streaming_data[start + 5u + species] = bitcast<u32>(concentrations[concentration]);
        concentrations[concentration] = 0.0;
    }
}

// Sums density difference from the implicit ambient atmosphere
