// Returns the world-cell direction represented by one pressure channel
fn world_cell_direction_from_pressure_channel(channel: u32) -> vec2<i32> {
    if channel == 0u {
        return vec2<i32>(1, 0);
    }
    if channel == 1u {
        return vec2<i32>(-1, 0);
    }
    if channel == 2u {
        return vec2<i32>(0, 1);
    }
    return vec2<i32>(0, -1);
}

// Returns the lateral stencil offset for one directional pressure channel
fn world_cell_side_offset_from_pressure_channel(channel: u32, side: i32) -> vec2<i32> {
    if channel < 2u {
        return vec2<i32>(0, side);
    }
    return vec2<i32>(side, 0);
}

// Converts logical tile-major dispatch order into a signed world cell
fn cellular_pressure_world_cell_from_logical_index(logical_index: u32) -> vec2<i32> {
    return
        world_cell_from_logical_tile_major_index(
            logical_index,
            cellular_pressure_parameters.buffered_origin,
            cellular_pressure_parameters.buffered_tile_size,
        );
}

// Maps one signed world cell through the two-dimensional physical tile ring
fn cellular_pressure_physical_cell_index_from_world_cell(world_cell: vec2<i32>) -> u32 {
    return
        physical_cell_index_from_world_cell(
            world_cell,
            cellular_pressure_parameters.buffered_origin,
            cellular_pressure_parameters.buffered_tile_size,
            cellular_pressure_parameters.ring_offset,
        );
}

// Maps one compacted active tile workgroup to its tile-major logical cell
fn logical_cell_index_from_active_pressure_workgroup(workgroup: u32, local_index: u32) -> u32 {
    return active_pressure_tile_indices[workgroup] * CELL_COUNT_PER_TILE + local_index;
}

// Produces a deterministic per-cell random value for fracture yield decisions
fn cellular_fracture_yield_random_from_world_cell(cell: vec2<i32>, tick: u32) -> f32 {
    var value: u32 = u32(cell.x) * 0x9e3779b9u ^ u32(cell.y) * 0x85ebca6bu ^ tick;
    value ^= value >> 16u;
    return f32(value) / 4294967295.0;
}
