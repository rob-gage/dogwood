// Deterministic global arbitration walks the compact, Accelerator-sorted candidate list.
@compute @workgroup_size(1)
fn reserve_fluid_authority(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if (invocation.x != 0u) {
        return;
    }
    let count = min(atomicLoad(&candidate_count[0]), arrayLength(&candidate_indices));
    for (var processed: u32 = 0u; processed < count; processed += 1u) {
        let best = candidate_indices[processed];
        if (best == 0xffffffffu || best >= arrayLength(&candidates)) {
            continue;
        }
        let candidate = candidates[best];
        candidates[best].padding0 = 2u;
        if (reserve_candidate_canonical(candidate)) {
            reserve_candidate(best);
            if (candidates[best].padding0 == 0u) {
                release_candidate_canonical(candidate);
            }
        }
    }
}

fn release_fluid_slot(slot: u32) {
    if (slot == 0xffffffffu || (slot & 0x80000000u) != 0u) {
        return;
    }
    let count = atomicAdd(&fluid_free_count[0], 1u);
    if (count < arrayLength(&fluid_free_indices)) {
        fluid_free_indices[count] = slot;
    }
}

fn spawn_fluid_product(slot: u32, material: u32, amount: f32, cell: u32, temperature: f32) {
    if ((slot & 0x80000000u) != 0u) {
        let existing_slot = slot & 0x7fffffffu;
        if (existing_slot >= arrayLength(&fluid_particles)) {
            return;
        }
        var existing = fluid_particles[existing_slot];
        let total_amount = max(existing.amount, 0.0) + amount;
        if (total_amount > 0.000001) {
            existing.temperature =
                (max(existing.amount, 0.0) * existing.temperature + amount * temperature)
                    / total_amount;
        }
        existing.amount = total_amount;
        fluid_particles[existing_slot] = existing;
        return;
    }
    let world =
        world_cell_from_physical_tile_ring_index(
            cell,
            fluid_spatial_parameters.buffered_origin,
            fluid_spatial_parameters.buffered_tile_size,
            fluid_spatial_parameters.ring_offset,
        );
    fluid_particles[slot] =
        FluidParticleAuthority(
            material,
            1u,
            (vec2<f32>(world) + vec2<f32>(0.5)) / CELLS_PER_TILE_FLOAT,
            vec2<f32>(0.0),
            vec2<f32>(0.0),
            amount,
            temperature,
        );
}

// Applies only representations which can be reserved deterministically in the
// current pass: source canonical inventory plus zero/two gas outputs, or one
// cellular output in a source cell completely vacated by the reaction. Other
// product shapes are rejected before any mutation; later form-specific passes
// add fluid and rigid transactions using the same candidate record.
