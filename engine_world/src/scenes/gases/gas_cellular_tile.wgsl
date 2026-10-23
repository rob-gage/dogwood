// Copyright Rob Gage 2026

struct GasUniforms {
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
    padding_1: u32,
    padding_2: u32,
};

/// An actor displacing gas, laid out like the kinematics actor records
struct GasActor {
    center: vec2<f32>,
    velocity: vec2<f32>,
    rotation: vec2<f32>,
    size: vec2<f32>,
    bounds: vec4<f32>,
    shape: u32,
    mass: f32,
    padding: vec2<u32>,
};

struct GasControl {
    active_tile_count: atomic<u32>,
    active_tiles: array<u32>,
};

/// One cell's gas with the properties derived from it
struct GasCell {
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
    /// Gas amount per open volume, treating closed and nearly full cells as a quarter open
    pressure: f32,
    /// Mass per open volume
    density: f32,
    /// Total gas amount
    total: f32,
};

@group(0) @binding(0)
var<uniform> uniforms: GasUniforms;

@group(0) @binding(1)
var<storage, read> material_identifiers: array<u32>;

@group(0) @binding(2)
var<storage, read> gas_in: array<vec4<u32>>;

@group(0) @binding(3)
var<storage, read_write> gas_out: array<vec4<u32>>;

@group(0) @binding(4)
var<storage, read_write> gas_awake: array<atomic<u32>>;

@group(0) @binding(5)
var<storage, read_write> volumes: array<vec2<u32>>;

@group(0) @binding(6)
var<storage, read_write> control: GasControl;

@group(0) @binding(7)
var<storage, read> gas_materials: array<vec4<f32>>;

@group(0) @binding(8)
var<storage, read> fluid_raster: array<u32>;

@group(0) @binding(9)
var<storage, read> fluid_materials: array<vec4<f32>>;

@group(0) @binding(10)
var<storage, read> actors: array<GasActor>;

@group(1) @binding(0)
var<storage, read_write> dispatches: array<u32>;

/// Species slots of the workgroup's tile and the three rings of cells around it, which every
/// face and transfer the tile depends on reads, fourteen cells across
var<workgroup> halo_slots: array<vec4<u32>, 196>;

/// Phantom air, flow bits, and open volume words of the cells in the halo
var<workgroup> halo_words: array<vec4<u32>, 196>;

/// Packed velocities of the actors dragging the cells in the halo
var<workgroup> halo_drags: array<u32, 196>;

/// Pressure, density, total amount, and whether active of the cells in the halo
var<workgroup> halo_properties: array<vec4<f32>, 196>;

/// New right and top face flows of the tile and the two rings around it, twelve cells across
var<workgroup> halo_flows: array<vec2<f32>, 144>;

/// Limited fractions given through the left, right, bottom, and top faces of the tile and the
/// ring around it, ten cells across
var<workgroup> halo_transfers: array<vec4<u32>, 100>;

/// Gases admitted into free slots by the tile and the ring around it
var<workgroup> halo_admissions: array<u32, 100>;

/// Cell position of the halo's bottom-left cell
var<private> halo_origin: vec2<i32>;

/// Tile flag: the tile's gas or open volume changed, so it and its neighbors are simulated
const TILE_AWAKE: u32 = 1u;

/// Tile flag: the tile is simulated this tick
const TILE_ACTIVE: u32 = 4u;

/// Gas amount in one atmosphere
const AMOUNT_PER_ATMOSPHERE: f32 = 65536.0;

/// Transfer fraction units of a whole cell
const TRANSFER_SCALE: f32 = 65536.0;

/// Open volume units of a whole cell
const VOLUME_SCALE: f32 = 65536.0;

/// Open volume word flag: the cell is inside or next to an actor dragging its gas
const VOLUME_DRAGGED: u32 = 0x80000000u;

/// Distance from an actor's core, in multiples of its radius, within which it drags gas
const ACTOR_DRAG_RANGE: f32 = 6.0;

/// Fraction of an actor's velocity at which the middle of its wake follows it
const ACTOR_WAKE: f32 = 1.0;

/// Distance behind an actor's core, in multiples of its radius, of the middle of its eddies
const ACTOR_WAKE_OFFSET: f32 = 3.0;

/// Length of an actor's eddies along its path, in multiples of its radius
const ACTOR_WAKE_LENGTH: f32 = 3.0;

