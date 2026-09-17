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

fn reserve_candidate(cell: u32) {
    let candidate = candidates[cell];
    if (candidate.reaction == 0xffffffffu || candidate.extent <= 0.000001 ){
        return;
    }
    candidates[cell].padding0 = 1u;
  // Reject product layouts that the commit pass cannot represent.  Keeping
  // this check in planning means apply never has to discover a late failure
  // (or return a slot it already reserved).
    let first_form = material_form_from_identifier(candidate.material0);
    let first_amount = source_amount(
      Source(cell, candidate.material0, 0.0, true, candidate.rigid_claims.x));
    let first_same_authority = candidate.partner == cell && candidate.material0 == candidate.material1 && candidate.rigid_claims.x == 0xffffffffu && candidate.rigid_claims.y == 0xffffffffu;
    let first_demand = bitcast<f32>(reactions[candidate.reaction].words[2]) + select(
      0.0,
      bitcast<f32>(reactions[candidate.reaction].words[6]),
      first_same_authority);
    let first_remaining = select(
      0.0,
      max(first_amount - first_demand * candidate.extent, 0.0),
      candidate.material0 != EMPTY_MATERIAL_IDENTIFIER && first_form != GAS_MATERIAL_FORM && first_form != FLUID_MATERIAL_FORM);
    var cellular_products = 0u;
    for (var product = 0u; product < 2u; product   += 1u) {
        let base = 8u + product * 4u;
        if (reactions[candidate.reaction].words[base + 2u] == 0u ){
            continue;
        }
        let form = material_form_from_identifier(reactions[candidate.reaction].words[base]);
        if (form == CELLULAR_STATIC_MATERIAL_FORM || form == CELLULAR_DYNAMIC_MATERIAL_FORM ){
            cellular_products   += 1u;
            if (candidate.rigid_claims.x != 0xffffffffu || candidate.rigid_claims.y != 0xffffffffu || (first_form != CELLULAR_STATIC_MATERIAL_FORM && first_form != CELLULAR_DYNAMIC_MATERIAL_FORM) || first_remaining > 0.00001 || bitcast<f32>(
            reactions[candidate.reaction].words[base + 1u]) * candidate.extent <= 0.000001 ){
                candidates[cell].padding0 = 0u;
                return;
            }
        } else if (form != GAS_MATERIAL_FORM && form != FLUID_MATERIAL_FORM ){
            candidates[cell].padding0 = 0u;
            return;
        }
    }
    if (cellular_products > 1u ){
        candidates[cell].padding0 = 0u;
        return;
    }
    if (material_form_from_identifier(
      candidate.material0) == FLUID_MATERIAL_FORM && !reserve_fluid_plan(
      cell,
      cell,
      0u,
      reactions[candidate.reaction],
      bitcast<f32>(reactions[candidate.reaction].words[2]) * candidate.extent) ){
        candidates[cell].padding0 = 0u;
        return;
    }
    if (material_form_from_identifier(
      candidate.material1) == FLUID_MATERIAL_FORM && !reserve_fluid_plan(
      cell,
      candidate.partner,
      1u,
      reactions[candidate.reaction],
      bitcast<f32>(reactions[candidate.reaction].words[6]) * candidate.extent) ){
        if (material_form_from_identifier(
        candidate.material0) == FLUID_MATERIAL_FORM ){
            rollback_fluid_plan(cell, 0u);
        }
        candidates[cell].padding0 = 0u;
        return;
    }
    if (material_form_from_identifier(
      candidate.material0) == GAS_MATERIAL_FORM && !reserve_gas(
      cell,
      candidate.material0,
      bitcast<f32>(reactions[candidate.reaction].words[2]) * candidate.extent) ){
        if (material_form_from_identifier(
        candidate.material0) == FLUID_MATERIAL_FORM ){
            rollback_fluid_plan(cell, 0u);
        }
        if (material_form_from_identifier(
        candidate.material1) == FLUID_MATERIAL_FORM ){
            rollback_fluid_plan(cell, 1u);
        }
        candidates[cell].padding0 = 0u;
        return;
    }
    if (material_form_from_identifier(
      candidate.material1) == GAS_MATERIAL_FORM && !reserve_gas(
      candidate.partner,
      candidate.material1,
      bitcast<f32>(reactions[candidate.reaction].words[6]) * candidate.extent) ){
        if (material_form_from_identifier(candidate.material0) == GAS_MATERIAL_FORM ){
            release_gas_reservation(
        cell,
        candidate.material0,
        bitcast<f32>(reactions[candidate.reaction].words[2]) * candidate.extent);
        }
        if (material_form_from_identifier(
        candidate.material0) == FLUID_MATERIAL_FORM ){
            rollback_fluid_plan(cell, 0u);
        }
        if (material_form_from_identifier(
        candidate.material1) == FLUID_MATERIAL_FORM ){
            rollback_fluid_plan(cell, 1u);
        }
        candidates[cell].padding0 = 0u;
        return;
    }
    var output_slots = vec2<u32>(0xffffffffu);
    for (var product = 0u; product < 2u; product   += 1u) {
        let base = 8u + product * 4u;
        if (reactions
        [candidate.reaction]
        .words[base + 2u] == 0u || material_form_from_identifier(
        reactions[candidate.reaction].words[base]) != FLUID_MATERIAL_FORM ){
            continue;
        }
        let slot = reserve_fluid_slot();
        if (slot == 0xffffffffu ){
            if (material_form_from_identifier(
          candidate.material0) == GAS_MATERIAL_FORM ){
                release_gas_reservation(
          cell,
          candidate.material0,
          bitcast<f32>(
            reactions[candidate.reaction].words[2]) * candidate.extent);
            }
            if (material_form_from_identifier(
          candidate.material1) == GAS_MATERIAL_FORM ){
                release_gas_reservation(
          candidate.partner,
          candidate.material1,
          bitcast<f32>(
            reactions[candidate.reaction].words[6]) * candidate.extent);
            }
            if (material_form_from_identifier(
          candidate.material0) == FLUID_MATERIAL_FORM ){
                rollback_fluid_plan(cell, 0u);
            }
            if (material_form_from_identifier(
          candidate.material1) == FLUID_MATERIAL_FORM ){
                rollback_fluid_plan(cell, 1u);
            }
            release_fluid_slot(output_slots.x);
            release_fluid_slot(output_slots.y);
            candidates[cell].padding0 = 0u;
            return;
        }
        if (product == 0u ){
            output_slots.x = slot;
        } else {
            output_slots.y = slot;
        }
    }
    candidates[cell].product_slots = output_slots;
    let gas_output_cell = select(
      cell,
      candidate.partner,
      first_remaining > 0.00001 && candidate.partner != 0xffffffffu);
    for (var product = 0u; product < 2u; product   += 1u) {
        let base = 8u + product * 4u;
        if (reactions[candidate.reaction].words[base + 2u] == 0u ){
            continue;
        }
        let product_material = reactions[candidate.reaction].words[base];
        if (material_form_from_identifier(
        product_material) == GAS_MATERIAL_FORM && !reserve_gas_output(
        gas_output_cell,
        product_material,
        bitcast<f32>(
          reactions[candidate.reaction].words[base + 1u]) * candidate.extent) ){
            for (var previous = 0u; previous < product; previous   += 1u) {
                let previous_base = 8u + previous * 4u;
                if (reactions
            [candidate.reaction]
            .words[previous_base + 2u] != 0u && material_form_from_identifier(
            reactions[candidate.reaction].words[previous_base]) == GAS_MATERIAL_FORM ){
                    release_gas_output_reservation(
            gas_output_cell,
            reactions[candidate.reaction].words[previous_base],
            bitcast<f32>(
              reactions[candidate.reaction].words[previous_base + 1u]) * candidate.extent);
                }
            }
            if (material_form_from_identifier(
          candidate.material0) == GAS_MATERIAL_FORM ){
                release_gas_reservation(
          cell,
          candidate.material0,
          bitcast<f32>(
            reactions[candidate.reaction].words[2]) * candidate.extent);
            }
            if (material_form_from_identifier(
          candidate.material1) == GAS_MATERIAL_FORM ){
                release_gas_reservation(
          candidate.partner,
          candidate.material1,
          bitcast<f32>(
            reactions[candidate.reaction].words[6]) * candidate.extent);
            }
            if (material_form_from_identifier(
          candidate.material0) == FLUID_MATERIAL_FORM ){
                rollback_fluid_plan(cell, 0u);
            }
            if (material_form_from_identifier(
          candidate.material1) == FLUID_MATERIAL_FORM ){
                rollback_fluid_plan(cell, 1u);
            }
            release_fluid_slot(output_slots.x);
            release_fluid_slot(output_slots.y);
            candidates[cell].padding0 = 0u;
            return;
        }
    }
    var request_count = 0u;
    let source0_needs_mutation = candidate.material0 != EMPTY_MATERIAL_IDENTIFIER && candidate.rigid_claims.x == 0xffffffffu && material_form_from_identifier(candidate.material0) != GAS_MATERIAL_FORM && material_form_from_identifier(
        candidate.material0) != FLUID_MATERIAL_FORM && (cellular_products != 0u || first_remaining <= 0.00001);
    if (source0_needs_mutation ){
        request_count   += 1u;
    }
    let second_amount = source_amount(
      Source(
        candidate.partner,
        candidate.material1,
        0.0,
        true,
        candidate.rigid_claims.y));
    let second_remaining = max(
      second_amount - bitcast<f32>(
        reactions[candidate.reaction].words[6]) * candidate.extent,
      0.0);
    let source1_needs_mutation = candidate.material1 != EMPTY_MATERIAL_IDENTIFIER && candidate.rigid_claims.y == 0xffffffffu && material_form_from_identifier(candidate.material1) != GAS_MATERIAL_FORM && material_form_from_identifier(
        candidate.material1) != FLUID_MATERIAL_FORM && candidate.partner != cell && second_remaining <= 0.00001;
    if (source1_needs_mutation ){
        request_count   += 1u;
    }
    let request_base = reserve_mutation_requests(request_count);
    if (request_base == 0xffffffffu ){
        if (material_form_from_identifier(candidate.material0) == GAS_MATERIAL_FORM ){
            release_gas_reservation(
        cell,
        candidate.material0,
        bitcast<f32>(reactions[candidate.reaction].words[2]) * candidate.extent);
        }
        if (material_form_from_identifier(candidate.material1) == GAS_MATERIAL_FORM ){
            release_gas_reservation(
        candidate.partner,
        candidate.material1,
        bitcast<f32>(reactions[candidate.reaction].words[6]) * candidate.extent);
        }
        for (var product = 0u; product < 2u; product   += 1u) {
            let base = 8u + product * 4u;
            if (reactions
          [candidate.reaction]
          .words[base + 2u] != 0u && material_form_from_identifier(
          reactions[candidate.reaction].words[base]) == GAS_MATERIAL_FORM ){
                release_gas_output_reservation(
          gas_output_cell,
          reactions[candidate.reaction].words[base],
          bitcast<f32>(
            reactions[candidate.reaction].words[base + 1u]) * candidate.extent);
            }
        }
        if (material_form_from_identifier(
        candidate.material0) == FLUID_MATERIAL_FORM ){
            rollback_fluid_plan(cell, 0u);
        }
        if (material_form_from_identifier(
        candidate.material1) == FLUID_MATERIAL_FORM ){
            rollback_fluid_plan(cell, 1u);
        }
        release_fluid_slot(output_slots.x);
        release_fluid_slot(output_slots.y);
        candidates[cell].padding0 = 0u;
        return;
    }
    candidates[cell].padding1 = request_base;
}

