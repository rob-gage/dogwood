// Copyright Rob Gage 2026

struct PressureUniforms {
    region_width: u32,
    region_height: u32,
    ring_offset_x: u32,
    ring_offset_y: u32,
    ambient_amount: u32,
    ambient_mass: f32,
    gravity: f32,
    fluid_raster_scale: f32,
    actor_count: u32,
    /// Actor velocity per unit of the actor records' velocity, in cells per substep
    actor_velocity_scale: f32,
    /// Flow added per substep for each unit of pressure difference between open cells, whose
    /// square root is the speed of pressure waves through gas in cells per substep
    gas_response: f32,
    /// Fraction of each face flow lost per substep
    flow_damping: f32,
    /// Flow added per substep for each unit of difference in the net outflow of the cells across
    /// a face, damping pressure waves without damping swirls
    bulk_viscosity: f32,
    /// Flows smaller than this many amount units stop, so resting cells sleep
    flow_minimum: f32,
    /// Fraction of phantom air exchanged with each neighboring cell per substep
    ambient_diffusivity: f32,
    /// Substeps of each tick, dividing gas diffusivities given per tick
    substep_count: f32,
    /// Fraction of the difference between a face flow and an actor's velocity removed per substep
    actor_drag: f32,
    /// Flow added per substep through a face of solid cells of unit mass for each unit of
    /// pressure difference
    solid_response: f32,
    /// Speed of pressure waves through solid cells in cells per substep
    solid_speed: f32,
    /// Fraction of a solid cell's stored pressure that relaxes away per substep
    solid_relaxation: f32,
    /// Fraction of the gas trapped in a solid cell pushed out through its open faces per substep
    trapped_expulsion: f32,
    /// Integrity a solid cell loses per substep for each atmosphere its load exceeds its
    /// strength, per unit of its material's absorption
    damage_rate: f32,
    /// Whether the open volume pass measures only the listed tiles instead of every tile
    is_volume_listed: u32,
    /// Factor of the gas diffusivities for the size of a pressure cell
    diffusion_scale: f32,
    /// Fraction of a cell's gas pressure beyond `RELAXATION_TOLERANCE` that relaxes away per
    /// substep
    open_relaxation: f32,
    /// Smallest gas pressure a cell relaxes per substep, in amount units per open volume
    open_relaxation_minimum: f32,
    padding_0: u32,
    padding_1: u32,
    padding_2: u32,
    padding_3: u32,
};

/// An actor displacing gas, laid out like the cellular tile actor records
struct PressureActor {
    center: vec2<f32>,
    velocity: vec2<f32>,
    rotation: vec2<f32>,
    size: vec2<f32>,
    bounds: vec4<f32>,
    shape: u32,
    mass: f32,
    padding: vec2<u32>,
};

/// Active tile count, the count of tiles whose open volumes are measured, the count of blocks
/// whose halo holds gas this substep, and the count of blocks of 2x2 tiles holding an active
/// tile, then the active tiles followed, each after room for every tile of the region, by the
/// measured tiles, which the blocks replace once measured, and the blocks holding gas, then four
/// words per pressure cell, `COARSE_CONSTANT_START`. Blocks are listed by their bottom-left tile
struct PressureControl {
    active_tile_count: atomic<u32>,
    volume_tile_count: atomic<u32>,
    gas_block_count: atomic<u32>,
    block_count: atomic<u32>,
    active_tiles: array<u32>,
};

/// One cell's gas and stored pressure with the properties derived from them
struct PressureCell {
    slots: vec4<u32>,
    phantom: u32,
    flow: vec2<f32>,
    /// Fraction of the cell open to gas, zero when closed or outside the active region
    volume: f32,
    /// Whether the cell is inside or next to an actor, whose velocity drags the gas
    is_dragged: bool,
    /// Velocity through the cell's right and top faces of the gas actors drag, in cells per
    /// substep
    drag: vec2<f32>,
    /// Whether the cell is inside the active region and in a tile simulated this tick
    is_active: bool,
    /// Gas amount per open volume, treating closed and nearly full cells as a quarter open, or for
    /// a solid cell the ambient amount plus its stored pressure
    pressure: f32,
    /// Mass per open volume, or for a solid cell its material's mass
    density: f32,
    /// Total gas amount, trapped in a closed cell
    total: f32,
    /// Owner of a solid cell in bits 8 and up, one for the cellular tiles and the rigid body index
    /// plus two for a rigid body, and its material's absorption in the low eight bits, zero for
    /// a cell that stores no pressure
    solid: u32,
    /// Stored pressure of a solid cell above the ambient pressure, in amount units per atmosphere
    stored: i32,
};

@group(0) @binding(0)
var<uniform> uniforms: PressureUniforms;

@group(0) @binding(1)
var<storage, read_write> material_identifiers: array<u32>;

@group(0) @binding(2)
var<storage, read> gas_in: array<vec4<u32>>;

@group(0) @binding(3)
var<storage, read_write> gas_out: array<vec4<u32>>;

@group(0) @binding(4)
var<storage, read_write> pressure_awake: array<atomic<u32>>;

@group(0) @binding(5)
var<storage, read_write> volumes: array<vec2<u32>>;

@group(0) @binding(6)
var<storage, read_write> control: PressureControl;

@group(0) @binding(7)
var<storage, read> gas_materials: array<vec4<f32>>;

@group(0) @binding(8)
var<storage, read> fluid_raster: array<u32>;

@group(0) @binding(9)
var<storage, read> fluid_materials: array<vec4<f32>>;

@group(0) @binding(10)
var<storage, read> actors: array<PressureActor>;

@group(0) @binding(11)
var<storage, read> static_materials: array<vec4<f32>>;

@group(0) @binding(12)
var<storage, read> rigid_body_cells: array<u32>;

/// Rigid body tile records as eight words each, laid out like the collision rigid body tile
/// records: the rigid body index, padding, the tile position, then the occupied and previously
/// rasterized cell masks
@group(0) @binding(13)
var<storage, read_write> rigid_body_cellular_tiles: array<atomic<u32>>;

@group(0) @binding(14)
var<storage, read_write> pressures: array<f32>;

@group(0) @binding(15)
var<storage, read_write> cellular_awake: array<atomic<u32>>;

@group(0) @binding(16)
var<storage, read_write> kinematics: array<vec2<u32>>;

@group(0) @binding(17)
var<storage, read_write> broken_cellular_tiles: array<atomic<u32>>;

@group(0) @binding(18)
var<storage, read_write> colors: array<u32>;

@group(0) @binding(19)
var<storage, read_write> broken_rigid_body_cells: array<atomic<u32>>;

@group(1) @binding(0)
var<storage, read_write> dispatches: array<u32>;

// Pressure is simulated on pressure cells of 2x2 cells, rebuilt from the cells at the start of
// each tick by `coarsen` and spread back over them by `refine`. A pressure tile of 8x8 pressure
// cells covers a block of 2x2 tiles, and a step workgroup simulates one, keeping little per cell
// in its halo and deriving the rest when read, because the workgroup memory of each workgroup
// limits how many run at once

/// Pressure cells across a tile
const PRESSURE_CELLS_PER_TILE: u32 = 4u;

/// Pressure cells across a pressure tile
const BLOCK_SIZE: u32 = 8u;

/// Pressure cells across the pressure tile and the three rings around it, which every face and
/// transfer the pressure tile depends on reads
const HALO_SIZE: u32 = 14u;

/// Pressure cells across the pressure tile and the two rings around it, whose new flows the
/// transfers read
const HALO_FLOW_SIZE: u32 = 12u;

/// Pressure cells across the pressure tile and the ring around it, whose transfers its cells read
const HALO_TRANSFER_SIZE: u32 = 10u;

/// Invocations of a step workgroup: one per pressure cell of its pressure tile and forty more
/// that only help load the halo and compute the flows and transfers around it
const STEP_WORKGROUP_SIZE: u32 = 104u;

/// Lists in the control words before the four words of each pressure cell that stay fixed through
/// a tick's substeps: its volume word, packed drag, solid word, and density bits
const COARSE_CONSTANT_START: u32 = 3u;

/// Flag of a pressure cell's volume word for an open pressure cell holding static cells, whose
/// gas presses on them
const VOLUME_PARTIAL: u32 = 0x20000000u;

/// Phantom air, flow bits, and open volume words with the activity flag of the cells in the halo
var<workgroup> halo_words: array<vec4<u32>, 196>;

/// Solid owner and absorption, then stored pressure bits, of the cells in the halo
var<workgroup> halo_solids: array<vec2<u32>, 196>;

/// Density of the cells in the halo
var<workgroup> halo_densities: array<f32, 196>;

/// New right and top face flows of the pressure tile and the two rings around it
var<workgroup> halo_flows: array<vec2<f32>, 144>;

/// Limited fractions given through the left, right, bottom, and top faces of the pressure tile and
/// the ring around it, sixteen bits each since no fraction reaches one
var<workgroup> halo_transfers: array<vec2<u32>, 100>;

/// Gases admitted into free slots by the pressure tile and the ring around it
var<workgroup> halo_admissions: array<u32, 100>;

/// Cell position of the halo's bottom-left cell
var<private> halo_origin: vec2<i32>;

/// Whether each tile of the 4x4 tiles around the workgroup's block is simulated this tick, which
/// stays fixed through the tick's substeps
var<workgroup> halo_tile_active: array<u32, 16>;

/// Whether any halo cell holds gas in a species slot
var<workgroup> halo_gas: atomic<u32>;

/// Whether any halo cell holds gas, read uniformly by the whole workgroup
var<workgroup> halo_gas_uniform: u32;

/// Whether no halo cell holds gas, so nothing is admitted and no species slot moves
var<private> is_halo_gas_free: bool;

/// Largest change of a cell's phantom air in one substep that still lets its tile sleep, so faint
/// residual pressure stops diffusing once it is too weak to matter instead of keeping wide areas
/// of open air awake
const SLEEP_PHANTOM_CHANGE_MAXIMUM: u32 = 64u;

/// Smallest face flow that keeps a tile awake or wakes a sleeping neighbor, below which a flow
/// moves no whole amount unit through its face
const SLEEP_FLOW_MINIMUM: f32 = 16.0;

/// Tile flag: the tile's gas, stored pressure, or open volume changed, so it and its neighbors
/// are simulated
const TILE_AWAKE: u32 = 1u;

/// Tile flag: the tile is simulated this tick
const TILE_ACTIVE: u32 = 4u;

/// Tile flag: a static cell of the tile lost all its integrity and breaks after the substeps
const TILE_BROKEN: u32 = 8u;

/// Tile flag: fluid or a dragging actor reached the tile or a neighbor when open volumes were
/// last measured, so they are measured again next tick
const TILE_VOLUME_DEPENDENT: u32 = 16u;

/// Tile flag: the halo of a listed block's pressure tile reaches the tile, so its pressure cells
/// are rebuilt this tick
const TILE_HALO: u32 = 32u;

/// Cellular tile flags of tiles whose cells may have changed this tick: awake, disturbed, or
/// simulated, matching the cellular tiles
const CELLULAR_TILE_CHANGED: u32 = 7u;

/// Whether fluid or a dragging actor reached any cell of the workgroup's tile
var<workgroup> volume_dependence: atomic<u32>;

/// Integrity damage of a static cell that breaks, in the low sixteen bits of its kinematics
const INTEGRITY_BROKEN: u32 = 0xffffu;

/// Velocity units of one cell per tick in packed dynamic cell kinematics, matching the cellular
/// tiles
const VELOCITY_SCALE: f32 = 1024.0;

/// Largest dynamic cell speed in cells per tick, matching the cellular tiles
const SPEED_MAXIMUM: f32 = 8.0;

/// Impulse in mass times cells per tick through one cell face per atmosphere over one tick,
/// matching the cellular tile and fluid shaders
const PRESSURE_IMPULSE: f32 = 0.1111;

