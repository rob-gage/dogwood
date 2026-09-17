fn export_fluid_particles(
  @builtin(global_invocation_id) invocation: vec3<u32>) {
  let particle_index: u32 = invocation.x;
  if
    particle_index >= parameters.particle_capacity || particles[particle_index].material_identifier == EMPTY_MATERIAL_IDENTIFIER
  {
    return;
  }
  let tile: vec2<i32> = vec2<i32>(floor(particles[particle_index].position));
  let relative: vec2<i32> = tile - parameters.streaming_origin;
  if
    any(relative < vec2<i32>(0))
      || relative.x >= i32(parameters.streaming_tile_size.x)
      || relative.y >= i32(parameters.streaming_tile_size.y)
  {
    return;
  }
  let output_index: u32 = atomicAdd(&streaming_count[0], 1u);
  if output_index >= arrayLength(&streaming_particles) {
    return;
  }
  streaming_particles[output_index] = particles[particle_index];
  release_fluid_particle_index(particle_index);
}

// Reclaims authoritative Accelerator slots and records each claim result for asynchronous ownership transfer
@compute @workgroup_size(64)
fn import_fluid_particles(
  @builtin(global_invocation_id) invocation: vec3<u32>) {
  let input_index: u32 = invocation.x;
  if input_index >= atomicLoad(&streaming_count[0]) {
    return;
  }
  let particle_index: u32 = claim_free_fluid_particle_index();
  if particle_index == INVALID_FLUID_PARTICLE_INDEX {
    streaming_results[input_index] = 0u;
    return;
  }
  particles[particle_index] = streaming_particles[input_index];
  streaming_results[input_index] = 1u;
}

// Each cell gathers its own result, so particles never contend for derived-cell writes
@compute @workgroup_size(64)
fn rasterize_fluid_particle_coverage_into_cells(
  @builtin(global_invocation_id) invocation: vec3<u32>) {
  let logical_index: u32 = invocation.x;
  if logical_index >= parameters.buffered_cell_count {
    return;
  }
  let cell: vec2<i32> = world_cell_from_fluid_logical_index(logical_index);
  let physical_index: u32 = fluid_physical_cell_index_from_world_cell(cell);
  let center: vec2<f32> = vec2<f32>(cell) + vec2<f32>(0.5);
  let sample: DerivedFluidCellSample =
    gather_fluid_particle_sample_for_cell(center);
  derived_material_identifiers[physical_index] = sample.material_identifier;
  derived_coverage[physical_index] = sample.coverage;
  derived_velocity[physical_index] = vec4<f32>(sample.velocity, 0.0, 0.0);
  mechanical_cells[physical_index] =
    MechanicalFluidCell(
      sample.mechanical_material_identifier,
      sample.mechanical_mass,
      sample.mechanical_velocity,);
  mechanical_original_velocity[physical_index] = sample.mechanical_velocity;
  derived_thermal[physical_index] = sample.thermal;
}

// Each authoritative particle retains its center cell's solved velocity delta.
@compute @workgroup_size(64)
fn scatter_fluid_mechanical_response(
  @builtin(global_invocation_id) invocation: vec3<u32>) {
  let particle_index: u32 = invocation.x;
  if
    particle_index >= parameters.particle_capacity
      || particles[particle_index].material_identifier == EMPTY_MATERIAL_IDENTIFIER
      || particles[particle_index].is_active == 0u
  {
    return;
  }
  let cell: vec2<i32> =
    vec2<i32>(floor(particles[particle_index].position * CELLS_PER_TILE_FLOAT,));
  let index: u32 = fluid_physical_cell_index_from_world_cell(cell);
  if
    index == INVALID_PHYSICAL_CELL_INDEX || mechanical_cells[index].mass <= 0.0

  {
    return;
  }
  particles[particle_index].velocity +=
    mechanical_cells[index].velocity - mechanical_original_velocity[index];
}

