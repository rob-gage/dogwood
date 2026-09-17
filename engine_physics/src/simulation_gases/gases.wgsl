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
@compute @workgroup_size(64)
fn advect_gas_velocity(@builtin(global_invocation_id) invocation: vec3<u32>) {
  let logical_index: u32 = invocation.x;
  if logical_index >= parameters.buffered_cell_count {
    return;
  }
  let cell: vec2<i32> = world_cell_from_gas_logical_index(logical_index);
  let index: u32 = gas_physical_cell_index_from_world_cell(cell);
  if world_cell_is_gas_obstacle(cell) {
    velocity_scratch[index] = vec2<f32>(0.0);
    return;
  }
  let center: vec2<f32> = vec2<f32>(cell) + vec2<f32>(0.5);
  let backtraced: vec2<f32> = center - velocity[index] * parameters.delta_time;
  velocity_scratch[index] =
    sample_gas_velocity_bilinear_at_cell_position(backtraced);
}

// Calculates scalar curl from the advected scratch velocity field
@compute @workgroup_size(64)
fn calculate_gas_curl(@builtin(global_invocation_id) invocation: vec3<u32>) {
  let logical_index: u32 = invocation.x;
  if logical_index >= parameters.buffered_cell_count {
    return;
  }
  let cell: vec2<i32> = world_cell_from_gas_logical_index(logical_index);
  let index: u32 = gas_physical_cell_index_from_world_cell(cell);
  if world_cell_is_gas_obstacle(cell) {
    curl[index] = 0.0;
    return;
  }
  let left: vec2<f32> =
    gas_scratch_velocity_at_world_cell(cell + vec2<i32>(-1, 0));
  let right: vec2<f32> =
    gas_scratch_velocity_at_world_cell(cell + vec2<i32>(1, 0));
  let bottom: vec2<f32> =
    gas_scratch_velocity_at_world_cell(cell + vec2<i32>(0, -1));
  let top: vec2<f32> =
    gas_scratch_velocity_at_world_cell(cell + vec2<i32>(0, 1));
  curl[index] = 0.5 * ((right.y - left.y) - (top.x - bottom.x));
}

// Applies buoyancy and vorticity confinement to advected gas velocity
@compute @workgroup_size(64)
fn apply_gas_forces(@builtin(global_invocation_id) invocation: vec3<u32>) {
  let logical_index: u32 = invocation.x;
  if logical_index >= parameters.buffered_cell_count {
    return;
  }
  let cell: vec2<i32> = world_cell_from_gas_logical_index(logical_index);
  let index: u32 = gas_physical_cell_index_from_world_cell(cell);
  if world_cell_is_gas_obstacle(cell) {
    velocity[index] = vec2<f32>(0.0);
    return;
  }
  let buoyancy: vec2<f32> =
    parameters.gravity
      * CELLS_PER_TILE_FLOAT
      * gas_density_offset_from_ambient(index)
      * parameters.buoyancy_coefficient;
  let confinement: vec2<f32> = calculate_gas_vorticity_confinement(cell, index);
  var next_velocity: vec2<f32> =
    velocity_scratch[index] + (buoyancy + confinement) * parameters.delta_time;
  let speed: f32 = length(next_velocity);
  if speed > parameters.maximum_speed {
    next_velocity *= parameters.maximum_speed / speed;
  }
  velocity[index] = next_velocity;
}

// Calculates velocity divergence for the incompressibility projection
@compute @workgroup_size(64)
fn calculate_gas_divergence(
  @builtin(global_invocation_id) invocation: vec3<u32>) {
  let logical_index: u32 = invocation.x;
  if logical_index >= parameters.buffered_cell_count {
    return;
  }
  let cell: vec2<i32> = world_cell_from_gas_logical_index(logical_index);
  let index: u32 = gas_physical_cell_index_from_world_cell(cell);
  if world_cell_is_gas_obstacle(cell) {
    divergence[index] = 0.0;
    return;
  }
  divergence[index] =
    0.5 * (gas_velocity_at_world_cell(cell + vec2<i32>(1, 0)).x
      - gas_velocity_at_world_cell(cell + vec2<i32>(-1, 0)).x
      + gas_velocity_at_world_cell(cell + vec2<i32>(0, 1)).y
      - gas_velocity_at_world_cell(cell + vec2<i32>(0, -1)).y);
}

