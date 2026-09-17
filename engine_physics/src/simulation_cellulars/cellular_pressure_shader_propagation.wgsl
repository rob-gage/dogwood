}

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

fn find_rigid_static_contact(source: u32) -> RigidStaticContact {
    if source >= parameters.rigid_cell_count {
        return empty_rigid_static_contact();
    }
    let cell: vec4<u32> = rigid_cells[source * 2u];
    let body: u32 = cell.z;
    if body >= parameters.rigid_body_count {
        return empty_rigid_static_contact();
    }
    let pose: vec4<f32> = rigid_transforms[body * 3u];
    let motion: vec4<f32> = rigid_transforms[body * 3u + 1u];
    let local_center: vec2<f32> = (vec2<f32>(bitcast<vec2<i32>>(cell.xy)) + vec2<f32>(0.5)) * CELL_SIZE;
    let axis_x: vec2<f32> = pose.zw;
    let axis_y: vec2<f32> = vec2<f32>(-axis_x.y, axis_x.x);
    let current: vec2<f32> = pose.xy + axis_x * local_center.x + axis_y * local_center.y;
    let angle_step: f32 = clamp(motion.z * parameters.delta_time, -3.14159265, 3.14159265);
    let previous_axis_x: vec2<f32> = vec2<f32>(
      axis_x.x * cos(angle_step) + axis_x.y * sin(angle_step),
      -axis_x.x * sin(angle_step) + axis_x.y * cos(angle_step));
    let previous_axis_y: vec2<f32> = vec2<f32>(-previous_axis_x.y, previous_axis_x.x);
    let previous_unbounded: vec2<f32> = pose.xy - motion.xy * parameters.delta_time + previous_axis_x * local_center.x + previous_axis_y * local_center.y;
    let sweep: vec2<f32> = current - previous_unbounded;
    let sweep_scale: f32 = min(1.0, 2.0 / max(length(sweep), 0.0001));
    let previous: vec2<f32> = current - sweep * sweep_scale;
    let rotational_expansion: f32 = min(CELL_SIZE * 2.0, abs(angle_step) * length(local_center));
    let extent: f32 = CELL_RADIUS + rotational_expansion;
    let minimum: vec2<i32> = vec2<i32>(floor((min(previous, current) - vec2<f32>(extent)) * 8.0));
    let maximum: vec2<i32> = vec2<i32>(floor((max(previous, current) + vec2<f32>(extent)) * 8.0));
    var best: RigidStaticContact = empty_rigid_static_contact();
    var best_time: f32 = 2.0;
    var best_overlap: RigidStaticContact = empty_rigid_static_contact();
    var best_blocking: f32 = -1.0;
    let mass_record: vec4<f32> = rigid_transforms[body * 3u + 2u];
    let radius: vec2<f32> = current - mass_record.xy;
    let inward_motion: vec2<f32> = motion.xy + motion.z * vec2<f32>(-radius.y, radius.x) + parameters.gravity * parameters.delta_time;
    for (var y: i32 = minimum.y; y <= maximum.y; y++) {
        for (var x: i32 = minimum.x; x <= maximum.x; x++) {
            let world_cell: vec2<i32> = vec2<i32>(x, y);
            let index: u32 = cellular_pressure_physical_cell_index_from_world_cell(world_cell);
            if index == INVALID_PHYSICAL_CELL_INDEX {
                continue;
            }
            let material: u32 = cellular_material_identifiers[index];
            if material_form_from_identifier(material) != CELLULAR_STATIC_MATERIAL_FORM {
                continue;
            }
            let static_center: vec2<f32> = (vec2<f32>(world_cell) + vec2<f32>(0.5)) * CELL_SIZE;
            let overlap: vec4<f32> = rigid_static_overlap_contact(current, axis_x, axis_y, static_center);
            if overlap.w >= 0.0 {
                let normal: vec2<f32> = overlap.xy;
                let blocking: f32 = max(0.0, -dot(inward_motion, normal));
                if blocking > best_blocking || (blocking == best_blocking && overlap.w > best_overlap.penetration) {
                    best_blocking = blocking;
                    best_overlap = RigidStaticContact(
              1u,
              body,
              material,
              dominant_cardinal_channel(normal),
              index,
              normal,
              current - axis_x * sign(dot(axis_x, normal)) * CELL_HALF - axis_y * sign(dot(axis_y, normal)) * CELL_HALF,
              overlap.w);
                }
                continue;
            }
            let hit: vec4<f32> = rigid_static_swept_contact(previous, current, static_center, extent);
            let time: f32 = hit.z;
            if hit.w >= 0.0 && time < best_time {
                best_time = time;
                let normal: vec2<f32> = hit.xy;
                let point_center: vec2<f32> = mix(previous, current, time);
                best = RigidStaticContact(
            1u,
            body,
            material,
            dominant_cardinal_channel(normal),
            index,
            normal,
            point_center - axis_x * sign(dot(axis_x, normal)) * CELL_HALF - axis_y * sign(dot(axis_y, normal)) * CELL_HALF,
            hit.w);
            }
        }
    }
    if best_overlap.found != 0u {
        return best_overlap;
    }
    return best;
}

