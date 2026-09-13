// Copyright Rob Gage 2026

struct Parameters {
    buffered_origin: vec2<i32>,
    buffered_tile_size: vec2<u32>,
    ring_offset: vec2<u32>,
    actor_center: vec2<f32>,
    actor_collider_size: vec2<f32>,
    gravity: vec2<f32>,
    actor_present: u32,
    padding: u32,
}

@group(0) @binding(0) var<storage, read> cellular_material_identifiers: array<u32>;
@group(0) @binding(1) var<storage, read_write> occupancy: array<vec2<u32>>;
@group(0) @binding(2) var<uniform> parameters: Parameters;

const CELLULAR_STATIC_FORM: u32 = 1u;
const CELLULAR_DYNAMIC_FORM: u32 = 2u;
const CELL_SIZE_TILES: f32 = 1.0 / 8.0;
const COLLISION_CLEARANCE: f32 = 1.0 / 16.0;

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
        let identifier = cellular_material_identifiers[cell_start + cell];
        let world_cell = (parameters.buffered_origin + vec2<i32>(logical)) * 8 +
            vec2<i32>(i32(cell % 8u), i32(cell / 8u));
        let form = identifier >> 30u;
        let occupied = identifier != 0u && (form == CELLULAR_STATIC_FORM ||
            (form == CELLULAR_DYNAMIC_FORM && !cell_inside_player_clearance(world_cell)));
        if occupied {
            mask[cell / 32u] |= 1u << (cell % 32u);
        }
    }
    occupancy[logical_index] = mask;
}

fn cell_inside_player_clearance(cell: vec2<i32>) -> bool {
    if parameters.actor_present == 0u { return false; }
    let gravity_length = length(parameters.gravity);
    let up = select(vec2<f32>(0.0, 1.0), -parameters.gravity / gravity_length,
        gravity_length > 0.0);
    let tangent = vec2<f32>(up.y, -up.x);
    let relative = (vec2<f32>(cell) + vec2<f32>(0.5)) * CELL_SIZE_TILES -
        parameters.actor_center;
    let tangent_distance = dot(relative, tangent);
    let up_distance = dot(relative, up);
    let radius = parameters.actor_collider_size.x * 0.5;
    let segment_half = max(0.0, (parameters.actor_collider_size.y - 2.0 * radius) * 0.5);
    let closest_y = clamp(up_distance, -segment_half, segment_half);
    let distance = length(vec2<f32>(tangent_distance, up_distance - closest_y));
    return distance <= radius + COLLISION_CLEARANCE;
}
