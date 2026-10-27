// Copyright Rob Gage 2026

struct ThermalUniforms {
    region_width: u32,
    region_height: u32,
    ring_offset_x: u32,
    ring_offset_y: u32,
    /// First thermal index of the static, dynamic, fluid, and gas materials
    thermal_offsets: vec4<u32>,
    /// Duration of one step in seconds
    time_step: f32,
    /// Temperature in kelvin of the phantom air and of the surroundings it relaxes toward
    ambient_temperature: f32,
    /// Heat capacity of one gas amount unit of phantom air
    ambient_heat_capacity: f32,
    /// Heat per second per kelvin through each face of phantom air and to its surroundings
    ambient_conduction: f32,
    /// Temperature change in kelvin per step below which a cell lets its tile sleep
    sleep_temperature: f32,
    /// Fixed-point units per heat capacity unit in the fluid heat
    fluid_capacity_scale: f32,
    /// Fixed-point units per heat unit in the fluid heat
    fluid_energy_scale: f32,
    /// Fixed-point units per heat capacity unit times conduction unit in the fluid heat
    fluid_conduction_scale: f32,
    padding_0: u32,
    padding_1: u32,
    padding_2: u32,
    padding_3: u32,
};

struct ThermalControl {
    active_tile_count: atomic<u32>,
    active_tiles: array<u32>,
};

/// One cell's heat, as seen by its neighbors and itself
struct ThermalNode {
    /// Temperature in kelvin of everything in the cell
    temperature: f32,
    /// Heat capacity of everything in the cell
    heat_capacity: f32,
    /// Heat per second per kelvin of the cell's matter, weighted by heat capacity
    conduction: f32,
};

@group(0) @binding(0)
var<uniform> uniforms: ThermalUniforms;

@group(0) @binding(1)
var<storage, read_write> material_identifiers: array<u32>;

@group(0) @binding(2)
var<storage, read_write> matter: array<vec2<u32>>;

@group(0) @binding(3)
var<storage, read_write> temperature_changes: array<vec2<u32>>;

@group(0) @binding(4)
var<storage, read_write> gas: array<vec4<u32>>;

@group(0) @binding(5)
var<storage, read_write> thermal_awake: array<atomic<u32>>;

@group(0) @binding(6)
var<storage, read_write> control: ThermalControl;

@group(0) @binding(7)
var<storage, read> thermal_materials: array<vec4<u32>>;

@group(0) @binding(8)
var<storage, read_write> fluid_heat: array<vec4<i32>>;

@group(0) @binding(9)
var<storage, read_write> steps: array<vec2<f32>>;

@group(0) @binding(10)
var<storage, read_write> awake: array<atomic<u32>>;

@group(0) @binding(11)
var<storage, read_write> gas_awake: array<atomic<u32>>;

@group(1) @binding(0)
var<storage, read_write> dispatches: array<u32>;

/// Tile flag: the tile's temperatures changed, so it and its neighbors are simulated next tick
const TILE_AWAKE: u32 = 1u;

/// Cellular tile awake value of a tile whose dynamic cells must be simulated, matching
/// `CELLULAR_TILE_AWAKE`
const CELLULAR_TILE_AWAKE: u32 = 3u;

/// Gas amount in one atmosphere
const AMOUNT_PER_ATMOSPHERE: f32 = 4096.0;

/// Largest amount of one gas species slot
const SLOT_AMOUNT_MAXIMUM: u32 = 0xffffffu;

/// Amount of a cell holding one whole unit of its material, matching
/// `CELLULAR_TILE_AMOUNT_PER_UNIT`
const AMOUNT_PER_UNIT: f32 = 65536.0;

/// Number of heat exchanges of one cell per step: its four faces and its surroundings, each
/// limited to this fraction of the smaller heat capacity so no cell overshoots
const EXCHANGE_COUNT: f32 = 5.0;

