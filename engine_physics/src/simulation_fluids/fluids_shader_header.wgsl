// Copyright Rob Gage 2026

#define_import_path compute::fluids

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

#import utility::actor_collision_shape::{
    ActorShape,
    actor_shape_from_parameters,
    actor_shape_world_extent,
    world_position_is_inside_actor_shape,
}
#import utility::cell_coordinates::{
    world_cell_from_logical_tile_major_index,
}
#import utility::material_identifier::{
    material_form_from_identifier,
    material_index_from_identifier,
}
#import utility::material_identifier::material_dense_index
#import utility::thermal_material::{
    ThermalMaterialRecord,
    ThermalMaterialParameters,
    thermal_material_conductivity,
    thermal_material_specific_heat_capacity
}
#import utility::tile_ring::physical_cell_index_from_world_cell
#import utility::fluid_spatial::{
    fluid_bucket_coordinates_from_position,
    fluid_bucket_index_from_coordinates,
    fluid_particle_belongs_to_cell
}

struct Particle {
  material_identifier: u32,
  is_active: u32,
  position: vec2<f32>,
  velocity: vec2<f32>,
  prediction_collision_displacement: vec2<f32>,
  amount: f32,
  temperature: f32,}

struct Parameters {
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
  padding_1: u32,}

struct DerivedFluidCellSample {
  material_identifier: u32,
  coverage: f32,
  velocity: vec2<f32>,
  mechanical_material_identifier: u32,
  mechanical_mass: f32,
  mechanical_velocity: vec2<f32>,
  thermal: vec4<f32>,}

struct MechanicalFluidCell {
  material_identifier: u32,
  mass: f32,
  velocity: vec2<f32>,}

struct DerivedFluidActorSample {
  capsule_cell_count: f32,
  coverage_sum: f32,
  velocity_sum: vec2<f32>,
  density_sum: f32,
  viscosity_sum: f32,}

@group(0) @binding(0) var<storage, read_write> particles: array<Particle>;
@group(0) @binding(1) var<storage, read_write> free_indices: array<u32>;
@group(0) @binding(2) var<storage, read_write> free_count: array<atomic<u32>>;
@group(0) @binding(3) var<storage, read_write> edit_cells: array<u32>;
@group(0) @binding(4) var<storage, read_write> bucket_heads: array<atomic<u32>>;
@group(0) @binding(5) var<storage, read_write> next_particle: array<u32>;
@group(0) @binding(6) var<storage, read_write> derived_material_identifiers: array<
  u32
>;
@group(0) @binding(7) var<storage, read_write> derived_coverage: array<f32>;
@group(0) @binding(8) var<storage, read_write> derived_velocity: array<
  vec4<f32>
>;
@group(0) @binding(9) var<storage, read> cellular_material_identifiers: array<
  u32
>;
@group(0) @binding(10) var<storage, read> external_body_occupancy: array<u32>;
@group(0) @binding(11) var<storage, read> external_body_velocity: array<
  vec4<f32>
>;
@group(0) @binding(12) var<uniform> parameters: Parameters;
@group(0) @binding(13) var<storage, read_write> predicted_positions: array<
  vec2<f32>
>;
@group(0) @binding(14) var<storage, read_write> lambdas: array<f32>;
@group(0) @binding(15) var<storage, read_write> position_corrections: array<
  vec2<f32>
>;
@group(0) @binding(16) var<storage, read> fluid_material_properties: array<
  vec4<f32>
>;
@group(0) @binding(17) var<storage, read_write> streaming_particles: array<
  Particle
>;
@group(0) @binding(18) var<storage, read_write> streaming_count: array<
  atomic<u32>
>;
@group(0) @binding(19) var<storage, read_write> streaming_results: array<u32>;
@group(0) @binding(20) var<storage, read_write> sample_output: array<vec4<f32>>;
@group(0) @binding(21) var<storage, read_write> mechanical_cells: array<
  MechanicalFluidCell
>;
@group(0) @binding(22) var<storage, read_write> mechanical_original_velocity: array<
  vec2<f32>
>;
@group(0) @binding(23) var<storage, read_write> accelerator_edits_pending: array<
  atomic<u32>
>;
@group(0) @binding(24) var<storage, read_write> edit_amounts: array<f32>;
@group(0) @binding(25) var<storage, read_write> edit_temperatures: array<f32>;
@group(0) @binding(26) var<storage, read> thermal_material_properties: array<
  ThermalMaterialRecord
>;
@group(0) @binding(27) var<uniform> thermal_material_parameters: ThermalMaterialParameters;
@group(0) @binding(28) var<storage, read_write> derived_thermal: array<
  vec4<f32>
>;
@group(1) @binding(0) var<storage, read_write> accelerator_edit_dispatch: array<u32>;


