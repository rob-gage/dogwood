// Copyright Rob Gage 2026

#define_import_path compute::cellular_physics_body_proxy

#import utility::capsule_collision::{ gravity_relative_capsule_from_parameters, world_position_is_inside_gravity_relative_capsule, }
#import utility::cell_coordinates::{ CELLS_PER_TILE_FLOAT, world_cell_from_logical_tile_major_index, }
#import utility::tile_ring::{INVALID_PHYSICAL_CELL_INDEX, physical_cell_index_from_world_cell}

struct Parameters {
    buffered_origin: vec2<i32>, buffered_tile_size: vec2<u32>, ring_offset: vec2<u32>,
    center: vec2<f32>, velocity: vec2<f32>, drive: vec2<f32>, collider: vec2<f32>,
    gravity: vec2<f32>, buffered_cell_count: u32, occupancy_kind: u32,
    rigid_cell_count: u32, rigid_body_count: u32, padding: vec4<u32>,
}

struct RigidCell {
    local: vec2<i32>, body: u32, material_identifier: u32,
    appearance: u32, padding_0: u32, padding_1: u32, padding_2: u32,
}

@group(0) @binding(0) var<storage, read_write> occupancy: array<u32>;
@group(0) @binding(1) var<storage, read_write> velocity: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read_write> count: array<atomic<u32>>;
@group(0) @binding(3) var<uniform> parameters: Parameters;
@group(0) @binding(4) var<storage, read_write> rigid_material_identifiers: array<u32>;
@group(0) @binding(5) var<storage, read_write> rigid_appearances: array<u32>;
@group(0) @binding(6) var<storage, read_write> rigid_claims: array<atomic<u32>>;
@group(0) @binding(7) var<storage, read> rigid_cells: array<RigidCell>;
@group(0) @binding(8) var<storage, read> rigid_transforms: array<vec4<f32>>;

@compute @workgroup_size(64)
fn clear_cellular_physics_body_proxy(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if invocation.x >= parameters.buffered_cell_count { return; }
    occupancy[invocation.x] = 0u;
    velocity[invocation.x] = vec4<f32>(0.0);
    rigid_material_identifiers[invocation.x] = 0u;
    rigid_appearances[invocation.x] = 0u;
    atomicStore(&rigid_claims[invocation.x], 0xffffffffu);
    if invocation.x == 0u { atomicStore(&count[0], 0u); }
}

@compute @workgroup_size(64)
fn rasterize_pawn_proxy(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let logical_index: u32 = invocation.x;
    if logical_index >= parameters.buffered_cell_count || parameters.occupancy_kind == 0u { return; }
    let world_cell: vec2<i32> = world_cell_from_logical_tile_major_index(
        logical_index, parameters.buffered_origin, parameters.buffered_tile_size);
    let radius: f32 = parameters.collider.x * 0.5;
    let capsule = gravity_relative_capsule_from_parameters(parameters.center, parameters.gravity,
        radius, max(0.0, (parameters.collider.y - 2.0 * radius) * 0.5));
    let world_position: vec2<f32> =
        (vec2<f32>(world_cell) + vec2<f32>(0.5)) / CELLS_PER_TILE_FLOAT;
    if !world_position_is_inside_gravity_relative_capsule(world_position, capsule) { return; }
    let physical_index: u32 = physical_cell_index_from_world_cell(world_cell,
        parameters.buffered_origin, parameters.buffered_tile_size, parameters.ring_offset);
    if physical_index == INVALID_PHYSICAL_CELL_INDEX { return; }
    occupancy[physical_index] = parameters.occupancy_kind;
    velocity[physical_index] = vec4<f32>(parameters.velocity, parameters.drive);
    atomicAdd(&count[0], 1u);
}

