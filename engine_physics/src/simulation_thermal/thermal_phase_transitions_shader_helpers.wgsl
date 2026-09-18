// Copyright Rob Gage 2026
#define_import_path compute::thermal_phase_transitions
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
    material_dense_index,
    material_form_from_identifier
}
#import utility::thermal_material::{
    ThermalMaterialRecord,
    ThermalMaterialParameters,
    thermal_material_specific_heat_capacity,
    thermal_material_cold_enabled,
    thermal_material_cold_threshold,
    thermal_material_cold_target,
    thermal_material_cold_yield,
    thermal_material_cold_latent_energy,
    thermal_material_hot_enabled,
    thermal_material_hot_threshold,
    thermal_material_hot_target,
    thermal_material_hot_yield,
    thermal_material_hot_latent_energy
}
#import utility::tile_ring::{
    physical_cell_index_from_world_cell,
    world_cell_from_physical_tile_ring_index
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

// A request is deliberately authority-addressed: cell, kind (0 cell/1 particle/2 gas), locator,
// expected source, replacement, amount, temperature, and source world position.
struct ThermalPhaseTransitionRequest {
    cell: u32,
    kind: u32,
    locator: u32,
    expected_source: u32,
    replacement: u32,
    amount: u32,
    temperature: u32,
    world_x: u32,
    world_y: u32,
}

struct ThermalPhaseTransitionParameters {
    origin: vec2<i32>,
    tiles: vec2<u32>,
    ring: vec2<u32>,
    cell_count: u32,
    particle_count: u32,
    gas_count: u32,
    tick: u32,
    rigid_count: u32,
}

@group(0) @binding(0) var<storage, read> thermal_cellular_material_identifiers: array<u32>;
@group(0) @binding(1) var<storage, read> amounts: array<f32>;
@group(0) @binding(2) var<storage, read> temperatures: array<f32>;
@group(0) @binding(3) var<storage, read> particles: array<Particle>;
@group(0) @binding(4) var<storage, read> concentrations: array<f32>;
@group(0) @binding(5) var<storage, read> gas_temperatures: array<f32>;
@group(0) @binding(6) var<storage, read> properties: array<ThermalMaterialRecord>;
@group(0) @binding(7) var<uniform> thermal: ThermalMaterialParameters;
@group(0) @binding(8) var<storage, read_write> requests: array<ThermalPhaseTransitionRequest>;
@group(0) @binding(9) var<storage, read_write> request_count: array<atomic<u32>>;
@group(0) @binding(10) var<uniform> thermal_phase_transition_parameters: ThermalPhaseTransitionParameters;
// DX12 lowers atomicLoad to an interlocked operation, which requires an RW
// buffer view even when the shader does not change the stored value.
@group(0) @binding(11) var<storage, read_write> rigid_claims: array<atomic<u32>>;

struct GasFluidCandidate {
    replacement: u32,
    amount: f32,
    temperature: f32,
    position: vec2<f32>,
}

@group(0) @binding(12) var<storage, read_write> gas_fluid_candidates: array<GasFluidCandidate>;

// Rigid transitions are deliberately published separately from raster-cell
// requests.  A descriptor is one authoritative state, not one raster claim.
// The CPU consumes this compact buffer asynchronously and performs the Rapier
// topology mutation after revalidating slot generation and source material.
struct RigidPhaseCandidate {
    state_slot: u32,
    state_generation: u32,
    expected_source: u32,
    replacement: u32,
    amount: f32,
    temperature: f32,
    body: u32,
    local_x: i32,
    local_y: i32,
    reserved_particle: u32,
}

@group(0) @binding(13) var<storage, read> rigid_cells: array<RigidCell>;
@group(0) @binding(14) var<storage, read> rigid_amounts: array<f32>;
@group(0) @binding(15) var<storage, read> rigid_temperatures: array<f32>;
@group(0) @binding(16) var<storage, read_write> rigid_phase_candidates: array<RigidPhaseCandidate>;
@group(0) @binding(17) var<storage, read_write> rigid_phase_count: array<atomic<u32>>;
@group(0) @binding(18) var<storage, read_write> fluid_free_indices: array<u32>;
@group(0) @binding(19) var<storage, read_write> fluid_free_count: array<atomic<u32>>;
@group(0) @binding(20) var<storage, read> rollback_slots: array<u32>;
@group(0) @binding(21) var<storage, read> rollback_count: array<u32>;