fn canonical_source(material: u32) -> bool {
    return
    material != EMPTY_MATERIAL_IDENTIFIER && material_form_from_identifier(material) != GAS_MATERIAL_FORM && material_form_from_identifier(material) != FLUID_MATERIAL_FORM;
}

fn reserve_candidate_canonical(candidate: Candidate) -> bool {
    let first = canonical_source(
      candidate.material0) && candidate.rigid_claims.x == 0xffffffffu;
    let second = canonical_source(
      candidate.material1) && candidate.rigid_claims.y == 0xffffffffu;
    if (rigid_source_candidate(candidate, 0u) && !reserve_rigid_authority(
      candidate,
      0u) ){
        return false;
    }
    if (rigid_source_candidate(candidate, 1u) && !reserve_rigid_authority(
      candidate,
      1u) ){
        release_rigid_authority(candidate, 0u);
        return false;
    }
    if (first && !reserve_canonical_authority(candidate.anchor) ){
        if (rigid_source_candidate(candidate, 0u) ){
            release_rigid_authority(candidate, 0u);
        }
        if (rigid_source_candidate(
        candidate,
        1u) && candidate.rigid_claims.y != candidate.rigid_claims.x ){
            release_rigid_authority(candidate, 1u);
        }
        return false;
    }
    if (second && candidate.partner != candidate.anchor && !reserve_canonical_authority(candidate.partner) ){
        if (first ){
            release_canonical_authority(candidate.anchor);
        }
        if (rigid_source_candidate(candidate, 0u) ){
            release_rigid_authority(candidate, 0u);
        }
        if (rigid_source_candidate(
        candidate,
        1u) && candidate.rigid_claims.y != candidate.rigid_claims.x ){
            release_rigid_authority(candidate, 1u);
        }
        return false;
    }
    return true;
}

