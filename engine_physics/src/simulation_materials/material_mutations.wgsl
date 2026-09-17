// Copyright Rob Gage 2026
#define_import_path compute::material_mutations
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
struct Request {
    cell: u32,
    kind: u32,
    locator: u32,
    expected_source: u32,
    replacement: u32,
    amount: u32,
    temperature: u32,
    world_x: u32,
    world_y: u32
}

struct Parameters {
    buffered_cell_count: u32,
    gas_count: u32,
    padding: vec2<u32>
}

@group(0) @binding(0) var<storage, read> requests: array<Request>;
@group(0) @binding(1) var<storage, read> request_count: array<atomic<u32>>;
@group(0) @binding(2) var<storage, read_write> cellular_material_identifiers: array<u32
>;
@group(0) @binding(3) var<storage, read_write> cellular_appearances: array<u32>;
@group(0) @binding(4) var<storage, read> static_defaults: array<u32>;
@group(0) @binding(5) var<storage, read_write> cellular_integrities: array<f32>;
@group(0) @binding(6) var<storage, read_write> cellular_kinematics: array<vec4<f32>
>;
@group(0) @binding(7) var<storage, read_write> fluid_edits: array<u32>;
@group(0) @binding(8) var<storage, read_write> gas_concentrations: array<f32>;
@group(0) @binding(9) var<uniform> parameters: Parameters;
@group(0) @binding(10) var<storage, read_write> fluid_edits_pending: array<atomic<u32>
>;
@group(0) @binding(11) var<storage, read_write> cellular_amounts: array<f32>;
@group(0) @binding(12) var<storage, read_write> cellular_temperatures: array<f32
>;
@group(0) @binding(13) var<storage, read_write> fluid_edit_amounts: array<f32>;
@group(0) @binding(14) var<storage, read_write> fluid_edit_temperatures: array<f32
>;
@group(0) @binding(15) var<storage, read_write> gas_temperatures: array<f32>;

struct Particle {
    material_identifier: u32,
    is_active: u32,
    position: vec2<f32>,
    velocity: vec2<f32>,
    prediction_collision_displacement: vec2<f32>,
    amount: f32,
    temperature: f32
}

@group(0) @binding(16) var<storage, read_write> particles: array<Particle>;
@group(0) @binding(17) var<storage, read_write> fluid_free_indices: array<u32>;
@group(0) @binding(18) var<storage, read_write> fluid_free_count: array<atomic<u32>
>;

// Gas is continuous inventory; fluid particles are discrete unit inventory.
// Continuous→discrete conversion must aggregate a full particle quantum before spawning.
struct GasFluidCandidate {
    replacement: u32,
    amount: f32,
    temperature: f32,
    position: vec2<f32>
}

@group(0) @binding(19) var<storage, read_write> gas_fluid_candidates: array<GasFluidCandidate
>;
@group(1) @binding(0) var<storage, read_write> indirect_dispatch: array<u32>;

@compute @workgroup_size(1)
fn prepare_material_mutation_dispatch(
    @builtin(global_invocation_id) invocation: vec3<u32>
) {
    if (invocation.x != 0u ){
        return;
    }
    let count: u32 = min(atomicLoad(&request_count[0]), arrayLength(&requests));
    indirect_dispatch[0] = (count + 63u) / 64u;
    indirect_dispatch[1] = 1u;
    indirect_dispatch[2] = 1u;
}