// Clears both Jacobi pressure fields before projection
@compute @workgroup_size(64)
fn clear_gas_pressure(@builtin(global_invocation_id) invocation: vec3<u32>) {
  if invocation.x >= parameters.buffered_cell_count {
    return;
  }
  pressure_a[invocation.x] = 0.0;
  pressure_b[invocation.x] = 0.0;
}

// Solves one Jacobi iteration from pressure B into pressure A
@compute @workgroup_size(64)
fn solve_gas_pressure_a(@builtin(global_invocation_id) invocation: vec3<u32>) {
  let logical_index: u32 = invocation.x;
  if logical_index >= parameters.buffered_cell_count {
    return;
  }
  let cell: vec2<i32> = world_cell_from_gas_logical_index(logical_index);
  let index: u32 = gas_physical_cell_index_from_world_cell(cell);
  if world_cell_is_gas_obstacle(cell) {
    pressure_a[index] = 0.0;
    return;
  }
  pressure_a[index] = calculate_gas_jacobi_pressure(cell, index, false);
}

// Solves one Jacobi iteration from pressure A into pressure B
@compute @workgroup_size(64)
fn solve_gas_pressure_b(@builtin(global_invocation_id) invocation: vec3<u32>) {
  let logical_index: u32 = invocation.x;
  if logical_index >= parameters.buffered_cell_count {
    return;
  }
  let cell: vec2<i32> = world_cell_from_gas_logical_index(logical_index);
  let index: u32 = gas_physical_cell_index_from_world_cell(cell);
  if world_cell_is_gas_obstacle(cell) {
    pressure_b[index] = 0.0;
    return;
  }
  pressure_b[index] = calculate_gas_jacobi_pressure(cell, index, true);
}

// Projects velocity with pressure B after the fixed twelve Jacobi iterations
@compute @workgroup_size(64)
fn project_gas_velocity(@builtin(global_invocation_id) invocation: vec3<u32>) {
  let logical_index: u32 = invocation.x;
  if logical_index >= parameters.buffered_cell_count {
    return;
  }
  let cell: vec2<i32> = world_cell_from_gas_logical_index(logical_index);
  let index: u32 = gas_physical_cell_index_from_world_cell(cell);
  if world_cell_is_gas_obstacle(cell) {
    velocity[index] = vec2<f32>(0.0);
    return;
  }
  let center_pressure: f32 = pressure_b[index];
  var projected: vec2<f32> =
    velocity[index] - 0.5 * vec2<f32>(
      gas_pressure_at_world_cell(
        cell + vec2<i32>(1, 0),
        center_pressure,
        false) - gas_pressure_at_world_cell(
        cell + vec2<i32>(-1, 0),
        center_pressure,
        false),
      gas_pressure_at_world_cell(
        cell + vec2<i32>(0, 1),
        center_pressure,
        false) - gas_pressure_at_world_cell(
        cell + vec2<i32>(0, -1),
        center_pressure,
        false),);
  velocity[index] =
    clamp_projected_gas_velocity_against_obstacle_faces(cell, projected);
}

