// Copyright Rob Gage 2026

#define_import_path utility::cell_coordinates
#import utility::simulation_constants::{CELLS_PER_TILE, CELLS_PER_TILE_FLOAT, CELL_COUNT_PER_TILE, EMPTY_MATERIAL_IDENTIFIER, GAS_MATERIAL_FORM, CELLULAR_STATIC_MATERIAL_FORM, CELLULAR_DYNAMIC_MATERIAL_FORM, FLUID_MATERIAL_FORM, MATERIAL_IDENTIFIER_INDEX_MASK, INVALID_MATERIAL_DENSE_INDEX, FLUID_EDIT_ERASE, INVALID_PHYSICAL_CELL_INDEX, INVALID_CELLULAR_DYNAMIC_CLAIM_INDEX, INVALID_FLUID_PARTICLE_INDEX, INVALID_FLUID_BUCKET_INDEX, ACTOR_SHAPE_CIRCLE, ACTOR_SHAPE_CAPSULE, ACTOR_SHAPE_RECTANGLE, PI, PBF_SUBSTEP_COUNT, PBF_CONSTRAINT_ITERATION_COUNT, CONSTRAINT_EPSILON, ARTIFICIAL_PRESSURE_DELTA_Q_RATIO, MAXIMUM_CORRECTION_CELLS, HARD_EXTERNAL_BODY_OCCUPANCY, SWIMMER_EXTERNAL_BODY_OCCUPANCY, RIGID_EXTERNAL_BODY_OCCUPANCY, IMMOVABLE_CONTACT_MASS, CONTACT_PRESSURE_TRANSFER, LINEAR_FIXED_SCALE, ANGULAR_FIXED_SCALE, CELL_SIZE, CELL_HALF, CELL_RADIUS, INCOMPRESSIBILITY_MIXING, RESERVATION_SCALE, RESERVATION_SCALE_U32}

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
