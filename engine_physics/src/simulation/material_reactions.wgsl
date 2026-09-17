// Copyright Rob Gage 2026
#define_import_path compute::material_reactions
#import utility::material_identifier::{EMPTY_MATERIAL_IDENTIFIER, CELLULAR_STATIC_MATERIAL_FORM, CELLULAR_DYNAMIC_MATERIAL_FORM, GAS_MATERIAL_FORM, material_form_from_identifier, material_index_from_identifier}

// This is the fixed 27-word representation written by ReactionMaterialTable.
struct Reaction { words: array<u32, 27>, }
struct Candidate { reaction: u32, anchor: u32, extent: f32, partner: u32, material0: u32, material1: u32, padding0: u32, padding1: u32, }
struct Request { cell:u32, kind:u32, locator:u32, expected_source:u32, replacement:u32, amount:u32, temperature:u32, world_x:u32, world_y:u32, }
struct Parameters { cell_count: u32, gas_count: u32, reaction_count: u32, cell_width: u32, }
@group(0) @binding(0) var<storage, read> reactions: array<Reaction>;
@group(0) @binding(1) var<storage, read> selector_members: array<u32>;
@group(0) @binding(2) var<storage, read> material_identifiers: array<u32>;
@group(0) @binding(3) var<storage, read> amounts: array<f32>;
@group(0) @binding(4) var<storage, read> temperatures: array<f32>;
@group(0) @binding(5) var<storage, read> retained_pressure: array<vec4<f32>>;
@group(0) @binding(6) var<storage, read> fluid_coverage: array<f32>;
@group(0) @binding(7) var<storage, read_write> gas_concentrations: array<f32>;
@group(0) @binding(8) var<storage, read> external_occupancy: array<u32>;
@group(0) @binding(9) var<storage, read> rigid_claims: array<atomic<u32>>;
@group(0) @binding(10) var<storage, read_write> candidates: array<Candidate>;
@group(0) @binding(11) var<uniform> parameters: Parameters;
@group(0) @binding(12) var<storage, read_write> mutation_requests: array<Request>;
@group(0) @binding(13) var<storage, read_write> mutation_request_count: array<atomic<u32>>;
@group(0) @binding(14) var<storage, read_write> reaction_energy: array<f32>;
@group(0) @binding(15) var<storage, read_write> pending_pressure: array<vec4<f32>>;

fn matches_selector(rule: Reaction, reactant: u32, material: u32) -> bool {
  let base = reactant * 4u;
  if (rule.words[base + 3u] == 0u) { return true; }
  let offset = rule.words[base]; let count = rule.words[base + 1u];
  for (var i = 0u; i < count; i += 1u) {
    if (offset + i < arrayLength(&selector_members) && selector_members[offset + i] == material) { return true; }
  }
  return false;
}

struct Source { cell: u32, material: u32, amount: f32, found: bool, }

fn gas_source(cell: u32, reactant: u32, rule: Reaction) -> Source {
  for (var species = 0u; species < parameters.gas_count; species += 1u) {
    let material = species + 1u;
    let concentration = gas_concentrations[species * parameters.cell_count + cell];
    if (concentration > 0.000001 && matches_selector(rule, reactant, material)) {
      return Source(cell, material, concentration, true);
    }
  }
  return Source(cell, EMPTY_MATERIAL_IDENTIFIER, 0.0, false);
}

fn source_for(cell: u32, reactant: u32, rule: Reaction) -> Source {
  let material = material_identifiers[cell];
  if (material != EMPTY_MATERIAL_IDENTIFIER && matches_selector(rule, reactant, material)) {
    return Source(cell, material, amounts[cell], amounts[cell] > 0.000001);
  }
  return gas_source(cell, reactant, rule);
}

fn neighbor(anchor: u32, direction: u32) -> u32 {
  let width = parameters.cell_width;
  let height = parameters.cell_count / width;
  let x = anchor % width;
  let y = anchor / width;
  if (direction == 1u) { if (y == 0u) { return 0xffffffffu; } return anchor - width; }
  if (direction == 2u) { if (x + 1u >= width) { return 0xffffffffu; } return anchor + 1u; }
  if (direction == 3u) { if (y + 1u >= height) { return 0xffffffffu; } return anchor + width; }
  if (x == 0u) { return 0xffffffffu; }
  return anchor - 1u;
}