// Gathers nearby authoritative particles for one derived cellular sample
fn gather_fluid_particle_sample_for_cell(
  center: vec2<f32>) -> DerivedFluidCellSample {
  let base_bucket_coordinates: vec2<i32> =
    fluid_spatial_bucket_coordinates_from_position(
      center / CELLS_PER_TILE_FLOAT,);
  var weight_sum: f32 = 0.0;
  var velocity_sum: vec2<f32> = vec2<f32>(0.0);
  var strongest_weight: f32 = 0.0;
  var material_identifier: u32 = EMPTY_MATERIAL_IDENTIFIER;
  var mechanical_material_identifier: u32 = EMPTY_MATERIAL_IDENTIFIER;
  var mechanical_mass: f32 = 0.0;
  var mechanical_momentum: vec2<f32> = vec2<f32>(0.0);
  var thermal_capacity: f32 = 0.0;
  var thermal_energy: f32 = 0.0;
  var conductivity_weighted: f32 = 0.0;
  var amount_sum: f32 = 0.0;
  for (var bucket_y: i32 = -1; bucket_y <= 1; bucket_y++) {
    for (var bucket_x: i32 = -1; bucket_x <= 1; bucket_x++) {
      let bucket_index: u32 =
        fluid_spatial_bucket_index_from_coordinates(
          base_bucket_coordinates + vec2<i32>(bucket_x, bucket_y),);
      if bucket_index == INVALID_FLUID_BUCKET_INDEX {
        continue;
      }
      var particle_index: u32 = atomicLoad(&bucket_heads[bucket_index]);
      for (
        var chain_length: u32 = 0u;
        particle_index != INVALID_FLUID_PARTICLE_INDEX && chain_length < parameters.particle_capacity;
        chain_length++
      ) {
        let particle: Particle = particles[particle_index];
        if
          particle.material_identifier != EMPTY_MATERIAL_IDENTIFIER
            && particle.is_active != 0u
            && fluid_particle_belongs_to_cell(
              particle.position,
              vec2<i32>(floor(center)),
              CELLS_PER_TILE_FLOAT)
        {
          let particle_mass: f32 =
            fluid_physical_properties_from_identifier(
              particle.material_identifier).x;
          mechanical_mass += particle_mass;
          mechanical_momentum += particle.velocity * particle_mass;
          mechanical_material_identifier = particle.material_identifier;
          let dense =
            material_dense_index(
              particle.material_identifier,
              thermal_material_parameters.offsets,
              thermal_material_parameters.counts);
          if
            dense != 0xffffffffu && dense < arrayLength(
              &thermal_material_properties)
          {
            let record = thermal_material_properties[dense];
            let amount = max(particle.amount, 0.0);
            let capacity =
              amount * thermal_material_specific_heat_capacity(record);
            thermal_capacity += capacity;
            thermal_energy += capacity * particle.temperature;
            conductivity_weighted +=
              amount * thermal_material_conductivity(record);
            amount_sum += amount;
          }
        }
        let distance_cells: f32 =
          length(particle.position * CELLS_PER_TILE_FLOAT - center,);
        // Support radius is for particle-based fluid simulation; one particle renders as about one cell
        let particle_diameter: f32 = 1.2;
        let weight: f32 = max(0.0, 1.0 - distance_cells / particle_diameter);
        if weight > 0.0 {
          weight_sum += weight;
          velocity_sum += particle.velocity * weight;
          if weight > strongest_weight {
            strongest_weight = weight;
            material_identifier = particle.material_identifier;
          }
        }
        particle_index = next_particle[particle_index];
      }
    }
  }
  let average_conductivity =
    select(0.0, conductivity_weighted / amount_sum, amount_sum > 0.000001);
  return
    DerivedFluidCellSample(
      material_identifier,
      min(weight_sum, 1.0),
      select(
        vec2<f32>(0.0),
        velocity_sum / max(weight_sum, 0.000001),
        weight_sum > 0.0,),
      mechanical_material_identifier,
      mechanical_mass,
      select(
        vec2<f32>(0.0),
        mechanical_momentum / max(mechanical_mass, 0.000001),
        mechanical_mass > 0.0),
      vec4<f32>(
        thermal_capacity,
        thermal_energy,
        clamp(weight_sum, 0.0, 1.0) * average_conductivity,
        0.0),);
}

