// Gathers nearby authoritative particles for one derived cellular sample
fn gather_fluid_particle_sample_for_cell(center: vec2<f32>) -> DerivedFluidCellSample {
    let base_bucket_coordinates: vec2<i32> =
        fluid_spatial_bucket_coordinates_from_position(center / CELLS_PER_TILE_FLOAT);
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
                    base_bucket_coordinates + vec2<i32>(bucket_x, bucket_y),
                );
            if bucket_index == INVALID_FLUID_BUCKET_INDEX {
                continue;
            }
            var particle_index: u32 = atomicLoad(&bucket_heads[bucket_index]);
            for (
                var chain_length: u32 = 0u;
                particle_index != INVALID_FLUID_PARTICLE_INDEX && chain_length < fluid_simulation_parameters.particle_capacity;
                chain_length++
            ) {
                let particle: Particle = particles[particle_index];
                if
                    particle.material_identifier != EMPTY_MATERIAL_IDENTIFIER
                        && particle.is_active != 0u
                        && fluid_particle_belongs_to_cell(
                            particle.position,
                            vec2<i32>(floor(center)),
                            CELLS_PER_TILE_FLOAT,
                        )
                {
                    let particle_mass: f32 =
                        fluid_physical_properties_from_identifier(particle.material_identifier).x;
                    mechanical_mass += particle_mass;
                    mechanical_momentum += particle.velocity * particle_mass;
                    mechanical_material_identifier = particle.material_identifier;
                    let dense =
                        material_dense_index(
                            particle.material_identifier,
                            thermal_material_parameters.offsets,
                            thermal_material_parameters.counts,
                        );
                    if dense != 0xffffffffu && dense < arrayLength(&thermal_material_properties) {
                        let record = thermal_material_properties[dense];
                        let amount = max(particle.amount, 0.0);
                        let capacity = amount * thermal_material_specific_heat_capacity(record);
                        thermal_capacity += capacity;
                        thermal_energy += capacity * particle.temperature;
                        conductivity_weighted += amount * thermal_material_conductivity(record);
                        amount_sum += amount;
                    }
                }
                let distance_cells: f32 = length(particle.position * CELLS_PER_TILE_FLOAT - center);
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
            select(vec2<f32>(0.0), velocity_sum / max(weight_sum, 0.000001), weight_sum > 0.0),
            mechanical_material_identifier,
            mechanical_mass,
            select(
                vec2<f32>(0.0),
                mechanical_momentum / max(mechanical_mass, 0.000001),
                mechanical_mass > 0.0,
            ),
            vec4<f32>(
                thermal_capacity,
                thermal_energy,
                clamp(weight_sum, 0.0, 1.0) * average_conductivity,
                0.0,
            ),
        );
}

// Reduces the possessed pawn capsule against the final derived fluid representation
@compute @workgroup_size(1)
fn sample_fluid_state_inside_pawn_capsule(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if invocation.x != 0u {
        return;
    }
    let shape: ActorShape =
        actor_shape_from_parameters(
            fluid_simulation_parameters.sample_center,
            fluid_simulation_parameters.gravity,
            fluid_simulation_parameters.sample_shape_parameters,
            fluid_simulation_parameters.sample_shape_kind,
        );
    let extent: vec2<f32> = actor_shape_world_extent(shape);
    let minimum: vec2<i32> =
        vec2<i32>(floor((fluid_simulation_parameters.sample_center - extent) * CELLS_PER_TILE_FLOAT));
    let maximum: vec2<i32> =
        vec2<i32>(floor((fluid_simulation_parameters.sample_center + extent) * CELLS_PER_TILE_FLOAT));
    let sample: DerivedFluidActorSample =
        gather_derived_fluid_sample_inside_actor(shape, minimum, maximum);
    let divisor: f32 = max(sample.coverage_sum, 0.000001);
    sample_output[0] =
        vec4<f32>(
            sample.coverage_sum / max(sample.capsule_cell_count, 1.0),
            sample.velocity_sum.x / divisor,
            sample.velocity_sum.y / divisor,
            sample.density_sum / divisor,
        );
    sample_output[1] = vec4<f32>(sample.viscosity_sum / divisor, 0.0, 0.0, 0.0);
}