/// Gas amount in one atmosphere
const AMOUNT_PER_ATMOSPHERE: f32 = 4096.0;

/// Largest amount of gas or stored pressure in one cell, `PRESSURE_MAXIMUM` atmospheres
const AMOUNT_MAXIMUM: u32 = 16773120u;

/// Transfer fraction units of a whole cell
const TRANSFER_SCALE: f32 = 65536.0;

/// Open volume units of a whole cell
const VOLUME_SCALE: f32 = 65536.0;

/// Open volume word flag: the cell is inside or next to an actor dragging its gas
const VOLUME_DRAGGED: u32 = 0x80000000u;

/// Flag of a halo cell's open volume word for a cell in a tile simulated this tick
const VOLUME_ACTIVE: u32 = 0x40000000u;

/// Distance from an actor's core, in multiples of its radius, within which it drags gas
const ACTOR_DRAG_RANGE: f32 = 6.0;

/// Fraction of an actor's velocity at which the middle of its wake follows it
const ACTOR_WAKE: f32 = 1.0;

/// Distance behind an actor's core, in multiples of its radius, of the middle of its eddies
const ACTOR_WAKE_OFFSET: f32 = 3.0;

/// Length of an actor's eddies along its path, in multiples of its radius
const ACTOR_WAKE_LENGTH: f32 = 3.0;

/// Largest actor velocity in cells per tick, matching the cellular tile actor records
const ACTOR_VELOCITY_MAXIMUM: f32 = 8.0;

/// Smoothed fluid occupancy above which a cell's gas volume follows the fluid's unoccupied fraction
const FLUID_OCCUPANCY_CLOSED: f32 = 0.5;

/// Smallest open volume used for pressure, so closed cells push their gas out without dividing
/// by zero
const VOLUME_MINIMUM: f32 = 0.25;

/// Largest speed in cells per substep that moves flows along themselves
const ADVECTION_SPEED_MAXIMUM: f32 = 0.9;

/// Largest fraction of a cell's gas its flows move out per substep
const BULK_FRACTION_MAXIMUM: f32 = 0.5;

/// Solid owner of the cellular tiles' own static cells
const SOLID_TERRAIN: u32 = 1u;

/// Cellular tile flag waking the tile's dynamic cells, matching `CELLULAR_TILE_AWAKE`
const CELLULAR_TILE_AWAKE: u32 = 1u;

/// Pressure in atmospheres above or below the ambient pressure within which gas stops relaxing,
/// well below what pushes dynamic cells, so the faint exchange of air with solid surfaces sleeps
const RELAXATION_TOLERANCE: f32 = 0.01;

/// Pressure in atmospheres above or below the ambient pressure that pushes dynamic cells, so
/// pressure at rest never wakes them
const PRESSURE_PUSH_MINIMUM: f32 = 0.02;

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

/// Returns whether a cell position lies in a tile simulated this tick
fn is_cell_active(cell: vec2<i32>) -> bool {
    if !is_cell_inside(cell) { return false; }
    return (atomicLoad(&pressure_awake[tile_index(vec2<u32>(cell) / 8u)]) & TILE_ACTIVE) != 0u;
}

/// Returns whether a pressure cell position lies inside the active region
fn is_coarse_inside(cell: vec2<i32>) -> bool {
    return cell.x >= 0 && cell.y >= 0
        && cell.x < i32(uniforms.region_width * PRESSURE_CELLS_PER_TILE)
        && cell.y < i32(uniforms.region_height * PRESSURE_CELLS_PER_TILE);
}

/// Returns the buffer index of a pressure cell inside the active region, laid out by position in
/// the region since pressure cells are rebuilt every tick
fn coarse_index(cell: vec2<i32>) -> u32 {
    return u32(cell.y) * uniforms.region_width * PRESSURE_CELLS_PER_TILE + u32(cell.x);
}

/// Returns the tile holding a pressure cell
fn coarse_tile(cell: vec2<i32>) -> vec2<u32> { return vec2<u32>(cell) / PRESSURE_CELLS_PER_TILE; }

/// Returns the index in the control words of a pressure cell's fixed words
fn coarse_constant_index(index: u32) -> u32 {
    return uniforms.region_width * uniforms.region_height * COARSE_CONSTANT_START + index * 4u;
}

/// Returns a pressure cell's fixed words: its volume word, packed drag, solid word, and density
fn coarse_constants(index: u32) -> vec4<u32> {
    let base: u32 = coarse_constant_index(index);
    return vec4<u32>(
        control.active_tiles[base],
        control.active_tiles[base + 1u],
        control.active_tiles[base + 2u],
        control.active_tiles[base + 3u],
    );
}

/// Reads one pressure cell's gas and stored pressure with its fixed words and derives its
/// pressure and density, treating pressure cells outside the active region as closed and empty
fn load_coarse_cell(cell: vec2<i32>, is_active: bool) -> PressureCell {
    var pressure_cell: PressureCell;
    if !is_coarse_inside(cell) { return pressure_cell; }
    let index: u32 = coarse_index(cell);
    let words: vec4<u32> = gas_in[index * 2u + 1u];
    pressure_cell.slots = gas_in[index * 2u];
    pressure_cell.phantom = words.x;
    pressure_cell.flow = vec2<f32>(bitcast<f32>(words.y), bitcast<f32>(words.z));
    let constants: vec4<u32> = coarse_constants(index);
    pressure_cell.volume = f32(constants.x & 0x1ffffu) / VOLUME_SCALE;
    pressure_cell.is_dragged = (constants.x & VOLUME_DRAGGED) != 0u;
    pressure_cell.drag = unpack2x16snorm(constants.y) * ACTOR_VELOCITY_MAXIMUM
        * uniforms.actor_velocity_scale;
    pressure_cell.is_active = is_active;
    var mass: f32 = f32(pressure_cell.phantom) * uniforms.ambient_mass;
    var total: f32 = f32(pressure_cell.phantom);
    for (var slot: u32 = 0u; slot < 4u; slot++) {
        let amount: f32 = f32(slot_amount(pressure_cell.slots[slot]));
        if amount <= 0.0 { continue; }
        mass += amount * gas_materials[slot_gas(pressure_cell.slots[slot]) * 2u].x;
        total += amount;
    }
    let volume: f32 = max(pressure_cell.volume, VOLUME_MINIMUM);
    pressure_cell.total = total;
    pressure_cell.pressure = total / volume;
    pressure_cell.density = mass / volume;
    // a pressure cell of only static cells carries stored pressure instead
    if constants.z != 0u {
        pressure_cell.solid = constants.z;
        pressure_cell.volume = 0.0;
        pressure_cell.stored = bitcast<i32>(words.w);
        pressure_cell.pressure = f32(uniforms.ambient_amount) + f32(pressure_cell.stored);
        pressure_cell.density = bitcast<f32>(constants.w);
    }
    return pressure_cell;
}

/// Returns the gas index held by a species slot
fn slot_gas(slot: u32) -> u32 { return slot >> 24u; }

/// Returns the amount held by a species slot, zero for a free slot
fn slot_amount(slot: u32) -> u32 { return slot & 0xffffffu; }

/// Returns the amount moved by a transfer fraction, rounded down exactly so both cells of a face
/// agree and no cell gives more than it holds
fn transfer(amount: u32, fraction: u32) -> u32 {
    return (amount >> 16u) * fraction + (((amount & 0xffffu) * fraction) >> 16u);
}

/// Returns whether a material identifier is a static material, whose cells store pressure
fn is_static(material_identifier: u32) -> bool {
    return material_identifier != 0u && material_identifier >> 30u == 0u;
}

/// Returns a solid cell's absorption per substep, from its material's absorption per cell crossed
fn solid_absorption(solid: u32) -> f32 {
    let absorption: f32 = f32(solid & 0xffu) / 255.0;
    return 1.0 - pow(1.0 - absorption, uniforms.solid_speed);
}

/// Returns the amount of gas a solid cell of a mass takes to raise its stored pressure by one
/// unit, so that pressure crosses every solid at the solid wave speed
fn solid_capacity(mass: f32) -> f32 {
    return uniforms.solid_response / (mass * uniforms.solid_speed * uniforms.solid_speed);
}

/// Reads one cell's gas and stored pressure and derives its pressure and density, treating cells
/// outside the active region as closed and empty
fn load_cell(cell: vec2<i32>) -> PressureCell {
    return load_cell_with_activity(cell, is_cell_active(cell));
}

/// Returns a static cell's solid owner and absorption, as `PressureCell.solid` holds them
fn static_solid(index: u32, material: vec4<f32>) -> u32 {
    let rigid_body_cell: u32 = rigid_body_cells[index];
    var owner: u32 = SOLID_TERRAIN;
    if rigid_body_cell != 0u {
        owner = atomicLoad(&rigid_body_cellular_tiles[(rigid_body_cell - 1u) / 64u * 8u]) + 2u;
    }
    return (owner << 8u) | u32(round(saturate(material.y) * 255.0));
}

/// Reads one cell like `load_cell`, given whether it lies in a tile simulated this tick
fn load_cell_with_activity(cell: vec2<i32>, is_active: bool) -> PressureCell {
    var pressure_cell: PressureCell;
    if !is_cell_inside(cell) { return pressure_cell; }
    let index: u32 = cell_index(cell);
    let words: vec4<u32> = gas_in[index * 2u + 1u];
    pressure_cell.slots = gas_in[index * 2u];
    pressure_cell.phantom = words.x;
    pressure_cell.flow = vec2<f32>(bitcast<f32>(words.y), bitcast<f32>(words.z));
    let volume_word: vec2<u32> = volumes[index];
    pressure_cell.volume = f32(volume_word.x & ~VOLUME_DRAGGED) / VOLUME_SCALE;
    pressure_cell.is_dragged = (volume_word.x & VOLUME_DRAGGED) != 0u;
    pressure_cell.drag = unpack2x16snorm(volume_word.y) * ACTOR_VELOCITY_MAXIMUM
        * uniforms.actor_velocity_scale;
    pressure_cell.is_active = is_active;
    var mass: f32 = f32(pressure_cell.phantom) * uniforms.ambient_mass;
    var total: f32 = f32(pressure_cell.phantom);
    for (var slot: u32 = 0u; slot < 4u; slot++) {
        let amount: f32 = f32(slot_amount(pressure_cell.slots[slot]));
        if amount <= 0.0 { continue; }
        mass += amount * gas_materials[slot_gas(pressure_cell.slots[slot]) * 2u].x;
        total += amount;
    }
    let volume: f32 = max(pressure_cell.volume, VOLUME_MINIMUM);
    pressure_cell.total = total;
    pressure_cell.pressure = total / volume;
    pressure_cell.density = mass / volume;
    // static cells, of the cellular tiles or of a rigid body, carry stored pressure instead
    let material_identifier: u32 = material_identifiers[index];
    if is_static(material_identifier) {
        let material: vec4<f32> = static_materials[material_identifier & 0x3fffffffu];
        let rigid_body_cell: u32 = rigid_body_cells[index];
        var owner: u32 = SOLID_TERRAIN;
        if rigid_body_cell != 0u {
            owner = atomicLoad(&rigid_body_cellular_tiles[(rigid_body_cell - 1u) / 64u * 8u]) + 2u;
        }
        pressure_cell.solid = (owner << 8u) | u32(round(saturate(material.y) * 255.0));
        // a cell turned solid since its open volume was measured is closed all the same
        pressure_cell.volume = 0.0;
        pressure_cell.stored = bitcast<i32>(words.w);
        pressure_cell.pressure = f32(uniforms.ambient_amount) + f32(pressure_cell.stored);
        pressure_cell.density = material.x;
    }
    return pressure_cell;
}

