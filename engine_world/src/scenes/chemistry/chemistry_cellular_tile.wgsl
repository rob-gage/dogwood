// Copyright Rob Gage 2026

struct ChemistryUniforms {
    region_width: u32,
    region_height: u32,
    ring_offset_x: u32,
    ring_offset_y: u32,
    /// First thermal index of the static, dynamic, fluid, and gas materials
    thermal_offsets: vec4<u32>,
    /// Number of thermal indices, the side of the pair lookup table
    thermal_count: u32,
    /// Offset in cells of this tick's blocks along both axes, zero or one
    block_offset: u32,
    /// Number of the tick, seeding the reactions' random rolls
    tick: u32,
    /// Duration of one tick in seconds
    time_step: f32,
    /// Phantom air amount of a cell at the scene's ambient pressure
    ambient_amount: f32,
    /// Fixed-point units per heat unit added to cells by changes of matter
    heat_scale: f32,
    /// Fixed-point units per heat capacity unit in the fluid heat
    fluid_capacity_scale: f32,
    /// Fixed-point units per heat unit in the fluid heat
    fluid_energy_scale: f32,
    /// Heat capacity of one gas amount unit of phantom air
    ambient_heat_capacity: f32,
    padding_0: u32,
    padding_1: u32,
    padding_2: u32,
};

/// A count of tiles, then their entries: the buffer indices of thermal's listed tiles for the
/// selection, and pairs of buffer index and reactive block mask for the reaction step
struct ChemistryTiles {
    active_tile_count: u32,
    active_tiles: array<u32>,
};

/// One cell a block reads: the cell's matter, gas, and fluid species, changed in place by the
/// block's reactions and written back when changed
struct ChemistryCell {
    /// Buffer index of the cell, or `CELL_OUTSIDE` outside the active region
    index: u32,
    /// Logical position of the cell in the active region
    position: vec2<i32>,
    material_identifier: u32,
    /// Temperature bits and amount of the cellular matter
    matter: vec2<u32>,
    /// Gas species slots
    slots: vec4<u32>,
    /// Phantom air amount
    phantom: u32,
    /// Fluid species: packed indices and the amounts left of each, measured against the raster's
    /// amounts when written back
    species: vec4<u32>,
    /// Mean temperature in kelvin of the cell's fluid particles
    fluid_temperature: f32,
    /// Fixed-point heat the block's reactions add to the cell
    heat: i32,
    /// Phantom air or stored pressure amount the block's reactions add to the cell
    pressure: u32,
    /// Which parts of the cell the block changed, of the `CHANGED_` flags
    changes: u32,
};

@group(0) @binding(0)
var<uniform> uniforms: ChemistryUniforms;

@group(0) @binding(1)
var<storage, read_write> material_identifiers: array<u32>;

@group(0) @binding(2)
var<storage, read_write> matter: array<vec2<u32>>;

@group(0) @binding(3)
var<storage, read_write> temperature_changes: array<vec2<u32>>;

@group(0) @binding(4)
var<storage, read_write> gas: array<vec4<u32>>;

@group(0) @binding(5)
var<storage, read_write> colors: array<vec2<u32>>;

@group(0) @binding(6)
var<storage, read_write> kinematics: array<vec2<u32>>;

@group(0) @binding(7)
var<storage, read_write> rigid_body_cells: array<atomic<u32>>;

@group(0) @binding(8)
var<storage, read_write> rigid_body_cellular_tiles: array<atomic<u32>>;

@group(0) @binding(9)
var<storage, read_write> changed_rigid_body_cells: array<atomic<u32>>;

@group(0) @binding(10)
var<storage, read_write> awake: array<atomic<u32>>;

@group(0) @binding(11)
var<storage, read_write> gas_awake: array<atomic<u32>>;

@group(0) @binding(12)
var<storage, read_write> thermal_awake: array<atomic<u32>>;

@group(0) @binding(13)
var<storage, read> tiles: ChemistryTiles;

@group(0) @binding(14)
var<storage, read> thermal_materials: array<vec4<u32>>;

@group(0) @binding(15)
var<storage, read> cellular_static_materials: array<vec4<f32>>;

@group(0) @binding(16)
var<storage, read> reactions: array<vec4<u32>>;

/// Color and radiance transformations of the reactions' products, read by the shared transition
/// helpers
@group(0) @binding(17)
var<storage, read> transition_colors: array<vec4<f32>>;

@group(0) @binding(18)
var<storage, read> reaction_lookup: array<u32>;

@group(0) @binding(19)
var<storage, read_write> fluid_species: array<vec4<u32>>;

@group(0) @binding(20)
var<storage, read> fluid_heat: array<vec4<i32>>;

@group(0) @binding(21)
var<storage, read_write> fluid_spawns: array<atomic<u32>>;

@group(0) @binding(22)
var<storage, read> pressures: array<f32>;

@group(0) @binding(23)
var<storage, read> fluid_materials: array<vec4<f32>>;

@group(0) @binding(24)
var<storage, read> gas_materials: array<vec4<f32>>;

/// The tiles with matter that can react this tick after their count, each as its buffer index
/// and a mask of its blocks that can react, which the selection fills and the reaction step
/// reads through `tiles`
@group(1) @binding(0)
var<storage, read_write> reacting_tiles: array<atomic<u32>>;

@group(1) @binding(1)
var<storage, read_write> dispatches: array<u32>;

/// Index of a cell outside the active region
const CELL_OUTSIDE: u32 = 0xffffffffu;

/// Unit of a cell's cellular matter; units 1 to 4 are its gas slots and 5 and 6 its fluid species
const UNIT_CELLULAR: u32 = 0u;

/// First unit of a cell's fluid species
const UNIT_FLUID: u32 = 5u;

/// Number of units a cell can hold
const UNIT_COUNT: u32 = 7u;

/// Thermal index of a missing unit
const UNIT_MISSING: u32 = 0xffffffffu;

