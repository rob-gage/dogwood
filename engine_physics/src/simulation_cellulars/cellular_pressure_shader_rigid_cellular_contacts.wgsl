fn process_rigid_cellular_contact(
  body: u32,
  rigid_material: u32,
  other: u32,
  normal: vec2<f32>,
  point: vec2<f32>,
  penetration: f32,
  gather: bool,) {
  let material: u32 = cellular_material_identifiers[other];
  let form: u32 = material_form_from_identifier(material);
  let dynamic: bool = form == CELLULAR_DYNAMIC_MATERIAL_FORM;
  let static_cell: bool = form == CELLULAR_STATIC_MATERIAL_FORM;
  var constrained: bool = static_cell;
  if dynamic {
    let world_cell: vec2<i32> =
      world_cell_from_physical_tile_ring_index(
        other,
        parameters.buffered_origin,
        parameters.buffered_tile_size,
        parameters.ring_offset);
    let away: u32 =
      cellular_pressure_physical_cell_index_from_world_cell(
        world_cell + world_cell_direction_from_pressure_channel(
          dominant_cardinal_channel(normal)));
    constrained = away == INVALID_PHYSICAL_CELL_INDEX;
    if !constrained {
      constrained =
        cellular_material_identifiers[away] != EMPTY_MATERIAL_IDENTIFIER || external_body_occupancy[away] != 0u;
    }
  }
  let fluid: bool =
    !dynamic && !static_cell && mechanical_fluid_cells[other].mass > 0.0;
  let gas: bool = !dynamic && !static_cell && !fluid && gas_cell_is_open(other);
  if !dynamic && !static_cell && !fluid && !gas {
    return;
  }
  let mass_record: vec4<f32> = rigid_transforms[body * 3u + 2u];
  let radius: vec2<f32> = point - mass_record.xy;
  let shadow: vec4<f32> =
    vec4<f32>(
      f32(atomicLoad(&rigid_predicted_motion[body].x)) / 4096.0,
      f32(atomicLoad(&rigid_predicted_motion[body].y)) / 4096.0,
      f32(atomicLoad(&rigid_predicted_motion[body].angular)) / 4096.0,
      0.0);
  let rigid_velocity: vec2<f32> =
    shadow.xy + shadow.z * vec2<f32>(-radius.y, radius.x);
  let other_velocity: vec2<f32> =
    select(
      select(vec2<f32>(0.0), cellular_kinematics[other].xy, dynamic),
      select(
        mechanical_fluid_cells[other].velocity,
        gas_velocity[other] / 8.0,
        gas),
      fluid || gas,);
  let relative: vec2<f32> = rigid_velocity - other_velocity;
  let approach: f32 = dot(relative, normal);
  let channel: u32 = dominant_cardinal_channel(normal);
  if gather {
    atomicAdd(&rigid_contact_statistics[body].geometric_support[channel], 1u);
    if
      approach > 0.02
        || dynamic && length(other_velocity) > 0.02
        || penetration > 0.0001
    {
      atomicAdd(&rigid_contact_statistics[body].motion_support[channel], 1u);
    }
    return;
  }
  let normal_arm: f32 = radius.x * normal.y - radius.y * normal.x;
  let rigid_inverse_effective_mass: f32 =
    mass_record.z + mass_record.w * normal_arm * normal_arm;
  var other_inverse_mass: f32 =
    select(
      0.0,
      1.0 / max(
        cellular_dynamic_properties[material_index_from_identifier(material)].x,
        0.000001),
      dynamic);
  other_inverse_mass =
    select(
      other_inverse_mass,
      1.0 / max(mechanical_fluid_cells[other].mass, 0.000001),
      fluid);
  other_inverse_mass =
    select(other_inverse_mass, gas_cell_inverse_mass(other), gas);
  let simultaneous_contacts: f32 =
    f32(
      max(
        atomicLoad(&rigid_contact_statistics[body].motion_support[channel]),
        1u));
  let inverse_mass_sum: f32 = rigid_inverse_effective_mass + other_inverse_mass;
  let rigid_index: u32 = material_index_from_identifier(rigid_material);
  let rigid_properties: StaticProperties =
    cellular_static_properties[rigid_index];
  var restitution: f32 = 0.0;
  var friction: f32 = 0.0;
  if dynamic {
    let properties: vec4<f32> =
      cellular_dynamic_properties[material_index_from_identifier(material)];
    restitution = min(rigid_properties.restitution, properties.w);
    friction = min(rigid_properties.friction, properties.z);
  } else if static_cell {
    let properties: StaticProperties =
      cellular_static_properties[material_index_from_identifier(material)];
    restitution = min(rigid_properties.restitution, properties.restitution);
    friction = min(rigid_properties.friction, properties.friction);
  } else if fluid {
    let properties: vec4<f32> =
      fluid_pressure_properties[material_index_from_identifier(
        mechanical_fluid_cells[other].material_identifier)];
    restitution = min(rigid_properties.restitution, properties.z);
    friction = min(rigid_properties.friction, properties.y);
  }
  var transfer_normal_impulse: f32 = 0.0;
  var constraint_normal_impulse: f32 = 0.0;
  if constrained && !dynamic {
    let rigid_toward: f32 = max(0.0, dot(rigid_velocity, normal));
    let grain_toward: f32 = max(0.0, -dot(other_velocity, normal));
    let toward_sum: f32 = rigid_toward + grain_toward;
    if toward_sum > 0.0001 {
      let relative_approach: f32 = max(approach, 0.0);
      let grain_approach: f32 = relative_approach * grain_toward / toward_sum;
      let rigid_approach: f32 = relative_approach * rigid_toward / toward_sum;
      if inverse_mass_sum > 0.0 {
        transfer_normal_impulse +=
          (1.0 + clamp(restitution, 0.0, 1.0))
            * grain_approach
            / inverse_mass_sum;
      }
      if rigid_inverse_effective_mass > 0.000001 {
        constraint_normal_impulse +=
          rigid_approach / rigid_inverse_effective_mass / simultaneous_contacts;
      }
    }
  } else if dynamic && inverse_mass_sum > 0.0 {
    transfer_normal_impulse =
      (1.0 + clamp(restitution, 0.0, 1.0))
        * max(approach, 0.0)
        / inverse_mass_sum;
  } else if inverse_mass_sum > 0.0 {
    transfer_normal_impulse =
      (1.0 + clamp(restitution, 0.0, 1.0))
        * max(approach, 0.0)
        / (simultaneous_contacts * rigid_inverse_effective_mass + other_inverse_mass);
  }
  var penetration_impulse: f32 = 0.0;
  if
    constrained
      && !dynamic
      && penetration > CELL_SIZE * 0.02
      && rigid_inverse_effective_mass > 0.000001
  {
    let correction_speed: f32 =
      min(0.25, (penetration - CELL_SIZE * 0.02) * 0.2 / parameters.delta_time);
    penetration_impulse =
      max(
        0.0,
        correction_speed - max(-approach, 0.0)) / (simultaneous_contacts * rigid_inverse_effective_mass);
    constraint_normal_impulse += penetration_impulse;
  }
  let geometric_contacts: u32 =
    atomicLoad(&rigid_contact_statistics[body].geometric_support[channel]);
  var support_impulse: f32 = 0.0;
  if
    geometric_contacts != 0u
      && mass_record.z > 0.000001
      && constrained
      && !dynamic
  {
    support_impulse =
      max(
        0.0,
        dot(parameters.gravity * parameters.delta_time / mass_record.z, normal)) / f32(geometric_contacts);
    constraint_normal_impulse += support_impulse;
  }
  // Rapier owns rigid/granular hard contact; only the material-side transfer remains here.
  if dynamic {
    constraint_normal_impulse = 0.0;
  }
  let normal_impulse: f32 =
    transfer_normal_impulse + constraint_normal_impulse;
  if normal_impulse <= 0.0 {
    return;
  }
  let tangent: vec2<f32> = vec2<f32>(-normal.y, normal.x);
  let tangent_arm: f32 = radius.x * tangent.y - radius.y * tangent.x;
  let tangent_inverse_mass: f32 =
    mass_record.z
      + mass_record.w * tangent_arm * tangent_arm
      + other_inverse_mass;
  let transfer_tangent_impulse: f32 =
    select(
      0.0,
      clamp(
        dot(relative, tangent) / max(tangent_inverse_mass, 0.000001),
        -max(friction, 0.0) * transfer_normal_impulse,
        max(friction, 0.0) * transfer_normal_impulse,),
      tangent_inverse_mass > 0.0);
  let rigid_tangent_inverse_mass: f32 =
    mass_record.z + mass_record.w * tangent_arm * tangent_arm;
  let constraint_tangent_impulse: f32 =
    clamp(
      (dot(
        relative,
        tangent) - transfer_tangent_impulse * tangent_inverse_mass) / max(
        simultaneous_contacts * rigid_tangent_inverse_mass,
        0.000001),
      -max(friction, 0.0) * constraint_normal_impulse,
      max(friction, 0.0) * constraint_normal_impulse);
  let tangent_impulse: f32 =
    transfer_tangent_impulse + constraint_tangent_impulse;
  let transfer_impulse: vec2<f32> =
    normal * transfer_normal_impulse + tangent * transfer_tangent_impulse;
  let impulse: vec2<f32> = normal * normal_impulse + tangent * tangent_impulse;
  if other_inverse_mass > 0.0 {
    let after: vec2<f32> =
      other_velocity + transfer_impulse * other_inverse_mass;
    let transferred_energy: f32 =
      max(
        0.0,
        (dot(other_velocity, other_velocity) - dot(
          after,
          after)) / (2.0 * other_inverse_mass));
    if transferred_energy < 1000000.0 {
      atomicAdd(
        &rigid_contact_statistics[body].padding_1,
        u32(round(transferred_energy * 256.0)));
    }
  }
  if dynamic {
    cellular_kinematics[other].x += transfer_impulse.x * other_inverse_mass;
    cellular_kinematics[other].y += transfer_impulse.y * other_inverse_mass;
  } else if fluid {
    mechanical_fluid_cells[other].velocity += impulse * other_inverse_mass;
  } else if gas {
    gas_velocity[other] += impulse * other_inverse_mass * 8.0;
  }
  if static_cell || dynamic {
    pending_pressure[other] +=
      encode_directional_pressure(
        normal * normal_impulse * CONTACT_PRESSURE_TRANSFER);
  }
  if dynamic {
    return;
  }
  let reaction: vec2<f32> = -impulse;
  let torque: f32 = radius.x * reaction.y - radius.y * reaction.x;
  if penetration_impulse > 0.0 {
    let support_reaction: vec2<f32> = -normal * penetration_impulse;
    let support_torque: f32 =
      radius.x * support_reaction.y - radius.y * support_reaction.x;
    let support_credit: f32 =
      max(
        0.0,
        dot(shadow.xy, support_reaction) + shadow.z * support_torque) + 0.5
        * max(f32(geometric_contacts), simultaneous_contacts)
        * (mass_record.z * dot(
          support_reaction,
          support_reaction) + mass_record.w * support_torque * support_torque);
    atomicAdd(
      &rigid_reactions[body].recovery_energy,
      u32(ceil(min(support_credit, 1000000.0) * 256.0)));
  }
  if
    any(reaction != reaction)
      || any(abs(reaction) > vec2<f32>(1000000.0))
      || abs(torque) > 1000000.0
  {
    atomicStore(&rigid_reactions[body].overflow, 1);
    return;
  }
  accumulate_rigid_sweep_reaction(body, -transfer_impulse, radius);
  let support_reaction: vec2<f32> = -normal * support_impulse;
  let support_torque: f32 =
    radius.x * support_reaction.y - radius.y * support_reaction.x;
  let support_credit: f32 =
    0.5
      * f32(geometric_contacts)
      * (mass_record.z * dot(support_reaction, support_reaction) + mass_record.w
        * support_torque
        * support_torque);
  accumulate_rigid_constraint(
    body,
    support_reaction,
    radius,
    support_credit,
    1u);
  let recovery_reaction: vec2<f32> = -normal * penetration_impulse;
  accumulate_rigid_constraint(body, recovery_reaction, radius, 0.0, 2u);
  accumulate_rigid_constraint(
    body,
    reaction + transfer_impulse - support_reaction - recovery_reaction,
    radius,
    0.0,
    0u);
  atomicAdd(
    &rigid_predicted_motion[body].x,
    i32(round((reaction.x - support_reaction.x) * mass_record.z * 4096.0)));
  atomicAdd(
    &rigid_predicted_motion[body].y,
    i32(round((reaction.y - support_reaction.y) * mass_record.z * 4096.0)));
  atomicAdd(
    &rigid_predicted_motion[body].angular,
    i32(round((torque - support_torque) * mass_record.w * 4096.0)));
  atomicAdd(&rigid_contact_statistics[body].contact_count, 1u);
  if static_cell {
    atomicAdd(&rigid_contact_statistics[body].static_contact_count, 1u);
  }
  if dynamic {
    let moving_grain: bool = approach > 0.02 || length(other_velocity) > 0.02;
    atomicAdd(
      &rigid_contact_statistics[body].padding_2,
      select(1u, 0x00010001u, moving_grain));
  }
}

