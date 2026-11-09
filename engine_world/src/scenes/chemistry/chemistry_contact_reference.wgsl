// Reference traversal before sparse no-op row skipping.
@compute @workgroup_size(64)
fn react_tiles(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    let listed: u32 = workgroup.x * TILES_PER_WORKGROUP + local_index / 16u;
    let local_block: u32 = local_index % 16u;
    // only the blocks the selection found able to react read their cells
    if ((tiles.active_tiles[listed * 2u + 1u] >> local_block) & 1u) == 0u { return; }
    let physical_tile: u32 = tiles.active_tiles[listed * 2u];
    // the tile owns the blocks whose first cell lies in it, shifted back by the block offset
    let origin: vec2<i32> = listed_tile(physical_tile) * 8 - i32(uniforms.block_offset)
        + 2 * vec2<i32>(i32(local_block % 4u), i32(local_block / 4u));
    for (var slot: u32 = 0u; slot < uniforms.loaded_cell_count; slot++) {
        load_cell(slot, block_cell_position(origin, slot));
        load_cell_heat(slot);
    }
    is_reacting = false;
    let units: u32 = uniforms.unit_count;
    let pairs: u32 = units * (units - 1u) / 2u;
    let per_cell: u32 = units + pairs;
    let cell_contacts: u32 = uniforms.block_cell_count * per_cell;
    let per_face: u32 = units * units;
    let face_contacts: u32 = uniforms.face_count * per_face;
    let contact_count: u32 = cell_contacts + face_contacts
        + uniforms.block_cell_count * 2u * per_face;
    let face_first = array<u32, 4>(0u, 2u, 0u, 1u);
    let face_second = array<u32, 4>(1u, 3u, 2u, 3u);
    // Enumerate unary/same-cell contacts, then internal faces, then outside catalysts in exactly
    // their original order. One shared contact call keeps the large solver from being inlined
    // separately for each traversal path.
    for (var contact: u32 = 0u; contact < contact_count; contact++) {
        var first_slot: u32;
        var second_slot: u32;
        var first_unit: u32;
        var second_unit: u32 = 0u;
        var scale: f32 = 1.0;
        if contact < cell_contacts {
            first_slot = contact / per_cell;
            let local: u32 = contact % per_cell;
            if local < units {
                first_unit = local;
                second_slot = CELL_OUTSIDE;
            } else {
                second_slot = first_slot;
                var pair: u32 = local - units;
                first_unit = 0u;
                loop {
                    let width: u32 = units - first_unit - 1u;
                    if pair < width { break; }
                    pair -= width;
                    first_unit++;
                }
                second_unit = first_unit + 1u + pair;
            }
        } else if contact < cell_contacts + face_contacts {
            let local: u32 = contact - cell_contacts;
            let face: u32 = local / per_face;
            first_slot = face_first[face];
            second_slot = face_second[face];
            first_unit = (local % per_face) / units;
            second_unit = local % units;
            scale = 2.0;
        } else {
            let local: u32 = contact - cell_contacts - face_contacts;
            let face: u32 = local / per_face;
            first_slot = face / 2u;
            second_slot = 4u + face;
            first_unit = (local % per_face) / units;
            second_unit = local % units;
        }
        if cells[first_slot].index == CELL_OUTSIDE { continue; }
        if second_slot != CELL_OUTSIDE && cells[second_slot].index == CELL_OUTSIDE { continue; }
        react_contact(first_slot, first_unit, second_slot, second_unit, scale);
    }
    for (var slot: u32 = 0u; slot < uniforms.block_cell_count; slot++) { store_cell(slot); }
    if is_reacting { atomicOr(&thermal_awake[physical_tile], THERMAL_TILE_AWAKE); }
}