/// The block changed the cell's cellular matter, color, or kinematics
const CHANGED_CELLULAR: u32 = 1u;

/// The block changed the cell's gas slots
const CHANGED_GAS: u32 = 2u;

/// The block consumed the cell's fluid species
const CHANGED_FLUID: u32 = 4u;

/// The block changed the cell's static occupancy
const CHANGED_STATIC: u32 = 8u;

/// The block added pressure to the cell
const CHANGED_PRESSURE: u32 = 16u;

/// Thermal tile flag: the tile's temperatures change
const THERMAL_TILE_AWAKE: u32 = 1u;

/// Thermal tile flag: a static cell of the tile changed, for thermal's apply to report its
/// occupancy to collisions
const THERMAL_TILE_STATIC_CHANGED: u32 = 2u;

/// Thermal tile flag: thermal listed the tile this tick, so its apply pass takes the changes
/// written into the tile
const THERMAL_TILE_LISTED: u32 = 4u;

/// Cellular tile flags waking the tile's dynamic cells
const CELLULAR_TILE_AWAKE: u32 = 3u;

/// Gas amount units per atmosphere
const AMOUNT_PER_ATMOSPHERE: f32 = 4096.0;

/// Largest amount of one gas slot, of phantom air, or of stored pressure
const SLOT_AMOUNT_MAXIMUM: u32 = 0xffffffu;

/// Amount units of one cellular cell or fluid particle
const AMOUNT_PER_UNIT: f32 = 65536.0;

/// Records of each reaction in `reactions`
const REACTION_RECORD_COUNT: u32 = 5u;

/// Words of each fluid spawn record after the count
const SPAWN_WORD_COUNT: u32 = 8u;

/// Heat capacity of a cell's gas and cellular matter below which reaction heat goes to its fluid
const HEAT_CAPACITY_MINIMUM: f32 = 0.01;

/// Reaction flag: the unit's material has reactions of its own
const FLAG_UNARY: u32 = 1u;

/// Reaction flag: the unit's material takes part in reactions with another
const FLAG_PAIRED: u32 = 2u;

/// Smallest extent of a reaction event worth applying
const EXTENT_MINIMUM: f32 = 1.0e-5;

/// The block's four cells, then the eight cells around it, two per block cell
var<private> cells: array<ChemistryCell, 12>;

/// Whether a reaction of the block was possible this tick, keeping its tile listed
var<private> is_reacting: bool;

/// Cells across the region a workgroup reads: the cells its blocks own and the ring around them
const REGION_SIZE: i32 = 10;

/// The classes of the units every cell of the workgroup's region holds, bit `thermal % 32` for
/// each unit's thermal index
var<workgroup> region_classes: array<u32, 100>;

/// The reaction flags and partner classes of every cell of the workgroup's region, of all its
/// units together
var<workgroup> region_reactions: array<vec2<u32>, 100>;

/// The blocks of the workgroup's tile that can react, one bit per block
var<workgroup> reactive_blocks: atomic<u32>;

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

/// Reads what a cell of the block or around it holds; a block cell of a tile thermal did not
/// list this tick counts as outside, so every change lands where this tick's thermal applies it
fn load_cell(slot: u32, position: vec2<i32>) {
    var cell: ChemistryCell;
    cell.position = position;
    cell.index = CELL_OUTSIDE;
    if is_cell_inside(position) && (slot >= 4u
        || (atomicLoad(&thermal_awake[cell_index(position) / 64u]) & THERMAL_TILE_LISTED) != 0u)
    {
        let index: u32 = cell_index(position);
        cell.index = index;
        cell.material_identifier = material_identifiers[index];
        cell.slots = gas[index * 2u];
        cell.species = fluid_species[index];
    }
    cells[slot] = cell;
}

/// Reads the temperatures and phantom air of a loaded cell, which only reacting blocks need
fn load_cell_heat(slot: u32) {
    let index: u32 = cells[slot].index;
    if index == CELL_OUTSIDE { return; }
    cells[slot].matter = matter[index];
    cells[slot].phantom = gas[index * 2u + 1u].x;
    if cells[slot].species.x != 0u {
        let heat: vec4<i32> = fluid_heat[index];
        let heat_capacity: f32 = f32(heat.x) / uniforms.fluid_capacity_scale;
        cells[slot].fluid_temperature = select(
            bitcast<f32>(cells[slot].matter.x),
            f32(heat.y) / uniforms.fluid_energy_scale / heat_capacity,
            heat_capacity > 0.0,
        );
    }
}

/// Returns the thermal index of a unit of a cell, or `UNIT_MISSING` when the cell lacks it
fn unit_thermal_index(slot: u32, unit: u32) -> u32 {
    if cells[slot].index == CELL_OUTSIDE { return UNIT_MISSING; }
    if unit == UNIT_CELLULAR {
        let material_identifier: u32 = cells[slot].material_identifier;
        if material_identifier == 0u { return UNIT_MISSING; }
        return thermal_index(material_identifier, uniforms.thermal_offsets);
    }
    if unit < UNIT_FLUID {
        let word: u32 = cells[slot].slots[unit - 1u];
        if (word & SLOT_AMOUNT_MAXIMUM) == 0u { return UNIT_MISSING; }
        return uniforms.thermal_offsets.w + (word >> 24u);
    }
    let fluid: u32 = (cells[slot].species.x >> ((unit - UNIT_FLUID) * 16u)) & 0xffffu;
    if fluid == 0u || cells[slot].species[1u + unit - UNIT_FLUID] == 0u { return UNIT_MISSING; }
    return uniforms.thermal_offsets.z + fluid - 1u;
}

/// Returns the reaction flags of a unit of a cell: `FLAG_UNARY` when it reacts alone and
/// `FLAG_PAIRED` when it reacts with another unit, or zero when it is missing
fn unit_flags(slot: u32, unit: u32) -> u32 {
    let thermal: u32 = unit_thermal_index(slot, unit);
    if thermal == UNIT_MISSING { return 0u; }
    let count: u32 = uniforms.thermal_count;
    return reaction_lookup[count * count + count + thermal];
}

