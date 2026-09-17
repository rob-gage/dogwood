@compute @workgroup_size(64)
fn propagate_pending_cellular_pressure(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    let logical_index: u32 = logical_cell_index_from_active_pressure_workgroup(workgroup.x, local_index);
    if logical_index >= parameters.buffered_cell_count {
        return;
    }
    let cell: vec2<i32> = cellular_pressure_world_cell_from_logical_index(logical_index);
    let index: u32 = cellular_pressure_physical_cell_index_from_world_cell(cell);
    if index == INVALID_PHYSICAL_CELL_INDEX {
        return;
    }
    retained_pressure[index] = vec4<f32>(0.0);
    pressure_a[index] = vec4<f32>(0.0);
    let source: vec4<f32> = pending_pressure_source(index);
    for (var channel: u32 = 0u; channel < 4u; channel++) {
        retained_pressure[index][channel] = source[channel] - calculate_outgoing_cellular_pressure(
        cell,
        channel,
        source[channel]);
        pressure_b[index][channel] = gather_pending_cellular_pressure(cell, channel);
    }
}
// Alternating entry points preserve explicit pressure ping-pong ordering on the CPU
@compute @workgroup_size(64)
fn propagate_cellular_pressure_a(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    propagate_cellular_pressure(
    logical_cell_index_from_active_pressure_workgroup(workgroup.x, local_index),
    true,);
}

// Runs the second ping-pong direction of one pressure propagation step
@compute @workgroup_size(64)
fn propagate_cellular_pressure_b(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    let logical_index: u32 = logical_cell_index_from_active_pressure_workgroup(workgroup.x, local_index);
    if logical_index < parameters.buffered_cell_count {
        let index: u32 = cellular_pressure_physical_cell_index_from_world_cell(
        cellular_pressure_world_cell_from_logical_index(logical_index));
        if index != INVALID_PHYSICAL_CELL_INDEX {
            pending_pressure[index] = vec4<f32>(0.0);
        }
    }
    propagate_cellular_pressure(logical_index, false,);
}

@compute @workgroup_size(64)
fn finalize_cellular_pressure(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    let logical_index: u32 = logical_cell_index_from_active_pressure_workgroup(workgroup.x, local_index);
    if logical_index >= parameters.buffered_cell_count {
        return;
    }
    let cell: vec2<i32> = cellular_pressure_world_cell_from_logical_index(logical_index);
    let index: u32 = cellular_pressure_physical_cell_index_from_world_cell(cell);
    if index == INVALID_PHYSICAL_CELL_INDEX {
        return;
    }
    let current: vec4<f32> = pressure_b[index];
    var load: vec4<f32> = retained_pressure[index];
    for (var channel: u32 = 0u; channel < 4u; channel++) {
        load[channel]   +=
      current[channel] - calculate_outgoing_cellular_pressure(
        cell,
        channel,
        current[channel]);
        load[channel]   += gather_incoming_cellular_pressure(cell, channel, false);
    }
    let material: u32 = effective_pressure_material(index);
    if rigid_owners[index] != 0u && material_form_from_identifier(
      material) == CELLULAR_STATIC_MATERIAL_FORM {
        accumulate_rigid_pressure_damage(index, material, load);
    } else if material_form_from_identifier(material) == CELLULAR_STATIC_MATERIAL_FORM {
        apply_cellular_static_pressure_damage(cell, index, material, load);
    }
    pressure_a[index] = vec4<f32>(0.0);
    pressure_b[index] = vec4<f32>(0.0);
    retained_pressure[index] = vec4<f32>(0.0);
    pending_pressure[index] = vec4<f32>(0.0);
}

