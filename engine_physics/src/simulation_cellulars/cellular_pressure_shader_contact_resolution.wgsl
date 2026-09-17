fn process_cellular_face(
    workgroup: u32,
    local_index: u32,
    horizontal: bool,
    parity: i32,
    gather: bool,
) {
    let logical_index: u32 = logical_cell_index_from_active_pressure_workgroup(workgroup, local_index);
    if logical_index >= parameters.buffered_cell_count {
        return;
    }
    let cell: vec2<i32> = cellular_pressure_world_cell_from_logical_index(logical_index);
    if (select(cell.y, cell.x, horizontal) & 1) != parity {
        return;
    }
    let direction: vec2<i32> = select(vec2<i32>(0, 1), vec2<i32>(1, 0), horizontal);
    let first: u32 = cellular_pressure_physical_cell_index_from_world_cell(cell);
    let second: u32 = cellular_pressure_physical_cell_index_from_world_cell(cell + direction);

    if first == INVALID_PHYSICAL_CELL_INDEX || second == INVALID_PHYSICAL_CELL_INDEX {
        return;
    }
    let first_material: u32 = cellular_material_identifiers[first];
    let second_material: u32 = cellular_material_identifiers[second];
    let first_form: u32 = material_form_from_identifier(first_material);
    let second_form: u32 = material_form_from_identifier(second_material);
    let owner_first: u32 = rigid_owners[first];
    let owner_second: u32 = rigid_owners[second];
    if owner_first != 0u && owner_second == 0u {
        process_rigid_cellular_face(
      first,
      second,
      vec2<f32>(direction),
      (vec2<f32>(cell) + vec2<f32>(0.5) + vec2<f32>(direction) * 0.5) / 8.0,
      gather);
    }
    if owner_second != 0u && owner_first == 0u {
        process_rigid_cellular_face(
      second,
      first,
      -vec2<f32>(direction),
      (vec2<f32>(cell) + vec2<f32>(0.5) + vec2<f32>(direction) * 0.5) / 8.0,
      gather);
    }
    if gather {
        return;
    }
    if external_body_occupancy[first] != 0u || external_body_occupancy[second] != 0u {
        return;
    }
    if (mechanical_fluid_cells[first].mass > 0.0 && second_form == CELLULAR_STATIC_MATERIAL_FORM) || (mechanical_fluid_cells[first].mass > 0.0 && second_form == CELLULAR_DYNAMIC_MATERIAL_FORM) {
        resolve_fluid_cellular_face(first, second, vec2<f32>(direction));
    }
    if (mechanical_fluid_cells[second].mass > 0.0 && first_form == CELLULAR_STATIC_MATERIAL_FORM) || (mechanical_fluid_cells[second].mass > 0.0 && first_form == CELLULAR_DYNAMIC_MATERIAL_FORM) {
        resolve_fluid_cellular_face(second, first, -vec2<f32>(direction));
    }
    if gas_cell_is_open(first) {
        if second_form == CELLULAR_STATIC_MATERIAL_FORM || second_form == CELLULAR_DYNAMIC_MATERIAL_FORM {
            resolve_gas_cellular_face(first, second, vec2<f32>(direction));
        } else if mechanical_fluid_cells[second].mass > 0.0 {
            resolve_gas_fluid_face(first, second, vec2<f32>(direction));
        }
    }
    if gas_cell_is_open(second) {
        if first_form == CELLULAR_STATIC_MATERIAL_FORM || first_form == CELLULAR_DYNAMIC_MATERIAL_FORM {
            resolve_gas_cellular_face(second, first, -vec2<f32>(direction));
        } else if mechanical_fluid_cells[first].mass > 0.0 {
            resolve_gas_fluid_face(second, first, -vec2<f32>(direction));
        }
    }
    let first_dynamic: bool = first_form == CELLULAR_DYNAMIC_MATERIAL_FORM;
    let second_dynamic: bool = second_form == CELLULAR_DYNAMIC_MATERIAL_FORM;
    if (!first_dynamic && first_form != CELLULAR_STATIC_MATERIAL_FORM) || (!second_dynamic && second_form != CELLULAR_STATIC_MATERIAL_FORM) || (!first_dynamic && !second_dynamic) {
        return;
    }
    let first_inverse_mass: f32 = select(
      0.0,
      1.0 / max(cellular_contact_mass_at_physical_cell_index(first), 0.000001),
      first_dynamic);
    let second_inverse_mass: f32 = select(
      0.0,
      1.0 / max(cellular_contact_mass_at_physical_cell_index(second), 0.000001),
      second_dynamic);
    let inverse_mass_sum: f32 = first_inverse_mass + second_inverse_mass;
    if inverse_mass_sum <= 0.0 {
        return;
    }
    let normal: vec2<f32> = vec2<f32>(direction);
    let relative: vec2<f32> = cellular_kinematics[first].xy - cellular_kinematics[second].xy;
    let approach: f32 = dot(relative, normal);
    if approach <= 0.0001 {
        return;
    }
    let restitution: f32 = clamp(cellular_contact_restitution(first, second), 0.0, 1.0);
    let normal_impulse: f32 = (1.0 + restitution) * approach / inverse_mass_sum;
    let tangent: vec2<f32> = vec2<f32>(-normal.y, normal.x);
    let friction: f32 = max(
      0.0,
      min(
        cellular_contact_friction_at_physical_cell_index(first),
        cellular_contact_friction_at_physical_cell_index(second),));
    let tangent_impulse: f32 = clamp(
      dot(relative, tangent) / inverse_mass_sum,
      -friction * normal_impulse,
      friction * normal_impulse,);
    let impulse: vec2<f32> = normal * normal_impulse + tangent * tangent_impulse;
    if first_dynamic {
        cellular_kinematics[first].x   -= impulse.x * first_inverse_mass;
        cellular_kinematics[first].y   -= impulse.y * first_inverse_mass;
    }
    if second_dynamic {
        cellular_kinematics[second].x   += impulse.x * second_inverse_mass;
        cellular_kinematics[second].y   += impulse.y * second_inverse_mass;
    }
    let stress: f32 = normal_impulse * CONTACT_PRESSURE_TRANSFER;
    if horizontal {
        pending_pressure[first].x   += stress;
        pending_pressure[second].y   += stress;
    } else {
        pending_pressure[first].z   += stress;
        pending_pressure[second].w   += stress;
    }
}