/// Nodes of a tile and the ring of cells around it, in rows of ten from its bottom-left halo cell
var<workgroup> nodes: array<ThermalNode, 100>;

/// Returns the buffer index of a tile position inside the active region
fn tile_index(tile: vec2<u32>) -> u32 {
    let physical_x: u32 = (tile.x + uniforms.ring_offset_x) % uniforms.region_width;
    let physical_y: u32 = (tile.y + uniforms.ring_offset_y) % uniforms.region_height;
    return physical_y * uniforms.region_width + physical_x;
}

/// Returns whether a cell position lies inside the active region
fn is_cell_inside(cell: vec2<i32>) -> bool {
    return cell.x >= 0 && cell.y >= 0
        && cell.x < i32(uniforms.region_width * 8u)
        && cell.y < i32(uniforms.region_height * 8u);
}

/// Returns the buffer index of a cell position inside the active region
fn cell_index(cell: vec2<i32>) -> u32 {
    let position: vec2<u32> = vec2<u32>(cell);
    return tile_index(position / 8u) * 64u + (position.y % 8u) * 8u + position.x % 8u;
}

/// Returns the thermal index of a material identifier, given its form-specific offsets
fn thermal_index(material_identifier: u32, thermal_offsets: vec4<u32>) -> u32 {
    return thermal_offsets[material_identifier >> 30u] + (material_identifier & 0x3fffffffu);
}

/// Returns the heat of everything in a cell: its cellular matter and gas at the cell's temperature,
/// and the fluid particles in it at theirs, or an insulating node outside the active region
fn cell_node(cell: vec2<i32>) -> ThermalNode {
    if !is_cell_inside(cell) { return ThermalNode(0.0, 0.0, 0.0); }
    let index: u32 = cell_index(cell);
    let cell_matter: vec2<u32> = matter[index];
    let temperature: f32 = bitcast<f32>(cell_matter.x);
    // gases and phantom air share the cell's temperature
    let slots: vec4<u32> = gas[index * 2u];
    let phantom: f32 = f32(gas[index * 2u + 1u].x);
    var gas_capacity: f32 = phantom * uniforms.ambient_heat_capacity;
    var conduction: f32 = gas_capacity * uniforms.ambient_conduction;
    for (var slot: u32 = 0u; slot < 4u; slot++) {
        let amount: f32 = f32(slots[slot] & SLOT_AMOUNT_MAXIMUM);
        if amount == 0.0 { continue; }
        let properties: vec4<u32> =
            thermal_materials[(uniforms.thermal_offsets.w + (slots[slot] >> 24u)) * 4u];
        let capacity: f32 = amount / AMOUNT_PER_ATMOSPHERE * bitcast<f32>(properties.x);
        gas_capacity += capacity;
        conduction += capacity * bitcast<f32>(properties.y);
    }
    var heat_capacity: f32 = gas_capacity;
    var energy: f32 = gas_capacity * temperature;
    let material_identifier: u32 = material_identifiers[index];
    if material_identifier != 0u {
        let index: u32 = thermal_index(material_identifier, uniforms.thermal_offsets);
        let properties: vec4<u32> = thermal_materials[index * 4u];
        let capacity: f32 = bitcast<f32>(properties.x) * f32(cell_matter.y) / AMOUNT_PER_UNIT;
        heat_capacity += capacity;
        energy += capacity * temperature;
        conduction += capacity * bitcast<f32>(properties.y);
    }
    // fluid particles deposited their heat capacity, heat, and conduction
    let heat: vec4<i32> = fluid_heat[index];
    heat_capacity += f32(heat.x) / uniforms.fluid_capacity_scale;
    energy += f32(heat.y) / uniforms.fluid_energy_scale;
    conduction += f32(heat.z) / uniforms.fluid_conduction_scale;
    if heat_capacity <= 0.0 { return ThermalNode(temperature, 0.0, 0.0); }
    return ThermalNode(energy / heat_capacity, heat_capacity, conduction / heat_capacity);
}