/// Returns the amount of a unit, in units for cellular matter and fluids and in atmospheres for
/// gases
fn unit_amount(slot: u32, unit: u32) -> f32 {
    let cell: ChemistryCell = cells[slot];
    if unit == UNIT_CELLULAR { return f32(cell.matter.y) / AMOUNT_PER_UNIT; }
    if unit < UNIT_FLUID {
        return f32(cell.slots[unit - 1u] & SLOT_AMOUNT_MAXIMUM) / AMOUNT_PER_ATMOSPHERE;
    }
    return f32(cell.species[1u + unit - UNIT_FLUID]) / AMOUNT_PER_UNIT;
}

/// Returns the temperature in kelvin of a unit
fn unit_temperature(slot: u32, unit: u32, thermal_index: u32) -> f32 {
    let cell: ChemistryCell = cells[slot];
    if unit == UNIT_CELLULAR {
        return effective_temperature(bitcast<f32>(cell.matter.x), thermal_index);
    }
    if unit < UNIT_FLUID { return bitcast<f32>(cell.matter.x); }
    return cell.fluid_temperature;
}

/// Returns the pressure in atmospheres in a cell, never below vacuum
fn cell_pressure(slot: u32) -> f32 {
    return max(
        uniforms.ambient_amount / AMOUNT_PER_ATMOSPHERE + pressures[cells[slot].index],
        0.0,
    );
}

/// Returns the heat capacity of the cellular matter, gases, and phantom air a block cell holds
/// now, which share its temperature
fn cell_heat_capacity(slot: u32) -> f32 {
    let cell: ChemistryCell = cells[slot];
    var heat_capacity: f32 = f32(cell.phantom) * uniforms.ambient_heat_capacity;
    if cell.material_identifier != 0u {
        heat_capacity += thermal_heat_capacity(
            thermal_index(cell.material_identifier, uniforms.thermal_offsets),
        ) * f32(cell.matter.y) / AMOUNT_PER_UNIT;
    }
    for (var gas_slot: u32 = 0u; gas_slot < 4u; gas_slot++) {
        let amount: u32 = cell.slots[gas_slot] & SLOT_AMOUNT_MAXIMUM;
        if amount == 0u { continue; }
        heat_capacity += thermal_heat_capacity(uniforms.thermal_offsets.w
            + (cell.slots[gas_slot] >> 24u)) * f32(amount) / AMOUNT_PER_ATMOSPHERE;
    }
    return heat_capacity;
}

/// Returns the heat capacity of the fluid species a block cell holds now
fn fluid_heat_capacity(slot: u32) -> f32 {
    let cell: ChemistryCell = cells[slot];
    var heat_capacity: f32 = 0.0;
    for (var species: u32 = 0u; species < 2u; species++) {
        let fluid: u32 = (cell.species.x >> (species * 16u)) & 0xffffu;
        if fluid == 0u { continue; }
        heat_capacity += thermal_heat_capacity(uniforms.thermal_offsets.z + fluid - 1u)
            * f32(cell.species[1u + species]) / AMOUNT_PER_UNIT;
    }
    return heat_capacity;
}

/// Returns the packed color and radiance a unit gives a product taking its place
fn unit_color(slot: u32, unit: u32) -> vec2<u32> {
    let cell: ChemistryCell = cells[slot];
    if unit == UNIT_CELLULAR { return colors[cell.index]; }
    if unit < UNIT_FLUID {
        let gas_index: u32 = cell.slots[unit - 1u] >> 24u;
        return vec2<u32>(
            bitcast<u32>(gas_materials[gas_index * 2u].w),
            bitcast<u32>(gas_materials[gas_index * 2u + 1u].y),
        );
    }
    let fluid: u32 = ((cell.species.x >> ((unit - UNIT_FLUID) * 16u)) & 0xffffu) - 1u;
    return vec2<u32>(bitcast<u32>(fluid_materials[fluid * 2u + 1u].w), 0u);
}

/// Returns whether a unit is one of a cell's gas slots
fn is_gas_unit(unit: u32) -> bool {
    return unit != UNIT_CELLULAR && unit < UNIT_FLUID;
}

/// Returns a random number from zero to one of a seed
fn chemistry_random(seed: u32) -> f32 {
    return transition_random(seed) * 0.5 + 0.5;
}

/// Lowers a unit's amount by a positive amount, emptying the unit when nothing is left
fn consume(slot: u32, unit: u32, amount: f32) {
    if unit == UNIT_CELLULAR {
        let used: u32 = min(u32(round(amount * AMOUNT_PER_UNIT)), cells[slot].matter.y);
        cells[slot].matter.y -= used;
        cells[slot].changes |= CHANGED_CELLULAR;
        if cells[slot].matter.y == 0u {
            if is_static(cells[slot].material_identifier) {
                cells[slot].changes |= CHANGED_STATIC;
            }
            cells[slot].material_identifier = 0u;
        }
    } else if unit < UNIT_FLUID {
        let word: u32 = cells[slot].slots[unit - 1u];
        let held: u32 = word & SLOT_AMOUNT_MAXIMUM;
        let left: u32 = held - min(u32(round(amount * AMOUNT_PER_ATMOSPHERE)), held);
        cells[slot].slots[unit - 1u] = select((word & ~SLOT_AMOUNT_MAXIMUM) | left, 0u, left == 0u);
        cells[slot].changes |= CHANGED_GAS;
    } else {
        let species: u32 = 1u + unit - UNIT_FLUID;
        let held: u32 = cells[slot].species[species];
        cells[slot].species[species] = held - min(u32(round(amount * AMOUNT_PER_UNIT)), held);
        cells[slot].changes |= CHANGED_FLUID;
    }
}

