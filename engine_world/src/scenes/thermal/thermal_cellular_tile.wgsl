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
    /// Fixed-point units per heat unit added to cells by changes of matter
    heat_scale: f32,
    /// Number of the tick, varying the colors transitions carry
    tick: u32,
    padding_0: u32,
    padding_1: u32,
};

struct ThermalControl {
    active_tile_count: atomic<u32>,
    active_tiles: array<u32>,
};

/// One cell's heat, as seen by its neighbors and itself
struct ThermalNode {
    /// Temperature in kelvin of everything in the cell, counting latent heat banks at their
    /// transition temperature
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

@group(0) @binding(12)
var<storage, read> transition_colors: array<vec4<f32>>;

@group(0) @binding(13)
var<storage, read_write> colors: array<u32>;

@group(0) @binding(14)
var<storage, read_write> kinematics: array<vec2<u32>>;

@group(0) @binding(15)
var<storage, read_write> rigid_body_cells: array<atomic<u32>>;

@group(0) @binding(16)
var<storage, read_write> rigid_body_cellular_tiles: array<atomic<u32>>;

@group(0) @binding(17)
var<storage, read_write> changed_cellular_tiles: array<atomic<u32>>;

@group(0) @binding(18)
var<storage, read_write> changed_rigid_body_cells: array<atomic<u32>>;

@group(0) @binding(19)
var<storage, read_write> conversions: array<atomic<u32>>;

@group(0) @binding(20)
var<storage, read> gas_materials: array<vec4<f32>>;

@group(0) @binding(21)
var<storage, read> cellular_static_materials: array<vec4<f32>>;

@group(1) @binding(0)
var<storage, read_write> dispatches: array<u32>;

/// Tile flag: the tile's temperatures changed, so it and its neighbors are simulated next tick
const TILE_AWAKE: u32 = 1u;

/// Tile flag: the tile's static cells changed outside this pass, so its static occupancy is
/// reported to collisions
const TILE_STATIC_CHANGED: u32 = 2u;

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

/// Conversion kind of a cell changing into fluid particles
const CONVERSION_CELL: u32 = 0u;

/// Nodes of a tile and the ring of cells around it, in rows of ten from its bottom-left halo cell
var<workgroup> nodes: array<ThermalNode, 100>;

/// Whether a static cell of the workgroup's tile changed this tick
var<workgroup> is_static_changed: atomic<u32>;

/// Static cells of the workgroup's tile outside rigid bodies after its changes, as two masks
var<workgroup> static_occupancy: array<atomic<u32>, 2>;

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

/// Returns whether a material identifier is a static material other than empty
fn is_static(material_identifier: u32) -> bool {
    return material_identifier != 0u && material_identifier >> 30u == 0u;
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
    // cellular matter holds its transition temperature while it banks latent heat
    let material_identifier: u32 = material_identifiers[index];
    if material_identifier != 0u {
        let index: u32 = thermal_index(material_identifier, uniforms.thermal_offsets);
        let properties: vec4<u32> = thermal_materials[index * 4u];
        let capacity: f32 = bitcast<f32>(properties.x) * f32(cell_matter.y) / AMOUNT_PER_UNIT;
        heat_capacity += capacity;
        energy += capacity * effective_temperature(temperature, index);
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

/// Clears the awake flag of every tile once collected, for this tick's step to set, keeping
/// whether its static cells changed for the apply pass
@compute @workgroup_size(64)
fn clear(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if invocation.x >= uniforms.region_width * uniforms.region_height { return; }
    atomicAnd(&thermal_awake[invocation.x], TILE_STATIC_CHANGED);
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
/// the step: each face exchanges the same heat both ways, phantom air exchanges heat with its
/// surroundings at the ambient temperature, and heat added by changes of matter spreads over the
/// cell. The new temperature of the cell's cellular matter and gas, and the change of its fluid
/// particles, wait in the steps for the apply pass
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
    // the cell's matter and its fluid particles each move to the stepped temperature, the
    // cellular matter keeping its latent heat bank
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

/// Adds an amount of a gas at a temperature to a cell's species slots, mixing it into the cell's
/// temperature by heat capacity, or returns false when no slot holds it
fn add_gas(index: u32, gas_index: u32, amount: u32, temperature: f32) -> bool {
    var slots: vec4<u32> = gas[index * 2u];
    var target_slot: u32 = 4u;
    for (var slot: u32 = 0u; slot < 4u; slot++) {
        let slot_amount: u32 = slots[slot] & SLOT_AMOUNT_MAXIMUM;
        if slot_amount > 0u && slots[slot] >> 24u == gas_index {
            target_slot = slot;
        } else if slot_amount == 0u && target_slot == 4u {
            target_slot = slot;
        }
    }
    if target_slot == 4u { return false; }
    let held: u32 = slots[target_slot] & SLOT_AMOUNT_MAXIMUM;
    if held + amount > SLOT_AMOUNT_MAXIMUM { return false; }
    // the cell's gas and the arriving gas mix into one temperature
    let words: vec4<u32> = gas[index * 2u + 1u];
    var heat_capacity: f32 = f32(words.x) * uniforms.ambient_heat_capacity;
    for (var slot: u32 = 0u; slot < 4u; slot++) {
        let slot_amount: u32 = slots[slot] & SLOT_AMOUNT_MAXIMUM;
        if slot_amount == 0u { continue; }
        heat_capacity += f32(slot_amount) / AMOUNT_PER_ATMOSPHERE
            * thermal_heat_capacity(uniforms.thermal_offsets.w + (slots[slot] >> 24u));
    }
    let added_heat_capacity: f32 = f32(amount) / AMOUNT_PER_ATMOSPHERE
        * thermal_heat_capacity(uniforms.thermal_offsets.w + gas_index);
    let cell_temperature: f32 = bitcast<f32>(matter[index].x);
    matter[index].x = bitcast<u32>(
        (heat_capacity * cell_temperature + added_heat_capacity * temperature)
            / max(heat_capacity + added_heat_capacity, 1.0e-6),
    );
    slots[target_slot] = (gas_index << 24u) | (held + amount);
    gas[index * 2u] = slots;
    return true;
}

/// Lists a cell, or one of its gases, for the fluid simulation to change into a fluid particle
fn list_conversion(index: u32, kind: u32) {
    let count: u32 = atomicAdd(&conversions[0], 1u);
    if 2u + count * 2u + 1u >= arrayLength(&conversions) { return; }
    atomicStore(&conversions[2u + count * 2u], index);
    atomicStore(&conversions[2u + count * 2u + 1u], kind);
}

/// Changes a cellular cell due for a transition in place: into another cellular material, or into
/// a gas left in the cell, carrying its color, amount, and heat, or lists it to become a fluid
/// particle. Returns whether the cell's static occupancy changed
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

/// Changes a cell due for a thermal transition in place, or lists it for fluid conversion
fn convert_cell(
    index: u32,
    tile: u32,
    cell: vec2<i32>,
    material_identifier: u32,
    temperature: f32,
) -> bool {
    let source: u32 = thermal_index(material_identifier, uniforms.thermal_offsets);
    let transition: ThermalTransition = due_transition(temperature, source, true);
    if !transition.is_due { return false; }
    let product_form: u32 = transition.product >> 30u;
    if product_form == 2u {
        list_conversion(index, CONVERSION_CELL);
        atomicOr(&thermal_awake[tile], TILE_AWAKE);
        return false;
    }
    let amount: f32 = f32(matter[index].y) / AMOUNT_PER_UNIT;
    let product: u32 = thermal_index(transition.product, uniforms.thermal_offsets);
    let product_amount: f32 = amount * transition.amount;
    let product_temperature: f32 = product_temperature(
        transition,
        temperature,
        amount,
        thermal_heat_capacity(source),
        product_amount,
        thermal_heat_capacity(product),
        true,
    );
    // a cell becoming gas leaves its gas in the emptied cell, or waits for a free slot
    if product_form == 3u {
        let gas_amount: u32 = u32(round(product_amount * AMOUNT_PER_ATMOSPHERE));
        let gas_index: u32 = transition.product & 0xffu;
        // a rigid body loses the cell first, so a cell that keeps waiting stays as terrain
        if !remove_rigid_body_cell(index) { return false; }
        if !add_gas(index, gas_index, gas_amount, product_temperature) {
            atomicOr(&thermal_awake[tile], TILE_AWAKE);
            return is_static(material_identifier);
        }
        material_identifiers[index] = 0u;
        colors[index] = 0u;
        kinematics[index] = vec2<u32>(0u);
        matter[index].y = 0u;
        gas[index * 2u + 1u].w = 0u;
        atomicOr(&gas_awake[tile], 1u);
        atomicOr(&awake[tile], CELLULAR_TILE_AWAKE);
        return is_static(material_identifier);
    }
    if !remove_rigid_body_cell(index) { return false; }
    let converted_material: u32 = conversion_debris(transition.product, cell);
    material_identifiers[index] = converted_material;
    colors[index] = transition_color(
        colors[index],
        source,
        transition.is_hot,
        index ^ (uniforms.tick * 0x9e3779b9u),
    );
    matter[index] = vec2<u32>(
        bitcast<u32>(product_temperature),
        u32(round(product_amount * AMOUNT_PER_UNIT)),
    );
    // dynamic cells keep moving, and new static cells start intact and unloaded
    if converted_material >> 30u != 1u
        || converted_material != transition.product
        || material_identifier >> 30u == 0u
    {
        kinematics[index] = vec2<u32>(0u);
    }
    gas[index * 2u + 1u].w = 0u;
    atomicOr(&awake[tile], CELLULAR_TILE_AWAKE);
    atomicOr(&gas_awake[tile], 1u);
    return is_static(material_identifier) || converted_material >> 30u == 0u;
}

/// Changes the first gas of an empty cell due for a transition: into another gas in place, or
/// into one unit of cellular matter once the cell holds enough of it, or lists it to become a
/// fluid particle. Returns whether the cell's static occupancy changed
fn convert_gas(index: u32, tile: u32, cell: vec2<i32>, temperature: f32) -> bool {
    let slots: vec4<u32> = gas[index * 2u];
    for (var slot: u32 = 0u; slot < 4u; slot++) {
        let amount: u32 = slots[slot] & SLOT_AMOUNT_MAXIMUM;
        if amount == 0u { continue; }
        let gas_index: u32 = slots[slot] >> 24u;
        let source: u32 = uniforms.thermal_offsets.w + gas_index;
        let transition: ThermalTransition = due_transition(temperature, source, false);
        if !transition.is_due { continue; }
        let product_form: u32 = transition.product >> 30u;
        let product: u32 = thermal_index(transition.product, uniforms.thermal_offsets);
        // gas becomes another gas whole, and other matter one unit at a time
        if product_form == 3u {
            let product_amount: f32 = f32(amount) / AMOUNT_PER_ATMOSPHERE * transition.amount;
            let product_temperature: f32 = product_temperature(
                transition,
                temperature,
                f32(amount) / AMOUNT_PER_ATMOSPHERE,
                thermal_heat_capacity(source),
                product_amount,
                thermal_heat_capacity(product),
                false,
            );
            var emptied: vec4<u32> = slots;
            emptied[slot] = 0u;
            gas[index * 2u] = emptied;
            if !add_gas(
                index,
                transition.product & 0xffu,
                u32(round(product_amount * AMOUNT_PER_ATMOSPHERE)),
                product_temperature,
            ) {
                gas[index * 2u] = slots;
                atomicOr(&thermal_awake[tile], TILE_AWAKE);
                return false;
            }
            temperature_changes[index].y = bitcast<u32>(i32(round(
                f32(amount) / AMOUNT_PER_ATMOSPHERE * transition.latent_heat
                    * select(1.0, -1.0, transition.is_hot) * uniforms.heat_scale,
            )));
            atomicOr(&gas_awake[tile], 1u);
            return false;
        }
        let needed: f32 = AMOUNT_PER_ATMOSPHERE / transition.amount;
        if f32(amount) < needed { continue; }
        if product_form == 2u {
            list_conversion(index, 1u + slot);
            atomicOr(&thermal_awake[tile], TILE_AWAKE);
            return false;
        }
        let product_temperature: f32 = product_temperature(
            transition,
            temperature,
            needed / AMOUNT_PER_ATMOSPHERE,
            thermal_heat_capacity(source),
            1.0,
            thermal_heat_capacity(product),
            false,
        );
        var remaining: vec4<u32> = slots;
        let left: u32 = amount - min(u32(ceil(needed)), amount);
        remaining[slot] = select((gas_index << 24u) | left, 0u, left == 0u);
        gas[index * 2u] = remaining;
        let converted_material: u32 = conversion_debris(transition.product, cell);
        material_identifiers[index] = converted_material;
        colors[index] = transition_color(
            bitcast<u32>(gas_materials[gas_index * 2u].w),
            source,
            transition.is_hot,
            index ^ (uniforms.tick * 0x9e3779b9u),
        );
        matter[index] = vec2<u32>(bitcast<u32>(product_temperature), u32(AMOUNT_PER_UNIT));
        temperature_changes[index].y = bitcast<u32>(i32(round(
            needed / AMOUNT_PER_ATMOSPHERE * transition.latent_heat
                * select(1.0, -1.0, transition.is_hot) * uniforms.heat_scale,
        )));
        kinematics[index] = vec2<u32>(0u);
        // the new cell holds no phantom air, and no stored pressure yet
        gas[index * 2u + 1u].x = 0u;
        gas[index * 2u + 1u].w = 0u;
        atomicOr(&awake[tile], CELLULAR_TILE_AWAKE);
        atomicOr(&gas_awake[tile], 1u);
        return converted_material >> 30u == 0u;
    }
    return false;
}

/// Writes every stepped cell of an active tile with the temperature edits added, leaves its fluid
/// particles their change, changes the matter due for a transition, wakes the tile while its
/// temperatures change, and reports its static occupancy to collisions when its static cells
/// changed
@compute @workgroup_size(64)
fn apply(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_cell: u32,
) {
    let tile: vec2<i32> = active_tile(workgroup.x);
    let physical_tile: u32 = control.active_tiles[workgroup.x];
    if local_cell == 0u {
        let flags: u32 = atomicAnd(&thermal_awake[physical_tile], ~TILE_STATIC_CHANGED);
        atomicStore(&is_static_changed, flags & TILE_STATIC_CHANGED);
    }
    if local_cell < 2u { atomicStore(&static_occupancy[local_cell], 0u); }
    workgroupBarrier();
    let cell: vec2<i32> = tile * 8 + vec2<i32>(i32(local_cell % 8u), i32(local_cell / 8u));
    let index: u32 = cell_index(cell);
    let changes: vec2<u32> = temperature_changes[index];
    let edited: f32 = bitcast<f32>(changes.x);
    if any(changes != vec2<u32>(0u)) { temperature_changes[index] = vec2<u32>(0u); }
    let step: vec2<f32> = steps[index];
    let previous: f32 = bitcast<f32>(matter[index].x);
    let heat_capacity: f32 = max(cell_node(cell).heat_capacity, 1.0e-6);
    let added_heat: f32 = f32(bitcast<i32>(changes.y))
        / (uniforms.heat_scale * heat_capacity);
    let temperature: f32 = max(step.x + edited + added_heat, 0.0);
    matter[index].x = bitcast<u32>(temperature);
    fluid_heat[index].w = bitcast<i32>(step.y + edited + added_heat);
    if abs(temperature - previous) > uniforms.sleep_temperature
        || abs(step.y + edited + added_heat) > uniforms.sleep_temperature
    {
        atomicOr(&thermal_awake[physical_tile], TILE_AWAKE);
    }
    // matter past its transition's latent heat bank changes
    let material_identifier: u32 = material_identifiers[index];
    var is_changed: bool;
    if material_identifier != 0u {
        is_changed = convert_cell(index, physical_tile, cell, material_identifier, temperature);
    } else {
        is_changed = convert_gas(index, physical_tile, cell, temperature);
    }
    if is_changed { atomicStore(&is_static_changed, 1u); }
    let current: u32 = material_identifiers[index];
    if is_static(current) && atomicLoad(&rigid_body_cells[index]) == 0u {
        atomicOr(&static_occupancy[local_cell / 32u], 1u << (local_cell % 32u));
    }
    workgroupBarrier();
    // the tile and its remaining static cells, three words per listed tile
    if local_cell == 0u && atomicLoad(&is_static_changed) != 0u {
        let count: u32 = atomicAdd(&changed_cellular_tiles[0], 1u);
        if count * 3u + 3u < arrayLength(&changed_cellular_tiles) {
            atomicStore(&changed_cellular_tiles[count * 3u + 1u], physical_tile);
            atomicStore(
                &changed_cellular_tiles[count * 3u + 2u],
                atomicLoad(&static_occupancy[0]),
            );
            atomicStore(
                &changed_cellular_tiles[count * 3u + 3u],
                atomicLoad(&static_occupancy[1]),
            );
        }
        atomicOr(&thermal_awake[physical_tile], TILE_AWAKE);
    }
}