/// Fraction of the difference between a face flow and an actor's velocity removed per substep
const ACTOR_DRAG: f32 = 0.2;

/// Largest actor velocity in cells per tick, matching the kinematics actor records
const ACTOR_VELOCITY_MAXIMUM: f32 = 8.0;

/// Flow added per substep for each unit of difference in the net outflow of the cells across a
/// face, damping pressure waves without damping swirls
const BULK_VISCOSITY: f32 = 0.1;

/// Smoothed fluid occupancy above which a cell is closed to gas, matching the fluid renderer
const FLUID_OCCUPANCY_CLOSED: f32 = 0.5;

/// Smallest open volume used for pressure, so closed cells push their gas out without dividing
/// by zero
const VOLUME_MINIMUM: f32 = 0.25;

/// Flow added per substep for each unit of pressure difference, whose square root is the speed
/// of pressure waves in cells per substep
const PRESSURE_RESPONSE: f32 = 0.2;

/// Fraction of each face flow lost per substep
const FLOW_DAMPING: f32 = 0.004;

/// Flows smaller than this many amount units stop, so resting gas sleeps
const FLOW_MINIMUM: f32 = 32.0;

/// Largest speed in cells per substep that moves flows along themselves
const ADVECTION_SPEED_MAXIMUM: f32 = 0.9;

/// Largest fraction of a cell's gas its flows move out per substep
const BULK_FRACTION_MAXIMUM: f32 = 0.5;

/// Fraction of phantom air exchanged with each neighboring cell per substep
const AMBIENT_DIFFUSIVITY: f32 = 0.02;

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
    return (atomicLoad(&gas_awake[tile_index(vec2<u32>(cell) / 8u)]) & TILE_ACTIVE) != 0u;
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

/// Reads one cell's gas and derives its pressure and density, treating cells outside the active
/// region as closed and empty
fn load_cell(cell: vec2<i32>) -> GasCell {
    var gas_cell: GasCell;
    if !is_cell_inside(cell) { return gas_cell; }
    let index: u32 = cell_index(cell);
    let words: vec4<u32> = gas_in[index * 2u + 1u];
    gas_cell.slots = gas_in[index * 2u];
    gas_cell.phantom = words.x;
    gas_cell.flow = vec2<f32>(bitcast<f32>(words.y), bitcast<f32>(words.z));
    let volume_word: vec2<u32> = volumes[index];
    gas_cell.volume = f32(volume_word.x & ~VOLUME_DRAGGED) / VOLUME_SCALE;
    gas_cell.is_dragged = (volume_word.x & VOLUME_DRAGGED) != 0u;
    gas_cell.drag = unpack2x16snorm(volume_word.y) * ACTOR_VELOCITY_MAXIMUM
        * uniforms.actor_velocity_scale;
    gas_cell.is_active = is_cell_active(cell);
    var mass: f32 = f32(gas_cell.phantom) * uniforms.ambient_mass;
    var total: f32 = f32(gas_cell.phantom);
    for (var slot: u32 = 0u; slot < 4u; slot++) {
        let amount: f32 = f32(slot_amount(gas_cell.slots[slot]));
        if amount <= 0.0 { continue; }
        mass += amount * gas_materials[slot_gas(gas_cell.slots[slot])].x;
        total += amount;
    }
    let volume: f32 = max(gas_cell.volume, VOLUME_MINIMUM);
    gas_cell.total = total;
    gas_cell.pressure = total / volume;
    gas_cell.density = mass / volume;
    return gas_cell;
}

/// Reads the cells of a workgroup's active tile and the rings around it into the halo once, so
/// every neighbor read after it is local, and returns the invocation's cell position
fn load_halo(workgroup: u32, local_cell: u32) -> vec2<i32> {
    let tile: u32 = control.active_tiles[workgroup];
    let tile_position: vec2<u32> =
        vec2<u32>(tile % uniforms.region_width, tile / uniforms.region_width);
    halo_origin = vec2<i32>(tile_position * 8u) - 3;
    for (var halo_index: u32 = local_cell; halo_index < 196u; halo_index += 64u) {
        let gas_cell: GasCell =
            load_cell(halo_origin + vec2<i32>(i32(halo_index % 14u), i32(halo_index / 14u)));
        halo_slots[halo_index] = gas_cell.slots;
        halo_words[halo_index] = vec4<u32>(
            gas_cell.phantom,
            bitcast<u32>(gas_cell.flow.x),
            bitcast<u32>(gas_cell.flow.y),
            u32(gas_cell.volume * VOLUME_SCALE) | select(0u, VOLUME_DRAGGED, gas_cell.is_dragged),
        );
        halo_drags[halo_index] = pack2x16float(gas_cell.drag);
        halo_properties[halo_index] = vec4<f32>(
            gas_cell.pressure,
            gas_cell.density,
            gas_cell.total,
            select(0.0, 1.0, gas_cell.is_active),
        );
    }
    workgroupBarrier();
    return halo_origin + 3 + vec2<i32>(i32(local_cell % 8u), i32(local_cell / 8u));
}

