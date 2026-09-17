// Clears the transient coarse mask before current pressure sources are discovered
@compute @workgroup_size(64)
fn clear_active_cellular_pressure_tiles(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let tile_count: u32 = parameters.buffered_tile_size.x * parameters.buffered_tile_size.y;
    if invocation.x < tile_count {
        atomicStore(&active_pressure_tiles[invocation.x], 0u);
    }
}

// Marks source tiles and the one-tile halo reachable by the fixed six-pass stencil
@compute @workgroup_size(64)
fn mark_active_cellular_pressure_tiles(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) lane: u32,
) {
    let logical_tile_index: u32 = workgroup.x;
    let tile_count: u32 = parameters.buffered_tile_size.x * parameters.buffered_tile_size.y;
    if logical_tile_index >= tile_count {
        return;
    }
    if lane == 0u {
        atomicStore(&pressure_tile_has_source, 0u);
    }
    workgroupBarrier();
    let logical_tile: vec2<u32> =
        vec2<u32>(
            logical_tile_index % parameters.buffered_tile_size.x,
            logical_tile_index / parameters.buffered_tile_size.x,
        );
    let physical_tile: vec2<u32> =
        physical_tile_from_logical_tile(
            logical_tile,
            parameters.buffered_tile_size,
            parameters.ring_offset,
        );
    let cell_start: u32 =
        (physical_tile.y * parameters.buffered_tile_size.x + physical_tile.x) * CELL_COUNT_PER_TILE;
    let index: u32 = cell_start + lane;
    let cell: vec2<i32> =
        world_cell_from_logical_tile_major_index(
            logical_tile_index * CELL_COUNT_PER_TILE + lane,
            parameters.buffered_origin,
            parameters.buffered_tile_size,
        );
    var has_source: bool =
        any(pending_pressure[index] != vec4<f32>(0.0))
            || external_body_occupancy[index] == 1u
            || external_body_occupancy[index] == 2u;
    let material: u32 = effective_pressure_material(index);
    let form: u32 = material_form_from_identifier(material);
    for (var channel: u32 = 0u; channel < 4u && !has_source; channel++) {
        let neighbor: u32 =
            cellular_pressure_physical_cell_index_from_world_cell(
                cell + world_cell_direction_from_pressure_channel(channel),
            );
        if neighbor == INVALID_PHYSICAL_CELL_INDEX {
            continue;
        }
        let neighbor_form: u32 =
            material_form_from_identifier(effective_pressure_material(neighbor));
        let rigid_interface: bool = (rigid_owners[index] != 0u) != (rigid_owners[neighbor] != 0u);
        let fluid_interface: bool =
            (mechanical_fluid_cells[index].mass > 0.0) != (mechanical_fluid_cells[neighbor].mass > 0.0)
                && (form == CELLULAR_DYNAMIC_MATERIAL_FORM
                || form == CELLULAR_STATIC_MATERIAL_FORM
                || neighbor_form == CELLULAR_DYNAMIC_MATERIAL_FORM
                || neighbor_form == CELLULAR_STATIC_MATERIAL_FORM
                || gas_cell_is_open(index)
                || gas_cell_is_open(neighbor));
        let gas_interface: bool =
            (gas_cell_is_open(index) != gas_cell_is_open(neighbor))
                && (form != 0u
                    || neighbor_form != 0u
                    || mechanical_fluid_cells[index].mass > 0.0
                    || mechanical_fluid_cells[neighbor].mass > 0.0)
                && (length(gas_velocity[index]) > 0.0001 || length(
                    gas_velocity[neighbor],
                ) > 0.0001);
        var granular_impact: bool = false;
        if
            form == CELLULAR_DYNAMIC_MATERIAL_FORM
                && (neighbor_form == CELLULAR_DYNAMIC_MATERIAL_FORM
                    || neighbor_form == CELLULAR_STATIC_MATERIAL_FORM)
        {
            granular_impact =
                dot(
                    cellular_kinematics[index].xy - cellular_kinematics[neighbor].xy,
                    vec2<f32>(world_cell_direction_from_pressure_channel(channel)),
                ) > 0.0001;
        }
        has_source = rigid_interface || fluid_interface || gas_interface || granular_impact;
    }
    if has_source {
        atomicStore(&pressure_tile_has_source, 1u);
    }
    workgroupBarrier();
    if lane != 0u || atomicLoad(&pressure_tile_has_source) == 0u {
        return;
    }
    for (var offset_y: i32 = -1; offset_y <= 1; offset_y++) {
        for (var offset_x: i32 = -1; offset_x <= 1; offset_x++) {
            let active_tile: vec2<i32> = vec2<i32>(logical_tile) + vec2<i32>(offset_x, offset_y);
            if
                any(active_tile < vec2<i32>(0))
                    || active_tile.x >= i32(parameters.buffered_tile_size.x)
                    || active_tile.y >= i32(parameters.buffered_tile_size.y)
            {
                continue;
            }
            atomicStore(
                &active_pressure_tiles[u32(active_tile.y) * parameters.buffered_tile_size.x + u32(
                    active_tile.x,
                )],
                1u,
            );
        }
    }
}

