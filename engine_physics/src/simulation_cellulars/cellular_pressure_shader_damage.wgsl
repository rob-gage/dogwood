fn effective_pressure_material(cellular_pressure_cell_index: u32) -> u32 {
    return
        select(
            cellular_material_identifiers[cellular_pressure_cell_index],
            rigid_material_identifiers[cellular_pressure_cell_index],
            rigid_owners[cellular_pressure_cell_index] != 0u,
        );
}

fn rigid_cell_state_slot(source: u32) -> u32 {
    if source >= arrayLength(&rigid_cells) / 2u {
        return 0xffffffffu;
    }
    return rigid_cells[source * 2u + 1u].y;
}

fn accumulate_rigid_pressure_damage(cellular_pressure_cell_index: u32, material: u32, load: vec4<f32>) {
    let source: u32 = rigid_claims[cellular_pressure_cell_index];
    if source == 0xffffffffu {
        return;
    }
    let slot: u32 = rigid_cell_state_slot(source);
    if slot >= arrayLength(&rigid_damage) {
        return;
    }
    let properties = cellular_static_properties[material_index_from_identifier(material)];
    let overload =
        max(0.0, load.x + load.y + load.z + load.w - properties.pressure_ignore_threshold);
    atomicMax(&rigid_damage[slot], bitcast<u32>(overload));
    if overload > 0.0 {
        atomicStore(&rigid_damage_dispatch[0], (cellular_pressure_parameters.rigid_cell_count + 63u) / 64u);
        atomicStore(&rigid_damage_dispatch[1], 1u);
        atomicStore(&rigid_damage_dispatch[2], 1u);
    }
}

@compute @workgroup_size(64)
fn apply_rigid_pressure_damage(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if invocation.x >= cellular_pressure_parameters.rigid_cell_count {
        return;
    }
    let slot = rigid_cell_state_slot(invocation.x);
    if slot >= arrayLength(&rigid_damage) {
        return;
    }
    let overload = bitcast<f32>(atomicExchange(&rigid_damage[slot], 0u));
    if overload == 0.0 {
        return;
    }
    rigid_cell_integrities[slot] -= overload * cellular_pressure_parameters.delta_time * cellular_pressure_parameters.damage_rate;
    if rigid_cell_integrities[slot] <= 0.0 {
        let mask = 1u << (slot % 32u);
        if (atomicOr(&rigid_fractures[slot / 32u], mask) & mask) == 0u {
            atomicAdd(&rigid_fracture_count, 1u);
        }
    }
}

@compute @workgroup_size(64)
fn initialize_rigid_contact_state(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let body: u32 = invocation.x;
    if body >= cellular_pressure_parameters.rigid_body_count {
        return;
    }
    atomicStore(&rigid_contact_statistics[body].contact_count, 0u);
    atomicStore(&rigid_contact_statistics[body].static_contact_count, 0u);
    atomicStore(&rigid_contact_statistics[body].padding_1, 0u);
    atomicStore(&rigid_contact_statistics[body].padding_2, 0u);
    let motion: vec4<f32> = rigid_transforms[body * 3u + 1u];
    rigid_reactions[body].source_motion = motion.xyz;
    atomicStore(&rigid_predicted_motion[body].x, i32(round(motion.x * 4096.0)));
    atomicStore(&rigid_predicted_motion[body].y, i32(round(motion.y * 4096.0)));
    atomicStore(&rigid_predicted_motion[body].angular, i32(round(motion.z * 4096.0)));
    for (var channel: u32 = 0u; channel < 4u; channel++) {
        atomicStore(&rigid_contact_statistics[body].geometric_support[channel], 0u);
        atomicStore(&rigid_contact_statistics[body].motion_support[channel], 0u);
    }
}

@compute @workgroup_size(64)
fn gather_rigid_static_contacts(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let contact: RigidStaticContact = find_rigid_static_contact(invocation.x);
    if contact.found == 0u {
        return;
    }
    process_rigid_cellular_contact(
        contact.body,
        rigid_cells[invocation.x * 2u].w,
        contact.cell_index,
        -contact.normal,
        contact.point,
        contact.penetration,
        true,
    );
}

@compute @workgroup_size(64)
fn resolve_rigid_static_contacts(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let contact: RigidStaticContact = find_rigid_static_contact(invocation.x);
    if contact.found == 0u {
        return;
    }
    process_rigid_cellular_contact(
        contact.body,
        rigid_cells[invocation.x * 2u].w,
        contact.cell_index,
        -contact.normal,
        contact.point,
        contact.penetration,
        false,
    );
}