/// Returns the halo index of a cell within three cells of the workgroup's tile
fn halo_index(cell: vec2<i32>) -> u32 {
    let local: vec2<u32> = vec2<u32>(cell - halo_origin);
    return local.y * 14u + local.x;
}

/// Returns the index of a cell within two cells of the workgroup's tile in the new flows
fn halo_flow_index(cell: vec2<i32>) -> u32 {
    let local: vec2<u32> = vec2<u32>(cell - halo_origin - 1);
    return local.y * 12u + local.x;
}

/// Returns the index of a cell within one cell of the workgroup's tile in the transfers
fn halo_transfer_index(cell: vec2<i32>) -> u32 {
    let local: vec2<u32> = vec2<u32>(cell - halo_origin - 2);
    return local.y * 10u + local.x;
}

/// Returns one cell of the halo
fn halo_cell(cell: vec2<i32>) -> GasCell {
    let index: u32 = halo_index(cell);
    let words: vec4<u32> = halo_words[index];
    let properties: vec4<f32> = halo_properties[index];
    var gas_cell: GasCell;
    gas_cell.slots = halo_slots[index];
    gas_cell.phantom = words.x;
    gas_cell.flow = vec2<f32>(bitcast<f32>(words.y), bitcast<f32>(words.z));
    gas_cell.volume = f32(words.w & ~VOLUME_DRAGGED) / VOLUME_SCALE;
    gas_cell.is_dragged = (words.w & VOLUME_DRAGGED) != 0u;
    gas_cell.drag = unpack2x16float(halo_drags[index]);
    gas_cell.is_active = properties.w != 0.0;
    gas_cell.pressure = properties.x;
    gas_cell.density = properties.y;
    gas_cell.total = properties.z;
    return gas_cell;
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
    giver: GasCell,
    receiver: GasCell,
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
    var next: f32 = carried + PRESSURE_RESPONSE * (giver.pressure - receiver.pressure)
        + BULK_VISCOSITY * divergence_difference;
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
            ACTOR_DRAG,
        );
    }
    next *= 1.0 - FLOW_DAMPING;
    // nothing flows into a closed cell
    if (receiver.volume <= 0.0 && next > 0.0) || (giver.volume <= 0.0 && next < 0.0) { next = 0.0; }
    if abs(next) < FLOW_MINIMUM { next = 0.0; }
    return next;
}

/// Returns whether a gas amount may move from one cell into another: only into open cells that
/// hold the gas or admitted it this substep, and only from clouds dense enough for its cohesion
/// unless the receiving cell already holds it or the giving cell is closed
fn is_transfer_allowed(
    giver: GasCell,
    receiver: GasCell,
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
    return f32(slot_amount(slot)) >= gas_materials[gas].z * AMOUNT_PER_ATMOSPHERE;
}

/// Returns the fraction a gas slot exchanges with a neighbor by diffusion, only between open cells
fn diffusion(giver: GasCell, receiver: GasCell, slot: u32) -> u32 {
    if giver.volume <= 0.0 || receiver.volume <= 0.0 { return 0u; }
    return u32(gas_materials[slot_gas(slot)].y * TRANSFER_SCALE);
}