// Advects, diffuses, and dissipates every gas-species concentration
@compute @workgroup_size(64)
fn advect_gas_concentrations(
  @builtin(global_invocation_id) invocation: vec3<u32>) {
  let field_index: u32 = invocation.x;
  let field_count: u32 = parameters.buffered_cell_count * parameters.gas_count;
  if field_index >= field_count {
    return;
  }
  let species: u32 = field_index / parameters.buffered_cell_count;
  let logical_index: u32 = field_index % parameters.buffered_cell_count;
  let cell: vec2<i32> = world_cell_from_gas_logical_index(logical_index);
  let index: u32 = gas_physical_cell_index_from_world_cell(cell);
  let output_index: u32 =
    gas_concentration_storage_index_from_species_and_physical_cell(
      species,
      index);
  if world_cell_is_gas_obstacle(cell) {
    let remaining: f32 =
      exp(-max(gas_properties[species * 2u].w, 0.0) * parameters.delta_time);
    concentration_scratch[output_index] =
      select(
        concentrations[output_index] * remaining,
        0.0,
        gas_open_neighbor_count(cell) > 0u);
    return;
  }
  let current: f32 = concentrations[output_index];
  let transported: f32 =
    current - parameters.delta_time * (gas_concentration_flux_across_positive_x_face(
      species,
      cell)
      - gas_concentration_flux_across_positive_x_face(
        species,
        cell + vec2<i32>(-1, 0))
      + gas_concentration_flux_across_positive_y_face(species, cell)
      - gas_concentration_flux_across_positive_y_face(
        species,
        cell + vec2<i32>(0, -1)));
  let properties: vec4<f32> = gas_properties[species * 2u];
  let compressibility: f32 =
    clamp(gas_properties[species * 2u + 1u].x, 0.0, 1.0);
  let mixing: f32 =
    min(
      (max(
        properties.y,
        0.0) + (1.0 - compressibility) * INCOMPRESSIBILITY_MIXING) * parameters.delta_time,
      0.24,);
  let mixed: f32 =
    transported
      + mixing * (gas_concentration_at_world_cell(
        species,
        cell + vec2<i32>(-1, 0),
        current)
        + gas_concentration_at_world_cell(
          species,
          cell + vec2<i32>(1, 0),
          current)
        + gas_concentration_at_world_cell(
          species,
          cell + vec2<i32>(0, -1),
          current)
        + gas_concentration_at_world_cell(
          species,
          cell + vec2<i32>(0, 1),
          current)
        - 4.0 * current)
      + gas_displaced_into_open_cell(species, cell);
  let remaining: f32 = exp(-max(properties.w, 0.0) * parameters.delta_time);
  concentration_scratch[output_index] = max(0.0, mixed) * remaining;
}

fn gas_open_neighbor_count(cell: vec2<i32>) -> u32 {
  var count = 0u;
  for (var direction = 0u; direction < 4u; direction += 1u) {
    let offset =
      select(
        select(vec2<i32>(0, -1), vec2<i32>(0, 1), direction == 3u),
        select(vec2<i32>(-1, 0), vec2<i32>(1, 0), direction == 1u),
        direction < 2u,);
    if !world_cell_is_gas_obstacle(cell + offset) {
      count += 1u;
    }
  }
  return count;
}

fn gas_displaced_into_open_cell(species: u32, cell: vec2<i32>) -> f32 {
  var displaced = 0.0;
  for (var direction = 0u; direction < 4u; direction += 1u) {
    let offset =
      select(
        select(vec2<i32>(0, -1), vec2<i32>(0, 1), direction == 3u),
        select(vec2<i32>(-1, 0), vec2<i32>(1, 0), direction == 1u),
        direction < 2u,);
    let source = cell + offset;
    if !world_cell_is_gas_obstacle(source) {
      continue;
    }
    let source_index = gas_physical_cell_index_from_world_cell(source);
    let open_neighbors = gas_open_neighbor_count(source);
    if source_index != INVALID_PHYSICAL_CELL_INDEX && open_neighbors > 0u {
      displaced +=
        concentrations[gas_concentration_storage_index_from_species_and_physical_cell(
          species,
          source_index)] / f32(open_neighbors);
    }
  }
  return displaced;
}

// Clears authoritative gas state after a physical ring slot is reassigned
@compute @workgroup_size(64)
fn clear_gas_area(@builtin(global_invocation_id) invocation: vec3<u32>) {
  if invocation.x >= parameters.streaming_cell_count {
    return;
  }
  let cell: vec2<i32> = world_cell_from_gas_streaming_index(invocation.x);
  let index: u32 = gas_physical_cell_index_from_world_cell(cell);
  if index == INVALID_PHYSICAL_CELL_INDEX {
    return;
  }
  velocity[index] = vec2<f32>(0.0);
  for (var species: u32 = 0u; species < parameters.gas_count; species++) {
    concentrations[gas_concentration_storage_index_from_species_and_physical_cell(
      species,
      index)] =
      0.0;
  }
  gas_temperature[index] = parameters.ambient_temperature;
}

