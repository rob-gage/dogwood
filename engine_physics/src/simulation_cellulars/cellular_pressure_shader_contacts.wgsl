fn effective_pressure_material(index: u32) -> u32 {
  return
    select(
      cellular_material_identifiers[index],
      rigid_material_identifiers[index],
      rigid_owners[index] != 0u);
}

fn rigid_cell_state_slot(source: u32) -> u32 {
  if source >= arrayLength(&rigid_cells) / 2u {
    return 0xffffffffu;
  }
  return rigid_cells[source * 2u + 1u].y;
}

fn accumulate_rigid_pressure_damage(
  index: u32,
  material: u32,
  load: vec4<f32>) {
  let source: u32 = rigid_claims[index];
  if source == 0xffffffffu {
    return;
  }
  let slot: u32 = rigid_cell_state_slot(source);
  if slot >= arrayLength(&rigid_damage) {
    return;
  }
  let properties =
    cellular_static_properties[material_index_from_identifier(material)];
  let overload =
    max(
      0.0,
      load.x + load.y + load.z + load.w - properties.pressure_ignore_threshold);
  atomicMax(&rigid_damage[slot], bitcast<u32>(overload));
  if overload > 0.0 {
    atomicStore(
      &rigid_damage_dispatch[0],
      (parameters.rigid_cell_count + 63u) / 64u);
    atomicStore(&rigid_damage_dispatch[1], 1u);
    atomicStore(&rigid_damage_dispatch[2], 1u);
  }
}

@compute @workgroup_size(64)
fn apply_rigid_pressure_damage(
  @builtin(global_invocation_id) invocation: vec3<u32>) {
  if invocation.x >= parameters.rigid_cell_count {
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
  rigid_cell_integrities[slot] -=
    overload * parameters.delta_time * parameters.damage_rate;
  if rigid_cell_integrities[slot] <= 0.0 {
    let mask = 1u << (slot % 32u);
    if (atomicOr(&rigid_fractures[slot / 32u], mask) & mask) == 0u {
      atomicAdd(&rigid_fracture_count, 1u);
    }
  }
}

@compute @workgroup_size(64)
fn initialize_rigid_contact_state(
  @builtin(global_invocation_id) invocation: vec3<u32>) {
  let body: u32 = invocation.x;
  if body >= parameters.rigid_body_count {
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
  atomicStore(
    &rigid_predicted_motion[body].angular,
    i32(round(motion.z * 4096.0)));
  for (var channel: u32 = 0u; channel < 4u; channel++) {
    atomicStore(&rigid_contact_statistics[body].geometric_support[channel], 0u);
    atomicStore(&rigid_contact_statistics[body].motion_support[channel], 0u);
  }
}

@compute @workgroup_size(64)
fn gather_rigid_static_contacts(
  @builtin(global_invocation_id) invocation: vec3<u32>) {
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
    true);
}

@compute @workgroup_size(64)
fn resolve_rigid_static_contacts(
  @builtin(global_invocation_id) invocation: vec3<u32>) {
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
    false);
}

// Clears the transient coarse mask before current pressure sources are discovered
@compute @workgroup_size(64)
fn clear_active_cellular_pressure_tiles(
  @builtin(global_invocation_id) invocation: vec3<u32>) {
  let tile_count: u32 =
    parameters.buffered_tile_size.x * parameters.buffered_tile_size.y;
  if invocation.x < tile_count {
    atomicStore(&active_pressure_tiles[invocation.x], 0u);
  }
}

