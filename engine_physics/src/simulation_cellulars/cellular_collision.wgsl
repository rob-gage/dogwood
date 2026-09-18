// Copyright Rob Gage 2026

#define_import_path compute::cellular_collision

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

#import utility::material_identifier::material_form_from_identifier
#import utility::tile_ring::physical_tile_from_logical_tile

struct CellularCollisionParameters {
    buffered_origin: vec2<i32>,
    buffered_tile_size: vec2<u32>,
    ring_offset: vec2<u32>,
    padding: array<vec4<u32>, 2>,
}

@group(0) @binding(0) var<storage, read> cellular_material_identifiers: array<u32>;
@group(0) @binding(1) var<storage, read_write> occupancy: array<vec4<u32>>;
@group(0) @binding(2) var<uniform> parameters: CellularCollisionParameters;

// Derives separate static and dynamic occupancy words for each logical buffered tile
@compute @workgroup_size(1)
fn extract_cellular_collision_occupancy(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let logical_index: u32 = invocation.x;
    let tile_count: u32 = parameters.buffered_tile_size.x * parameters.buffered_tile_size.y;
    if logical_index >= tile_count {
        return;
    }
    let logical: vec2<u32> =
        vec2<u32>(
            logical_index % parameters.buffered_tile_size.x,
            logical_index / parameters.buffered_tile_size.x,
        );
    let physical: vec2<u32> =
        physical_tile_from_logical_tile(
            logical,
            parameters.buffered_tile_size,
            parameters.ring_offset,
        );
    let cell_start: u32 =
        (physical.y * parameters.buffered_tile_size.x + physical.x) * CELL_COUNT_PER_TILE;
    var static_mask = vec2<u32>(0u);
    var dynamic_mask = vec2<u32>(0u);
    for (var cell: u32 = 0u; cell < CELL_COUNT_PER_TILE; cell++) {
        let identifier = cellular_material_identifiers[cell_start + cell];
        let form: u32 = material_form_from_identifier(identifier);
        if identifier != EMPTY_MATERIAL_IDENTIFIER && form == CELLULAR_STATIC_MATERIAL_FORM {
            static_mask[cell / 32u] |= 1u << (cell % 32u);
        } else if
            identifier != EMPTY_MATERIAL_IDENTIFIER && form == CELLULAR_DYNAMIC_MATERIAL_FORM
        {
            dynamic_mask[cell / 32u] |= 1u << (cell % 32u);
        }
    }
    occupancy[logical_index] = vec4<u32>(static_mask, dynamic_mask);
}
