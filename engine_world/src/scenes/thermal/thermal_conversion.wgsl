// Copyright Rob Gage 2026

// Changes of cells shared by every grid shader that changes matter in place: the shader declares
// `is_cell_inside`, `cell_index`, and `is_static`, `material_identifiers: array<u32>`,
// `cellular_static_materials: array<vec4<f32>>`, and the atomic `rigid_body_cells`,
// `rigid_body_cellular_tiles`, and `changed_rigid_body_cells`

/// Removes a rigid body cell from its body, which then leaves the cell where it is, and reports
/// it for its body to lose it, or returns false when no more cells fit this tick
fn remove_rigid_body_cell(index: u32) -> bool {
    let rigid_body_cell: u32 = atomicLoad(&rigid_body_cells[index]);
    if rigid_body_cell == 0u { return true; }
    let count: u32 = atomicAdd(&changed_rigid_body_cells[0], 1u);
    if count + 1u >= arrayLength(&changed_rigid_body_cells) { return false; }
    atomicStore(&changed_rigid_body_cells[count + 1u], rigid_body_cell - 1u);
    let body_cell: u32 = rigid_body_cell - 1u;
    let bit: u32 = body_cell % 64u;
    atomicAnd(
        &rigid_body_cellular_tiles[body_cell / 64u * 8u + 4u + bit / 32u],
        ~(1u << (bit % 32u)),
    );
    atomicStore(&rigid_body_cells[index], 0u);
    return true;
}

/// Returns whether a cell has a static or rigid body side-neighbor
fn has_static_neighbor(cell: vec2<i32>) -> bool {
    for (var direction: u32 = 0u; direction < 4u; direction++) {
        var neighbor: vec2<i32> = cell;
        if direction == 0u { neighbor.x += 1; }
        if direction == 1u { neighbor.x -= 1; }
        if direction == 2u { neighbor.y += 1; }
        if direction == 3u { neighbor.y -= 1; }
        if !is_cell_inside(neighbor) { continue; }
        let neighbor_index: u32 = cell_index(neighbor);
        if is_static(material_identifiers[neighbor_index])
            || atomicLoad(&rigid_body_cells[neighbor_index]) != 0u
        { return true; }
    }
    return false;
}

/// Returns a static product as dynamic debris when it has no static side-neighbor
fn conversion_debris(product: u32, cell: vec2<i32>) -> u32 {
    if product >> 30u != 0u || has_static_neighbor(cell) { return product; }
    let properties: vec4<f32> = cellular_static_materials[product & 0x3fffffffu];
    let debris: u32 = bitcast<u32>(properties.w);
    return select(product, debris, debris != 0u);
}
