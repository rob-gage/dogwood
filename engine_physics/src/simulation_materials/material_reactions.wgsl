// Copyright Rob Gage 2026
#define_import_path compute::material_reactions
#import utility::simulation_constants::{
    CELLS_PER_TILE,
    CELLS_PER_TILE_FLOAT,
    CELL_COUNT_PER_TILE,
    EMPTY_MATERIAL_IDENTIFIER,
    GAS_MATERIAL_FORM,
    CELLULAR_STATIC_MATERIAL_FORM,
    CELLULAR_DYNAMIC_MATERIAL_FORM,
    FLUID_MATERIAL_FORM,
    MATERIAL_IDENTIFIER_INDEX_MASK,
    INVALID_MATERIAL_DENSE_INDEX,
    FLUID_EDIT_ERASE,
    INVALID_PHYSICAL_CELL_INDEX,
    INVALID_CELLULAR_DYNAMIC_CLAIM_INDEX,
    INVALID_FLUID_PARTICLE_INDEX,
    INVALID_FLUID_BUCKET_INDEX,
    ACTOR_SHAPE_CIRCLE,
    ACTOR_SHAPE_CAPSULE,
    ACTOR_SHAPE_RECTANGLE,
    PI,
    PBF_SUBSTEP_COUNT,
    PBF_CONSTRAINT_ITERATION_COUNT,
    CONSTRAINT_EPSILON,
    ARTIFICIAL_PRESSURE_DELTA_Q_RATIO,
    MAXIMUM_CORRECTION_CELLS,
    HARD_EXTERNAL_BODY_OCCUPANCY,
    SWIMMER_EXTERNAL_BODY_OCCUPANCY,
    RIGID_EXTERNAL_BODY_OCCUPANCY,
    IMMOVABLE_CONTACT_MASS,
    CONTACT_PRESSURE_TRANSFER,
    LINEAR_FIXED_SCALE,
    ANGULAR_FIXED_SCALE,
    CELL_SIZE,
    CELL_HALF,
    CELL_RADIUS,
    INCOMPRESSIBILITY_MIXING,
    RESERVATION_SCALE,
    RESERVATION_SCALE_U32
}
#import utility::material_identifier::{
    material_form_from_identifier,
    material_index_from_identifier
}
#import utility::fluid_spatial::{
    fluid_particle_world_cell,
    fluid_bucket_coordinates_from_position,
    fluid_bucket_index_from_coordinates,
    fluid_particle_belongs_to_cell
}
#import utility::tile_ring::{
    physical_cell_index_from_world_cell,
    world_cell_from_physical_tile_ring_index
}

// This is the fixed reaction metadata representation written by ReactionMaterialTable.
struct Reaction {
  words: array<u32, 27>,}

struct Candidate {
  reaction: u32,
  anchor: u32,
  extent: f32,
  partner: u32,
  material0: u32,
  material1: u32,
  padding0: u32,
  padding1: u32,
  fluid0_indices: vec4<u32>,
  fluid0_amounts: vec4<f32>,
  fluid1_indices: vec4<u32>,
  fluid1_amounts: vec4<f32>,
  product_slots: vec2<u32>,
  rigid_claims: vec2<u32>,
  rigid_padding: vec2<u32>,}

struct Request {
  cell: u32,
  kind: u32,
  locator: u32,
  expected_source: u32,
  replacement: u32,
  amount: u32,
  temperature: u32,
  world_x: u32,
  world_y: u32,}

struct Parameters {
  cell_count: u32,
  gas_count: u32,
  reaction_count: u32,
  padding: u32,}

struct RigidCell {
  local: vec2<i32>,
  body: u32,
  material_identifier: u32,
  appearance: u32,
  state_slot: u32,
  state_generation: u32,
  padding: u32}

@group(0) @binding(0) var<storage, read> reactions: array<Reaction>;
@group(0) @binding(1) var<storage, read> selector_members: array<u32>;
@group(0) @binding(2) var<storage, read> material_identifiers: array<u32>;
@group(0) @binding(3) var<storage, read_write> amounts: array<f32>;
@group(0) @binding(4) var<storage, read> temperatures: array<f32>;
@group(0) @binding(5) var<storage, read> retained_pressure: array<vec4<f32>>;
@group(0) @binding(6) var<storage, read> fluid_coverage: array<f32>;
@group(0) @binding(7) var<storage, read_write> gas_concentrations: array<f32>;
@group(0) @binding(8) var<storage, read> external_occupancy: array<u32>;
@group(0) @binding(9) var<storage, read> rigid_claims: array<atomic<u32>>;
@group(0) @binding(10) var<storage, read_write> candidates: array<Candidate>;
@group(0) @binding(11) var<uniform> parameters: Parameters;
@group(0) @binding(12) var<storage, read_write> mutation_requests: array<
  Request
>;
@group(0) @binding(13) var<storage, read_write> mutation_request_count: array<
  atomic<u32>
>;
@group(0) @binding(14) var<storage, read_write> reaction_energy: array<f32>;
@group(0) @binding(15) var<storage, read_write> pending_pressure: array<
  vec4<f32>
>;

// Read-only authoritative fluid bridge; discovery will use the shared spatial
// utility and these chains in the fluid-reaction pass.
struct FluidParticleAuthority {
  material_identifier: u32,
  is_active: u32,
  position: vec2<f32>,
  velocity: vec2<f32>,
  prediction_collision_displacement: vec2<f32>,
  amount: f32,
  temperature: f32,}

@group(0) @binding(16) var<storage, read_write> fluid_particles: array<
  FluidParticleAuthority
>;
@group(0) @binding(17) var<storage, read> fluid_bucket_heads: array<
  atomic<u32>
>;
@group(0) @binding(18) var<storage, read> fluid_next_particle: array<u32>;

struct FluidSpatialParameters {
  buffered_origin: vec2<i32>,
  buffered_tile_size: vec2<u32>,
  active_origin: vec2<i32>,
  active_tile_size: vec2<u32>,
  ring_offset: vec2<u32>,
  bucket_dimensions: vec2<u32>,
  streaming_origin: vec2<i32>,
  streaming_tile_size: vec2<u32>,
  gravity: vec2<f32>,
  delta_time: f32,
  particle_capacity: u32,
  buffered_cell_count: u32,
  bucket_count: u32,
  support_radius_cells: f32,
  particle_radius_cells: f32,
  maximum_movement_cells: u32,
  padding_0: u32,
  sample_center: vec2<f32>,
  sample_shape_parameters: vec2<f32>,
  sample_shape_kind: u32,
  padding_1: u32,}