/// Returns the slots of a cell with an amount of a gas added, or the slots unchanged with the
/// fourth gas index set to `UNIT_MISSING` when no slot holds it
fn added_gas(slots: vec4<u32>, gas_index: u32, amount: u32) -> vec4<u32> {
    var target_slot: u32 = 4u;
    for (var slot: u32 = 0u; slot < 4u; slot++) {
        let held: u32 = slots[slot] & SLOT_AMOUNT_MAXIMUM;
        if held > 0u && slots[slot] >> 24u == gas_index {
            target_slot = slot;
        } else if held == 0u && target_slot == 4u {
            target_slot = slot;
        }
    }
    var result: vec4<u32> = slots;
    if target_slot == 4u { return vec4<u32>(UNIT_MISSING); }
    let held: u32 = slots[target_slot] & SLOT_AMOUNT_MAXIMUM;
    if amount > SLOT_AMOUNT_MAXIMUM - held { return vec4<u32>(UNIT_MISSING); }
    result[target_slot] = (gas_index << 24u) | (held + amount);
    return result;
}

/// Wakes a tile of a cell for the systems its changes concern
fn wake_cell(slot: u32) {
    let changes: u32 = cells[slot].changes;
    let tile: u32 = cells[slot].index / 64u;
    if (changes & CHANGED_CELLULAR) != 0u { atomicOr(&awake[tile], CELLULAR_TILE_AWAKE); }
    if (changes & (CHANGED_GAS | CHANGED_PRESSURE | CHANGED_CELLULAR)) != 0u {
        atomicOr(&gas_awake[tile], 1u);
    }
    atomicOr(&thermal_awake[tile], THERMAL_TILE_AWAKE
        | select(0u, THERMAL_TILE_STATIC_CHANGED, (changes & CHANGED_STATIC) != 0u));
}

