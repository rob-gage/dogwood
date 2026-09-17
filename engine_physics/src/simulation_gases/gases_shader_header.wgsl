// Copyright Rob Gage 2026

#define_import_path compute::gases

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
#import utility::material_identifier::material_form_from_identifier
#import utility::tile_ring::physical_cell_index_from_world_cell

struct Parameters {
  buffered_origin: vec2<i32>,
  buffered_tile_size: vec2<u32>,
  ring_offset: vec2<u32>,
  gravity: vec2<f32>,
  delta_time: f32,
  buffered_cell_count: u32,
  gas_count: u32,
  streaming_cell_count: u32,
  streaming_origin: vec2<i32>,
  streaming_tile_size: vec2<u32>,
  vorticity_confinement: f32,
  buoyancy_coefficient: f32,
  maximum_speed: f32,
  fluid_obstacle_coverage: f32,
  ambient_density: f32,
  ambient_temperature: f32,
  padding_1: vec2<u32>,}

@group(0) @binding(0) var<storage, read_write> velocity: array<vec2<f32>>;
@group(0) @binding(1) var<storage, read_write> velocity_scratch: array<
  vec2<f32>
>;
@group(0) @binding(2) var<storage, read_write> concentrations: array<f32>;
@group(0) @binding(3) var<storage, read_write> concentration_scratch: array<
  f32
>;
@group(0) @binding(4) var<storage, read_write> divergence: array<f32>;
@group(0) @binding(5) var<storage, read_write> pressure_a: array<f32>;
@group(0) @binding(6) var<storage, read_write> pressure_b: array<f32>;
@group(0) @binding(7) var<storage, read_write> curl: array<f32>;
@group(0) @binding(8) var<storage, read> cellular_material_identifiers: array<
  u32
>;
@group(0) @binding(9) var<storage, read> external_body_occupancy: array<u32>;
@group(0) @binding(10) var<storage, read> fluid_coverage: array<f32>;
@group(0) @binding(11) var<storage, read> gas_properties: array<vec4<f32>>;
@group(0) @binding(12) var<storage, read_write> streaming_data: array<u32>;
@group(0) @binding(14) var<storage, read_write> gas_temperature: array<f32>;
@group(0) @binding(13) var<uniform> parameters: Parameters;

// Semi-Lagrangian backtracing from immutable velocity into separate scratch