@group(0) @binding(19) var<uniform> fluid_spatial_parameters: FluidSpatialParameters;
@group(0) @binding(20) var<storage, read_write> fluid_free_indices: array<u32>;
@group(0) @binding(21) var<storage, read_write> fluid_free_count: array<
  atomic<u32>
>;
@group(0) @binding(22) var<storage, read_write> fluid_reservations: array<
  atomic<u32>
>;
@group(0) @binding(23) var<storage, read_write> gas_reservations: array<
  atomic<u32>
>;
@group(0) @binding(24) var<storage, read_write> gas_output_reservations: array<
  atomic<u32>
>;
@group(0) @binding(25) var<storage, read_write> canonical_reservations: array<
  atomic<u32>
>;
@group(0) @binding(26) var<storage, read_write> fluid_reservation_owners: array<
  atomic<u32>
>;
@group(0) @binding(27) var<storage, read> rigid_cells: array<RigidCell>;
@group(0) @binding(28) var<storage, read_write> rigid_amounts: array<f32>;
@group(0) @binding(29) var<storage, read_write> rigid_reservations: array<
  atomic<u32>
>;
@group(0) @binding(30) var<storage, read_write> rigid_removal_events: array<
  vec4<u32>
>;
@group(0) @binding(31) var<storage, read_write> rigid_removal_count: array<
  atomic<u32>
>;
@group(0) @binding(32) var<storage, read_write> candidate_indices: array<u32>;
@group(0) @binding(33) var<storage, read_write> candidate_count: array<
  atomic<u32>
>;

struct SortParameters {
  k: u32,
  j: u32,}

@group(0) @binding(34) var<uniform> sort_parameters: SortParameters;
@group(0) @binding(35) var<storage, read> sort_steps: array<vec2<u32>>;
@group(0) @binding(36) var<storage, read_write> sort_indirect: array<u32>;
@group(0) @binding(37) var<storage, read> gas_temperatures: array<f32>;
@group(0) @binding(38) var<storage, read> rigid_temperatures: array<f32>;

fn matches_selector(rule: Reaction, reactant: u32, material: u32) -> bool {
  let base = reactant * 4u;
  if (rule.words[base + 3u] == 0u) {
    return true;
  }
  let offset = rule.words[base];
  let count = rule.words[base + 1u];
  for (var i = 0u; i < count; i += 1u) {
    if
      (offset + i < arrayLength(
        &selector_members) && selector_members[offset + i] == material)
    {
      return true;
    }
  }
  return false;
}

struct Source {
  cell: u32,
  material: u32,
  amount: f32,
  found: bool,
  rigid_claim: u32,}

fn source_amount(source: Source) -> f32 {
  if
    (source.rigid_claim != 0xffffffffu && source.rigid_claim < arrayLength(
      &rigid_cells))
  {
    let rigid = rigid_cells[source.rigid_claim];
    if (rigid.state_slot < arrayLength(&rigid_amounts)) {
      return rigid_amounts[rigid.state_slot];
    }
  }
  if (source.cell < arrayLength(&amounts)) {
    return amounts[source.cell];
  }
  return 0.0;
}

fn rigid_source(cell: u32, reactant: u32, rule: Reaction) -> Source {
  if (cell >= arrayLength(&rigid_claims)) {
    return Source(cell, EMPTY_MATERIAL_IDENTIFIER, 0.0, false, 0xffffffffu);
  }
  let claim = atomicLoad(&rigid_claims[cell]);
  if (claim == 0xffffffffu || claim >= arrayLength(&rigid_cells)) {
    return Source(cell, EMPTY_MATERIAL_IDENTIFIER, 0.0, false, 0xffffffffu);
  }
  let rigid = rigid_cells[claim];
  if
    (rigid.state_slot >= arrayLength(&rigid_amounts) || !matches_selector(
      rule,
      reactant,
      rigid.material_identifier))
  {
    return Source(cell, EMPTY_MATERIAL_IDENTIFIER, 0.0, false, 0xffffffffu);
  }
  let amount = rigid_amounts[rigid.state_slot];
  return
    Source(cell, rigid.material_identifier, amount, amount > 0.000001, claim);
}

fn gas_source(cell: u32, reactant: u32, rule: Reaction) -> Source {
  for (var species = 0u; species < parameters.gas_count; species += 1u) {
    let material = species + 1u;
    let concentration =
      gas_concentrations[species * parameters.cell_count + cell];
    if
      (concentration > 0.000001 && matches_selector(rule, reactant, material))
    {
      return Source(cell, material, concentration, true, 0xffffffffu);
    }
  }
  return Source(cell, EMPTY_MATERIAL_IDENTIFIER, 0.0, false, 0xffffffffu);
}

fn fluid_temperature(cell: u32, reactant: u32, rule: Reaction) -> f32 {
  let world =
    world_cell_from_physical_tile_ring_index(
      cell,
      fluid_spatial_parameters.buffered_origin,
      fluid_spatial_parameters.buffered_tile_size,
      fluid_spatial_parameters.ring_offset);
  let center = (vec2<f32>(world) + vec2<f32>(0.5)) / CELLS_PER_TILE_FLOAT;
  let base =
    fluid_bucket_coordinates_from_position(
      center,
      fluid_spatial_parameters.buffered_origin,
      fluid_spatial_parameters.support_radius_cells,
      CELLS_PER_TILE_FLOAT);
  var chosen = 0xffffffffu;
  var temperature = temperatures[cell];
  for (var y: i32 = -1; y <= 1; y += 1) {
    for (var x: i32 = -1; x <= 1; x += 1) {
      let bucket =
        fluid_bucket_index_from_coordinates(
          base + vec2<i32>(x, y),
          fluid_spatial_parameters.bucket_dimensions,
          0xffffffffu);
      if (bucket == 0xffffffffu) {
        continue;
      }
      var p = atomicLoad(&fluid_bucket_heads[bucket]);
      for (
        var n: u32 = 0u;
        p != 0xffffffffu && n < fluid_spatial_parameters.particle_capacity;
        n += 1u
      ) {
        let particle = fluid_particles[p];
        if
          (particle.is_active != 0u
            && p < chosen
            && material_form_from_identifier(
              particle.material_identifier) == FLUID_MATERIAL_FORM
            && fluid_particle_belongs_to_cell(
              particle.position,
              world,
              CELLS_PER_TILE_FLOAT)
            && particle.amount > 0.000001
            && matches_selector(rule, reactant, particle.material_identifier))
        {
          chosen = p;
          temperature = particle.temperature;
        }
        p = fluid_next_particle[p];
      }
    }
  }
  return temperature;
}

