fn phase_cells(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if
        (invocation.x >= parameters.cell_count || atomicLoad(
            &rigid_claims[invocation.x],
        ) != 0xffffffffu)
    {
        return;
    }
    let w =
        world_cell_from_physical_tile_ring_index(
            invocation.x,
            parameters.origin,
            parameters.tiles,
            parameters.ring,
        );
    transition(
        cells[invocation.x],
        amounts[invocation.x],
        temperatures[invocation.x],
        invocation.x,
        0u,
        invocation.x,
        w,
    );
}

@compute @workgroup_size(64)
fn phase_rigid(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if (invocation.x >= parameters.rigid_count || invocation.x >= arrayLength(&rigid_cells)) {
        return;
    }
    let rigid = rigid_cells[invocation.x];
    let slot = rigid.state_slot;
    if (slot >= arrayLength(&rigid_amounts) || slot >= arrayLength(&rigid_temperatures)) {
        return;
    }
    let amount = rigid_amounts[slot];
    let temperature = rigid_temperatures[slot];
    let source = rigid.material_identifier;
    if (!(amount >= 0.999 && amount <= 1.001) || !(temperature == temperature)) {
        return;
    } // TODO fractional rigid inventory needs a non-particle-fluid product representation.
    let dense = material_dense_index(source, thermal.offsets, thermal.counts);
    if (dense == 0xffffffffu || dense >= arrayLength(&properties)) {
        return;
    }
    let record = properties[dense];
    let cp = thermal_material_specific_heat_capacity(record);
    if (!(cp > 0.000001)) {
        return;
    }
    var replacement = EMPTY_MATERIAL_IDENTIFIER;
    var threshold = 0.0;
    var latent = 0.0;
    var rate = 0.0;
    var signed = 0.0;
    if
        (thermal_material_hot_enabled(record) && temperature >= thermal_material_hot_threshold(
            record,
        ))
    {
        replacement = thermal_material_hot_target(record);
        threshold = thermal_material_hot_threshold(record);
        latent = thermal_material_hot_latent_energy(record);
        rate = thermal_material_hot_yield(record);
        signed = 1.0;
    } else if
        (thermal_material_cold_enabled(record) && temperature <= thermal_material_cold_threshold(
            record,
        ))
    {
        replacement = thermal_material_cold_target(record);
        threshold = thermal_material_cold_threshold(record);
        latent = thermal_material_cold_latent_energy(record);
        rate = thermal_material_cold_yield(record);
        signed = -1.0;
    } else {
        return;
    }
    let td = material_dense_index(replacement, thermal.offsets, thermal.counts);
    if
        (replacement == EMPTY_MATERIAL_IDENTIFIER
            || replacement == source
            || td == 0xffffffffu
            || td >= arrayLength(&properties)
            || material_form_from_identifier(replacement) != FLUID_MATERIAL_FORM)
    {
        return;
    }
    let tcp = thermal_material_specific_heat_capacity(properties[td]);
    if (!(tcp > 0.000001)) {
        return;
    }
    let sensible = amount * cp * max(signed * (temperature - threshold), 0.0);
    let required = amount * max(latent, 0.0);
    if (latent > 0.0 && sensible < required || !should_yield(rate, slot, source)) {
        return;
    }
    let reserved = reserve_fluid_particle();
    if (reserved == 0xffffffffu) {
        return;
    }
    let n = atomicAdd(&rigid_phase_count[0], 1u);
    if (n >= arrayLength(&rigid_phase_candidates)) {
        release_reserved_fluid_particle(reserved);
        return;
    }
    rigid_phase_candidates[n] =
        RigidPhaseCandidate(
            slot,
            rigid.state_generation,
            source,
            replacement,
            amount,
            max(threshold + signed * max(sensible - required, 0.0) / (amount * tcp), 0.0),
            rigid.body,
            rigid.local.x,
            rigid.local.y,
            reserved,
        );
}

@compute @workgroup_size(64)
fn phase_particles(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if (invocation.x >= parameters.particle_count) {
        return;
    }
    let p = particles[invocation.x];
    if (p.is_active == 0u || p.material_identifier == EMPTY_MATERIAL_IDENTIFIER) {
        return;
    }
    let world = vec2<i32>(floor(p.position * CELLS_PER_TILE_FLOAT));
    let cell =
        physical_cell_index_from_world_cell(
            world,
            parameters.origin,
            parameters.tiles,
            parameters.ring,
        );
    if (cell == 0xffffffffu) {
        return;
    }
    transition(p.material_identifier, p.amount, p.temperature, cell, 1u, invocation.x, world);
}

