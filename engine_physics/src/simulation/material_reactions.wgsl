// Copyright Rob Gage 2026
#define_import_path compute::material_reactions
#import utility::material_identifier::{EMPTY_MATERIAL_IDENTIFIER, CELLULAR_STATIC_MATERIAL_FORM, CELLULAR_DYNAMIC_MATERIAL_FORM, FLUID_MATERIAL_FORM, GAS_MATERIAL_FORM, material_form_from_identifier, material_index_from_identifier}
#import utility::fluid_spatial::{fluid_particle_world_cell, fluid_bucket_coordinates_from_position, fluid_bucket_index_from_coordinates, fluid_particle_belongs_to_cell}
#import utility::cell_coordinates::CELLS_PER_TILE_FLOAT
#import utility::tile_ring::world_cell_from_physical_tile_ring_index

// This is the fixed 27-word representation written by ReactionMaterialTable.
struct Reaction { words: array<u32, 27>, }
struct Candidate {
  reaction: u32, anchor: u32, extent: f32, partner: u32,
  material0: u32, material1: u32, padding0: u32, padding1: u32,
  fluid0_indices: vec4<u32>, fluid0_amounts: vec4<f32>,
  fluid1_indices: vec4<u32>, fluid1_amounts: vec4<f32>,
  product_slots: vec2<u32>,
}
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
// Read-only authoritative fluid bridge; discovery will use the shared spatial
// utility and these chains in the fluid-reaction pass.
struct FluidParticleAuthority { material_identifier: u32, is_active: u32, position: vec2<f32>, velocity: vec2<f32>, prediction_collision_displacement: vec2<f32>, amount: f32, temperature: f32, }
@group(0) @binding(16) var<storage, read_write> fluid_particles: array<FluidParticleAuthority>;
@group(0) @binding(17) var<storage, read> fluid_bucket_heads: array<atomic<u32>>;
@group(0) @binding(18) var<storage, read> fluid_next_particle: array<u32>;
struct FluidSpatialParameters {
  buffered_origin: vec2<i32>, buffered_tile_size: vec2<u32>,
  active_origin: vec2<i32>, active_tile_size: vec2<u32>,
  ring_offset: vec2<u32>, bucket_dimensions: vec2<u32>,
  streaming_origin: vec2<i32>, streaming_tile_size: vec2<u32>,
  gravity: vec2<f32>, delta_time: f32, particle_capacity: u32,
  buffered_cell_count: u32, bucket_count: u32, support_radius_cells: f32,
  particle_radius_cells: f32, maximum_movement_cells: u32, padding_0: u32,
  sample_center: vec2<f32>, sample_shape_parameters: vec2<f32>,
  sample_shape_kind: u32, padding_1: u32,
}
@group(0) @binding(19) var<uniform> fluid_spatial_parameters: FluidSpatialParameters;
@group(0) @binding(20) var<storage, read_write> fluid_free_indices: array<u32>;
@group(0) @binding(21) var<storage, read_write> fluid_free_count: array<atomic<u32>>;
@group(0) @binding(22) var<storage, read_write> fluid_reservations: array<atomic<u32>>;

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