// Exports one fixed gas record and clears the same outgoing physical cell
@compute @workgroup_size(64)
fn export_gas_area(@builtin(global_invocation_id) invocation: vec3<u32>) {
  let output_index: u32 = invocation.x;
  if output_index >= parameters.streaming_cell_count {
    return;
  }
  let cell: vec2<i32> = world_cell_from_gas_streaming_index(output_index);
  let index: u32 = gas_physical_cell_index_from_world_cell(cell);
  let stride: u32 = 5u + parameters.gas_count;
  let start: u32 = output_index * stride;
  streaming_data[start] = bitcast<u32>(cell.x);
  streaming_data[start + 1u] = bitcast<u32>(cell.y);
  streaming_data[start + 2u] = bitcast<u32>(velocity[index].x);
  streaming_data[start + 3u] = bitcast<u32>(velocity[index].y);
  velocity[index] = vec2<f32>(0.0);
  for (var species: u32 = 0u; species < parameters.gas_count; species++) {
    let concentration: u32 =
      gas_concentration_storage_index_from_species_and_physical_cell(
        species,
        index);
    streaming_data[start + 4u] = bitcast<u32>(gas_temperature[index]);
    streaming_data[start + 5u + species] =
      bitcast<u32>(concentrations[concentration]);
    concentrations[concentration] = 0.0;
  }
}

// Sums density difference from the implicit ambient atmosphere
fn gas_density_offset_from_ambient(physical_cell_index: u32) -> f32 {
  var density_offset: f32 = 0.0;
  for (var species: u32 = 0u; species < parameters.gas_count; species++) {
    density_offset +=
      concentrations[gas_concentration_storage_index_from_species_and_physical_cell(
        species,
        physical_cell_index,)] * (gas_properties[species * 2u].x - parameters.ambient_density);
  }
  return density_offset;
}

// Calculates vorticity confinement from neighboring curl magnitudes
fn calculate_gas_vorticity_confinement(
  world_cell: vec2<i32>,
  physical_cell_index: u32,) -> vec2<f32> {
  let curl_gradient: vec2<f32> =
    0.5 * vec2<f32>(
      abs(gas_curl_at_world_cell(world_cell + vec2<i32>(1, 0))) - abs(
        gas_curl_at_world_cell(world_cell + vec2<i32>(-1, 0))),
      abs(gas_curl_at_world_cell(world_cell + vec2<i32>(0, 1))) - abs(
        gas_curl_at_world_cell(world_cell + vec2<i32>(0, -1))),);
  let gradient_length: f32 = length(curl_gradient);
  let normal: vec2<f32> =
    select(
      vec2<f32>(0.0),
      curl_gradient / max(gradient_length, 0.000001),
      gradient_length > 0.000001,);
  return
    parameters.vorticity_confinement
      * vec2<f32>(normal.y, -normal.x)
      * curl[physical_cell_index];
}

// Calculates one Jacobi gas-pressure value from the selected immutable field
fn calculate_gas_jacobi_pressure(
  world_cell: vec2<i32>,
  physical_cell_index: u32,
  read_pressure_a: bool,) -> f32 {
  let center: f32 =
    select(
      pressure_b[physical_cell_index],
      pressure_a[physical_cell_index],
      read_pressure_a,);
  return
    (gas_pressure_at_world_cell(
      world_cell + vec2<i32>(-1, 0),
      center,
      read_pressure_a)
      + gas_pressure_at_world_cell(
        world_cell + vec2<i32>(1, 0),
        center,
        read_pressure_a)
      + gas_pressure_at_world_cell(
        world_cell + vec2<i32>(0, -1),
        center,
        read_pressure_a)
      + gas_pressure_at_world_cell(
        world_cell + vec2<i32>(0, 1),
        center,
        read_pressure_a)
      - divergence[physical_cell_index]) * 0.25;
}

