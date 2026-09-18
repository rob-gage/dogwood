// Copyright Rob Gage 2026

#define_import_path compute::material_extraction
#import utility::simulation_constants::{
    CELLS_PER_TILE_FLOAT,
    EMPTY_MATERIAL_IDENTIFIER,
    INVALID_PHYSICAL_CELL_INDEX,
    RIGID_EXTERNAL_BODY_OCCUPANCY,
}
#import utility::cell_coordinates::world_cell_from_logical_tile_major_index
#import utility::material_identifier::material_dense_index
#import utility::tile_ring::physical_cell_index_from_world_cell

struct MaterialExtractionParameters {
    region_center: vec2<f32>,
    region_radius: f32,
    fixed_point_scale: f32,
    buffered_origin: vec2<i32>,
    buffered_tile_size: vec2<u32>,
    ring_offset: vec2<u32>,
    buffered_cell_count: u32,
    gas_count: u32,
    particle_capacity: u32,
    rigid_cell_count: u32,
    material_count: u32,
    padding: u32,
    material_offsets: vec4<u32>,
    material_counts: vec4<u32>,
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

@group(0) @binding(0) var<storage, read_write> cellular_material_identifiers: array<u32>;
@group(0) @binding(1) var<storage, read_write> cellular_appearances: array<u32>;
@group(0) @binding(2) var<storage, read_write> cellular_integrities: array<f32>;
@group(0) @binding(3) var<storage, read_write> cellular_kinematics: array<vec4<f32>>;
@group(0) @binding(4) var<storage, read_write> cellular_amounts: array<f32>;
@group(0) @binding(5) var<storage, read_write> cellular_temperatures: array<f32>;
@group(0) @binding(6) var<storage, read> external_body_occupancy: array<u32>;
@group(0) @binding(7) var<storage, read_write> particles: array<Particle>;
@group(0) @binding(8) var<storage, read_write> fluid_free_indices: array<u32>;
@group(0) @binding(9) var<storage, read_write> fluid_free_count: array<atomic<u32>>;
@group(0) @binding(10) var<storage, read_write> gas_concentrations: array<f32>;
@group(0) @binding(11) var<storage, read> material_filter_mask: array<u32>;
@group(0) @binding(12) var<storage, read_write> material_amounts: array<atomic<u32>>;
@group(0) @binding(13) var<storage, read> rigid_cells: array<RigidCell>;
@group(0) @binding(14) var<storage, read> rigid_transforms: array<vec4<f32>>;
@group(0) @binding(15) var<storage, read_write> rigid_cell_amounts: array<f32>;
@group(0) @binding(16) var<uniform> material_extraction_parameters: MaterialExtractionParameters;

fn material_matches(material_identifier: u32) -> bool {
    let dense_index: u32 = material_dense_index(
        material_identifier,
        material_extraction_parameters.material_offsets,
        material_extraction_parameters.material_counts,
    );
    if dense_index >= material_extraction_parameters.material_count {
        return false;
    }
    return (material_filter_mask[dense_index / 32u] & (1u << (dense_index % 32u))) != 0u;
}

fn inside_region(world_position: vec2<f32>) -> bool {
    let difference: vec2<f32> = world_position - material_extraction_parameters.region_center;
    return dot(difference, difference)
        <= material_extraction_parameters.region_radius
            * material_extraction_parameters.region_radius;
}

fn accumulate(material_identifier: u32, amount: f32) {
    let dense_index: u32 = material_dense_index(
        material_identifier,
        material_extraction_parameters.material_offsets,
        material_extraction_parameters.material_counts,
    );
    if dense_index < material_extraction_parameters.material_count && amount > 0.0 {
        atomicAdd(
            &material_amounts[dense_index],
            u32(amount * material_extraction_parameters.fixed_point_scale),
        );
    }
}

fn release_fluid_particle(particle_index: u32) {
    particles[particle_index].material_identifier = EMPTY_MATERIAL_IDENTIFIER;
    particles[particle_index].is_active = 0u;
    particles[particle_index].amount = 0.0;
    var free_count: u32 = atomicLoad(&fluid_free_count[0]);
    loop {
        if free_count >= arrayLength(&fluid_free_indices) {
            return;
        }
        let exchange = atomicCompareExchangeWeak(&fluid_free_count[0], free_count, free_count + 1u);
        if exchange.exchanged {
            fluid_free_indices[free_count] = particle_index;
            return;
        }
        free_count = exchange.old_value;
    }
}

fn extract_cellular_material(logical_cell_index: u32) {
    let world_cell: vec2<i32> = world_cell_from_logical_tile_major_index(
        logical_cell_index,
        material_extraction_parameters.buffered_origin,
        material_extraction_parameters.buffered_tile_size,
    );
    let physical_cell_index: u32 = physical_cell_index_from_world_cell(
        world_cell,
        material_extraction_parameters.buffered_origin,
        material_extraction_parameters.buffered_tile_size,
        material_extraction_parameters.ring_offset,
    );
    if physical_cell_index == INVALID_PHYSICAL_CELL_INDEX
        || external_body_occupancy[physical_cell_index] == RIGID_EXTERNAL_BODY_OCCUPANCY
        || !inside_region((vec2<f32>(world_cell) + vec2<f32>(0.5)) / CELLS_PER_TILE_FLOAT)
    {
        return;
    }
    let material_identifier: u32 = cellular_material_identifiers[physical_cell_index];
    let amount: f32 = cellular_amounts[physical_cell_index];
    if amount <= 0.0 || !material_matches(material_identifier) {
        return;
    }
    cellular_material_identifiers[physical_cell_index] = EMPTY_MATERIAL_IDENTIFIER;
    cellular_appearances[physical_cell_index] = 0u;
    cellular_integrities[physical_cell_index] = 0.0;
    cellular_kinematics[physical_cell_index] = vec4<f32>(0.0);
    cellular_amounts[physical_cell_index] = 0.0;
    cellular_temperatures[physical_cell_index] = 0.0;
    accumulate(material_identifier, amount);
}

fn extract_fluid_material(particle_index: u32) {
    if particle_index >= material_extraction_parameters.particle_capacity
        || particles[particle_index].is_active == 0u
        || particles[particle_index].material_identifier == EMPTY_MATERIAL_IDENTIFIER
        || !inside_region(particles[particle_index].position)
    {
        return;
    }
    let material_identifier: u32 = particles[particle_index].material_identifier;
    let amount: f32 = particles[particle_index].amount;
    if amount <= 0.0 || !material_matches(material_identifier) {
        return;
    }
    release_fluid_particle(particle_index);
    accumulate(material_identifier, amount);
}

fn extract_gas_material(gas_cell_index: u32) {
    if gas_cell_index >= material_extraction_parameters.gas_count
        * material_extraction_parameters.buffered_cell_count
    {
        return;
    }
    let logical_cell_index: u32 = gas_cell_index
        % material_extraction_parameters.buffered_cell_count;
    let world_cell: vec2<i32> = world_cell_from_logical_tile_major_index(
        logical_cell_index,
        material_extraction_parameters.buffered_origin,
        material_extraction_parameters.buffered_tile_size,
    );
    let physical_cell_index: u32 = physical_cell_index_from_world_cell(
        world_cell,
        material_extraction_parameters.buffered_origin,
        material_extraction_parameters.buffered_tile_size,
        material_extraction_parameters.ring_offset,
    );
    if physical_cell_index == INVALID_PHYSICAL_CELL_INDEX {
        return;
    }
    if !inside_region((vec2<f32>(world_cell) + vec2<f32>(0.5)) / CELLS_PER_TILE_FLOAT) {
        return;
    }
    let species: u32 = gas_cell_index / material_extraction_parameters.buffered_cell_count;
    let material_identifier: u32 = species + 1u;
    let amount_index: u32 = species * material_extraction_parameters.buffered_cell_count
        + physical_cell_index;
    let amount: f32 = gas_concentrations[amount_index];
    if amount <= 0.0 || !material_matches(material_identifier) {
        return;
    }
    gas_concentrations[amount_index] = 0.0;
    accumulate(material_identifier, amount);
}

fn extract_rigid_material(rigid_cell_index: u32) {
    if rigid_cell_index >= material_extraction_parameters.rigid_cell_count
        || rigid_cell_index >= arrayLength(&rigid_cells)
    {
        return;
    }
    let rigid_cell: RigidCell = rigid_cells[rigid_cell_index];
    let transform: vec4<f32> = rigid_transforms[rigid_cell.body * 3u];
    let local_center: vec2<f32> = (vec2<f32>(rigid_cell.local) + vec2<f32>(0.5))
        / CELLS_PER_TILE_FLOAT;
    let world_position: vec2<f32> = transform.xy + vec2<f32>(
        transform.z * local_center.x - transform.w * local_center.y,
        transform.w * local_center.x + transform.z * local_center.y,
    );
    if !inside_region(world_position) || !material_matches(rigid_cell.material_identifier) {
        return;
    }
    let amount: f32 = rigid_cell_amounts[rigid_cell.state_slot];
    if amount <= 0.0 {
        return;
    }
    rigid_cell_amounts[rigid_cell.state_slot] = 0.0;
    accumulate(rigid_cell.material_identifier, amount);
}

@compute @workgroup_size(64)
fn extract_materials(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let work_index: u32 = invocation.x;
    if work_index < material_extraction_parameters.buffered_cell_count {
        extract_cellular_material(work_index);
    }
    if work_index < material_extraction_parameters.particle_capacity {
        extract_fluid_material(work_index);
    }
    if work_index < material_extraction_parameters.gas_count
        * material_extraction_parameters.buffered_cell_count
    {
        extract_gas_material(work_index);
    }
    if work_index < material_extraction_parameters.rigid_cell_count {
        extract_rigid_material(work_index);
    }
}