fn rigid_static_overlap_contact(
    center: vec2<f32>,
    axis_x: vec2<f32>,
    axis_y: vec2<f32>,
    static_center: vec2<f32>,
) -> vec4<f32> {
    let difference: vec2<f32> = center - static_center;
    var best_axis: vec2<f32> = vec2<f32>(0.0);
    var best_penetration: f32 = 3.402823e+38;
    let axes: array<vec2<f32>, 4> = array<vec2<f32>, 4>(
      vec2<f32>(1.0, 0.0),
      vec2<f32>(0.0, 1.0),
      axis_x,
      axis_y);
    for (var index: u32 = 0u; index < 4u; index++) {
        let axis: vec2<f32> = axes[index];
        let rigid_radius: f32 = CELL_HALF * (abs(dot(axis_x, axis)) + abs(dot(axis_y, axis)));
        let static_radius: f32 = CELL_HALF * (abs(axis.x) + abs(axis.y));
        let penetration: f32 = rigid_radius + static_radius - abs(dot(difference, axis));
        if penetration < 0.0 {
            return vec4<f32>(0.0, 0.0, 0.0, -1.0);
        }
        if penetration < best_penetration {
            best_penetration = penetration;
            best_axis = select(-axis, axis, dot(difference, axis) >= 0.0);
        }
    }
    return vec4<f32>(best_axis, 0.0, best_penetration);
}

fn rigid_static_swept_contact(
    start: vec2<f32>,
    finish: vec2<f32>,
    static_center: vec2<f32>,
    extent: f32,
) -> vec4<f32> {
    let movement: vec2<f32> = finish - start;
    let minimum: vec2<f32> = static_center - vec2<f32>(CELL_HALF + extent);
    let maximum: vec2<f32> = static_center + vec2<f32>(CELL_HALF + extent);
    var enter: f32 = 0.0;
    var leave: f32 = 1.0;
    var normal: vec2<f32> = vec2<f32>(0.0);
    for (var axis: u32 = 0u; axis < 2u; axis++) {
        if abs(movement[axis]) <= 0.0001 {
            if start[axis] < minimum[axis] || start[axis] > maximum[axis] {
                return vec4<f32>(0.0, 0.0, 2.0, -1.0);
            }

            continue;
        }
        let first: f32 = (minimum[axis] - start[axis]) / movement[axis];
        let second: f32 = (maximum[axis] - start[axis]) / movement[axis];
        let axis_enter: f32 = min(first, second);
        let axis_leave: f32 = max(first, second);
        if axis_enter > enter {
            enter = axis_enter;
            normal = vec2<f32>(0.0);
            normal[axis] = select(1.0, -1.0, movement[axis] > 0.0);
        }
        leave = min(leave, axis_leave);
    }
    if enter > leave || enter < 0.0 || enter > 1.0 {
        return vec4<f32>(0.0, 0.0, 2.0, -1.0);
    }
    return vec4<f32>(normal, enter, 0.0);
}

fn empty_rigid_static_contact() -> RigidStaticContact {
    return
    RigidStaticContact(
      0u,
      0u,
      0u,
      0u,
      0u,
      vec2<f32>(0.0),
      vec2<f32>(0.0),
      -1.0);
}

