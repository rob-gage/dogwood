// Tests continuous tile-space position against buffered residency
fn fluid_position_is_inside_buffered_region(position: vec2<f32>) -> bool {
    let relative: vec2<f32> = position - vec2<f32>(fluid_simulation_parameters.buffered_origin);
    return
        all(relative >= vec2<f32>(0.0))
            && relative.x < f32(fluid_simulation_parameters.buffered_tile_size.x)
            && relative.y < f32(fluid_simulation_parameters.buffered_tile_size.y);
}

// Tests continuous tile-space position against active simulation bounds
fn fluid_position_is_inside_active_region(position: vec2<f32>) -> bool {
    let relative: vec2<f32> = position - vec2<f32>(fluid_simulation_parameters.active_origin);
    return
        all(relative >= vec2<f32>(0.0))
            && relative.x < f32(fluid_simulation_parameters.active_tile_size.x)
            && relative.y < f32(fluid_simulation_parameters.active_tile_size.y);
}

// Includes neighbors needed by active particles and their lambda neighbors
fn fluid_position_supports_active_neighbor_buckets(position: vec2<f32>) -> bool {
    let predicted_movement_cells: f32 =
        f32(
            fluid_simulation_parameters.maximum_movement_cells,
        ) / PBF_SUBSTEP_COUNT + MAXIMUM_CORRECTION_CELLS * PBF_CONSTRAINT_ITERATION_COUNT;
    return
        fluid_position_is_inside_active_padding(
            (fluid_simulation_parameters.support_radius_cells * 2.0 + predicted_movement_cells) / CELLS_PER_TILE_FLOAT,
            position,
        );
}

// Includes particles whose lambdas directly support the active fluid region
fn fluid_position_supports_active_lambdas(position: vec2<f32>) -> bool {
    let predicted_movement_cells: f32 =
        f32(
            fluid_simulation_parameters.maximum_movement_cells,
        ) / PBF_SUBSTEP_COUNT + MAXIMUM_CORRECTION_CELLS * PBF_CONSTRAINT_ITERATION_COUNT;
    return
        fluid_position_is_inside_active_padding(
            (fluid_simulation_parameters.support_radius_cells + predicted_movement_cells) / CELLS_PER_TILE_FLOAT,
            position,
        );
}

// Tests continuous tile-space position against padded active bounds
fn fluid_position_is_inside_active_padding(padding: f32, position: vec2<f32>) -> bool {
    let minimum: vec2<f32> = vec2<f32>(fluid_simulation_parameters.active_origin) - vec2<f32>(padding);
    let maximum: vec2<f32> =
        vec2<f32>(fluid_simulation_parameters.active_origin)
            + vec2<f32>(fluid_simulation_parameters.active_tile_size)
            + vec2<f32>(padding);
    return all(position >= minimum) && all(position < maximum);
}

// Converts continuous tile-space position into fluid spatial-bucket coordinates
fn fluid_spatial_bucket_coordinates_from_position(position: vec2<f32>) -> vec2<i32> {
    return
        fluid_bucket_coordinates_from_position(
            position,
            fluid_simulation_parameters.buffered_origin,
            fluid_simulation_parameters.support_radius_cells,
            CELLS_PER_TILE_FLOAT,
        );
}

// Converts continuous tile-space position into a fluid spatial-bucket index
fn fluid_spatial_bucket_index_from_position(position: vec2<f32>) -> u32 {
    return
        fluid_spatial_bucket_index_from_coordinates(
            fluid_spatial_bucket_coordinates_from_position(position),
        );
}

// Converts bounded fluid bucket coordinates into row-major storage
fn fluid_spatial_bucket_index_from_coordinates(bucket_coordinates: vec2<i32>) -> u32 {
    return
        fluid_bucket_index_from_coordinates(
            bucket_coordinates,
            fluid_simulation_parameters.bucket_dimensions,
            INVALID_FLUID_BUCKET_INDEX,
        );
}

// Maps a world cell through the fluid solver's current physical tile ring
fn fluid_physical_cell_index_from_world_cell(world_cell: vec2<i32>) -> u32 {
    return
        physical_cell_index_from_world_cell(
            world_cell,
            fluid_simulation_parameters.buffered_origin,
            fluid_simulation_parameters.buffered_tile_size,
            fluid_simulation_parameters.ring_offset,
        );
}

// Converts fluid cell dispatch order into a signed buffered world cell
fn world_cell_from_fluid_logical_index(logical_index: u32) -> vec2<i32> {
    return
        world_cell_from_logical_tile_major_index(
            logical_index,
            fluid_simulation_parameters.buffered_origin,
            fluid_simulation_parameters.buffered_tile_size,
        );
}