fn process_rigid_static_overlap(cell: vec2<i32>, gather: bool) {
    let index: u32 = cellular_pressure_physical_cell_index_from_world_cell(cell);
    if index == INVALID_PHYSICAL_CELL_INDEX || rigid_owners[index] == 0u || material_form_from_identifier(
        cellular_material_identifiers[index]) != CELLULAR_STATIC_MATERIAL_FORM {
        return;
    }
    let left: u32 = cellular_pressure_physical_cell_index_from_world_cell(
      cell + vec2<i32>(-1, 0));
    let right: u32 = cellular_pressure_physical_cell_index_from_world_cell(
      cell + vec2<i32>(1, 0));
    let below: u32 = cellular_pressure_physical_cell_index_from_world_cell(
      cell + vec2<i32>(0, -1));
    let above: u32 = cellular_pressure_physical_cell_index_from_world_cell(
      cell + vec2<i32>(0, 1));
    let gradient: vec2<f32> = vec2<f32>(
      f32(is_static_cell(left)) - f32(is_static_cell(right)),
      f32(is_static_cell(below)) - f32(is_static_cell(above)),);
    let body: u32 = rigid_owners[index] - 1u;
    let motion: vec2<f32> = rigid_transforms[body * 3u + 1u].xy;
    var normal: vec2<f32> = vec2<f32>(0.0);
    if length(gradient) > 0.0 {
        normal = -normalize(gradient);
    } else if length(motion) > 0.0001 {
        normal = normalize(motion);
    } else {
        return;
    }
    process_rigid_cellular_face(
    index,
    index,
    normal,
    (vec2<f32>(cell) + vec2<f32>(0.5)) / 8.0,
    gather);
}

fn is_static_cell(index: u32) -> bool {
    return
    index != INVALID_PHYSICAL_CELL_INDEX && material_form_from_identifier(
      cellular_material_identifiers[index]) == CELLULAR_STATIC_MATERIAL_FORM;
}

fn process_rigid_cellular_face(
    rigid: u32,
    other: u32,
    normal: vec2<f32>,
    point: vec2<f32>,
    gather: bool,
) {
    let owner: u32 = rigid_owners[rigid];
    if owner == 0u || owner > parameters.rigid_body_count {
        return;
    }
    if external_body_occupancy[other] == 1u || external_body_occupancy[other] == 2u {
        process_rigid_actor_contact(owner - 1u, other, normal, point, gather);
    }
  // Static contacts are discovered by the swept/overlap pass. Sending the
  // adjacent grid face as well would solve the same contact twice.
    if material_form_from_identifier(
      cellular_material_identifiers[other]) == CELLULAR_STATIC_MATERIAL_FORM {
        return;
    }
    process_rigid_cellular_contact(
    owner - 1u,
    rigid_material_identifiers[rigid],
    other,
    normal,
    point,
    0.0,
    gather);
}