@compute @workgroup_size(64)
fn resolve_material_mutations_nonallocating(
    @builtin(global_invocation_id) invocation: vec3<u32>
) {
    let i = invocation.x;
    if (i >= atomicLoad(&request_count[0]) || i >= arrayLength(&requests) ){
        return;
    }
    let request = requests[i];
    if (request.cell >= parameters.buffered_cell_count ){
        return;
    }
    if (request.kind == 1u ){
        resolve_particle(request);
        return;
    }
    if (request.kind == 2u ){
        resolve_gas_nonallocating(request);
        return;
    }
    if (request.kind != 0u || cellular_material_identifiers[request.cell] != request.expected_source ){
        return;
    }
    let form = material_form_from_identifier(request.replacement);
    let source_amount = cellular_amounts[request.cell];
    let source_temperature = cellular_temperatures[request.cell];
    let result_amount = select(source_amount, bitcast<f32>(request.amount), request.amount != 0u);
    let result_temperature = select(
      source_temperature,
      bitcast<f32>(request.temperature),
      request.temperature != 0u);
    cellular_kinematics[request.cell] = vec4<f32>(0.0);
    if (request.replacement == EMPTY_MATERIAL_IDENTIFIER ){
        cellular_material_identifiers[request.cell] = 0u;
        cellular_appearances[request.cell] = 0u;
        cellular_integrities[request.cell] = 0.0;
        cellular_amounts[request.cell] = 0.0;
        cellular_temperatures[request.cell] = 0.0;
        fluid_edits[request.cell] = FLUID_EDIT_ERASE;
        atomicStore(&fluid_edits_pending[0], 1u);
        return;
    }
    if (form == CELLULAR_STATIC_MATERIAL_FORM ){
        let index = material_index_from_identifier(request.replacement);
        if (index >= arrayLength(&static_defaults) ){
            return;
        }
        cellular_material_identifiers[request.cell] = request.replacement;
        cellular_amounts[request.cell] = result_amount;
        cellular_temperatures[request.cell] = result_temperature;
        cellular_integrities[request.cell] = bitcast<f32>(static_defaults[index]);
        fluid_edits[request.cell] = FLUID_EDIT_ERASE;
        atomicStore(&fluid_edits_pending[0], 1u);
        return;
    }
    if (form == CELLULAR_DYNAMIC_MATERIAL_FORM ){
        cellular_material_identifiers[request.cell] = request.replacement;
        cellular_amounts[request.cell] = result_amount;
        cellular_temperatures[request.cell] = result_temperature;
        cellular_integrities[request.cell] = 0.0;
        fluid_edits[request.cell] = FLUID_EDIT_ERASE;
        atomicStore(&fluid_edits_pending[0], 1u);
        return;
    }
    cellular_material_identifiers[request.cell] = 0u;
    cellular_appearances[request.cell] = 0u;
    cellular_integrities[request.cell] = 0.0;
    if (form == FLUID_MATERIAL_FORM ){
        fluid_edits[request.cell] = request.replacement;
        fluid_edit_amounts[request.cell] = result_amount;
        fluid_edit_temperatures[request.cell] = result_temperature;
        cellular_amounts[request.cell] = 0.0;
        cellular_temperatures[request.cell] = 0.0;
        atomicStore(&fluid_edits_pending[0], 1u);
        return;
    }
    let species = material_index_from_identifier(request.replacement);
    if (species >= parameters.gas_count ){
        return;
    }
    gas_temperatures[request.cell] = result_temperature;
    gas_concentrations[species * parameters.buffered_cell_count + request.cell]   +=
    result_amount;
    cellular_amounts[request.cell] = 0.0;
    cellular_temperatures[request.cell] = 0.0;
}

// This resolver only releases slots; condensation claims them in the later dispatch pass.
fn release_particle(index: u32) {
    if (index >= arrayLength(&particles) ){
        return;
    }
    particles[index].material_identifier = EMPTY_MATERIAL_IDENTIFIER;
    particles[index].is_active = 0u;
    var n = atomicLoad(&fluid_free_count[0]);
        loop {
            if (n >= arrayLength(&fluid_free_indices) ){
                return;
            }
            let r = atomicCompareExchangeWeak(&fluid_free_count[0], n, n + 1u);
            if (r.exchanged ){
                fluid_free_indices[n] = index;
                return;
            }
            n = r.old_value;
        }
}

fn claim_particle() -> u32 {
    var n = atomicLoad(&fluid_free_count[0]);
        loop {
            if (n == 0u || n > arrayLength(&fluid_free_indices) ){
                return 0xffffffffu;
            }
            let r = atomicCompareExchangeWeak(&fluid_free_count[0], n, n - 1u);
            if (r.exchanged ){
                let index = fluid_free_indices[n - 1u];
                if (index >= arrayLength(&particles) ){
                    return 0xffffffffu;
                }
                return index;
            }
            n = r.old_value;
        }
    return 0xffffffffu;
}

fn spawn_fluid(request: Request) -> bool {
    let index = claim_particle();
    if (index == 0xffffffffu ){
        return false;
    }
    particles[index] = Particle(
      request.replacement,
      1u,
      vec2<f32>(bitcast<f32>(request.world_x), bitcast<f32>(request.world_y)),
      vec2<f32>(0.0),
      vec2<f32>(0.0),
      bitcast<f32>(request.amount),
      bitcast<f32>(request.temperature));
    return true;
}

fn place_cell(request: Request) -> bool {
    if (cellular_material_identifiers[request.cell] != EMPTY_MATERIAL_IDENTIFIER ){
        return false;
    }
    let form = material_form_from_identifier(request.replacement);
    if (form != CELLULAR_STATIC_MATERIAL_FORM && form != CELLULAR_DYNAMIC_MATERIAL_FORM ){
        return false;
    }
    cellular_material_identifiers[request.cell] = request.replacement;
    cellular_amounts[request.cell] = bitcast<f32>(request.amount);
    cellular_temperatures[request.cell] = bitcast<f32>(request.temperature);
    cellular_kinematics[request.cell] = vec4<f32>(0.0);
    if (form == CELLULAR_STATIC_MATERIAL_FORM ){
        let n = material_index_from_identifier(request.replacement);
        if (n >= arrayLength(&static_defaults) ){
            cellular_material_identifiers[request.cell] = 0u;
            return false;
        }
        cellular_integrities[request.cell] = bitcast<f32>(static_defaults[n]);
    }
    return true;
}