/// Returns the heat flowing per step into a node from a neighbor through their shared face: the
/// series conduction of their halves, limited so neither overshoots
fn face_heat(node: ThermalNode, neighbor: ThermalNode) -> f32 {
    if node.conduction <= 0.0 || neighbor.conduction <= 0.0 { return 0.0; }
    let conduction: f32 = 2.0 * node.conduction * neighbor.conduction
        / (node.conduction + neighbor.conduction);
    let limit: f32 = min(node.heat_capacity, neighbor.heat_capacity) / EXCHANGE_COUNT;
    return min(conduction * uniforms.time_step, limit)
        * (neighbor.temperature - node.temperature);
}

/// Lists every tile awake for any reason, or beside one, as active
@compute @workgroup_size(64)
fn collect(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if invocation.x >= uniforms.region_width * uniforms.region_height { return; }
    let tile: vec2<i32> = vec2<i32>(
        i32(invocation.x % uniforms.region_width),
        i32(invocation.x / uniforms.region_width),
    );
    // moving dynamic cells, fluid particles, and gas carry heat, so their tiles stay active
    var is_active: bool = false;
    for (var y: i32 = -1; y <= 1; y++) {
        for (var x: i32 = -1; x <= 1; x++) {
            let neighbor: vec2<i32> = tile + vec2<i32>(x, y);
            if neighbor.x < 0 || neighbor.y < 0
                || neighbor.x >= i32(uniforms.region_width)
                || neighbor.y >= i32(uniforms.region_height)
            { continue; }
            let index: u32 = tile_index(vec2<u32>(neighbor));
            if atomicLoad(&thermal_awake[index]) != 0u || atomicLoad(&awake[index]) != 0u
                || atomicLoad(&gas_awake[index]) != 0u
            {
                is_active = true;
            }
        }
    }
    if is_active {
        control.active_tiles[atomicAdd(&control.active_tile_count, 1u)] =
            tile_index(vec2<u32>(tile));
    }
}

/// Clears the awake flag of every tile once collected, for this tick's step to set
@compute @workgroup_size(64)
fn clear(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if invocation.x >= uniforms.region_width * uniforms.region_height { return; }
    atomicStore(&thermal_awake[invocation.x], 0u);
}

/// Writes the indirect dispatch of one workgroup per active tile
@compute @workgroup_size(1)
fn prepare() {
    dispatches[0] = atomicLoad(&control.active_tile_count);
    dispatches[1] = 1u;
    dispatches[2] = 1u;
}

/// Returns the logical position of an active tile from its buffer index
fn active_tile(workgroup: u32) -> vec2<i32> {
    let index: u32 = control.active_tiles[workgroup];
    let physical: vec2<u32> =
        vec2<u32>(index % uniforms.region_width, index / uniforms.region_width);
    return vec2<i32>(
        i32((physical.x + uniforms.region_width - uniforms.ring_offset_x) % uniforms.region_width),
        i32((physical.y + uniforms.region_height - uniforms.ring_offset_y)
            % uniforms.region_height),
    );
}

