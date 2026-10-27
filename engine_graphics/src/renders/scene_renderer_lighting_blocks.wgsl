// Copyright Rob Gage 2026

/// Emission per cell of path in `rgb` and extinction per cell in `a` of every domain cell
@group(0) @binding(0)
var optical: texture_2d<f32>;

/// The shared optical value of every uniform block of 8x8 cells, or a negative extinction
@group(0) @binding(1)
var blocks: texture_storage_2d<rgba16float, write>;

/// Whether a cell of the block differs from its first cell
var<workgroup> differs: atomic<u32>;

/// Marks each block of 8x8 cells whose cells share one optical value, so rays cross it at once
@compute @workgroup_size(8, 8, 1)
fn classify(
    @builtin(workgroup_id) block: vec3<u32>,
    @builtin(local_invocation_id) local: vec3<u32>,
) {
    if all(local.xy == vec2<u32>(0u)) {
        atomicStore(&differs, 0u);
    }
    workgroupBarrier();
    let size: vec2<u32> = textureDimensions(optical, 0);
    let first: vec4<f32> = textureLoad(optical, min(block.xy * 8u, size - 1u), 0);
    let value: vec4<f32> = textureLoad(optical, min(block.xy * 8u + local.xy, size - 1u), 0);
    if any(value != first) {
        atomicStore(&differs, 1u);
    }
    workgroupBarrier();
    if all(local.xy == vec2<u32>(0u)) {
        let is_uniform: bool = atomicLoad(&differs) == 0u;
        textureStore(blocks, block.xy, select(vec4<f32>(0.0, 0.0, 0.0, -1.0), first, is_uniform));
    }
}