// Removes projected velocity components directed into neighboring obstacles
fn clamp_projected_gas_velocity_against_obstacle_faces(
  world_cell: vec2<i32>,
  projected_velocity: vec2<f32>,) -> vec2<f32> {
  var clamped_velocity: vec2<f32> = projected_velocity;
  if
    world_cell_is_gas_obstacle(
      world_cell + vec2<i32>(-1, 0)) && clamped_velocity.x < 0.0
  {
    clamped_velocity.x = 0.0;
  }
  if
    world_cell_is_gas_obstacle(
      world_cell + vec2<i32>(1, 0)) && clamped_velocity.x > 0.0
  {
    clamped_velocity.x = 0.0;
  }
  if
    world_cell_is_gas_obstacle(
      world_cell + vec2<i32>(0, -1)) && clamped_velocity.y < 0.0
  {
    clamped_velocity.y = 0.0;
  }
  if
    world_cell_is_gas_obstacle(
      world_cell + vec2<i32>(0, 1)) && clamped_velocity.y > 0.0
  {
    clamped_velocity.y = 0.0;
  }
  return clamped_velocity;
}

// Bilinearly samples the authoritative gas velocity field
fn sample_gas_velocity_bilinear_at_cell_position(
  cell_position: vec2<f32>) -> vec2<f32> {
  let shifted: vec2<f32> = cell_position - vec2<f32>(0.5);
  let base: vec2<i32> = vec2<i32>(floor(shifted));
  let fraction: vec2<f32> = fract(shifted);
  let bottom: vec2<f32> =
    mix(
      gas_velocity_at_world_cell(base),
      gas_velocity_at_world_cell(base + vec2<i32>(1, 0)),
      fraction.x,);
  let top: vec2<f32> =
    mix(
      gas_velocity_at_world_cell(base + vec2<i32>(0, 1)),
      gas_velocity_at_world_cell(base + vec2<i32>(1, 1)),
      fraction.x,);
  return mix(bottom, top, fraction.y);
}

// Calculates conservative gas flux across a cell's positive-X face
fn gas_concentration_flux_across_positive_x_face(
  species: u32,
  left: vec2<i32>) -> f32 {
  let right: vec2<i32> = left + vec2<i32>(1, 0);
  if world_cell_is_gas_obstacle(left) || world_cell_is_gas_obstacle(right) {
    return 0.0;
  }
  let face_velocity: f32 =
    0.5 * (gas_velocity_at_world_cell(left).x + gas_velocity_at_world_cell(
      right).x);
  let upstream: vec2<i32> = select(right, left, face_velocity >= 0.0);
  return
    face_velocity * concentrations[gas_concentration_storage_index_from_species_and_physical_cell(
      species,
      gas_physical_cell_index_from_world_cell(upstream),)];
}

// Calculates conservative gas flux across a cell's positive-Y face
fn gas_concentration_flux_across_positive_y_face(
  species: u32,
  bottom: vec2<i32>) -> f32 {
  let top: vec2<i32> = bottom + vec2<i32>(0, 1);
  if world_cell_is_gas_obstacle(bottom) || world_cell_is_gas_obstacle(top) {
    return 0.0;
  }
  let face_velocity: f32 =
    0.5 * (gas_velocity_at_world_cell(bottom).y + gas_velocity_at_world_cell(
      top).y);
  let upstream: vec2<i32> = select(top, bottom, face_velocity >= 0.0);
  return
    face_velocity * concentrations[gas_concentration_storage_index_from_species_and_physical_cell(
      species,
      gas_physical_cell_index_from_world_cell(upstream),)];
}

