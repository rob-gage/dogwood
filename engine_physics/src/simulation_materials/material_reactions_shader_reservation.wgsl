@compute @workgroup_size(64)
fn clear_transaction_state(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let material_reservation_invocation_index = invocation.x;
    if (material_reservation_invocation_index < arrayLength(&fluid_reservations)) {
        atomicStore(&fluid_reservations[material_reservation_invocation_index], 0u);
    }
    if (material_reservation_invocation_index < arrayLength(&fluid_reservation_owners)) {
        atomicStore(&fluid_reservation_owners[material_reservation_invocation_index], 0xffffffffu);
    }
    if (material_reservation_invocation_index < arrayLength(&gas_reservations)) {
        atomicStore(&gas_reservations[material_reservation_invocation_index], 0u);
    }
    if (material_reservation_invocation_index < arrayLength(&gas_output_reservations)) {
        atomicStore(&gas_output_reservations[material_reservation_invocation_index], 0u);
    }
    if (material_reservation_invocation_index < arrayLength(&canonical_reservations)) {
        atomicStore(&canonical_reservations[material_reservation_invocation_index], 0u);
    }
    if (material_reservation_invocation_index < arrayLength(&rigid_reservations)) {
        atomicStore(&rigid_reservations[material_reservation_invocation_index], 0u);
    }
    if (material_reservation_invocation_index == 0u) {
        atomicStore(&rigid_removal_count[0], 0u);
    }
    if (material_reservation_invocation_index < arrayLength(&candidate_indices)) {
        candidate_indices[material_reservation_invocation_index] = 0xffffffffu;
    }
    if (material_reservation_invocation_index == 0u) {
        atomicStore(&candidate_count[0], 0u);
    }
}

@compute @workgroup_size(64)
fn compact_candidates(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let cell = invocation.x;
    if (cell >= arrayLength(&candidates)) {
        return;
    }
    let candidate = candidates[cell];
    if (candidate.reaction != 0xffffffffu && candidate.extent > 0.000001) {
        let material_candidate_index = atomicAdd(&candidate_count[0], 1u);
        if (material_candidate_index < arrayLength(&candidate_indices)) {
            candidate_indices[material_candidate_index] = cell;
        }
    }
}

@compute @workgroup_size(1)
fn prepare_sort_dispatch(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if (invocation.x == 0u) {
        let active_padded_capacity = active_candidate_capacity();
        sort_indirect[0] = (active_padded_capacity + 63u) / 64u;
        sort_indirect[1] = 1u;
        sort_indirect[2] = 1u;
    }
}

fn active_candidate_capacity() -> u32 {
    let candidate_count_value = min(atomicLoad(&candidate_count[0]), arrayLength(&candidate_indices));
    var active_padded_capacity = 1u;
    while (active_padded_capacity < candidate_count_value) {
        active_padded_capacity = active_padded_capacity << 1u;
    }
    return min(active_padded_capacity, arrayLength(&candidate_indices));
}

fn candidate_before(a: u32, b: u32) -> bool {
    if (a == 0xffffffffu) {
        return false;
    }
    if (b == 0xffffffffu) {
        return true;
    }
    return candidate_better(candidates[a], candidates[b]);
}

@compute @workgroup_size(64)
fn sort_candidates(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let material_sort_index = invocation.x;
    let active_padded_capacity = active_candidate_capacity();
    if (sort_parameters.k > active_padded_capacity || material_sort_index >= active_padded_capacity) {
        return;
    }
    let partner = material_sort_index ^ sort_parameters.j;
    if (partner <= material_sort_index || partner >= active_padded_capacity) {
        return;
    }
    let a = candidate_indices[material_sort_index];
    let b = candidate_indices[partner];
    let ascending = (material_sort_index & sort_parameters.k) == 0u;
    if ((ascending && candidate_before(b, a)) || (!ascending && candidate_before(a, b))) {
        candidate_indices[material_sort_index] = b;
        candidate_indices[partner] = a;
    }
}

fn rigid_source_candidate(candidate: Candidate, reactant: u32) -> bool {
    return
        select(candidate.rigid_claims.x, candidate.rigid_claims.y, reactant == 1u) != 0xffffffffu;
}

fn reserve_rigid_authority(candidate: Candidate, reactant: u32) -> bool {
    let claim = select(candidate.rigid_claims.x, candidate.rigid_claims.y, reactant == 1u);
    if (claim == 0xffffffffu || claim >= arrayLength(&rigid_cells)) {
        return true;
    }
    let rigid = rigid_cells[claim];
    if
        (rigid.state_slot >= arrayLength(&rigid_reservations) || rigid.state_slot >= arrayLength(
            &rigid_amounts,
        ))
    {
        return false;
    }
    if (reactant == 1u && candidate.rigid_claims.x == claim) {
        return true;
    }
    return atomicCompareExchangeWeak(&rigid_reservations[rigid.state_slot], 0u, 1u).exchanged;
}

fn release_rigid_authority(candidate: Candidate, reactant: u32) {
    let claim = select(candidate.rigid_claims.x, candidate.rigid_claims.y, reactant == 1u);
    if (claim != 0xffffffffu && claim < arrayLength(&rigid_cells)) {
        atomicStore(&rigid_reservations[rigid_cells[claim].state_slot], 0u);
    }
}