// Reduces the possessed pawn capsule against the final derived fluid representation
@compute @workgroup_size(1)
fn sample_fluid_state_inside_pawn_capsule(
  @builtin(global_invocation_id) invocation: vec3<u32>) {
  if invocation.x != 0u {
    return;
  }
  let shape: ActorShape =
    actor_shape_from_parameters(
      parameters.sample_center,
      parameters.gravity,
      parameters.sample_shape_parameters,
      parameters.sample_shape_kind);
  let extent: vec2<f32> = actor_shape_world_extent(shape);
  let minimum: vec2<i32> =
    vec2<i32>(
      floor((parameters.sample_center - extent) * CELLS_PER_TILE_FLOAT,));
  let maximum: vec2<i32> =
    vec2<i32>(
      floor((parameters.sample_center + extent) * CELLS_PER_TILE_FLOAT,));
  let sample: DerivedFluidActorSample =
    gather_derived_fluid_sample_inside_actor(shape, minimum, maximum,);
  let divisor: f32 = max(sample.coverage_sum, 0.000001);
  sample_output[0] =
    vec4<f32>(
      sample.coverage_sum / max(sample.capsule_cell_count, 1.0),
      sample.velocity_sum.x / divisor,
      sample.velocity_sum.y / divisor,
      sample.density_sum / divisor,);
  sample_output[1] = vec4<f32>(sample.viscosity_sum / divisor, 0.0, 0.0, 0.0);
}

// Gathers derived coverage and physical properties beneath one pawn capsule
fn gather_derived_fluid_sample_inside_actor(
  shape: ActorShape,
  minimum: vec2<i32>,
  maximum: vec2<i32>,) -> DerivedFluidActorSample {
  var capsule_cell_count: f32 = 0.0;
  var coverage_sum: f32 = 0.0;
  var velocity_sum: vec2<f32> = vec2<f32>(0.0);
  var density_sum: f32 = 0.0;
  var viscosity_sum: f32 = 0.0;
  for (var y: i32 = minimum.y; y <= maximum.y; y++) {
    for (var x: i32 = minimum.x; x <= maximum.x; x++) {
      let cell: vec2<i32> = vec2<i32>(x, y);
      let world_position: vec2<f32> =
        (vec2<f32>(cell) + vec2<f32>(0.5)) / CELLS_PER_TILE_FLOAT;
      if !world_position_is_inside_actor_shape(world_position, shape) {
        continue;
      }
      capsule_cell_count += 1.0;
      let index: u32 = fluid_physical_cell_index_from_world_cell(cell);
      if
        index == INVALID_PHYSICAL_CELL_INDEX || derived_material_identifiers[index] == EMPTY_MATERIAL_IDENTIFIER
      {
        continue;
      }
      let coverage: f32 = clamp(derived_coverage[index], 0.0, 1.0);
      let properties: vec2<f32> =
        fluid_physical_properties_from_identifier(
          derived_material_identifiers[index],);
      coverage_sum += coverage;
      velocity_sum += derived_velocity[index].xy * coverage;
      density_sum += properties.x * coverage;
      viscosity_sum += properties.y * coverage;
    }
  }
  return
    DerivedFluidActorSample(
      capsule_cell_count,
      coverage_sum,
      velocity_sum,
      density_sum,
      viscosity_sum,);
}