/// Reads the pressure cells of a workgroup's pressure tile, listed by the bottom-left tile of its
/// block, and the rings around it into the halo once, so every neighbor read after it is local,
/// and returns the invocation's pressure cell position
fn load_halo(tile: u32, local_cell: u32) -> vec2<i32> {
    let tile_position: vec2<u32> =
        vec2<u32>(tile % uniforms.region_width, tile / uniforms.region_width);
    halo_origin = vec2<i32>(tile_position * PRESSURE_CELLS_PER_TILE) - 3;
    // the halo reaches three cells into the tiles around, whose activity each block reads once
    if local_cell < 16u {
        let neighbor_tile: vec2<i32> = vec2<i32>(tile_position)
            + vec2<i32>(i32(local_cell % 4u), i32(local_cell / 4u)) - 1;
        halo_tile_active[local_cell] = select(
            0u,
            atomicLoad(&pressure_awake[tile_index(vec2<u32>(max(neighbor_tile, vec2<i32>(0))))])
                & TILE_ACTIVE,
            all(neighbor_tile >= vec2<i32>(0))
                && all(neighbor_tile < vec2<i32>(i32(uniforms.region_width), i32(uniforms.region_height))),
        );
    }
    if local_cell == 0u { atomicStore(&halo_gas, 0u); }
    workgroupBarrier();
    for (var halo_index: u32 = local_cell; halo_index < HALO_SIZE * HALO_SIZE;
        halo_index += STEP_WORKGROUP_SIZE)
    {
        let halo_position: vec2<u32> = vec2<u32>(halo_index % HALO_SIZE, halo_index / HALO_SIZE);
        // three pressure cells into the tile before the block's first
        let tile_offset: vec2<u32> = (halo_position + 1u) / PRESSURE_CELLS_PER_TILE;
        let pressure_cell: PressureCell = load_coarse_cell(
            halo_origin + vec2<i32>(halo_position),
            halo_tile_active[tile_offset.y * 4u + tile_offset.x] != 0u,
        );
        if any(pressure_cell.slots != vec4<u32>(0u)) { atomicOr(&halo_gas, 1u); }
        halo_words[halo_index] = vec4<u32>(
            pressure_cell.phantom,
            bitcast<u32>(pressure_cell.flow.x),
            bitcast<u32>(pressure_cell.flow.y),
            u32(pressure_cell.volume * VOLUME_SCALE)
                | select(0u, VOLUME_DRAGGED, pressure_cell.is_dragged)
                | select(0u, VOLUME_ACTIVE, pressure_cell.is_active),
        );
        halo_densities[halo_index] = pressure_cell.density;
        halo_solids[halo_index] =
            vec2<u32>(pressure_cell.solid, bitcast<u32>(pressure_cell.stored));
    }
    workgroupBarrier();
    return halo_origin + 3
        + vec2<i32>(i32(local_cell % BLOCK_SIZE), i32(local_cell / BLOCK_SIZE));
}

/// Returns the halo index of a cell within three cells of the workgroup's block
fn halo_index(cell: vec2<i32>) -> u32 {
    let local: vec2<u32> = vec2<u32>(cell - halo_origin);
    return local.y * HALO_SIZE + local.x;
}

/// Returns the index of a cell within two cells of the workgroup's block in the new flows
fn halo_flow_index(cell: vec2<i32>) -> u32 {
    let local: vec2<u32> = vec2<u32>(cell - halo_origin - 1);
    return local.y * HALO_FLOW_SIZE + local.x;
}

/// Returns the index of a cell within one cell of the workgroup's block in the transfers
fn halo_transfer_index(cell: vec2<i32>) -> u32 {
    let local: vec2<u32> = vec2<u32>(cell - halo_origin - 2);
    return local.y * HALO_TRANSFER_SIZE + local.x;
}

/// Returns the species slots of a halo cell, read from the gas buffer since a halo without gas
/// has none
fn halo_slots(cell: vec2<i32>) -> vec4<u32> {
    if is_halo_gas_free || !is_coarse_inside(cell) { return vec4<u32>(0u); }
    return gas_in[coarse_index(cell) * 2u];
}

/// Returns the limited fractions a cell within one cell of the block gives through each face
fn halo_transfer_fractions(cell: vec2<i32>) -> vec4<u32> {
    let packed: vec2<u32> = halo_transfers[halo_transfer_index(cell)];
    return vec4<u32>(packed.x & 0xffffu, packed.x >> 16u, packed.y & 0xffffu, packed.y >> 16u);
}

/// Returns one cell of the halo, deriving its total amount and pressure as `load_cell` does
fn halo_cell(cell: vec2<i32>) -> PressureCell {
    let index: u32 = halo_index(cell);
    let words: vec4<u32> = halo_words[index];
    let solids: vec2<u32> = halo_solids[index];
    var pressure_cell: PressureCell;
    pressure_cell.slots = halo_slots(cell);
    pressure_cell.phantom = words.x;
    pressure_cell.flow = vec2<f32>(bitcast<f32>(words.y), bitcast<f32>(words.z));
    pressure_cell.volume = f32(words.w & ~(VOLUME_DRAGGED | VOLUME_ACTIVE)) / VOLUME_SCALE;
    pressure_cell.is_dragged = (words.w & VOLUME_DRAGGED) != 0u;
    // only dragged cells use their drag
    if pressure_cell.is_dragged {
        pressure_cell.drag = unpack2x16snorm(coarse_constants(coarse_index(cell)).y)
            * ACTOR_VELOCITY_MAXIMUM * uniforms.actor_velocity_scale;
    }
    pressure_cell.is_active = (words.w & VOLUME_ACTIVE) != 0u;
    pressure_cell.density = halo_densities[index];
    pressure_cell.solid = solids.x;
    pressure_cell.stored = bitcast<i32>(solids.y);
    var total: f32 = f32(pressure_cell.phantom);
    for (var slot: u32 = 0u; slot < 4u; slot++) {
        let amount: f32 = f32(slot_amount(pressure_cell.slots[slot]));
        if amount <= 0.0 { continue; }
        total += amount;
    }
    pressure_cell.total = total;
    pressure_cell.pressure = total / max(pressure_cell.volume, VOLUME_MINIMUM);
    if pressure_cell.solid != 0u {
        pressure_cell.pressure = f32(uniforms.ambient_amount) + f32(pressure_cell.stored);
    }
    return pressure_cell;
}

/// Returns whether pressure crosses the face between two cells as stored pressure: between solid
/// cells of the same owner, and between a solid cell and an open cell
fn is_stress_face(first: PressureCell, second: PressureCell) -> bool {
    if first.solid != 0u && second.solid != 0u { return first.solid >> 8u == second.solid >> 8u; }
    return (first.solid != 0u && second.volume > 0.0) || (second.solid != 0u && first.volume > 0.0);
}

/// Returns a halo cell's flow through its right face from the previous substep, zero outside the
/// active region
fn flow_x(cell: vec2<i32>) -> f32 { return bitcast<f32>(halo_words[halo_index(cell)].y); }

/// Returns a halo cell's flow through its top face from the previous substep, zero outside the
/// active region
fn flow_y(cell: vec2<i32>) -> f32 { return bitcast<f32>(halo_words[halo_index(cell)].z); }

/// Samples right-face flows at a position in cells, where the right face of cell (i, j) is at
/// (i + 1, j + 0.5)
fn sample_flow_x(position: vec2<f32>) -> f32 {
    let grid: vec2<f32> = position - vec2<f32>(1.0, 0.5);
    let base: vec2<i32> = vec2<i32>(floor(grid));
    let fraction: vec2<f32> = grid - floor(grid);
    let bottom: f32 = mix(flow_x(base), flow_x(base + vec2<i32>(1, 0)), fraction.x);
    let top: f32 =
        mix(flow_x(base + vec2<i32>(0, 1)), flow_x(base + vec2<i32>(1, 1)), fraction.x);
    return mix(bottom, top, fraction.y);
}

/// Samples top-face flows at a position in cells, where the top face of cell (i, j) is at
/// (i + 0.5, j + 1)
fn sample_flow_y(position: vec2<f32>) -> f32 {
    let grid: vec2<f32> = position - vec2<f32>(0.5, 1.0);
    let base: vec2<i32> = vec2<i32>(floor(grid));
    let fraction: vec2<f32> = grid - floor(grid);
    let bottom: f32 = mix(flow_y(base), flow_y(base + vec2<i32>(1, 0)), fraction.x);
    let top: f32 =
        mix(flow_y(base + vec2<i32>(0, 1)), flow_y(base + vec2<i32>(1, 1)), fraction.x);
    return mix(bottom, top, fraction.y);
}

/// Returns the flow through the face from one cell to its neighbor after a substep: carried
/// along by the surrounding flows, pushed by the pressure difference, and on vertical faces
/// lifted or sunk by the difference from the ambient gas density, so ambient gas at rest stays
/// at rest
fn face_flow(
    giver: PressureCell,
    receiver: PressureCell,
    flow: f32,
    face: vec2<f32>,
    cross_flow: f32,
    divergence_difference: f32,
    is_vertical: bool,
) -> f32 {
    if giver.volume <= 0.0 && receiver.volume <= 0.0 { return 0.0; }
    let mean_pressure: f32 = 0.5 * (giver.pressure + receiver.pressure);
    var carried: f32 = flow;
    if mean_pressure > 0.0 {
        // move the flow along itself, measuring speed as flow per amount
        var velocity: vec2<f32> = select(
            vec2<f32>(flow, cross_flow),
            vec2<f32>(cross_flow, flow),
            is_vertical,
        ) / mean_pressure;
        let speed: f32 = length(velocity);
        if speed > ADVECTION_SPEED_MAXIMUM { velocity *= ADVECTION_SPEED_MAXIMUM / speed; }
        carried = select(
            sample_flow_x(face - velocity),
            sample_flow_y(face - velocity),
            is_vertical,
        );
    }
    var next: f32 = carried + uniforms.gas_response * (giver.pressure - receiver.pressure)
        + uniforms.bulk_viscosity * divergence_difference;
    let mean_density: f32 = 0.5 * (giver.density + receiver.density);
    if is_vertical && mean_density > 0.0 {
        let ambient_density: f32 = f32(uniforms.ambient_amount) * uniforms.ambient_mass;
        next += mean_pressure * uniforms.gravity * (ambient_density - mean_density) / mean_density;
    }
    // actors drag the open gas around them along the flow they make, which the giving cell
    // holds for its own right and top faces
    if giver.is_dragged && giver.volume > 0.0 && receiver.volume > 0.0 {
        next = mix(
            next,
            select(giver.drag.x, giver.drag.y, is_vertical) * mean_pressure,
            uniforms.actor_drag,
        );
    }
    next *= 1.0 - uniforms.flow_damping;
    // nothing flows into a closed cell
    if (receiver.volume <= 0.0 && next > 0.0) || (giver.volume <= 0.0 && next < 0.0) { next = 0.0; }
    if abs(next) < uniforms.flow_minimum { next = 0.0; }
    return next;
}

/// Returns the flow through a face touching a solid cell after a substep: pushed by the pressure
/// difference against the mean inertia of both cells, which grows with a solid's mass so heavy
/// solids reflect more of the pressure reaching them, and damped by the solid's absorption
fn stress_flow(giver: PressureCell, receiver: PressureCell, flow: f32) -> f32 {
    var giver_inertia: f32 = 1.0 / uniforms.gas_response;
    var receiver_inertia: f32 = 1.0 / uniforms.gas_response;
    var absorption: f32 = 0.0;
    if giver.solid != 0u {
        giver_inertia = giver.density / uniforms.solid_response;
        absorption = solid_absorption(giver.solid);
    }
    if receiver.solid != 0u {
        receiver_inertia = receiver.density / uniforms.solid_response;
        absorption = max(absorption, solid_absorption(receiver.solid));
    }
    var next: f32 = flow
        + (giver.pressure - receiver.pressure) * 2.0 / (giver_inertia + receiver_inertia);
    next *= max(1.0 - uniforms.flow_damping - absorption, 0.0);
    if abs(next) < uniforms.flow_minimum { next = 0.0; }
    return next;
}