fn authority_temperature(source: Source, rule: Reaction, reactant: u32) -> f32 {
  if
    (source.rigid_claim != 0xffffffffu && source.rigid_claim < arrayLength(
      &rigid_cells))
  {
    let slot = rigid_cells[source.rigid_claim].state_slot;
    if (slot < arrayLength(&rigid_temperatures)) {
      return rigid_temperatures[slot];
    }
  }
  let form = material_form_from_identifier(source.material);
  if
    (form == GAS_MATERIAL_FORM && source.cell < arrayLength(&gas_temperatures))
  {
    return gas_temperatures[source.cell];
  }
  if (form == FLUID_MATERIAL_FORM) {
    return fluid_temperature(source.cell, reactant, rule);
  }
  return temperatures[source.cell];
}

fn fluid_source(cell: u32, reactant: u32, rule: Reaction) -> Source {
  let world =
    world_cell_from_physical_tile_ring_index(
      cell,
      fluid_spatial_parameters.buffered_origin,
      fluid_spatial_parameters.buffered_tile_size,
      fluid_spatial_parameters.ring_offset);
  let center = (vec2<f32>(world) + vec2<f32>(0.5)) / CELLS_PER_TILE_FLOAT;
  let base =
    fluid_bucket_coordinates_from_position(
      center,
      fluid_spatial_parameters.buffered_origin,
      fluid_spatial_parameters.support_radius_cells,
      CELLS_PER_TILE_FLOAT);
  var total = 0.0;
  var chosen = EMPTY_MATERIAL_IDENTIFIER;
  var chosen_index = 0xffffffffu;
  for (var y: i32 = -1; y <= 1; y += 1) {
    for (var x: i32 = -1; x <= 1; x += 1) {
      let bucket =
        fluid_bucket_index_from_coordinates(
          base + vec2<i32>(x, y),
          fluid_spatial_parameters.bucket_dimensions,
          0xffffffffu);
      if (bucket == 0xffffffffu) {
        continue;
      }
      var p = atomicLoad(&fluid_bucket_heads[bucket]);
      for (
        var n: u32 = 0u;
        p != 0xffffffffu && n < fluid_spatial_parameters.particle_capacity;
        n += 1u
      ) {
        let particle = fluid_particles[p];
        if
          (particle.is_active != 0u
            && particle.material_identifier != EMPTY_MATERIAL_IDENTIFIER
            && material_form_from_identifier(
              particle.material_identifier) == FLUID_MATERIAL_FORM
            && fluid_particle_belongs_to_cell(
              particle.position,
              world,
              CELLS_PER_TILE_FLOAT)
            && particle.amount > 0.000001
            && matches_selector(rule, reactant, particle.material_identifier))
        {
          total += max(particle.amount, 0.0);
          if (p < chosen_index) {
            chosen_index = p;
            chosen = particle.material_identifier;
          }
        }
        p = fluid_next_particle[p];
      }
    }
  }
  return Source(cell, chosen, total, total > 0.000001, 0xffffffffu);
}

fn source_for(cell: u32, reactant: u32, rule: Reaction) -> Source {
  let rigid = rigid_source(cell, reactant, rule);
  if (rigid.found) {
    return rigid;
  }
  let material = material_identifiers[cell];
  if
    (material != EMPTY_MATERIAL_IDENTIFIER && matches_selector(
      rule,
      reactant,
      material))
  {
    return
      Source(
        cell,
        material,
        amounts[cell],
        amounts[cell] > 0.000001,
        0xffffffffu);
  }
  let gas = gas_source(cell, reactant, rule);
  if (gas.found) {
    return gas;
  }
  return fluid_source(cell, reactant, rule);
}

fn local_air(cell: u32) -> f32 {
  if (cell == INVALID_PHYSICAL_CELL_INDEX || cell >= parameters.cell_count) {
    return 1.0;
  }
  var gas = 0.0;
  for (var species = 0u; species < parameters.gas_count; species += 1u) {
    gas += max(gas_concentrations[species * parameters.cell_count + cell], 0.0);
  }
  let blocked =
    material_identifiers[cell] != EMPTY_MATERIAL_IDENTIFIER
      || external_occupancy[cell] != 0u
      || atomicLoad(&rigid_claims[cell]) != 0xffffffffu;
  return
    select(
      clamp(1.0 - clamp(fluid_coverage[cell], 0.0, 1.0) - gas, 0.0, 1.0),
      0.0,
      blocked);
}

fn neighbor(anchor: u32, direction: u32) -> u32 {
  if
    (anchor == INVALID_PHYSICAL_CELL_INDEX || anchor >= parameters.cell_count)
  {
    return INVALID_PHYSICAL_CELL_INDEX;
  }
  let world =
    world_cell_from_physical_tile_ring_index(
      anchor,
      fluid_spatial_parameters.buffered_origin,
      fluid_spatial_parameters.buffered_tile_size,
      fluid_spatial_parameters.ring_offset);
  var offset = vec2<i32>(0);
  if (direction == 1u) {
    offset = vec2<i32>(0, -1);
  } else if (direction == 2u) {
    offset = vec2<i32>(1, 0);
  } else if (direction == 3u) {
    offset = vec2<i32>(0, 1);
  } else if (direction == 4u) {
    offset = vec2<i32>(-1, 0);
  } else {
    return INVALID_PHYSICAL_CELL_INDEX;
  }
  return
    physical_cell_index_from_world_cell(
      world + offset,
      fluid_spatial_parameters.buffered_origin,
      fluid_spatial_parameters.buffered_tile_size,
      fluid_spatial_parameters.ring_offset);
}

fn find_partner(anchor: u32, rule: Reaction) -> Source {
  let same = source_for(anchor, 1u, rule);
  if (same.found) {
    return same;
  }
  for (var direction = 1u; direction <= 4u; direction += 1u) {
    let cell = neighbor(anchor, direction);
    if (cell == 0xffffffffu) {
      continue;
    }
    let source = source_for(cell, 1u, rule);
    if (source.found) {
      return source;
    }
  }
  return Source(0u, EMPTY_MATERIAL_IDENTIFIER, 0.0, false, 0xffffffffu);
}

fn gas_total(cell: u32) -> f32 {
  var total = 0.0;
  for (var species = 0u; species < parameters.gas_count; species += 1u) {
    total +=
      max(gas_concentrations[species * parameters.cell_count + cell], 0.0);
  }
  return total;
}