fn reserve_fluid_particle() -> u32 {
    var n = atomicLoad(&fluid_free_count[0]);
    loop {
        if (n == 0u || n > arrayLength(&fluid_free_indices)) {
            return 0xffffffffu;
        }
        let r = atomicCompareExchangeWeak(&fluid_free_count[0], n, n - 1u);
        if (r.exchanged) {
            let thermal_fluid_particle_index = fluid_free_indices[n - 1u];
            if (thermal_fluid_particle_index >= arrayLength(&particles)) {
                return 0xffffffffu;
            }
            return thermal_fluid_particle_index;
        }
        n = r.old_value;
    }
    return 0xffffffffu;
}

fn release_reserved_fluid_particle(slot: u32) {
    if (slot >= arrayLength(&fluid_free_indices)) {
        return;
    }
    var n = atomicLoad(&fluid_free_count[0]);
    loop {
        if (n >= arrayLength(&fluid_free_indices)) {
            return;
        }
        let r = atomicCompareExchangeWeak(&fluid_free_count[0], n, n + 1u);
        if (r.exchanged) {
            fluid_free_indices[n] = slot;
            return;
        }
        n = r.old_value;
    }
}

@compute @workgroup_size(64)
fn rollback_rigid_reservations(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if (invocation.x >= rollback_count[0] || invocation.x >= arrayLength(&rollback_slots)) {
        return;
    }
    release_reserved_fluid_particle(rollback_slots[invocation.x]);
}

fn should_yield(rate: f32, cell: u32, source: u32) -> bool {
    if (rate >= 1.0) {
        return true;
    }
    if (rate <= 0.0) {
        return false;
    }
    let h = (cell * 1664525u + source * 1013904223u + thermal_phase_transition_parameters.tick * 747796405u);
    return f32(h & 0x00ffffffu) / 16777216.0 < rate;
}

fn transition(
    source: u32,
    amount: f32,
    temperature: f32,
    cell: u32,
    kind: u32,
    locator: u32,
    world: vec2<i32>,
) {
    if (!(amount > 0.000001) || temperature != temperature || abs(temperature) > 3.4e38) {
        return;
    }
    let dense = material_dense_index(source, thermal.offsets, thermal.counts);
    if (dense == 0xffffffffu || dense >= arrayLength(&properties)) {
        return;
    }
    let record = properties[dense];
    let cp = thermal_material_specific_heat_capacity(record);
    if (!(cp > 0.000001)) {
        return;
    }
    var replacement = EMPTY_MATERIAL_IDENTIFIER;
    var threshold = 0.0;
    var latent = 0.0;
    var rate = 0.0;
    var signed = 0.0;
    if
        (thermal_material_hot_enabled(record) && temperature >= thermal_material_hot_threshold(
            record,
        ))
    {
        replacement = thermal_material_hot_target(record);
        threshold = thermal_material_hot_threshold(record);
        latent = thermal_material_hot_latent_energy(record);
        rate = thermal_material_hot_yield(record);
        signed = 1.0;
    } else if
        (thermal_material_cold_enabled(record) && temperature <= thermal_material_cold_threshold(
            record,
        ))
    {
        replacement = thermal_material_cold_target(record);
        threshold = thermal_material_cold_threshold(record);
        latent = thermal_material_cold_latent_energy(record);
        rate = thermal_material_cold_yield(record);
        signed = -1.0;
    } else {
        return;
    }
    let target_dense = material_dense_index(replacement, thermal.offsets, thermal.counts);
    if
        (replacement == EMPTY_MATERIAL_IDENTIFIER
            || replacement == source
            || target_dense == 0xffffffffu
            || target_dense >= arrayLength(&properties))
    {
        return;
    }
    let target_cp = thermal_material_specific_heat_capacity(properties[target_dense]);
    if (!(target_cp > 0.000001)) {
        return;
    }
    let sensible = amount * cp * max(signed * (temperature - threshold), 0.0);
    let required = amount * max(latent, 0.0);
    if (latent > 0.0 && sensible < required) {
        return;
    }
    if (!should_yield(rate, cell, source)) {
        return;
    }
    let target_temperature =
        max(threshold + signed * max(sensible - required, 0.0) / (amount * target_cp), 0.0);
    let n = atomicAdd(&request_count[0], 1u);
    if (n >= arrayLength(&requests)) {
        return;
    }
    let position = (vec2<f32>(world) + vec2<f32>(0.5)) / CELLS_PER_TILE_FLOAT;
    requests[n] =
        ThermalPhaseTransitionRequest(
            cell,
            kind,
            locator,
            source,
            replacement,
            bitcast<u32>(amount),
            bitcast<u32>(target_temperature),
            bitcast<u32>(position.x),
            bitcast<u32>(position.y),
        );
}

@compute @workgroup_size(64)
