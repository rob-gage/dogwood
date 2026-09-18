// Reserves fluid inventory without touching authoritative particles. The
// Retains the complete contributor plan in per-particle reservation state.
fn reserve_fluid_plan(
    record: u32,
    cell: u32,
    reactant: u32,
    rule: Reaction,
    required: f32,
) -> bool {
    var remaining = required;
    let world =
        world_cell_from_physical_tile_ring_index(
            cell,
            fluid_spatial_parameters.buffered_origin,
            fluid_spatial_parameters.buffered_tile_size,
            fluid_spatial_parameters.ring_offset,
        );
    let center = (vec2<f32>(world) + vec2<f32>(0.5)) / CELLS_PER_TILE_FLOAT;
    let base =
        fluid_bucket_coordinates_from_position(
            center,
            fluid_spatial_parameters.buffered_origin,
            fluid_spatial_parameters.support_radius_cells,
            CELLS_PER_TILE_FLOAT,
        );
    for (
        var iteration: u32 = 0u;
        iteration < fluid_spatial_parameters.particle_capacity && remaining > 0.000001;
        iteration += 1u
    ) {
        var selected = 0xffffffffu;
        for (var y: i32 = -1; y <= 1; y += 1) {
            for (var x: i32 = -1; x <= 1; x += 1) {
                let bucket =
                    fluid_bucket_index_from_coordinates(
                        base + vec2<i32>(x, y),
                        fluid_spatial_parameters.bucket_dimensions,
                        0xffffffffu,
                    );
                if (bucket == 0xffffffffu) {
                    continue;
                }
                var p = atomicLoad(&fluid_bucket_heads[bucket]);
                for (
                    var n: u32 = 0u;
                    p != 0xffffffffu && n < fluid_spatial_parameters.particle_capacity;
                    n += 1u
                ) {
                    let particle = fluid_particles[p];
                    if (p >= selected && selected != 0xffffffffu) {
                        p = fluid_next_particle[p];
                        continue;
                    }
                    if
                        (particle.is_active != 0u
                            && material_form_from_identifier(
                                particle.material_identifier,
                            ) == FLUID_MATERIAL_FORM
                            && fluid_particle_belongs_to_cell(
                                particle.position,
                                world,
                                CELLS_PER_TILE_FLOAT,
                            )
                            && particle.amount > 0.000001
                            && matches_selector(rule, reactant, particle.material_identifier)
                            && atomicLoad(&fluid_reservation_owners[p]) == 0xffffffffu
                            && atomicLoad(&fluid_reservations[p]) < u32(
                                max(particle.amount, 0.0) * RESERVATION_SCALE,
                            ))
                    {
                        selected = p;
                    }
                    p = fluid_next_particle[p];
                }
            }
        }
        if (selected == 0xffffffffu) {
            rollback_fluid_plan(cell, reactant);
            return false;
        }
        let particle = fluid_particles[selected];
        let already = f32(atomicLoad(&fluid_reservations[selected])) / RESERVATION_SCALE;
        let available = max(particle.amount - already, 0.0);
        let take = min(remaining, available);
        let units = u32(max(take, 0.0) * RESERVATION_SCALE);
        let old = atomicLoad(&fluid_reservations[selected]);
        let result = atomicCompareExchangeWeak(&fluid_reservations[selected], old, old + units);
        if (result.exchanged) {
            atomicStore(&fluid_reservation_owners[selected], record);
            remaining -= take;
        }
    }
    if (remaining > 0.00001) {
        rollback_fluid_plan(cell, reactant);
        return false;
    }
    return true;
}

fn reserve_fluid_slot() -> u32 {
    var fluid_free_slot_count = atomicLoad(&fluid_free_count[0]);
    loop {
        if (fluid_free_slot_count == 0u) {
            return 0xffffffffu;
        }
        let result = atomicCompareExchangeWeak(&fluid_free_count[0], fluid_free_slot_count, fluid_free_slot_count - 1u);
        if (result.exchanged) {
            return fluid_free_indices[fluid_free_slot_count - 1u];
        }
        fluid_free_slot_count = result.old_value;
    }
    return 0xffffffffu;
}