// Reads one gas concentration with obstacle and residency boundary handling
fn gas_concentration_at_world_cell(
  species: u32,
  cell: vec2<i32>,
  boundary: f32) -> f32 {
  if world_cell_is_gas_obstacle(cell) {
    return boundary;
  }
  let index: u32 = gas_physical_cell_index_from_world_cell(cell);
  if index == INVALID_PHYSICAL_CELL_INDEX {
    return boundary;
  }
  return
    concentrations[gas_concentration_storage_index_from_species_and_physical_cell(
      species,
      index)];
}

// Maps a species and physical cell to species-major concentration storage
fn gas_concentration_storage_index_from_species_and_physical_cell(
  species: u32,
  physical_cell_index: u32,) -> u32 {
  return species * parameters.buffered_cell_count + physical_cell_index;
}

// Reads authoritative gas velocity with a zero solid boundary
fn gas_velocity_at_world_cell(cell: vec2<i32>) -> vec2<f32> {
  if world_cell_is_gas_obstacle(cell) {
    return vec2<f32>(0.0);
  }
  let index: u32 = gas_physical_cell_index_from_world_cell(cell);
  if index == INVALID_PHYSICAL_CELL_INDEX {
    return vec2<f32>(0.0);
  }
  return velocity[index];
}

// Reads advected scratch velocity with a zero solid boundary
fn gas_scratch_velocity_at_world_cell(cell: vec2<i32>) -> vec2<f32> {
  if world_cell_is_gas_obstacle(cell) {
    return vec2<f32>(0.0);
  }
  let index: u32 = gas_physical_cell_index_from_world_cell(cell);
  if index == INVALID_PHYSICAL_CELL_INDEX {
    return vec2<f32>(0.0);
  }
  return velocity_scratch[index];
}

// Reads scalar gas curl with a zero solid boundary
fn gas_curl_at_world_cell(cell: vec2<i32>) -> f32 {
  if world_cell_is_gas_obstacle(cell) {
    return 0.0;
  }
  let index: u32 = gas_physical_cell_index_from_world_cell(cell);
  if index == INVALID_PHYSICAL_CELL_INDEX {
    return 0.0;
  }
  return curl[index];
}

// Reads the selected gas pressure field with a Neumann solid boundary
fn gas_pressure_at_world_cell(
  cell: vec2<i32>,
  boundary: f32,
  read_pressure_a: bool,) -> f32 {
  if world_cell_is_gas_obstacle(cell) {
    return boundary;
  }
  let physical_index: u32 = gas_physical_cell_index_from_world_cell(cell);
  return
    select(
      pressure_b[physical_index],
      pressure_a[physical_index],
      read_pressure_a);
}

// Classifies solid cellular, external-body, fluid, and nonresident gas cells
fn world_cell_is_gas_obstacle(cell: vec2<i32>) -> bool {
  let index: u32 = gas_physical_cell_index_from_world_cell(cell);
  return
    index == INVALID_PHYSICAL_CELL_INDEX
      || cellular_material_identifiers[index] != EMPTY_MATERIAL_IDENTIFIER
      || external_body_occupancy[index] != 0u
      || fluid_coverage[index] >= parameters.fluid_obstacle_coverage;
}

// Maps a world cell through the gas solver's current physical tile ring
fn gas_physical_cell_index_from_world_cell(world_cell: vec2<i32>) -> u32 {
  return
    physical_cell_index_from_world_cell(
      world_cell,
      parameters.buffered_origin,
      parameters.buffered_tile_size,
      parameters.ring_offset,);
}

// Converts gas dispatch order into a signed buffered world cell
fn world_cell_from_gas_logical_index(logical_index: u32) -> vec2<i32> {
  return
    world_cell_from_logical_tile_major_index(
      logical_index,
      parameters.buffered_origin,
      parameters.buffered_tile_size,);
}

// Converts row-major gas streaming order into a signed world cell
fn world_cell_from_gas_streaming_index(streaming_index: u32) -> vec2<i32> {
  let width: u32 = parameters.streaming_tile_size.x * CELLS_PER_TILE;
  return
    parameters.streaming_origin * i32(CELLS_PER_TILE) + vec2<i32>(
      i32(streaming_index % width),
      i32(streaming_index / width),);
}
