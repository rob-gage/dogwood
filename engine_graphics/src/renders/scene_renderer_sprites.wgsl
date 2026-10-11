// Copyright Rob Gage 2026

struct SpriteUniforms {
    viewport_width: f32,
    viewport_height: f32,
    pixels_per_tile: f32,
    padding: f32,
};

struct SpriteInstance {
    position_size: vec4<f32>,
    rotation: f32,
    padding: f32,
    texture_coordinates: vec4<f32>,
    color: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> uniforms: SpriteUniforms;

@group(0) @binding(1)
var sprite_sampler: sampler;

@group(1) @binding(0)
var sprite_texture: texture_2d<f32>;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) texture_coordinates: vec2<f32>,
    @location(1) color: vec4<f32>,
};

/// Transforms one sprite corner into viewport clip space
@vertex
fn vertex(
    @location(0) position_size: vec4<f32>,
    @location(1) rotation: f32,
    @location(2) texture_coordinates: vec4<f32>,
    @location(3) color: vec4<f32>,
    @builtin(vertex_index) vertex_index: u32,
) -> VertexOutput {
    let corners: array<vec2<f32>, 4> = array<vec2<f32>, 4>(
        vec2<f32>(-0.5, 0.5),
        vec2<f32>(0.5, 0.5),
        vec2<f32>(-0.5, -0.5),
        vec2<f32>(0.5, -0.5),
    );
    let corner: vec2<f32> = corners[vertex_index];
    let scaled_corner: vec2<f32> = corner * position_size.zw;
    let sine: f32 = sin(rotation);
    let cosine: f32 = cos(rotation);
    let rotated_corner: vec2<f32> = vec2<f32>(
        scaled_corner.x * cosine - scaled_corner.y * sine,
        scaled_corner.x * sine + scaled_corner.y * cosine,
    );
    let camera_relative_position: vec2<f32> = position_size.xy + rotated_corner;
    let clip_position: vec2<f32> = camera_relative_position * uniforms.pixels_per_tile * 2.0
        / vec2<f32>(uniforms.viewport_width, uniforms.viewport_height);
    let texture_coordinate_fraction: vec2<f32> = corner + vec2<f32>(0.5);
    let output_texture_coordinates: vec2<f32> = vec2<f32>(
        mix(texture_coordinates.x, texture_coordinates.z, texture_coordinate_fraction.x),
        mix(texture_coordinates.w, texture_coordinates.y, texture_coordinate_fraction.y),
    );
    var output: VertexOutput;
    output.position = vec4<f32>(clip_position, 0.0, 1.0);
    output.texture_coordinates = output_texture_coordinates;
    output.color = color;
    return output;
}

/// Samples and premultiplies one sprite texel
@fragment
fn fragment(input: VertexOutput) -> @location(0) vec4<f32> {
    let texel: vec4<f32> = textureSampleLevel(
        sprite_texture,
        sprite_sampler,
        input.texture_coordinates,
        0.0,
    );
    let color: vec4<f32> = texel * input.color;
    return vec4<f32>(color.rgb * color.a, color.a);
}