// Marks source tiles and the one-tile halo reachable by the fixed six-pass stencil
@compute @workgroup_size(64)
fn mark_active_cellular_pressure_tiles(
  @builtin(workgroup_id) workgroup: vec3<u32>,
  @builtin(local_invocation_index) lane: u32,) {
  let logical_tile_index: u32 = workgroup.x;
  let tile_count: u32 =
    parameters.buffered_tile_size.x * parameters.buffered_tile_size.y;
  if logical_tile_index >= tile_count {
    return;
  }
  if lane == 0u {
    atomicStore(&pressure_tile_has_source, 0u);
  }
  workgroupBarrier();
  let logical_tile: vec2<u32> =
    vec2<u32>(
      logical_tile_index % parameters.buffered_tile_size.x,
      logical_tile_index / parameters.buffered_tile_size.x,);
  let physical_tile: vec2<u32> =
    physical_tile_from_logical_tile(
      logical_tile,
      parameters.buffered_tile_size,
      parameters.ring_offset,);
  let cell_start: u32 =
    (physical_tile.y * parameters.buffered_tile_size.x + physical_tile.x) * CELL_COUNT_PER_TILE;
  let index: u32 = cell_start + lane;
  let cell: vec2<i32> =
    world_cell_from_logical_tile_major_index(
      logical_tile_index * CELL_COUNT_PER_TILE + lane,
      parameters.buffered_origin,
      parameters.buffered_tile_size,);
  var has_source: bool =
    any(pending_pressure[index] != vec4<f32>(0.0))
      || external_body_occupancy[index] == 1u
      || external_body_occupancy[index] == 2u;
  let material: u32 = effective_pressure_material(index);
  let form: u32 = material_form_from_identifier(material);
  for (var channel: u32 = 0u; channel < 4u && !has_source; channel++) {
    let neighbor: u32 =
      cellular_pressure_physical_cell_index_from_world_cell(
        cell + world_cell_direction_from_pressure_channel(channel));
    if neighbor == INVALID_PHYSICAL_CELL_INDEX {
      continue;
    }
    let neighbor_form: u32 =
      material_form_from_identifier(effective_pressure_material(neighbor));
    let rigid_interface: bool =
      (rigid_owners[index] != 0u) != (rigid_owners[neighbor] != 0u);
    let fluid_interface: bool =
      (mechanical_fluid_cells[index].mass > 0.0) != (mechanical_fluid_cells[neighbor].mass > 0.0) && (form == CELLULAR_DYNAMIC_MATERIAL_FORM
        || form == CELLULAR_STATIC_MATERIAL_FORM
        || neighbor_form == CELLULAR_DYNAMIC_MATERIAL_FORM
        || neighbor_form == CELLULAR_STATIC_MATERIAL_FORM
        || gas_cell_is_open(index)
        || gas_cell_is_open(neighbor));
    let gas_interface: bool =
      (gas_cell_is_open(index) != gas_cell_is_open(neighbor))
        && (form != 0u
          || neighbor_form != 0u
          || mechanical_fluid_cells[index].mass > 0.0
          || mechanical_fluid_cells[neighbor].mass > 0.0)
        && (length(gas_velocity[index]) > 0.0001 || length(
          gas_velocity[neighbor]) > 0.0001);
    var granular_impact: bool = false;
    if
      form == CELLULAR_DYNAMIC_MATERIAL_FORM && (neighbor_form == CELLULAR_DYNAMIC_MATERIAL_FORM || neighbor_form == CELLULAR_STATIC_MATERIAL_FORM)
    {
      granular_impact =
        dot(
          cellular_kinematics[index].xy - cellular_kinematics[neighbor].xy,
          vec2<f32>(world_cell_direction_from_pressure_channel(channel)),) > 0.0001;
    }
    has_source =
      rigid_interface || fluid_interface || gas_interface || granular_impact;
  }
  if has_source {
    atomicStore(&pressure_tile_has_source, 1u);
  }
  workgroupBarrier();
  if lane != 0u || atomicLoad(&pressure_tile_has_source) == 0u {
    return;
  }
  for (var offset_y: i32 = -1; offset_y <= 1; offset_y++) {
    for (var offset_x: i32 = -1; offset_x <= 1; offset_x++) {
      let active_tile: vec2<i32> =
        vec2<i32>(logical_tile) + vec2<i32>(offset_x, offset_y);
      if
        any(active_tile < vec2<i32>(0))
          || active_tile.x >= i32(parameters.buffered_tile_size.x)
          || active_tile.y >= i32(parameters.buffered_tile_size.y)
      {
        continue;
      }
      atomicStore(
        &active_pressure_tiles[u32(
          active_tile.y) * parameters.buffered_tile_size.x + u32(active_tile.x)],
        1u);
    }
  }
}