fn accumulate_rigid_constraint(
  body: u32,
  impulse: vec2<f32>,
  radius: vec2<f32>,
  credit: f32,
  kind: u32) {
  let torque: f32 = radius.x * impulse.y - radius.y * impulse.x;
  // State uses finer quantization: per-step support must not lose gravity to rounding.
  let scaled: vec3<f32> = round(vec3<f32>(impulse, torque) * 65536.0);
  if any(scaled != scaled) || any(abs(scaled) > vec3<f32>(2147483000.0)) {
    atomicStore(&rigid_reactions[body].overflow, 1);
    return;
  }
  let value: vec3<i32> = vec3<i32>(scaled);
  var old: vec3<i32>;
  if kind == 1u {
    old.x = atomicAdd(&rigid_reactions[body].support_x, value.x);
    old.y = atomicAdd(&rigid_reactions[body].support_y, value.y);
    old.z = atomicAdd(&rigid_reactions[body].support_angular, value.z);
    atomicAdd(
      &rigid_reactions[body].support_energy,
      u32(ceil(min(credit, 1000000.0) * 256.0)));
  } else if kind == 2u {
    old.x = atomicAdd(&rigid_reactions[body].recovery_x, value.x);
    old.y = atomicAdd(&rigid_reactions[body].recovery_y, value.y);
    old.z = atomicAdd(&rigid_reactions[body].recovery_angular, value.z);
  } else {
    old.x = atomicAdd(&rigid_reactions[body].constraint_x, value.x);
    old.y = atomicAdd(&rigid_reactions[body].constraint_y, value.y);
    old.z =
      atomicAdd(&rigid_reactions[body].constraint_angular, value.z);
    atomicAdd(
      &rigid_reactions[body].constraint_energy,
      u32(ceil(min(credit, 1000000.0) * 256.0)));
  }
  if any(abs(vec3<f32>(old) + scaled) > vec3<f32>(2147483000.0)) {
    atomicStore(&rigid_reactions[body].overflow, 1);
  }
}

