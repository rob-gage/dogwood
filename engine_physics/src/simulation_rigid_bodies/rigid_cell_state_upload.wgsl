struct Record {
    slot: u32,
    integrity: u32,
    amount: u32,
    temperature: u32,
}

@group(0) @binding(0) var<storage, read> records: array<Record>;
@group(0) @binding(1) var<storage, read_write> integrities: array<f32>;
@group(0) @binding(2) var<storage, read_write> amounts: array<f32>;
@group(0) @binding(3) var<storage, read_write> temperatures: array<f32>;
@group(0) @binding(4) var<uniform> count: u32;

@compute @workgroup_size(64)
fn upload(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if (invocation.x >= count || records[invocation.x].slot >= arrayLength(&integrities)) {
        return;
    }
    let r = records[invocation.x];
    integrities[r.slot] = bitcast<f32>(r.integrity);
    amounts[r.slot] = bitcast<f32>(r.amount);
    temperatures[r.slot] = bitcast<f32>(r.temperature);
}
