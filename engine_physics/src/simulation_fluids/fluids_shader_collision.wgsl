// Projects a fluid particle out of cellular or hard-body geometry
fn project_fluid_particle_out_of_cellular_collision(initial_position: vec2<f32>) -> vec2<f32> {
    var position: vec2<f32> = initial_position;
    let radius: f32 = fluid_simulation_parameters.particle_radius_cells / CELLS_PER_TILE_FLOAT;
    for (var iteration: u32 = 0u; iteration < 2u; iteration++) {
        let center_cell: vec2<i32> = vec2<i32>(floor(position * CELLS_PER_TILE_FLOAT));
        var resolved: bool = false;
        for (var offset_y: i32 = -1; offset_y <= 1 && !resolved; offset_y++) {
            for (var offset_x: i32 = -1; offset_x <= 1; offset_x++) {
                let cell: vec2<i32> = center_cell + vec2<i32>(offset_x, offset_y);
                let index: u32 = fluid_physical_cell_index_from_world_cell(cell);
                if
                    index == INVALID_PHYSICAL_CELL_INDEX
                    || (cellular_material_identifiers[index] == EMPTY_MATERIAL_IDENTIFIER && !is_hard_external_body(
                        external_body_occupancy[index],
                    ))
                {
                    continue;
                }
                let minimum: vec2<f32> = vec2<f32>(cell) / CELLS_PER_TILE_FLOAT;
                let maximum: vec2<f32> = vec2<f32>(cell + vec2<i32>(1)) / CELLS_PER_TILE_FLOAT;
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
                            maximum.y - position.y,
                        );
                    let side: f32 =
                        min(min(distances.x, distances.y), min(distances.z, distances.w));
                    normal =
                        select(
                            select(vec2<f32>(-1.0, 0.0), vec2<f32>(1.0, 0.0), side == distances.y),
                            select(vec2<f32>(0.0, -1.0), vec2<f32>(0.0, 1.0), side == distances.w),
                            side == distances.z || side == distances.w,
                        );
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
        let result = atomicCompareExchangeWeak(&free_count[0], available, available - 1u);
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
        let result = atomicCompareExchangeWeak(&free_count[0], available, available + 1u);
        if result.exchanged {
            particles[particle_index].material_identifier = EMPTY_MATERIAL_IDENTIFIER;
            free_indices[available] = particle_index;
            return;
        }
        available = result.old_value;
    }
}