fn rigid_cell_world_bounds(index: u32) -> vec4<i32> {
    let cell: RigidCell = rigid_cells[index];
    let transform: vec4<f32> = rigid_transforms[cell.body * 3u];
    let local_center: vec2<f32> = (vec2<f32>(cell.local) + vec2<f32>(0.5)) /
        CELLS_PER_TILE_FLOAT;
    let center: vec2<f32> = transform.xy + vec2<f32>(
        transform.z * local_center.x - transform.w * local_center.y,
        transform.w * local_center.x + transform.z * local_center.y);
    let extent: f32 = (abs(transform.z) + abs(transform.w)) * 0.5;
    let minimum: vec2<i32> = vec2<i32>(floor(center * CELLS_PER_TILE_FLOAT - extent));
    let maximum: vec2<i32> = vec2<i32>(ceil(center * CELLS_PER_TILE_FLOAT + extent));
    return vec4<i32>(minimum, maximum);
}

// Maps a candidate world-cell center back into body-local cell space
fn rigid_source_contains_world_cell(source: u32, world_cell: vec2<i32>) -> bool {
    let cell: RigidCell = rigid_cells[source];
    let transform: vec4<f32> = rigid_transforms[cell.body * 3u];
    let world_center: vec2<f32> =
        (vec2<f32>(world_cell) + vec2<f32>(0.5)) / CELLS_PER_TILE_FLOAT;
    let relative: vec2<f32> = world_center - transform.xy;
    let local: vec2<f32> = vec2<f32>(
        transform.z * relative.x + transform.w * relative.y,
        -transform.w * relative.x + transform.z * relative.y,
    );
    return all(vec2<i32>(floor(local * CELLS_PER_TILE_FLOAT)) == cell.local);
}

@compute @workgroup_size(64)
fn claim_rigid_cell_proxy(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let source: u32 = invocation.x;
    if source >= parameters.rigid_cell_count { return; }
    let bounds: vec4<i32> = rigid_cell_world_bounds(source);
    for (var y: i32 = bounds.y; y < bounds.w; y++) {
        for (var x: i32 = bounds.x; x < bounds.z; x++) {
            let index: u32 = physical_cell_index_from_world_cell(vec2<i32>(x, y),
                parameters.buffered_origin, parameters.buffered_tile_size, parameters.ring_offset);
            if index != INVALID_PHYSICAL_CELL_INDEX && occupancy[index] == 0u &&
                    rigid_source_contains_world_cell(source, vec2<i32>(x, y)) {
                atomicMin(&rigid_claims[index], source);
            }
        }
    }
}

@compute @workgroup_size(64)
fn resolve_rigid_cell_proxy(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let source: u32 = invocation.x;
    if source >= parameters.rigid_cell_count { return; }
    let cell: RigidCell = rigid_cells[source];
    let motion: vec4<f32> = rigid_transforms[cell.body * 3u + 1u];
    let center_of_mass: vec2<f32> = rigid_transforms[cell.body * 3u + 2u].xy;
    let bounds: vec4<i32> = rigid_cell_world_bounds(source);
    for (var y: i32 = bounds.y; y < bounds.w; y++) {
        for (var x: i32 = bounds.x; x < bounds.z; x++) {
            let index: u32 = physical_cell_index_from_world_cell(vec2<i32>(x, y),
                parameters.buffered_origin, parameters.buffered_tile_size, parameters.ring_offset);
            if index == INVALID_PHYSICAL_CELL_INDEX ||
                    atomicLoad(&rigid_claims[index]) != source { continue; }
            let point: vec2<f32> = (vec2<f32>(f32(x), f32(y)) + vec2<f32>(0.5)) /
                CELLS_PER_TILE_FLOAT;
            let radius: vec2<f32> = point - center_of_mass;
            occupancy[index] = 1u;
            let point_velocity: vec2<f32> =
                motion.xy + motion.z * vec2<f32>(-radius.y, radius.x);
            velocity[index] = vec4<f32>(point_velocity.x, point_velocity.y, 0.0, 0.0);
            rigid_material_identifiers[index] = cell.material_identifier;
            rigid_appearances[index] = cell.appearance;
            atomicAdd(&count[0], 1u);
        }
    }
}