// Pressure propagation is deliberately narrower than interaction discovery: an idle
// rigid boundary still needs contact work, but is not a pressure source by itself.
@compute @workgroup_size(64)
fn mark_pressure_active_cellular_pressure_tiles(
  @builtin(workgroup_id) workgroup: vec3<u32>,
  @builtin(local_invocation_index) lane: u32,) {
  let logical_tile_index: u32 = workgroup.x;
  let tile_count: u32 =
    parameters.buffered_tile_size.x * parameters.buffered_tile_size.y;
  if logical_tile_index >= tile_count {
    return;
  }
  if lane == 0u {
    atomicStore(&pressure_tile_has_source, 0u);
  }
  workgroupBarrier();
  let logical_tile: vec2<u32> =
    vec2<u32>(
      logical_tile_index % parameters.buffered_tile_size.x,
      logical_tile_index / parameters.buffered_tile_size.x,);
  let physical_tile: vec2<u32> =
    physical_tile_from_logical_tile(
      logical_tile,
      parameters.buffered_tile_size,
      parameters.ring_offset,);
  let index: u32 =
    (physical_tile.y * parameters.buffered_tile_size.x + physical_tile.x) * CELL_COUNT_PER_TILE + lane;
  if
    any(pending_pressure[index] != vec4<f32>(0.0))
      || external_body_occupancy[index] == 1u
      || external_body_occupancy[index] == 2u
  {
    atomicStore(&pressure_tile_has_source, 1u);
  }
  workgroupBarrier();
  if lane != 0u || atomicLoad(&pressure_tile_has_source) == 0u {
    return;
  }
  for (var offset_y: i32 = -1; offset_y <= 1; offset_y++) {
    for (var offset_x: i32 = -1; offset_x <= 1; offset_x++) {
      let active_tile: vec2<i32> =
        vec2<i32>(logical_tile) + vec2<i32>(offset_x, offset_y);
      if
        any(active_tile < vec2<i32>(0))
          || active_tile.x >= i32(parameters.buffered_tile_size.x)
          || active_tile.y >= i32(parameters.buffered_tile_size.y)
      {
        continue;
      }
      atomicStore(
        &active_pressure_tiles[u32(
          active_tile.y) * parameters.buffered_tile_size.x + u32(active_tile.x)],
        1u);
    }
  }
}

// Converts the coarse pressure mask into one indirect workgroup per active tile
@compute @workgroup_size(64)
fn compact_active_cellular_pressure_tiles(
  @builtin(global_invocation_id) invocation: vec3<u32>) {
  if invocation.x == 0u {
    atomicStore(&pressure_indirect_dispatch[1], 1u);
    atomicStore(&pressure_indirect_dispatch[2], 1u);
  }
  let tile_count: u32 =
    parameters.buffered_tile_size.x * parameters.buffered_tile_size.y;
  let logical_tile_index: u32 = invocation.x;
  if
    logical_tile_index >= tile_count || atomicLoad(
      &active_pressure_tiles[logical_tile_index]) == 0u
  {
    return;
  }
  let slot: u32 = atomicAdd(&pressure_indirect_dispatch[0], 1u);
  active_pressure_tile_indices[slot] = logical_tile_index;
}

// Queues editor impulse pressure without bypassing material transmission or mass response
@compute @workgroup_size(64)
fn queue_cellular_radial_impulse(
  @builtin(global_invocation_id) invocation: vec3<u32>) {
  if invocation.x >= parameters.impulse_size.x * parameters.impulse_size.y {
    return;
  }
  let cell: vec2<i32> =
    parameters.impulse_min + vec2<i32>(
      i32(invocation.x % parameters.impulse_size.x),
      i32(invocation.x / parameters.impulse_size.x));
  let index: u32 = cellular_pressure_physical_cell_index_from_world_cell(cell);
  if
    index == INVALID_PHYSICAL_CELL_INDEX || effective_pressure_material(
      index) == EMPTY_MATERIAL_IDENTIFIER
  {
    return;
  }
  let delta: vec2<f32> =
    vec2<f32>(cell) + vec2<f32>(0.5) - parameters.impulse_center;
  let distance: f32 = length(delta);
  if distance > parameters.impulse_radius {
    return;
  }
  let direction: vec2<f32> =
    normalize(select(vec2<f32>(1.0, 0.0), delta, distance > 0.0001,));
  let falloff: f32 = 1.0 - distance / max(parameters.impulse_radius, 0.0001);
  let impulse: vec2<f32> = direction * parameters.impulse_strength * falloff;
  pending_pressure[index] += encode_directional_pressure(impulse);
  let material: u32 = effective_pressure_material(index);
  if material_form_from_identifier(material) == CELLULAR_DYNAMIC_MATERIAL_FORM {
    let mass: f32 =
      cellular_dynamic_properties[material_index_from_identifier(material)].x;
    cellular_kinematics[index].x += impulse.x / mass;
    cellular_kinematics[index].y += impulse.y / mass;
  }
}

