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

#import compute::material_reactions_resources::{
    Reaction,
    Candidate,
    Request,
    Parameters,
    RigidCell,
    reactions,
    selector_members,
    material_identifiers,
    amounts,
    temperatures,
    retained_pressure,
    fluid_coverage,
    gas_concentrations,
    external_occupancy,
    rigid_claims,
    candidates,
    parameters,
    mutation_requests,
    mutation_request_count,
    reaction_energy,
    pending_pressure,
    FluidParticleAuthority,
    fluid_particles,
    fluid_bucket_heads,
    fluid_next_particle,
    FluidSpatialParameters,
    fluid_spatial_parameters,
    fluid_free_indices,
    fluid_free_count,
    fluid_reservations,
    gas_reservations,
    gas_output_reservations,
    canonical_reservations,
    fluid_reservation_owners,
    rigid_cells,
    rigid_amounts,
    rigid_reservations,
    rigid_removal_events,
    rigid_removal_count,
    candidate_indices,
    candidate_count,
    SortParameters,
    sort_parameters,
    sort_steps,
    sort_indirect,
    gas_temperatures,
    rigid_temperatures
}

fn matches_selector(rule: Reaction, reactant: u32, material: u32) -> bool {
    let base = reactant * 4u;
    if (rule.words[base + 3u] == 0u ){
        return true;
    }
    let offset = rule.words[base];
    let count = rule.words[base + 1u];
    for (var i = 0u; i < count; i   += 1u) {
        if (offset + i < arrayLength(
        &selector_members) && selector_members[offset + i] == material ){
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
    rigid_claim: u32,
}

fn source_amount(source: Source) -> f32 {
    if (source.rigid_claim != 0xffffffffu && source.rigid_claim < arrayLength(
      &rigid_cells) ){
        let rigid = rigid_cells[source.rigid_claim];
        if (rigid.state_slot < arrayLength(&rigid_amounts) ){
            return rigid_amounts[rigid.state_slot];
        }
    }
    if (source.cell < arrayLength(&amounts) ){
        return amounts[source.cell];
    }
    return 0.0;
}

fn rigid_source(cell: u32, reactant: u32, rule: Reaction) -> Source {
    if (cell >= arrayLength(&rigid_claims) ){
        return Source(cell, EMPTY_MATERIAL_IDENTIFIER, 0.0, false, 0xffffffffu);
    }
    let claim = atomicLoad(&rigid_claims[cell]);
    if (claim == 0xffffffffu || claim >= arrayLength(&rigid_cells) ){
        return Source(cell, EMPTY_MATERIAL_IDENTIFIER, 0.0, false, 0xffffffffu);
    }
    let rigid = rigid_cells[claim];
    if (rigid.state_slot >= arrayLength(&rigid_amounts) || !matches_selector(
      rule,
      reactant,
      rigid.material_identifier) ){
        return Source(cell, EMPTY_MATERIAL_IDENTIFIER, 0.0, false, 0xffffffffu);
    }
    let amount = rigid_amounts[rigid.state_slot];
    return
    Source(cell, rigid.material_identifier, amount, amount > 0.000001, claim);
}

fn gas_source(cell: u32, reactant: u32, rule: Reaction) -> Source {
    for (var species = 0u; species < parameters.gas_count; species   += 1u) {
        let material = species + 1u;
        let concentration = gas_concentrations[species * parameters.cell_count + cell];
        if (concentration > 0.000001 && matches_selector(rule, reactant, material) ){
            return Source(cell, material, concentration, true, 0xffffffffu);
        }
    }
    return Source(cell, EMPTY_MATERIAL_IDENTIFIER, 0.0, false, 0xffffffffu);
}

fn fluid_temperature(cell: u32, reactant: u32, rule: Reaction) -> f32 {
    let world = world_cell_from_physical_tile_ring_index(
      cell,
      fluid_spatial_parameters.buffered_origin,
      fluid_spatial_parameters.buffered_tile_size,
      fluid_spatial_parameters.ring_offset);
    let center = (vec2<f32>(world) + vec2<f32>(0.5)) / CELLS_PER_TILE_FLOAT;
    let base = fluid_bucket_coordinates_from_position(
      center,
      fluid_spatial_parameters.buffered_origin,
      fluid_spatial_parameters.support_radius_cells,
      CELLS_PER_TILE_FLOAT);
    var chosen = 0xffffffffu;
    var temperature = temperatures[cell];
    for (var y: i32 = -1; y <= 1; y   += 1) {
        for (var x: i32 = -1; x <= 1; x   += 1) {
            let bucket = fluid_bucket_index_from_coordinates(
          base + vec2<i32>(x, y),
          fluid_spatial_parameters.bucket_dimensions,
          0xffffffffu);
            if (bucket == 0xffffffffu ){
                continue;
            }
            var p = atomicLoad(&fluid_bucket_heads[bucket]);
            for (var n: u32 = 0u; p != 0xffffffffu && n < fluid_spatial_parameters.particle_capacity; n   += 1u
      ) {
                let particle = fluid_particles[p];
                if (particle.is_active != 0u && p < chosen && material_form_from_identifier(
              particle.material_identifier) == FLUID_MATERIAL_FORM && fluid_particle_belongs_to_cell(
              particle.position,
              world,
              CELLS_PER_TILE_FLOAT) && particle.amount > 0.000001 && matches_selector(rule, reactant, particle.material_identifier) ){
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
    if (source.rigid_claim != 0xffffffffu && source.rigid_claim < arrayLength(
      &rigid_cells) ){
        let slot = rigid_cells[source.rigid_claim].state_slot;
        if (slot < arrayLength(&rigid_temperatures) ){
            return rigid_temperatures[slot];
        }
    }
    let form = material_form_from_identifier(source.material);
    if (form == GAS_MATERIAL_FORM && source.cell < arrayLength(&gas_temperatures) ){
        return gas_temperatures[source.cell];
    }
    if (form == FLUID_MATERIAL_FORM ){
        return fluid_temperature(source.cell, reactant, rule);
    }
    return temperatures[source.cell];
}

fn fluid_source(cell: u32, reactant: u32, rule: Reaction) -> Source {
    let world = world_cell_from_physical_tile_ring_index(
      cell,
      fluid_spatial_parameters.buffered_origin,
      fluid_spatial_parameters.buffered_tile_size,
      fluid_spatial_parameters.ring_offset);
    let center = (vec2<f32>(world) + vec2<f32>(0.5)) / CELLS_PER_TILE_FLOAT;
    let base = fluid_bucket_coordinates_from_position(
      center,
      fluid_spatial_parameters.buffered_origin,
      fluid_spatial_parameters.support_radius_cells,
      CELLS_PER_TILE_FLOAT);
    var total = 0.0;
    var chosen = EMPTY_MATERIAL_IDENTIFIER;
    var chosen_index = 0xffffffffu;
    for (var y: i32 = -1; y <= 1; y   += 1) {
        for (var x: i32 = -1; x <= 1; x   += 1) {
            let bucket = fluid_bucket_index_from_coordinates(
          base + vec2<i32>(x, y),
          fluid_spatial_parameters.bucket_dimensions,
          0xffffffffu);
            if (bucket == 0xffffffffu ){
                continue;
            }
            var p = atomicLoad(&fluid_bucket_heads[bucket]);
            for (var n: u32 = 0u; p != 0xffffffffu && n < fluid_spatial_parameters.particle_capacity; n   += 1u
      ) {
                let particle = fluid_particles[p];
                if (particle.is_active != 0u && particle.material_identifier != EMPTY_MATERIAL_IDENTIFIER && material_form_from_identifier(
              particle.material_identifier) == FLUID_MATERIAL_FORM && fluid_particle_belongs_to_cell(
              particle.position,
              world,
              CELLS_PER_TILE_FLOAT) && particle.amount > 0.000001 && matches_selector(rule, reactant, particle.material_identifier) ){
                    total   += max(particle.amount, 0.0);
                    if (p < chosen_index ){
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
    if (rigid.found ){
        return rigid;
    }
    let material = material_identifiers[cell];
    if (material != EMPTY_MATERIAL_IDENTIFIER && matches_selector(
      rule,
      reactant,
      material) ){
        return
      Source(
        cell,
        material,
        amounts[cell],
        amounts[cell] > 0.000001,
        0xffffffffu);
    }
    let gas = gas_source(cell, reactant, rule);
    if (gas.found ){
        return gas;
    }
    return fluid_source(cell, reactant, rule);
}

fn local_air(cell: u32) -> f32 {
    if (cell == INVALID_PHYSICAL_CELL_INDEX || cell >= parameters.cell_count ){
        return 1.0;
    }
    var gas = 0.0;
    for (var species = 0u; species < parameters.gas_count; species   += 1u) {
        gas   += max(gas_concentrations[species * parameters.cell_count + cell], 0.0);
    }
    let blocked = material_identifiers[cell] != EMPTY_MATERIAL_IDENTIFIER || external_occupancy[cell] != 0u || atomicLoad(&rigid_claims[cell]) != 0xffffffffu;
    return
    select(
      clamp(1.0 - clamp(fluid_coverage[cell], 0.0, 1.0) - gas, 0.0, 1.0),
      0.0,
      blocked);
}

fn neighbor(anchor: u32, direction: u32) -> u32 {
    if (anchor == INVALID_PHYSICAL_CELL_INDEX || anchor >= parameters.cell_count ){
        return INVALID_PHYSICAL_CELL_INDEX;
    }
    let world = world_cell_from_physical_tile_ring_index(
      anchor,
      fluid_spatial_parameters.buffered_origin,
      fluid_spatial_parameters.buffered_tile_size,
      fluid_spatial_parameters.ring_offset);
    var offset = vec2<i32>(0);
    if (direction == 1u ){
        offset = vec2<i32>(0, -1);
    } else if (direction == 2u ){
        offset = vec2<i32>(1, 0);
    } else if (direction == 3u ){
        offset = vec2<i32>(0, 1);
    } else if (direction == 4u ){
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
    if (same.found ){
        return same;
    }
    for (var direction = 1u; direction <= 4u; direction   += 1u) {
        let cell = neighbor(anchor, direction);
        if (cell == 0xffffffffu ){
            continue;
        }
        let source = source_for(cell, 1u, rule);
        if (source.found ){
            return source;
        }
    }
    return Source(0u, EMPTY_MATERIAL_IDENTIFIER, 0.0, false, 0xffffffffu);
}

fn gas_total(cell: u32) -> f32 {
    var total = 0.0;
    for (var species = 0u; species < parameters.gas_count; species   += 1u) {
        total   +=
      max(gas_concentrations[species * parameters.cell_count + cell], 0.0);
    }
    return total;
}

fn reserve_gas(cell: u32, material: u32, amount: f32) -> bool {
    let species = material_index_from_identifier(material);
    let index = species * parameters.cell_count + cell;
    if (index >= arrayLength(&gas_reservations) ){
        return false;
    }
    let required = u32(max(amount, 0.0) * RESERVATION_SCALE);
    let available = u32(max(gas_concentrations[index], 0.0) * RESERVATION_SCALE);
    var old = atomicLoad(&gas_reservations[index]);
        loop {
            if (old + required > available ){
                return false;
            }
            let result = atomicCompareExchangeWeak(&gas_reservations[index], old, old + required);
            if (result.exchanged ){
                return true;
            }
            old = result.old_value;
        }
    return false;
}

fn reserve_gas_output(cell: u32, material: u32, amount: f32) -> bool {
    let species = material_index_from_identifier(material);
    let index = species * parameters.cell_count + cell;
    if (index >= arrayLength(&gas_output_reservations) ){
        return false;
    }
    let required = u32(max(amount, 0.0) * RESERVATION_SCALE);
    let current = u32(max(gas_concentrations[index], 0.0) * RESERVATION_SCALE);
    let inputs = atomicLoad(&gas_reservations[index]);
    let old = atomicLoad(&gas_output_reservations[index]);
    let net_current = select(current, current - min(current, inputs), inputs > 0u);
    var output_reserved = old;
        loop {
            if (net_current + output_reserved + required > RESERVATION_SCALE_U32 ){
                return false;
            }
            let result = atomicCompareExchangeWeak(
        &gas_output_reservations[index],
        output_reserved,
        output_reserved + required);
            if (result.exchanged ){
                return true;
            }
            output_reserved = result.old_value;
        }
    return false;
}

fn release_gas_reservation(cell: u32, material: u32, amount: f32) {
    let index = material_index_from_identifier(material) * parameters.cell_count + cell;
    if (index < arrayLength(&gas_reservations) ){
        atomicSub(
      &gas_reservations[index],
      u32(max(amount, 0.0) * RESERVATION_SCALE));
    }
}

fn release_gas_output_reservation(cell: u32, material: u32, amount: f32) {
    let index = material_index_from_identifier(material) * parameters.cell_count + cell;
    if (index < arrayLength(&gas_output_reservations) ){
        atomicSub(
      &gas_output_reservations[index],
      u32(max(amount, 0.0) * RESERVATION_SCALE));
    }
}

fn reserve_mutation_requests(count: u32) -> u32 {
    if (count == 0u ){
        return 0u;
    }
    var base = atomicLoad(&mutation_request_count[0]);
        loop {
            if (base + count > arrayLength(&mutation_requests) ){
                return 0xffffffffu;
            }
            let result = atomicCompareExchangeWeak(&mutation_request_count[0], base, base + count);
            if (result.exchanged ){
                return base;
            }
            base = result.old_value;
        }
    return 0xffffffffu;
}

fn reserve_canonical_authority(cell: u32) -> bool {
    if (cell >= arrayLength(&canonical_reservations) ){
        return false;
    }
    return
    atomicCompareExchangeWeak(&canonical_reservations[cell], 0u, 1u).exchanged;
}

fn release_canonical_authority(cell: u32) {
    if (cell < arrayLength(&canonical_reservations) ){
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

