// Copyright Rob Gage 2026
#define_import_path compute::material_reactions
#import utility::material_identifier::{EMPTY_MATERIAL_IDENTIFIER, CELLULAR_STATIC_MATERIAL_FORM, CELLULAR_DYNAMIC_MATERIAL_FORM, GAS_MATERIAL_FORM, material_form_from_identifier, material_index_from_identifier}

// This is the fixed 27-word representation written by ReactionMaterialTable.
struct Reaction { words: array<u32, 27>, }
struct Candidate { reaction: u32, anchor: u32, extent: f32, padding: u32, }
struct Request { cell:u32, kind:u32, locator:u32, expected_source:u32, replacement:u32, amount:u32, temperature:u32, world_x:u32, world_y:u32, }
struct Parameters { cell_count: u32, gas_count: u32, reaction_count: u32, padding: u32, }
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
  reaction_energy[cell] += bitcast<f32>(rule.words[23]) * candidate.extent;
  let first_present = rule.words[3] != 0u; let second_present = rule.words[7] != 0u;
  if (second_present) { return; }
  let source = material_identifiers[cell];
  if (first_present && (!matches_selector(rule, 0u, source) || amounts[cell] <= 0.000001)) { return; }
  let coefficient = select(0.0, bitcast<f32>(rule.words[2]), first_present);
  let consumed = coefficient * candidate.extent;
  if (first_present && (consumed <= 0.0 || consumed > amounts[cell] + 0.00001)) { return; }
  let remaining = max(amounts[cell] - consumed, 0.0);
  var cellular_product = EMPTY_MATERIAL_IDENTIFIER; var cellular_amount = 0.0;
  for (var product = 0u; product < 2u; product += 1u) {
    let base = 8u + product * 4u; if (rule.words[base + 2u] == 0u) { continue; }
    let replacement = rule.words[base]; let amount = bitcast<f32>(rule.words[base + 1u]) * candidate.extent;
    let form = material_form_from_identifier(replacement);
    if (form == GAS_MATERIAL_FORM) { continue; }
    if ((form != CELLULAR_STATIC_MATERIAL_FORM && form != CELLULAR_DYNAMIC_MATERIAL_FORM) || cellular_product != EMPTY_MATERIAL_IDENTIFIER || remaining > 0.00001 || !(amount > 0.000001)) { return; }
    cellular_product = replacement; cellular_amount = amount;
  }
  // Reserve the source mutation queue entry before writing any gas products.
  if (first_present) {
    let slot = atomicAdd(&mutation_request_count[0], 1u);
    if (slot >= arrayLength(&mutation_requests)) { return; }
    let replacement = select(select(source, cellular_product, cellular_product != EMPTY_MATERIAL_IDENTIFIER), EMPTY_MATERIAL_IDENTIFIER, remaining <= 0.00001 && cellular_product == EMPTY_MATERIAL_IDENTIFIER);
    let result_amount = select(remaining, cellular_amount, cellular_product != EMPTY_MATERIAL_IDENTIFIER);
    mutation_requests[slot] = Request(cell, 0u, cell, source, replacement, bitcast<u32>(result_amount), bitcast<u32>(temperatures[cell]), 0u, 0u);
  }
  let pressure_output = bitcast<f32>(rule.words[24]) * candidate.extent;
  pending_pressure[cell] += vec4<f32>(pressure_output);
  for (var product = 0u; product < 2u; product += 1u) {
    let base = 8u + product * 4u; if (rule.words[base + 2u] == 0u) { continue; }
    let replacement = rule.words[base];
    if (material_form_from_identifier(replacement) == GAS_MATERIAL_FORM) {
      let species = material_index_from_identifier(replacement);
      if (species < parameters.gas_count) { gas_concentrations[species * parameters.cell_count + cell] += bitcast<f32>(rule.words[base + 1u]) * candidate.extent; }
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
  candidates[cell] = Candidate(0xffffffffu, cell, 0.0, 0u);
  reaction_energy[cell] = 0.0;
  if (atomicLoad(&rigid_claims[cell]) != 0xffffffffu) { return; }
  let material = material_identifiers[cell];
  let blocked = material != EMPTY_MATERIAL_IDENTIFIER || external_occupancy[cell] != 0u;
  var gas = 0.0;
  for (var species = 0u; species < parameters.gas_count; species += 1u) {
    gas += max(gas_concentrations[species * parameters.cell_count + cell], 0.0);
  }
  let air = select(clamp(1.0 - clamp(fluid_coverage[cell], 0.0, 1.0) - gas, 0.0, 1.0), 0.0, blocked);
  var winner = 0xffffffffu; var winner_priority = -2147483647i; var winner_order = 0xffffffffu;
  for (var reaction_index = 0u; reaction_index < parameters.reaction_count && reaction_index < arrayLength(&reactions); reaction_index += 1u) {
    let rule = reactions[reaction_index];
    if (!environment_matches(rule, cell, air)) { continue; }
    let first_present = rule.words[3] != 0u; let second_present = rule.words[7] != 0u;
    // Canonical-only candidates are the base path. Other authoritative forms
    // are discovered by their form-specific passes, sharing this exact record.
    if (first_present && (!matches_selector(rule, 0u, material) || amounts[cell] <= 0.000001)) { continue; }
    if (second_present) { continue; }
    if (!first_present && !has_environment(rule)) { continue; }
    let priority = bitcast<i32>(rule.words[25]); let order = rule.words[26];
    if (priority > winner_priority || (priority == winner_priority && order < winner_order)) {
      winner = reaction_index; winner_priority = priority; winner_order = order;
    }
  }
  if (winner != 0xffffffffu) {
    let first_present = reactions[winner].words[3] != 0u;
    let coefficient = max(bitcast<f32>(reactions[winner].words[2]), 0.000001);
    let extent = select(bitcast<f32>(reactions[winner].words[22]), min(bitcast<f32>(reactions[winner].words[22]), amounts[cell] / coefficient), first_present);
    candidates[cell] = Candidate(winner, cell, extent, 0u);
  }
}