// Pressure propagation is deliberately narrower than interaction discovery: an idle
// rigid boundary still needs contact work, but is not a pressure source by itself.
@compute @workgroup_size(64)
fn mark_pressure_active_cellular_pressure_tiles(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) lane: u32,
) {
    let logical_tile_index: u32 = workgroup.x;
    let tile_count: u32 = parameters.buffered_tile_size.x * parameters.buffered_tile_size.y;
    if logical_tile_index >= tile_count {
        return;
    }
    if lane == 0u {
        atomicStore(&pressure_tile_has_source, 0u);
    }
    workgroupBarrier();
    let logical_tile: vec2<u32> =
        vec2<u32>(
            logical_tile_index % parameters.buffered_tile_size.x,
            logical_tile_index / parameters.buffered_tile_size.x,
        );
    let physical_tile: vec2<u32> =
        physical_tile_from_logical_tile(
            logical_tile,
            parameters.buffered_tile_size,
            parameters.ring_offset,
        );
    let index: u32 =
        (physical_tile.y * parameters.buffered_tile_size.x + physical_tile.x) * CELL_COUNT_PER_TILE + lane;
    if
        any(pending_pressure[index] != vec4<f32>(0.0))
            || external_body_occupancy[index] == 1u
            || external_body_occupancy[index] == 2u
    {
        atomicStore(&pressure_tile_has_source, 1u);
    }
    workgroupBarrier();
    if lane != 0u || atomicLoad(&pressure_tile_has_source) == 0u {
        return;
    }
    for (var offset_y: i32 = -1; offset_y <= 1; offset_y++) {
        for (var offset_x: i32 = -1; offset_x <= 1; offset_x++) {
            let active_tile: vec2<i32> = vec2<i32>(logical_tile) + vec2<i32>(offset_x, offset_y);
            if
                any(active_tile < vec2<i32>(0))
                    || active_tile.x >= i32(parameters.buffered_tile_size.x)
                    || active_tile.y >= i32(parameters.buffered_tile_size.y)
            {
                continue;
            }
            atomicStore(
                &active_pressure_tiles[u32(active_tile.y) * parameters.buffered_tile_size.x + u32(
                    active_tile.x,
                )],
                1u,
            );
        }
    }
}

// Converts the coarse pressure mask into one indirect workgroup per active tile
@compute @workgroup_size(64)
fn compact_active_cellular_pressure_tiles(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if invocation.x == 0u {
        atomicStore(&pressure_indirect_dispatch[1], 1u);
        atomicStore(&pressure_indirect_dispatch[2], 1u);
    }
    let tile_count: u32 = parameters.buffered_tile_size.x * parameters.buffered_tile_size.y;
    let logical_tile_index: u32 = invocation.x;
    if
        logical_tile_index >= tile_count || atomicLoad(
            &active_pressure_tiles[logical_tile_index],
        ) == 0u
    {
        return;
    }
    let slot: u32 = atomicAdd(&pressure_indirect_dispatch[0], 1u);
    active_pressure_tile_indices[slot] = logical_tile_index;
}

// Queues editor impulse pressure without bypassing material transmission or mass response
@compute @workgroup_size(64)
fn queue_cellular_radial_impulse(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if invocation.x >= parameters.impulse_size.x * parameters.impulse_size.y {
        return;
    }
    let cell: vec2<i32> =
        parameters.impulse_min + vec2<i32>(
            i32(invocation.x % parameters.impulse_size.x),
            i32(invocation.x / parameters.impulse_size.x),
        );
    let index: u32 = cellular_pressure_physical_cell_index_from_world_cell(cell);
    if
        index == INVALID_PHYSICAL_CELL_INDEX || effective_pressure_material(
            index,
        ) == EMPTY_MATERIAL_IDENTIFIER
    {
        return;
    }
    let delta: vec2<f32> = vec2<f32>(cell) + vec2<f32>(0.5) - parameters.impulse_center;
    let distance: f32 = length(delta);
    if distance > parameters.impulse_radius {
        return;
    }
    let direction: vec2<f32> = normalize(select(vec2<f32>(1.0, 0.0), delta, distance > 0.0001));
    let falloff: f32 = 1.0 - distance / max(parameters.impulse_radius, 0.0001);
    let impulse: vec2<f32> = direction * parameters.impulse_strength * falloff;
    pending_pressure[index] += encode_directional_pressure(impulse);
    let material: u32 = effective_pressure_material(index);
    if material_form_from_identifier(material) == CELLULAR_DYNAMIC_MATERIAL_FORM {
        let mass: f32 = cellular_dynamic_properties[material_index_from_identifier(material)].x;
        cellular_kinematics[index].x += impulse.x / mass;
        cellular_kinematics[index].y += impulse.y / mass;
    }
}