fn gas_cell_is_open(index: u32) -> bool {
  return
    cellular_material_identifiers[index] == EMPTY_MATERIAL_IDENTIFIER
      && external_body_occupancy[index] == 0u
      && fluid_coverage[index] < 0.85;
}

fn gas_cell_inverse_mass(index: u32) -> f32 {
  var density: f32 = 1.0;
  for (var species: u32 = 0u; species < parameters.gas_count; species++) {
    let concentration: f32 =
      gas_concentrations[species * parameters.buffered_cell_count + index];
    density += concentration * (gas_properties[species * 2u].x - 1.0);
  }
  return 64.0 / max(density, 0.000001);
}

fn resolve_gas_cellular_face(gas: u32, cellular: u32, normal: vec2<f32>) {
  let dynamic: bool =
    material_form_from_identifier(
      cellular_material_identifiers[cellular]) == CELLULAR_DYNAMIC_MATERIAL_FORM;
  let inverse_gas_mass: f32 = gas_cell_inverse_mass(gas);
  let inverse_cellular_mass: f32 =
    select(
      0.0,
      1.0 / max(
        cellular_contact_mass_at_physical_cell_index(cellular),
        0.000001),
      dynamic);
  let relative: vec2<f32> =
    gas_velocity[gas] / 8.0 - cellular_kinematics[cellular].xy;
  let approach: f32 = dot(relative, normal);

  if approach <= 0.0001 {
    return;
  }
  let normal_impulse: f32 =
    approach / (inverse_gas_mass + inverse_cellular_mass);
  gas_velocity[gas] -= normal * normal_impulse * inverse_gas_mass * 8.0;
  if dynamic {
    cellular_kinematics[cellular].x +=
      normal.x * normal_impulse * inverse_cellular_mass;
    cellular_kinematics[cellular].y +=
      normal.y * normal_impulse * inverse_cellular_mass;
  }
  pending_pressure[cellular] +=
    encode_directional_pressure(
      normal * normal_impulse * CONTACT_PRESSURE_TRANSFER);
}

