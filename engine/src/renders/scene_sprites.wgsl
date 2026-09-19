// Copyright Rob Gage 2026

#define_import_path graphics::scene_sprites

#import utility::simulation_constants::CELLS_PER_TILE_FLOAT

struct Uniforms {
    camera_position: vec2<f32>,
    window_size: vec2<f32>,
    camera_size: vec2<f32>,
    viewport_origin: vec2<f32>,
    lighting_origin: vec2<i32>,
    lighting_size: vec2<u32>,
    render_mode: u32,
    sprite_count: u32,
    _padding: vec2<u32>,
}

struct SpriteInstance {
    position_size: vec4<f32>,
    world_offset: vec2<f32>,
    _padding: vec2<f32>,
    texture_coordinates: vec4<f32>,
}

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var<storage, read> sprites: array<SpriteInstance>;
@group(0) @binding(2) var sprite_texture: texture_2d<f32>;
@group(0) @binding(3) var radiance_texture: texture_2d<f32>;
@group(0) @binding(4) var sprite_sampler: sampler;
@group(0) @binding(5) var illumination: texture_2d<f32>;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) texture_coordinates: vec2<f32>,
    @location(1) world_position: vec2<f32>,
}

@vertex
fn vertex(
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) instance_index: u32,
) -> VertexOutput {
    let corners = array<vec2<f32>, 6>(
        vec2<f32>(-0.5, -0.5), vec2<f32>(0.5, -0.5), vec2<f32>(0.5, 0.5),
        vec2<f32>(-0.5, -0.5), vec2<f32>(0.5, 0.5), vec2<f32>(-0.5, 0.5),
    );
    let instance = sprites[instance_index];
    let world = instance.position_size.xy + instance.world_offset +
        corners[vertex_index] * instance.position_size.zw;
    var clip: vec2<f32>;
    if uniforms.render_mode == 0u {
        let cell = world * CELLS_PER_TILE_FLOAT - vec2<f32>(uniforms.lighting_origin);
        clip = cell / vec2<f32>(uniforms.lighting_size) * 2.0 - 1.0;
    } else {
        let normalized = (world - uniforms.camera_position) / uniforms.camera_size + 0.5;
        let screen = vec2<f32>(normalized.x, 1.0 - normalized.y) * uniforms.window_size +
            uniforms.viewport_origin;
        clip = screen / uniforms.window_size * 2.0 - 1.0;
        clip.y = -clip.y;
    }
    var output: VertexOutput;
    output.position = vec4<f32>(clip, 0.0, 1.0);
    output.texture_coordinates = mix(
        instance.texture_coordinates.xy,
        instance.texture_coordinates.zw,
        corners[vertex_index] + 0.5,
    );
    output.world_position = world;
    return output;
}

@fragment
fn optical_fragment(input: VertexOutput) -> @location(0) vec4<f32> {
    let ordinary = textureSampleLevel(sprite_texture, sprite_sampler, input.texture_coordinates, 0.0);
    let radiance = textureSampleLevel(radiance_texture, sprite_sampler, input.texture_coordinates, 0.0);
    return vec4<f32>(radiance.rgb * ordinary.a, 0.0);
}

@fragment
fn visible_fragment(input: VertexOutput) -> @location(0) vec4<f32> {
    let ordinary = textureSampleLevel(sprite_texture, sprite_sampler, input.texture_coordinates, 0.0);
    if ordinary.a == 0.0 {
        discard;
    }
    let radiance = textureSampleLevel(radiance_texture, sprite_sampler, input.texture_coordinates, 0.0);
    let incoming = sample_illumination(input.world_position);
    return vec4<f32>(ordinary.rgb * (0.12 + incoming * 3.0) + radiance.rgb, ordinary.a);
}

fn sample_illumination(world_position: vec2<f32>) -> vec3<f32> {
    if any(uniforms.lighting_size == vec2<u32>(0u)) {
        return vec3<f32>(0.0);
    }
    let cell_position = world_position * CELLS_PER_TILE_FLOAT -
        vec2<f32>(uniforms.lighting_origin);
    let coordinate = cell_position - vec2<f32>(0.5);
    let base = vec2<i32>(floor(coordinate));
    if any(base < vec2<i32>(0)) || any(base >= vec2<i32>(uniforms.lighting_size)) {
        return vec3<f32>(0.0);
    }
    let fraction = fract(coordinate);
    let maximum = vec2<i32>(uniforms.lighting_size) - vec2<i32>(1);
    let p00 = textureLoad(illumination, clamp(base, vec2<i32>(0), maximum), 0).rgb;
    let p10 = textureLoad(illumination, clamp(base + vec2<i32>(1, 0), vec2<i32>(0), maximum), 0).rgb;
    let p01 = textureLoad(illumination, clamp(base + vec2<i32>(0, 1), vec2<i32>(0), maximum), 0).rgb;
    let p11 = textureLoad(illumination, clamp(base + vec2<i32>(1), vec2<i32>(0), maximum), 0).rgb;
    return mix(mix(p00, p10, fraction.x), mix(p01, p11, fraction.x), fraction.y);
}