@compute @workgroup_size(64)
fn phase_gases(
    @builtin(local_invocation_id) local: vec3<u32>,
    @builtin(workgroup_id) group: vec3<u32>,
) {
    let cell = group.x * 64u + local.x;
    let species = group.y;
    let branch = group.z;
    let slot = (branch * parameters.gas_count + species) * parameters.cell_count + cell;
    gas_fluid_candidates[slot] =
        GasFluidCandidate(EMPTY_MATERIAL_IDENTIFIER, 0.0, 0.0, vec2<f32>(0.0));
    if (cell >= parameters.cell_count || species >= parameters.gas_count) {
        return;
    }
    let source = species + 1u;
    let amount = concentrations[species * parameters.cell_count + cell];
    let temperature = gas_temperatures[cell];
    if (!(amount > 0.000001) || temperature != temperature || abs(temperature) > 3.4e38) {
        return;
    }
    let dense = material_dense_index(source, thermal.offsets, thermal.counts);
    if (dense == 0xffffffffu || dense >= arrayLength(&properties)) {
        return;
    }
    let record = properties[dense];
    let cp = thermal_material_specific_heat_capacity(record);
    if (!(cp > 0.000001)) {
        return;
    }
    var replacement = EMPTY_MATERIAL_IDENTIFIER;
    var threshold = 0.0;
    var latent = 0.0;
    var rate = 0.0;
    var signed = 0.0;
    if (branch == 0u) {
        if
            (!(thermal_material_hot_enabled(
                record,
            ) && temperature >= thermal_material_hot_threshold(record)))
        {
            return;
        }
        replacement = thermal_material_hot_target(record);
        threshold = thermal_material_hot_threshold(record);
        latent = thermal_material_hot_latent_energy(record);
        rate = thermal_material_hot_yield(record);
        signed = 1.0;
    } else {
        if
            (thermal_material_hot_enabled(record) && temperature >= thermal_material_hot_threshold(
                record,
            ))
        {
            return;
        }
        if
            (!(thermal_material_cold_enabled(
                record,
            ) && temperature <= thermal_material_cold_threshold(record)))
        {
            return;
        }
        replacement = thermal_material_cold_target(record);
        threshold = thermal_material_cold_threshold(record);
        latent = thermal_material_cold_latent_energy(record);
        rate = thermal_material_cold_yield(record);
        signed = -1.0;
    }
    let target_dense = material_dense_index(replacement, thermal.offsets, thermal.counts);
    if
        (replacement == EMPTY_MATERIAL_IDENTIFIER
            || replacement == source
            || target_dense == 0xffffffffu
            || target_dense >= arrayLength(&properties))
    {
        return;
    }
    let target_cp = thermal_material_specific_heat_capacity(properties[target_dense]);
    if (!(target_cp > 0.000001)) {
        return;
    }
    let sensible = amount * cp * max(signed * (temperature - threshold), 0.0);
    let required = amount * max(latent, 0.0);
    if (latent > 0.0 && sensible < required) {
        return;
    }
    if (!should_yield(rate, cell, source)) {
        return;
    }
    let target_temperature =
        max(threshold + signed * max(sensible - required, 0.0) / (amount * target_cp), 0.0);
    let world =
        world_cell_from_physical_tile_ring_index(
            cell,
            parameters.origin,
            parameters.tiles,
            parameters.ring,
        );
    let position = (vec2<f32>(world) + vec2<f32>(0.5)) / CELLS_PER_TILE_FLOAT;
    if (material_form_from_identifier(replacement) == FLUID_MATERIAL_FORM) {
        gas_fluid_candidates[slot] =
            GasFluidCandidate(replacement, amount, target_temperature, position);
        return;
    }
    let n = atomicAdd(&request_count[0], 1u);
    if (n < arrayLength(&requests)) {
        requests[n] =
            Request(
                cell,
                2u,
                species,
                source,
                replacement,
                bitcast<u32>(amount),
                bitcast<u32>(target_temperature),
                bitcast<u32>(position.x),
                bitcast<u32>(position.y),
            );
    }
}