// Calculates one neighbor-velocity-smoothing correction from committed neighbors
fn calculate_fluid_particle_velocity_smoothing(
  particle_index: u32,
  particle: Particle,) -> vec2<f32> {
  let base_bucket_coordinates: vec2<i32> =
    fluid_spatial_bucket_coordinates_from_position(particle.position);
  var difference_sum: vec2<f32> = vec2<f32>(0.0);
  var weight_sum: f32 = 0.0;
  for (var bucket_y: i32 = -1; bucket_y <= 1; bucket_y++) {
    for (var bucket_x: i32 = -1; bucket_x <= 1; bucket_x++) {
      let bucket_index: u32 =
        fluid_spatial_bucket_index_from_coordinates(
          base_bucket_coordinates + vec2<i32>(bucket_x, bucket_y),);
      if bucket_index == INVALID_FLUID_BUCKET_INDEX {
        continue;
      }
      var neighbor_index: u32 = atomicLoad(&bucket_heads[bucket_index]);
      for (
        var chain_length: u32 = 0u;
        neighbor_index != INVALID_FLUID_PARTICLE_INDEX && chain_length < parameters.particle_capacity;
        chain_length++
      ) {
        if neighbor_index != particle_index {
          let distance: f32 =
            length(
              (particle.position - particles[neighbor_index].position) * CELLS_PER_TILE_FLOAT,);
          let weight: f32 = fluid_poly6_kernel_weight(distance);
          difference_sum +=
            (particles[neighbor_index].velocity - particle.velocity) * weight;
          weight_sum += weight;
        }
        neighbor_index = next_particle[neighbor_index];
      }
    }
  }
  return
    fluid_constraint_properties_from_identifier(particle.material_identifier).z
      * difference_sum
      / max(weight_sum, 0.000001);
}

// Calculates one particle-fluid density constraint multiplier from predicted neighbors
fn calculate_fluid_density_constraint_lambda(
  particle_index: u32,
  position: vec2<f32>,
  rest_density: f32,) -> f32 {
  let base_bucket_coordinates: vec2<i32> =
    fluid_spatial_bucket_coordinates_from_position(position);
  var density: f32 = fluid_poly6_kernel_weight(0.0);
  var self_gradient: vec2<f32> = vec2<f32>(0.0);
  var gradient_squared_sum: f32 = 0.0;
  for (var bucket_y: i32 = -1; bucket_y <= 1; bucket_y++) {
    for (var bucket_x: i32 = -1; bucket_x <= 1; bucket_x++) {
      let bucket_index: u32 =
        fluid_spatial_bucket_index_from_coordinates(
          base_bucket_coordinates + vec2<i32>(bucket_x, bucket_y),);
      if bucket_index == INVALID_FLUID_BUCKET_INDEX {
        continue;
      }
      var neighbor_index: u32 = atomicLoad(&bucket_heads[bucket_index]);
      for (
        var chain_length: u32 = 0u;
        neighbor_index != INVALID_FLUID_PARTICLE_INDEX && chain_length < parameters.particle_capacity;
        chain_length++
      ) {
        if neighbor_index != particle_index {
          let separation: vec2<f32> =
            (position - predicted_positions[neighbor_index]) * CELLS_PER_TILE_FLOAT;
          let distance: f32 = length(separation);
          if distance < parameters.support_radius_cells {
            density += fluid_poly6_kernel_weight(distance);
            let gradient: vec2<f32> =
              fluid_spiky_kernel_gradient(separation, distance,) / rest_density;
            self_gradient += gradient;
            gradient_squared_sum += dot(gradient, gradient);
          }
        }
        neighbor_index = next_particle[neighbor_index];
      }
    }
  }
  gradient_squared_sum += dot(self_gradient, self_gradient);
  let constraint: f32 = density / rest_density - 1.0;
  return -constraint / (gradient_squared_sum + CONSTRAINT_EPSILON);
}

