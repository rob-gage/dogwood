// Copyright Rob Gage 2026

/// One finer mip level of the light texture
@group(0) @binding(0)
var source: texture_2d<f32>;

/// The next coarser mip level, averaging the finer one
@group(0) @binding(1)
var destination: texture_storage_2d<rgba16float, write>;

/// Averages the finer texels covering one coarser texel; the last coarser texel along an axis
/// also covers the finer level's odd trailing texel
@compute @workgroup_size(8, 8, 1)
fn downsample(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let size: vec2<u32> = textureDimensions(destination);
    if any(invocation.xy >= size) {
        return;
    }
    let source_size: vec2<u32> = textureDimensions(source, 0);
    let first: vec2<u32> = invocation.xy * 2u;
    let last: vec2<u32> = select(
        min(first + 1u, source_size - 1u),
        source_size - 1u,
        invocation.xy == size - 1u,
    );
    var sum: vec4<f32> = vec4<f32>(0.0);
    for (var y: u32 = first.y; y <= last.y; y++) {
        for (var x: u32 = first.x; x <= last.x; x++) {
            sum += textureLoad(source, vec2<u32>(x, y), 0);
        }
    }
    let count: vec2<u32> = last - first + 1u;
    textureStore(destination, invocation.xy, sum / f32(count.x * count.y));
}
