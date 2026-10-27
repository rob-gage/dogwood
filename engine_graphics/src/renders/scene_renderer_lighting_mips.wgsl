// Copyright Rob Gage 2026

/// One finer mip level of the light texture
@group(0) @binding(0)
var source: texture_2d<f32>;

/// The next coarser mip level, averaging the finer one
@group(0) @binding(1)
var destination: texture_storage_2d<rgba16float, write>;

/// Averages the finer texels covering one coarser texel
@compute @workgroup_size(8, 8, 1)
fn downsample(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let size: vec2<u32> = textureDimensions(destination);
    if any(invocation.xy >= size) {
        return;
    }
    let source_size: vec2<u32> = textureDimensions(source, 0);
    var sum: vec4<f32> = vec4<f32>(0.0);
    var count: f32 = 0.0;
    for (var corner: u32 = 0u; corner < 4u; corner++) {
        let texel: vec2<u32> = invocation.xy * 2u + vec2<u32>(corner & 1u, corner >> 1u);
        if all(texel < source_size) {
            sum += textureLoad(source, texel, 0);
            count += 1.0;
        }
    }
    textureStore(destination, invocation.xy, sum / max(count, 1.0));
}