// Gathers one race-free particle-fluid position correction from immutable neighbor state
fn calculate_fluid_particle_position_correction_cells(
  particle_index: u32,
  position: vec2<f32>,) -> vec2<f32> {
  let material_identifier: u32 = particles[particle_index].material_identifier;
  let rest_density: f32 =
    fluid_constraint_properties_from_identifier(material_identifier).x;
  let base_bucket_coordinates: vec2<i32> =
    fluid_spatial_bucket_coordinates_from_position(position);
  var correction_cells: vec2<f32> = vec2<f32>(0.0);
  for (var bucket_y: i32 = -1; bucket_y <= 1; bucket_y++) {
    for (var bucket_x: i32 = -1; bucket_x <= 1; bucket_x++) {
      let bucket_index: u32 =
        fluid_spatial_bucket_index_from_coordinates(
          base_bucket_coordinates + vec2<i32>(bucket_x, bucket_y),);
      if bucket_index == INVALID_FLUID_BUCKET_INDEX {
        continue;
      }
      var neighbor_index: u32 = atomicLoad(&bucket_heads[bucket_index]);
      for (
        var chain_length: u32 = 0u;
        neighbor_index != INVALID_FLUID_PARTICLE_INDEX && chain_length < parameters.particle_capacity;
        chain_length++
      ) {
        if neighbor_index != particle_index {
          let separation: vec2<f32> =
            (position - predicted_positions[neighbor_index]) * CELLS_PER_TILE_FLOAT;
          let distance: f32 = length(separation);
          if distance > 0.000001 && distance < parameters.support_radius_cells {
            correction_cells +=
              (lambdas[particle_index]
                + lambdas[neighbor_index]
                + calculate_fluid_artificial_pressure(
                  distance,
                  material_identifier))
                * fluid_spiky_kernel_gradient(separation, distance)
                / rest_density;
          }
        }
        neighbor_index = next_particle[neighbor_index];
      }
    }
  }
  return correction_cells;
}

// Evaluates the two-dimensional particle-fluid poly6 density kernel
fn fluid_poly6_kernel_weight(distance: f32) -> f32 {
  let h: f32 = parameters.support_radius_cells;
  if distance >= h {
    return 0.0;
  }
  let difference: f32 = h * h - distance * distance;
  return
    4.0
      * difference
      * difference
      * difference
      / (PI * h * h * h * h * h * h * h * h);
}

// Evaluates the two-dimensional particle-fluid spiky-kernel gradient
fn fluid_spiky_kernel_gradient(separation: vec2<f32>, distance: f32) -> vec2<
  f32
> {
  let h: f32 = parameters.support_radius_cells;
  if distance <= 0.000001 || distance >= h {
    return vec2<f32>(0.0);
  }
  let remaining: f32 = h - distance;
  return
    -30.0
      * remaining
      * remaining
      / (PI * h * h * h * h * h)
      * separation
      / distance;
}

// Calculates tensile-instability correction for one neighboring particle
fn calculate_fluid_artificial_pressure(
  distance: f32,
  material_identifier: u32) -> f32 {
  let reference: f32 =
    fluid_poly6_kernel_weight(
      ARTIFICIAL_PRESSURE_DELTA_Q_RATIO * parameters.support_radius_cells,);
  let ratio: f32 =
    fluid_poly6_kernel_weight(distance) / max(reference, 0.000001);
  let squared: f32 = ratio * ratio;
  return
    -fluid_constraint_properties_from_identifier(material_identifier).y
      * squared
      * squared;
}

// Reads density, artificial pressure, smoothing, and body-push properties
fn fluid_constraint_properties_from_identifier(
  material_identifier: u32) -> vec4<f32> {
  return
    fluid_material_properties[material_index_from_identifier(
      material_identifier) * 2u];
}

// Reads density and viscosity for derived fluid interaction
fn fluid_physical_properties_from_identifier(material_identifier: u32) -> vec2<
  f32
> {
  return
    fluid_material_properties[material_index_from_identifier(
      material_identifier) * 2u + 1u].zw;
}