fn fluid_source(cell: u32, reactant: u32, rule: Reaction) -> Source {
  let world = world_cell_from_physical_tile_ring_index(cell,
    fluid_spatial_parameters.buffered_origin, fluid_spatial_parameters.buffered_tile_size,
    fluid_spatial_parameters.ring_offset);
  let center = (vec2<f32>(world) + vec2<f32>(0.5)) / CELLS_PER_TILE_FLOAT;
  let base = fluid_bucket_coordinates_from_position(center,
    fluid_spatial_parameters.buffered_origin, fluid_spatial_parameters.support_radius_cells,
    CELLS_PER_TILE_FLOAT);
  var total = 0.0; var chosen = EMPTY_MATERIAL_IDENTIFIER; var chosen_index = 0xffffffffu;
  for (var y: i32 = -1; y <= 1; y += 1) {
    for (var x: i32 = -1; x <= 1; x += 1) {
      let bucket = fluid_bucket_index_from_coordinates(base + vec2<i32>(x, y),
        fluid_spatial_parameters.bucket_dimensions, 0xffffffffu);
      if (bucket == 0xffffffffu) { continue; }
      var p = atomicLoad(&fluid_bucket_heads[bucket]);
      for (var n: u32 = 0u; p != 0xffffffffu && n < fluid_spatial_parameters.particle_capacity; n += 1u) {
        let particle = fluid_particles[p];
        if (particle.is_active != 0u && particle.material_identifier != EMPTY_MATERIAL_IDENTIFIER &&
            material_form_from_identifier(particle.material_identifier) == FLUID_MATERIAL_FORM &&
            fluid_particle_belongs_to_cell(particle.position, world, CELLS_PER_TILE_FLOAT) &&
            particle.amount > 0.000001 && matches_selector(rule, reactant, particle.material_identifier)) {
          total += max(particle.amount, 0.0);
          if (p < chosen_index) { chosen_index = p; chosen = particle.material_identifier; }
        }
        p = fluid_next_particle[p];
      }
    }
  }
  return Source(cell, chosen, total, total > 0.000001);
}

