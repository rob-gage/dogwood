// Copyright Rob Gage 2026

struct EditCellularTile {
    cellular_tile_index: u32,
    inserted_cells: vec2<u32>,
    replaced_cells: vec2<u32>,
    material_identifiers: array<u32, 64>,
    colors: array<u32, 64>,
};

@group(0) @binding(0)
var<storage, read> edit_cellular_tiles: array<EditCellularTile>;

@group(0) @binding(1)
var<storage, read_write> material_identifiers: array<u32>;

@group(0) @binding(2)
var<storage, read_write> colors: array<u32>;

/// Writes one edited cell per invocation, inserting only into empty cells
@compute @workgroup_size(64)
fn compute(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) cell: u32,
) {
    let edit_cellular_tile_index: u32 = workgroup.x;
    let cell_bit: u32 = 1u << (cell % 32u);
    let is_inserted: bool =
        (edit_cellular_tiles[edit_cellular_tile_index].inserted_cells[cell / 32u] & cell_bit) != 0u;
    let is_replaced: bool =
        (edit_cellular_tiles[edit_cellular_tile_index].replaced_cells[cell / 32u] & cell_bit) != 0u;
    let cell_index: u32 = edit_cellular_tiles[edit_cellular_tile_index].cellular_tile_index * 64u
        + cell;
    // empty shares the static tag, so only identifier zero is empty
    if is_replaced || (is_inserted && material_identifiers[cell_index] == 0u) {
        material_identifiers[cell_index] =
            edit_cellular_tiles[edit_cellular_tile_index].material_identifiers[cell];
        colors[cell_index] = edit_cellular_tiles[edit_cellular_tile_index].colors[cell];
    }
}