fn resolve_gas_fluid_face(gas: u32, fluid: u32, normal: vec2<f32>) {
  let mass: f32 = mechanical_fluid_cells[fluid].mass;
  if
    mass <= 0.0 || cellular_material_identifiers[fluid] != EMPTY_MATERIAL_IDENTIFIER
  {
    return;
  }
  let inverse_gas_mass: f32 = gas_cell_inverse_mass(gas);
  let inverse_fluid_mass: f32 = 1.0 / mass;
  let relative: vec2<f32> =
    gas_velocity[gas] / 8.0 - mechanical_fluid_cells[fluid].velocity;
  let approach: f32 = dot(relative, normal);
  if approach <= 0.0001 {
    return;
  }
  let normal_impulse: f32 = approach / (inverse_gas_mass + inverse_fluid_mass);
  gas_velocity[gas] -= normal * normal_impulse * inverse_gas_mass * 8.0;
  mechanical_fluid_cells[fluid].velocity +=
    normal * normal_impulse * inverse_fluid_mass;
}

fn resolve_fluid_cellular_face(fluid: u32, cellular: u32, normal: vec2<f32>) {
  if cellular_material_identifiers[fluid] != EMPTY_MATERIAL_IDENTIFIER {
    return;
  }
  let fluid_state: MechanicalFluidCell = mechanical_fluid_cells[fluid];
  if fluid_state.mass <= 0.0 {
    return;
  }
  let material: u32 = cellular_material_identifiers[cellular];
  let dynamic: bool =
    material_form_from_identifier(material) == CELLULAR_DYNAMIC_MATERIAL_FORM;
  let inverse_fluid_mass: f32 = 1.0 / fluid_state.mass;
  let inverse_cellular_mass: f32 =
    select(
      0.0,
      1.0 / max(
        cellular_contact_mass_at_physical_cell_index(cellular),
        0.000001),
      dynamic);
  let inverse_mass_sum: f32 = inverse_fluid_mass + inverse_cellular_mass;
  let relative: vec2<f32> =
    fluid_state.velocity - cellular_kinematics[cellular].xy;
  let approach: f32 = dot(relative, normal);
  if approach <= 0.0001 {
    return;
  }
  let properties: vec4<f32> =
    fluid_pressure_properties[material_index_from_identifier(
      fluid_state.material_identifier)];
  let restitution: f32 =
    clamp(
      min(
        properties.z,
        cellular_material_restitution_at_physical_cell_index(cellular)),
      0.0,
      1.0);
  let normal_impulse: f32 = (1.0 + restitution) * approach / inverse_mass_sum;
  let tangent: vec2<f32> = vec2<f32>(-normal.y, normal.x);
  let friction: f32 =
    max(
      0.0,
      min(
        properties.y,
        cellular_contact_friction_at_physical_cell_index(cellular)));
  let tangent_impulse: f32 =
    clamp(
      dot(relative, tangent) / inverse_mass_sum,
      -friction * normal_impulse,
      friction * normal_impulse);
  let impulse: vec2<f32> = normal * normal_impulse + tangent * tangent_impulse;
  mechanical_fluid_cells[fluid].velocity -= impulse * inverse_fluid_mass;
  if dynamic {
    cellular_kinematics[cellular].x += impulse.x * inverse_cellular_mass;
    cellular_kinematics[cellular].y += impulse.y * inverse_cellular_mass;
  }
  pending_pressure[cellular] +=
    encode_directional_pressure(
      normal * normal_impulse * CONTACT_PRESSURE_TRANSFER);

