// Copyright Rob Gage 2026
// This is the fixed reaction metadata representation written by ReactionMaterialTable.
struct Reaction {
    words: array<u32, 27>,
}

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
    rigid_padding: vec2<u32>,
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
    world_y: u32,
}

struct Parameters {
    cell_count: u32,
    gas_count: u32,
    reaction_count: u32,
    padding: u32,
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
@group(0) @binding(12) var<storage, read_write> mutation_requests: array<Request>;
@group(0) @binding(13) var<storage, read_write> mutation_request_count: array<atomic<u32>>;
@group(0) @binding(14) var<storage, read_write> reaction_energy: array<f32>;
@group(0) @binding(15) var<storage, read_write> pending_pressure: array<vec4<f32>>;

// Read-only authoritative fluid bridge; discovery will use the shared spatial
// utility and these chains in the fluid-reaction pass.
struct FluidParticleAuthority {
    material_identifier: u32,
    is_active: u32,
    position: vec2<f32>,
    velocity: vec2<f32>,
    prediction_collision_displacement: vec2<f32>,
    amount: f32,
    temperature: f32,
}

@group(0) @binding(16) var<storage, read_write> fluid_particles: array<FluidParticleAuthority>;
@group(0) @binding(17) var<storage, read> fluid_bucket_heads: array<atomic<u32>>;
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
    padding_1: u32,
}

@group(0) @binding(19) var<uniform> fluid_spatial_parameters: FluidSpatialParameters;
@group(0) @binding(20) var<storage, read_write> fluid_free_indices: array<u32>;
@group(0) @binding(21) var<storage, read_write> fluid_free_count: array<atomic<u32>>;
@group(0) @binding(22) var<storage, read_write> fluid_reservations: array<atomic<u32>>;
@group(0) @binding(23) var<storage, read_write> gas_reservations: array<atomic<u32>>;
@group(0) @binding(24) var<storage, read_write> gas_output_reservations: array<atomic<u32>>;
@group(0) @binding(25) var<storage, read_write> canonical_reservations: array<atomic<u32>>;
@group(0) @binding(26) var<storage, read_write> fluid_reservation_owners: array<atomic<u32>>;
@group(0) @binding(27) var<storage, read> rigid_cells: array<RigidCell>;
@group(0) @binding(28) var<storage, read_write> rigid_amounts: array<f32>;
@group(0) @binding(29) var<storage, read_write> rigid_reservations: array<atomic<u32>>;
@group(0) @binding(30) var<storage, read_write> rigid_removal_events: array<vec4<u32>>;
@group(0) @binding(31) var<storage, read_write> rigid_removal_count: array<atomic<u32>>;
@group(0) @binding(32) var<storage, read_write> candidate_indices: array<u32>;
@group(0) @binding(33) var<storage, read_write> candidate_count: array<atomic<u32>>;

struct SortParameters {
    k: u32,
    j: u32,
}

@group(0) @binding(34) var<uniform> sort_parameters: SortParameters;
@group(0) @binding(35) var<storage, read> sort_steps: array<vec2<u32>>;
@group(0) @binding(36) var<storage, read_write> sort_indirect: array<u32>;
@group(0) @binding(37) var<storage, read> gas_temperatures: array<f32>;
@group(0) @binding(38) var<storage, read> rigid_temperatures: array<f32>;