/// Tries one event of a reaction between its reactants, the second missing when its slot is
/// `CELL_OUTSIDE`, at a rate multiplied by the number of ticks between the contact's chances:
/// checks the reaction's temperature and pressure, rolls for the event, then consumes the
/// reactants by the event's extent and places its products in the reactants' cells of the block
fn react(
    reaction: u32,
    first_slot: u32,
    first_unit: u32,
    second_slot: u32,
    second_unit: u32,
    rate_scale: f32,
) {
    let base: u32 = reaction * REACTION_RECORD_COUNT;
    let header: vec4<u32> = reactions[base];
    let ranges: vec4<f32> = bitcast<vec4<f32>>(reactions[base + 1u]);
    let amounts: vec2<f32> = bitcast<vec2<f32>>(reactions[base + 2u].xy);
    let reactant_count: u32 = header.w & 3u;
    let product_count: u32 = (header.w >> 2u) & 3u;
    let slots: array<u32, 2> = array<u32, 2>(first_slot, second_slot);
    let units: array<u32, 2> = array<u32, 2>(first_unit, second_unit);
    // the reactants' mean temperature and pressure meet the reaction's ranges
    var temperature: f32 = 0.0;
    var pressure: f32 = 0.0;
    var extent: f32 = 1.0;
    for (var reactant: u32 = 0u; reactant < reactant_count; reactant++) {
        let thermal: u32 = unit_thermal_index(slots[reactant], units[reactant]);
        if thermal == UNIT_MISSING { return; }
        temperature += unit_temperature(slots[reactant], units[reactant], thermal);
        pressure += cell_pressure(slots[reactant]);
        if amounts[reactant] > 0.0 {
            extent = min(extent, unit_amount(slots[reactant], units[reactant]) / amounts[reactant]);
        }
    }
    temperature /= f32(reactant_count);
    pressure /= f32(reactant_count);
    if temperature < ranges.x || temperature > ranges.y || pressure < ranges.z
        || pressure > ranges.w || extent < EXTENT_MINIMUM
    { return; }
    is_reacting = true;
    let position: vec2<i32> = cells[first_slot].position;
    let seed: u32 = (u32(position.x) * 73856093u) ^ (u32(position.y) * 19349663u)
        ^ (uniforms.tick * 0x9e3779b9u) ^ (reaction * 2654435761u)
        ^ ((first_unit * 8u + second_unit) * 83492791u);
    let probability: f32 =
        1.0 - exp(-bitcast<f32>(header.x) * rate_scale * uniforms.time_step);
    if chemistry_random(seed) >= probability { return; }
    // products, heat, and pressure go where the consumed reactants were, never to a catalyst: gas
    // into the first consumed gas reactant's cell or the first consumed reactant's, and other
    // matter into cells the reaction leaves without cellular matter
    var gas_slot: u32 = CELL_OUTSIDE;
    var first_consumed: u32 = CELL_OUTSIDE;
    for (var reactant: u32 = 0u; reactant < reactant_count; reactant++) {
        if slots[reactant] >= 4u || amounts[reactant] <= 0.0 { continue; }
        if first_consumed == CELL_OUTSIDE { first_consumed = slots[reactant]; }
        if gas_slot == CELL_OUTSIDE && is_gas_unit(units[reactant]) { gas_slot = slots[reactant]; }
    }
    if first_consumed == CELL_OUTSIDE { return; }
    if gas_slot == CELL_OUTSIDE { gas_slot = first_consumed; }
    // a cell is free for cellular matter when it holds none, or its reactant is used up
    var is_free: array<bool, 2>;
    for (var reactant: u32 = 0u; reactant < reactant_count; reactant++) {
        let slot: u32 = slots[reactant];
        if slot >= 4u { continue; }
        var has_cellular: bool = cells[slot].material_identifier != 0u;
        if units[reactant] == UNIT_CELLULAR && has_cellular && amounts[reactant] > 0.0 {
            has_cellular = u32(round(extent * amounts[reactant] * AMOUNT_PER_UNIT))
                < cells[slot].matter.y;
        }
        is_free[reactant] = !has_cellular;
    }
    // every product fits before anything changes
    var gas_slots: vec4<u32> = cells[gas_slot].slots;
    for (var reactant: u32 = 0u; reactant < reactant_count; reactant++) {
        if slots[reactant] == gas_slot && is_gas_unit(units[reactant]) && amounts[reactant] > 0.0
        {
            let unit_slot: u32 = units[reactant] - 1u;
            let held: u32 = gas_slots[unit_slot] & SLOT_AMOUNT_MAXIMUM;
            let left: u32 = held
                - min(u32(round(extent * amounts[reactant] * AMOUNT_PER_ATMOSPHERE)), held);
            gas_slots[unit_slot] = select(
                (gas_slots[unit_slot] & ~SLOT_AMOUNT_MAXIMUM) | left, 0u, left == 0u,
            );
        }
    }
    var product_slots: array<u32, 3>;
    var product_sources: array<u32, 3>;
    var fluid_count: u32 = 0u;
    for (var product: u32 = 0u; product < product_count; product++) {
        let record: vec4<u32> = reactions[base + 3u + product / 2u];
        let identifier: u32 = select(record.x, record.z, product % 2u == 1u);
        let amount: f32 = bitcast<f32>(select(record.y, record.w, product % 2u == 1u)) * extent;
        let form: u32 = identifier >> 30u;
        // fluid particles are made whole, so a reaction making them waits for whole events
        if form == 2u && extent < 1.0 { return; }
        if form == 3u {
            let gas_amount: u32 = u32(round(amount * AMOUNT_PER_ATMOSPHERE));
            if gas_amount == 0u { continue; }
            gas_slots = added_gas(gas_slots, identifier & 0xffu, gas_amount);
            if gas_slots.w == UNIT_MISSING { return; }
            continue;
        }
        // other products take the place of a reactant: cellular matter needs a free cell, and
        // fluid particles a record of the spawn list
        if form == 2u { fluid_count += 1u; }
        var chosen: u32 = 4u;
        for (var reactant: u32 = 0u; reactant < reactant_count; reactant++) {
            if slots[reactant] >= 4u || chosen != 4u { continue; }
            if form == 2u || is_free[reactant] { chosen = reactant; }
        }
        if chosen == 4u { return; }
        if form < 2u { is_free[chosen] = false; }
        product_slots[product] = slots[chosen];
        product_sources[product] = chosen;
    }
    // a rigid body cell the event uses up leaves its body, one per event so a full change list
    // never leaves half an event
    var rigid_slot: u32 = CELL_OUTSIDE;
    for (var reactant: u32 = 0u; reactant < reactant_count; reactant++) {
        let slot: u32 = slots[reactant];
        if slot >= 4u || units[reactant] != UNIT_CELLULAR || amounts[reactant] <= 0.0 { continue; }
        if u32(round(extent * amounts[reactant] * AMOUNT_PER_UNIT)) < cells[slot].matter.y {
            continue;
        }
        if atomicLoad(&rigid_body_cells[cells[slot].index]) == 0u { continue; }
        if rigid_slot != CELL_OUTSIDE { return; }
        rigid_slot = slot;
    }
    // the fluid particles' spawn records are reserved before anything changes
    var spawn_record: u32 = 0u;
    if fluid_count > 0u {
        spawn_record = atomicAdd(&fluid_spawns[0], fluid_count);
        if 1u + (spawn_record + fluid_count) * SPAWN_WORD_COUNT > arrayLength(&fluid_spawns) {
            atomicSub(&fluid_spawns[0], fluid_count);
            return;
        }
    }
    if rigid_slot != CELL_OUTSIDE && !remove_rigid_body_cell(cells[rigid_slot].index) {
        // the reserved records keep no material, spawning nothing
        for (var record: u32 = 0u; record < fluid_count; record++) {
            atomicStore(&fluid_spawns[1u + (spawn_record + record) * SPAWN_WORD_COUNT + 1u], 0u);
        }
        return;
    }
    // the products take their colors from the reactants before these are consumed
    var product_colors: array<vec2<u32>, 3>;
    for (var product: u32 = 0u; product < product_count; product++) {
        if reactions[base + 3u + product / 2u][(product % 2u) * 2u] >> 30u == 3u { continue; }
        let reactant: u32 = product_sources[product];
        product_colors[product] = unit_color(slots[reactant], units[reactant]);
    }
    for (var reactant: u32 = 0u; reactant < reactant_count; reactant++) {
        if slots[reactant] >= 4u || amounts[reactant] <= 0.0 { continue; }
        if slots[reactant] == gas_slot && is_gas_unit(units[reactant])
        { continue; }
        consume(slots[reactant], units[reactant], extent * amounts[reactant]);
    }
    if any(gas_slots != cells[gas_slot].slots) {
        cells[gas_slot].slots = gas_slots;
        cells[gas_slot].changes |= CHANGED_GAS;
    }
    for (var product: u32 = 0u; product < product_count; product++) {
        let record: vec4<u32> = reactions[base + 3u + product / 2u];
        let identifier: u32 = select(record.x, record.z, product % 2u == 1u);
        let amount: f32 = bitcast<f32>(select(record.y, record.w, product % 2u == 1u)) * extent;
        let form: u32 = identifier >> 30u;
        if form == 3u { continue; }
        let slot: u32 = product_slots[product];
        let color_base: u32 = (reaction * 3u + product) * 14u;
        let color: u32 = transformed_color(product_colors[product].x, color_base, seed + product);
        let radiance: u32 =
            transformed_radiance(product_colors[product].y, color_base + 7u, seed + product);
        if form == 2u {
            // fluid particles are spawned by the fluid simulation next tick
            let record_base: u32 = 1u + spawn_record * SPAWN_WORD_COUNT;
            spawn_record += 1u;
            atomicStore(&fluid_spawns[record_base], cells[slot].index);
            atomicStore(&fluid_spawns[record_base + 1u], identifier);
            atomicStore(
                &fluid_spawns[record_base + 2u],
                max(u32(round(amount * AMOUNT_PER_UNIT)), 1u),
            );
            atomicStore(&fluid_spawns[record_base + 3u], bitcast<u32>(temperature));
            atomicStore(&fluid_spawns[record_base + 4u], color);
            atomicStore(&fluid_spawns[record_base + 5u], radiance);
            continue;
        }
        let material: u32 = conversion_debris(identifier, cells[slot].position);
        cells[slot].material_identifier = material;
        cells[slot].matter.y = max(u32(round(amount * AMOUNT_PER_UNIT)), 1u);
        colors[cells[slot].index] = vec2<u32>(color, radiance);
        kinematics[cells[slot].index] = vec2<u32>(0u);
        cells[slot].changes |= CHANGED_CELLULAR
            | select(0u, CHANGED_STATIC, is_static(material));
    }
    // heat spreads over the consumed reactants' cells by the heat capacity left in them, so a
    // cell the reaction empties takes none
    let heat: f32 = bitcast<f32>(header.y) * extent;
    if heat != 0.0 {
        var capacities: array<f32, 2>;
        var capacity_sum: f32 = 0.0;
        for (var reactant: u32 = 0u; reactant < reactant_count; reactant++) {
            if slots[reactant] >= 4u || amounts[reactant] <= 0.0 { continue; }
            if reactant == 1u && slots[1] == slots[0] { continue; }
            capacities[reactant] =
                cell_heat_capacity(slots[reactant]) + fluid_heat_capacity(slots[reactant]);
            capacity_sum += capacities[reactant];
        }
        for (var reactant: u32 = 0u; reactant < reactant_count; reactant++) {
            if capacities[reactant] <= 0.0 { continue; }
            cells[slots[reactant]].heat +=
                i32(round(heat * capacities[reactant] / capacity_sum * uniforms.heat_scale));
        }
    }
    let added_pressure: u32 = u32(round(bitcast<f32>(header.z) * extent * AMOUNT_PER_ATMOSPHERE));
    if added_pressure > 0u {
        cells[gas_slot].pressure += added_pressure;
        cells[gas_slot].changes |= CHANGED_PRESSURE;
    }
}