fn reserve_gas(cell: u32, material: u32, amount: f32) -> bool {
  let species = material_index_from_identifier(material);
  let index = species * parameters.cell_count + cell;
  if (index >= arrayLength(&gas_reservations)) {
    return false;
  }
  let required = u32(max(amount, 0.0) * RESERVATION_SCALE);
  let available = u32(max(gas_concentrations[index], 0.0) * RESERVATION_SCALE);
  var old = atomicLoad(&gas_reservations[index]);
  loop {
    if (old + required > available) {
      return false;
    }
    let result =
      atomicCompareExchangeWeak(&gas_reservations[index], old, old + required);
    if (result.exchanged) {
      return true;
    }
    old = result.old_value;
  }
  return false;
}

fn reserve_gas_output(cell: u32, material: u32, amount: f32) -> bool {
  let species = material_index_from_identifier(material);
  let index = species * parameters.cell_count + cell;
  if (index >= arrayLength(&gas_output_reservations)) {
    return false;
  }
  let required = u32(max(amount, 0.0) * RESERVATION_SCALE);
  let current = u32(max(gas_concentrations[index], 0.0) * RESERVATION_SCALE);
  let inputs = atomicLoad(&gas_reservations[index]);
  let old = atomicLoad(&gas_output_reservations[index]);
  let net_current =
    select(current, current - min(current, inputs), inputs > 0u);
  var output_reserved = old;
  loop {
    if (net_current + output_reserved + required > RESERVATION_SCALE_U32) {
      return false;
    }
    let result =
      atomicCompareExchangeWeak(
        &gas_output_reservations[index],
        output_reserved,
        output_reserved + required);
    if (result.exchanged) {
      return true;
    }
    output_reserved = result.old_value;
  }
  return false;
}

fn release_gas_reservation(cell: u32, material: u32, amount: f32) {
  let index =
    material_index_from_identifier(material) * parameters.cell_count + cell;
  if (index < arrayLength(&gas_reservations)) {
    atomicSub(
      &gas_reservations[index],
      u32(max(amount, 0.0) * RESERVATION_SCALE));
  }
}

fn release_gas_output_reservation(cell: u32, material: u32, amount: f32) {
  let index =
    material_index_from_identifier(material) * parameters.cell_count + cell;
  if (index < arrayLength(&gas_output_reservations)) {
    atomicSub(
      &gas_output_reservations[index],
      u32(max(amount, 0.0) * RESERVATION_SCALE));
  }
}

fn reserve_mutation_requests(count: u32) -> u32 {
  if (count == 0u) {
    return 0u;
  }
  var base = atomicLoad(&mutation_request_count[0]);
  loop {
    if (base + count > arrayLength(&mutation_requests)) {
      return 0xffffffffu;
    }
    let result =
      atomicCompareExchangeWeak(&mutation_request_count[0], base, base + count);
    if (result.exchanged) {
      return base;
    }
    base = result.old_value;
  }
  return 0xffffffffu;
}

fn reserve_canonical_authority(cell: u32) -> bool {
  if (cell >= arrayLength(&canonical_reservations)) {
    return false;
  }
  return
    atomicCompareExchangeWeak(&canonical_reservations[cell], 0u, 1u).exchanged;
}

fn release_canonical_authority(cell: u32) {
  if (cell < arrayLength(&canonical_reservations)) {
    atomicStore(&canonical_reservations[cell], 0u);
  }
}

fn candidate_better(a: Candidate, b: Candidate) -> bool {
  let ap = bitcast<i32>(reactions[a.reaction].words[25]);
  let bp = bitcast<i32>(reactions[b.reaction].words[25]);
  return
    ap > bp || (ap == bp && (reactions[a.reaction].words[26] < reactions
      [b.reaction]
      .words[26] || (reactions[a.reaction].words[26] == reactions
      [b.reaction]
      .words[26] && (a.anchor < b.anchor || (a.anchor == b.anchor && a.reaction < b.reaction)))));
}

// Reserves fluid inventory without touching authoritative particles. The
// Retains the complete contributor plan in per-particle reservation state.
fn reserve_fluid_plan(
  record: u32,
  cell: u32,
  reactant: u32,
  rule: Reaction,
  required: f32) -> bool {
  var remaining = required;
  let world =
    world_cell_from_physical_tile_ring_index(
      cell,
      fluid_spatial_parameters.buffered_origin,
      fluid_spatial_parameters.buffered_tile_size,
      fluid_spatial_parameters.ring_offset);
  let center = (vec2<f32>(world) + vec2<f32>(0.5)) / CELLS_PER_TILE_FLOAT;
  let base =
    fluid_bucket_coordinates_from_position(
      center,
      fluid_spatial_parameters.buffered_origin,
      fluid_spatial_parameters.support_radius_cells,
      CELLS_PER_TILE_FLOAT);
  for (
    var iteration: u32 = 0u;
    iteration < fluid_spatial_parameters.particle_capacity && remaining > 0.000001;
    iteration += 1u
  ) {
    var selected = 0xffffffffu;
    for (var y: i32 = -1; y <= 1; y += 1) {
      for (var x: i32 = -1; x <= 1; x += 1) {
        let bucket =
          fluid_bucket_index_from_coordinates(
            base + vec2<i32>(x, y),
            fluid_spatial_parameters.bucket_dimensions,
            0xffffffffu);
        if (bucket == 0xffffffffu) {
          continue;
        }
        var p = atomicLoad(&fluid_bucket_heads[bucket]);
        for (
          var n: u32 = 0u;
          p != 0xffffffffu && n < fluid_spatial_parameters.particle_capacity;
          n += 1u
        ) {
          let particle = fluid_particles[p];
          if (p >= selected && selected != 0xffffffffu) {
            p = fluid_next_particle[p];
            continue;
          }
          if
            (particle.is_active != 0u
              && material_form_from_identifier(
                particle.material_identifier) == FLUID_MATERIAL_FORM
              && fluid_particle_belongs_to_cell(
                particle.position,
                world,
                CELLS_PER_TILE_FLOAT)
              && particle.amount > 0.000001
              && matches_selector(rule, reactant, particle.material_identifier)
              && atomicLoad(&fluid_reservations[p]) < u32(
                max(particle.amount, 0.0) * RESERVATION_SCALE))
          {
            selected = p;
          }
          p = fluid_next_particle[p];
        }
      }
    }
    if (selected == 0xffffffffu) {
      rollback_fluid_plan(cell, reactant);
      return false;
    }
    let particle = fluid_particles[selected];
    let already =
      f32(atomicLoad(&fluid_reservations[selected])) / RESERVATION_SCALE;
    let available = max(particle.amount - already, 0.0);
    let take = min(remaining, available);
    let units = u32(max(take, 0.0) * RESERVATION_SCALE);
    let old = atomicLoad(&fluid_reservations[selected]);
    let result =
      atomicCompareExchangeWeak(
        &fluid_reservations[selected],
        old,
        old + units);
    if (result.exchanged) {
      atomicStore(&fluid_reservation_owners[selected], record);
      remaining -= take;
    }
  }
  if (remaining > 0.00001) {
    rollback_fluid_plan(cell, reactant);
    return false;
  }
  return true;
}