// Projects a fluid particle out of cellular or hard-body geometry
fn project_fluid_particle_out_of_cellular_collision(
  initial_position: vec2<f32>) -> vec2<f32> {
  var position: vec2<f32> = initial_position;
  let radius: f32 = parameters.particle_radius_cells / CELLS_PER_TILE_FLOAT;
  for (var iteration: u32 = 0u; iteration < 2u; iteration++) {
    let center_cell: vec2<i32> =
      vec2<i32>(floor(position * CELLS_PER_TILE_FLOAT));
    var resolved: bool = false;
    for (var offset_y: i32 = -1; offset_y <= 1 && !resolved; offset_y++) {
      for (var offset_x: i32 = -1; offset_x <= 1; offset_x++) {
        let cell: vec2<i32> = center_cell + vec2<i32>(offset_x, offset_y);
        let index: u32 = fluid_physical_cell_index_from_world_cell(cell);
        if
          index == INVALID_PHYSICAL_CELL_INDEX || (cellular_material_identifiers[index] == EMPTY_MATERIAL_IDENTIFIER && !is_hard_external_body(
            external_body_occupancy[index]))
        {
          continue;
        }
        let minimum: vec2<f32> = vec2<f32>(cell) / CELLS_PER_TILE_FLOAT;
        let maximum: vec2<f32> =
          vec2<f32>(cell + vec2<i32>(1)) / CELLS_PER_TILE_FLOAT;
        let nearest: vec2<f32> = clamp(position, minimum, maximum);
        let delta: vec2<f32> = position - nearest;
        let distance: f32 = length(delta);
        if distance >= radius {
          continue;
        }
        var normal: vec2<f32>;
        var penetration: f32;
        if distance > 0.000001 {
          normal = delta / distance;
          penetration = radius - distance;
        } else {
          let distances: vec4<f32> =
            vec4<f32>(
              position.x - minimum.x,
              maximum.x - position.x,
              position.y - minimum.y,
              maximum.y - position.y,);
          let side: f32 =
            min(min(distances.x, distances.y), min(distances.z, distances.w));
          normal =
            select(
              select(
                vec2<f32>(-1.0, 0.0),
                vec2<f32>(1.0, 0.0),
                side == distances.y),
              select(
                vec2<f32>(0.0, -1.0),
                vec2<f32>(0.0, 1.0),
                side == distances.w),
              side == distances.z || side == distances.w);
          penetration = radius + side;
        }
        position += normal * penetration;
        resolved = true;
        break;
      }
    }
    if !resolved {
      break;
    }
  }
  return position;
}

// Atomically pops one reusable authoritative fluid-particle slot
fn claim_free_fluid_particle_index() -> u32 {
  var available: u32 = atomicLoad(&free_count[0]);
  loop {
    if available == 0u || available > arrayLength(&free_indices) {
      return INVALID_FLUID_PARTICLE_INDEX;
    }
    let result =
      atomicCompareExchangeWeak(&free_count[0], available, available - 1u);
    if result.exchanged {
      let particle_index = free_indices[available - 1u];
      if particle_index >= arrayLength(&particles) {
        return INVALID_FLUID_PARTICLE_INDEX;
      }
      return particle_index;
    }
    available = result.old_value;
  }
  return INVALID_FLUID_PARTICLE_INDEX;
}

// Atomically returns one authoritative fluid-particle slot to the free stack.
// Release and claim operations are in distinct ordered passes.
fn release_fluid_particle_index(particle_index: u32) {
  if particle_index >= arrayLength(&particles) {
    return;
  }
  var available: u32 = atomicLoad(&free_count[0]);
  loop {
    if available >= arrayLength(&free_indices) {
      return;
    }
    let result =
      atomicCompareExchangeWeak(&free_count[0], available, available + 1u);
    if result.exchanged {
      particles[particle_index].material_identifier = EMPTY_MATERIAL_IDENTIFIER;
      free_indices[available] = particle_index;
      return;
    }
    available = result.old_value;
  }
}

// Tests continuous tile-space position against buffered residency
fn fluid_position_is_inside_buffered_region(position: vec2<f32>) -> bool {
  let relative: vec2<f32> = position - vec2<f32>(parameters.buffered_origin);
  return
    all(relative >= vec2<f32>(0.0))
      && relative.x < f32(parameters.buffered_tile_size.x)
      && relative.y < f32(parameters.buffered_tile_size.y);
}