// Applies all retained pressure after the fixed propagation budget has been consumed
@compute @workgroup_size(64)
fn apply_retained_cellular_pressure(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    let logical_index: u32 = logical_cell_index_from_active_pressure_workgroup(workgroup.x, local_index);
    if logical_index >= parameters.buffered_cell_count {
        return;
    }
    let cell: vec2<i32> = cellular_pressure_world_cell_from_logical_index(logical_index);
    let index: u32 = cellular_pressure_physical_cell_index_from_world_cell(cell);
    if index == INVALID_PHYSICAL_CELL_INDEX {
        return;
    }
  // Six propagation passes finish in A; the remaining in-flight pressure is retained here
    let load: vec4<f32> = retained_pressure[index] + pressure_a[index];
    let material: u32 = effective_pressure_material(index);
    let form: u32 = material_form_from_identifier(material);
    if rigid_owners[index] != 0u && form == CELLULAR_STATIC_MATERIAL_FORM {
        accumulate_rigid_pressure_damage(index, material, load);
    } else if form == CELLULAR_STATIC_MATERIAL_FORM {
        apply_cellular_static_pressure_damage(cell, index, material, load);
    }
    pressure_a[index] = vec4<f32>(0.0);
    pressure_b[index] = vec4<f32>(0.0);
    retained_pressure[index] = vec4<f32>(0.0);
}

// Damages or fractures one static cell using scalar compression from all four channels
fn apply_cellular_static_pressure_damage(
    cell: vec2<i32>,
    index: u32,
    material: u32,
    load: vec4<f32>,
) {
    let properties: StaticProperties = cellular_static_properties[material_index_from_identifier(material)];
    let compression: f32 = load.x + load.y + load.z + load.w;
    let overload: f32 = compression - properties.pressure_ignore_threshold;
    if overload <= 0.0 {
        return;
    }
    cellular_integrities[index]   -=
    overload * parameters.delta_time * parameters.damage_rate;
    if cellular_integrities[index] > 0.0 {
        return;
    }
    var replacement: u32 = EMPTY_MATERIAL_IDENTIFIER;
    if properties.debris != EMPTY_MATERIAL_IDENTIFIER && cellular_fracture_yield_random_from_world_cell(
      cell,
      parameters.tick) < properties.debris_yield_rate {
        replacement = properties.debris;
    }
    let request_index: u32 = atomicAdd(&material_mutation_request_count[0], 1u);
    if request_index < parameters.buffered_cell_count {
        material_mutation_requests[request_index] = MaterialMutationRequest(
        index,
        0u,
        index,
        material,
        replacement,
        0u,
        0u,
        0u,
        0u);
    }
    cellular_integrities[index] = 0.0;
}

// Retains pressure that the source material or blocked stencil routes cannot transmit
fn propagate_cellular_pressure(logical_index: u32, read_pressure_a: bool) {
    if logical_index >= parameters.buffered_cell_count {
        return;
    }
    let cell: vec2<i32> = cellular_pressure_world_cell_from_logical_index(logical_index);
    let index: u32 = cellular_pressure_physical_cell_index_from_world_cell(cell);
    if index == INVALID_PHYSICAL_CELL_INDEX {
        return;
    }
    let current: vec4<f32> = select(pressure_b[index], pressure_a[index], read_pressure_a);
    var local_retained: vec4<f32> = vec4<f32>(0.0);
    for (var channel: u32 = 0u; channel < 4u; channel++) {
        local_retained[channel] = current[channel] - calculate_outgoing_cellular_pressure(
        cell,
        channel,
        current[channel]);
    }
    retained_pressure[index]   += local_retained;
    var gathered: vec4<f32> = vec4<f32>(0.0);
    for (var channel: u32 = 0u; channel < 4u; channel++) {
        gathered[channel] = gather_incoming_cellular_pressure(cell, channel, read_pressure_a,);
    }
    if read_pressure_a {
        pressure_b[index] = gathered;
    } else {
        pressure_a[index] = gathered;
    }
}