/// Returns the stored pressure a solid cell gains from an amount of gas or stored pressure
/// arriving through its faces, or loses for a negative amount
fn stored_change(solid: PressureCell, amount: i32) -> i32 {
    return i32(round(f32(amount) / solid_capacity(solid.density)));
}

/// Returns the amount of stored pressure a solid cell gives through a face for a flow out of it,
/// never so much that its pressure falls below a vacuum or an open cell overfills, and zero from
/// any other cell
fn stored_amount(giver: PressureCell, receiver: PressureCell, flow: f32) -> i32 {
    if giver.solid == 0u || flow <= 0.0 { return 0; }
    var amount: f32 = min(
        trunc(flow),
        (f32(giver.stored) + f32(uniforms.ambient_amount)) * solid_capacity(giver.density),
    );
    // each face may fill an open cell by a quarter of its room, so four faces never overfill it
    if receiver.solid == 0u {
        amount = min(amount, max(f32(AMOUNT_MAXIMUM) - receiver.total, 0.0) * 0.25);
    }
    return i32(max(amount, 0.0));
}

/// Returns whether a gas amount may move from one cell into another: only into open cells that
/// hold the gas or admitted it this substep, and only from clouds dense enough for its cohesion
/// unless the receiving cell already holds it or the giving cell is closed
fn is_transfer_allowed(
    giver: PressureCell,
    receiver: PressureCell,
    receiver_admission: u32,
    slot: u32,
) -> bool {
    if receiver.volume <= 0.0 { return false; }
    let gas: u32 = slot_gas(slot);
    var is_held: bool = false;
    for (var receiver_slot: u32 = 0u; receiver_slot < 4u; receiver_slot++) {
        let held: u32 = receiver.slots[receiver_slot];
        if slot_amount(held) > 0u && slot_gas(held) == gas { is_held = true; }
    }
    if !is_held {
        var is_admitted: bool = false;
        for (var admitted: u32 = 0u; admitted < 4u; admitted++) {
            if extractBits(receiver_admission, admitted * 8u, 8u) == gas + 1u {
                is_admitted = true;
            }
        }
        if !is_admitted { return false; }
    }
    if is_held || giver.volume <= 0.0 { return true; }
    return f32(slot_amount(slot)) >= gas_materials[gas * 2u].z * AMOUNT_PER_ATMOSPHERE;
}

/// Returns the fraction a gas slot exchanges with a neighbor by diffusion, only between open cells
fn diffusion(giver: PressureCell, receiver: PressureCell, slot: u32) -> u32 {
    if giver.volume <= 0.0 || receiver.volume <= 0.0 { return 0u; }
    return u32(
        gas_materials[slot_gas(slot) * 2u].y / uniforms.substep_count * uniforms.diffusion_scale
            * TRANSFER_SCALE,
    );
}

/// Returns the fraction of phantom air exchanged with a neighbor by diffusion
fn ambient_diffusion(giver: PressureCell, receiver: PressureCell) -> u32 {
    if giver.volume <= 0.0 || receiver.volume <= 0.0 { return 0u; }
    return u32(uniforms.ambient_diffusivity * TRANSFER_SCALE);
}

/// Returns the offset of a face numbered left, right, bottom, then top
fn face_offset(face: u32) -> vec2<i32> {
    switch face {
        case 0u: { return vec2<i32>(-1, 0); }
        case 1u: { return vec2<i32>(1, 0); }
        case 2u: { return vec2<i32>(0, -1); }
        default: { return vec2<i32>(0, 1); }
    }
}

/// Returns the face number of the same face seen from the neighbor across it
fn opposite_face(face: u32) -> u32 { return face ^ 1u; }

/// Returns whether a point lies inside an actor's collision shape, measured like the kinematics
/// actor distance in the actor's unrotated frame
fn is_inside_actor(actor: PressureActor, point: vec2<f32>) -> bool {
    let relative: vec2<f32> = point - actor.center;
    let local: vec2<f32> = vec2<f32>(
        actor.rotation.x * relative.x + actor.rotation.y * relative.y,
        actor.rotation.x * relative.y - actor.rotation.y * relative.x,
    );
    switch actor.shape {
        // a rectangle with half extents as its size
        case 1u: { return all(abs(local) < actor.size); }
        // a vertical capsule with its half segment length and radius as its size
        case 2u: {
            let offset: vec2<f32> =
                local - vec2<f32>(0.0, clamp(local.y, -actor.size.x, actor.size.x));
            return length(offset) < actor.size.y;
        }
        // a circle with its radius as its size
        default: { return length(local) < actor.size.x; }
    }
}

/// Returns the stream function in cells squared per tick of the gas an actor moves at a point,
/// with the shape rounded to a segment and radius: the flow of an ideal gas around a cylinder
/// moving through it plus a pair of eddies behind it, and in the second component whether the
/// point is close enough for the actor to drag its gas. Differences of the stream function
/// between the corners of a face give flows that neither compress nor expand any cell
fn actor_stream(actor: PressureActor, point: vec2<f32>) -> vec2<f32> {
    let relative: vec2<f32> = point - actor.center;
    let local: vec2<f32> = vec2<f32>(
        actor.rotation.x * relative.x + actor.rotation.y * relative.y,
        actor.rotation.x * relative.y - actor.rotation.y * relative.x,
    );
    var core: vec2<f32> = vec2<f32>(0.0);
    var radius: f32 = actor.size.x;
    if actor.shape == 1u {
        // a rectangle as a segment along its longer side
        radius = min(actor.size.x, actor.size.y);
        core = clamp(local, -(actor.size - radius), actor.size - radius);
    } else if actor.shape == 2u {
        core = vec2<f32>(0.0, clamp(local.y, -actor.size.x, actor.size.x));
        radius = actor.size.y;
    }
    let offset: vec2<f32> = local - core;
    let distance: f32 = max(length(offset), radius);
    let velocity: vec2<f32> = vec2<f32>(
        actor.rotation.x * actor.velocity.x + actor.rotation.y * actor.velocity.y,
        actor.rotation.x * actor.velocity.y - actor.rotation.y * actor.velocity.x,
    );
    // inside the radius the gas moves with the actor, outside it flows around
    var stream: f32 = (velocity.x * offset.y - velocity.y * offset.x)
        * radius * radius / (distance * distance);
    let speed: f32 = length(velocity);
    if speed > 0.0 {
        // forward flow along the middle of the wake and back flow beside it
        let direction: vec2<f32> = velocity / speed;
        let along: f32 = (dot(offset, direction) + ACTOR_WAKE_OFFSET * radius)
            / (ACTOR_WAKE_LENGTH * radius);
        let across: f32 = (direction.x * offset.y - direction.y * offset.x) / radius;
        stream += ACTOR_WAKE * speed * radius * across
            * exp(-0.5 * across * across - along * along);
    }
    return vec2<f32>(stream, select(0.0, 1.0, distance < radius * ACTOR_DRAG_RANGE));
}

/// Returns a cell's fluid occupancy, in cells of the densest fluid present
fn fluid_occupancy(cell: vec2<i32>) -> f32 {
    let raster_index: u32 = cell_index(cell) * 4u;
    let mass: f32 = max(f32(bitcast<i32>(fluid_raster[raster_index])), 0.0);
    if mass <= 0.0 { return 0.0; }
    let dominant_identifier: u32 = fluid_raster[raster_index + 3u];
    let density: f32 = fluid_materials[(dominant_identifier & 0x3fffffffu) * 2u].x;
    if density <= 0.0 { return 0.0; }
    return mass / uniforms.fluid_raster_scale / density;
}

/// Measures the open volume of every cell of the listed tiles, or of the whole ring: closed under
/// cellular matter, fluid the renderer draws, and actors, open otherwise; waking tiles whose
/// cells opened or closed and marking tiles near fluid or dragging actors to measure again
@compute @workgroup_size(64)
fn volume(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_cell: u32,
) {
    var physical_index: u32 = workgroup.x;
    if uniforms.is_volume_listed != 0u {
        physical_index =
            control.active_tiles[uniforms.region_width * uniforms.region_height + workgroup.x];
    }
    if local_cell == 0u { atomicStore(&volume_dependence, 0u); }
    workgroupBarrier();
    let index: u32 = physical_index * 64u + local_cell;
    let physical_tile: vec2<u32> =
        vec2<u32>(physical_index % uniforms.region_width, physical_index / uniforms.region_width);
    let tile: vec2<u32> = vec2<u32>(
        (physical_tile.x + uniforms.region_width - uniforms.ring_offset_x) % uniforms.region_width,
        (physical_tile.y + uniforms.region_height - uniforms.ring_offset_y)
            % uniforms.region_height,
    );
    let cell: vec2<i32> = vec2<i32>(tile * 8u + vec2<u32>(local_cell % 8u, local_cell / 8u));
    let occupancy: f32 = fluid_occupancy(cell);
    var open_volume: f32 = 0.0;
    if material_identifiers[index] == 0u {
        open_volume = select(1.0, 0.0, occupancy > FLUID_OCCUPANCY_CLOSED);
    }
    let center: vec2<f32> = vec2<f32>(cell) + 0.5;
    let corner: vec2<f32> = vec2<f32>(cell);
    // the stream function at the top right, bottom right, and top left corners
    var streams: vec3<f32> = vec3<f32>(0.0);
    var is_dragged: bool = false;
    for (var actor_index: u32 = 0u; actor_index < uniforms.actor_count; actor_index++) {
        let actor: PressureActor = actors[actor_index];
        let reach: vec2<f32> = 0.5 * (actor.bounds.zw - actor.bounds.xy) * ACTOR_DRAG_RANGE;
        if any(abs(center - actor.center) > reach) { continue; }
        if actor_stream(actor, center).y == 0.0 { continue; }
        is_dragged = true;
        if is_inside_actor(actor, center) { open_volume = 0.0; }
        streams += vec3<f32>(
            actor_stream(actor, corner + vec2<f32>(1.0, 1.0)).x,
            actor_stream(actor, corner + vec2<f32>(1.0, 0.0)).x,
            actor_stream(actor, corner + vec2<f32>(0.0, 1.0)).x,
        );
    }
    let drag: vec2<f32> = vec2<f32>(streams.x - streams.y, streams.z - streams.x);
    let encoded: vec2<u32> = vec2<u32>(
        u32(round(open_volume * VOLUME_SCALE)) | select(0u, VOLUME_DRAGGED, is_dragged),
        select(0u, pack2x16snorm(drag / ACTOR_VELOCITY_MAXIMUM), is_dragged),
    );
    if any(volumes[index] != encoded) {
        volumes[index] = encoded;
        atomicOr(&pressure_awake[physical_index], TILE_AWAKE);
    }
    // fluid or actors near the tile can change it without waking it, so it and its neighbors are
    // measured again next tick
    if occupancy > 0.0 || is_dragged { atomicStore(&volume_dependence, 1u); }
    workgroupBarrier();
    if local_cell == 0u && atomicLoad(&volume_dependence) != 0u {
        for (var y: i32 = -1; y <= 1; y++) {
            for (var x: i32 = -1; x <= 1; x++) {
                let neighbor: vec2<i32> = vec2<i32>(tile) + vec2<i32>(x, y);
                if neighbor.x < 0 || neighbor.y < 0
                    || neighbor.x >= i32(uniforms.region_width)
                    || neighbor.y >= i32(uniforms.region_height)
                { continue; }
                atomicOr(&pressure_awake[tile_index(vec2<u32>(neighbor))], TILE_VOLUME_DEPENDENT);
            }
        }
    }
}