/// Tries every reaction of a lookup entry between two units, swapping them for reactions listing
/// the second unit first; catalytic contacts react only with the outside unit as the catalyst
fn react_pair(first_slot: u32, first_unit: u32, second_slot: u32, second_unit: u32, scale: f32) {
    let first: u32 = unit_thermal_index(first_slot, first_unit);
    let second: u32 = unit_thermal_index(second_slot, second_unit);
    if first == UNIT_MISSING || second == UNIT_MISSING { return; }
    let count: u32 = uniforms.thermal_count;
    let flags: u32 = count * count + count;
    if (reaction_lookup[flags + first] & reaction_lookup[flags + second] & FLAG_PAIRED) == 0u {
        return;
    }
    let entry: u32 = reaction_lookup[first * count + second];
    for (var item: u32 = 0u; item < (entry & 0xffu); item++) {
        let word: u32 = reaction_lookup[(entry >> 8u) + item];
        let reaction: u32 = word & 0x7fffffffu;
        let is_swapped: bool = (word >> 31u) != 0u;
        let amounts: vec2<f32> =
            bitcast<vec2<f32>>(reactions[reaction * REACTION_RECORD_COUNT + 2u].xy);
        // the second unit's amount in this reaction, and the first's
        let second_amount: f32 = select(amounts.y, amounts.x, is_swapped);
        let first_amount: f32 = select(amounts.x, amounts.y, is_swapped);
        var rate_scale: f32 = scale;
        if second_slot >= 4u {
            // outside the block, only a catalyst the first unit reacts with
            if second_amount > 0.0 || first_amount <= 0.0 { continue; }
        } else if first_amount == 0.0 || second_amount == 0.0 {
            // catalytic contacts react every tick, inside or outside the block
            rate_scale = 1.0;
        }
        if is_swapped {
            react(reaction, second_slot, second_unit, first_slot, first_unit, rate_scale);
        } else {
            react(reaction, first_slot, first_unit, second_slot, second_unit, rate_scale);
        }
        // a missing unit reacts no further
        if unit_thermal_index(first_slot, first_unit) == UNIT_MISSING
            || unit_thermal_index(second_slot, second_unit) == UNIT_MISSING
        { return; }
    }
}

/// Tries the reactions of every unit of a block cell alone and with the other units in it
fn react_cell(slot: u32) {
    for (var unit: u32 = 0u; unit < UNIT_COUNT; unit++) {
        if (unit_flags(slot, unit) & FLAG_UNARY) == 0u { continue; }
        let thermal: u32 = unit_thermal_index(slot, unit);
        let entry: u32 = reaction_lookup[
            uniforms.thermal_count * uniforms.thermal_count + thermal
        ];
        for (var item: u32 = 0u; item < (entry & 0xffu); item++) {
            if unit_thermal_index(slot, unit) == UNIT_MISSING { break; }
            react(reaction_lookup[(entry >> 8u) + item], slot, unit, CELL_OUTSIDE, 0u, 1.0);
        }
    }
    for (var first: u32 = 0u; first < UNIT_COUNT; first++) {
        if (unit_flags(slot, first) & FLAG_PAIRED) == 0u { continue; }
        for (var second: u32 = first + 1u; second < UNIT_COUNT; second++) {
            react_pair(slot, first, slot, second, 1.0);
        }
    }
}

/// Tries the reactions across a face between two cells of the block, whose two-sided reactions
/// get their chance every second tick
fn react_face(first_slot: u32, second_slot: u32) {
    if cells[first_slot].index == CELL_OUTSIDE || cells[second_slot].index == CELL_OUTSIDE {
        return;
    }
    for (var first: u32 = 0u; first < UNIT_COUNT; first++) {
        if (unit_flags(first_slot, first) & FLAG_PAIRED) == 0u { continue; }
        for (var second: u32 = 0u; second < UNIT_COUNT; second++) {
            react_pair(first_slot, first, second_slot, second, 2.0);
        }
    }
}

