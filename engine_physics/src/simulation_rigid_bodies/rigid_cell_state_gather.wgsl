@group(0) @binding(0) var<storage, read> descriptors: array<u32>;
@group(0) @binding(1) var<storage, read> integrities: array<f32>;
@group(0) @binding(2) var<storage, read> amounts: array<f32>;
@group(0) @binding(3) var<storage, read> temperatures: array<f32>;
@group(0) @binding(4) var<storage, read_write> output: array<vec4<f32>>;
@group(0) @binding(5) var<uniform> count: u32;
@compute @workgroup_size(64)
fn gather(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if (invocation.x >= count ){
        return;
    }
    let slot = descriptors[invocation.x];
    output[invocation.x] = vec4<f32>(
        integrities[slot],
        amounts[slot],
        temperatures[slot],
        0.0,
    );
}