fn find_partner(anchor: u32, rule: Reaction) -> Source {
  let same = source_for(anchor, 1u, rule);
  if (same.found && (material_form_from_identifier(material_identifiers[anchor]) != material_form_from_identifier(same.material) || material_identifiers[anchor] == EMPTY_MATERIAL_IDENTIFIER)) { return same; }
  for (var direction = 1u; direction <= 4u; direction += 1u) {
    let cell = neighbor(anchor, direction);
    if (cell == 0xffffffffu) { continue; }
    let source = source_for(cell, 1u, rule);
    if (source.found) { return source; }
  }
  return Source(0u, EMPTY_MATERIAL_IDENTIFIER, 0.0, false);
}

fn gas_total(cell: u32) -> f32 {
  var total = 0.0;
  for (var species = 0u; species < parameters.gas_count; species += 1u) {
    total += max(gas_concentrations[species * parameters.cell_count + cell], 0.0);
  }
  return total;
}

// Applies only representations which can be reserved deterministically in the
// current pass: source canonical inventory plus zero/two gas outputs, or one
// cellular output in a source cell completely vacated by the reaction. Other
// product shapes are rejected before any mutation; later form-specific passes
// add fluid and rigid transactions using the same candidate record.
@compute @workgroup_size(64)
fn apply_canonical(@builtin(global_invocation_id) id: vec3<u32>) {
  let cell = id.x; if (cell >= parameters.cell_count || cell >= arrayLength(&candidates)) { return; }
  let candidate = candidates[cell]; if (candidate.reaction == 0xffffffffu || candidate.reaction >= arrayLength(&reactions) || candidate.extent <= 0.000001) { return; }
  let rule = reactions[candidate.reaction];
  let first_present = rule.words[3] != 0u; let second_present = rule.words[7] != 0u;
  let source0 = source_for(cell, 0u, rule);
  if (first_present && !source0.found) { return; }
  var source1 = Source(cell, EMPTY_MATERIAL_IDENTIFIER, 0.0, false);
  if (second_present) {
    source1 = Source(candidate.partner, candidate.material1, 0.0, candidate.partner != 0xffffffffu);
    if (source1.cell == cell) { source1.amount = source_for(cell, 1u, rule).amount; }
    else if (source1.found && material_form_from_identifier(source1.material) == GAS_MATERIAL_FORM) { source1.amount = gas_concentrations[material_index_from_identifier(source1.material) * parameters.cell_count + source1.cell]; }
    else if (source1.found) { source1.amount = amounts[source1.cell]; }
    if (!source1.found || source1.amount <= 0.000001) { return; }
  }
  let coefficient0 = select(0.0, bitcast<f32>(rule.words[2]), first_present);
  let coefficient1 = select(0.0, bitcast<f32>(rule.words[6]), second_present);
  if (first_present && source0.amount + 0.00001 < coefficient0 * candidate.extent) { return; }
  if (second_present && source1.amount + 0.00001 < coefficient1 * candidate.extent) { return; }
  let remaining = select(0.0, max(amounts[cell] - coefficient0 * candidate.extent, 0.0), first_present && material_form_from_identifier(source0.material) != GAS_MATERIAL_FORM);
  var cellular_product = EMPTY_MATERIAL_IDENTIFIER; var cellular_amount = 0.0;
  for (var product = 0u; product < 2u; product += 1u) {
    let base = 8u + product * 4u; if (rule.words[base + 2u] == 0u) { continue; }
    let replacement = rule.words[base]; let amount = bitcast<f32>(rule.words[base + 1u]) * candidate.extent;
    let form = material_form_from_identifier(replacement);
    if (form == GAS_MATERIAL_FORM) { continue; }
    if ((form != CELLULAR_STATIC_MATERIAL_FORM && form != CELLULAR_DYNAMIC_MATERIAL_FORM) || cellular_product != EMPTY_MATERIAL_IDENTIFIER || remaining > 0.00001 || !(amount > 0.000001)) { return; }
    cellular_product = replacement; cellular_amount = amount;
  }
  if (first_present && material_form_from_identifier(source0.material) != GAS_MATERIAL_FORM) {
    let slot = atomicAdd(&mutation_request_count[0], 1u);
    if (slot >= arrayLength(&mutation_requests)) { return; }
    let replacement = select(select(source0.material, cellular_product, cellular_product != EMPTY_MATERIAL_IDENTIFIER), EMPTY_MATERIAL_IDENTIFIER, remaining <= 0.00001 && cellular_product == EMPTY_MATERIAL_IDENTIFIER);
    let result_amount = select(remaining, cellular_amount, cellular_product != EMPTY_MATERIAL_IDENTIFIER);
    mutation_requests[slot] = Request(cell, 0u, cell, source0.material, replacement, bitcast<u32>(result_amount), bitcast<u32>(temperatures[cell]), 0u, 0u);
  }
  if (second_present && material_form_from_identifier(source1.material) != GAS_MATERIAL_FORM) {
    let slot = atomicAdd(&mutation_request_count[0], 1u);
    if (slot >= arrayLength(&mutation_requests)) { return; }
    mutation_requests[slot] = Request(source1.cell, 0u, source1.cell, source1.material, EMPTY_MATERIAL_IDENTIFIER, 0u, bitcast<u32>(temperatures[source1.cell]), 0u, 0u);
  }
  if (first_present && material_form_from_identifier(source0.material) == GAS_MATERIAL_FORM) { gas_concentrations[material_index_from_identifier(source0.material) * parameters.cell_count + cell] = max(source0.amount - coefficient0 * candidate.extent, 0.0); }
  if (second_present && material_form_from_identifier(source1.material) == GAS_MATERIAL_FORM) { gas_concentrations[material_index_from_identifier(source1.material) * parameters.cell_count + source1.cell] = max(source1.amount - coefficient1 * candidate.extent, 0.0); }
  reaction_energy[cell] += bitcast<f32>(rule.words[23]) * candidate.extent;
  let pressure_output = bitcast<f32>(rule.words[24]) * candidate.extent;
  pending_pressure[cell] += vec4<f32>(pressure_output);
  for (var product = 0u; product < 2u; product += 1u) {
    let base = 8u + product * 4u; if (rule.words[base + 2u] == 0u) { continue; }
    let replacement = rule.words[base];
    if (material_form_from_identifier(replacement) == GAS_MATERIAL_FORM) {
      let species = material_index_from_identifier(replacement);
      if (species < parameters.gas_count) {
        let output_cell = select(cell, source1.cell, remaining > 0.00001 && source1.found);
        gas_concentrations[species * parameters.cell_count + output_cell] += bitcast<f32>(rule.words[base + 1u]) * candidate.extent;
      }
    }
  }
}
fn has_environment(rule: Reaction) -> bool {
  let a = bitcast<f32>(rule.words[16]); let b = bitcast<f32>(rule.words[17]);
  let c = bitcast<f32>(rule.words[18]); let d = bitcast<f32>(rule.words[19]);
  let e = bitcast<f32>(rule.words[20]); let f = bitcast<f32>(rule.words[21]);
  return a == a || b == b || c == c || d == d || e == e || f == f;
}
fn environment_matches(rule: Reaction, cell: u32, air: f32) -> bool {
  let temperature = temperatures[cell];
  let pressure = length(retained_pressure[cell].xy);
  let min_temperature = bitcast<f32>(rule.words[16]); let max_temperature = bitcast<f32>(rule.words[17]);
  let min_pressure = bitcast<f32>(rule.words[18]); let max_pressure = bitcast<f32>(rule.words[19]);
  let min_air = bitcast<f32>(rule.words[20]); let max_air = bitcast<f32>(rule.words[21]);
  return (min_temperature != min_temperature || temperature >= min_temperature) &&
    (max_temperature != max_temperature || temperature <= max_temperature) &&
    (min_pressure != min_pressure || pressure >= min_pressure) &&
    (max_pressure != max_pressure || pressure <= max_pressure) &&
    (min_air != min_air || air >= min_air) && (max_air != max_air || air <= max_air);
}