fn process_rigid_actor_contact(
    body: u32,
    other: u32,
    normal: vec2<f32>,
    point: vec2<f32>,
    gather: bool,
) {
    let mass_record: vec4<f32> = rigid_transforms[body * 3u + 2u];
    let radius: vec2<f32> = point - mass_record.xy;
    let linear: vec2<f32> = vec2<f32>(
      f32(atomicLoad(&rigid_predicted_motion[body].x)),
      f32(atomicLoad(&rigid_predicted_motion[body].y))) / 4096.0;
    let angular: f32 = f32(atomicLoad(&rigid_predicted_motion[body].angular)) / 4096.0;
    let rigid_velocity: vec2<f32> = linear + angular * vec2<f32>(-radius.y, radius.x);
    let actor: vec4<f32> = external_body_velocity[other];
    let approach: f32 = max(0.0, dot(rigid_velocity - actor.xy, normal));
    let drive: f32 = max(0.0, -dot(actor.zw, normal));
    let moving: bool = -dot(actor.xy, normal) > 0.02 || drive > 0.000001;
    let channel: u32 = dominant_cardinal_channel(normal);
    if gather {
        if approach > 0.02 || moving {
            atomicAdd(&rigid_contact_statistics[body].motion_support[channel], 1u);
        }
        return;
    }
    let arm: f32 = radius.x * normal.y - radius.y * normal.x;
    let inverse_mass: f32 = mass_record.z + mass_record.w * arm * arm;
    if inverse_mass <= 0.000001 {
        return;
    }
    let count: f32 = f32(
      max(
        atomicLoad(&rigid_contact_statistics[body].motion_support[channel]),
        1u));
  // Drive is already an impulse distributed over actor proxy cells.
    let reaction: vec2<f32> = -normal * (approach / (inverse_mass * count) + drive);
    let torque: f32 = radius.x * reaction.y - radius.y * reaction.x;
    if any(reaction != reaction) || any(abs(reaction) > vec2<f32>(1000000.0)) || abs(torque) > 1000000.0 {
        return;
    }
    if all(reaction == vec2<f32>(0.0)) {
        return;
    }
    let event: vec2<f32> = -normal * drive;
    let event_torque: f32 = radius.x * event.y - radius.y * event.x;
    let event_credit: f32 = max(
      0.0,
      dot(linear, event) + angular * event_torque + 0.5 * count * (mass_record.z * dot(event, event) + mass_record.w * event_torque * event_torque));
    atomicAdd(
    &rigid_contact_statistics[body].padding_1,
    u32(ceil(min(event_credit, 1000000.0) * 256.0)));
    accumulate_rigid_sweep_reaction(body, event, radius);
    let kinematic: vec2<f32> = reaction - event;
    let kinematic_torque: f32 = torque - event_torque;
    let credit: f32 = max(
      0.0,
      dot(linear, kinematic) + angular * kinematic_torque + 0.5 * count * (mass_record.z * dot(kinematic, kinematic) + mass_record.w * kinematic_torque * kinematic_torque));
    if any(kinematic != vec2<f32>(0.0)) {
        atomicStore(&rigid_reactions[body].kinematic_constraint, 1u);
    }
    accumulate_rigid_constraint(body, kinematic, radius, credit, 0u);
    atomicAdd(
    &rigid_predicted_motion[body].x,
    i32(round(reaction.x * mass_record.z * 4096.0)));
    atomicAdd(
    &rigid_predicted_motion[body].y,
    i32(round(reaction.y * mass_record.z * 4096.0)));
    atomicAdd(
    &rigid_predicted_motion[body].angular,
    i32(round(torque * mass_record.w * 4096.0)));
    atomicAdd(&rigid_contact_statistics[body].contact_count, 1u);
    if moving {
        atomicAdd(&rigid_contact_statistics[body].padding_2, 0x00010000u);
    }
}

// The single rigid/cellular response solver. Static-specific code may discover a swept or
// overlap contact, but it must feed that geometry here rather than implement different physics.