fn release_candidate_canonical(candidate: Candidate) {
    if (canonical_source(
      candidate.material0) && candidate.rigid_claims.x == 0xffffffffu ){
        release_canonical_authority(candidate.anchor);
    }
    if (canonical_source(candidate.material1) && candidate.rigid_claims.y == 0xffffffffu && candidate.partner != candidate.anchor ){
        release_canonical_authority(candidate.partner);
    }
    if (rigid_source_candidate(candidate, 0u) ){
        release_rigid_authority(candidate, 0u);
    }
    if (rigid_source_candidate(
      candidate,
      1u) && candidate.rigid_claims.y != candidate.rigid_claims.x ){
        release_rigid_authority(candidate, 1u);
    }
}

fn consume_rigid(claim: u32, demand: f32) {
    if (claim == 0xffffffffu || claim >= arrayLength(&rigid_cells) ){
        return;
    }
    let rigid = rigid_cells[claim];
    if (rigid.state_slot >= arrayLength(&rigid_amounts) ){
        return;
    }
    let remaining = max(rigid_amounts[rigid.state_slot] - demand, 0.0);
    rigid_amounts[rigid.state_slot] = remaining;
    if (remaining <= 0.000001 ){
        let event = atomicAdd(&rigid_removal_count[0], 1u);
        if (event * 2u + 1u < arrayLength(&rigid_removal_events) ){
            rigid_removal_events[event * 2u] = vec4<u32>(
          rigid.state_slot,
          rigid.state_generation,
          rigid.material_identifier,
          rigid.body);
            rigid_removal_events[event * 2u + 1u] = vec4<u32>(
          bitcast<u32>(rigid.local.x),
          bitcast<u32>(rigid.local.y),
          0u,
          0u);
        }
    }
}

