#define_import_path compute::thermal_scatter
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
#import utility::cell_coordinates::world_cell_from_logical_tile_major_index
#import utility::tile_ring::physical_cell_index_from_world_cell
#import utility::fluid_spatial::fluid_particle_world_cell

struct ThermalScatterParameters {
    origin: vec2<i32>,
    tiles: vec2<u32>,
    ring: vec2<u32>,
    cell_count: u32,
    particle_capacity: u32,
    gas_count: u32,
    rigid_capacity: u32,
    empty_space_heat_capacity: f32,
    padding: vec3<u32>
}

struct Particle {
    material_identifier: u32,
    is_active: u32,
    position: vec2<f32>,
    velocity: vec2<f32>,
    prediction_collision_displacement: vec2<f32>,
    amount: f32,
    temperature: f32,
}

struct RigidCell {
    local: vec2<i32>,
    body: u32,
    material_identifier: u32,
    appearance: u32,
    state_slot: u32,
    state_generation: u32,
    padding: u32,
}

@group(0) @binding(0) var<storage, read> solved: array<vec4<f32>>;
@group(0) @binding(1) var<storage, read> cellular_materials: array<u32>;
@group(0) @binding(2) var<storage, read> cellular_amounts: array<f32>;
@group(0) @binding(3) var<storage, read_write> cellular_temperatures: array<f32>;
@group(0) @binding(4) var<storage, read_write> particles: array<Particle>;
@group(0) @binding(5) var<storage, read> gas_concentrations: array<f32>;
@group(0) @binding(6) var<storage, read_write> gas_temperatures: array<f32>;
@group(0) @binding(7) var<storage, read> fluid_coverage: array<f32>;
@group(0) @binding(8) var<storage, read> rigid_claims: array<u32>;
@group(0) @binding(9) var<storage, read> rigid_cells: array<RigidCell>;
@group(0) @binding(10) var<storage, read> rigid_amounts: array<f32>;
@group(0) @binding(11) var<storage, read_write> rigid_temperatures: array<f32>;
@group(0) @binding(12) var<storage, read> claim_counts: array<u32>;
@group(0) @binding(13) var<storage, read_write> rigid_temperature_sum: array<atomic<u32>>;
@group(0) @binding(14) var<storage, read> occupancy: array<u32>;
@group(0) @binding(15) var<uniform> parameters: ThermalScatterParameters;

fn cell_index(world: vec2<i32>) -> u32 {
    return
        physical_cell_index_from_world_cell(
            world,
            parameters.origin,
            parameters.tiles,
            parameters.ring,
        );
}

fn physical_index_from_logical(logical: u32) -> u32 {
    let world =
        world_cell_from_logical_tile_major_index(logical, parameters.origin, parameters.tiles);
    return
        physical_cell_index_from_world_cell(
            world,
            parameters.origin,
            parameters.tiles,
            parameters.ring,
        );
}

fn atomic_add_f32(index: u32, v: f32) {
    var old = atomicLoad(&rigid_temperature_sum[index]);
    loop {
        let next = bitcast<u32>(bitcast<f32>(old) + v);
        let r = atomicCompareExchangeWeak(&rigid_temperature_sum[index], old, next);
        if (r.exchanged) {
            break;
        }
        old = r.old_value;
    }
}

@compute @workgroup_size(64)
fn scatter_cellular_gas(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let logical = invocation.x;
    if (logical >= parameters.cell_count) {
        return;
    }
    let i = physical_index_from_logical(logical);
    if (i == INVALID_PHYSICAL_CELL_INDEX || i >= parameters.cell_count) {
        return;
    }
    let s = solved[i].w;
    if (cellular_materials[i] != 0u && cellular_amounts[i] > 0.000001) {
        cellular_temperatures[i] = max(s, 0.0);
    }
    var gas = 0.0;
    for (var n = 0u; n < parameters.gas_count; n++) {
        gas += max(gas_concentrations[n * parameters.cell_count + i], 0.0);
    }
    let claim = rigid_claims[i];
    let blocked =
        cellular_materials[i] != 0u
            || claim != 0xffffffffu
            || occupancy[i] == 1u
            || occupancy[i] == 3u;
    let available = select(1.0 - clamp(fluid_coverage[i], 0.0, 1.0), 0.0, blocked);
    let air = clamp(available - gas, 0.0, 1.0);
    if (gas > 0.000001 || air * parameters.empty_space_heat_capacity > 0.000001) {
        gas_temperatures[i] = max(s, 0.0);
    }
}

@compute @workgroup_size(64)
fn scatter_fluid_particles(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let i = invocation.x;
    if
        (i >= parameters.particle_capacity
            || particles[i].is_active == 0u
            || particles[i].material_identifier == 0u
            || particles[i].amount <= 0.000001)
    {
        return;
    }
    let p = particles[i];
    let idx = cell_index(fluid_particle_world_cell(p.position, CELLS_PER_TILE_FLOAT));
    if (idx != INVALID_PHYSICAL_CELL_INDEX && idx < parameters.cell_count) {
        particles[i].temperature = max(solved[idx].w, 0.0);
    }
}

@compute @workgroup_size(64)
fn clear_rigid_temperature_sums(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if (invocation.x < parameters.rigid_capacity) {
        atomicStore(&rigid_temperature_sum[invocation.x], bitcast<u32>(0.0));
    }
}

@compute @workgroup_size(64)
fn accumulate_rigid_temperature_sums(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let logical = invocation.x;
    if (logical >= parameters.cell_count) {
        return;
    }
    let i = physical_index_from_logical(logical);
    if (i == INVALID_PHYSICAL_CELL_INDEX || i >= parameters.cell_count) {
        return;
    }
    let claim = rigid_claims[i];
    if (claim == 0xffffffffu || claim >= arrayLength(&rigid_cells)) {
        return;
    }
    let slot = rigid_cells[claim].state_slot;
    if
        (slot >= parameters.rigid_capacity
            || claim_counts[slot] == 0u
            || rigid_amounts[slot] <= 0.000001)
    {
        return;
    }
    atomic_add_f32(slot, max(solved[i].w, 0.0));
}

@compute @workgroup_size(64)
fn apply_rigid_temperatures(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let i = invocation.x;
    if (i >= parameters.rigid_capacity) {
        return;
    }
    let c = claim_counts[i];
    if (c > 0u && rigid_amounts[i] > 0.000001) {
        rigid_temperatures[i] =
            max(bitcast<f32>(atomicLoad(&rigid_temperature_sum[i])) / f32(c), 0.0);
    }
}