fn resolve_particle(request: Request) {
    if (request.locator >= arrayLength(&particles) ){
        return;
    }
    let p = particles[request.locator];
    if (p.is_active == 0u || p.material_identifier != request.expected_source ){
        return;
    }
    let form = material_form_from_identifier(request.replacement);
    if (form == FLUID_MATERIAL_FORM ){
        particles[request.locator].material_identifier = request.replacement;
        particles[request.locator].temperature = bitcast<f32>(request.temperature);
        return;
    }
    if (form == 0u ){
        let s = material_index_from_identifier(request.replacement);
        if (s >= parameters.gas_count ){
            return;
        }
        gas_concentrations[s * parameters.buffered_cell_count + request.cell]   +=
      p.amount;
        gas_temperatures[request.cell] = bitcast<f32>(request.temperature);
        release_particle(request.locator);
        return;
    }
    if (place_cell(request) ){
        release_particle(request.locator);
    }
}

fn resolve_gas_nonallocating(request: Request) {
    let source = material_index_from_identifier(request.expected_source);
    if (source >= parameters.gas_count ){
        return;
    }
    let at = source * parameters.buffered_cell_count + request.cell;
    let requested = bitcast<f32>(request.amount);
    let current = gas_concentrations[at];
    if (!(requested > 0.000001) || current + 0.00001 < requested ){
        return;
    }
    let target_species = material_form_from_identifier(request.replacement);
    if (target_species == FLUID_MATERIAL_FORM ){
        return;
    }
    if (target_species == 0u ){
        let species = material_index_from_identifier(request.replacement);
        if (species >= parameters.gas_count ){
            return;
        }
        gas_concentrations[at] = max(current - requested, 0.0);
        gas_concentrations[species * parameters.buffered_cell_count + request.cell]   +=
      requested;
        gas_temperatures[request.cell] = bitcast<f32>(request.temperature);
        return;
    }
    if (place_cell(request) ){
        gas_concentrations[at] = max(current - requested, 0.0);
    }
}

var<workgroup> condensation_amount: array<f32, 64>;
var<workgroup> condensation_temperature: array<f32, 64>;
var<workgroup> condensation_position: array<vec2<f32>, 64>;
var<workgroup> condensation_index: u32;

@compute @workgroup_size(64)
fn resolve_gas_fluid_condensation(
    @builtin(local_invocation_id) local: vec3<u32>,
    @builtin(workgroup_id) group: vec3<u32>
) {
    let lane = local.x;
    let cell = group.x * 64u + lane;
    let candidate_index = (group.z * parameters.gas_count + group.y) * parameters.buffered_cell_count + cell;
    let candidate = gas_fluid_candidates[candidate_index];
    let valid = candidate.replacement != EMPTY_MATERIAL_IDENTIFIER && candidate.amount > 0.0;
    condensation_amount[lane] = select(0.0, candidate.amount, valid);
    condensation_temperature[lane] = select(0.0, candidate.amount * candidate.temperature, valid);
    condensation_position[lane] = select(vec2<f32>(0.0), candidate.amount * candidate.position, valid);
    workgroupBarrier();
    var stride = 32u;
        loop {
            if (lane < stride ){
                condensation_amount[lane]   += condensation_amount[lane + stride];
                condensation_temperature[lane]   += condensation_temperature[lane + stride];
                condensation_position[lane]   += condensation_position[lane + stride];
            }
            workgroupBarrier();
            if (stride == 1u ){
                break;
            }
            stride   /= 2u;
        }
    if (lane == 0u ){
        condensation_index = 0xffffffffu;
        if (condensation_amount[0] >= 1.0 ){
            var i = 0u;
                loop {
                    if (i >= 64u ){
                        break;
                    }
                    let c = gas_fluid_candidates[(group.z * parameters.gas_count + group.y) * parameters.buffered_cell_count + group.x * 64u + i];
                    if (c.replacement != EMPTY_MATERIAL_IDENTIFIER && c.amount > 0.0 ){
                        let index = claim_particle();
                        if (index != 0xffffffffu ){
                            particles[index] = Particle(
                c.replacement,
                1u,
                condensation_position[0] / condensation_amount[0],
                vec2<f32>(0.0),
                vec2<f32>(0.0),
                1.0,
                condensation_temperature[0] / condensation_amount[0]);
                            condensation_index = index;
                        }
                        break;
                    }
                    i   += 1u;
                }
        }
    }
    workgroupBarrier();
    if (condensation_index != 0xffffffffu && valid ){
        let at = group.y * parameters.buffered_cell_count + cell;
        gas_concentrations[at] = max(
        gas_concentrations[at] - candidate.amount / condensation_amount[0],
        0.0);
    }
}