fn reserve_fluid_slot() -> u32 {
  var count = atomicLoad(&fluid_free_count[0]);
  loop {
    if (count == 0u) {
      return 0xffffffffu;
    }
    let result =
      atomicCompareExchangeWeak(&fluid_free_count[0], count, count - 1u);
    if (result.exchanged) {
      return fluid_free_indices[count - 1u];
    }
    count = result.old_value;
  }
  return 0xffffffffu;
}

fn fluid_source_cell(cell: u32, source_cell: u32, commit: bool) {
  let world =
    world_cell_from_physical_tile_ring_index(
      source_cell,
      fluid_spatial_parameters.buffered_origin,
      fluid_spatial_parameters.buffered_tile_size,
      fluid_spatial_parameters.ring_offset);
  let center = (vec2<f32>(world) + vec2<f32>(0.5)) / CELLS_PER_TILE_FLOAT;
  let base =
    fluid_bucket_coordinates_from_position(
      center,
      fluid_spatial_parameters.buffered_origin,
      fluid_spatial_parameters.support_radius_cells,
      CELLS_PER_TILE_FLOAT);
  for (var y: i32 = -1; y <= 1; y += 1) {
    for (var x: i32 = -1; x <= 1; x += 1) {
      let bucket =
        fluid_bucket_index_from_coordinates(
          base + vec2<i32>(x, y),
          fluid_spatial_parameters.bucket_dimensions,
          0xffffffffu);
      if (bucket == 0xffffffffu) {
        continue;
      }
      var index = atomicLoad(&fluid_bucket_heads[bucket]);
      for (
        var n = 0u;
        index != 0xffffffffu && n < fluid_spatial_parameters.particle_capacity;
        n += 1u
      ) {
        if (atomicLoad(&fluid_reservation_owners[index]) == cell) {
          if (commit) {
            let amount =
              f32(atomicLoad(&fluid_reservations[index])) / RESERVATION_SCALE;
            let next_amount = max(fluid_particles[index].amount - amount, 0.0);
            fluid_particles[index].amount = next_amount;
            atomicStore(&fluid_reservations[index], 0u);
            atomicStore(&fluid_reservation_owners[index], 0xffffffffu);
            if (next_amount <= 0.000001) {
              fluid_particles[index].material_identifier =
                EMPTY_MATERIAL_IDENTIFIER;
              fluid_particles[index].is_active = 0u;
              let free = atomicAdd(&fluid_free_count[0], 1u);
              if (free < arrayLength(&fluid_free_indices)) {
                fluid_free_indices[free] = index;
              }
            }
          } else {
            atomicStore(&fluid_reservations[index], 0u);
            atomicStore(&fluid_reservation_owners[index], 0xffffffffu);
          }
        }
        index = fluid_next_particle[index];
      }
    }
  }
}

fn apply_fluid_plan(cell: u32, reactant: u32) {
  let candidate = candidates[cell];
  fluid_source_cell(cell, candidate.anchor, true);
  if (candidate.partner != candidate.anchor) {
    fluid_source_cell(cell, candidate.partner, true);
  }
}

fn rollback_fluid_plan(cell: u32, reactant: u32) {
  let candidate = candidates[cell];
  let source_cell = select(candidate.anchor, candidate.partner, reactant == 1u);
  fluid_source_cell(cell, source_cell, false);
}

@compute @workgroup_size(64)
fn clear_transaction_state(@builtin(global_invocation_id) id: vec3<u32>) {
  let index = id.x;
  if (index < arrayLength(&fluid_reservations)) {
    atomicStore(&fluid_reservations[index], 0u);
  }
  if (index < arrayLength(&fluid_reservation_owners)) {
    atomicStore(&fluid_reservation_owners[index], 0xffffffffu);
  }
  if (index < arrayLength(&gas_reservations)) {
    atomicStore(&gas_reservations[index], 0u);
  }
  if (index < arrayLength(&gas_output_reservations)) {
    atomicStore(&gas_output_reservations[index], 0u);
  }
  if (index < arrayLength(&canonical_reservations)) {
    atomicStore(&canonical_reservations[index], 0u);
  }
  if (index < arrayLength(&rigid_reservations)) {
    atomicStore(&rigid_reservations[index], 0u);
  }
  if (index == 0u) {
    atomicStore(&rigid_removal_count[0], 0u);
  }
  if (index < arrayLength(&candidate_indices)) {
    candidate_indices[index] = 0xffffffffu;
  }
  if (index == 0u) {
    atomicStore(&candidate_count[0], 0u);
  }
}

@compute @workgroup_size(64)
fn compact_candidates(@builtin(global_invocation_id) id: vec3<u32>) {
  let cell = id.x;
  if (cell >= arrayLength(&candidates)) {
    return;
  }
  let candidate = candidates[cell];
  if (candidate.reaction != 0xffffffffu && candidate.extent > 0.000001) {
    let index = atomicAdd(&candidate_count[0], 1u);
    if (index < arrayLength(&candidate_indices)) {
      candidate_indices[index] = cell;
    }
  }
}

@compute @workgroup_size(1)
fn prepare_sort_dispatch(@builtin(global_invocation_id) id: vec3<u32>) {
  if (id.x == 0u) {
    sort_indirect[0] = (arrayLength(&candidate_indices) + 63u) / 64u;
    sort_indirect[1] = 1u;
    sort_indirect[2] = 1u;
  }
}

fn candidate_before(a: u32, b: u32) -> bool {
  if (a == 0xffffffffu) {
    return false;
  }
  if (b == 0xffffffffu) {
    return true;
  }
  return candidate_better(candidates[a], candidates[b]);
}

@compute @workgroup_size(64)
fn sort_candidates(@builtin(global_invocation_id) id: vec3<u32>) {
  let index = id.x;
  let count = arrayLength(&candidate_indices);
  let partner = index ^ sort_parameters.j;
  if (partner <= index || partner >= arrayLength(&candidate_indices)) {
    return;
  }
  let a = candidate_indices[index];
  let b = candidate_indices[partner];
  let ascending = (index & sort_parameters.k) == 0u;
  if
    ((ascending && candidate_before(b, a)) || (!ascending && candidate_before(
      a,
      b)))
  {
    candidate_indices[index] = b;
    candidate_indices[partner] = a;
  }
}