// One color writes each ordinary cell at most once, so both sides receive equal momentum.
@compute @workgroup_size(64)
fn resolve_cellular_contacts_horizontal_even(
  @builtin(workgroup_id) workgroup: vec3<u32>,
  @builtin(local_invocation_index) local_index: u32,) {
  process_cellular_face(workgroup.x, local_index, true, 0, false);
}

@compute @workgroup_size(64)
fn resolve_cellular_contacts_horizontal_odd(
  @builtin(workgroup_id) workgroup: vec3<u32>,
  @builtin(local_invocation_index) local_index: u32,) {
  process_cellular_face(workgroup.x, local_index, true, 1, false);
}

@compute @workgroup_size(64)
fn resolve_cellular_contacts_vertical_even(
  @builtin(workgroup_id) workgroup: vec3<u32>,
  @builtin(local_invocation_index) local_index: u32,) {
  process_cellular_face(workgroup.x, local_index, false, 0, false);
}

@compute @workgroup_size(64)
fn resolve_cellular_contacts_vertical_odd(
  @builtin(workgroup_id) workgroup: vec3<u32>,
  @builtin(local_invocation_index) local_index: u32,) {
  process_cellular_face(workgroup.x, local_index, false, 1, false);
}

@compute @workgroup_size(64)
fn gather_rigid_contacts_horizontal_even(
  @builtin(workgroup_id) workgroup: vec3<u32>,
  @builtin(local_invocation_index) local_index: u32,) {
  process_cellular_face(workgroup.x, local_index, true, 0, true);
}

@compute @workgroup_size(64)
fn gather_rigid_contacts(
  @builtin(workgroup_id) workgroup: vec3<u32>,
  @builtin(local_invocation_index) local_index: u32,) {
  let logical_index: u32 =
    logical_cell_index_from_active_pressure_workgroup(workgroup.x, local_index);
  if logical_index >= parameters.buffered_cell_count {
    return;
  }
  let cell: vec2<i32> =
    cellular_pressure_world_cell_from_logical_index(logical_index);
  let index: u32 = cellular_pressure_physical_cell_index_from_world_cell(cell);
  if index == INVALID_PHYSICAL_CELL_INDEX {
    return;
  }
  for (var channel: u32 = 0u; channel < 4u; channel++) {
    let direction: vec2<i32> =
      world_cell_direction_from_pressure_channel(channel);
    let neighbor: u32 =
      cellular_pressure_physical_cell_index_from_world_cell(cell + direction);
    if neighbor == INVALID_PHYSICAL_CELL_INDEX {
      continue;
    }
    if rigid_owners[index] != 0u && rigid_owners[neighbor] == 0u {
      process_rigid_cellular_face(
        index,
        neighbor,
        vec2<f32>(direction),
        (vec2<f32>(cell) + vec2<f32>(0.5) + vec2<f32>(direction) * 0.5) / 8.0,
        true);
    }
  }
}

@compute @workgroup_size(64)
fn gather_rigid_contacts_horizontal_odd(
  @builtin(workgroup_id) workgroup: vec3<u32>,
  @builtin(local_invocation_index) local_index: u32,) {
  process_cellular_face(workgroup.x, local_index, true, 1, true);
}

@compute @workgroup_size(64)
fn gather_rigid_contacts_vertical_even(
  @builtin(workgroup_id) workgroup: vec3<u32>,
  @builtin(local_invocation_index) local_index: u32,) {
  process_cellular_face(workgroup.x, local_index, false, 0, true);
}

@compute @workgroup_size(64)
fn gather_rigid_contacts_vertical_odd(
  @builtin(workgroup_id) workgroup: vec3<u32>,
  @builtin(local_invocation_index) local_index: u32,) {
  process_cellular_face(workgroup.x, local_index, false, 1, true);
}

