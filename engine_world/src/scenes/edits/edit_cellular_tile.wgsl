// Copyright Rob Gage 2026

struct EditCellularTile {
    cellular_tile_index: u32,
    ambient_gas_amount: u32,
    inserted_cells: vec2<u32>,
    replaced_cells: vec2<u32>,
    material_identifiers: array<u32, 64>,
    colors: array<u32, 64>,
    radiances: array<u32, 64>,
    pressures: array<f32, 64>,
    temperatures: array<f32, 64>,
    temperature_changes: array<f32, 64>,
};

@group(0) @binding(0)
var<storage, read> edit_cellular_tiles: array<EditCellularTile>;

@group(0) @binding(1)
var<storage, read_write> material_identifiers: array<u32>;

@group(0) @binding(2)
var<storage, read_write> colors: array<vec2<u32>>;

@group(0) @binding(3)
var<storage, read_write> kinematics: array<vec2<u32>>;

@group(0) @binding(4)
var<storage, read_write> awake: array<atomic<u32>>;

@group(0) @binding(5)
var<storage, read_write> gas: array<vec4<u32>>;

@group(0) @binding(6)
var<storage, read_write> gas_awake: array<atomic<u32>>;

@group(0) @binding(7)
var<storage, read_write> matter: array<vec2<u32>>;

@group(0) @binding(8)
var<storage, read_write> temperature_changes: array<vec2<u32>>;

@group(0) @binding(9)
var<storage, read_write> thermal_awake: array<atomic<u32>>;

/// Amount of a cell holding one whole unit of its material, matching
/// `CELLULAR_TILE_AMOUNT_PER_UNIT`
const AMOUNT_PER_UNIT: u32 = 65536u;

/// Gas amount in one atmosphere
const AMOUNT_PER_ATMOSPHERE: u32 = 4096u;

/// Largest amount of gas or stored pressure in one cell, `PRESSURE_MAXIMUM` atmospheres
const AMOUNT_MAXIMUM: u32 = 16773120u;