/// Returns the fraction of phantom air exchanged with a neighbor by diffusion
fn ambient_diffusion(giver: GasCell, receiver: GasCell) -> u32 {
    if giver.volume <= 0.0 || receiver.volume <= 0.0 { return 0u; }
    return u32(AMBIENT_DIFFUSIVITY * TRANSFER_SCALE);
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
fn is_inside_actor(actor: GasActor, point: vec2<f32>) -> bool {
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
fn actor_stream(actor: GasActor, point: vec2<f32>) -> vec2<f32> {
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

/// Returns a cell's fluid occupancy smoothed over its neighbors like the fluid renderer, in
/// cells of the densest fluid present
fn fluid_occupancy(cell: vec2<i32>) -> f32 {
    var weighted_mass: f32 = 0.0;
    var dominant_identifier: u32 = 0u;
    for (var offset_y: i32 = -1; offset_y <= 1; offset_y++) {
        for (var offset_x: i32 = -1; offset_x <= 1; offset_x++) {
            let neighbor: vec2<i32> = cell + vec2<i32>(offset_x, offset_y);
            if !is_cell_inside(neighbor) { continue; }
            let raster_index: u32 = cell_index(neighbor) * 4u;
            let mass: f32 = max(f32(bitcast<i32>(fluid_raster[raster_index])), 0.0);
            if mass <= 0.0 { continue; }
            let weight: f32 = select(select(2.0, 1.0, offset_x != 0 && offset_y != 0), 4.0,
                offset_x == 0 && offset_y == 0);
            weighted_mass += mass * weight;
            dominant_identifier = max(dominant_identifier, fluid_raster[raster_index + 3u]);
        }
    }
    if dominant_identifier == 0u { return 0.0; }
    let density: f32 = fluid_materials[(dominant_identifier & 0x3fffffffu) * 2u].x;
    if density <= 0.0 { return 0.0; }
    return weighted_mass / (uniforms.fluid_raster_scale * 16.0) / density;
}

/// Measures the open volume of every cell of the ring: closed under cellular matter, fluid the
/// renderer draws, and actors, open otherwise; waking tiles whose cells opened or closed
@compute @workgroup_size(64)
fn volume(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_cell: u32,
) {
    let index: u32 = workgroup.x * 64u + local_cell;
    let physical_tile: vec2<u32> =
        vec2<u32>(workgroup.x % uniforms.region_width, workgroup.x / uniforms.region_width);
    let tile: vec2<u32> = vec2<u32>(
        (physical_tile.x + uniforms.region_width - uniforms.ring_offset_x) % uniforms.region_width,
        (physical_tile.y + uniforms.region_height - uniforms.ring_offset_y)
            % uniforms.region_height,
    );
    let cell: vec2<i32> = vec2<i32>(tile * 8u + vec2<u32>(local_cell % 8u, local_cell / 8u));
    var is_open: bool =
        material_identifiers[index] == 0u && fluid_occupancy(cell) <= FLUID_OCCUPANCY_CLOSED;
    let center: vec2<f32> = vec2<f32>(cell) + 0.5;
    let corner: vec2<f32> = vec2<f32>(cell);
    // the stream function at the top right, bottom right, and top left corners
    var streams: vec3<f32> = vec3<f32>(0.0);
    var is_dragged: bool = false;
    for (var actor_index: u32 = 0u; actor_index < uniforms.actor_count; actor_index++) {
        let actor: GasActor = actors[actor_index];
        let reach: vec2<f32> = 0.5 * (actor.bounds.zw - actor.bounds.xy) * ACTOR_DRAG_RANGE;
        if any(abs(center - actor.center) > reach) { continue; }
        if actor_stream(actor, center).y == 0.0 { continue; }
        is_dragged = true;
        if is_inside_actor(actor, center) { is_open = false; }
        streams += vec3<f32>(
            actor_stream(actor, corner + vec2<f32>(1.0, 1.0)).x,
            actor_stream(actor, corner + vec2<f32>(1.0, 0.0)).x,
            actor_stream(actor, corner + vec2<f32>(0.0, 1.0)).x,
        );
    }
    let drag: vec2<f32> = vec2<f32>(streams.x - streams.y, streams.z - streams.x);
    let encoded: vec2<u32> = vec2<u32>(
        select(0u, u32(VOLUME_SCALE), is_open) | select(0u, VOLUME_DRAGGED, is_dragged),
        select(0u, pack2x16snorm(drag / ACTOR_VELOCITY_MAXIMUM), is_dragged),
    );
    if any(volumes[index] != encoded) {
        volumes[index] = encoded;
        atomicOr(&gas_awake[workgroup.x], TILE_AWAKE);
    }
}

/// Lists the tiles whose gas is simulated this tick: those awake and their neighbors
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
            if (atomicLoad(&gas_awake[tile_index(vec2<u32>(neighbor))]) & TILE_AWAKE) != 0u {
                is_active = true;
            }
        }
    }
    // neighbors only read the awake flag, which this store keeps
    let index: u32 = tile_index(vec2<u32>(tile));
    let flags: u32 = atomicLoad(&gas_awake[index]) & TILE_AWAKE;
    atomicStore(&gas_awake[index], select(flags, flags | TILE_ACTIVE, is_active));
    if is_active {
        control.active_tiles[atomicAdd(&control.active_tile_count, 1u)] = invocation.x;
    }
}