fn rigid_source_candidate(candidate: Candidate, reactant: u32) -> bool {
  return
    select(
      candidate.rigid_claims.x,
      candidate.rigid_claims.y,
      reactant == 1u) != 0xffffffffu;
}

fn reserve_rigid_authority(candidate: Candidate, reactant: u32) -> bool {
  let claim =
    select(candidate.rigid_claims.x, candidate.rigid_claims.y, reactant == 1u);
  if (claim == 0xffffffffu || claim >= arrayLength(&rigid_cells)) {
    return true;
  }
  let rigid = rigid_cells[claim];
  if
    (rigid.state_slot >= arrayLength(
      &rigid_reservations) || rigid.state_slot >= arrayLength(&rigid_amounts))
  {
    return false;
  }
  if (reactant == 1u && candidate.rigid_claims.x == claim) {
    return true;
  }
  return
    atomicCompareExchangeWeak(
      &rigid_reservations[rigid.state_slot],
      0u,
      1u).exchanged;
}

fn release_rigid_authority(candidate: Candidate, reactant: u32) {
  let claim =
    select(candidate.rigid_claims.x, candidate.rigid_claims.y, reactant == 1u);
  if (claim != 0xffffffffu && claim < arrayLength(&rigid_cells)) {
    atomicStore(&rigid_reservations[rigid_cells[claim].state_slot], 0u);
  }
}