fn source_for(cell: u32, reactant: u32, rule: Reaction) -> Source {
  let material = material_identifiers[cell];
  if (material != EMPTY_MATERIAL_IDENTIFIER && matches_selector(rule, reactant, material)) {
    return Source(cell, material, amounts[cell], amounts[cell] > 0.000001);
  }
  let gas = gas_source(cell, reactant, rule);
  if (gas.found) { return gas; }
  return fluid_source(cell, reactant, rule);
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

// Reserves fluid inventory without touching authoritative particles. The
// bounded four-contributor plan is retained on the candidate for apply.
fn reserve_fluid_plan(record: u32, cell: u32, reactant: u32, rule: Reaction, required: f32) -> bool {
  var remaining = required;
  let world = world_cell_from_physical_tile_ring_index(cell,
    fluid_spatial_parameters.buffered_origin, fluid_spatial_parameters.buffered_tile_size,
    fluid_spatial_parameters.ring_offset);
  let center = (vec2<f32>(world) + vec2<f32>(0.5)) / CELLS_PER_TILE_FLOAT;
  let base = fluid_bucket_coordinates_from_position(center,
    fluid_spatial_parameters.buffered_origin, fluid_spatial_parameters.support_radius_cells,
    CELLS_PER_TILE_FLOAT);
  var contributor = 0u;
  for (var iteration: u32 = 0u; iteration < fluid_spatial_parameters.particle_capacity && remaining > 0.000001; iteration += 1u) {
    var selected = 0xffffffffu;
    for (var y: i32 = -1; y <= 1; y += 1) {
      for (var x: i32 = -1; x <= 1; x += 1) {
        let bucket = fluid_bucket_index_from_coordinates(base + vec2<i32>(x, y), fluid_spatial_parameters.bucket_dimensions, 0xffffffffu);
        if (bucket == 0xffffffffu) { continue; }
        var p = atomicLoad(&fluid_bucket_heads[bucket]);
        for (var n: u32 = 0u; p != 0xffffffffu && n < fluid_spatial_parameters.particle_capacity; n += 1u) {
          let particle = fluid_particles[p];
          if (p >= selected && selected != 0xffffffffu) { p = fluid_next_particle[p]; continue; }
          if (particle.is_active != 0u && material_form_from_identifier(particle.material_identifier) == FLUID_MATERIAL_FORM &&
              fluid_particle_belongs_to_cell(particle.position, world, CELLS_PER_TILE_FLOAT) &&
              particle.amount > 0.000001 && matches_selector(rule, reactant, particle.material_identifier) &&
              atomicLoad(&fluid_reservations[p]) < u32(max(particle.amount, 0.0) * 1000000.0)) { selected = p; }
          p = fluid_next_particle[p];
        }
      }
    }
    if (selected == 0xffffffffu || contributor >= 4u) {
      for (var rollback = 0u; rollback < contributor; rollback += 1u) {
        let index = select(candidates[record].fluid0_indices[rollback], candidates[record].fluid1_indices[rollback], reactant == 1u);
        let amount = select(candidates[record].fluid0_amounts[rollback], candidates[record].fluid1_amounts[rollback], reactant == 1u);
        atomicSub(&fluid_reservations[index], u32(amount * 1000000.0));
      }
      return false;
    }
    let particle = fluid_particles[selected];
    let already = f32(atomicLoad(&fluid_reservations[selected])) / 1000000.0;
    let available = max(particle.amount - already, 0.0);
    let take = min(remaining, available);
    let units = u32(max(take, 0.0) * 1000000.0);
    let old = atomicLoad(&fluid_reservations[selected]);
    let result = atomicCompareExchangeWeak(&fluid_reservations[selected], old, old + units);
    if (result.exchanged) {
      if (reactant == 0u) {
        candidates[record].fluid0_indices[contributor] = selected;
        candidates[record].fluid0_amounts[contributor] = f32(units) / 1000000.0;
      } else {
        candidates[record].fluid1_indices[contributor] = selected;
        candidates[record].fluid1_amounts[contributor] = f32(units) / 1000000.0;
      }
      contributor += 1u;
      remaining -= take;
    }
  }
  if (remaining > 0.00001) {
    for (var rollback = 0u; rollback < contributor; rollback += 1u) {
      let index = select(candidates[record].fluid0_indices[rollback], candidates[record].fluid1_indices[rollback], reactant == 1u);
      let amount = select(candidates[record].fluid0_amounts[rollback], candidates[record].fluid1_amounts[rollback], reactant == 1u);
      atomicSub(&fluid_reservations[index], u32(amount * 1000000.0));
    }
    return false;
  }
  return true;
}

fn reserve_fluid_slot() -> u32 {
  var count = atomicLoad(&fluid_free_count[0]);
  loop {
    if (count == 0u) { return 0xffffffffu; }
    let result = atomicCompareExchangeWeak(&fluid_free_count[0], count, count - 1u);
    if (result.exchanged) { return fluid_free_indices[count - 1u]; }
    count = result.old_value;
  }
  return 0xffffffffu;
}

fn apply_fluid_plan(cell: u32, reactant: u32) {
  for (var i = 0u; i < 4u; i += 1u) {
    let index = select(candidates[cell].fluid0_indices[i], candidates[cell].fluid1_indices[i], reactant == 1u);
    let amount = select(candidates[cell].fluid0_amounts[i], candidates[cell].fluid1_amounts[i], reactant == 1u);
    if (index == 0xffffffffu || amount <= 0.0) { continue; }
    let particle = fluid_particles[index];
    let next_amount = max(particle.amount - amount, 0.0);
    fluid_particles[index].amount = next_amount;
    atomicStore(&fluid_reservations[index], 0u);
    if (next_amount <= 0.000001) {
      fluid_particles[index].material_identifier = EMPTY_MATERIAL_IDENTIFIER;
      fluid_particles[index].is_active = 0u;
      let free = atomicAdd(&fluid_free_count[0], 1u);
      if (free < arrayLength(&fluid_free_indices)) { fluid_free_indices[free] = index; }
    }
  }
}

fn rollback_fluid_plan(cell: u32, reactant: u32) {
  for (var i = 0u; i < 4u; i += 1u) {
    let index = select(candidates[cell].fluid0_indices[i], candidates[cell].fluid1_indices[i], reactant == 1u);
    let amount = select(candidates[cell].fluid0_amounts[i], candidates[cell].fluid1_amounts[i], reactant == 1u);
    if (index != 0xffffffffu && amount > 0.0) { atomicSub(&fluid_reservations[index], u32(amount * 1000000.0)); }
  }
}

@compute @workgroup_size(64)
fn reserve_fluid_authority(@builtin(global_invocation_id) id: vec3<u32>) {
  let cell = id.x;
  if (cell >= parameters.cell_count || cell >= arrayLength(&candidates)) { return; }
  if (cell < arrayLength(&fluid_reservations)) { atomicStore(&fluid_reservations[cell], 0u); }
  let candidate = candidates[cell];
  if (candidate.reaction == 0xffffffffu || candidate.extent <= 0.000001) { return; }
  candidates[cell].padding0 = 1u;
  if (material_form_from_identifier(candidate.material0) == FLUID_MATERIAL_FORM &&
      !reserve_fluid_plan(cell, cell, 0u, reactions[candidate.reaction],
        bitcast<f32>(reactions[candidate.reaction].words[2]) * candidate.extent)) {
    candidates[cell].padding0 = 0u; return;
  }
  if (material_form_from_identifier(candidate.material1) == FLUID_MATERIAL_FORM &&
      !reserve_fluid_plan(cell, candidate.partner, 1u, reactions[candidate.reaction],
        bitcast<f32>(reactions[candidate.reaction].words[6]) * candidate.extent)) {
    if (material_form_from_identifier(candidate.material0) == FLUID_MATERIAL_FORM) { rollback_fluid_plan(cell, 0u); }
    candidates[cell].padding0 = 0u;
    return;
  }
  var output_slots = vec2<u32>(0xffffffffu);
  for (var product = 0u; product < 2u; product += 1u) {
    let base = 8u + product * 4u;
    if (reactions[candidate.reaction].words[base + 2u] == 0u ||
        material_form_from_identifier(reactions[candidate.reaction].words[base]) != FLUID_MATERIAL_FORM) { continue; }
    let slot = reserve_fluid_slot();
    if (slot == 0xffffffffu) {
      if (material_form_from_identifier(candidate.material0) == FLUID_MATERIAL_FORM) { rollback_fluid_plan(cell, 0u); }
      if (material_form_from_identifier(candidate.material1) == FLUID_MATERIAL_FORM) { rollback_fluid_plan(cell, 1u); }
      release_fluid_slot(output_slots.x); release_fluid_slot(output_slots.y);
      candidates[cell].padding0 = 0u;
      return;
    }
    if (product == 0u) { output_slots.x = slot; } else { output_slots.y = slot; }
  }
  candidates[cell].product_slots = output_slots;
  var request_count = 0u;
  if (candidate.material0 != EMPTY_MATERIAL_IDENTIFIER &&
      material_form_from_identifier(candidate.material0) != GAS_MATERIAL_FORM &&
      material_form_from_identifier(candidate.material0) != FLUID_MATERIAL_FORM) { request_count += 1u; }
  if (candidate.material1 != EMPTY_MATERIAL_IDENTIFIER &&
      material_form_from_identifier(candidate.material1) != GAS_MATERIAL_FORM &&
      material_form_from_identifier(candidate.material1) != FLUID_MATERIAL_FORM) { request_count += 1u; }
  let request_base = atomicAdd(&mutation_request_count[0], request_count);
  if (request_base + request_count > arrayLength(&mutation_requests)) {
    if (material_form_from_identifier(candidate.material0) == FLUID_MATERIAL_FORM) { rollback_fluid_plan(cell, 0u); }
    if (material_form_from_identifier(candidate.material1) == FLUID_MATERIAL_FORM) { rollback_fluid_plan(cell, 1u); }
    release_fluid_slot(output_slots.x); release_fluid_slot(output_slots.y);
    candidates[cell].padding0 = 0u;
    return;
  }
  candidates[cell].padding1 = request_base;
}

fn release_fluid_slot(slot: u32) {
  if (slot == 0xffffffffu) { return; }
  let count = atomicAdd(&fluid_free_count[0], 1u);
  if (count < arrayLength(&fluid_free_indices)) { fluid_free_indices[count] = slot; }
}

fn spawn_fluid_product(slot: u32, material: u32, amount: f32, cell: u32, temperature: f32) {
  let world = world_cell_from_physical_tile_ring_index(cell,
    fluid_spatial_parameters.buffered_origin, fluid_spatial_parameters.buffered_tile_size,
    fluid_spatial_parameters.ring_offset);
  fluid_particles[slot] = FluidParticleAuthority(material, 1u,
    (vec2<f32>(world) + vec2<f32>(0.5)) / CELLS_PER_TILE_FLOAT,
    vec2<f32>(0.0), vec2<f32>(0.0), amount, temperature);
}

// Applies only representations which can be reserved deterministically in the
// current pass: source canonical inventory plus zero/two gas outputs, or one
// cellular output in a source cell completely vacated by the reaction. Other
// product shapes are rejected before any mutation; later form-specific passes
// add fluid and rigid transactions using the same candidate record.
@compute @workgroup_size(64)
fn apply_canonical(@builtin(global_invocation_id) id: vec3<u32>) {
  let cell = id.x; if (cell >= parameters.cell_count || cell >= arrayLength(&candidates)) { return; }
  let candidate = candidates[cell]; if (candidate.reaction == 0xffffffffu || candidate.reaction >= arrayLength(&reactions) || candidate.extent <= 0.000001 || candidate.padding0 == 0u) { return; }
  let rule = reactions[candidate.reaction];
  let first_present = rule.words[3] != 0u; let second_present = rule.words[7] != 0u;
  let source0 = source_for(cell, 0u, rule);
  if (first_present && !source0.found) { return; }
  var source1 = Source(cell, EMPTY_MATERIAL_IDENTIFIER, 0.0, false);
  if (second_present) {
    source1 = Source(candidate.partner, candidate.material1, 0.0, candidate.partner != 0xffffffffu);
    if (source1.cell == cell) { source1.amount = source_for(cell, 1u, rule).amount; }
    else if (source1.found && material_form_from_identifier(source1.material) == GAS_MATERIAL_FORM) { source1.amount = gas_concentrations[material_index_from_identifier(source1.material) * parameters.cell_count + source1.cell]; }
    else if (source1.found && material_form_from_identifier(source1.material) == FLUID_MATERIAL_FORM) { source1.amount = fluid_source(source1.cell, 1u, rule).amount; }
    else if (source1.found) { source1.amount = amounts[source1.cell]; }
    if (!source1.found || source1.amount <= 0.000001) { return; }
  }
  // Preflight the deferred canonical mutation queue before reserving any
  // fluid output slots or consuming fluid inventory. This keeps queue-capacity
  // failure from producing a partially applied cross-form reaction.
  var required_requests = 0u;
  if (first_present && material_form_from_identifier(source0.material) != GAS_MATERIAL_FORM &&
      material_form_from_identifier(source0.material) != FLUID_MATERIAL_FORM) { required_requests += 1u; }
  if (second_present && material_form_from_identifier(source1.material) != GAS_MATERIAL_FORM &&
      material_form_from_identifier(source1.material) != FLUID_MATERIAL_FORM) { required_requests += 1u; }
  if (atomicLoad(&mutation_request_count[0]) + required_requests > arrayLength(&mutation_requests)) { return; }
  let coefficient0 = select(0.0, bitcast<f32>(rule.words[2]), first_present);
  let coefficient1 = select(0.0, bitcast<f32>(rule.words[6]), second_present);
  if (first_present && source0.amount + 0.00001 < coefficient0 * candidate.extent) { return; }
  if (second_present && source1.amount + 0.00001 < coefficient1 * candidate.extent) { return; }
  let remaining = select(0.0, max(amounts[cell] - coefficient0 * candidate.extent, 0.0), first_present && material_form_from_identifier(source0.material) != GAS_MATERIAL_FORM);
  var cellular_product = EMPTY_MATERIAL_IDENTIFIER; var cellular_amount = 0.0;
  let fluid_product_slots = candidate.product_slots;
  for (var product = 0u; product < 2u; product += 1u) {
    let base = 8u + product * 4u; if (rule.words[base + 2u] == 0u) { continue; }
    let replacement = rule.words[base]; let amount = bitcast<f32>(rule.words[base + 1u]) * candidate.extent;
    let form = material_form_from_identifier(replacement);
    if (form == GAS_MATERIAL_FORM) { continue; }
    if (form == FLUID_MATERIAL_FORM) {
      let product_index = product;
      continue;
    }
    if ((form != CELLULAR_STATIC_MATERIAL_FORM && form != CELLULAR_DYNAMIC_MATERIAL_FORM) || cellular_product != EMPTY_MATERIAL_IDENTIFIER || remaining > 0.00001 || !(amount > 0.000001)) {
      release_fluid_slot(fluid_product_slots.x); release_fluid_slot(fluid_product_slots.y); return;
    }
    cellular_product = replacement; cellular_amount = amount;
  }
  let source0_fluid = first_present && material_form_from_identifier(source0.material) == FLUID_MATERIAL_FORM;
  let source1_fluid = second_present && material_form_from_identifier(source1.material) == FLUID_MATERIAL_FORM;
  if (first_present && material_form_from_identifier(source0.material) != GAS_MATERIAL_FORM && !source0_fluid) {
    let slot = candidate.padding1;
    let replacement = select(select(source0.material, cellular_product, cellular_product != EMPTY_MATERIAL_IDENTIFIER), EMPTY_MATERIAL_IDENTIFIER, remaining <= 0.00001 && cellular_product == EMPTY_MATERIAL_IDENTIFIER);
    let result_amount = select(remaining, cellular_amount, cellular_product != EMPTY_MATERIAL_IDENTIFIER);
    mutation_requests[slot] = Request(cell, 0u, cell, source0.material, replacement, bitcast<u32>(result_amount), bitcast<u32>(temperatures[cell]), 0u, 0u);
  }
  if (second_present && material_form_from_identifier(source1.material) != GAS_MATERIAL_FORM && !source1_fluid) {
    let slot = candidate.padding1 + select(0u, 1u, first_present && !source0_fluid && material_form_from_identifier(source0.material) != GAS_MATERIAL_FORM);
    mutation_requests[slot] = Request(source1.cell, 0u, source1.cell, source1.material, EMPTY_MATERIAL_IDENTIFIER, 0u, bitcast<u32>(temperatures[source1.cell]), 0u, 0u);
  }
  if (first_present && material_form_from_identifier(source0.material) == GAS_MATERIAL_FORM) { gas_concentrations[material_index_from_identifier(source0.material) * parameters.cell_count + cell] = max(source0.amount - coefficient0 * candidate.extent, 0.0); }
  if (second_present && material_form_from_identifier(source1.material) == GAS_MATERIAL_FORM) { gas_concentrations[material_index_from_identifier(source1.material) * parameters.cell_count + source1.cell] = max(source1.amount - coefficient1 * candidate.extent, 0.0); }
  if (source0_fluid) { apply_fluid_plan(cell, 0u); }
  if (source1_fluid) { apply_fluid_plan(cell, 1u); }
  for (var product = 0u; product < 2u; product += 1u) {
    let base = 8u + product * 4u; if (rule.words[base + 2u] == 0u) { continue; }
    let replacement = rule.words[base];
    if (material_form_from_identifier(replacement) == FLUID_MATERIAL_FORM) {
      let slot = select(fluid_product_slots.x, fluid_product_slots.y, product == 1u);
      spawn_fluid_product(slot, replacement, bitcast<f32>(rule.words[base + 1u]) * candidate.extent,
        cell, temperatures[cell]);
    }
  }
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
  if (cell < arrayLength(&fluid_reservations)) { atomicStore(&fluid_reservations[cell], 0u); }
  if (cell >= parameters.cell_count || cell >= arrayLength(&candidates)) { return; }
  candidates[cell] = Candidate(0xffffffffu, cell, 0.0, 0xffffffffu, 0u, 0u, 0u, 0u,
    vec4<u32>(0xffffffffu), vec4<f32>(0.0), vec4<u32>(0xffffffffu), vec4<f32>(0.0), vec2<u32>(0xffffffffu));
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
    candidates[cell] = Candidate(winner, cell, winner_extent, source1.cell, source0.material, source1.material, 0u, 0u,
      vec4<u32>(0xffffffffu), vec4<f32>(0.0), vec4<u32>(0xffffffffu), vec4<f32>(0.0), vec2<u32>(0xffffffffu));
  }
}