// Discovery only reads the immutable fields for this chemistry tick. Applying
// a later candidate is intentionally a separate pass, preventing same-tick
// reaction cascades from products or field outputs.
@compute @workgroup_size(64)
fn discover_canonical(@builtin(global_invocation_id) id: vec3<u32>) {
  let cell = id.x;
  if (cell >= parameters.cell_count || cell >= arrayLength(&candidates)) { return; }
  candidates[cell] = Candidate(0xffffffffu, cell, 0.0, 0xffffffffu, 0u, 0u, 0u, 0u);
  reaction_energy[cell] = 0.0;
  if (atomicLoad(&rigid_claims[cell]) != 0xffffffffu) { return; }
  let material = material_identifiers[cell];
  let blocked = material != EMPTY_MATERIAL_IDENTIFIER || external_occupancy[cell] != 0u;
  var gas = 0.0;
  for (var species = 0u; species < parameters.gas_count; species += 1u) {
    gas += max(gas_concentrations[species * parameters.cell_count + cell], 0.0);
  }
  let air = select(clamp(1.0 - clamp(fluid_coverage[cell], 0.0, 1.0) - gas, 0.0, 1.0), 0.0, blocked);
  var winner = 0xffffffffu; var winner_extent = 0.0; var winner_priority = -2147483647i; var winner_order = 0xffffffffu;
  for (var reaction_index = 0u; reaction_index < parameters.reaction_count && reaction_index < arrayLength(&reactions); reaction_index += 1u) {
    let rule = reactions[reaction_index];
    if (!environment_matches(rule, cell, air)) { continue; }
    let first_present = rule.words[3] != 0u; let second_present = rule.words[7] != 0u;
    let source0 = source_for(cell, 0u, rule);
    if (first_present && !source0.found) { continue; }
    if (!first_present && !has_environment(rule)) { continue; }
    var source1 = Source(cell, EMPTY_MATERIAL_IDENTIFIER, 0.0, false);
    if (second_present) {
      source1 = find_partner(cell, rule);
      if (!source1.found || (source1.cell != cell && cell > source1.cell)) { continue; }
    }
    let extent0 = select(1.0, source0.amount / max(bitcast<f32>(rule.words[2]), 0.000001), first_present);
    let extent1 = select(1.0, source1.amount / max(bitcast<f32>(rule.words[6]), 0.000001), second_present);
    let extent = min(bitcast<f32>(rule.words[22]), min(extent0, extent1));
    var output_cell = cell;
    if (source0.found && material_form_from_identifier(source0.material) != GAS_MATERIAL_FORM && source1.found && source1.cell != cell && source0.amount <= bitcast<f32>(rule.words[2]) * extent + 0.00001) { output_cell = source1.cell; }
    var gas_product = 0.0;
    for (var product = 0u; product < 2u; product += 1u) {
      let base = 8u + product * 4u;
      if (rule.words[base + 2u] != 0u && material_form_from_identifier(rule.words[base]) == GAS_MATERIAL_FORM) { gas_product += max(bitcast<f32>(rule.words[base + 1u]), 0.0); }
    }
    let capacity_extent = select(extent, max(1.0 - gas_total(output_cell), 0.0) / max(gas_product, 0.000001), gas_product > 0.0);
    if (!(min(extent, capacity_extent) > 0.000001)) { continue; }
    let priority = bitcast<i32>(rule.words[25]); let order = rule.words[26];
    if (priority > winner_priority || (priority == winner_priority && order < winner_order)) {
      winner = reaction_index; winner_extent = min(extent, capacity_extent); winner_priority = priority; winner_order = order;
    }
  }
  if (winner != 0xffffffffu) {
    let rule = reactions[winner];
    let source0 = source_for(cell, 0u, rule);
    var source1 = Source(cell, EMPTY_MATERIAL_IDENTIFIER, 0.0, false);
    if (rule.words[7] != 0u) { source1 = find_partner(cell, rule); }
    let extent0 = select(1.0, source0.amount / max(bitcast<f32>(rule.words[2]), 0.000001), rule.words[3] != 0u);
    let extent1 = select(1.0, source1.amount / max(bitcast<f32>(rule.words[6]), 0.000001), rule.words[7] != 0u);
    candidates[cell] = Candidate(winner, cell, winner_extent, source1.cell, source0.material, source1.material, 0u, 0u);
  }
}