/// Conducts one step of heat through every cell of an active tile from the temperatures before
/// the step: each face exchanges the same heat both ways, and phantom air exchanges heat with its
/// surroundings at the ambient temperature. The new temperature of the cell's cellular matter
/// and gas, and the change of its fluid particles, wait in the steps for the apply pass
@compute @workgroup_size(64)
fn step(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_cell: u32,
) {
    let tile: vec2<i32> = active_tile(workgroup.x);
    let origin: vec2<i32> = tile * 8 - 1;
    // every invocation loads its own cell, and the first 36 load the ring around the tile
    let local: vec2<i32> = vec2<i32>(i32(local_cell % 8u), i32(local_cell / 8u)) + 1;
    nodes[u32(local.y * 10 + local.x)] = cell_node(origin + local);
    if local_cell < 36u {
        var halo: vec2<i32>;
        if local_cell < 10u {
            halo = vec2<i32>(i32(local_cell), 0);
        } else if local_cell < 20u {
            halo = vec2<i32>(i32(local_cell) - 10, 9);
        } else if local_cell < 28u {
            halo = vec2<i32>(0, i32(local_cell) - 19);
        } else {
            halo = vec2<i32>(9, i32(local_cell) - 27);
        }
        nodes[u32(halo.y * 10 + halo.x)] = cell_node(origin + halo);
    }
    workgroupBarrier();
    let node: ThermalNode = nodes[u32(local.y * 10 + local.x)];
    let cell: vec2<i32> = origin + local;
    let index: u32 = cell_index(cell);
    let temperature: f32 = bitcast<f32>(matter[index].x);
    if node.heat_capacity <= 0.0 {
        steps[index] = vec2<f32>(temperature, 0.0);
        return;
    }
    var heat: f32 = face_heat(node, nodes[u32(local.y * 10 + local.x - 1)])
        + face_heat(node, nodes[u32(local.y * 10 + local.x + 1)])
        + face_heat(node, nodes[u32((local.y - 1) * 10 + local.x)])
        + face_heat(node, nodes[u32((local.y + 1) * 10 + local.x)]);
    // phantom air relaxes toward the surroundings' temperature
    let phantom: f32 = f32(gas[index * 2u + 1u].x);
    if phantom > 0.0 && uniforms.ambient_conduction > 0.0 {
        heat += min(uniforms.ambient_conduction * uniforms.time_step,
            node.heat_capacity / EXCHANGE_COUNT)
            * (uniforms.ambient_temperature - node.temperature);
    }
    let stepped: f32 = node.temperature + heat / node.heat_capacity;
    // the cell's matter and its fluid particles each move to the stepped temperature
    let fluid_heat_capacity: f32 = f32(fluid_heat[index].x) / uniforms.fluid_capacity_scale;
    let fluid_energy: f32 = f32(fluid_heat[index].y) / uniforms.fluid_energy_scale;
    let cell_heat_capacity: f32 = node.heat_capacity - fluid_heat_capacity;
    var cell_temperature: f32 = stepped;
    if cell_heat_capacity > 0.0 {
        let shown: f32 = (node.temperature * node.heat_capacity - fluid_energy)
            / cell_heat_capacity;
        cell_temperature = temperature + stepped - shown;
    }
    var fluid_change: f32 = 0.0;
    if fluid_heat_capacity > 0.0 { fluid_change = stepped - fluid_energy / fluid_heat_capacity; }
    steps[index] = vec2<f32>(cell_temperature, fluid_change);
}

/// Writes every stepped cell of an active tile with the temperature edits added, and wakes the
/// tile while its temperatures change
@compute @workgroup_size(64)
fn apply(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_cell: u32,
) {
    let tile: vec2<i32> = active_tile(workgroup.x);
    let physical_tile: u32 = control.active_tiles[workgroup.x];
    let cell: vec2<i32> = tile * 8 + vec2<i32>(i32(local_cell % 8u), i32(local_cell / 8u));
    let index: u32 = cell_index(cell);
    let edit: u32 = temperature_changes[index].x;
    let edited: f32 = bitcast<f32>(edit);
    if edit != 0u { temperature_changes[index] = vec2<u32>(0u); }
    let step: vec2<f32> = steps[index];
    let previous: f32 = bitcast<f32>(matter[index].x);
    let temperature: f32 = max(step.x + edited, 0.0);
    matter[index].x = bitcast<u32>(temperature);
    fluid_heat[index].w = bitcast<i32>(step.y + edited);
    if abs(temperature - previous) > uniforms.sleep_temperature
        || abs(step.y + edited) > uniforms.sleep_temperature
    {
        atomicOr(&thermal_awake[physical_tile], TILE_AWAKE);
    }
}
