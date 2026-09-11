// Copyright Rob Gage 2026

struct Parameters {
    buffered_tile_size: vec2<u32>,
    ring_offset: vec2<u32>,
}

@group(0) @binding(0) var<storage, read> cellular_material_identifiers: array<u32>;
@group(0) @binding(1) var<storage, read_write> occupancy: array<vec2<u32>>;
@group(0) @binding(2) var<uniform> parameters: Parameters;

// Derives two occupancy words for each logical buffered tile from its 64 material identifiers
@compute @workgroup_size(1)
fn extract_occupancy(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let logical_index: u32 = invocation.x;
    let tile_count: u32 = parameters.buffered_tile_size.x * parameters.buffered_tile_size.y;
    if logical_index >= tile_count { return; }
    let logical: vec2<u32> = vec2<u32>(
        logical_index % parameters.buffered_tile_size.x,
        logical_index / parameters.buffered_tile_size.x,
    );
    let physical: vec2<u32> = (logical + parameters.ring_offset) % parameters.buffered_tile_size;
    let cell_start: u32 = (physical.y * parameters.buffered_tile_size.x + physical.x) * 64u;
    var mask = vec2<u32>(0u);
    for (var cell = 0u; cell < 64u; cell++) {
        if cellular_material_identifiers[cell_start + cell] != 0u {
            mask[cell / 32u] |= 1u << (cell % 32u);
        }
    }
    occupancy[logical_index] = mask;
}