// Gathers derived coverage and physical properties beneath one pawn capsule
fn gather_derived_fluid_sample_inside_actor(
    shape: ActorShape,
    minimum: vec2<i32>,
    maximum: vec2<i32>,
) -> DerivedFluidActorSample {
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
            let fluid_physical_cell_index: u32 = fluid_physical_cell_index_from_world_cell(cell);
            if
                fluid_physical_cell_index == INVALID_PHYSICAL_CELL_INDEX || derived_material_identifiers[fluid_physical_cell_index] == EMPTY_MATERIAL_IDENTIFIER
            {
                continue;
            }
            let coverage: f32 = clamp(derived_coverage[fluid_physical_cell_index], 0.0, 1.0);
            let properties: vec2<f32> =
                fluid_physical_properties_from_identifier(derived_material_identifiers[fluid_physical_cell_index]);
            coverage_sum += coverage;
            velocity_sum += derived_velocity[fluid_physical_cell_index].xy * coverage;
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
            viscosity_sum,
        );
}

// Calculates one neighbor-velocity-smoothing correction from committed neighbors
fn calculate_fluid_particle_velocity_smoothing(particle_index: u32, particle: Particle) -> vec2<
    f32,
> {
    let base_bucket_coordinates: vec2<i32> =
        fluid_spatial_bucket_coordinates_from_position(particle.position);
    var difference_sum: vec2<f32> = vec2<f32>(0.0);
    var weight_sum: f32 = 0.0;
    for (var bucket_y: i32 = -1; bucket_y <= 1; bucket_y++) {
        for (var bucket_x: i32 = -1; bucket_x <= 1; bucket_x++) {
            let bucket_index: u32 =
                fluid_spatial_bucket_index_from_coordinates(
                    base_bucket_coordinates + vec2<i32>(bucket_x, bucket_y),
                );
            if bucket_index == INVALID_FLUID_BUCKET_INDEX {
                continue;
            }
            var neighbor_index: u32 = atomicLoad(&bucket_heads[bucket_index]);
            for (
                var chain_length: u32 = 0u;
                neighbor_index != INVALID_FLUID_PARTICLE_INDEX && chain_length < fluid_simulation_parameters.particle_capacity;
                chain_length++
            ) {
                if neighbor_index != particle_index {
                    let distance: f32 =
                        length(
                            (particle.position - particles[neighbor_index].position) * CELLS_PER_TILE_FLOAT,
                        );
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
    rest_density: f32,
) -> f32 {
    let base_bucket_coordinates: vec2<i32> =
        fluid_spatial_bucket_coordinates_from_position(position);
    var density: f32 = fluid_poly6_kernel_weight(0.0);
    var self_gradient: vec2<f32> = vec2<f32>(0.0);
    var gradient_squared_sum: f32 = 0.0;
    for (var bucket_y: i32 = -1; bucket_y <= 1; bucket_y++) {
        for (var bucket_x: i32 = -1; bucket_x <= 1; bucket_x++) {
            let bucket_index: u32 =
                fluid_spatial_bucket_index_from_coordinates(
                    base_bucket_coordinates + vec2<i32>(bucket_x, bucket_y),
                );
            if bucket_index == INVALID_FLUID_BUCKET_INDEX {
                continue;
            }
            var neighbor_index: u32 = atomicLoad(&bucket_heads[bucket_index]);
            for (
                var chain_length: u32 = 0u;
                neighbor_index != INVALID_FLUID_PARTICLE_INDEX && chain_length < fluid_simulation_parameters.particle_capacity;
                chain_length++
            ) {
                if neighbor_index != particle_index {
                    let separation: vec2<f32> =
                        (position - predicted_positions[neighbor_index]) * CELLS_PER_TILE_FLOAT;
                    let distance: f32 = length(separation);
                    if distance < fluid_simulation_parameters.support_radius_cells {
                        density += fluid_poly6_kernel_weight(distance);
                        let gradient: vec2<f32> =
                            fluid_spiky_kernel_gradient(separation, distance) / rest_density;
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
    position: vec2<f32>,
) -> vec2<f32> {
    let material_identifier: u32 = particles[particle_index].material_identifier;
    let rest_density: f32 = fluid_constraint_properties_from_identifier(material_identifier).x;
    let base_bucket_coordinates: vec2<i32> =
        fluid_spatial_bucket_coordinates_from_position(position);
    var correction_cells: vec2<f32> = vec2<f32>(0.0);
    for (var bucket_y: i32 = -1; bucket_y <= 1; bucket_y++) {
        for (var bucket_x: i32 = -1; bucket_x <= 1; bucket_x++) {
            let bucket_index: u32 =
                fluid_spatial_bucket_index_from_coordinates(
                    base_bucket_coordinates + vec2<i32>(bucket_x, bucket_y),
                );
            if bucket_index == INVALID_FLUID_BUCKET_INDEX {
                continue;
            }
            var neighbor_index: u32 = atomicLoad(&bucket_heads[bucket_index]);
            for (
                var chain_length: u32 = 0u;
                neighbor_index != INVALID_FLUID_PARTICLE_INDEX && chain_length < fluid_simulation_parameters.particle_capacity;
                chain_length++
            ) {
                if neighbor_index != particle_index {
                    let separation: vec2<f32> =
                        (position - predicted_positions[neighbor_index]) * CELLS_PER_TILE_FLOAT;
                    let distance: f32 = length(separation);
                    if distance > 0.000001 && distance < fluid_simulation_parameters.support_radius_cells {
                        correction_cells +=
                            (lambdas[particle_index]
                                + lambdas[neighbor_index]
                                + calculate_fluid_artificial_pressure(
                                    distance,
                                    material_identifier,
                                ))
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
    let h: f32 = fluid_simulation_parameters.support_radius_cells;
    if distance >= h {
        return 0.0;
    }
    let difference: f32 = h * h - distance * distance;
    return 4.0 * difference * difference * difference / (PI * h * h * h * h * h * h * h * h);
}

// Evaluates the two-dimensional particle-fluid spiky-kernel gradient
fn fluid_spiky_kernel_gradient(separation: vec2<f32>, distance: f32) -> vec2<f32> {
    let h: f32 = fluid_simulation_parameters.support_radius_cells;
    if distance <= 0.000001 || distance >= h {
        return vec2<f32>(0.0);
    }
    let remaining: f32 = h - distance;
    return -30.0 * remaining * remaining / (PI * h * h * h * h * h) * separation / distance;
}

// Calculates tensile-instability correction for one neighboring particle
fn calculate_fluid_artificial_pressure(distance: f32, material_identifier: u32) -> f32 {
    let reference: f32 =
        fluid_poly6_kernel_weight(
            ARTIFICIAL_PRESSURE_DELTA_Q_RATIO * fluid_simulation_parameters.support_radius_cells,
        );
    let ratio: f32 = fluid_poly6_kernel_weight(distance) / max(reference, 0.000001);
    let squared: f32 = ratio * ratio;
    return -fluid_constraint_properties_from_identifier(material_identifier).y * squared * squared;
}

// Reads density, artificial pressure, smoothing, and body-push properties
fn fluid_constraint_properties_from_identifier(material_identifier: u32) -> vec4<f32> {
    return fluid_material_properties[material_index_from_identifier(material_identifier) * 2u];
}

// Reads density and viscosity for derived fluid interaction
fn fluid_physical_properties_from_identifier(material_identifier: u32) -> vec2<f32> {
    return
        fluid_material_properties[material_index_from_identifier(material_identifier) * 2u + 1u].zw;
}