// Calculates the fraction that actually leaves one source through its fixed stencil
fn calculate_outgoing_cellular_pressure(
    cell: vec2<i32>,
    channel: u32,
    value: f32
) -> f32 {
    let source_transmission: f32 = cellular_pressure_transmission_at_physical_cell_index(
      cellular_pressure_physical_cell_index_from_world_cell(cell),);
    if source_transmission <= 0.0 {
        return 0.0;
    }
    var transmitted: f32 = 0.0;
    for (var side: i32 = -1; side <= 1; side++) {
        let destination_cell: vec2<i32> = cell + world_cell_direction_from_pressure_channel(channel) + world_cell_side_offset_from_pressure_channel(channel, side);
        let destination: u32 = cellular_pressure_physical_cell_index_from_world_cell(destination_cell);
        if destination != INVALID_PHYSICAL_CELL_INDEX && cellular_pressure_transmission_at_physical_cell_index(
        destination) > 0.0 {
            let weight: f32 = select(0.2, 0.6, side == 0);
            transmitted   +=
        value * min(
            source_transmission,
            cellular_pressure_transmission_at_physical_cell_index(destination)) * weight;
        }
    }
    return transmitted;
}

// Gathers only valid source-routed pressure into one destination cell
fn gather_incoming_cellular_pressure(
    cell: vec2<i32>,
    channel: u32,
    read_pressure_a: bool,
) -> f32 {
    if cellular_pressure_transmission_at_physical_cell_index(
      cellular_pressure_physical_cell_index_from_world_cell(cell)) <= 0.0 {
        return 0.0;
    }
    var gathered: f32 = 0.0;
    for (var side: i32 = -1; side <= 1; side++) {
        let source_cell: vec2<i32> = cell - world_cell_direction_from_pressure_channel(channel) - world_cell_side_offset_from_pressure_channel(channel, side);
        let source: u32 = cellular_pressure_physical_cell_index_from_world_cell(source_cell);
        if source == INVALID_PHYSICAL_CELL_INDEX {
            continue;
        }
        let source_transmission: f32 = cellular_pressure_transmission_at_physical_cell_index(source);
        if source_transmission <= 0.0 {
            continue;
        }
        let source_pressure: f32 = select(
        pressure_b[source][channel],
        pressure_a[source][channel],
        read_pressure_a,);
        let weight: f32 = select(0.2, 0.6, side == 0);
        gathered   +=
      source_pressure * min(
          source_transmission,
          cellular_pressure_transmission_at_physical_cell_index(
            cellular_pressure_physical_cell_index_from_world_cell(cell))) * weight;
    }
    return gathered;
}

fn pending_pressure_source(index: u32) -> vec4<f32> {
    var source: vec4<f32> = pending_pressure[index];
    if external_body_occupancy[index] == 1u || external_body_occupancy[index] == 2u {
        source   += encode_directional_pressure(external_body_velocity[index].zw);
    }
    return source;
}

fn gather_pending_cellular_pressure(cell: vec2<i32>, channel: u32) -> f32 {
    let destination: u32 = cellular_pressure_physical_cell_index_from_world_cell(cell);
    let destination_transmission: f32 = cellular_pressure_transmission_at_physical_cell_index(destination);
    if destination_transmission <= 0.0 {
        return 0.0;
    }
    var gathered: f32 = 0.0;
    for (var side: i32 = -1; side <= 1; side++) {
        let source_cell: vec2<i32> = cell - world_cell_direction_from_pressure_channel(channel) - world_cell_side_offset_from_pressure_channel(channel, side);
        let source: u32 = cellular_pressure_physical_cell_index_from_world_cell(source_cell);
        if source == INVALID_PHYSICAL_CELL_INDEX {
            continue;
        }
        let source_transmission: f32 = cellular_pressure_transmission_at_physical_cell_index(source);
        if source_transmission <= 0.0 {
            continue;
        }
        let weight: f32 = select(0.2, 0.6, side == 0);
        gathered   +=
      pending_pressure_source(source)[channel] * min(source_transmission, destination_transmission) * weight;
    }
    return gathered;
}

