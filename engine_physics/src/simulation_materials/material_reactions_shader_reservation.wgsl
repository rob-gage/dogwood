@compute @workgroup_size(64)

fn clear_transaction_state(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let index = invocation.x;
    if (index < arrayLength(&fluid_reservations) ){
        atomicStore(&fluid_reservations[index], 0u);
    }
    if (index < arrayLength(&fluid_reservation_owners) ){
        atomicStore(&fluid_reservation_owners[index], 0xffffffffu);
    }
    if (index < arrayLength(&gas_reservations) ){
        atomicStore(&gas_reservations[index], 0u);
    }
    if (index < arrayLength(&gas_output_reservations) ){
        atomicStore(&gas_output_reservations[index], 0u);
    }
    if (index < arrayLength(&canonical_reservations) ){
        atomicStore(&canonical_reservations[index], 0u);
    }
    if (index < arrayLength(&rigid_reservations) ){
        atomicStore(&rigid_reservations[index], 0u);
    }
    if (index == 0u ){
        atomicStore(&rigid_removal_count[0], 0u);
    }
    if (index < arrayLength(&candidate_indices) ){
        candidate_indices[index] = 0xffffffffu;
    }
    if (index == 0u ){
        atomicStore(&candidate_count[0], 0u);
    }
}

@compute @workgroup_size(64)
fn compact_candidates(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let cell = invocation.x;
    if (cell >= arrayLength(&candidates) ){
        return;
    }
    let candidate = candidates[cell];
    if (candidate.reaction != 0xffffffffu && candidate.extent > 0.000001 ){
        let index = atomicAdd(&candidate_count[0], 1u);
        if (index < arrayLength(&candidate_indices) ){
            candidate_indices[index] = cell;
        }
    }
}

@compute @workgroup_size(1)
fn prepare_sort_dispatch(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if (invocation.x == 0u ){
        sort_indirect[0] = (arrayLength(&candidate_indices) + 63u) / 64u;
        sort_indirect[1] = 1u;
        sort_indirect[2] = 1u;
    }
}

fn candidate_before(a: u32, b: u32) -> bool {
    if (a == 0xffffffffu ){
        return false;
    }
    if (b == 0xffffffffu ){
        return true;
    }
    return candidate_better(candidates[a], candidates[b]);
}

@compute @workgroup_size(64)
fn sort_candidates(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let index = invocation.x;
    let count = arrayLength(&candidate_indices);
    let partner = index ^ sort_parameters.j;
    if (partner <= index || partner >= arrayLength(&candidate_indices) ){
        return;
    }
    let a = candidate_indices[index];
    let b = candidate_indices[partner];
    let ascending = (index & sort_parameters.k) == 0u;
    if ((ascending && candidate_before(b, a)) || (!ascending && candidate_before(
      a,
      b)) ){
        candidate_indices[index] = b;
        candidate_indices[partner] = a;
    }
}

fn rigid_source_candidate(candidate: Candidate, reactant: u32) -> bool {
    return
    select(
      candidate.rigid_claims.x,
      candidate.rigid_claims.y,
      reactant == 1u) != 0xffffffffu;
}

fn reserve_rigid_authority(candidate: Candidate, reactant: u32) -> bool {
    let claim = select(candidate.rigid_claims.x, candidate.rigid_claims.y, reactant == 1u);
    if (claim == 0xffffffffu || claim >= arrayLength(&rigid_cells) ){
        return true;
    }
    let rigid = rigid_cells[claim];
    if (rigid.state_slot >= arrayLength(
      &rigid_reservations) || rigid.state_slot >= arrayLength(&rigid_amounts) ){
        return false;
    }
    if (reactant == 1u && candidate.rigid_claims.x == claim ){
        return true;
    }
    return
    atomicCompareExchangeWeak(
      &rigid_reservations[rigid.state_slot],
      0u,
      1u).exchanged;
}

fn release_rigid_authority(candidate: Candidate, reactant: u32) {
    let claim = select(candidate.rigid_claims.x, candidate.rigid_claims.y, reactant == 1u);
    if (claim != 0xffffffffu && claim < arrayLength(&rigid_cells) ){
        atomicStore(&rigid_reservations[rigid_cells[claim].state_slot], 0u);
    }
}
