@group(0) @binding(0) var<storage, read> descriptors: array<u32>;
@group(0) @binding(1) var<storage, read> material_identifiers: array<u32>;
@group(0) @binding(2) var<storage, read> appearances: array<u32>;
@group(0) @binding(3) var<storage, read> integrities: array<f32>;
@group(0) @binding(4) var<storage, read> amounts: array<f32>;
@group(0) @binding(5) var<storage, read> temperatures: array<f32>;
@group(0) @binding(6) var<storage, read_write> output: array<vec4<u32>>;
@group(0) @binding(7) var<uniform> count: u32;

@compute @workgroup_size(64)
fn gather(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if (invocation.x >= count ){ return; }
    let index = descriptors[invocation.x];
    output[invocation.x * 2u] = vec4<u32>(
        material_identifiers[index], appearances[index],
        bitcast<u32>(integrities[index]), bitcast<u32>(amounts[index]),
    );
    output[invocation.x * 2u + 1u] = vec4<u32>(bitcast<u32>(temperatures[index]), 0u, 0u, 0u);
}