// Returns source-material transmission, with transient body cells effectively perfect
fn cellular_pressure_transmission_at_physical_cell_index(index: u32) -> f32 {
    if index == INVALID_PHYSICAL_CELL_INDEX {
        return 0.0;
    }
    if external_body_occupancy[index] != 0u {
        return 1.0;
    }
    if mechanical_fluid_cells[index].mass > 0.0 {
        return
      fluid_pressure_properties[material_index_from_identifier(
        mechanical_fluid_cells[index].material_identifier)].x;
    }
    let material: u32 = effective_pressure_material(index);
    let form: u32 = material_form_from_identifier(material);
    if form == CELLULAR_STATIC_MATERIAL_FORM {
        return
      cellular_static_properties[material_index_from_identifier(
        material)].pressure_transmission;
    }
    if form == CELLULAR_DYNAMIC_MATERIAL_FORM {
        return
      cellular_dynamic_properties[material_index_from_identifier(material)].y;
    }
    return 0.0;
}

// Returns the friction coefficient for one cellular contact medium
fn cellular_contact_friction_at_physical_cell_index(index: u32) -> f32 {
    if index == INVALID_PHYSICAL_CELL_INDEX || external_body_occupancy[index] != 0u {
        return 0.0;
    }
    let material: u32 = cellular_material_identifiers[index];
    let form: u32 = material_form_from_identifier(material);
    if form == CELLULAR_STATIC_MATERIAL_FORM {
        return
      cellular_static_properties[material_index_from_identifier(
        material)].friction;
    }
    if form == CELLULAR_DYNAMIC_MATERIAL_FORM {
        return
      cellular_dynamic_properties[material_index_from_identifier(material)].z;
    }
    return 0.0;
}

// Returns the restitution coefficient for one cellular contact medium
fn cellular_material_restitution_at_physical_cell_index(index: u32) -> f32 {
    if index == INVALID_PHYSICAL_CELL_INDEX || external_body_occupancy[index] != 0u {
        return 0.0;
    }
    let material: u32 = cellular_material_identifiers[index];
    let form: u32 = material_form_from_identifier(material);
    if form == CELLULAR_STATIC_MATERIAL_FORM {
        return
      cellular_static_properties[material_index_from_identifier(
        material)].restitution;
    }
    if form == CELLULAR_DYNAMIC_MATERIAL_FORM {
        return
      cellular_dynamic_properties[material_index_from_identifier(material)].w;
    }
    return 0.0;
}

// An immovable body uses the cellular material's restitution rather than suppressing bounce
fn cellular_contact_restitution(first_index: u32, second_index: u32) -> f32 {
    if external_body_occupancy[first_index] != 0u {
        return cellular_material_restitution_at_physical_cell_index(second_index);
    }
    if external_body_occupancy[second_index] != 0u {
        return cellular_material_restitution_at_physical_cell_index(first_index);
    }
    return
    min(
      cellular_material_restitution_at_physical_cell_index(first_index),
      cellular_material_restitution_at_physical_cell_index(second_index),);
}

// Returns dynamic mass or the immovable mass used for static and proxy cells
fn cellular_contact_mass_at_physical_cell_index(index: u32) -> f32 {
    if index != INVALID_PHYSICAL_CELL_INDEX && material_form_from_identifier(
        cellular_material_identifiers[index]) == CELLULAR_DYNAMIC_MATERIAL_FORM && external_body_occupancy[index] == 0u {
        return
      cellular_dynamic_properties[material_index_from_identifier(
        cellular_material_identifiers[index])].x;
    }
    return IMMOVABLE_CONTACT_MASS;
}

// Channel order is +X, -X, +Y, -Y so opposing compression cannot cancel
fn encode_directional_pressure(value: vec2<f32>) -> vec4<f32> {
    return
    vec4<f32>(
      max(value.x, 0.0),
      max(-value.x, 0.0),
      max(value.y, 0.0),
      max(-value.y, 0.0),);
}
