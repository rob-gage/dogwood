// Copyright Rob Gage 2026

#define_import_path utility::fluid_spatial

// Shared authoritative fluid spatial mapping.  Callers provide the current
// ring parameters so this remains valid for streamed, non-zero origins.
fn fluid_particle_world_cell(position: vec2<f32>, cells_per_tile: f32) -> vec2<i32> {
    return vec2<i32>(floor(position * cells_per_tile));
}

fn fluid_particle_belongs_to_cell(
    position: vec2<f32>,
    cell: vec2<i32>,
    cells_per_tile: f32,
) -> bool {
    return all(fluid_particle_world_cell(position, cells_per_tile) == cell);
}

fn fluid_bucket_coordinates_from_position(
    position: vec2<f32>,
    buffered_origin: vec2<i32>,
    support_radius_cells: f32,
    cells_per_tile: f32,
) -> vec2<i32> {
    let bucket_size = support_radius_cells / cells_per_tile;
    return vec2<i32>(floor((position - vec2<f32>(buffered_origin)) / bucket_size));
}

fn fluid_bucket_index_from_coordinates(
    coordinates: vec2<i32>,
    bucket_dimensions: vec2<u32>,
    invalid_bucket: u32,
) -> u32 {
    if any(coordinates < vec2<i32>(0)) ||
            coordinates.x >= i32(bucket_dimensions.x) ||
            coordinates.y >= i32(bucket_dimensions.y) {
        return invalid_bucket;
    }
    return u32(coordinates.y) * bucket_dimensions.x + u32(coordinates.x);
}

// Converts a chemistry/raster cell center into the bucket containing it. The
// caller can apply the fixed -1..1 neighborhood around this base coordinate.
fn fluid_bucket_coordinates_from_cell_center(
    cell_center: vec2<f32>,
    buffered_origin: vec2<i32>,
    support_radius_cells: f32,
    cells_per_tile: f32,
) -> vec2<i32> {
    return fluid_bucket_coordinates_from_position(
        cell_center / cells_per_tile, buffered_origin, support_radius_cells,
        cells_per_tile,
    );
}

// Stable tie-breaking primitive for consumers that need deterministic
// selection independent of linked-list insertion order.
fn fluid_particle_index_is_preferred(candidate: u32, current: u32, invalid: u32) -> bool {
    return current == invalid || candidate < current;
}
