// Copyright Rob Gage 2026

#define_import_path utility::cell_coordinates

const CELLS_PER_TILE: u32 = 8u;
const CELLS_PER_TILE_FLOAT: f32 = 8.0;
const CELL_COUNT_PER_TILE: u32 = 64u;

// Divides signed coordinates toward negative infinity
fn floor_divide_signed_coordinate(value: i32, divisor: i32) -> i32 {
    if value < 0 { return (value - divisor + 1) / divisor; }
    return value / divisor;
}

// Returns a nonnegative remainder for signed coordinates
fn floor_modulo_signed_coordinate(value: i32, divisor: i32) -> i32 {
    return value - floor_divide_signed_coordinate(value, divisor) * divisor;
}

// Converts logical tile-major cell order into a signed world cell
fn world_cell_from_logical_tile_major_index(
    logical_index: u32,
    buffered_origin: vec2<i32>,
    buffered_tile_size: vec2<u32>,
) -> vec2<i32> {
    let logical_tile_index: u32 = logical_index / CELL_COUNT_PER_TILE;
    let local_cell_index: u32 = logical_index % CELL_COUNT_PER_TILE;
    let logical_tile: vec2<u32> = vec2<u32>(
        logical_tile_index % buffered_tile_size.x,
        logical_tile_index / buffered_tile_size.x,
    );
    return (buffered_origin + vec2<i32>(logical_tile)) * i32(CELLS_PER_TILE) +
        vec2<i32>(
            i32(local_cell_index % CELLS_PER_TILE),
            i32(local_cell_index / CELLS_PER_TILE),
        );
}