/// Lists the tiles whose open volumes may have changed: tiles whose dynamic cells moved this
/// tick, tiles awake from edits, streaming, or moving gas, tiles that fluid or dragging actors
/// reached when last measured, and tiles within an actor's drag range
@compute @workgroup_size(64)
fn volume_collect(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if invocation.x >= uniforms.region_width * uniforms.region_height { return; }
    let tile: vec2<u32> =
        vec2<u32>(invocation.x % uniforms.region_width, invocation.x / uniforms.region_width);
    let index: u32 = tile_index(tile);
    var is_listed: bool = (atomicLoad(&cellular_awake[index]) & CELLULAR_TILE_CHANGED) != 0u
        || (atomicAnd(&pressure_awake[index], ~TILE_VOLUME_DEPENDENT)
            & (TILE_AWAKE | TILE_VOLUME_DEPENDENT)) != 0u;
    let minimum: vec2<f32> = vec2<f32>(tile * 8u);
    let maximum: vec2<f32> = minimum + 8.0;
    for (var actor_index: u32 = 0u; actor_index < uniforms.actor_count; actor_index++) {
        let actor: PressureActor = actors[actor_index];
        let reach: vec2<f32> = 0.5 * (actor.bounds.zw - actor.bounds.xy) * ACTOR_DRAG_RANGE;
        if all(actor.center + reach >= minimum) && all(actor.center - reach <= maximum) {
            is_listed = true;
        }
    }
    if is_listed {
        let volume_tile: u32 = atomicAdd(&control.volume_tile_count, 1u);
        control.active_tiles[uniforms.region_width * uniforms.region_height + volume_tile] = index;
    }
}

/// Writes the indirect dispatch of one workgroup per tile whose open volumes are measured
@compute @workgroup_size(1)
fn prepare_volume() {
    dispatches[3] = atomicLoad(&control.volume_tile_count);
    dispatches[4] = 1u;
    dispatches[5] = 1u;
}

/// Lists the tiles whose pressure is simulated this tick: those awake and their neighbors
@compute @workgroup_size(64)
fn collect(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if invocation.x >= uniforms.region_width * uniforms.region_height { return; }
    let tile: vec2<i32> = vec2<i32>(
        i32(invocation.x % uniforms.region_width),
        i32(invocation.x / uniforms.region_width),
    );
    var is_active: bool = false;
    for (var y: i32 = -1; y <= 1; y++) {
        for (var x: i32 = -1; x <= 1; x++) {
            let neighbor: vec2<i32> = tile + vec2<i32>(x, y);
            if neighbor.x < 0 || neighbor.y < 0
                || neighbor.x >= i32(uniforms.region_width)
                || neighbor.y >= i32(uniforms.region_height)
            { continue; }
            if (atomicLoad(&pressure_awake[tile_index(vec2<u32>(neighbor))]) & TILE_AWAKE) != 0u {
                is_active = true;
            }
        }
    }
    // neighbors only read the awake flag, which this store keeps with the volume dependence
    let index: u32 = tile_index(vec2<u32>(tile));
    let flags: u32 = atomicLoad(&pressure_awake[index]) & (TILE_AWAKE | TILE_VOLUME_DEPENDENT);
    atomicStore(&pressure_awake[index], select(flags, flags | TILE_ACTIVE, is_active));
    if is_active {
        control.active_tiles[atomicAdd(&control.active_tile_count, 1u)] = invocation.x;
    }
}

/// Clears the awake flag of every tile once collected, for this tick's simulation to set, keeping
/// its volume dependence for the next tick
@compute @workgroup_size(64)
fn clear(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if invocation.x >= uniforms.region_width * uniforms.region_height { return; }
    atomicAnd(&pressure_awake[invocation.x], TILE_ACTIVE | TILE_VOLUME_DEPENDENT);
}

/// Lists the blocks of 2x2 tiles holding an active tile, by their bottom-left tile, after the
/// measured tiles are no longer needed
@compute @workgroup_size(64)
fn collect_blocks(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let block_columns: u32 = (uniforms.region_width + 1u) / 2u;
    if invocation.x >= block_columns * ((uniforms.region_height + 1u) / 2u) { return; }
    let tile: vec2<u32> = 2u * vec2<u32>(invocation.x % block_columns, invocation.x / block_columns);
    var is_active: bool = false;
    for (var offset: u32 = 0u; offset < 4u; offset++) {
        let block_tile: vec2<u32> = tile + vec2<u32>(offset % 2u, offset / 2u);
        if block_tile.x >= uniforms.region_width || block_tile.y >= uniforms.region_height {
            continue;
        }
        if (atomicLoad(&pressure_awake[tile_index(block_tile)]) & TILE_ACTIVE) != 0u {
            is_active = true;
        }
    }
    if is_active {
        let block: u32 = atomicAdd(&control.block_count, 1u);
        control.active_tiles[uniforms.region_width * uniforms.region_height + block] =
            tile.y * uniforms.region_width + tile.x;
        // the halo of the block's pressure tile reaches the ring of blocks around it, whose
        // pressure cells are rebuilt
        for (var offset: u32 = 0u; offset < 36u; offset++) {
            let halo_tile: vec2<i32> =
                vec2<i32>(tile) + vec2<i32>(i32(offset % 6u), i32(offset / 6u)) - 2;
            if any(halo_tile < vec2<i32>(0))
                || halo_tile.x >= i32(uniforms.region_width)
                || halo_tile.y >= i32(uniforms.region_height)
            { continue; }
            atomicOr(&pressure_awake[tile_index(vec2<u32>(halo_tile))], TILE_HALO);
        }
    }
}

/// Writes the indirect dispatches of one workgroup per active tile and per listed block
@compute @workgroup_size(1)
fn prepare() {
    dispatches[0] = atomicLoad(&control.active_tile_count);
    dispatches[1] = 1u;
    dispatches[2] = 1u;
    dispatches[9] = atomicLoad(&control.block_count);
    dispatches[10] = 1u;
    dispatches[11] = 1u;
}

/// Returns a halo cell's net outflow through its faces from the previous substep
fn cell_divergence(cell: vec2<i32>) -> f32 {
    return flow_x(cell) - flow_x(cell + vec2<i32>(-1, 0))
        + flow_y(cell) - flow_y(cell + vec2<i32>(0, -1));
}

/// Returns the new flows through an active cell's right and top faces, waking sleeping neighbors
/// the flows would push against, or zero for a cell outside the simulated tiles
fn cell_flows(cell: vec2<i32>) -> vec2<f32> {
    let center: PressureCell = halo_cell(cell);
    var flows: vec2<f32> = vec2<f32>(0.0);
    if !center.is_active { return flows; }
    for (var axis: u32 = 0u; axis < 2u; axis++) {
        let is_vertical: bool = axis == 1u;
        let neighbor_cell: vec2<i32> = cell + select(vec2<i32>(1, 0), vec2<i32>(0, 1), is_vertical);
        if !is_coarse_inside(neighbor_cell) { continue; }
        let neighbor: PressureCell = halo_cell(neighbor_cell);
        let previous: f32 = select(center.flow.x, center.flow.y, is_vertical);
        var flow: f32 = 0.0;
        if is_stress_face(center, neighbor) {
            flow = stress_flow(center, neighbor, previous);
        } else if center.solid == 0u && neighbor.solid == 0u {
            let face: vec2<f32> =
                vec2<f32>(cell) + select(vec2<f32>(1.0, 0.5), vec2<f32>(0.5, 1.0), is_vertical);
            // the flow across a face, averaged from the four perpendicular faces around it
            var cross_flow: f32;
            if is_vertical {
                cross_flow = 0.25 * (center.flow.x + neighbor.flow.x
                    + flow_x(cell + vec2<i32>(-1, 0)) + flow_x(neighbor_cell + vec2<i32>(-1, 0)));
            } else {
                cross_flow = 0.25 * (center.flow.y + neighbor.flow.y
                    + flow_y(cell + vec2<i32>(0, -1)) + flow_y(neighbor_cell + vec2<i32>(0, -1)));
            }
            flow = face_flow(
                center,
                neighbor,
                previous,
                face,
                cross_flow,
                cell_divergence(neighbor_cell) - cell_divergence(cell),
                is_vertical,
            );
        }
        // a sleeping neighbor wakes for the next tick instead of exchanging this tick
        if !neighbor.is_active {
            if abs(flow) >= SLEEP_FLOW_MINIMUM {
                atomicOr(&pressure_awake[tile_index(coarse_tile(neighbor_cell))], TILE_AWAKE);
            }
            continue;
        }
        flows[axis] = flow;
    }
    return flows;
}

/// Returns the new flows of each face of a cell within one cell of the block, positive out of it
fn cell_outgoing_flows(cell: vec2<i32>) -> vec4<f32> {
    let flows: vec2<f32> = halo_flows[halo_flow_index(cell)];
    return vec4<f32>(
        -halo_flows[halo_flow_index(cell + vec2<i32>(-1, 0))].x,
        flows.x,
        -halo_flows[halo_flow_index(cell + vec2<i32>(0, -1))].y,
        flows.y,
    );
}

/// Returns the limited fractions of an active cell's gas given through each face: by its
/// outgoing flows from an open or other closed cell, or for a solid cell a share of its trapped
/// gas through each open face, never filling a neighbor past `PRESSURE_MAXIMUM`. Zero outside
/// the simulated tiles
fn cell_transfers(cell: vec2<i32>) -> vec4<u32> {
    let center: PressureCell = halo_cell(cell);
    if !center.is_active || center.total <= 0.0 { return vec4<u32>(0u); }
    var fractions: vec4<f32> = vec4<f32>(0.0);
    if center.solid != 0u {
        var open_count: f32 = 0.0;
        for (var face: u32 = 0u; face < 4u; face++) {
            if halo_cell(cell + face_offset(face)).volume > 0.0 { open_count += 1.0; }
        }
        for (var face: u32 = 0u; face < 4u; face++) {
            if halo_cell(cell + face_offset(face)).volume > 0.0 {
                fractions[face] = uniforms.trapped_expulsion / open_count;
            }
        }
    } else {
        fractions = max(cell_outgoing_flows(cell), vec4<f32>(0.0)) / center.total;
        let fraction_sum: f32 = fractions.x + fractions.y + fractions.z + fractions.w;
        if fraction_sum > BULK_FRACTION_MAXIMUM {
            fractions *= BULK_FRACTION_MAXIMUM / fraction_sum;
        }
    }
    // each face may fill a neighbor by a quarter of its room, so four faces never overfill it
    for (var face: u32 = 0u; face < 4u; face++) {
        if fractions[face] <= 0.0 { continue; }
        let neighbor: PressureCell = halo_cell(cell + face_offset(face));
        var room: f32 = max(f32(AMOUNT_MAXIMUM) - neighbor.total, 0.0);
        if neighbor.solid != 0u {
            room = max(
                f32(AMOUNT_MAXIMUM) - f32(uniforms.ambient_amount) - f32(neighbor.stored),
                0.0,
            ) * solid_capacity(neighbor.density);
        }
        fractions[face] = min(fractions[face], 0.25 * room / center.total);
    }
    return vec4<u32>(fractions * TRANSFER_SCALE);
}

