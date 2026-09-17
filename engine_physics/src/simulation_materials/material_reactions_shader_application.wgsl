fn apply_canonical(@builtin(global_invocation_id) invocation: vec3<u32>) {
  let cell = invocation.x;
  if (cell >= parameters.cell_count || cell >= arrayLength(&candidates)) {
    return;
  }
  let candidate = candidates[cell];
  if
    (candidate.reaction == 0xffffffffu
      || candidate.reaction >= arrayLength(&reactions)
      || candidate.extent <= 0.000001
      || candidate.padding0 != 1u)
  {
    return;
  }
  let rule = reactions[candidate.reaction];
  let first_present = rule.words[3] != 0u;
  let second_present = rule.words[7] != 0u;
  let source0 =
    Source(
      cell,
      candidate.material0,
      0.0,
      first_present && candidate.material0 != EMPTY_MATERIAL_IDENTIFIER,
      candidate.rigid_claims.x);
  let source1 =
    Source(
      candidate.partner,
      candidate.material1,
      0.0,
      second_present
        && candidate.partner != 0xffffffffu
        && candidate.material1 != EMPTY_MATERIAL_IDENTIFIER,
      candidate.rigid_claims.y);
  let source0_form = material_form_from_identifier(source0.material);
  let source1_form = material_form_from_identifier(source1.material);
  let reaction_temperature = authority_temperature(source0, rule, 0u);
  let coefficient0 = select(0.0, bitcast<f32>(rule.words[2]), first_present);
  let coefficient1 = select(0.0, bitcast<f32>(rule.words[6]), second_present);
  // All capacity and authority checks happened in reserve_fluid_authority.
  // Apply is deliberately a commit-only pass and never re-discovers sources.
  let source0_amount = source_amount(source0);
  let source0_canonical =
    first_present
      && source0_form != GAS_MATERIAL_FORM
      && source0_form != FLUID_MATERIAL_FORM
      && source0.rigid_claim == 0xffffffffu;
  let source1_canonical =
    second_present
      && source1_form != GAS_MATERIAL_FORM
      && source1_form != FLUID_MATERIAL_FORM
      && source1.rigid_claim == 0xffffffffu;
  let same_canonical =
    source0_canonical
      && source1_canonical
      && source0.cell == source1.cell
      && source0.material == source1.material;
  let source0_demand = coefficient0 + select(0.0, coefficient1, same_canonical);
  let remaining =
    select(
      0.0,
      max(source0_amount - source0_demand * candidate.extent, 0.0),
      source0_canonical);
  let source1_amount = source_amount(source1);
  let remaining1 =
    select(
      0.0,
      max(source1_amount - coefficient1 * candidate.extent, 0.0),
      source1_canonical);
  var cellular_product = EMPTY_MATERIAL_IDENTIFIER;
  var cellular_amount = 0.0;
  let fluid_product_slots = candidate.product_slots;
  for (var product = 0u; product < 2u; product += 1u) {
    let base = 8u + product * 4u;
    if (rule.words[base + 2u] == 0u) {
      continue;
    }
    let replacement = rule.words[base];
    let amount = bitcast<f32>(rule.words[base + 1u]) * candidate.extent;
    let form = material_form_from_identifier(replacement);
    if (form == GAS_MATERIAL_FORM) {
      continue;
    }
    if (form == FLUID_MATERIAL_FORM) {
      continue;
    }
    // Unsupported product forms are rejected during reservation.  Keep apply
    // commit-only even if a malformed/stale candidate reaches this pass.
    if
      (form != CELLULAR_STATIC_MATERIAL_FORM && form != CELLULAR_DYNAMIC_MATERIAL_FORM)
    {
      continue;
    }
    cellular_product = replacement;
    cellular_amount = amount;
  }
  let source0_fluid = first_present && source0_form == FLUID_MATERIAL_FORM;
  let source1_fluid = second_present && source1_form == FLUID_MATERIAL_FORM;
  if (source0.rigid_claim != 0xffffffffu) {
    let demand =
      coefficient0 * candidate.extent + select(
        0.0,
        coefficient1 * candidate.extent,
        source1.rigid_claim == source0.rigid_claim);
    consume_rigid(source0.rigid_claim, demand);
  }
  if
    (source1.rigid_claim != 0xffffffffu && source1.rigid_claim != source0.rigid_claim)
  {
    consume_rigid(source1.rigid_claim, coefficient1 * candidate.extent);
  }
  if (source0_canonical) {
    let needs_mutation =
      cellular_product != EMPTY_MATERIAL_IDENTIFIER || remaining <= 0.00001;
    if (needs_mutation) {
      let slot = candidate.padding1;
      let replacement =
        select(
          select(
            source0.material,
            cellular_product,
            cellular_product != EMPTY_MATERIAL_IDENTIFIER),
          EMPTY_MATERIAL_IDENTIFIER,
          remaining <= 0.00001 && cellular_product == EMPTY_MATERIAL_IDENTIFIER);
      let result_amount =
        select(
          remaining,
          cellular_amount,
          cellular_product != EMPTY_MATERIAL_IDENTIFIER);
      mutation_requests[slot] =
        Request(
          cell,
          0u,
          cell,
          source0.material,
          replacement,
          bitcast<u32>(result_amount),
          bitcast<u32>(temperatures[cell]),
          0u,
          0u);
    } else {
      amounts[cell] = remaining;
    }
  }
  if (source1_canonical && !same_canonical) {
    if (remaining1 <= 0.00001) {
      let slot =
        candidate.padding1 + select(
          0u,
          1u,
          source0_canonical && (cellular_product != EMPTY_MATERIAL_IDENTIFIER || remaining <= 0.00001));
      mutation_requests[slot] =
        Request(
          source1.cell,
          0u,
          source1.cell,
          source1.material,
          EMPTY_MATERIAL_IDENTIFIER,
          bitcast<u32>(remaining1),
          bitcast<u32>(temperatures[source1.cell]),
          0u,
          0u);
    } else {
      amounts[source1.cell] = remaining1;
    }
  }
  if (first_present && source0_form == GAS_MATERIAL_FORM) {
    let gas_index =
      material_index_from_identifier(
        source0.material) * parameters.cell_count + source0.cell;
    let demand =
      coefficient0 * candidate.extent + select(
        0.0,
        coefficient1 * candidate.extent,
        second_present
          && source1_form == GAS_MATERIAL_FORM
          && source1.material == source0.material
          && source1.cell == source0.cell);
    gas_concentrations[gas_index] =
      max(gas_concentrations[gas_index] - demand, 0.0);
  }
  if
    (second_present
      && source1_form == GAS_MATERIAL_FORM
      && !(source0_form == GAS_MATERIAL_FORM
        && source1.material == source0.material
        && source1.cell == source0.cell))
  {
    let gas_index =
      material_index_from_identifier(
        source1.material) * parameters.cell_count + source1.cell;
    gas_concentrations[gas_index] =
      max(gas_concentrations[gas_index] - coefficient1 * candidate.extent, 0.0);
  }
  if (source0_fluid) {
    apply_fluid_plan(cell, 0u);
  }
  if (source1_fluid) {
    apply_fluid_plan(cell, 1u);
  }
  for (var product = 0u; product < 2u; product += 1u) {
    let base = 8u + product * 4u;
    if (rule.words[base + 2u] == 0u) {
      continue;
    }
    let replacement = rule.words[base];
    if (material_form_from_identifier(replacement) == FLUID_MATERIAL_FORM) {
      let slot =
        select(fluid_product_slots.x, fluid_product_slots.y, product == 1u);
      spawn_fluid_product(
        slot,
        replacement,
        bitcast<f32>(rule.words[base + 1u]) * candidate.extent,
        cell,
        reaction_temperature);
    }
  }
  reaction_energy[cell] += bitcast<f32>(rule.words[23]) * candidate.extent;
  let pressure_output = bitcast<f32>(rule.words[24]) * candidate.extent;
  pending_pressure[cell] += vec4<f32>(pressure_output);
  for (var product = 0u; product < 2u; product += 1u) {
    let base = 8u + product * 4u;
    if (rule.words[base + 2u] == 0u) {
      continue;
    }
    let replacement = rule.words[base];
    if (material_form_from_identifier(replacement) == GAS_MATERIAL_FORM) {
      let species = material_index_from_identifier(replacement);
      if (species < parameters.gas_count) {
        let output_cell =
          select(cell, source1.cell, remaining > 0.00001 && source1.found);
        gas_concentrations[species * parameters.cell_count + output_cell] +=
          bitcast<f32>(rule.words[base + 1u]) * candidate.extent;
      }
    }
  }
}