// Tests continuous tile-space position against active simulation bounds
fn fluid_position_is_inside_active_region(position: vec2<f32>) -> bool {
  let relative: vec2<f32> = position - vec2<f32>(parameters.active_origin);
  return
    all(relative >= vec2<f32>(0.0))
      && relative.x < f32(parameters.active_tile_size.x)
      && relative.y < f32(parameters.active_tile_size.y);
}

// Includes neighbors needed by active particles and their lambda neighbors
fn fluid_position_supports_active_neighbor_buckets(
  position: vec2<f32>) -> bool {
  let predicted_movement_cells: f32 =
    f32(
      parameters.maximum_movement_cells) / PBF_SUBSTEP_COUNT + MAXIMUM_CORRECTION_CELLS * PBF_CONSTRAINT_ITERATION_COUNT;
  return
    fluid_position_is_inside_active_padding(
      (parameters.support_radius_cells * 2.0 + predicted_movement_cells) / CELLS_PER_TILE_FLOAT,
      position,);
}

// Includes particles whose lambdas directly support the active fluid region
fn fluid_position_supports_active_lambdas(position: vec2<f32>) -> bool {
  let predicted_movement_cells: f32 =
    f32(
      parameters.maximum_movement_cells) / PBF_SUBSTEP_COUNT + MAXIMUM_CORRECTION_CELLS * PBF_CONSTRAINT_ITERATION_COUNT;
  return
    fluid_position_is_inside_active_padding(
      (parameters.support_radius_cells + predicted_movement_cells) / CELLS_PER_TILE_FLOAT,
      position,);
}

// Tests continuous tile-space position against padded active bounds
fn fluid_position_is_inside_active_padding(
  padding: f32,
  position: vec2<f32>) -> bool {
  let minimum: vec2<f32> =
    vec2<f32>(parameters.active_origin) - vec2<f32>(padding);
  let maximum: vec2<f32> =
    vec2<f32>(parameters.active_origin)
      + vec2<f32>(parameters.active_tile_size)
      + vec2<f32>(padding);
  return all(position >= minimum) && all(position < maximum);
}

// Converts continuous tile-space position into fluid spatial-bucket coordinates
fn fluid_spatial_bucket_coordinates_from_position(position: vec2<f32>) -> vec2<
  i32
> {
  return
    fluid_bucket_coordinates_from_position(
      position,
      parameters.buffered_origin,
      parameters.support_radius_cells,
      CELLS_PER_TILE_FLOAT,);
}

// Converts continuous tile-space position into a fluid spatial-bucket index
fn fluid_spatial_bucket_index_from_position(position: vec2<f32>) -> u32 {
  return
    fluid_spatial_bucket_index_from_coordinates(
      fluid_spatial_bucket_coordinates_from_position(position),);
}

// Converts bounded fluid bucket coordinates into row-major storage
fn fluid_spatial_bucket_index_from_coordinates(
  bucket_coordinates: vec2<i32>) -> u32 {
  return
    fluid_bucket_index_from_coordinates(
      bucket_coordinates,
      parameters.bucket_dimensions,
      INVALID_FLUID_BUCKET_INDEX,);
}

// Maps a world cell through the fluid solver's current physical tile ring
fn fluid_physical_cell_index_from_world_cell(world_cell: vec2<i32>) -> u32 {
  return
    physical_cell_index_from_world_cell(
      world_cell,
      parameters.buffered_origin,
      parameters.buffered_tile_size,
      parameters.ring_offset,);
}

// Converts fluid cell dispatch order into a signed buffered world cell
fn world_cell_from_fluid_logical_index(logical_index: u32) -> vec2<i32> {
  return
    world_cell_from_logical_tile_major_index(
      logical_index,
      parameters.buffered_origin,
      parameters.buffered_tile_size,);
}
