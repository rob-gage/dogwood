struct Parameters {
    buffered_origin: vec2<i32>, buffered_tile_size: vec2<u32>, ring_offset: vec2<u32>,
    center: vec2<f32>, velocity: vec2<f32>, drive: vec2<f32>, collider: vec2<f32>,
    gravity: vec2<f32>, buffered_cell_count: u32, occupancy_kind: u32, padding: vec2<u32>,
}

@group(0) @binding(0) var<storage, read_write> occupancy: array<u32>;
@group(0) @binding(1) var<storage, read_write> velocity: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read_write> count: array<atomic<u32>>;
@group(0) @binding(3) var<uniform> parameters: Parameters;

const INVALID: u32 = 0xffffffffu;

@compute @workgroup_size(64)
fn clear_cellular_physics_body_proxy(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if invocation.x >= parameters.buffered_cell_count { return; }
    occupancy[invocation.x] = 0u;
    velocity[invocation.x] = vec4<f32>(0.0);
    if invocation.x == 0u { atomicStore(&count[0], 0u); }
}

@compute @workgroup_size(64)
fn rasterize_cellular_physics_body_proxy(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let logical = invocation.x;
    if logical >= parameters.buffered_cell_count || parameters.occupancy_kind == 0u { return; }
    let tile = logical / 64u;
    let cell = (parameters.buffered_origin + vec2<i32>(i32(tile % parameters.buffered_tile_size.x),
        i32(tile / parameters.buffered_tile_size.x))) * 8 + vec2<i32>(i32(logical % 8u), i32((logical % 64u) / 8u));
    let gravity_length = length(parameters.gravity);
    let up = select(vec2<f32>(0.0, 1.0), -parameters.gravity / gravity_length, gravity_length > 0.0);
    let tangent = vec2<f32>(up.y, -up.x);
    let relative = (vec2<f32>(cell) + vec2<f32>(0.5)) / 8.0 - parameters.center;
    let radius = parameters.collider.x * 0.5;
    let half_segment = max(0.0, (parameters.collider.y - 2.0 * radius) * 0.5);
    let nearest = clamp(dot(relative, up), -half_segment, half_segment);
    if length(vec2<f32>(dot(relative, tangent), dot(relative, up) - nearest)) > radius { return; }
    let physical = physical_index(cell);
    if physical == INVALID { return; }
    occupancy[physical] = parameters.occupancy_kind;
    velocity[physical] = vec4<f32>(parameters.velocity, parameters.drive);
    atomicAdd(&count[0], 1u);
}

fn physical_index(cell: vec2<i32>) -> u32 {
    let tile = vec2<i32>(floor_div(cell.x, 8), floor_div(cell.y, 8));
    let relative = tile - parameters.buffered_origin;
    if any(relative < vec2<i32>(0)) || relative.x >= i32(parameters.buffered_tile_size.x) || relative.y >= i32(parameters.buffered_tile_size.y) { return INVALID; }
    let physical = (vec2<u32>(relative) + parameters.ring_offset) % parameters.buffered_tile_size;
    let local = vec2<u32>(cell - tile * 8);
    return (physical.y * parameters.buffered_tile_size.x + physical.x) * 64u + local.y * 8u + local.x;
}
fn floor_div(value: i32, divisor: i32) -> i32 { if value < 0 { return (value - divisor + 1) / divisor; } return value / divisor; }
