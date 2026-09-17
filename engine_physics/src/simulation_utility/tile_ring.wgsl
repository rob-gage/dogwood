// Copyright Rob Gage 2026

#define_import_path utility::tile_ring

#import utility::cell_coordinates::{
    floor_divide_signed_coordinate,
}
#import utility::simulation_constants::{
    CELLS_PER_TILE,
    CELLS_PER_TILE_FLOAT,
    CELL_COUNT_PER_TILE,
    EMPTY_MATERIAL_IDENTIFIER,
    GAS_MATERIAL_FORM,
    CELLULAR_STATIC_MATERIAL_FORM,
    CELLULAR_DYNAMIC_MATERIAL_FORM,
    FLUID_MATERIAL_FORM,
    MATERIAL_IDENTIFIER_INDEX_MASK,
    INVALID_MATERIAL_DENSE_INDEX,
    FLUID_EDIT_ERASE,
    INVALID_PHYSICAL_CELL_INDEX,
    INVALID_CELLULAR_DYNAMIC_CLAIM_INDEX,
    INVALID_FLUID_PARTICLE_INDEX,
    INVALID_FLUID_BUCKET_INDEX,
    ACTOR_SHAPE_CIRCLE,
    ACTOR_SHAPE_CAPSULE,
    ACTOR_SHAPE_RECTANGLE,
    PI,
    PBF_SUBSTEP_COUNT,
    PBF_CONSTRAINT_ITERATION_COUNT,
    CONSTRAINT_EPSILON,
    ARTIFICIAL_PRESSURE_DELTA_Q_RATIO,
    MAXIMUM_CORRECTION_CELLS,
    HARD_EXTERNAL_BODY_OCCUPANCY,
    SWIMMER_EXTERNAL_BODY_OCCUPANCY,
    RIGID_EXTERNAL_BODY_OCCUPANCY,
    IMMOVABLE_CONTACT_MASS,
    CONTACT_PRESSURE_TRANSFER,
    LINEAR_FIXED_SCALE,
    ANGULAR_FIXED_SCALE,
    CELL_SIZE,
    CELL_HALF,
    CELL_RADIUS,
    INCOMPRESSIBILITY_MIXING,
    RESERVATION_SCALE,
    RESERVATION_SCALE_U32
}

// Maps a logical tile to its current physical tile-ring slot
fn physical_tile_from_logical_tile(
    logical_tile: vec2<u32>,
    buffered_tile_size: vec2<u32>,
    ring_offset: vec2<u32>,
) -> vec2<u32> {
    return (logical_tile + ring_offset) % buffered_tile_size;
}

// Maps a world cell through the two-dimensional ring to tile-major storage
fn physical_cell_index_from_world_cell(
    world_cell: vec2<i32>,
    buffered_origin: vec2<i32>,
    buffered_tile_size: vec2<u32>,
    ring_offset: vec2<u32>,
) -> u32 {
    let world_tile: vec2<i32> = vec2<i32>(
      floor_divide_signed_coordinate(world_cell.x, i32(CELLS_PER_TILE)),
      floor_divide_signed_coordinate(world_cell.y, i32(CELLS_PER_TILE)),);
    let logical_tile: vec2<i32> = world_tile - buffered_origin;
    if any(logical_tile < vec2<i32>(0)) || logical_tile.x >= i32(buffered_tile_size.x) || logical_tile.y >= i32(buffered_tile_size.y) {
        return INVALID_PHYSICAL_CELL_INDEX;
    }
    let physical_tile: vec2<u32> = physical_tile_from_logical_tile(
      vec2<u32>(logical_tile),
      buffered_tile_size,
      ring_offset,);
    let local_cell: vec2<u32> = vec2<u32>(world_cell - world_tile * i32(CELLS_PER_TILE),);
    return
    (physical_tile.y * buffered_tile_size.x + physical_tile.x) * CELL_COUNT_PER_TILE + local_cell.y * CELLS_PER_TILE + local_cell.x;
}

// Converts one physical tile-ring cell index back into a signed world cell
fn world_cell_from_physical_tile_ring_index(
    physical_cell_index: u32,
    buffered_origin: vec2<i32>,
    buffered_tile_size: vec2<u32>,
    ring_offset: vec2<u32>,
) -> vec2<i32> {
    let physical_tile_index: u32 = physical_cell_index / CELL_COUNT_PER_TILE;
    let local_cell_index: u32 = physical_cell_index % CELL_COUNT_PER_TILE;
    let physical_tile: vec2<u32> = vec2<u32>(
      physical_tile_index % buffered_tile_size.x,
      physical_tile_index / buffered_tile_size.x,);
    let logical_tile: vec2<u32> = (physical_tile + buffered_tile_size - ring_offset) % buffered_tile_size;
    return
    (buffered_origin + vec2<i32>(logical_tile)) * i32(CELLS_PER_TILE) + vec2<i32
    >(
      i32(local_cell_index % CELLS_PER_TILE),
      i32(local_cell_index / CELLS_PER_TILE),);
}