fn accumulate_rigid_sweep_reaction(
    body: u32,
    impulse: vec2<f32>,
    radius: vec2<f32>
) {
    let torque: f32 = radius.x * impulse.y - radius.y * impulse.x;
    if any(impulse != impulse) || any(abs(impulse) > vec2<f32>(1000000.0)) || abs(torque) > 1000000.0 {
        atomicStore(&rigid_reactions[body].overflow, 1);
        return;
    }
    var overflowed: bool = saturating_rigid_atomic_add(
      body,
      i32(round(impulse.x * LINEAR_FIXED_SCALE)),
      0u);
    overflowed = saturating_rigid_atomic_add(
      body,
      i32(round(impulse.y * LINEAR_FIXED_SCALE)),
      1u) || overflowed;
    overflowed = saturating_rigid_atomic_add(
      body,
      i32(round(torque * ANGULAR_FIXED_SCALE)),
      2u) || overflowed;
    if overflowed {
        atomicStore(&rigid_reactions[body].overflow, 1);
    }
}

fn dominant_cardinal_channel(normal: vec2<f32>) -> u32 {
    if abs(normal.x) >= abs(normal.y) {
        return select(1u, 0u, normal.x >= 0.0);
    }
    return select(3u, 2u, normal.y >= 0.0);
}

fn saturating_rigid_atomic_add(
    body: u32,
    value: i32,
    accumulator: u32
) -> bool {
    let maximum: i32 = bitcast<i32>(0x7fffffffu);
    let minimum: i32 = bitcast<i32>(0x80000000u);
    var old: i32 = 0;
    if accumulator == 0u {
        old = atomicLoad(&rigid_reactions[body].impulse_x);
    } else if accumulator == 1u {
        old = atomicLoad(&rigid_reactions[body].impulse_y);
    } else {
        old = atomicLoad(&rigid_reactions[body].angular_impulse);
    }
        loop {
            var next: i32 = 0;
            var overflowed: bool = false;
            if value > 0 && old > maximum - value {
                next = maximum;
                overflowed = true;
            } else if value < 0 && old < minimum - value {
                next = minimum;
                overflowed = true;
            } else {
                next = old + value;
            }
            if accumulator == 0u {
                let result = atomicCompareExchangeWeak(&rigid_reactions[body].impulse_x, old, next);
                if result.exchanged {
                    return overflowed;
                }
                old = result.old_value;
            } else if accumulator == 1u {
                let result = atomicCompareExchangeWeak(&rigid_reactions[body].impulse_y, old, next);
                if result.exchanged {
                    return overflowed;
                }
                old = result.old_value;
            } else {
                let result = atomicCompareExchangeWeak(
          &rigid_reactions[body].angular_impulse,
          old,
          next);
                if result.exchanged {
                    return overflowed;
                }
                old = result.old_value;
            }
        }
    return false;
}

// Returns the world-cell direction represented by one pressure channel
fn world_cell_direction_from_pressure_channel(channel: u32) -> vec2<i32> {
    if channel == 0u {
        return vec2<i32>(1, 0);
    }
    if channel == 1u {
        return vec2<i32>(-1, 0);
    }
    if channel == 2u {
        return vec2<i32>(0, 1);
    }
    return vec2<i32>(0, -1);
}

// Returns the lateral stencil offset for one directional pressure channel
fn world_cell_side_offset_from_pressure_channel(
    channel: u32,
    side: i32
) -> vec2<i32> {
    if channel < 2u {
        return vec2<i32>(0, side);
    }
    return vec2<i32>(side, 0);
}

// Converts logical tile-major dispatch order into a signed world cell
fn cellular_pressure_world_cell_from_logical_index(logical_index: u32) -> vec2<i32
> {
    return
    world_cell_from_logical_tile_major_index(
      logical_index,
      parameters.buffered_origin,
      parameters.buffered_tile_size,);
}

// Maps one signed world cell through the two-dimensional physical tile ring
fn cellular_pressure_physical_cell_index_from_world_cell(
    world_cell: vec2<i32>
) -> u32 {
    return
    physical_cell_index_from_world_cell(
      world_cell,
      parameters.buffered_origin,
      parameters.buffered_tile_size,
      parameters.ring_offset,);
}

// Maps one compacted active tile workgroup to its tile-major logical cell
fn logical_cell_index_from_active_pressure_workgroup(
    workgroup: u32,
    local_index: u32
) -> u32 {
    return
    active_pressure_tile_indices[workgroup] * CELL_COUNT_PER_TILE + local_index;
}

// Produces a deterministic per-cell random value for fracture yield decisions
fn cellular_fracture_yield_random_from_world_cell(
    cell: vec2<i32>,
    tick: u32
) -> f32 {
    var value: u32 = u32(cell.x) * 0x9e3779b9u ^ u32(cell.y) * 0x85ebca6bu ^ tick;
    value   ^= value >> 16u;
    return f32(value) / 4294967295.0;
}

