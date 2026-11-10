// Copyright Rob Gage 2026

struct SurfaceUniforms {
    region_width: u32,
    region_height: u32,
    ring_x: u32,
    ring_y: u32,
    tile_size: u32,
    first_cell_x: u32,
    first_cell_y: u32,
    /// Which alternate rows of the field to write: 0 or 1, or 2 for all of them
    phase: u32,
    /// Offset from region-local cells to the field, which wraps by world position
    field_offset_x: u32,
    field_offset_y: u32,
};

@group(0) @binding(0) var<uniform> uniforms: SurfaceUniforms;
@group(0) @binding(1) var<storage, read> materials: array<u32>;
@group(0) @binding(2) var surface_field: texture_storage_2d<rgba8unorm, write>;

/// Offsets from nearest to farthest, so the first differing one gives the distance
const OFFSET_COUNT: u32 = 24u;
const OFFSETS: array<vec2<i32>, 24> = array<vec2<i32>, 24>(
    vec2<i32>(1, 0), vec2<i32>(-1, 0), vec2<i32>(0, 1), vec2<i32>(0, -1),
    vec2<i32>(1, 1), vec2<i32>(-1, 1), vec2<i32>(1, -1), vec2<i32>(-1, -1),
    vec2<i32>(2, 0), vec2<i32>(-2, 0), vec2<i32>(0, 2), vec2<i32>(0, -2),
    vec2<i32>(2, 2), vec2<i32>(-2, 2), vec2<i32>(2, -2), vec2<i32>(-2, -2),
    vec2<i32>(3, 0), vec2<i32>(-3, 0), vec2<i32>(0, 3), vec2<i32>(0, -3),
    vec2<i32>(4, 0), vec2<i32>(-4, 0), vec2<i32>(0, 4), vec2<i32>(0, -4),
);
const DISTANCES: array<f32, 6> = array<f32, 6>(1.0, 1.41421, 2.0, 2.82843, 3.0, 4.0);

/// Returns whether a sparse pattern around a cell finds anything of the opposite state; touching
/// cells are all checked, so only the fading of a surface near very thin features can be missed
fn has_opposite_nearby(cell: vec2<i32>, solid: bool) -> bool {
    for (var index: u32 = 0u; index < 4u; index += 1u) {
        if is_solid(cell + OFFSETS[index]) != solid { return true; }
    }
    for (var index: u32 = 12u; index < 16u; index += 1u) {
        if is_solid(cell + OFFSETS[index]) != solid { return true; }
    }
    for (var index: u32 = 20u; index < 24u; index += 1u) {
        if is_solid(cell + OFFSETS[index]) != solid { return true; }
    }
    return false;
}

/// Returns whether a region-local cell holds a material, treating cells outside as solid
fn is_solid(cell: vec2<i32>) -> bool {
    let cell_width: i32 = i32(uniforms.region_width * uniforms.tile_size);
    let cell_height: i32 = i32(uniforms.region_height * uniforms.tile_size);
    if cell.x < 0 || cell.y < 0 || cell.x >= cell_width || cell.y >= cell_height {
        return true;
    }
    let size: i32 = i32(uniforms.tile_size);
    let tile: vec2<u32> = vec2<u32>(cell / size);
    let within: vec2<u32> = vec2<u32>(cell % size);
    let physical_x: u32 = (tile.x + uniforms.ring_x) % uniforms.region_width;
    let physical_y: u32 = (tile.y + uniforms.ring_y) % uniforms.region_height;
    let index: u32 = (physical_y * uniforms.region_width + physical_x)
        * uniforms.tile_size * uniforms.tile_size
        + within.y * uniforms.tile_size + within.x;
    return materials[index] != 0u;
}

/// Writes one cell's closeness to the opposite state, the direction toward open space, and
/// whether it is solid
@compute @workgroup_size(8, 8, 1)
fn compute(@builtin(global_invocation_id) id: vec3<u32>) {
    let cell: vec2<i32> = vec2<i32>(id.xy) + vec2<i32>(
        i32(uniforms.first_cell_x),
        i32(uniforms.first_cell_y),
    );
    if cell.x >= i32(uniforms.region_width * uniforms.tile_size)
        || cell.y >= i32(uniforms.region_height * uniforms.tile_size)
    { return; }
    let size: vec2<i32> = vec2<i32>(
        i32(uniforms.region_width * uniforms.tile_size),
        i32(uniforms.region_height * uniforms.tile_size),
    );
    let target_cell: vec2<i32> = (cell + vec2<i32>(
        i32(uniforms.field_offset_x),
        i32(uniforms.field_offset_y),
    )) % size;
    if uniforms.phase < 2u && (u32(target_cell.y) & 1u) != uniforms.phase { return; }
    let solid: bool = is_solid(cell);
    if !has_opposite_nearby(cell, solid) {
        textureStore(surface_field, target_cell, vec4<f32>(0.0, 0.5, 0.5, select(0.0, 1.0, solid)));
        return;
    }
    var nearest: f32 = 0.0;
    var direction: vec2<f32> = vec2<f32>(0.0);
    for (var index: u32 = 0u; index < OFFSET_COUNT; index += 1u) {
        let offset: vec2<i32> = OFFSETS[index];
        if is_solid(cell + offset) != solid {
            let distance: f32 = DISTANCES[index / 4u];
            if nearest == 0.0 { nearest = distance; }
            // weigh close samples more, always pointing toward open space
            let sign: f32 = select(-1.0, 1.0, solid);
            direction += sign * vec2<f32>(offset) / (distance * distance);
        }
    }
    let closeness: f32 = select(0.0, 1.0 - (nearest - 1.0) / 4.0, nearest > 0.0);
    let length_squared: f32 = dot(direction, direction);
    let unit: vec2<f32> = select(
        vec2<f32>(0.0),
        direction / sqrt(max(length_squared, 1e-6)),
        length_squared > 1e-6,
    );
    textureStore(
        surface_field,
        target_cell,
        vec4<f32>(closeness, unit * 0.5 + 0.5, select(0.0, 1.0, solid)),
    );
}

