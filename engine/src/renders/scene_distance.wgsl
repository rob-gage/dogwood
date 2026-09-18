// Copyright Rob Gage 2026

struct JumpUniforms {
    step: u32,
    _padding: vec3<u32>,
}

@group(0) @binding(0) var optical_field: texture_2d<f32>;
@group(0) @binding(1) var seed_output: texture_storage_2d<rg32sint, write>;
@group(0) @binding(2) var nearest_input: texture_2d<i32>;
@group(0) @binding(3) var nearest_output: texture_storage_2d<rg32sint, write>;
@group(0) @binding(4) var<uniform> jump_uniforms: JumpUniforms;
@group(0) @binding(5) var final_nearest: texture_2d<i32>;
@group(0) @binding(6) var distance_output: texture_storage_2d<r32float, write>;

@compute @workgroup_size(8, 8)
fn seed(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let dimensions = textureDimensions(optical_field);
    if any(invocation.xy >= dimensions) { return; }
    let position = vec2<i32>(invocation.xy);
    let optical = textureLoad(optical_field, position, 0);
    let nearest = select(vec2<i32>(-1), position, optical.a > 0.000001);
    textureStore(seed_output, position, vec4<i32>(nearest, 0, 0));
}

@compute @workgroup_size(8, 8)
fn jump(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let dimensions = textureDimensions(nearest_input);
    if any(invocation.xy >= dimensions) { return; }
    let position = vec2<i32>(invocation.xy);
    var nearest = vec2<i32>(-1);
    var distance_squared = 0x7fffffffi;
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let candidate_position = position + vec2<i32>(x, y) * i32(jump_uniforms.step);
            if any(candidate_position < vec2<i32>(0)) ||
                    any(candidate_position >= vec2<i32>(dimensions)) { continue; }
            let candidate = textureLoad(nearest_input, candidate_position, 0).xy;
            if candidate.x < 0 { continue; }
            let difference = position - candidate;
            let candidate_distance_squared = dot(difference, difference);
            if candidate_distance_squared < distance_squared {
                nearest = candidate;
                distance_squared = candidate_distance_squared;
            }
        }
    }
    textureStore(nearest_output, position, vec4<i32>(nearest, 0, 0));
}

@compute @workgroup_size(8, 8)
fn finalize(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let dimensions = textureDimensions(final_nearest);
    if any(invocation.xy >= dimensions) { return; }
    let position = vec2<i32>(invocation.xy);
    let nearest = textureLoad(final_nearest, position, 0).xy;
    var distance = length(vec2<f32>(dimensions));
    if nearest.x >= 0 {
        distance = length(vec2<f32>(position - nearest));
    }
    textureStore(distance_output, position, vec4<f32>(distance, 0.0, 0.0, 0.0));
}