/// Returns the gases an active cell admits into its free slots: the largest new gases its
/// neighbors could give it by flow or diffusion, packed as gas index plus one per byte
fn cell_admission(cell: vec2<i32>) -> u32 {
    if is_halo_gas_free { return 0u; }
    let center: PressureCell = halo_cell(cell);
    if !center.is_active || center.volume <= 0.0 { return 0u; }
    var free_count: u32 = 0u;
    for (var slot: u32 = 0u; slot < 4u; slot++) {
        if slot_amount(center.slots[slot]) == 0u { free_count += 1u; }
    }
    if free_count == 0u { return 0u; }
    // No species can be admitted if the center and its four neighbors hold no gas.
    var has_gas: bool = false;
    for (var slot: u32 = 0u; slot < 4u; slot++) {
        if slot_amount(center.slots[slot]) > 0u { has_gas = true; }
    }
    for (var face: u32 = 0u; face < 4u; face++) {
        let neighbor_slots: vec4<u32> = halo_slots(cell + face_offset(face));
        for (var slot: u32 = 0u; slot < 4u; slot++) {
            if slot_amount(neighbor_slots[slot]) > 0u { has_gas = true; }
        }
    }
    if !has_gas { return 0u; }
    let signed_outgoing: vec4<f32> = cell_outgoing_flows(cell);
    var candidate_gases: array<u32, 16>;
    var candidate_amounts: array<u32, 16>;
    var candidate_count: u32 = 0u;
    for (var face: u32 = 0u; face < 4u; face++) {
        let neighbor_cell: vec2<i32> = cell + face_offset(face);
        let neighbor: PressureCell = halo_cell(neighbor_cell);
        if !neighbor.is_active { continue; }
        // solid cells push their trapped gas out whatever the stress flow across the face
        let is_flowing_in: bool = signed_outgoing[face] < 0.0 || neighbor.solid != 0u;
        for (var slot: u32 = 0u; slot < 4u; slot++) {
            let neighbor_slot: u32 = neighbor.slots[slot];
            let amount: u32 = slot_amount(neighbor_slot);
            if amount == 0u { continue; }
            if !is_flowing_in && diffusion(neighbor, center, neighbor_slot) == 0u { continue; }
            let gas: u32 = slot_gas(neighbor_slot);
            var is_held: bool = false;
            for (var held: u32 = 0u; held < 4u; held++) {
                if slot_amount(center.slots[held]) > 0u && slot_gas(center.slots[held]) == gas {
                    is_held = true;
                }
            }
            if is_held { continue; }
            if neighbor.volume > 0.0
                && f32(amount) < gas_materials[gas * 2u].z * AMOUNT_PER_ATMOSPHERE
            { continue; }
            candidate_gases[candidate_count] = gas;
            candidate_amounts[candidate_count] = amount;
            candidate_count += 1u;
        }
    }
    // pick distinct gases by largest offered amount, ties by the lower gas index
    var admission: u32 = 0u;
    var admitted_count: u32 = 0u;
    var admitted: array<u32, 4>;
    for (var pick: u32 = 0u; pick < free_count; pick++) {
        var best: u32 = 16u;
        for (var candidate: u32 = 0u; candidate < candidate_count; candidate++) {
            let gas: u32 = candidate_gases[candidate];
            var is_taken: bool = false;
            for (var taken: u32 = 0u; taken < admitted_count; taken++) {
                if admitted[taken] == gas { is_taken = true; }
            }
            if is_taken { continue; }
            if best == 16u || candidate_amounts[candidate] > candidate_amounts[best]
                || (candidate_amounts[candidate] == candidate_amounts[best]
                    && gas < candidate_gases[best])
            { best = candidate; }
        }
        if best == 16u { break; }
        admitted[admitted_count] = candidate_gases[best];
        admission |= (candidate_gases[best] + 1u) << (admitted_count * 8u);
        admitted_count += 1u;
    }
    return admission;
}

/// Simulates one substep of each block holding an active tile: updates the face flows, limits the gas they give,
/// chooses the gases free slots admit, then moves gas through every face by the limited flows
/// and diffusion and stored pressure through every face touching a solid cell, writing each
/// cell's new species slots, phantom air, stored pressure, and the flows its faces actually
/// carried, in the cells of its active tiles. Each workgroup recomputes the flows and transfers of
/// the rings around its block instead of waiting for its neighbors, which is exact because every
/// workgroup computes them from the same previous substep. This entry point simulates the blocks
/// whose halo holds no gas, compiled without the species slot work, and lists the others for
/// `step_gas`
@compute @workgroup_size(STEP_WORKGROUP_SIZE)
fn step(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_cell: u32,
) {
    let tile_count: u32 = uniforms.region_width * uniforms.region_height;
    let block: u32 = control.active_tiles[tile_count + workgroup.x];
    let cell: vec2<i32> = load_halo(block, local_cell);
    if local_cell == 0u { halo_gas_uniform = atomicLoad(&halo_gas); }
    if workgroupUniformLoad(&halo_gas_uniform) != 0u {
        if local_cell == 0u {
            let gas_block: u32 = atomicAdd(&control.gas_block_count, 1u);
            control.active_tiles[tile_count * 2u + gas_block] = block;
        }
        return;
    }
    is_halo_gas_free = true;
    simulate_substep(cell, local_cell);
}

/// Simulates one substep of the blocks whose halo holds gas, listed by `step`
@compute @workgroup_size(STEP_WORKGROUP_SIZE)
fn step_gas(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_cell: u32,
) {
    let cell: vec2<i32> = load_halo(
        control.active_tiles[uniforms.region_width * uniforms.region_height * 2u + workgroup.x],
        local_cell,
    );
    is_halo_gas_free = false;
    simulate_substep(cell, local_cell);
}

/// Writes the indirect dispatch of one workgroup per block `step` listed as holding gas, and
/// clears the list for the next substep
@compute @workgroup_size(1)
fn prepare_gas() {
    dispatches[6] = atomicLoad(&control.gas_block_count);
    dispatches[7] = 1u;
    dispatches[8] = 1u;
    atomicStore(&control.gas_block_count, 0u);
}

/// Simulates one substep of the workgroup's block from its loaded halo, as `step` describes
fn simulate_substep(cell: vec2<i32>, local_cell: u32) {
    for (var flow_index: u32 = local_cell; flow_index < HALO_FLOW_SIZE * HALO_FLOW_SIZE;
        flow_index += STEP_WORKGROUP_SIZE)
    {
        halo_flows[flow_index] = cell_flows(halo_origin + 1
            + vec2<i32>(i32(flow_index % HALO_FLOW_SIZE), i32(flow_index / HALO_FLOW_SIZE)));
    }
    workgroupBarrier();
    for (var transfer_index: u32 = local_cell;
        transfer_index < HALO_TRANSFER_SIZE * HALO_TRANSFER_SIZE;
        transfer_index += STEP_WORKGROUP_SIZE)
    {
        let transfer_cell: vec2<i32> = halo_origin + 2 + vec2<i32>(
            i32(transfer_index % HALO_TRANSFER_SIZE),
            i32(transfer_index / HALO_TRANSFER_SIZE),
        );
        let transfers: vec4<u32> = cell_transfers(transfer_cell);
        halo_transfers[transfer_index] =
            vec2<u32>(transfers.x | (transfers.y << 16u), transfers.z | (transfers.w << 16u));
        halo_admissions[transfer_index] = cell_admission(transfer_cell);
    }
    workgroupBarrier();
    // only the pressure cells in active tiles change, and the helpers have none of their own
    if local_cell >= 64u || (halo_words[halo_index(cell)].w & VOLUME_ACTIVE) == 0u { return; }
    let index: u32 = coarse_index(cell);
    let center: PressureCell = halo_cell(cell);
    let center_transfers: vec4<u32> = halo_transfer_fractions(cell);
    let center_admission: u32 = halo_admissions[halo_transfer_index(cell)];
    let center_flows: vec4<f32> = cell_outgoing_flows(cell);
    var amounts: array<u32, 4>;
    var gases: array<u32, 4>;
    for (var slot: u32 = 0u; slot < 4u; slot++) {
        amounts[slot] = slot_amount(center.slots[slot]);
        gases[slot] = slot_gas(center.slots[slot]);
    }
    var phantom: u32 = center.phantom;
    var stored: i32 = center.stored;
    var flows: vec2<f32> = vec2<f32>(0.0);
    for (var face: u32 = 0u; face < 4u; face++) {
        let neighbor_cell: vec2<i32> = cell + face_offset(face);
        let neighbor: PressureCell = halo_cell(neighbor_cell);
        if !neighbor.is_active { continue; }
        let neighbor_transfers: vec4<u32> = halo_transfer_fractions(neighbor_cell);
        let neighbor_admission: u32 = halo_admissions[halo_transfer_index(neighbor_cell)];
        let giving: u32 = center_transfers[face];
        let receiving: u32 = neighbor_transfers[opposite_face(face)];
        var carried: f32 = (f32(giving) * center.total - f32(receiving) * neighbor.total)
            / TRANSFER_SCALE;
        // a solid cell gives stored pressure by its outgoing flow, and an open cell gives its
        // phantom air into a solid cell by its limited fraction
        if is_stress_face(center, neighbor) {
            let given: i32 = stored_amount(center, neighbor, center_flows[face]);
            let received: i32 = stored_amount(neighbor, center, -center_flows[face]);
            var phantom_given: i32 = 0;
            var phantom_received: i32 = 0;
            if center.solid == 0u { phantom_given = i32(transfer(center.phantom, giving)); }
            if neighbor.solid == 0u {
                phantom_received = i32(transfer(neighbor.phantom, receiving));
            }
            carried = f32(given - received + phantom_given - phantom_received);
            if center.solid != 0u {
                stored += stored_change(center, received + phantom_received - given);
            } else {
                phantom = u32(i32(phantom) + received - phantom_given);
            }
        }
        // record the flow this cell's right and top faces carried, positive toward the neighbor
        if face == 1u || face == 3u { flows[face / 2u] = carried; }
        // without gas in the halo only phantom air and stored pressure move
        if is_halo_gas_free {
            if neighbor.volume > 0.0 {
                phantom -= transfer(center.phantom, giving + ambient_diffusion(center, neighbor));
            }
            if center.volume > 0.0 {
                phantom +=
                    transfer(neighbor.phantom, receiving + ambient_diffusion(neighbor, center));
            }
            continue;
        }
        for (var slot: u32 = 0u; slot < 4u; slot++) {
            let center_slot: u32 = center.slots[slot];
            if slot_amount(center_slot) > 0u
                && is_transfer_allowed(center, neighbor, neighbor_admission, center_slot)
            {
                amounts[slot] -= transfer(
                    slot_amount(center_slot),
                    giving + diffusion(center, neighbor, center_slot),
                );
            }
        }
        // phantom air given into a solid cell left above as stored pressure
        if neighbor.volume > 0.0 {
            phantom -= transfer(center.phantom, giving + ambient_diffusion(center, neighbor));
        }
        for (var slot: u32 = 0u; slot < 4u; slot++) {
            let neighbor_slot: u32 = neighbor.slots[slot];
            if slot_amount(neighbor_slot) == 0u
                || !is_transfer_allowed(neighbor, center, center_admission, neighbor_slot)
            { continue; }
            let amount: u32 = transfer(
                slot_amount(neighbor_slot),
                receiving + diffusion(neighbor, center, neighbor_slot),
            );
            if amount == 0u { continue; }
            // add to the slot holding the gas, or to the free slot its admission reserved
            let gas: u32 = slot_gas(neighbor_slot);
            var target_slot: u32 = 4u;
            for (var held: u32 = 0u; held < 4u; held++) {
                if slot_amount(center.slots[held]) > 0u && gases[held] == gas {
                    target_slot = held;
                }
            }
            if target_slot == 4u {
                var admitted: u32 = 0u;
                for (var position: u32 = 0u; position < 4u; position++) {
                    if extractBits(center_admission, position * 8u, 8u) == gas + 1u {
                        admitted = position;
                    }
                }
                // the admitted position counts free slots in order
                var free_index: u32 = 0u;
                for (var free: u32 = 0u; free < 4u; free++) {
                    if slot_amount(center.slots[free]) != 0u { continue; }
                    if free_index == admitted { target_slot = free; }
                    free_index += 1u;
                }
                if target_slot == 4u { continue; }
                gases[target_slot] = gas;
            }
            // givers leave room for every face, so the amount always fits
            amounts[target_slot] += amount;
        }
        if center.volume > 0.0 {
            phantom +=
                transfer(neighbor.phantom, receiving + ambient_diffusion(neighbor, center));
        }
    }
    // stored pressure slowly relaxes toward the ambient pressure
    if center.solid != 0u {
        stored -= i32(trunc(f32(stored) * uniforms.solid_relaxation));
        stored = clamp(
            stored,
            -i32(uniforms.ambient_amount),
            i32(AMOUNT_MAXIMUM - uniforms.ambient_amount),
        );
    }
    // gas pressure relaxes toward the ambient pressure, taking phantom air before gas, so sealed
    // pockets and the wake of a wave never hold it, and closed cells only lose the gas they trap
    var is_relaxing: bool = false;
    var gas_total: u32 = 0u;
    for (var slot: u32 = 0u; slot < 4u; slot++) { gas_total += amounts[slot]; }
    let volume: f32 = max(center.volume, VOLUME_MINIMUM);
    let excess: f32 = f32(phantom + gas_total) / volume - f32(uniforms.ambient_amount);
    if center.solid == 0u && (center.volume > 0.0 || excess > 0.0) {
        let beyond: f32 =
            abs(excess) - RELAXATION_TOLERANCE * AMOUNT_PER_ATMOSPHERE / uniforms.diffusion_scale;
        var change: u32 = u32(volume * clamp(
            max(beyond * uniforms.open_relaxation, uniforms.open_relaxation_minimum),
            0.0,
            beyond,
        ));
        if change > 0u {
            if excess > 0.0 {
                let from_phantom: u32 = min(change, phantom);
                phantom -= from_phantom;
                change -= from_phantom;
                if change > 0u && gas_total > 0u {
                    let kept: f32 = max(1.0 - f32(change) / f32(gas_total), 0.0);
                    for (var slot: u32 = 0u; slot < 4u; slot++) {
                        amounts[slot] = u32(f32(amounts[slot]) * kept);
                    }
                }
            } else {
                phantom = min(phantom + change, AMOUNT_MAXIMUM - min(gas_total, AMOUNT_MAXIMUM));
            }
            is_relaxing = true;
        }
    }
    // static cells absorb the load beyond their strength, a solid pressure cell's from its own
    // stored pressure or the gas pressing on its open faces and an open one's from its gas,
    // losing integrity until they break
    let is_partial: bool = (coarse_constants(index).x & VOLUME_PARTIAL) != 0u;
    if center.solid != 0u || is_partial {
        var load: f32 = abs(center.pressure - f32(uniforms.ambient_amount));
        if center.solid != 0u {
            load = abs(f32(center.stored));
            for (var face: u32 = 0u; face < 4u; face++) {
                let neighbor: PressureCell = halo_cell(cell + face_offset(face));
                if neighbor.volume > 0.0 {
                    load = max(load, abs(neighbor.pressure - f32(uniforms.ambient_amount)));
                }
            }
        }
        // a pressure cell holds the gas of four cells, so each cell bears a quarter
        damage_static_cells(cell, 0.25 * load);
    }
    var slots: vec4<u32> = vec4<u32>(0u);
    for (var slot: u32 = 0u; slot < 4u; slot++) {
        if amounts[slot] > 0u { slots[slot] = (gases[slot] << 24u) | amounts[slot]; }
    }
    // faint residual pressure that diffuses by a few units, or flows too small to carry a whole
    // unit through a face, let the tile sleep
    if any(slots != center.slots)
        || u32(abs(i32(phantom) - i32(center.phantom))) > SLEEP_PHANTOM_CHANGE_MAXIMUM
        || stored != center.stored
        || is_relaxing
        || any(abs(flows) >= vec2<f32>(SLEEP_FLOW_MINIMUM))
    {
        atomicOr(&pressure_awake[tile_index(coarse_tile(cell))], TILE_AWAKE);
    }
    gas_out[index * 2u] = slots;
    gas_out[index * 2u + 1u] =
        vec4<u32>(phantom, bitcast<u32>(flows.x), bitcast<u32>(flows.y), bitcast<u32>(stored));
}