/// Cells of halo around a workgroup's cells held in `solid_tile`, the farthest an offset reaches
const HALO: i32 = 4;

/// Side of the workgroup's cells plus its halo on both sides
const TILE_SIDE: u32 = 16u;

/// Whether each cell of a workgroup and its halo holds a material, loaded once so the offsets
/// around a cell read workgroup memory instead of recomputing ring positions in storage
var<workgroup> solid_tile: array<u32, 256>;

/// Returns whether the cell at an offset from a workgroup-local cell holds a material
fn tile_is_solid(local: vec2<i32>, offset: vec2<i32>) -> bool {
    let position: vec2<i32> = local + offset + vec2<i32>(HALO);
    return solid_tile[u32(position.y) * TILE_SIDE + u32(position.x)] != 0u;
}

/// Same field as `compute`, reading neighbors from workgroup memory
@compute @workgroup_size(8, 8, 1)
fn compute_shared(
    @builtin(global_invocation_id) id: vec3<u32>,
    @builtin(local_invocation_id) local_id: vec3<u32>,
    @builtin(workgroup_id) group: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    let origin: vec2<i32> = vec2<i32>(group.xy) * 8 + vec2<i32>(
        i32(uniforms.first_cell_x),
        i32(uniforms.first_cell_y),
    ) - vec2<i32>(HALO);
    for (var load: u32 = local_index; load < TILE_SIDE * TILE_SIDE; load += 64u) {
        let position: vec2<i32> = vec2<i32>(i32(load % TILE_SIDE), i32(load / TILE_SIDE));
        solid_tile[load] = select(0u, 1u, is_solid(origin + position));
    }
    workgroupBarrier();
    let cell: vec2<i32> = vec2<i32>(id.xy) + vec2<i32>(
        i32(uniforms.first_cell_x),
        i32(uniforms.first_cell_y),
    );
    if cell.x >= i32(uniforms.region_width * uniforms.tile_size)
        || cell.y >= i32(uniforms.region_height * uniforms.tile_size)
    { return; }
    let size: vec2<i32> = vec2<i32>(
        i32(uniforms.region_width * uniforms.tile_size),
        i32(uniforms.region_height * uniforms.tile_size),
    );
    let target_cell: vec2<i32> = (cell + vec2<i32>(
        i32(uniforms.field_offset_x),
        i32(uniforms.field_offset_y),
    )) % size;
    if uniforms.phase < 2u && (u32(target_cell.y) & 1u) != uniforms.phase { return; }
    let local: vec2<i32> = vec2<i32>(local_id.xy);
    let solid: bool = tile_is_solid(local, vec2<i32>(0));
    var is_exposed: bool = false;
    for (var index: u32 = 0u; index < 4u; index += 1u) {
        if tile_is_solid(local, OFFSETS[index]) != solid { is_exposed = true; }
    }
    for (var index: u32 = 12u; index < 16u; index += 1u) {
        if tile_is_solid(local, OFFSETS[index]) != solid { is_exposed = true; }
    }
    for (var index: u32 = 20u; index < 24u; index += 1u) {
        if tile_is_solid(local, OFFSETS[index]) != solid { is_exposed = true; }
    }
    if !is_exposed {
        textureStore(surface_field, target_cell, vec4<f32>(0.0, 0.5, 0.5, select(0.0, 1.0, solid)));
        return;
    }
    var nearest: f32 = 0.0;
    var direction: vec2<f32> = vec2<f32>(0.0);
    for (var index: u32 = 0u; index < OFFSET_COUNT; index += 1u) {
        let offset: vec2<i32> = OFFSETS[index];
        if tile_is_solid(local, offset) != solid {
            let distance: f32 = DISTANCES[index / 4u];
            if nearest == 0.0 { nearest = distance; }
            let sign: f32 = select(-1.0, 1.0, solid);
            direction += sign * vec2<f32>(offset) / (distance * distance);
        }
    }
    let closeness: f32 = select(0.0, 1.0 - (nearest - 1.0) / 4.0, nearest > 0.0);
    let length_squared: f32 = dot(direction, direction);
    let unit: vec2<f32> = select(
        vec2<f32>(0.0),
        direction / sqrt(max(length_squared, 1e-6)),
        length_squared > 1e-6,
    );
    textureStore(
        surface_field,
        target_cell,
        vec4<f32>(closeness, unit * 0.5 + 0.5, select(0.0, 1.0, solid)),
    );
}
