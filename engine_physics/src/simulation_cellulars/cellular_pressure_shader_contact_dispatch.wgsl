// One color writes each ordinary cell at most once, so both sides receive equal momentum.
@compute @workgroup_size(64)
fn resolve_cellular_contacts_horizontal_even(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    process_cellular_face(workgroup.x, local_index, true, 0, false);
}

@compute @workgroup_size(64)
fn resolve_cellular_contacts_horizontal_odd(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    process_cellular_face(workgroup.x, local_index, true, 1, false);
}

@compute @workgroup_size(64)
fn resolve_cellular_contacts_vertical_even(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    process_cellular_face(workgroup.x, local_index, false, 0, false);
}

@compute @workgroup_size(64)
fn resolve_cellular_contacts_vertical_odd(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    process_cellular_face(workgroup.x, local_index, false, 1, false);
}

@compute @workgroup_size(64)
fn gather_rigid_contacts_horizontal_even(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    process_cellular_face(workgroup.x, local_index, true, 0, true);
}

@compute @workgroup_size(64)
fn gather_rigid_contacts(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    let logical_index: u32 =
        logical_cell_index_from_active_pressure_workgroup(workgroup.x, local_index);
    if logical_index >= parameters.buffered_cell_count {
        return;
    }
    let cell: vec2<i32> = cellular_pressure_world_cell_from_logical_index(logical_index);
    let index: u32 = cellular_pressure_physical_cell_index_from_world_cell(cell);
    if index == INVALID_PHYSICAL_CELL_INDEX {
        return;
    }
    for (var channel: u32 = 0u; channel < 4u; channel++) {
        let direction: vec2<i32> = world_cell_direction_from_pressure_channel(channel);
        let neighbor: u32 = cellular_pressure_physical_cell_index_from_world_cell(cell + direction);
        if neighbor == INVALID_PHYSICAL_CELL_INDEX {
            continue;
        }
        if rigid_owners[index] != 0u && rigid_owners[neighbor] == 0u {
            process_rigid_cellular_face(
                index,
                neighbor,
                vec2<f32>(direction),
                (vec2<f32>(cell) + vec2<f32>(0.5) + vec2<f32>(direction) * 0.5) / 8.0,
                true,
            );
        }
    }
}

@compute @workgroup_size(64)
fn gather_rigid_contacts_horizontal_odd(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    process_cellular_face(workgroup.x, local_index, true, 1, true);
}

@compute @workgroup_size(64)
fn gather_rigid_contacts_vertical_even(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    process_cellular_face(workgroup.x, local_index, false, 0, true);
}

@compute @workgroup_size(64)
fn gather_rigid_contacts_vertical_odd(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    process_cellular_face(workgroup.x, local_index, false, 1, true);
}
