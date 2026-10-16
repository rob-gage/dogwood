// Copyright Rob Gage 2026

struct CellularTileUniforms {
    region_width: u32,
    region_height: u32,
    ring_offset_x: u32,
    ring_offset_y: u32,
};

@group(0) @binding(0)
var<uniform> uniforms: CellularTileUniforms;

@group(0) @binding(1)
var<storage, read> material_identifiers: array<u32>;

@group(0) @binding(2)
var<storage, read_write> cellular_tiles: array<vec2<u32>>;

@group(0) @binding(3)
var<storage, read> rigid_body_cells: array<u32>;

/// Packs the solid static cells outside rigid bodies of one active tile into bit `y * 8 + x` in
/// active region order
@compute @workgroup_size(64)
fn compute(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let tile_count: u32 = uniforms.region_width * uniforms.region_height;
    let logical_index: u32 = invocation.x;
    if logical_index >= tile_count { return; }
    let logical_x: u32 = logical_index % uniforms.region_width;
    let logical_y: u32 = logical_index / uniforms.region_width;
    let physical_x: u32 = (logical_x + uniforms.ring_offset_x) % uniforms.region_width;
    let physical_y: u32 = (logical_y + uniforms.ring_offset_y) % uniforms.region_height;
    let cell_start: u32 = (physical_y * uniforms.region_width + physical_x) * 64u;
    var occupancy: vec2<u32> = vec2<u32>(0u, 0u);
    for (var cell: u32 = 0u; cell < 64u; cell++) {
        let material_identifier: u32 = material_identifiers[cell_start + cell];
        // empty shares the static tag, so only nonzero static identifiers are solid, and rigid
        // body cells collide through their own bodies
        if material_identifier != 0u && (material_identifier >> 30u) == 0u
            && rigid_body_cells[cell_start + cell] == 0u
        {
            occupancy[cell / 32u] |= 1u << (cell % 32u);
        }
    }
    cellular_tiles[logical_index] = occupancy;
}