/// Writes a changed cell of the block back and wakes its tile
fn store_cell(slot: u32) {
    let cell: ChemistryCell = cells[slot];
    if cell.index == CELL_OUTSIDE { return; }
    if (cell.changes & CHANGED_CELLULAR) != 0u {
        let previous: u32 = material_identifiers[cell.index];
        material_identifiers[cell.index] = cell.material_identifier;
        matter[cell.index].y = cell.matter.y;
        if cell.material_identifier == 0u {
            colors[cell.index] = vec2<u32>(0u);
            kinematics[cell.index] = vec2<u32>(0u);
        }
        // a used up cell releases its stored pressure, and a new cell holds no phantom air
        if cell.material_identifier != previous { gas[cell.index * 2u + 1u].w = 0u; }
        if previous == 0u && cell.material_identifier != 0u { gas[cell.index * 2u + 1u].x = 0u; }
    }
    if (cell.changes & CHANGED_GAS) != 0u { gas[cell.index * 2u] = cell.slots; }
    if (cell.changes & CHANGED_PRESSURE) != 0u {
        var words: vec4<u32> = gas[cell.index * 2u + 1u];
        if is_static(cell.material_identifier) {
            let stored: i32 = bitcast<i32>(words.w);
            words.w = bitcast<u32>(min(stored + i32(cell.pressure), i32(SLOT_AMOUNT_MAXIMUM)));
        } else {
            var total: u32 = words.x;
            for (var gas_slot: u32 = 0u; gas_slot < 4u; gas_slot++) {
                total += cell.slots[gas_slot] & SLOT_AMOUNT_MAXIMUM;
            }
            words.x += min(cell.pressure, SLOT_AMOUNT_MAXIMUM - min(total, SLOT_AMOUNT_MAXIMUM));
        }
        gas[cell.index * 2u + 1u] = words;
    }
    if (cell.changes & CHANGED_FLUID) != 0u {
        // the fluid particles lose the fraction of each species the reactions consumed
        let original: vec4<u32> = fluid_species[cell.index];
        var consumed: u32 = 0u;
        for (var species: u32 = 0u; species < 2u; species++) {
            let held: u32 = original[1u + species];
            if held == 0u { continue; }
            let left: u32 = cell.species[1u + species];
            let fraction: u32 = select(
                min(u32(round(f32(held - left) / f32(held) * 65535.0)), 65534u),
                65535u,
                left == 0u,
            );
            consumed |= fraction << (species * 16u);
        }
        fluid_species[cell.index].w = consumed;
    }
    // reaction heat warms the gas and cellular matter of the cell, like a flame above a cooler
    // liquid, or else its fluid particles through thermal
    if cell.heat != 0 {
        let heat_capacity: f32 = cell_heat_capacity(slot);
        if heat_capacity >= HEAT_CAPACITY_MINIMUM {
            let heat: f32 = f32(cell.heat) / uniforms.heat_scale;
            matter[cell.index].x = bitcast<u32>(max(
                bitcast<f32>(matter[cell.index].x) + heat / heat_capacity,
                0.0,
            ));
        } else {
            let previous: i32 = bitcast<i32>(temperature_changes[cell.index].y);
            temperature_changes[cell.index].y = bitcast<u32>(previous + cell.heat);
        }
    }
    if cell.changes != 0u || cell.heat != 0 { wake_cell(slot); }
}

/// Returns the position of a cell of a block: its four cells, then for each of them its outside
/// neighbor across x and its outside neighbor across y
fn block_cell_position(origin: vec2<i32>, slot: u32) -> vec2<i32> {
    if slot < 4u { return origin + vec2<i32>(i32(slot % 2u), i32(slot / 2u)); }
    let block_slot: u32 = (slot - 4u) / 2u;
    let offset: vec2<i32> = vec2<i32>(i32(block_slot % 2u), i32(block_slot / 2u));
    return origin + offset + select(
        vec2<i32>(0, offset.y * 2 - 1),
        vec2<i32>(offset.x * 2 - 1, 0),
        slot % 2u == 0u,
    );
}

/// Returns the thermal indices of what a cell holds, `UNIT_MISSING` for missing units, reading
/// only its material, gas slots, and fluid species; a block cell of a tile thermal did not list
/// this tick holds nothing
fn position_units(position: vec2<i32>, is_owned: bool) -> array<u32, 7> {
    var units: array<u32, 7>;
    for (var unit: u32 = 0u; unit < UNIT_COUNT; unit++) { units[unit] = UNIT_MISSING; }
    if !is_cell_inside(position) { return units; }
    let index: u32 = cell_index(position);
    if is_owned
        && (atomicLoad(&thermal_awake[index / 64u]) & THERMAL_TILE_LISTED) == 0u
    { return units; }
    let material_identifier: u32 = material_identifiers[index];
    if material_identifier != 0u {
        units[UNIT_CELLULAR] = thermal_index(material_identifier, uniforms.thermal_offsets);
    }
    let slots: vec4<u32> = gas[index * 2u];
    for (var slot: u32 = 0u; slot < 4u; slot++) {
        if (slots[slot] & SLOT_AMOUNT_MAXIMUM) != 0u {
            units[1u + slot] = uniforms.thermal_offsets.w + (slots[slot] >> 24u);
        }
    }
    let species: u32 = fluid_species[index].x;
    for (var slot: u32 = 0u; slot < 2u; slot++) {
        let fluid: u32 = (species >> (slot * 16u)) & 0xffffu;
        if fluid != 0u { units[UNIT_FLUID + slot] = uniforms.thermal_offsets.z + fluid - 1u; }
    }
    return units;
}