fn reserve_candidate(cell: u32) {
  let candidate = candidates[cell];
  if (candidate.reaction == 0xffffffffu || candidate.extent <= 0.000001) {
    return;
  }
  candidates[cell].padding0 = 1u;
  // Reject product layouts that the commit pass cannot represent.  Keeping
  // this check in planning means apply never has to discover a late failure
  // (or return a slot it already reserved).
  let first_form = material_form_from_identifier(candidate.material0);
  let first_amount =
    source_amount(
      Source(cell, candidate.material0, 0.0, true, candidate.rigid_claims.x));
  let first_same_authority =
    candidate.partner == cell
      && candidate.material0 == candidate.material1
      && candidate.rigid_claims.x == 0xffffffffu
      && candidate.rigid_claims.y == 0xffffffffu;
  let first_demand =
    bitcast<f32>(reactions[candidate.reaction].words[2]) + select(
      0.0,
      bitcast<f32>(reactions[candidate.reaction].words[6]),
      first_same_authority);
  let first_remaining =
    select(
      0.0,
      max(first_amount - first_demand * candidate.extent, 0.0),
      candidate.material0 != EMPTY_MATERIAL_IDENTIFIER
        && first_form != GAS_MATERIAL_FORM
        && first_form != FLUID_MATERIAL_FORM);
  var cellular_products = 0u;
  for (var product = 0u; product < 2u; product += 1u) {
    let base = 8u + product * 4u;
    if (reactions[candidate.reaction].words[base + 2u] == 0u) {
      continue;
    }
    let form =
      material_form_from_identifier(reactions[candidate.reaction].words[base]);
    if
      (form == CELLULAR_STATIC_MATERIAL_FORM || form == CELLULAR_DYNAMIC_MATERIAL_FORM)
    {
      cellular_products += 1u;
      if
        (candidate.rigid_claims.x != 0xffffffffu
          || candidate.rigid_claims.y != 0xffffffffu
          || (first_form != CELLULAR_STATIC_MATERIAL_FORM && first_form != CELLULAR_DYNAMIC_MATERIAL_FORM)
          || first_remaining > 0.00001
          || bitcast<f32>(
            reactions[candidate.reaction].words[base + 1u]) * candidate.extent <= 0.000001)
      {
        candidates[cell].padding0 = 0u;
        return;
      }
    } else if (form != GAS_MATERIAL_FORM && form != FLUID_MATERIAL_FORM) {
      candidates[cell].padding0 = 0u;
      return;
    }
  }
  if (cellular_products > 1u) {
    candidates[cell].padding0 = 0u;
    return;
  }
  if
    (material_form_from_identifier(
      candidate.material0) == FLUID_MATERIAL_FORM && !reserve_fluid_plan(
      cell,
      cell,
      0u,
      reactions[candidate.reaction],
      bitcast<f32>(reactions[candidate.reaction].words[2]) * candidate.extent))
  {
    candidates[cell].padding0 = 0u;
    return;
  }
  if
    (material_form_from_identifier(
      candidate.material1) == FLUID_MATERIAL_FORM && !reserve_fluid_plan(
      cell,
      candidate.partner,
      1u,
      reactions[candidate.reaction],
      bitcast<f32>(reactions[candidate.reaction].words[6]) * candidate.extent))
  {
    if
      (material_form_from_identifier(
        candidate.material0) == FLUID_MATERIAL_FORM)
    {
      rollback_fluid_plan(cell, 0u);
    }
    candidates[cell].padding0 = 0u;
    return;
  }
  if
    (material_form_from_identifier(
      candidate.material0) == GAS_MATERIAL_FORM && !reserve_gas(
      cell,
      candidate.material0,
      bitcast<f32>(reactions[candidate.reaction].words[2]) * candidate.extent))
  {
    if
      (material_form_from_identifier(
        candidate.material0) == FLUID_MATERIAL_FORM)
    {
      rollback_fluid_plan(cell, 0u);
    }
    if
      (material_form_from_identifier(
        candidate.material1) == FLUID_MATERIAL_FORM)
    {
      rollback_fluid_plan(cell, 1u);
    }
    candidates[cell].padding0 = 0u;
    return;
  }
  if
    (material_form_from_identifier(
      candidate.material1) == GAS_MATERIAL_FORM && !reserve_gas(
      candidate.partner,
      candidate.material1,
      bitcast<f32>(reactions[candidate.reaction].words[6]) * candidate.extent))
  {
    if
      (material_form_from_identifier(candidate.material0) == GAS_MATERIAL_FORM)
    {
      release_gas_reservation(
        cell,
        candidate.material0,
        bitcast<f32>(reactions[candidate.reaction].words[2]) * candidate.extent);
    }
    if
      (material_form_from_identifier(
        candidate.material0) == FLUID_MATERIAL_FORM)
    {
      rollback_fluid_plan(cell, 0u);
    }
    if
      (material_form_from_identifier(
        candidate.material1) == FLUID_MATERIAL_FORM)
    {
      rollback_fluid_plan(cell, 1u);
    }
    candidates[cell].padding0 = 0u;
    return;
  }
  var output_slots = vec2<u32>(0xffffffffu);
  for (var product = 0u; product < 2u; product += 1u) {
    let base = 8u + product * 4u;
    if
      (reactions
        [candidate.reaction]
        .words[base + 2u] == 0u || material_form_from_identifier(
        reactions[candidate.reaction].words[base]) != FLUID_MATERIAL_FORM)
    {
      continue;
    }
    let slot = reserve_fluid_slot();
    if (slot == 0xffffffffu) {
      if
        (material_form_from_identifier(
          candidate.material0) == GAS_MATERIAL_FORM)
      {
        release_gas_reservation(
          cell,
          candidate.material0,
          bitcast<f32>(
            reactions[candidate.reaction].words[2]) * candidate.extent);
      }
      if
        (material_form_from_identifier(
          candidate.material1) == GAS_MATERIAL_FORM)
      {
        release_gas_reservation(
          candidate.partner,
          candidate.material1,
          bitcast<f32>(
            reactions[candidate.reaction].words[6]) * candidate.extent);
      }
      if
        (material_form_from_identifier(
          candidate.material0) == FLUID_MATERIAL_FORM)
      {
        rollback_fluid_plan(cell, 0u);
      }
      if
        (material_form_from_identifier(
          candidate.material1) == FLUID_MATERIAL_FORM)
      {
        rollback_fluid_plan(cell, 1u);
      }
      release_fluid_slot(output_slots.x);
      release_fluid_slot(output_slots.y);
      candidates[cell].padding0 = 0u;
      return;
    }
    if (product == 0u) {
      output_slots.x = slot;
    } else {
      output_slots.y = slot;
    }
  }
  candidates[cell].product_slots = output_slots;
  let gas_output_cell =
    select(
      cell,
      candidate.partner,
      first_remaining > 0.00001 && candidate.partner != 0xffffffffu);
  for (var product = 0u; product < 2u; product += 1u) {
    let base = 8u + product * 4u;
    if (reactions[candidate.reaction].words[base + 2u] == 0u) {
      continue;
    }
    let product_material = reactions[candidate.reaction].words[base];
    if
      (material_form_from_identifier(
        product_material) == GAS_MATERIAL_FORM && !reserve_gas_output(
        gas_output_cell,
        product_material,
        bitcast<f32>(
          reactions[candidate.reaction].words[base + 1u]) * candidate.extent))
    {
      for (var previous = 0u; previous < product; previous += 1u) {
        let previous_base = 8u + previous * 4u;
        if
          (reactions
            [candidate.reaction]
            .words[previous_base + 2u] != 0u && material_form_from_identifier(
            reactions[candidate.reaction].words[previous_base]) == GAS_MATERIAL_FORM)
        {
          release_gas_output_reservation(
            gas_output_cell,
            reactions[candidate.reaction].words[previous_base],
            bitcast<f32>(
              reactions[candidate.reaction].words[previous_base + 1u]) * candidate.extent);
        }
      }
      if
        (material_form_from_identifier(
          candidate.material0) == GAS_MATERIAL_FORM)
      {
        release_gas_reservation(
          cell,
          candidate.material0,
          bitcast<f32>(
            reactions[candidate.reaction].words[2]) * candidate.extent);
      }
      if
        (material_form_from_identifier(
          candidate.material1) == GAS_MATERIAL_FORM)
      {
        release_gas_reservation(
          candidate.partner,
          candidate.material1,
          bitcast<f32>(
            reactions[candidate.reaction].words[6]) * candidate.extent);
      }
      if
        (material_form_from_identifier(
          candidate.material0) == FLUID_MATERIAL_FORM)
      {
        rollback_fluid_plan(cell, 0u);
      }
      if
        (material_form_from_identifier(
          candidate.material1) == FLUID_MATERIAL_FORM)
      {
        rollback_fluid_plan(cell, 1u);
      }
      release_fluid_slot(output_slots.x);
      release_fluid_slot(output_slots.y);
      candidates[cell].padding0 = 0u;
      return;
    }
  }
  var request_count = 0u;
  let source0_needs_mutation =
    candidate.material0 != EMPTY_MATERIAL_IDENTIFIER
      && candidate.rigid_claims.x == 0xffffffffu
      && material_form_from_identifier(candidate.material0) != GAS_MATERIAL_FORM
      && material_form_from_identifier(
        candidate.material0) != FLUID_MATERIAL_FORM
      && (cellular_products != 0u || first_remaining <= 0.00001);
  if (source0_needs_mutation) {
    request_count += 1u;
  }
  let second_amount =
    source_amount(
      Source(
        candidate.partner,
        candidate.material1,
        0.0,
        true,
        candidate.rigid_claims.y));
  let second_remaining =
    max(
      second_amount - bitcast<f32>(
        reactions[candidate.reaction].words[6]) * candidate.extent,
      0.0);
  let source1_needs_mutation =
    candidate.material1 != EMPTY_MATERIAL_IDENTIFIER
      && candidate.rigid_claims.y == 0xffffffffu
      && material_form_from_identifier(candidate.material1) != GAS_MATERIAL_FORM
      && material_form_from_identifier(
        candidate.material1) != FLUID_MATERIAL_FORM
      && candidate.partner != cell
      && second_remaining <= 0.00001;
  if (source1_needs_mutation) {
    request_count += 1u;
  }
  let request_base = reserve_mutation_requests(request_count);
  if (request_base == 0xffffffffu) {
    if
      (material_form_from_identifier(candidate.material0) == GAS_MATERIAL_FORM)
    {
      release_gas_reservation(
        cell,
        candidate.material0,
        bitcast<f32>(reactions[candidate.reaction].words[2]) * candidate.extent);
    }
    if
      (material_form_from_identifier(candidate.material1) == GAS_MATERIAL_FORM)
    {
      release_gas_reservation(
        candidate.partner,
        candidate.material1,
        bitcast<f32>(reactions[candidate.reaction].words[6]) * candidate.extent);
    }
    for (var product = 0u; product < 2u; product += 1u) {
      let base = 8u + product * 4u;
      if
        (reactions
          [candidate.reaction]
          .words[base + 2u] != 0u && material_form_from_identifier(
          reactions[candidate.reaction].words[base]) == GAS_MATERIAL_FORM)
      {
        release_gas_output_reservation(
          gas_output_cell,
          reactions[candidate.reaction].words[base],
          bitcast<f32>(
            reactions[candidate.reaction].words[base + 1u]) * candidate.extent);
      }
    }
    if
      (material_form_from_identifier(
        candidate.material0) == FLUID_MATERIAL_FORM)
    {
      rollback_fluid_plan(cell, 0u);
    }
    if
      (material_form_from_identifier(
        candidate.material1) == FLUID_MATERIAL_FORM)
    {
      rollback_fluid_plan(cell, 1u);
    }
    release_fluid_slot(output_slots.x);
    release_fluid_slot(output_slots.y);
    candidates[cell].padding0 = 0u;
    return;
  }
  candidates[cell].padding1 = request_base;
}