/// Writes one edited cell per invocation at rest, inserting only into empty cells, and wakes its
/// tile so dynamic cells and gas around the edit notice it. A gas fills an emptied cell in place
/// of its phantom air at the ambient pressure, or one atmosphere when there is none; emptying a
/// cell leaves only ambient phantom air; and a cellular material keeps the cell's gases, which it
/// then pushes out, but removes its phantom air. Added pressure raises a static cell's stored
/// pressure or another cell's phantom air, and removed pressure lowers the stored pressure or
/// every gas amount in proportion, waking the tile's pressure but not its cellular tiles. Written
/// cells take their temperature, and cellular cells one unit of amount; added temperatures wait
/// for the next thermal step to warm or cool everything in their cells
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
        let material_identifier: u32 =
            edit_cellular_tiles[edit_cellular_tile_index].material_identifiers[cell];
        let ambient_gas_amount: u32 =
            edit_cellular_tiles[edit_cellular_tile_index].ambient_gas_amount;
        if material_identifier >> 30u == 3u {
            let amount: u32 = min(max(ambient_gas_amount, AMOUNT_PER_ATMOSPHERE), 0xffffffu);
            material_identifiers[cell_index] = 0u;
            colors[cell_index] = vec2<u32>(0u);
            gas[cell_index * 2u] = vec4<u32>(
                ((material_identifier & 0xffu) << 24u) | amount,
                0u,
                0u,
                0u,
            );
            gas[cell_index * 2u + 1u] = vec4<u32>(0u);
        } else {
            material_identifiers[cell_index] = material_identifier;
            colors[cell_index] = vec2<u32>(
                edit_cellular_tiles[edit_cellular_tile_index].colors[cell],
                edit_cellular_tiles[edit_cellular_tile_index].radiances[cell],
            );
            if material_identifier == 0u {
                gas[cell_index * 2u] = vec4<u32>(0u);
                gas[cell_index * 2u + 1u] = vec4<u32>(ambient_gas_amount, 0u, 0u, 0u);
            } else {
                gas[cell_index * 2u + 1u] = vec4<u32>(0u);
            }
        }
        kinematics[cell_index] = vec2<u32>(0u, 0u);
        // a negative temperature, written for fluid fills, keeps the emptied cell's temperature
        let temperature: f32 = edit_cellular_tiles[edit_cellular_tile_index].temperatures[cell];
        if temperature >= 0.0 { matter[cell_index].x = bitcast<u32>(temperature); }
        matter[cell_index].y = select(0u, AMOUNT_PER_UNIT, material_identifiers[cell_index] != 0u);
        // awake, matching `CELLULAR_TILE_THERMAL_AWAKE`
        atomicOr(
            &thermal_awake[edit_cellular_tiles[edit_cellular_tile_index].cellular_tile_index],
            1u,
        );
        // awake and disturbed, matching `CELLULAR_TILE_AWAKE`
        atomicOr(&awake[edit_cellular_tiles[edit_cellular_tile_index].cellular_tile_index], 3u);
        // awake, matching `CELLULAR_TILE_GAS_AWAKE`
        atomicOr(&gas_awake[edit_cellular_tiles[edit_cellular_tile_index].cellular_tile_index], 1u);
    }
    let temperature_change: f32 =
        edit_cellular_tiles[edit_cellular_tile_index].temperature_changes[cell];
    if temperature_change != 0.0 {
        let previous: f32 = bitcast<f32>(temperature_changes[cell_index].x);
        temperature_changes[cell_index].x = bitcast<u32>(previous + temperature_change);
        atomicOr(
            &thermal_awake[edit_cellular_tiles[edit_cellular_tile_index].cellular_tile_index],
            1u,
        );
    }
    let pressure: f32 = edit_cellular_tiles[edit_cellular_tile_index].pressures[cell];
    if pressure != 0.0 {
        let amount: u32 = u32(round(abs(pressure) * f32(AMOUNT_PER_ATMOSPHERE)));
        let material_identifier: u32 = material_identifiers[cell_index];
        if material_identifier != 0u && material_identifier >> 30u == 0u {
            let ambient_gas_amount: f32 = f32(
                edit_cellular_tiles[edit_cellular_tile_index].ambient_gas_amount,
            );
            let stored: f32 = f32(bitcast<i32>(gas[cell_index * 2u + 1u].w));
            let pressure_change: f32 = select(-f32(amount), f32(amount), pressure > 0.0);
            let updated: f32 = clamp(
                stored + pressure_change,
                -ambient_gas_amount,
                f32(AMOUNT_MAXIMUM) - ambient_gas_amount,
            );
            gas[cell_index * 2u + 1u].w = bitcast<u32>(i32(updated));
        } else {
            var total: u32 = gas[cell_index * 2u + 1u].x;
            for (var slot: u32 = 0u; slot < 4u; slot += 1u) {
                total += gas[cell_index * 2u][slot] & 0xffffffu;
            }
            if pressure > 0.0 {
                gas[cell_index * 2u + 1u].x += min(amount, AMOUNT_MAXIMUM - total);
            } else if total > 0u {
                let remaining: u32 = total - min(amount, total);
                let ratio: f32 = f32(remaining) / f32(total);
                for (var slot: u32 = 0u; slot < 4u; slot += 1u) {
                    let word: u32 = gas[cell_index * 2u][slot];
                    let slot_amount: u32 = word & 0xffffffu;
                    let updated_amount: u32 = u32(floor(f32(slot_amount) * ratio));
                    gas[cell_index * 2u][slot] = select(
                        ((word >> 24u) << 24u) | updated_amount,
                        0u,
                        updated_amount == 0u,
                    );
                }
                gas[cell_index * 2u + 1u].x = u32(
                    floor(f32(gas[cell_index * 2u + 1u].x) * ratio),
                );
            }
        }
        atomicOr(&gas_awake[edit_cellular_tiles[edit_cellular_tile_index].cellular_tile_index], 1u);
    }
}
