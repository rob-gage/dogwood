// Copyright Rob Gage 2026

#define_import_path compute::cellular_collision

#import utility::material_identifier::{
    CELLULAR_DYNAMIC_MATERIAL_FORM,
    CELLULAR_STATIC_MATERIAL_FORM,
    EMPTY_MATERIAL_IDENTIFIER,
    material_form_from_identifier,
}
#import utility::cell_coordinates::CELL_COUNT_PER_TILE
#import utility::tile_ring::physical_tile_from_logical_tile

struct Parameters {
    buffered_origin: vec2<i32>,
    buffered_tile_size: vec2<u32>,
    ring_offset: vec2<u32>,
    padding: array<vec4<u32>, 2>,
}

@group(0) @binding(0) var<storage, read> cellular_material_identifiers: array<u32>;
@group(0) @binding(1) var<storage, read_write> occupancy: array<vec4<u32>>;
@group(0) @binding(2) var<uniform> parameters: Parameters;

// Derives separate static and dynamic occupancy words for each logical buffered tile
@compute @workgroup_size(1)
fn extract_cellular_collision_occupancy(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let logical_index: u32 = invocation.x;
    let tile_count: u32 = parameters.buffered_tile_size.x * parameters.buffered_tile_size.y;
    if logical_index >= tile_count { return; }
    let logical: vec2<u32> = vec2<u32>(
        logical_index % parameters.buffered_tile_size.x,
        logical_index / parameters.buffered_tile_size.x,
    );
    let physical: vec2<u32> = physical_tile_from_logical_tile(
        logical, parameters.buffered_tile_size, parameters.ring_offset,
    );
    let cell_start: u32 = (physical.y * parameters.buffered_tile_size.x + physical.x) *
        CELL_COUNT_PER_TILE;
    var static_mask = vec2<u32>(0u);
    var dynamic_mask = vec2<u32>(0u);
    for (var cell = 0u; cell < CELL_COUNT_PER_TILE; cell++) {
        let identifier = cellular_material_identifiers[cell_start + cell];
        let form: u32 = material_form_from_identifier(identifier);
        if identifier != EMPTY_MATERIAL_IDENTIFIER && form == CELLULAR_STATIC_MATERIAL_FORM {
            static_mask[cell / 32u] |= 1u << (cell % 32u);
        } else if identifier != EMPTY_MATERIAL_IDENTIFIER &&
                form == CELLULAR_DYNAMIC_MATERIAL_FORM {
            dynamic_mask[cell / 32u] |= 1u << (cell % 32u);
        }
    }
    occupancy[logical_index] = vec4<u32>(static_mask, dynamic_mask);
}
