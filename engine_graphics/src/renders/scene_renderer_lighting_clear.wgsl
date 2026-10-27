// Copyright Rob Gage 2026

struct Params {
    width: u32,
    height: u32,
    count: u32,
    force: u32,
};

@group(0) @binding(0)
var<uniform> params: Params;

@group(0) @binding(1)
var<storage, read_write> fluence: array<vec2<u32>>;

@group(0) @binding(2)
var light: texture_storage_2d<rgba16float, write>;

/// With no emitting optical cells, every traced interval and integrated fluence are zero.
@compute @workgroup_size(64, 1, 1)
fn clear_fluence(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let i: u32 = invocation.x + invocation.y * 65535u * 64u;
    if i < params.width * params.height {
        fluence[i] = vec2<u32>(0u);
    }
}

/// With no optical or background emission, all computed light is exactly zero.
@compute @workgroup_size(8, 8, 1)
fn clear_light(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if invocation.x < params.width && invocation.y < params.height {
        textureStore(light, invocation.xy, vec4<f32>(0.0, 0.0, 0.0, 1.0));
    }
}
