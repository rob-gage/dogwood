// Copyright Rob Gage 2026

// Per-frame camera and tile-ring metadata.
struct Uniforms {
    camera_position: vec2<f32>,
    window_size: vec2<f32>,
    camera_size: vec2<f32>,
    _padding: vec2<f32>,
    buffered_origin: vec2<i32>,
    buffered_tile_size: vec2<u32>,
    ring_offset: vec2<u32>,
    _padding2: vec2<u32>,
}

@group(0) @binding(0) var<storage, read> cellular_material_identifiers: array<u32>;

@group(0) @binding(1) var<uniform> uniforms: Uniforms;

// A fullscreen triangle delegates all scene lookup to the fragment shader
@vertex
fn vertex(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0),
    );
    return vec4<f32>(positions[index], 0.0, 1.0);
}

// Convert each pixel to a world cell, resolve it through the tile ring, then color its form
@fragment
fn fragment(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let normalized = vec2<f32>(
        position.x / uniforms.window_size.x,
        1.0 - position.y / uniforms.window_size.y,
    );
    let world = uniforms.camera_position + (normalized - vec2<f32>(0.5)) * uniforms.camera_size;
    let cell = vec2<i32>(floor(world * 8.0));
    let tile = vec2<i32>(floor_divide(cell.x, 8), floor_divide(cell.y, 8));
    let relative = tile - uniforms.buffered_origin;
    if any(relative < vec2<i32>(0)) { return vec4<f32>(0.0, 0.0, 0.0, 1.0); }
    if relative.x >= i32(uniforms.buffered_tile_size.x) || relative.y >= i32(uniforms.buffered_tile_size.y) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    let physical = (vec2<u32>(relative) + uniforms.ring_offset) % uniforms.buffered_tile_size;
    let tile_index = physical.y * uniforms.buffered_tile_size.x + physical.x;
    let local = vec2<u32>(cell - tile * 8);
    let material_identifier = cellular_material_identifiers[tile_index * 64u + local.y * 8u + local.x];
    if material_identifier == 0u { return vec4<f32>(0.0, 0.0, 0.0, 1.0); }
    // temporary form colors will be replaced by material graphics buffers
    let form = material_identifier >> 30u;
    if form == 1u { return vec4<f32>(0.7, 0.7, 0.7, 1.0); }
    if form == 2u { return vec4<f32>(0.8, 0.6, 0.2, 1.0); }
    return vec4<f32>(0.2, 0.5, 0.9, 1.0);
}

// Signed floor division keeps negative world coordinates in the correct tile.
fn floor_divide(value: i32, divisor: i32) -> i32 {
    if value < 0 { return (value - divisor + 1) / divisor; }
    return value / divisor;
}