/// Damages the static cells under a pressure cell by a load in amount units of a cell
fn damage_static_cells(cell: vec2<i32>, load: f32) {
    for (var offset: u32 = 0u; offset < 4u; offset++) {
        let fine_cell: vec2<i32> = cell * 2 + vec2<i32>(i32(offset % 2u), i32(offset / 2u));
        let index: u32 = cell_index(fine_cell);
        let material_identifier: u32 = material_identifiers[index];
        if !is_static(material_identifier) { continue; }
        let material: vec4<f32> = static_materials[material_identifier & 0x3fffffffu];
        let overload: f32 = load / AMOUNT_PER_ATMOSPHERE - material.z;
        if overload <= 0.0 { continue; }
        let damage: u32 = kinematics[index].y & INTEGRITY_BROKEN;
        let next: u32 = min(
            damage + u32(ceil(overload * material.y * uniforms.damage_rate)),
            INTEGRITY_BROKEN,
        );
        if next != damage {
            kinematics[index].y = (kinematics[index].y & ~INTEGRITY_BROKEN) | next;
        }
        if next == INTEGRITY_BROKEN {
            atomicOr(&pressure_awake[tile_index(vec2<u32>(fine_cell) / 8u)], TILE_BROKEN);
        }
    }
}

/// Flags of the tile a `coarsen` workgroup rebuilds
var<workgroup> coarsened_tile_flags: u32;

/// Rebuilds the pressure cells of every tile a listed block's halo reaches from the cells under
/// them, one workgroup per tile of the region and one invocation per pressure cell, and clears
/// their flags: a pressure cell holds its cells' phantom air and gases, merged into at most four
/// species, the flows its first cell keeps, their mean open volume and drag, and, when all its
/// cells are static, their summed stored pressure, mean density, and the first one's solid word
@compute @workgroup_size(16)
fn coarsen(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_cell: u32,
) {
    let tile_position: vec2<u32> =
        vec2<u32>(workgroup.x % uniforms.region_width, workgroup.x / uniforms.region_width);
    let physical_tile: u32 = tile_index(tile_position);
    if local_cell == 0u { coarsened_tile_flags = atomicLoad(&pressure_awake[physical_tile]); }
    if (workgroupUniformLoad(&coarsened_tile_flags) & TILE_HALO) == 0u { return; }
    if local_cell == 0u { atomicAnd(&pressure_awake[physical_tile], ~TILE_HALO); }
    let cell: vec2<i32> = vec2<i32>(
        tile_position * PRESSURE_CELLS_PER_TILE + vec2<u32>(local_cell % 4u, local_cell / 4u),
    );
    var phantom: u32 = 0u;
    var flows: vec2<u32> = vec2<u32>(0u);
    var stored: i32 = 0;
    var open_volume: u32 = 0u;
    var static_count: u32 = 0u;
    var solid: u32 = 0u;
    var density: f32 = 0.0;
    var drag: vec2<f32> = vec2<f32>(0.0);
    var drag_count: u32 = 0u;
    var gases: array<u32, 4>;
    var amounts: array<u32, 4>;
    var species_count: u32 = 0u;
    for (var offset: u32 = 0u; offset < 4u; offset++) {
        let index: u32 =
            cell_index(cell * 2 + vec2<i32>(i32(offset % 2u), i32(offset / 2u)));
        let slots: vec4<u32> = gas_in[index * 2u];
        let words: vec4<u32> = gas_in[index * 2u + 1u];
        phantom = min(phantom + words.x, AMOUNT_MAXIMUM);
        if offset == 0u { flows = words.yz; }
        for (var slot: u32 = 0u; slot < 4u; slot++) {
            let amount: u32 = slot_amount(slots[slot]);
            if amount == 0u { continue; }
            let gas: u32 = slot_gas(slots[slot]);
            var held: u32 = 4u;
            for (var species: u32 = 0u; species < species_count; species++) {
                if gases[species] == gas { held = species; }
            }
            // a fifth species in one pressure cell is lost
            if held == 4u && species_count < 4u {
                held = species_count;
                gases[held] = gas;
                amounts[held] = 0u;
                species_count += 1u;
            }
            if held < 4u { amounts[held] = min(amounts[held] + amount, AMOUNT_MAXIMUM); }
        }
        let volume_word: vec2<u32> = volumes[index];
        let material_identifier: u32 = material_identifiers[index];
        if is_static(material_identifier) {
            let material: vec4<f32> = static_materials[material_identifier & 0x3fffffffu];
            static_count += 1u;
            stored += bitcast<i32>(words.w);
            density += material.x;
            if solid == 0u { solid = static_solid(index, material); }
        } else {
            open_volume += volume_word.x & ~VOLUME_DRAGGED;
            if (volume_word.x & VOLUME_DRAGGED) != 0u {
                drag += unpack2x16snorm(volume_word.y);
                drag_count += 1u;
            }
        }
    }
    var slots: vec4<u32> = vec4<u32>(0u);
    for (var species: u32 = 0u; species < species_count; species++) {
        slots[species] = (gases[species] << 24u) | amounts[species];
    }
    let is_solid: bool = static_count == 4u;
    let index: u32 = coarse_index(cell);
    gas_out[index * 2u] = slots;
    gas_out[index * 2u + 1u] =
        vec4<u32>(phantom, flows, bitcast<u32>(select(0, stored, is_solid)));
    // drags are velocities in cells per tick, halved in pressure cells
    let base: u32 = coarse_constant_index(index);
    control.active_tiles[base] = open_volume / 4u
        | select(0u, VOLUME_DRAGGED, drag_count > 0u)
        | select(0u, VOLUME_PARTIAL, static_count > 0u && !is_solid);
    control.active_tiles[base + 1u] =
        pack2x16snorm(0.5 * drag / f32(max(drag_count, 1u)));
    control.active_tiles[base + 2u] = select(0u, solid, is_solid);
    control.active_tiles[base + 3u] = bitcast<u32>(density / 4.0);
}