/// Returns the logical position of a tile from its buffer index
fn listed_tile(physical_tile: u32) -> vec2<i32> {
    let physical: vec2<u32> =
        vec2<u32>(physical_tile % uniforms.region_width, physical_tile / uniforms.region_width);
    return vec2<i32>(
        i32((physical.x + uniforms.region_width - uniforms.ring_offset_x) % uniforms.region_width),
        i32((physical.y + uniforms.region_height - uniforms.ring_offset_y)
            % uniforms.region_height),
    );
}

/// Lists every tile thermal listed whose blocks hold matter that can react, with a mask of those
/// blocks: a unit reacting alone, or a unit with a partner class in its cell or a side neighbor,
/// which may rarely count a unit whose partner only shares a class. One invocation reads each
/// cell the tile's blocks own, which shift back by the block offset, and the first 36 the ring
/// around them; most tiles, like stone beside stone, end here
@compute @workgroup_size(64)
fn select_tiles(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    let physical_tile: u32 = tiles.active_tiles[workgroup.x];
    let owned_origin: vec2<i32> = listed_tile(physical_tile) * 8 - i32(uniforms.block_offset);
    if local_index == 0u { atomicStore(&reactive_blocks, 0u); }
    let count: u32 = uniforms.thermal_count;
    for (var region_index: u32 = local_index; region_index < 100u; region_index += 64u) {
        let region_position: vec2<i32> = vec2<i32>(
            i32(region_index) % REGION_SIZE,
            i32(region_index) / REGION_SIZE,
        );
        let is_owned: bool = all(region_position >= vec2<i32>(1))
            && all(region_position <= vec2<i32>(8));
        let units: array<u32, 7> =
            position_units(owned_origin - 1 + region_position, is_owned);
        var classes: u32 = 0u;
        var reactions: vec2<u32> = vec2<u32>(0u);
        for (var unit: u32 = 0u; unit < UNIT_COUNT; unit++) {
            let thermal: u32 = units[unit];
            if thermal == UNIT_MISSING { continue; }
            classes |= 1u << (thermal % 32u);
            reactions |= vec2<u32>(
                reaction_lookup[count * count + count + thermal],
                reaction_lookup[count * count + 2u * count + thermal],
            );
        }
        region_classes[region_index] = classes;
        region_reactions[region_index] = reactions;
    }
    workgroupBarrier();
    let region_index: u32 = (local_index / 8u + 1u) * u32(REGION_SIZE) + local_index % 8u + 1u;
    let block_bit: u32 = 1u << ((local_index / 16u) * 4u + (local_index % 8u) / 2u);
    let nearby: u32 = region_classes[region_index]
        | region_classes[region_index - 1u] | region_classes[region_index + 1u]
        | region_classes[region_index - u32(REGION_SIZE)]
        | region_classes[region_index + u32(REGION_SIZE)];
    let reactions: vec2<u32> = region_reactions[region_index];
    if (reactions.x & FLAG_UNARY) != 0u || (reactions.y & nearby) != 0u {
        atomicOr(&reactive_blocks, block_bit);
    }
    workgroupBarrier();
    let blocks: u32 = atomicLoad(&reactive_blocks);
    if local_index == 0u && blocks != 0u {
        let entry: u32 = 1u + 2u * atomicAdd(&reacting_tiles[0], 1u);
        atomicStore(&reacting_tiles[entry], physical_tile);
        atomicStore(&reacting_tiles[entry + 1u], blocks);
    }
}

/// Reacting tiles one workgroup reacts, sixteen invocations each
const TILES_PER_WORKGROUP: u32 = 4u;

/// Pads the reacting tiles with tiles of no reactive blocks to whole workgroups, writes the
/// indirect dispatch of their workgroups, then empties the list for the next tick's selection
@compute @workgroup_size(1)
fn prepare_reactions() {
    let count: u32 = atomicLoad(&reacting_tiles[0]);
    let padded: u32 = (count + TILES_PER_WORKGROUP - 1u) / TILES_PER_WORKGROUP * TILES_PER_WORKGROUP;
    for (var entry: u32 = count; entry < padded; entry++) {
        atomicStore(&reacting_tiles[2u + 2u * entry], 0u);
    }
    dispatches[0] = padded / TILES_PER_WORKGROUP;
    dispatches[1] = 1u;
    dispatches[2] = 1u;
    atomicStore(&reacting_tiles[0], 0u);
}

/// Reacts the matter of every block of a reacting tile: one invocation owns the four cells of a
/// block, whose blocks shift by one cell along both axes every other tick, and tries their
/// reactions alone, within each cell, across the block's inner faces, and with catalysts in the
/// cells around the block, keeping the tile listed while a reaction is possible
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
    for (var slot: u32 = 0u; slot < 12u; slot++) {
        load_cell(slot, block_cell_position(origin, slot));
        load_cell_heat(slot);
    }
    is_reacting = false;
    for (var slot: u32 = 0u; slot < 4u; slot++) {
        if cells[slot].index != CELL_OUTSIDE { react_cell(slot); }
    }
    react_face(0u, 1u);
    react_face(2u, 3u);
    react_face(0u, 2u);
    react_face(1u, 3u);
    for (var slot: u32 = 0u; slot < 4u; slot++) {
        if cells[slot].index == CELL_OUTSIDE { continue; }
        for (var side: u32 = 0u; side < 2u; side++) {
            let neighbor: u32 = 4u + slot * 2u + side;
            if cells[neighbor].index == CELL_OUTSIDE { continue; }
            for (var unit: u32 = 0u; unit < UNIT_COUNT; unit++) {
                if (unit_flags(slot, unit) & FLAG_PAIRED) == 0u { continue; }
                for (var neighbor_unit: u32 = 0u; neighbor_unit < UNIT_COUNT; neighbor_unit++) {
                    react_pair(slot, unit, neighbor, neighbor_unit, 1.0);
                }
            }
        }
    }
    for (var slot: u32 = 0u; slot < 4u; slot++) { store_cell(slot); }
    if is_reacting { atomicOr(&thermal_awake[physical_tile], THERMAL_TILE_AWAKE); }
}
