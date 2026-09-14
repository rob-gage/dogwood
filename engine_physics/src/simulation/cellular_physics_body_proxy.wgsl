// Copyright Rob Gage 2026

#define_import_path compute::cellular_physics_body_proxy

#import utility::capsule_collision::{
    gravity_relative_capsule_from_parameters,
    world_position_is_inside_gravity_relative_capsule,
}
#import utility::cell_coordinates::{
    CELLS_PER_TILE_FLOAT,
    world_cell_from_logical_tile_major_index,
}
#import utility::tile_ring::{INVALID_PHYSICAL_CELL_INDEX, physical_cell_index_from_world_cell}

struct Parameters {
    buffered_origin: vec2<i32>, buffered_tile_size: vec2<u32>, ring_offset: vec2<u32>,
    center: vec2<f32>, velocity: vec2<f32>, drive: vec2<f32>, collider: vec2<f32>,
    gravity: vec2<f32>, buffered_cell_count: u32, occupancy_kind: u32, padding: vec2<u32>,
}

@group(0) @binding(0) var<storage, read_write> occupancy: array<u32>;
@group(0) @binding(1) var<storage, read_write> velocity: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read_write> count: array<atomic<u32>>;
@group(0) @binding(3) var<uniform> parameters: Parameters;

// Clears the previous transient body proxy before rerasterization
@compute @workgroup_size(64)
fn clear_cellular_physics_body_proxy(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if invocation.x >= parameters.buffered_cell_count { return; }
    occupancy[invocation.x] = 0u;
    velocity[invocation.x] = vec4<f32>(0.0);
    if invocation.x == 0u { atomicStore(&count[0], 0u); }
}

// Rasterizes one gravity-relative pawn capsule into cellular interaction fields
@compute @workgroup_size(64)
fn rasterize_cellular_physics_body_proxy(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let logical_index: u32 = invocation.x;
    if logical_index >= parameters.buffered_cell_count ||
            parameters.occupancy_kind == 0u { return; }
    let world_cell: vec2<i32> = world_cell_from_logical_tile_major_index(
        logical_index, parameters.buffered_origin, parameters.buffered_tile_size,
    );
    let radius: f32 = parameters.collider.x * 0.5;
    let capsule = gravity_relative_capsule_from_parameters(
        parameters.center,
        parameters.gravity,
        radius,
        max(0.0, (parameters.collider.y - 2.0 * radius) * 0.5),
    );
    let world_position: vec2<f32> =
        (vec2<f32>(world_cell) + vec2<f32>(0.5)) / CELLS_PER_TILE_FLOAT;
    if !world_position_is_inside_gravity_relative_capsule(world_position, capsule) { return; }
    let physical_index: u32 = physical_cell_index_from_world_cell(
        world_cell, parameters.buffered_origin, parameters.buffered_tile_size,
        parameters.ring_offset,
    );
    if physical_index == INVALID_PHYSICAL_CELL_INDEX { return; }
    occupancy[physical_index] = parameters.occupancy_kind;
    velocity[physical_index] = vec4<f32>(parameters.velocity, parameters.drive);
    atomicAdd(&count[0], 1u);
}