/// Clears the awake flag of every tile once collected, for this tick's simulation to set
@compute @workgroup_size(64)
fn clear(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if invocation.x >= uniforms.region_width * uniforms.region_height { return; }
    atomicAnd(&gas_awake[invocation.x], TILE_ACTIVE);
}

/// Writes the indirect dispatch of one workgroup per active tile
@compute @workgroup_size(1)
fn prepare() {
    dispatches[0] = atomicLoad(&control.active_tile_count);
    dispatches[1] = 1u;
    dispatches[2] = 1u;
}

/// Returns a halo cell's net outflow through its faces from the previous substep
fn cell_divergence(cell: vec2<i32>) -> f32 {
    return flow_x(cell) - flow_x(cell + vec2<i32>(-1, 0))
        + flow_y(cell) - flow_y(cell + vec2<i32>(0, -1));
}

/// Returns the new flows through an active cell's right and top faces, waking sleeping neighbors
/// the flows would push against, or zero for a cell outside the simulated tiles
fn cell_flows(cell: vec2<i32>) -> vec2<f32> {
    let center: GasCell = halo_cell(cell);
    var flows: vec2<f32> = vec2<f32>(0.0);
    if !center.is_active { return flows; }
    for (var axis: u32 = 0u; axis < 2u; axis++) {
        let is_vertical: bool = axis == 1u;
        let neighbor_cell: vec2<i32> = cell + select(vec2<i32>(1, 0), vec2<i32>(0, 1), is_vertical);
        if !is_cell_inside(neighbor_cell) { continue; }
        let neighbor: GasCell = halo_cell(neighbor_cell);
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
        let flow: f32 = face_flow(
            center,
            neighbor,
            select(center.flow.x, center.flow.y, is_vertical),
            face,
            cross_flow,
            cell_divergence(neighbor_cell) - cell_divergence(cell),
            is_vertical,
        );
        // a sleeping neighbor wakes for the next tick instead of exchanging gas this tick
        if !neighbor.is_active {
            if flow != 0.0 {
                atomicOr(&gas_awake[tile_index(vec2<u32>(neighbor_cell) / 8u)], TILE_AWAKE);
            }
            continue;
        }
        flows[axis] = flow;
    }
    return flows;
}

/// Returns the new flows of each face of a cell within one cell of the tile, positive out of it
fn cell_outgoing_flows(cell: vec2<i32>) -> vec4<f32> {
    let flows: vec2<f32> = halo_flows[halo_flow_index(cell)];
    return vec4<f32>(
        -halo_flows[halo_flow_index(cell + vec2<i32>(-1, 0))].x,
        flows.x,
        -halo_flows[halo_flow_index(cell + vec2<i32>(0, -1))].y,
        flows.y,
    );
}

/// Returns the limited fractions of an active cell's gas its outgoing flows give through each
/// face, zero for a cell outside the simulated tiles
fn cell_transfers(cell: vec2<i32>) -> vec4<u32> {
    let center: GasCell = halo_cell(cell);
    if !center.is_active || center.total <= 0.0 { return vec4<u32>(0u); }
    var fractions: vec4<f32> = max(cell_outgoing_flows(cell), vec4<f32>(0.0)) / center.total;
    let fraction_sum: f32 = fractions.x + fractions.y + fractions.z + fractions.w;
    if fraction_sum > BULK_FRACTION_MAXIMUM { fractions *= BULK_FRACTION_MAXIMUM / fraction_sum; }
    return vec4<u32>(fractions * TRANSFER_SCALE);
}

