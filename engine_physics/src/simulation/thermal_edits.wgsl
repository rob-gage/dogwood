#define_import_path compute::thermal_edits
#import utility::tile_ring::{physical_cell_index_from_world_cell, INVALID_PHYSICAL_CELL_INDEX}

// 48 bytes: six 8-byte vectors followed by six scalar u32/f32 words.
struct Parameters { ring_origin: vec2<i32>, ring_tiles: vec2<u32>, ring_offset: vec2<u32>, capacity: u32, request_count: u32, generation: u32, padding_0: u32, padding_1: u32, padding_2: u32 }
struct Particle { material_identifier: u32, is_active: u32, position: vec2<f32>, velocity: vec2<f32>, prediction_collision_displacement: vec2<f32>, amount: f32, temperature: f32 }
struct RigidCell { local: vec2<i32>, body: u32, material_identifier: u32, appearance: u32, state_slot: u32, state_generation: u32, padding: u32 }
@group(0) @binding(0) var<storage, read> requests: array<vec2<u32>>;
@group(0) @binding(1) var<storage, read> request_count: array<u32>;
@group(0) @binding(2) var<storage, read_write> cellular_materials: array<u32>;
@group(0) @binding(3) var<storage, read_write> cellular_temperatures: array<f32>;
@group(0) @binding(4) var<storage, read_write> gas_temperatures: array<f32>;
@group(0) @binding(5) var<storage, read_write> particles: array<Particle>;
@group(0) @binding(6) var<storage, read> rigid_claims: array<u32>;
@group(0) @binding(7) var<storage, read> rigid_cells: array<RigidCell>;
@group(0) @binding(8) var<storage, read_write> rigid_temperatures: array<f32>;
@group(0) @binding(9) var<storage, read_write> deltas: array<vec2<u32>>;
@group(0) @binding(10) var<storage, read_write> rigid_flags: array<atomic<u32>>;
@group(0) @binding(11) var<uniform> parameters: Parameters;

@compute @workgroup_size(64)
fn apply_thermal_requests(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= min(request_count[0], parameters.capacity)) { return; }
    let request = requests[id.x]; let cell = request.x; let delta = bitcast<f32>(request.y);
    if (cell >= parameters.capacity) { return; }
    deltas[cell] = vec2<u32>(bitcast<u32>(delta), parameters.generation);
    if (cellular_materials[cell] != 0u) { cellular_temperatures[cell] = max(0.0, cellular_temperatures[cell] + delta); }
    if (cellular_materials[cell] == 0u && rigid_claims[cell] == 0xffffffffu) { gas_temperatures[cell] = max(0.0, gas_temperatures[cell] + delta); }
    let claim = rigid_claims[cell];
    if (claim != 0xffffffffu && claim < arrayLength(&rigid_cells)) {
        let slot = rigid_cells[claim].state_slot;
        if (slot < arrayLength(&rigid_temperatures)) {
            var old = atomicLoad(&rigid_flags[slot]);
            loop {
                let next = bitcast<u32>(bitcast<f32>(old) + delta);
                let result = atomicCompareExchangeWeak(&rigid_flags[slot], old, next);
                if (result.exchanged) { break; }
                old = result.old_value;
            }
        }
    }
}

@compute @workgroup_size(64)
fn apply_thermal_fluid(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= arrayLength(&particles) || particles[id.x].is_active == 0u) { return; }
    let p = particles[id.x]; let cell = vec2<i32>(floor(p.position * 8.0));
    let index = physical_cell_index_from_world_cell(cell, parameters.ring_origin, parameters.ring_tiles, parameters.ring_offset);
    if (index != INVALID_PHYSICAL_CELL_INDEX && index < parameters.capacity && deltas[index].y == parameters.generation) { particles[id.x].temperature = max(0.0, p.temperature + bitcast<f32>(deltas[index].x)); }
}

@compute @workgroup_size(64)
fn apply_thermal_rigid(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= arrayLength(&rigid_temperatures) || atomicLoad(&rigid_flags[id.x]) == 0u) { return; }
    rigid_temperatures[id.x] = max(0.0, rigid_temperatures[id.x] + bitcast<f32>(atomicLoad(&rigid_flags[id.x])));
    atomicStore(&rigid_flags[id.x], 0u);
}
