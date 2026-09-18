// Copyright Rob Gage 2026

#define_import_path compute::cellular_pressure

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

#import utility::cell_coordinates::{
    world_cell_from_logical_tile_major_index,
}
#import utility::material_identifier::{
    material_form_from_identifier,
    material_index_from_identifier,
}
#import utility::tile_ring::{
    physical_cell_index_from_world_cell,
    physical_tile_from_logical_tile,
    world_cell_from_physical_tile_ring_index,
}

struct CellularPressureParameters {
    buffered_origin: vec2<i32>,
    buffered_tile_size: vec2<u32>,
    ring_offset: vec2<u32>,
    impulse_center: vec2<f32>,
    impulse_radius: f32,
    impulse_strength: f32,
    delta_time: f32,
    tick: u32,
    buffered_cell_count: u32,
    damage_rate: f32,
    gas_count: u32,
    rigid_body_count: u32,
    gravity: vec2<f32>,
    rigid_cell_count: u32,
    padding: u32,
    impulse_min: vec2<i32>,
    impulse_size: vec2<u32>,
}

struct StaticProperties {
    pressure_ignore_threshold: f32,
    pressure_transmission: f32,
    debris: u32,
    debris_yield_rate: f32,
    friction: f32,
    restitution: f32,
    padding: vec2<f32>,
}

struct MechanicalFluidCell {
    material_identifier: u32,
    mass: f32,
    velocity: vec2<f32>,
}

@group(0) @binding(0) var<storage, read_write> cellular_material_identifiers: array<u32>;
@group(0) @binding(1) var<storage, read_write> cellular_appearances: array<u32>;
@group(0) @binding(2) var<storage, read_write> cellular_integrities: array<f32>;
@group(0) @binding(3) var<storage, read_write> cellular_kinematics: array<vec4<f32>>;
@group(0) @binding(4) var<storage, read> cellular_static_properties: array<StaticProperties>;
@group(0) @binding(5) var<storage, read> cellular_dynamic_properties: array<vec4<f32>>;
@group(0) @binding(6) var<storage, read_write> pending_pressure: array<vec4<f32>>;
@group(0) @binding(7) var<storage, read_write> pressure_a: array<vec4<f32>>;
@group(0) @binding(8) var<storage, read_write> pressure_b: array<vec4<f32>>;
@group(0) @binding(9) var<storage, read_write> retained_pressure: array<vec4<f32>>;
@group(0) @binding(10) var<storage, read> external_body_occupancy: array<u32>;
@group(0) @binding(11) var<storage, read> external_body_velocity: array<vec4<f32>>;
@group(0) @binding(13) var<uniform> cellular_pressure_parameters: CellularPressureParameters;
@group(0) @binding(14) var<storage, read_write> active_pressure_tiles: array<atomic<u32>>;
@group(0) @binding(15) var<storage, read_write> active_pressure_tile_indices: array<u32>;
@group(0) @binding(16) var<storage, read_write> mechanical_fluid_cells: array<MechanicalFluidCell>;
@group(0) @binding(17) var<storage, read> fluid_pressure_properties: array<vec4<f32>>;
@group(0) @binding(18) var<storage, read_write> gas_velocity: array<vec2<f32>>;
@group(0) @binding(19) var<storage, read> gas_concentrations: array<f32>;
@group(0) @binding(20) var<storage, read> gas_properties: array<vec4<f32>>;
@group(0) @binding(21) var<storage, read> fluid_coverage: array<f32>;

struct MaterialMutationRequest {
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

@group(0) @binding(37) var<storage, read_write> material_mutation_requests: array<
    MaterialMutationRequest,
>;
@group(0) @binding(38) var<storage, read_write> material_mutation_request_count: array<atomic<u32>>;
@group(0) @binding(22) var<storage, read> rigid_owners: array<u32>;
@group(0) @binding(23) var<storage, read> rigid_material_identifiers: array<u32>;
@group(0) @binding(24) var<storage, read> rigid_transforms: array<vec4<f32>>;

struct RigidReaction {
    impulse_x: atomic<i32>,
    impulse_y: atomic<i32>,
    angular_impulse: atomic<i32>,
    overflow: atomic<i32>,
    constraint_x: atomic<i32>,
    constraint_y: atomic<i32>,
    constraint_angular: atomic<i32>,
    constraint_energy: atomic<u32>,
    support_x: atomic<i32>,
    support_y: atomic<i32>,
    support_angular: atomic<i32>,
    support_energy: atomic<u32>,
    recovery_x: atomic<i32>,
    recovery_y: atomic<i32>,
    recovery_angular: atomic<i32>,
    recovery_energy: atomic<u32>,
    source_motion: vec3<f32>,
    kinematic_constraint: atomic<u32>,
}

struct RigidContactStatistics {
    contact_count: atomic<u32>,
    static_contact_count: atomic<u32>,
    padding_1: atomic<u32>,
    padding_2: atomic<u32>,
    geometric_support: array<atomic<u32>, 4>,
    motion_support: array<atomic<u32>, 4>,
}

struct RigidStaticContact {
    found: u32,
    body: u32,
    material: u32,
    channel: u32,
    cell_index: u32,
    normal: vec2<f32>,
    point: vec2<f32>,
    penetration: f32,
}

@group(0) @binding(27) var<storage, read_write> rigid_reactions: array<RigidReaction>;
@group(0) @binding(28) var<storage, read_write> rigid_contact_statistics: array<
    RigidContactStatistics,
>;
@group(0) @binding(29) var<storage, read> rigid_cells: array<vec4<u32>>;

struct RigidPredictedMotion {
    x: atomic<i32>,
    y: atomic<i32>,
    angular: atomic<i32>,
    padding: atomic<i32>,
}

@group(0) @binding(30) var<storage, read_write> rigid_predicted_motion: array<RigidPredictedMotion>;
@group(0) @binding(31) var<storage, read_write> rigid_cell_integrities: array<f32>;
@group(0) @binding(32) var<storage, read_write> rigid_damage: array<atomic<u32>>;
@group(0) @binding(33) var<storage, read> rigid_claims: array<u32>;
@group(0) @binding(34) var<storage, read_write> rigid_fractures: array<atomic<u32>>;
@group(0) @binding(36) var<storage, read_write> rigid_fracture_count: atomic<u32>;
@group(1) @binding(0) var<storage, read_write> pressure_indirect_dispatch: array<atomic<u32>>;
@group(1) @binding(1) var<storage, read_write> rigid_damage_dispatch: array<atomic<u32>>;

var<workgroup> pressure_tile_has_source: atomic<u32>;