/// Spreads the pressure cells of a listed block's active tiles back over the cells under them,
/// one invocation per cell: phantom air and each gas go to the open cells by open volume, or
/// evenly to closed ones, with the remainder to the most open cell so nothing is lost, every
/// cell keeps its pressure cell's flows, and static cells of a solid pressure cell a quarter of
/// its stored pressure
@compute @workgroup_size(256)
fn refine(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_cell: u32,
) {
    let block: u32 =
        control.active_tiles[uniforms.region_width * uniforms.region_height + workgroup.x];
    let block_position: vec2<u32> =
        vec2<u32>(block % uniforms.region_width, block / uniforms.region_width);
    let fine_cell: vec2<u32> = block_position * 8u + vec2<u32>(local_cell % 16u, local_cell / 16u);
    if fine_cell.x >= uniforms.region_width * 8u || fine_cell.y >= uniforms.region_height * 8u {
        return;
    }
    if (atomicLoad(&pressure_awake[tile_index(fine_cell / 8u)]) & TILE_ACTIVE) == 0u { return; }
    let cell: vec2<i32> = vec2<i32>(fine_cell / 2u);
    let own: u32 = (fine_cell.x % 2u) + 2u * (fine_cell.y % 2u);
    // the open volume of each cell under the pressure cell weighs its share
    var weights: array<u32, 4>;
    var weight_sum: u32 = 0u;
    var heaviest: u32 = 0u;
    for (var offset: u32 = 0u; offset < 4u; offset++) {
        let index: u32 =
            cell_index(cell * 2 + vec2<i32>(i32(offset % 2u), i32(offset / 2u)));
        var weight: u32 = 0u;
        if !is_static(material_identifiers[index]) { weight = volumes[index].x & ~VOLUME_DRAGGED; }
        weights[offset] = weight;
        weight_sum += weight;
        if weight > weights[heaviest] { heaviest = offset; }
    }
    if weight_sum == 0u {
        for (var offset: u32 = 0u; offset < 4u; offset++) { weights[offset] = 1u; }
        weight_sum = 4u;
    }
    let index: u32 = coarse_index(cell);
    let coarse_slots: vec4<u32> = gas_in[index * 2u];
    let words: vec4<u32> = gas_in[index * 2u + 1u];
    var slots: vec4<u32> = vec4<u32>(0u);
    for (var slot: u32 = 0u; slot < 4u; slot++) {
        let amount: u32 = share(slot_amount(coarse_slots[slot]), &weights, weight_sum, heaviest, own);
        if amount > 0u { slots[slot] = (coarse_slots[slot] & 0xff000000u) | amount; }
    }
    let fine_index: u32 = cell_index(vec2<i32>(fine_cell));
    // a static cell takes its share of a solid pressure cell's stored pressure, or bears the gas
    // pressure of an open one, so broken cells and the pressure view see the load on it
    var stored: i32 = 0;
    let constants: vec4<u32> = coarse_constants(index);
    if is_static(material_identifiers[fine_index]) {
        if constants.z != 0u {
            stored = bitcast<i32>(words.w) / 4;
        } else {
            var total: f32 = f32(words.x);
            for (var slot: u32 = 0u; slot < 4u; slot++) {
                total += f32(slot_amount(coarse_slots[slot]));
            }
            let pressure: f32 =
                total / max(f32(constants.x & 0x1ffffu) / VOLUME_SCALE, VOLUME_MINIMUM);
            stored = i32(round(0.25 * (pressure - f32(uniforms.ambient_amount))));
        }
    }
    gas_out[fine_index * 2u] = slots;
    gas_out[fine_index * 2u + 1u] = vec4<u32>(
        share(words.x, &weights, weight_sum, heaviest, own),
        words.y,
        words.z,
        bitcast<u32>(stored),
    );
}

/// Returns one cell's share of an amount split by weights, rounded down with the remainder going
/// to the heaviest cell so the shares add up to the amount
fn share(
    amount: u32,
    weights: ptr<function, array<u32, 4>>,
    weight_sum: u32,
    heaviest: u32,
    own: u32,
) -> u32 {
    var shares: array<u32, 4>;
    var given: u32 = 0u;
    for (var offset: u32 = 0u; offset < 4u; offset++) {
        shares[offset] = min(
            u32(floor(f32(amount) * f32((*weights)[offset]) / f32(weight_sum))),
            amount - given,
        );
        given += shares[offset];
    }
    shares[heaviest] += amount - given;
    return shares[own];
}

/// Returns a pressure cell's pressure above the ambient pressure in amount units of a cell, and
/// whether it is open: its gas per open volume, its stored pressure when solid, or for another
/// closed pressure cell its trapped gas beyond the ambient pressure as if it filled it, since a
/// cell full of matter holds no vacuum
fn coarse_gauge(cell: vec2<i32>) -> vec2<f32> {
    if !is_coarse_inside(cell) { return vec2<f32>(0.0); }
    let index: u32 = coarse_index(cell);
    let slots: vec4<u32> = gas_in[index * 2u];
    let words: vec4<u32> = gas_in[index * 2u + 1u];
    let constants: vec4<u32> = coarse_constants(index);
    let ambient: f32 = f32(uniforms.ambient_amount);
    if constants.z != 0u { return vec2<f32>(0.25 * f32(bitcast<i32>(words.w)), 0.0); }
    var total: f32 = f32(words.x);
    for (var slot: u32 = 0u; slot < 4u; slot++) { total += f32(slot_amount(slots[slot])); }
    let volume: f32 = f32(constants.x & 0x1ffffu) / VOLUME_SCALE;
    if volume <= 0.0 { return vec2<f32>(0.25 * max(total - ambient, 0.0), 0.0); }
    return vec2<f32>(0.25 * (total / max(volume, VOLUME_MINIMUM) - ambient), 1.0);
}

/// Publishes the pressure above the ambient pressure of every cell of the active tiles, in
/// atmospheres, from the newest pressure cells: a static cell's stored pressure, an open cell's
/// pressure interpolated between the open pressure cells around it so pressure pushes evenly
/// across them, or its own pressure cell's; waking the dynamic cells beside cells whose pressure
/// pushes them
@compute @workgroup_size(64)
fn publish(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_cell: u32,
) {
    let tile: u32 = control.active_tiles[workgroup.x];
    let tile_position: vec2<u32> =
        vec2<u32>(tile % uniforms.region_width, tile / uniforms.region_width);
    let cell: vec2<i32> =
        vec2<i32>(tile_position * 8u + vec2<u32>(local_cell % 8u, local_cell / 8u));
    let index: u32 = cell_index(cell);
    var pressure: f32;
    if is_static(material_identifiers[index]) {
        pressure = f32(bitcast<i32>(gas_out[index * 2u + 1u].w));
    } else {
        // the cell's center among the pressure cell centers around it
        let position: vec2<f32> = (vec2<f32>(cell) + 0.5) * 0.5 - 0.5;
        let base: vec2<i32> = vec2<i32>(floor(position));
        let fraction: vec2<f32> = position - floor(position);
        var weighted: f32 = 0.0;
        var weight_sum: f32 = 0.0;
        for (var corner: u32 = 0u; corner < 4u; corner++) {
            let offset: vec2<i32> = vec2<i32>(i32(corner % 2u), i32(corner / 2u));
            let gauge: vec2<f32> = coarse_gauge(base + offset);
            let weight: f32 = gauge.y * select(1.0 - fraction.x, fraction.x, offset.x == 1)
                * select(1.0 - fraction.y, fraction.y, offset.y == 1);
            weighted += weight * gauge.x;
            weight_sum += weight;
        }
        pressure = select(coarse_gauge(cell / 2).x, weighted / weight_sum, weight_sum > 0.0);
    }
    let gauge: f32 = pressure / AMOUNT_PER_ATMOSPHERE;
    pressures[index] = gauge;
    if abs(gauge) < PRESSURE_PUSH_MINIMUM { return; }
    for (var face: u32 = 0u; face < 4u; face++) {
        let neighbor: vec2<i32> = cell + face_offset(face);
        if !is_cell_inside(neighbor) { continue; }
        if material_identifiers[cell_index(neighbor)] >> 30u == 1u {
            atomicOr(&cellular_awake[tile_index(vec2<u32>(neighbor) / 8u)], CELLULAR_TILE_AWAKE);
        }
    }
}

/// Static cells of the cellular tiles left in the workgroup's tile after breaking, as cell masks
var<workgroup> damage_occupancy: array<atomic<u32>, 2>;

/// Breaks every static cell of the tiles that lost all its integrity this tick: it becomes its
/// material's debris moving with the push of the pressure around it, keeping its color, or empty
/// when the material has none, and its stored pressure joins its new open volume as phantom air.
/// A broken rigid body cell also leaves its body's occupied cells and is listed for its body to
/// split, and every tile where cellular tile cells broke is listed with its remaining static
/// cells for collisions. Wakes the tile's dynamic cells and pressure and clears its broken flag
@compute @workgroup_size(64)
fn damage(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_cell: u32,
) {
    let tile: u32 = control.active_tiles[workgroup.x];
    let tile_position: vec2<u32> =
        vec2<u32>(tile % uniforms.region_width, tile / uniforms.region_width);
    let physical_tile: u32 = tile_index(tile_position);
    if (atomicLoad(&pressure_awake[physical_tile]) & TILE_BROKEN) == 0u { return; }
    if local_cell < 2u { atomicStore(&damage_occupancy[local_cell], 0u); }
    workgroupBarrier();
    let cell: vec2<i32> =
        vec2<i32>(tile_position * 8u + vec2<u32>(local_cell % 8u, local_cell / 8u));
    let index: u32 = cell_index(cell);
    var material_identifier: u32 = material_identifiers[index];
    let rigid_body_cell: u32 = rigid_body_cells[index];
    var is_broken: bool = is_static(material_identifier)
        && (kinematics[index].y & INTEGRITY_BROKEN) == INTEGRITY_BROKEN;
    // a rigid body cell breaks only once listed for its body to lose it, or waits a tick
    if is_broken && rigid_body_cell != 0u {
        let count: u32 = atomicAdd(&broken_rigid_body_cells[0], 1u);
        is_broken = count + 1u < arrayLength(&broken_rigid_body_cells);
        if is_broken { atomicStore(&broken_rigid_body_cells[count + 1u], rigid_body_cell - 1u); }
    }
    if is_broken {
        let material: vec4<f32> = static_materials[material_identifier & 0x3fffffffu];
        let debris: u32 = bitcast<u32>(material.w);
        // the pressure pushing on the cell's faces flings its debris
        var push: vec2<f32> = vec2<f32>(0.0);
        for (var face: u32 = 0u; face < 4u; face++) {
            let neighbor: vec2<i32> = cell + face_offset(face);
            if !is_cell_inside(neighbor) { continue; }
            push -= vec2<f32>(face_offset(face)) * pressures[cell_index(neighbor)];
        }
        let velocity: vec2<i32> = vec2<i32>(round(clamp(
            push * PRESSURE_IMPULSE / material.x,
            vec2<f32>(-SPEED_MAXIMUM),
            vec2<f32>(SPEED_MAXIMUM),
        ) * VELOCITY_SCALE));
        material_identifier = debris;
        material_identifiers[index] = debris;
        if debris == 0u { colors[index] = 0u; }
        kinematics[index] = select(
            vec2<u32>(0u),
            vec2<u32>((bitcast<u32>(velocity.x) & 0xffffu) | (bitcast<u32>(velocity.y) << 16u), 0u),
            debris != 0u,
        );
        // the stored pressure above the ambient pressure stays as gas in the opened cell, in the
        // buffer the last substep wrote
        let words: vec4<u32> = gas_out[index * 2u + 1u];
        let excess: f32 = f32(bitcast<i32>(words.w)) * solid_capacity(material.x);
        let phantom: f32 = clamp(
            f32(words.x) + f32(uniforms.ambient_amount) + excess,
            0.0,
            f32(AMOUNT_MAXIMUM),
        );
        gas_out[index * 2u + 1u] = vec4<u32>(u32(phantom), 0u, 0u, 0u);
        atomicOr(&cellular_awake[physical_tile], 3u);
        // a rigid body loses the cell, which stays here as debris until its body releases it
        if rigid_body_cell != 0u {
            let body_cell: u32 = rigid_body_cell - 1u;
            let bit: u32 = body_cell % 64u;
            atomicAnd(
                &rigid_body_cellular_tiles[body_cell / 64u * 8u + 4u + bit / 32u],
                ~(1u << (bit % 32u)),
            );
        }
    }
    if is_static(material_identifier) && rigid_body_cell == 0u {
        atomicOr(&damage_occupancy[local_cell / 32u], 1u << (local_cell % 32u));
    }
    workgroupBarrier();
    if local_cell == 0u {
        atomicAnd(&pressure_awake[physical_tile], ~TILE_BROKEN);
        atomicOr(&pressure_awake[physical_tile], TILE_AWAKE);
        // the tile and its remaining static cells, three words per listed tile
        let count: u32 = atomicAdd(&broken_cellular_tiles[0], 1u);
        if count * 3u + 3u < arrayLength(&broken_cellular_tiles) {
            atomicStore(&broken_cellular_tiles[count * 3u + 1u], physical_tile);
            atomicStore(&broken_cellular_tiles[count * 3u + 2u], atomicLoad(&damage_occupancy[0]));
            atomicStore(&broken_cellular_tiles[count * 3u + 3u], atomicLoad(&damage_occupancy[1]));
        }
    }
}