// Deterministic global arbitration walks the compact, Accelerator-sorted candidate list.
@compute @workgroup_size(1)
fn reserve_fluid_authority(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if (invocation.x != 0u ){
        return;
    }
    let count = min(atomicLoad(&candidate_count[0]), arrayLength(&candidate_indices));
    for (var processed = 0u; processed < count; processed   += 1u) {
        let best = candidate_indices[processed];
        if (best == 0xffffffffu || best >= arrayLength(&candidates) ){
            continue;
        }
        let candidate = candidates[best];
        candidates[best].padding0 = 2u;
        if (reserve_candidate_canonical(candidate) ){
            reserve_candidate(best);
            if (candidates[best].padding0 == 0u ){
                release_candidate_canonical(candidate);
            }
        }
    }
}

fn release_fluid_slot(slot: u32) {
    if (slot == 0xffffffffu ){
        return;
    }
    let count = atomicAdd(&fluid_free_count[0], 1u);
    if (count < arrayLength(&fluid_free_indices) ){
        fluid_free_indices[count] = slot;
    }
}

fn spawn_fluid_product(
    slot: u32,
    material: u32,
    amount: f32,
    cell: u32,
    temperature: f32
) {
    let world = world_cell_from_physical_tile_ring_index(
      cell,
      fluid_spatial_parameters.buffered_origin,
      fluid_spatial_parameters.buffered_tile_size,
      fluid_spatial_parameters.ring_offset);
    fluid_particles[slot] = FluidParticleAuthority(
      material,
      1u,
      (vec2<f32>(world) + vec2<f32>(0.5)) / CELLS_PER_TILE_FLOAT,
      vec2<f32>(0.0),
      vec2<f32>(0.0),
      amount,
      temperature);
}

// Applies only representations which can be reserved deterministically in the
// current pass: source canonical inventory plus zero/two gas outputs, or one
// cellular output in a source cell completely vacated by the reaction. Other
// product shapes are rejected before any mutation; later form-specific passes
// add fluid and rigid transactions using the same candidate record.
@compute @workgroup_size(64)
