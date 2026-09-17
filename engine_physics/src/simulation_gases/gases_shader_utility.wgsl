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

