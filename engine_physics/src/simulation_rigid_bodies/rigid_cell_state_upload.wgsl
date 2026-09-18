struct RigidCellStateUploadRecord {
    slot: u32,
    integrity: u32,
    amount: u32,
    temperature: u32,
}

@group(0) @binding(0) var<storage, read> rigid_cell_state_upload_records: array<RigidCellStateUploadRecord>;
@group(0) @binding(1) var<storage, read_write> integrities: array<f32>;
@group(0) @binding(2) var<storage, read_write> amounts: array<f32>;
@group(0) @binding(3) var<storage, read_write> temperatures: array<f32>;
@group(0) @binding(4) var<uniform> rigid_cell_state_record_count: u32;

@compute @workgroup_size(64)
fn upload(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if (invocation.x >= rigid_cell_state_record_count
        || rigid_cell_state_upload_records[invocation.x].slot >= arrayLength(&integrities)) {
        return;
    }
    let rigid_cell_state_upload_record = rigid_cell_state_upload_records[invocation.x];
    integrities[rigid_cell_state_upload_record.slot] =
        bitcast<f32>(rigid_cell_state_upload_record.integrity);
    amounts[rigid_cell_state_upload_record.slot] =
        bitcast<f32>(rigid_cell_state_upload_record.amount);
    temperatures[rigid_cell_state_upload_record.slot] =
        bitcast<f32>(rigid_cell_state_upload_record.temperature);
}