fn process_cellular_face(
  workgroup: u32,
  local_index: u32,
  horizontal: bool,
  parity: i32,
  gather: bool,) {
  let logical_index: u32 =
    logical_cell_index_from_active_pressure_workgroup(workgroup, local_index);
  if logical_index >= parameters.buffered_cell_count {
    return;
  }
  let cell: vec2<i32> =
    cellular_pressure_world_cell_from_logical_index(logical_index);
  if (select(cell.y, cell.x, horizontal) & 1) != parity {
    return;
  }
  let direction: vec2<i32> =
    select(vec2<i32>(0, 1), vec2<i32>(1, 0), horizontal);
  let first: u32 = cellular_pressure_physical_cell_index_from_world_cell(cell);
  let second: u32 =
    cellular_pressure_physical_cell_index_from_world_cell(cell + direction);

  if
    first == INVALID_PHYSICAL_CELL_INDEX || second == INVALID_PHYSICAL_CELL_INDEX
  {
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
  if
    external_body_occupancy[first] != 0u || external_body_occupancy[second] != 0u
  {
    return;
  }
  if
    (mechanical_fluid_cells[first].mass > 0.0 && second_form == CELLULAR_STATIC_MATERIAL_FORM)
      || (mechanical_fluid_cells[first].mass > 0.0 && second_form == CELLULAR_DYNAMIC_MATERIAL_FORM)
  {
    resolve_fluid_cellular_face(first, second, vec2<f32>(direction));
  }
  if
    (mechanical_fluid_cells[second].mass > 0.0 && first_form == CELLULAR_STATIC_MATERIAL_FORM)
      || (mechanical_fluid_cells[second].mass > 0.0 && first_form == CELLULAR_DYNAMIC_MATERIAL_FORM)
  {
    resolve_fluid_cellular_face(second, first, -vec2<f32>(direction));
  }
  if gas_cell_is_open(first) {
    if
      second_form == CELLULAR_STATIC_MATERIAL_FORM || second_form == CELLULAR_DYNAMIC_MATERIAL_FORM
    {
      resolve_gas_cellular_face(first, second, vec2<f32>(direction));
    } else if mechanical_fluid_cells[second].mass > 0.0 {
      resolve_gas_fluid_face(first, second, vec2<f32>(direction));
    }
  }
  if gas_cell_is_open(second) {
    if
      first_form == CELLULAR_STATIC_MATERIAL_FORM || first_form == CELLULAR_DYNAMIC_MATERIAL_FORM
    {
      resolve_gas_cellular_face(second, first, -vec2<f32>(direction));
    } else if mechanical_fluid_cells[first].mass > 0.0 {
      resolve_gas_fluid_face(second, first, -vec2<f32>(direction));
    }
  }
  let first_dynamic: bool = first_form == CELLULAR_DYNAMIC_MATERIAL_FORM;
  let second_dynamic: bool = second_form == CELLULAR_DYNAMIC_MATERIAL_FORM;
  if
    (!first_dynamic && first_form != CELLULAR_STATIC_MATERIAL_FORM)
      || (!second_dynamic && second_form != CELLULAR_STATIC_MATERIAL_FORM)
      || (!first_dynamic && !second_dynamic)
  {
    return;
  }
  let first_inverse_mass: f32 =
    select(
      0.0,
      1.0 / max(cellular_contact_mass_at_physical_cell_index(first), 0.000001),
      first_dynamic);
  let second_inverse_mass: f32 =
    select(
      0.0,
      1.0 / max(cellular_contact_mass_at_physical_cell_index(second), 0.000001),
      second_dynamic);
  let inverse_mass_sum: f32 = first_inverse_mass + second_inverse_mass;
  if inverse_mass_sum <= 0.0 {
    return;
  }
  let normal: vec2<f32> = vec2<f32>(direction);
  let relative: vec2<f32> =
    cellular_kinematics[first].xy - cellular_kinematics[second].xy;
  let approach: f32 = dot(relative, normal);
  if approach <= 0.0001 {
    return;
  }
  let restitution: f32 =
    clamp(cellular_contact_restitution(first, second), 0.0, 1.0);
  let normal_impulse: f32 = (1.0 + restitution) * approach / inverse_mass_sum;
  let tangent: vec2<f32> = vec2<f32>(-normal.y, normal.x);
  let friction: f32 =
    max(
      0.0,
      min(
        cellular_contact_friction_at_physical_cell_index(first),
        cellular_contact_friction_at_physical_cell_index(second),));
  let tangent_impulse: f32 =
    clamp(
      dot(relative, tangent) / inverse_mass_sum,
      -friction * normal_impulse,
      friction * normal_impulse,);
  let impulse: vec2<f32> = normal * normal_impulse + tangent * tangent_impulse;
  if first_dynamic {
    cellular_kinematics[first].x -= impulse.x * first_inverse_mass;
    cellular_kinematics[first].y -= impulse.y * first_inverse_mass;
  }
  if second_dynamic {
    cellular_kinematics[second].x += impulse.x * second_inverse_mass;
    cellular_kinematics[second].y += impulse.y * second_inverse_mass;
  }
  let stress: f32 = normal_impulse * CONTACT_PRESSURE_TRANSFER;
  if horizontal {
    pending_pressure[first].x += stress;
    pending_pressure[second].y += stress;
  } else {
    pending_pressure[first].z += stress;
    pending_pressure[second].w += stress;
  }
}

fn process_rigid_static_overlap(cell: vec2<i32>, gather: bool) {
  let index: u32 = cellular_pressure_physical_cell_index_from_world_cell(cell);
  if
    index == INVALID_PHYSICAL_CELL_INDEX
      || rigid_owners[index] == 0u
      || material_form_from_identifier(
        cellular_material_identifiers[index]) != CELLULAR_STATIC_MATERIAL_FORM
  {
    return;
  }
  let left: u32 =
    cellular_pressure_physical_cell_index_from_world_cell(
      cell + vec2<i32>(-1, 0));
  let right: u32 =
    cellular_pressure_physical_cell_index_from_world_cell(
      cell + vec2<i32>(1, 0));
  let below: u32 =
    cellular_pressure_physical_cell_index_from_world_cell(
      cell + vec2<i32>(0, -1));
  let above: u32 =
    cellular_pressure_physical_cell_index_from_world_cell(
      cell + vec2<i32>(0, 1));
  let gradient: vec2<f32> =
    vec2<f32>(
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
  gather: bool,) {
  let owner: u32 = rigid_owners[rigid];
  if owner == 0u || owner > parameters.rigid_body_count {
    return;
  }
  if
    external_body_occupancy[other] == 1u || external_body_occupancy[other] == 2u
  {
    process_rigid_actor_contact(owner - 1u, other, normal, point, gather);
  }
  // Static contacts are discovered by the swept/overlap pass. Sending the
  // adjacent grid face as well would solve the same contact twice.
  if
    material_form_from_identifier(
      cellular_material_identifiers[other]) == CELLULAR_STATIC_MATERIAL_FORM
  {
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
  gather: bool,) {
  let mass_record: vec4<f32> = rigid_transforms[body * 3u + 2u];
  let radius: vec2<f32> = point - mass_record.xy;
  let linear: vec2<f32> =
    vec2<f32>(
      f32(atomicLoad(&rigid_predicted_motion[body].x)),
      f32(atomicLoad(&rigid_predicted_motion[body].y))) / 4096.0;
  let angular: f32 =
    f32(atomicLoad(&rigid_predicted_motion[body].angular)) / 4096.0;
  let rigid_velocity: vec2<f32> =
    linear + angular * vec2<f32>(-radius.y, radius.x);
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
  let count: f32 =
    f32(
      max(
        atomicLoad(&rigid_contact_statistics[body].motion_support[channel]),
        1u));
  // Drive is already an impulse distributed over actor proxy cells.
  let reaction: vec2<f32> =
    -normal * (approach / (inverse_mass * count) + drive);
  let torque: f32 = radius.x * reaction.y - radius.y * reaction.x;
  if
    any(reaction != reaction)
      || any(abs(reaction) > vec2<f32>(1000000.0))
      || abs(torque) > 1000000.0
  {
    return;
  }
  if all(reaction == vec2<f32>(0.0)) {
    return;
  }
  let event: vec2<f32> = -normal * drive;
  let event_torque: f32 = radius.x * event.y - radius.y * event.x;
  let event_credit: f32 =
    max(
      0.0,
      dot(linear, event)
        + angular * event_torque
        + 0.5
          * count
          * (mass_record.z * dot(event, event) + mass_record.w
            * event_torque
            * event_torque));
  atomicAdd(
    &rigid_contact_statistics[body].padding_1,
    u32(ceil(min(event_credit, 1000000.0) * 256.0)));
  accumulate_rigid_sweep_reaction(body, event, radius);
  let kinematic: vec2<f32> = reaction - event;
  let kinematic_torque: f32 = torque - event_torque;
  let credit: f32 =
    max(
      0.0,
      dot(linear, kinematic)
        + angular * kinematic_torque
        + 0.5
          * count
          * (mass_record.z * dot(kinematic, kinematic) + mass_record.w
            * kinematic_torque
            * kinematic_torque));
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
