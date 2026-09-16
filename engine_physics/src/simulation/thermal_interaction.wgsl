#define_import_path compute::thermal_interaction
#import utility::cell_coordinates::{world_cell_from_logical_tile_major_index}
#import utility::tile_ring::{physical_cell_index_from_world_cell}
#import utility::material_identifier::{material_dense_index, EMPTY_MATERIAL_IDENTIFIER}
#import utility::thermal_material::{ThermalMaterialRecord, ThermalMaterialParameters, thermal_material_conductivity, thermal_material_specific_heat_capacity}
struct Parameters { ambient: f32, empty_k: f32, empty_capacity: f32, cell_count: u32, gas_count: u32, padding: vec2<u32>, buffered_origin: vec2<i32>, buffered_tiles: vec2<u32>, ring_offset: vec2<u32> }
struct RigidCell { local: vec2<i32>, body: u32, material_identifier: u32, appearance: u32, state_slot: u32, state_generation: u32, padding: u32 }
@group(0) @binding(0) var<storage, read> cellular_materials: array<u32>;
@group(0) @binding(1) var<storage, read> cellular_amounts: array<f32>;
@group(0) @binding(2) var<storage, read> cellular_temperatures: array<f32>;
@group(0) @binding(3) var<storage, read> rigid_claims: array<u32>;
@group(0) @binding(4) var<storage, read> rigid_cells: array<RigidCell>;
@group(0) @binding(5) var<storage, read> rigid_amounts: array<f32>;
@group(0) @binding(6) var<storage, read> rigid_temperatures: array<f32>;
@group(0) @binding(7) var<storage, read> fluid_thermal: array<vec4<f32>>;
@group(0) @binding(8) var<storage, read> gas_concentrations: array<f32>;
@group(0) @binding(9) var<storage, read> gas_temperatures: array<f32>;
@group(0) @binding(10) var<storage, read> thermal_properties: array<ThermalMaterialRecord>;
@group(0) @binding(11) var<storage, read> external_occupancy: array<u32>;
@group(0) @binding(12) var<storage, read> fluid_coverage: array<f32>;
@group(0) @binding(13) var<storage, read_write> interaction: array<vec4<f32>>;
@group(0) @binding(14) var<uniform> parameters: Parameters;
@group(0) @binding(15) var<uniform> material_parameters: ThermalMaterialParameters;
@group(0) @binding(16) var<storage, read_write> rigid_raster_claim_counts: array<atomic<u32>>;

@compute @workgroup_size(64)
fn clear_rigid_claim_counts(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x < arrayLength(&rigid_raster_claim_counts)) { atomicStore(&rigid_raster_claim_counts[id.x], 0u); }
}

@compute @workgroup_size(64)
fn count_rigid_claims(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= parameters.cell_count) { return; }
    let cell = world_cell_from_logical_tile_major_index(id.x, parameters.buffered_origin, parameters.buffered_tiles);
    let index = physical_cell_index_from_world_cell(cell, parameters.buffered_origin, parameters.buffered_tiles, parameters.ring_offset);
    let claim = rigid_claims[index];
    if (claim >= arrayLength(&rigid_cells)) { return; }
    let slot = rigid_cells[claim].state_slot;
    if (slot < arrayLength(&rigid_raster_claim_counts)) { atomicAdd(&rigid_raster_claim_counts[slot], 1u); }
}

@compute @workgroup_size(64)
fn gather_thermal_interaction(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= parameters.cell_count) { return; }
    let cell = world_cell_from_logical_tile_major_index(id.x, parameters.buffered_origin, parameters.buffered_tiles);
    let index = physical_cell_index_from_world_cell(cell, parameters.buffered_origin, parameters.buffered_tiles, parameters.ring_offset);
    var capacity = 0.0; var energy = 0.0; var conductivity = 0.0;
    let canonical = cellular_materials[index];
    if (canonical != EMPTY_MATERIAL_IDENTIFIER) {
        let dense = material_dense_index(canonical, material_parameters.offsets, material_parameters.counts);
        let amount = max(cellular_amounts[index], 0.0);
        if (dense != 0xffffffffu && dense < arrayLength(&thermal_properties) && amount > 0.000001) {
            let record = thermal_properties[dense]; let c = amount * thermal_material_specific_heat_capacity(record);
            capacity += c; energy += c * cellular_temperatures[index]; conductivity += thermal_material_conductivity(record);
        }
    }
    let claim = rigid_claims[index];
    if (claim < arrayLength(&rigid_cells)) {
        let rigid = rigid_cells[claim]; let dense = material_dense_index(rigid.material_identifier, material_parameters.offsets, material_parameters.counts);
        if (dense != 0xffffffffu && dense < arrayLength(&thermal_properties) && rigid.state_slot < arrayLength(&rigid_amounts)) {
            let amount = max(rigid_amounts[rigid.state_slot], 0.0); let share = 1.0 / f32(max(atomicLoad(&rigid_raster_claim_counts[rigid.state_slot]), 1u));
            let record = thermal_properties[dense]; let c = amount * thermal_material_specific_heat_capacity(record) * share;
            capacity += c; energy += c * rigid_temperatures[rigid.state_slot]; if (amount > 0.000001) { conductivity += thermal_material_conductivity(record); }
        }
    }
    let fluid = fluid_thermal[index]; capacity += fluid.x; energy += fluid.y; conductivity += fluid.z;
    var gas_amount = 0.0; var gas_capacity = 0.0; var gas_conductivity = 0.0;
    for (var species=0u; species < parameters.gas_count; species++) {
            let amount = max(gas_concentrations[species * parameters.cell_count + index], 0.0); gas_amount += amount;
            let dense = material_dense_index(species + 1u, material_parameters.offsets, material_parameters.counts);
            if (dense != 0xffffffffu && dense < arrayLength(&thermal_properties)) { let record = thermal_properties[dense]; gas_capacity += amount * thermal_material_specific_heat_capacity(record); gas_conductivity += amount * thermal_material_conductivity(record); }
    }
    let available = select(1.0 - clamp(fluid_coverage[index], 0.0, 1.0), 0.0, canonical != EMPTY_MATERIAL_IDENTIFIER || claim < arrayLength(&rigid_cells) || external_occupancy[index] == 1u || external_occupancy[index] == 3u);
        let air = clamp(available - gas_amount, 0.0, 1.0);
        capacity += gas_capacity + air * parameters.empty_capacity; energy += (gas_capacity + air * parameters.empty_capacity) * gas_temperatures[index]; conductivity += gas_conductivity + air * parameters.empty_k;
    let equilibrium = select(parameters.ambient, energy / capacity, capacity > 0.000001);
    interaction[index] = vec4<f32>(capacity, energy, conductivity, max(equilibrium, 0.0));
}