fn canonical_source(material: u32) -> bool {
  return
    material != EMPTY_MATERIAL_IDENTIFIER
      && material_form_from_identifier(material) != GAS_MATERIAL_FORM
      && material_form_from_identifier(material) != FLUID_MATERIAL_FORM;
}

fn reserve_candidate_canonical(candidate: Candidate) -> bool {
  let first =
    canonical_source(
      candidate.material0) && candidate.rigid_claims.x == 0xffffffffu;
  let second =
    canonical_source(
      candidate.material1) && candidate.rigid_claims.y == 0xffffffffu;
  if
    (rigid_source_candidate(candidate, 0u) && !reserve_rigid_authority(
      candidate,
      0u))
  {
    return false;
  }
  if
    (rigid_source_candidate(candidate, 1u) && !reserve_rigid_authority(
      candidate,
      1u))
  {
    release_rigid_authority(candidate, 0u);
    return false;
  }
  if (first && !reserve_canonical_authority(candidate.anchor)) {
    if (rigid_source_candidate(candidate, 0u)) {
      release_rigid_authority(candidate, 0u);
    }
    if
      (rigid_source_candidate(
        candidate,
        1u) && candidate.rigid_claims.y != candidate.rigid_claims.x)
    {
      release_rigid_authority(candidate, 1u);
    }
    return false;
  }
  if
    (second
      && candidate.partner != candidate.anchor
      && !reserve_canonical_authority(candidate.partner))
  {
    if (first) {
      release_canonical_authority(candidate.anchor);
    }
    if (rigid_source_candidate(candidate, 0u)) {
      release_rigid_authority(candidate, 0u);
    }
    if
      (rigid_source_candidate(
        candidate,
        1u) && candidate.rigid_claims.y != candidate.rigid_claims.x)
    {
      release_rigid_authority(candidate, 1u);
    }
    return false;
  }
  return true;
}

fn release_candidate_canonical(candidate: Candidate) {
  if
    (canonical_source(
      candidate.material0) && candidate.rigid_claims.x == 0xffffffffu)
  {
    release_canonical_authority(candidate.anchor);
  }
  if
    (canonical_source(candidate.material1)
      && candidate.rigid_claims.y == 0xffffffffu
      && candidate.partner != candidate.anchor)
  {
    release_canonical_authority(candidate.partner);
  }
  if (rigid_source_candidate(candidate, 0u)) {
    release_rigid_authority(candidate, 0u);
  }
  if
    (rigid_source_candidate(
      candidate,
      1u) && candidate.rigid_claims.y != candidate.rigid_claims.x)
  {
    release_rigid_authority(candidate, 1u);
  }
}

fn consume_rigid(claim: u32, demand: f32) {
  if (claim == 0xffffffffu || claim >= arrayLength(&rigid_cells)) {
    return;
  }
  let rigid = rigid_cells[claim];
  if (rigid.state_slot >= arrayLength(&rigid_amounts)) {
    return;
  }
  let remaining = max(rigid_amounts[rigid.state_slot] - demand, 0.0);
  rigid_amounts[rigid.state_slot] = remaining;
  if (remaining <= 0.000001) {
    let event = atomicAdd(&rigid_removal_count[0], 1u);
    if (event * 2u + 1u < arrayLength(&rigid_removal_events)) {
      rigid_removal_events[event * 2u] =
        vec4<u32>(
          rigid.state_slot,
          rigid.state_generation,
          rigid.material_identifier,
          rigid.body);
      rigid_removal_events[event * 2u + 1u] =
        vec4<u32>(
          bitcast<u32>(rigid.local.x),
          bitcast<u32>(rigid.local.y),
          0u,
          0u);
    }
  }
}

// Deterministic global arbitration walks the compact, Accelerator-sorted candidate list.
@compute @workgroup_size(1)
fn reserve_fluid_authority(@builtin(global_invocation_id) id: vec3<u32>) {
  if (id.x != 0u) {
    return;
  }
  let count =
    min(atomicLoad(&candidate_count[0]), arrayLength(&candidate_indices));
  for (var processed = 0u; processed < count; processed += 1u) {
    let best = candidate_indices[processed];
    if (best == 0xffffffffu || best >= arrayLength(&candidates)) {
      continue;
    }
    let candidate = candidates[best];
    candidates[best].padding0 = 2u;
    if (reserve_candidate_canonical(candidate)) {
      reserve_candidate(best);
      if (candidates[best].padding0 == 0u) {
        release_candidate_canonical(candidate);
      }
    }
  }
}

fn release_fluid_slot(slot: u32) {
  if (slot == 0xffffffffu) {
    return;
  }
  let count = atomicAdd(&fluid_free_count[0], 1u);
  if (count < arrayLength(&fluid_free_indices)) {
    fluid_free_indices[count] = slot;
  }
}

fn spawn_fluid_product(
  slot: u32,
  material: u32,
  amount: f32,
  cell: u32,
  temperature: f32) {
  let world =
    world_cell_from_physical_tile_ring_index(
      cell,
      fluid_spatial_parameters.buffered_origin,
      fluid_spatial_parameters.buffered_tile_size,
      fluid_spatial_parameters.ring_offset);
  fluid_particles[slot] =
    FluidParticleAuthority(
      material,
      1u,
      (vec2<f32>(world) + vec2<f32>(0.5)) / CELLS_PER_TILE_FLOAT,
      vec2<f32>(0.0),
      vec2<f32>(0.0),
      amount,
      temperature);
}

// Applies only representations which can be reserved deterministically in the
// current pass: source canonical inventory plus zero/two gas outputs, or one
// cellular output in a source cell completely vacated by the reaction. Other
// product shapes are rejected before any mutation; later form-specific passes
// add fluid and rigid transactions using the same candidate record.
@compute @workgroup_size(64)
fn apply_canonical(@builtin(global_invocation_id) id: vec3<u32>) {
  let cell = id.x;
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
fn discover_canonical(@builtin(global_invocation_id) id: vec3<u32>) {
  let cell = id.x;
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