// The authority reservation pass is serialized, so a product can reuse an
// unreserved particle that already represents the same material in its output
// cell. The high bit distinguishes that existing particle from a newly
// claimed free-list slot in Candidate.product_slots.
fn find_fluid_product_slot(cell: u32, material: u32) -> u32 {
    let world =
        world_cell_from_physical_tile_ring_index(
            cell,
            fluid_spatial_parameters.buffered_origin,
            fluid_spatial_parameters.buffered_tile_size,
            fluid_spatial_parameters.ring_offset,
        );
    let center = (vec2<f32>(world) + vec2<f32>(0.5)) / CELLS_PER_TILE_FLOAT;
    let base =
        fluid_bucket_coordinates_from_position(
            center,
            fluid_spatial_parameters.buffered_origin,
            fluid_spatial_parameters.support_radius_cells,
            CELLS_PER_TILE_FLOAT,
        );
    var selected = 0xffffffffu;
    for (var y: i32 = -1; y <= 1; y += 1) {
        for (var x: i32 = -1; x <= 1; x += 1) {
            let bucket =
                fluid_bucket_index_from_coordinates(
                    base + vec2<i32>(x, y),
                    fluid_spatial_parameters.bucket_dimensions,
                    0xffffffffu,
                );
            if (bucket == 0xffffffffu) {
                continue;
            }
            var p = atomicLoad(&fluid_bucket_heads[bucket]);
            for (
                var n: u32 = 0u;
                p != 0xffffffffu && n < fluid_spatial_parameters.particle_capacity;
                n += 1u
            ) {
                let particle = fluid_particles[p];
                if
                    (p < selected
                        && particle.is_active != 0u
                        && particle.material_identifier == material
                        && particle.amount > 0.000001
                        && distance(particle.position, center)
                            <= fluid_spatial_parameters.support_radius_cells / CELLS_PER_TILE_FLOAT
                        && atomicLoad(&fluid_reservation_owners[p]) == 0xffffffffu
                        && atomicLoad(&fluid_reservations[p]) == 0u)
                {
                    selected = p;
                }
                p = fluid_next_particle[p];
            }
        }
    }
    if (selected == 0xffffffffu) {
        return selected;
    }
    atomicStore(&fluid_reservation_owners[selected], 0xfffffffeu);
    return selected | 0x80000000u;
}

fn fluid_source_cell(cell: u32, source_cell: u32, commit: bool) {
    let world =
        world_cell_from_physical_tile_ring_index(
            source_cell,
            fluid_spatial_parameters.buffered_origin,
            fluid_spatial_parameters.buffered_tile_size,
            fluid_spatial_parameters.ring_offset,
        );
    let center = (vec2<f32>(world) + vec2<f32>(0.5)) / CELLS_PER_TILE_FLOAT;
    let base =
        fluid_bucket_coordinates_from_position(
            center,
            fluid_spatial_parameters.buffered_origin,
            fluid_spatial_parameters.support_radius_cells,
            CELLS_PER_TILE_FLOAT,
        );
    for (var y: i32 = -1; y <= 1; y += 1) {
        for (var x: i32 = -1; x <= 1; x += 1) {
            let bucket =
                fluid_bucket_index_from_coordinates(
                    base + vec2<i32>(x, y),
                    fluid_spatial_parameters.bucket_dimensions,
                    0xffffffffu,
                );
            if (bucket == 0xffffffffu) {
                continue;
            }
            var fluid_particle_index = atomicLoad(&fluid_bucket_heads[bucket]);
            for (
                var n = 0u;
                fluid_particle_index != 0xffffffffu && n < fluid_spatial_parameters.particle_capacity;
                n += 1u
            ) {
                if (atomicLoad(&fluid_reservation_owners[fluid_particle_index]) == cell) {
                    if (commit) {
                        let amount =
                            f32(atomicLoad(&fluid_reservations[fluid_particle_index])) / RESERVATION_SCALE;
                        let next_amount = max(fluid_particles[fluid_particle_index].amount - amount, 0.0);
                        fluid_particles[fluid_particle_index].amount = next_amount;
                        atomicStore(&fluid_reservations[fluid_particle_index], 0u);
                        atomicStore(&fluid_reservation_owners[fluid_particle_index], 0xffffffffu);
                        if (next_amount <= 0.000001) {
                            fluid_particles[fluid_particle_index].material_identifier = EMPTY_MATERIAL_IDENTIFIER;
                            fluid_particles[fluid_particle_index].is_active = 0u;
                            let free = atomicAdd(&fluid_free_count[0], 1u);
                            if (free < arrayLength(&fluid_free_indices)) {
                                fluid_free_indices[free] = fluid_particle_index;
                            }
                        }
                    } else {
                        atomicStore(&fluid_reservations[fluid_particle_index], 0u);
                        atomicStore(&fluid_reservation_owners[fluid_particle_index], 0xffffffffu);
                    }
                }
                fluid_particle_index = fluid_next_particle[fluid_particle_index];
            }
        }
    }
}

fn apply_fluid_plan(cell: u32, reactant: u32) {
    let candidate = candidates[cell];
    fluid_source_cell(cell, candidate.anchor, true);
    if (candidate.partner != candidate.anchor) {
        fluid_source_cell(cell, candidate.partner, true);
    }
}

fn rollback_fluid_plan(cell: u32, reactant: u32) {
    let candidate = candidates[cell];
    let source_cell = select(candidate.anchor, candidate.partner, reactant == 1u);
    fluid_source_cell(cell, source_cell, false);
}