/// Returns the gases an active cell admits into its free slots: the largest new gases its
/// neighbors could give it by flow or diffusion, packed as gas index plus one per byte
fn cell_admission(cell: vec2<i32>) -> u32 {
    let center: GasCell = halo_cell(cell);
    if !center.is_active || center.volume <= 0.0 { return 0u; }
    var free_count: u32 = 0u;
    for (var slot: u32 = 0u; slot < 4u; slot++) {
        if slot_amount(center.slots[slot]) == 0u { free_count += 1u; }
    }
    if free_count == 0u { return 0u; }
    let signed_outgoing: vec4<f32> = cell_outgoing_flows(cell);
    var candidate_gases: array<u32, 16>;
    var candidate_amounts: array<u32, 16>;
    var candidate_count: u32 = 0u;
    for (var face: u32 = 0u; face < 4u; face++) {
        let neighbor_cell: vec2<i32> = cell + face_offset(face);
        let neighbor: GasCell = halo_cell(neighbor_cell);
        if !neighbor.is_active { continue; }
        let is_flowing_in: bool = signed_outgoing[face] < 0.0;
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
                && f32(amount) < gas_materials[gas].z * AMOUNT_PER_ATMOSPHERE
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

/// Simulates one substep of each active tile's gas: updates the face flows, limits the gas they
/// give, chooses the gases free slots admit, then moves gas through every face by the limited
/// flows and diffusion, writing each cell's new species slots, phantom air, and the flows its
/// faces actually carried. Each workgroup recomputes the flows and transfers of the rings around
/// its tile instead of waiting for its neighbors, which is exact because every workgroup
/// computes them from the same previous substep
@compute @workgroup_size(64)
fn step(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_cell: u32,
) {
    let cell: vec2<i32> = load_halo(workgroup.x, local_cell);
    for (var flow_index: u32 = local_cell; flow_index < 144u; flow_index += 64u) {
        halo_flows[flow_index] = cell_flows(
            halo_origin + 1 + vec2<i32>(i32(flow_index % 12u), i32(flow_index / 12u)),
        );
    }
    workgroupBarrier();
    for (var transfer_index: u32 = local_cell; transfer_index < 100u; transfer_index += 64u) {
        let transfer_cell: vec2<i32> =
            halo_origin + 2 + vec2<i32>(i32(transfer_index % 10u), i32(transfer_index / 10u));
        halo_transfers[transfer_index] = cell_transfers(transfer_cell);
        halo_admissions[transfer_index] = cell_admission(transfer_cell);
    }
    workgroupBarrier();
    let index: u32 = cell_index(cell);
    let center: GasCell = halo_cell(cell);
    let center_transfers: vec4<u32> = halo_transfers[halo_transfer_index(cell)];
    let center_admission: u32 = halo_admissions[halo_transfer_index(cell)];
    var amounts: array<u32, 4>;
    var gases: array<u32, 4>;
    for (var slot: u32 = 0u; slot < 4u; slot++) {
        amounts[slot] = slot_amount(center.slots[slot]);
        gases[slot] = slot_gas(center.slots[slot]);
    }
    var phantom: u32 = center.phantom;
    var flows: vec2<f32> = vec2<f32>(0.0);
    for (var face: u32 = 0u; face < 4u; face++) {
        let neighbor_cell: vec2<i32> = cell + face_offset(face);
        let neighbor: GasCell = halo_cell(neighbor_cell);
        if !neighbor.is_active { continue; }
        let neighbor_transfers: vec4<u32> = halo_transfers[halo_transfer_index(neighbor_cell)];
        let neighbor_admission: u32 = halo_admissions[halo_transfer_index(neighbor_cell)];
        let giving: u32 = center_transfers[face];
        let receiving: u32 = neighbor_transfers[opposite_face(face)];
        // record the flow this cell's right and top faces carried, positive toward the neighbor
        if face == 1u || face == 3u {
            flows[face / 2u] = (f32(giving) * center.total - f32(receiving) * neighbor.total)
                / TRANSFER_SCALE;
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
            amounts[target_slot] = min(amounts[target_slot] + amount, 0xffffffu);
        }
        if center.volume > 0.0 {
            phantom += transfer(neighbor.phantom, receiving + ambient_diffusion(neighbor, center));
        }
    }
    var slots: vec4<u32> = vec4<u32>(0u);
    for (var slot: u32 = 0u; slot < 4u; slot++) {
        if amounts[slot] > 0u { slots[slot] = (gases[slot] << 24u) | amounts[slot]; }
    }
    if any(slots != center.slots) || phantom != center.phantom || any(flows != vec2<f32>(0.0)) {
        atomicOr(&gas_awake[tile_index(vec2<u32>(cell) / 8u)], TILE_AWAKE);
    }
    gas_out[index * 2u] = slots;
    gas_out[index * 2u + 1u] = vec4<u32>(phantom, bitcast<u32>(flows.x), bitcast<u32>(flows.y), 0u);
}