fn has_environment(rule: Reaction) -> bool {
  return rule.words[11] != 0u;
}

fn environment_matches(
  rule: Reaction,
  cell: u32,
  source0: Source,
  air: f32) -> bool {
  let temperature = authority_temperature(source0, rule, 0u);
  let pressure = length(retained_pressure[cell].xy);
  let min_temperature = bitcast<f32>(rule.words[16]);
  let max_temperature = bitcast<f32>(rule.words[17]);
  let min_pressure = bitcast<f32>(rule.words[18]);
  let max_pressure = bitcast<f32>(rule.words[19]);
  let min_air = bitcast<f32>(rule.words[20]);
  let max_air = bitcast<f32>(rule.words[21]);
  return
    temperature >= min_temperature
      && temperature <= max_temperature
      && pressure >= min_pressure
      && pressure <= max_pressure
      && air >= min_air
      && air <= max_air;
}

// Discovery only reads the immutable fields for this chemistry tick. Applying
// a later candidate is intentionally a separate pass, preventing same-tick
// reaction cascades from products or field outputs.
@compute @workgroup_size(64)
fn discover_canonical(@builtin(global_invocation_id) invocation: vec3<u32>) {
  let cell = invocation.x;
  if (cell >= parameters.cell_count || cell >= arrayLength(&candidates)) {
    return;
  }
  candidates[cell] =
    Candidate(
      0xffffffffu,
      cell,
      0.0,
      0xffffffffu,
      0u,
      0u,
      0u,
      0u,
      vec4<u32>(0xffffffffu),
      vec4<f32>(0.0),
      vec4<u32>(0xffffffffu),
      vec4<f32>(0.0),
      vec2<u32>(0xffffffffu),
      vec2<u32>(0xffffffffu),
      vec2<u32>(0u));
  reaction_energy[cell] = 0.0;
  let material = material_identifiers[cell];
  let blocked =
    material != EMPTY_MATERIAL_IDENTIFIER
      || external_occupancy[cell] != 0u
      || atomicLoad(&rigid_claims[cell]) != 0xffffffffu;
  var air = local_air(cell);
  if (blocked) {
    for (var direction = 1u; direction <= 4u; direction += 1u) {
      air = max(air, local_air(neighbor(cell, direction)));
    }
  }
  var winner = 0xffffffffu;
  var winner_extent = 0.0;
  var winner_priority = -2147483647i;
  var winner_order = 0xffffffffu;
  for (
    var reaction_index = 0u;
    reaction_index < parameters.reaction_count && reaction_index < arrayLength(
      &reactions);
    reaction_index += 1u
  ) {
    let rule = reactions[reaction_index];
    let first_present = rule.words[3] != 0u;
    let second_present = rule.words[7] != 0u;
    let source0 = source_for(cell, 0u, rule);
    if (first_present && !source0.found) {
      continue;
    }
    if (!first_present && !has_environment(rule)) {
      continue;
    }
    let environment_allowed = environment_matches(rule, cell, source0, air);
    if (!environment_allowed) {
      continue;
    }
    var source1 =
      Source(cell, EMPTY_MATERIAL_IDENTIFIER, 0.0, false, 0xffffffffu);
    if (second_present) {
      source1 = find_partner(cell, rule);
      if (!source1.found) {
        continue;
      }
    }
    let extent0 =
      select(
        1.0,
        source0.amount / max(bitcast<f32>(rule.words[2]), 0.000001),
        first_present);
    let extent1 =
      select(
        1.0,
        source1.amount / max(bitcast<f32>(rule.words[6]), 0.000001),
        second_present);
    var extent = min(bitcast<f32>(rule.words[22]), min(extent0, extent1));
    // Both slots can resolve to one authority (for example FluidA + FluidA
    // or overlapping tag/exact selectors).  Constrain the common extent by
    // the combined demand instead of counting the snapshot inventory twice.
    if
      (first_present
        && second_present
        && source0.rigid_claim != 0xffffffffu
        && source0.rigid_claim == source1.rigid_claim)
    {
      extent =
        min(
          extent,
          source0.amount / max(
            bitcast<f32>(rule.words[2]) + bitcast<f32>(rule.words[6]),
            0.000001));
    } else if
      (first_present
        && second_present
        && source0.cell == source1.cell
        && source0.material == source1.material)
    {
      extent =
        min(
          extent,
          source0.amount / max(
            bitcast<f32>(rule.words[2]) + bitcast<f32>(rule.words[6]),
            0.000001));
    }
    var output_cell = cell;
    if
      (source0.found
        && material_form_from_identifier(source0.material) != GAS_MATERIAL_FORM
        && source1.found
        && source1.cell != cell
        && source0.amount <= bitcast<f32>(rule.words[2]) * extent + 0.00001)
    {
      output_cell = source1.cell;
    }
    var gas_product = 0.0;
    for (var product = 0u; product < 2u; product += 1u) {
      let base = 8u + product * 4u;
      if
        (rule.words[base + 2u] != 0u && material_form_from_identifier(
          rule.words[base]) == GAS_MATERIAL_FORM)
      {
        gas_product += max(bitcast<f32>(rule.words[base + 1u]), 0.0);
      }
    }
    let capacity_extent =
      select(
        extent,
        max(1.0 - gas_total(output_cell), 0.0) / max(gas_product, 0.000001),
        gas_product > 0.0);
    if (!(min(extent, capacity_extent) > 0.000001)) {
      continue;
    }
    let priority = bitcast<i32>(rule.words[25]);
    let order = rule.words[26];
    if
      (priority > winner_priority || (priority == winner_priority && order < winner_order))
    {
      winner = reaction_index;
      winner_extent = min(extent, capacity_extent);
      winner_priority = priority;
      winner_order = order;
    }
  }
  if (winner != 0xffffffffu) {
    let rule = reactions[winner];
    let source0 = source_for(cell, 0u, rule);
    var source1 =
      Source(cell, EMPTY_MATERIAL_IDENTIFIER, 0.0, false, 0xffffffffu);
    if (rule.words[7] != 0u) {
      source1 = find_partner(cell, rule);
    }
    let extent0 =
      select(
        1.0,
        source0.amount / max(bitcast<f32>(rule.words[2]), 0.000001),
        rule.words[3] != 0u);
    let extent1 =
      select(
        1.0,
        source1.amount / max(bitcast<f32>(rule.words[6]), 0.000001),
        rule.words[7] != 0u);
    candidates[cell] =
      Candidate(
        winner,
        cell,
        winner_extent,
        source1.cell,
        source0.material,
        source1.material,
        0u,
        0u,
        vec4<u32>(0xffffffffu),
        vec4<f32>(0.0),
        vec4<u32>(0xffffffffu),
        vec4<f32>(0.0),
        vec2<u32>(0xffffffffu),
        vec2<u32>(source0.rigid_claim, source1.rigid_claim),
        vec2<u32>(0u));
  }
}
