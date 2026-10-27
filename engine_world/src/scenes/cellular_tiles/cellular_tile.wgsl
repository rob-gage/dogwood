// Copyright Rob Gage 2026

struct CellularTileUniforms {
    region_width: u32,
    region_height: u32,
    ring_offset_x: u32,
    ring_offset_y: u32,
    tick: u32,
    substep: u32,
    actor_count: u32,
    gravity: f32,
    resolve: u32,
    padding: u32,
    region_origin: vec2<i32>,
};

struct CellularTileActor {
    center: vec2<f32>,
    velocity: vec2<f32>,
    rotation: vec2<f32>,
    size: vec2<f32>,
    bounds: vec4<f32>,
    shape: u32,
    mass: f32,
    padding: vec2<u32>,
};

struct CellularTileRigidBodyCellularTile {
    rigid_body: u32,
    padding_0: u32,
    tile: vec2<i32>,
    cells: vec2<u32>,
    previous_cells: vec2<u32>,
};

struct CellularTileRigidBody {
    position: vec2<f32>,
    rotation: vec2<f32>,
    previous_position: vec2<f32>,
    previous_rotation: vec2<f32>,
    velocity: vec2<f32>,
    center: vec2<f32>,
    angular_velocity: f32,
    mass: f32,
    padding: vec2<u32>,
};

struct CellularTileControl {
    active_tile_count: atomic<u32>,
    step_count: atomic<u32>,
    pending_resolves: atomic<u32>,
    active_tiles: array<u32>,
};

struct CellKinematics {
    velocity: vec2<f32>,
    progress: vec2<f32>,
    flags: u32,
};

@group(0) @binding(0)
var<uniform> uniforms: CellularTileUniforms;

@group(0) @binding(1)
var<storage, read_write> material_identifiers: array<u32>;

@group(0) @binding(2)
var<storage, read_write> colors: array<u32>;

@group(0) @binding(3)
var<storage, read_write> kinematics: array<vec2<u32>>;

@group(0) @binding(4)
var<storage, read_write> awake: array<atomic<u32>>;

@group(0) @binding(5)
var<storage, read> materials: array<vec4<f32>>;

@group(0) @binding(6)
var<storage, read_write> motions: array<vec4<u32>>;

@group(0) @binding(7)
var<storage, read_write> staged: array<vec4<u32>>;

@group(0) @binding(8)
var<storage, read_write> control: CellularTileControl;

@group(0) @binding(9)
var<storage, read> actors: array<CellularTileActor>;

@group(0) @binding(10)
var<storage, read_write> contacts: array<atomic<i32>>;

@group(0) @binding(11)
var<storage, read> rigid_body_cells: array<u32>;

@group(0) @binding(12)
var<storage, read> rigid_body_cellular_tile_cells: array<vec2<u32>>;

@group(0) @binding(13)
var<storage, read> rigid_body_cellular_tiles: array<CellularTileRigidBodyCellularTile>;

@group(0) @binding(14)
var<storage, read> rigid_bodies: array<CellularTileRigidBody>;

@group(0) @binding(15)
var<storage, read_write> rigid_body_contacts: array<atomic<u32>>;

@group(0) @binding(18)
var<storage, read> pressure: array<f32>;

@group(0) @binding(20)
var<storage, read_write> matter: array<vec2<u32>>;

@group(0) @binding(21)
var<storage, read> rigid_body_cell_states: array<vec4<u32>>;

@group(1) @binding(0)
var<storage, read_write> dispatches: array<u32>;

/// Tile flag: the tile holds dynamic cells that are not resting
const TILE_AWAKE: u32 = 1u;

/// Tile flag: the tile's cells changed, so it and its neighbors are simulated next tick
const TILE_DISTURBED: u32 = 2u;

/// Tile flag: the tile is simulated this tick
const TILE_ACTIVE: u32 = 4u;

/// Cell flag: the cell rests on support and is left in place until disturbed
const CELL_RESTING: u32 = 1u << 28u;

/// Cell flag: the cell cannot fall, even while sliding, so it holds up the cells above it
const CELL_SUPPORTED: u32 = 1u << 29u;

/// Motion flag beside the requested direction: an actor covers the cell
const MOTION_COVERED: u32 = 16u;

/// Mask of the requested direction in a motion
const MOTION_DIRECTION: u32 = 15u;

/// Move status waiting on the cell it points to
const STATUS_PENDING: u32 = 0u;

/// Move status of a cell that moves this substep
const STATUS_SUCCEEDED: u32 = 1u << 30u;

/// Move status of a cell that stays this substep
const STATUS_FAILED: u32 = 2u << 30u;

/// Move status of a cell that requested no move
const STATUS_NONE: u32 = 3u << 30u;

/// Mask of the status above the cell index a pending status points to
const STATUS_MASK: u32 = 3u << 30u;

/// Packed velocity units per cell per tick
const VELOCITY_SCALE: f32 = 1024.0;

/// Packed progress units per cell
const PROGRESS_SCALE: f32 = 512.0;

/// Largest speed along each axis in cells per tick, one cell per substep
const SPEED_MAXIMUM: f32 = 8.0;

/// Largest progress toward the next cell along each axis, which one tick's substeps can consume
const PROGRESS_MAXIMUM: f32 = 8.99;

/// Speed in cells per tick below which a supported cell comes to rest
const RESTING_SPEED: f32 = 0.02;

/// Fraction of the rebound two dynamic cells keep when they collide, so that colliding cells lose
/// energy beyond their restitution and settle
const COLLISION_DAMPING: f32 = 0.5;

/// Fixed-point units per cell of accumulated contacts
const CONTACT_SCALE: f32 = 1024.0;

/// Number of `i32` accumulated per actor: impulse, normal, contact count, depth, and padding
const CONTACT_STRIDE: u32 = 8u;

/// Number of `f32` accumulated per rigid body: impulse, angular impulse, and wet face fluid data
const RIGID_BODY_CONTACT_STRIDE: u32 = 11u;

/// Speed in cells per tick below which a rigid body pushes covered cells away from its center
const RIGID_BODY_RESTING_SPEED: f32 = 0.01;

/// Pressure impulse in mass * cells per tick per atmosphere, matching the other pressure shaders
const PRESSURE_IMPULSE: f32 = 0.1111;

/// Absolute gauge pressure below this value in atmospheres does not produce an impulse
const PRESSURE_PUSH_MINIMUM: f32 = 0.02;

/// Returns the physical index of a tile from its position in the active region
fn tile_index(tile: vec2<u32>) -> u32 {
    let physical_x: u32 = (tile.x + uniforms.ring_offset_x) % uniforms.region_width;
    let physical_y: u32 = (tile.y + uniforms.ring_offset_y) % uniforms.region_height;
    return physical_y * uniforms.region_width + physical_x;
}

/// Returns the pressure push on a cell from the gauges on its in-region faces
fn pressure_push(cell: vec2<i32>) -> vec2<f32> {
    let offsets: array<vec2<i32>, 4> = array<vec2<i32>, 4>(
        vec2<i32>(-1, 0),
        vec2<i32>(1, 0),
        vec2<i32>(0, -1),
        vec2<i32>(0, 1),
    );
    var push: vec2<f32> = vec2<f32>(0.0);
    for (var face: u32 = 0u; face < 4u; face++) {
        let neighbor: vec2<i32> = cell + offsets[face];
        if !is_cell_inside(neighbor) { continue; }
        let gauge: f32 = pressure[cell_index(neighbor)];
        if abs(gauge) < PRESSURE_PUSH_MINIMUM { continue; }
        push -= vec2<f32>(offsets[face]) * gauge;
    }
    return push;
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
    return (atomicLoad(&awake[tile_index(vec2<u32>(cell) / 8u)]) & TILE_ACTIVE) != 0u;
}

/// Returns the cell position of one invocation of a workgroup simulating one active tile
fn workgroup_cell(workgroup: u32, cell: u32) -> vec2<i32> {
    let tile: u32 = control.active_tiles[workgroup];
    let tile_position: vec2<u32> =
        vec2<u32>(tile % uniforms.region_width, tile / uniforms.region_width);
    return vec2<i32>(tile_position * 8u + vec2<u32>(cell % 8u, cell / 8u));
}

/// Returns whether a material identifier is a dynamic cellular material
fn is_dynamic(material_identifier: u32) -> bool { return material_identifier >> 30u == 1u; }

/// Returns whether a material identifier is a static cellular material other than empty
fn is_static(material_identifier: u32) -> bool {
    return material_identifier != 0u && material_identifier >> 30u == 0u;
}

/// Returns the offset of a direction numbered 1 to 8 counterclockwise from +x, or zero
fn direction_offset(direction: u32) -> vec2<i32> {
    switch direction {
        case 1u: { return vec2<i32>(1, 0); }
        case 2u: { return vec2<i32>(1, 1); }
        case 3u: { return vec2<i32>(0, 1); }
        case 4u: { return vec2<i32>(-1, 1); }
        case 5u: { return vec2<i32>(-1, 0); }
        case 6u: { return vec2<i32>(-1, -1); }
        case 7u: { return vec2<i32>(0, -1); }
        case 8u: { return vec2<i32>(1, -1); }
        default: { return vec2<i32>(0, 0); }
    }
}

/// Returns the direction of an offset with components of -1, 0, or 1, or zero for no offset
fn offset_direction(offset: vec2<i32>) -> u32 {
    for (var direction: u32 = 1u; direction <= 8u; direction++) {
        if all(direction_offset(direction) == offset) { return direction; }
    }
    return 0u;
}

/// Returns the direction pointing the opposite way
fn opposite_direction(direction: u32) -> u32 { return (direction + 3u) % 8u + 1u; }

/// Returns a direction turned counterclockwise by a number of eighth turns
fn turn_direction(direction: u32, turns: i32) -> u32 {
    return u32((i32(direction) - 1 + turns + 8) % 8) + 1u;
}

/// Returns a well-distributed hash of four values
fn hash(value: vec4<u32>) -> u32 {
    var state: u32 = 0u;
    for (var component: u32 = 0u; component < 4u; component++) {
        state = (state ^ value[component]) * 747796405u + 2891336453u;
        let word: u32 = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
        state = (word >> 22u) ^ word;
    }
    return state;
}

/// Unpacks velocity, progress toward the next cell, and flags from two words
fn unpack_kinematics(packed: vec2<u32>) -> CellKinematics {
    let velocity_bits: i32 = bitcast<i32>(packed.x);
    let progress_bits: i32 = bitcast<i32>(packed.y);
    return CellKinematics(
        vec2<f32>(
            f32(extractBits(velocity_bits, 0u, 16u)),
            f32(extractBits(velocity_bits, 16u, 16u)),
        ) / VELOCITY_SCALE,
        vec2<f32>(
            f32(extractBits(progress_bits, 0u, 14u)),
            f32(extractBits(progress_bits, 14u, 14u)),
        ) / PROGRESS_SCALE,
        packed.y & 0xf0000000u,
    );
}

/// Packs velocity as two 16-bit and progress as two 14-bit signed values below four flag bits
fn pack_kinematics(cell_kinematics: CellKinematics) -> vec2<u32> {
    let velocity: vec2<i32> = vec2<i32>(round(
        clamp(cell_kinematics.velocity, vec2<f32>(-SPEED_MAXIMUM), vec2<f32>(SPEED_MAXIMUM))
            * VELOCITY_SCALE,
    ));
    let progress: vec2<i32> = vec2<i32>(round(
        clamp(cell_kinematics.progress, vec2<f32>(-15.99), vec2<f32>(15.99)) * PROGRESS_SCALE,
    ));
    return vec2<u32>(
        (bitcast<u32>(velocity.x) & 0xffffu) | (bitcast<u32>(velocity.y) << 16u),
        (bitcast<u32>(progress.x) & 0x3fffu)
            | ((bitcast<u32>(progress.y) & 0x3fffu) << 14u)
            | cell_kinematics.flags,
    );
}

/// Returns the rigid body whose cell is rasterized at a buffer index plus one, or zero for none
fn cell_rigid_body(index: u32) -> u32 {
    let rigid_body_cell: u32 = rigid_body_cells[index];
    if rigid_body_cell == 0u { return 0u; }
    return rigid_body_cellular_tiles[(rigid_body_cell - 1u) / 64u].rigid_body + 1u;
}

/// Returns the arm in cells from a rigid body's center of mass to a cell position's center
fn rigid_body_arm(rigid_body: u32, cell: vec2<i32>) -> vec2<f32> {
    return vec2<f32>(uniforms.region_origin + cell) + 0.5 - rigid_bodies[rigid_body].center;
}

/// Returns the velocity in cells per tick of a rigid body at a cell position
fn rigid_body_velocity(rigid_body: u32, cell: vec2<i32>) -> vec2<f32> {
    let arm: vec2<f32> = rigid_body_arm(rigid_body, cell);
    return rigid_bodies[rigid_body].velocity
        + rigid_bodies[rigid_body].angular_velocity * vec2<f32>(-arm.y, arm.x);
}

/// Adds a value to one accumulated rigid body contact
fn add_rigid_body_contact(index: u32, value: f32) {
    var accumulated: u32 = atomicLoad(&rigid_body_contacts[index]);
    loop {
        let exchange = atomicCompareExchangeWeak(
            &rigid_body_contacts[index],
            accumulated,
            bitcast<u32>(bitcast<f32>(accumulated) + value),
        );
        if exchange.exchanged { break; }
        accumulated = exchange.old_value;
    }
}

/// Accumulates an impulse in mass cells per tick on a rigid body at a cell position, and the
/// angular impulse it applies around the body's center of mass
fn push_rigid_body(rigid_body: u32, cell: vec2<i32>, impulse: vec2<f32>) {
    let arm: vec2<f32> = rigid_body_arm(rigid_body, cell);
    let contact_index: u32 = rigid_body * RIGID_BODY_CONTACT_STRIDE;
    add_rigid_body_contact(contact_index, impulse.x);
    add_rigid_body_contact(contact_index + 1u, impulse.y);
    add_rigid_body_contact(contact_index + 2u, arm.x * impulse.y - arm.y * impulse.x);
}

/// Returns the velocity in cells per tick of the rigid body cell at a cell position, or zero when
/// the position holds other matter
fn wall_velocity(cell: vec2<i32>) -> vec2<f32> {
    if !is_cell_inside(cell) { return vec2<f32>(0.0); }
    let index: u32 = cell_index(cell);
    if rigid_body_cells[index] == 0u || !is_static(material_identifiers[index]) {
        return vec2<f32>(0.0);
    }
    return unpack_kinematics(kinematics[index]).velocity;
}

/// Accumulates the impulse a dynamic cell applies to the rigid body cell it hits, if any
fn push_wall(cell: vec2<i32>, impulse: vec2<f32>) {
    if !is_cell_inside(cell) { return; }
    let index: u32 = cell_index(cell);
    if !is_static(material_identifiers[index]) { return; }
    let rigid_body: u32 = cell_rigid_body(index);
    if rigid_body != 0u { push_rigid_body(rigid_body - 1u, cell, impulse); }
}

/// Returns the signed distance in cells from an actor's surface to a point, negative inside,
/// followed by the surface normal pointing out of the actor
fn actor_distance(actor: CellularTileActor, point: vec2<f32>) -> vec3<f32> {
    // measure in the actor's unrotated frame
    let relative: vec2<f32> = point - actor.center;
    let local: vec2<f32> = vec2<f32>(
        actor.rotation.x * relative.x + actor.rotation.y * relative.y,
        actor.rotation.x * relative.y - actor.rotation.y * relative.x,
    );
    var distance: f32;
    var normal: vec2<f32>;
    switch actor.shape {
        // a rectangle with half extents as its size
        case 1u: {
            let excess: vec2<f32> = abs(local) - actor.size;
            let side: vec2<f32> = select(vec2<f32>(1.0), vec2<f32>(-1.0), local < vec2<f32>(0.0));
            if excess.x > 0.0 || excess.y > 0.0 {
                let outside: vec2<f32> = max(excess, vec2<f32>(0.0));
                distance = length(outside);
                normal = outside * side / distance;
            } else if excess.x > excess.y {
                distance = excess.x;
                normal = vec2<f32>(side.x, 0.0);
            } else {
                distance = excess.y;
                normal = vec2<f32>(0.0, side.y);
            }
        }
        // a vertical capsule with its half segment length and radius as its size
        case 2u: {
            let offset: vec2<f32> =
                local - vec2<f32>(0.0, clamp(local.y, -actor.size.x, actor.size.x));
            let offset_length: f32 = length(offset);
            distance = offset_length - actor.size.y;
            normal = select(vec2<f32>(0.0, 1.0), offset / offset_length, offset_length > 0.0001);
        }
        // a circle with its radius as its size
        default: {
            let local_length: f32 = length(local);
            distance = local_length - actor.size.x;
            normal = select(vec2<f32>(0.0, 1.0), local / local_length, local_length > 0.0001);
        }
    }
    return vec3<f32>(
        distance,
        actor.rotation.x * normal.x - actor.rotation.y * normal.y,
        actor.rotation.y * normal.x + actor.rotation.x * normal.y,
    );
}

/// Returns the index of the first actor covering a point, or the actor count when none does
fn covering_actor(point: vec2<f32>) -> u32 {
    for (var actor_index: u32 = 0u; actor_index < uniforms.actor_count; actor_index++) {
        let actor: CellularTileActor = actors[actor_index];
        if any(point < actor.bounds.xy) || any(point > actor.bounds.zw) { continue; }
        if actor_distance(actor, point).x < 0.0 { return actor_index; }
    }
    return uniforms.actor_count;
}

/// Returns whether a cell holds up a dynamic cell above it: the region's edge, static matter,
/// an actor, or a dynamic cell that cannot fall
fn is_cell_supporting(cell: vec2<i32>) -> bool {
    if !is_cell_inside(cell) { return true; }
    let index: u32 = cell_index(cell);
    let material_identifier: u32 = material_identifiers[index];
    if is_static(material_identifier) { return true; }
    if is_dynamic(material_identifier)
        && (kinematics[index].y & (CELL_RESTING | CELL_SUPPORTED)) != 0u
    { return true; }
    if rigid_body_cells[index] != 0u { return true; }
    return covering_actor(vec2<f32>(cell) + 0.5) < uniforms.actor_count;
}

/// Returns whether a moving cell cannot enter a cell at all this substep, rather than only
/// losing it to another cell or waiting for its occupant to leave
fn is_cell_blocking(cell: vec2<i32>) -> bool {
    if !is_cell_inside(cell) { return true; }
    let index: u32 = cell_index(cell);
    let material_identifier: u32 = material_identifiers[index];
    if is_static(material_identifier) { return true; }
    if !is_cell_active(cell) { return material_identifier != 0u; }
    let motion: u32 = motions[index].x;
    if (motion & MOTION_COVERED) != 0u { return true; }
    return is_dynamic(material_identifier) && (motion & MOTION_DIRECTION) == 0u;
}

/// Marks every tile simulated this tick: tiles that are awake or disturbed, their neighbors, and
/// tiles near actors, listing them for indirect dispatches
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
            let flags: u32 = atomicLoad(&awake[tile_index(vec2<u32>(neighbor))]);
            if (flags & (TILE_AWAKE | TILE_DISTURBED)) != 0u { is_active = true; }
        }
    }
    let minimum: vec2<f32> = vec2<f32>(tile * 8) - 8.0;
    let maximum: vec2<f32> = vec2<f32>(tile * 8) + 16.0;
    for (var actor_index: u32 = 0u; actor_index < uniforms.actor_count; actor_index++) {
        let bounds: vec4<f32> = actors[actor_index].bounds;
        if all(bounds.xy <= maximum) && all(bounds.zw >= minimum) { is_active = true; }
    }
    if !is_active && tile_has_fluid_interaction(tile) { is_active = true; }
    // neighbors only read the awake and disturbed flags, which this store keeps
    let index: u32 = tile_index(vec2<u32>(tile));
    let flags: u32 = atomicLoad(&awake[index]) & (TILE_AWAKE | TILE_DISTURBED);
    atomicStore(&awake[index], select(flags, flags | TILE_ACTIVE, is_active));
    if is_active {
        control.active_tiles[atomicAdd(&control.active_tile_count, 1u)] = invocation.x;
    }
}

/// Clears the awake and disturbed flags of every tile once they are collected
@compute @workgroup_size(64)
fn clear(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if invocation.x >= uniforms.region_width * uniforms.region_height { return; }
    atomicAnd(&awake[invocation.x], TILE_ACTIVE);
}

/// Writes the indirect dispatches of whole ticks and of the substeps the fastest cell needs
@compute @workgroup_size(1)
fn prepare() {
    let active_tile_count: u32 = atomicLoad(&control.active_tile_count);
    let step_count: u32 = atomicLoad(&control.step_count);
    dispatches[0] = active_tile_count;
    dispatches[1] = 1u;
    dispatches[2] = 1u;
    for (var substep: u32 = 0u; substep < 8u; substep++) {
        dispatches[3u + substep * 3u] = select(0u, active_tile_count, substep < step_count);
        dispatches[4u + substep * 3u] = 1u;
        dispatches[5u + substep * 3u] = 1u;
    }
}

/// Accelerates every dynamic cell once per tick: cells covered by actors are pushed out of them,
/// supported cells slide down slopes their friction cannot hold and otherwise come to rest, and
/// unsupported cells fall
@compute @workgroup_size(64)
fn integrate(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_cell: u32,
) {
    let cell: vec2<i32> = workgroup_cell(workgroup.x, local_cell);
    let index: u32 = cell_index(cell);
    let center: vec2<f32> = vec2<f32>(cell) + 0.5;
    let covering: u32 = covering_actor(center);
    let material_identifier: u32 = material_identifiers[index];
    // a dynamic cell under a rigid body is covered by it until it escapes
    let rigid_body: u32 = select(0u, cell_rigid_body(index), is_dynamic(material_identifier));
    let is_covered: bool = covering < uniforms.actor_count || rigid_body != 0u;
    motions[index] = vec4<u32>(
        select(0u, MOTION_COVERED, is_covered),
        0u,
        STATUS_NONE,
        STATUS_NONE,
    );
    if !is_dynamic(material_identifier) { return; }
    let material: vec4<f32> = materials[material_identifier & 0x3fffffffu];
    let mass: f32 = material.x;
    let friction: f32 = material.y;
    let gravity: f32 = uniforms.gravity;
    var cell_kinematics: CellKinematics = unpack_kinematics(kinematics[index]);
    cell_kinematics.flags = 0u;
    var pressure_velocity_change: vec2<f32> = vec2<f32>(0.0);
    if !is_covered {
        pressure_velocity_change = pressure_push(cell) * PRESSURE_IMPULSE / mass;
        if is_cell_supporting(cell + vec2<i32>(0, -1)) {
            pressure_velocity_change.y = max(pressure_velocity_change.y, 0.0);
        }
        cell_kinematics.velocity += pressure_velocity_change;
    }
    if is_covered {
        // leave an actor through the free cell nearest its outward normal, and a rigid body the
        // way it moves through the cell, or away from its center while it rests
        var normal: vec2<f32>;
        var pushing_velocity: vec2<f32>;
        var pushing_mass: f32;
        if covering < uniforms.actor_count {
            let actor: CellularTileActor = actors[covering];
            normal = actor_distance(actor, center).yz;
            pushing_velocity = actor.velocity;
            pushing_mass = actor.mass;
        } else {
            pushing_velocity = rigid_body_velocity(rigid_body - 1u, cell);
            normal = select(
                rigid_body_arm(rigid_body - 1u, cell),
                pushing_velocity,
                length(pushing_velocity) >= RIGID_BODY_RESTING_SPEED,
            );
            pushing_mass = rigid_bodies[rigid_body - 1u].mass;
        }
        let turns: i32 = i32(round(atan2(normal.y, normal.x) / 0.7853982));
        let outward: u32 = u32((turns + 8) % 8) + 1u;
        var escape: u32 = 0u;
        for (var attempt: i32 = 0; attempt < 3; attempt++) {
            // try straight out first, then an eighth turn to either side
            let turns: i32 = select(select(-1, 1, attempt == 1), 0, attempt == 0);
            let direction: u32 = turn_direction(outward, turns);
            let target_cell: vec2<i32> = cell + direction_offset(direction);
            if is_cell_active(target_cell)
                && material_identifiers[cell_index(target_cell)] == 0u
                && covering_actor(vec2<f32>(target_cell) + 0.5) == uniforms.actor_count
            {
                escape = direction;
                break;
            }
        }
        if escape == 0u {
            // a cell with nowhere to go holds the actor or rigid body back instead
            cell_kinematics.velocity = vec2<f32>(0.0);
            cell_kinematics.progress = vec2<f32>(0.0);
        } else {
            // collide inelastically with the pusher along the escape, so heavier pushers shove
            // cells harder and lose less speed doing it
            let escape_offset: vec2<f32> = vec2<f32>(direction_offset(escape));
            let along: vec2<f32> = normalize(escape_offset);
            let cell_speed: f32 = dot(cell_kinematics.velocity, along);
            let shared_speed: f32 =
                (pushing_mass * dot(pushing_velocity, along) + mass * cell_speed)
                    / (pushing_mass + mass);
            if shared_speed > cell_speed {
                let velocity_change: vec2<f32> = (shared_speed - cell_speed) * along;
                cell_kinematics.velocity += velocity_change;
                if rigid_body != 0u {
                    push_rigid_body(rigid_body - 1u, cell, -mass * velocity_change);
                } else {
                    let impulse: vec2<i32> =
                        vec2<i32>(round(-mass * velocity_change * CONTACT_SCALE));
                    atomicAdd(&contacts[covering * CONTACT_STRIDE], impulse.x);
                    atomicAdd(&contacts[covering * CONTACT_STRIDE + 1u], impulse.y);
                }
            }
            // step out this tick however slowly it moves
            cell_kinematics.progress = escape_offset;
        }
    } else if is_cell_supporting(cell + vec2<i32>(0, -1)) {
        cell_kinematics.flags = CELL_SUPPORTED;
        // estimate the surface normal as pointing away from the solid cells around this one
        var surface: vec2<f32> = vec2<f32>(0.0);
        for (var y: i32 = -2; y <= 2; y++) {
            for (var x: i32 = -2; x <= 2; x++) {
                let neighbor: vec2<i32> = cell + vec2<i32>(x, y);
                if !is_cell_inside(neighbor) || material_identifiers[cell_index(neighbor)] != 0u {
                    surface -= vec2<f32>(f32(x), f32(y));
                }
            }
        }
        var normal: vec2<f32> = vec2<f32>(0.0, 1.0);
        if length(surface) > 0.5 { normal = normalize(surface); }
        let cosine: f32 = max(normal.y, 0.0);
        let sine: f32 = abs(normal.x);
        // gravity pulls sideways down the slope when there is room to slide into, and friction
        // removes up to its share of speed, so cells only start sliding where the slope is
        // steeper than their friction holds
        cell_kinematics.velocity.y = max(cell_kinematics.velocity.y - gravity, 0.0);
        // a rigid body holding the cell up carries its weight, which its voxel support cannot
        push_wall(cell + vec2<i32>(0, -1), vec2<f32>(0.0, -mass * gravity));
        cell_kinematics.progress.y = max(cell_kinematics.progress.y, 0.0);
        let downhill: i32 = i32(sign(normal.x));
        let side: vec2<i32> = cell + vec2<i32>(downhill, 0);
        let side_below: vec2<i32> = cell + vec2<i32>(downhill, -1);
        if downhill != 0
            && ((is_cell_inside(side) && material_identifiers[cell_index(side)] == 0u)
                || (is_cell_inside(side_below)
                    && material_identifiers[cell_index(side_below)] == 0u))
        {
            cell_kinematics.velocity.x += f32(downhill) * gravity * sine;
        }
        let friction_speed: f32 = min(abs(cell_kinematics.velocity.x), friction * gravity * cosine);
        cell_kinematics.velocity.x -= sign(cell_kinematics.velocity.x) * friction_speed;
        if abs(cell_kinematics.velocity.x) < RESTING_SPEED
            && cell_kinematics.velocity.y < RESTING_SPEED
            && length(pressure_velocity_change) < RESTING_SPEED
        {
            cell_kinematics.velocity = vec2<f32>(0.0);
            cell_kinematics.progress = vec2<f32>(0.0);
            cell_kinematics.flags = CELL_RESTING | CELL_SUPPORTED;
        } else if length(pressure_velocity_change) >= RESTING_SPEED {
            cell_kinematics.flags = 0u;
        }
    } else {
        cell_kinematics.velocity.y -= gravity;
    }
    if !is_covered {
        let fluid: vec4<f32> = sample_neighboring_fluid(cell);
        if fluid.x > 0.0 {
            let buoyancy: f32 = gravity * fluid.x * fluid.y / mass;
            cell_kinematics.velocity.y += buoyancy;
            // each wet face exchanges a contact impulse with the fluid cell across it
            let offsets: array<vec2<i32>, 4> = array<vec2<i32>, 4>(
                vec2<i32>(-1, 0),
                vec2<i32>(1, 0),
                vec2<i32>(0, -1),
                vec2<i32>(0, 1),
            );
            let restitution: f32 = material.z;
            for (var neighbor_index: u32 = 0u; neighbor_index < 4u; neighbor_index++) {
                let neighbor: vec2<i32> = cell + offsets[neighbor_index];
                if !is_cell_inside(neighbor) { continue; }
                let fluid_index: u32 = cell_index(neighbor);
                if material_identifiers[fluid_index] != 0u
                    || rigid_body_cells[fluid_index] != 0u
                { continue; }
                let neighbor_fluid: vec3<f32> = fluid_raster_cell(fluid_index);
                if neighbor_fluid.x <= 0.0 { continue; }
                let fluid_inverse_mass: f32 =
                    fluid_contact_face_count(neighbor) / neighbor_fluid.x;
                let contact_material: vec2<f32> = fluid_raster_contact_material(fluid_index);
                let face_impulse: vec2<f32> = fluid_face_impulse(
                    neighbor_fluid.yz,
                    cell_kinematics.velocity,
                    -vec2<f32>(offsets[neighbor_index]),
                    fluid_inverse_mass,
                    1.0 / mass,
                    min(friction, contact_material.x),
                    min(restitution, contact_material.y),
                );
                cell_kinematics.velocity += face_impulse / mass;
                // the impulse was solved against a face share, but the raster change reaches every
                // particle in the cell, so it divides by the cell's full mass
                add_fluid_contact_velocity(fluid_index, -face_impulse / neighbor_fluid.x);
            }
            if length(cell_kinematics.velocity) >= RESTING_SPEED
                || length(fluid.zw) >= FLUID_CURRENT_WAKE_SPEED
            {
                cell_kinematics.flags = cell_kinematics.flags & ~CELL_RESTING;
            }
        }
    }
    cell_kinematics.velocity =
        clamp(cell_kinematics.velocity, vec2<f32>(-SPEED_MAXIMUM), vec2<f32>(SPEED_MAXIMUM));
    if !is_covered {
        cell_kinematics.progress = clamp(
            cell_kinematics.progress + cell_kinematics.velocity,
            vec2<f32>(-PROGRESS_MAXIMUM),
            vec2<f32>(PROGRESS_MAXIMUM),
        );
    }
    // the fastest cell decides how many substeps this tick runs
    let step_count: u32 = u32(max(
        floor(abs(cell_kinematics.progress.x)),
        floor(abs(cell_kinematics.progress.y)),
    ));
    if step_count > 0u { atomicMax(&control.step_count, min(step_count, 8u)); }
    if (cell_kinematics.flags & CELL_RESTING) == 0u {
        atomicOr(&awake[tile_index(vec2<u32>(cell) / 8u)], TILE_AWAKE);
    }
    kinematics[index] = pack_kinematics(cell_kinematics);
}

/// Requests a one-cell move toward every whole cell of progress a dynamic cell has made
@compute @workgroup_size(64)
fn request(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_cell: u32,
) {
    let cell: vec2<i32> = workgroup_cell(workgroup.x, local_cell);
    let index: u32 = cell_index(cell);
    var direction: u32 = 0u;
    if is_dynamic(material_identifiers[index]) {
        let progress: vec2<f32> = unpack_kinematics(kinematics[index]).progress;
        direction = offset_direction(vec2<i32>(
            select(0, 1, progress.x >= 1.0) - select(0, 1, progress.x <= -1.0),
            select(0, 1, progress.y >= 1.0) - select(0, 1, progress.y <= -1.0),
        ));
    }
    motions[index].x = (motions[index].x & MOTION_COVERED) | direction;
}

/// Picks which requesting neighbor enters each empty or emptying cell: the one moving fastest
/// toward it, with ties broken by a hash that changes every cell, tick, and substep
@compute @workgroup_size(64)
fn pick(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_cell: u32,
) {
    let cell: vec2<i32> = workgroup_cell(workgroup.x, local_cell);
    let index: u32 = cell_index(cell);
    let motion: u32 = motions[index].x;
    let material_identifier: u32 = material_identifiers[index];
    var picked: u32 = 0u;
    let is_enterable: bool = (motion & MOTION_COVERED) == 0u
        && (material_identifier == 0u
            || (is_dynamic(material_identifier) && (motion & MOTION_DIRECTION) != 0u));
    if is_enterable {
        var picked_speed: f32 = -1.0e30;
        var picked_order: u32 = 0u;
        for (var direction: u32 = 1u; direction <= 8u; direction++) {
            let claimant: vec2<i32> = cell + direction_offset(direction);
            if !is_cell_active(claimant) { continue; }
            let claimant_index: u32 = cell_index(claimant);
            if (motions[claimant_index].x & MOTION_DIRECTION) != opposite_direction(direction) {
                continue;
            }
            let speed: f32 = dot(
                unpack_kinematics(kinematics[claimant_index]).velocity,
                normalize(-vec2<f32>(direction_offset(direction))),
            );
            let order: u32 = hash(vec4<u32>(
                vec2<u32>(cell),
                uniforms.tick,
                uniforms.substep * 8u + direction,
            ));
            if speed > picked_speed || (speed == picked_speed && order > picked_order) {
                picked = direction;
                picked_speed = speed;
                picked_order = order;
            }
        }
    }
    motions[index].y = picked;
}

/// Starts resolving each requested move: it fails unless picked, succeeds into an empty cell, and
/// otherwise waits on the cell whose occupant must leave first
@compute @workgroup_size(64)
fn resolve_begin(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_cell: u32,
) {
    let cell: vec2<i32> = workgroup_cell(workgroup.x, local_cell);
    let index: u32 = cell_index(cell);
    let direction: u32 = motions[index].x & MOTION_DIRECTION;
    var status: u32 = STATUS_NONE;
    if direction != 0u {
        status = STATUS_FAILED;
        let target_cell: vec2<i32> = cell + direction_offset(direction);
        if is_cell_active(target_cell) {
            let target_index: u32 = cell_index(target_cell);
            if motions[target_index].y == opposite_direction(direction) {
                status = select(
                    STATUS_PENDING | target_index,
                    STATUS_SUCCEEDED,
                    material_identifiers[target_index] == 0u,
                );
            }
        }
    }
    motions[index].z = status;
}

/// Returns a pending status advanced by the status it waits on, jumping twice as far each pass
fn resolve_status(status: u32, waited_status: u32) -> u32 {
    if (status & STATUS_MASK) != STATUS_PENDING { return status; }
    switch waited_status & STATUS_MASK {
        case STATUS_SUCCEEDED: { return STATUS_SUCCEEDED; }
        case STATUS_PENDING: { return waited_status; }
        default: { return STATUS_FAILED; }
    }
}

/// Advances pending moves from the first status to the second
@compute @workgroup_size(64)
fn resolve_even(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_cell: u32,
) {
    let index: u32 = cell_index(workgroup_cell(workgroup.x, local_cell));
    let status: u32 = motions[index].z;
    var waited_status: u32 = STATUS_FAILED;
    if (status & STATUS_MASK) == STATUS_PENDING { waited_status = motions[status].z; }
    let resolved_status: u32 = resolve_status(status, waited_status);
    motions[index].w = resolved_status;
    // only the first pending cell of each pass needs to set its bit
    let pending_bit: u32 = 1u << uniforms.resolve;
    if (resolved_status & STATUS_MASK) == STATUS_PENDING
        && (atomicLoad(&control.pending_resolves) & pending_bit) == 0u
    { atomicOr(&control.pending_resolves, pending_bit); }
}

/// Advances pending moves from the second status back to the first
@compute @workgroup_size(64)
fn resolve_odd(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_cell: u32,
) {
    let index: u32 = cell_index(workgroup_cell(workgroup.x, local_cell));
    let status: u32 = motions[index].w;
    var waited_status: u32 = STATUS_FAILED;
    if (status & STATUS_MASK) == STATUS_PENDING { waited_status = motions[status].w; }
    let resolved_status: u32 = resolve_status(status, waited_status);
    motions[index].z = resolved_status;
    // only the first pending cell of each pass needs to set its bit
    let pending_bit: u32 = 1u << uniforms.resolve;
    if (resolved_status & STATUS_MASK) == STATUS_PENDING
        && (atomicLoad(&control.pending_resolves) & pending_bit) == 0u
    { atomicOr(&control.pending_resolves, pending_bit); }
}

/// Returns whether a cell holds a cell pressed against it in place: the region's edge, an actor,
/// or matter that is not moving away this substep
fn is_cell_backing(cell: vec2<i32>) -> bool {
    if !is_cell_inside(cell) { return true; }
    let index: u32 = cell_index(cell);
    if !is_cell_active(cell) { return material_identifiers[index] != 0u; }
    if (motions[index].x & MOTION_COVERED) != 0u { return true; }
    let material_identifier: u32 = material_identifiers[index];
    if material_identifier == 0u { return false; }
    return !is_dynamic(material_identifier)
        || (motions[index].z & STATUS_MASK) != STATUS_SUCCEEDED;
}

/// Exchanges momentum between a dynamic cell that stays this substep and each staying neighbor it
/// moves into or that moves into it, writing the velocity it is left with beside its motion; both
/// cells of a pair decide from the same velocities and cells, so free pairs keep their momentum
fn collide(cell: vec2<i32>, index: u32, motion: vec4<u32>) {
    let material_identifier: u32 = material_identifiers[index];
    if !is_dynamic(material_identifier) || (motion.z & STATUS_MASK) == STATUS_SUCCEEDED { return; }
    let packed: vec2<u32> = kinematics[index];
    let direction: u32 = motion.x & MOTION_DIRECTION;
    let tile: vec2<i32> = cell >> vec2<u32>(3u);
    var material: vec4<f32>;
    var velocity: vec2<f32>;
    var velocity_change: vec2<f32> = vec2<f32>(0.0);
    var is_collided: bool = false;
    for (var neighbor_direction: u32 = 1u; neighbor_direction <= 8u; neighbor_direction++) {
        // check the cheapest conditions first, and only read the awake flags of other tiles
        let neighbor: vec2<i32> = cell + direction_offset(neighbor_direction);
        if any(neighbor >> vec2<u32>(3u) != tile) && !is_cell_active(neighbor) { continue; }
        let neighbor_index: u32 = cell_index(neighbor);
        // read only the components no other cell writes while staging
        if direction != neighbor_direction
            && (motions[neighbor_index].x & MOTION_DIRECTION)
                != opposite_direction(neighbor_direction)
        { continue; }
        if (motions[neighbor_index].z & STATUS_MASK) == STATUS_SUCCEEDED { continue; }
        let neighbor_material_identifier: u32 = material_identifiers[neighbor_index];
        if !is_dynamic(neighbor_material_identifier) { continue; }
        if !is_collided {
            is_collided = true;
            material = materials[material_identifier & 0x3fffffffu];
            velocity = unpack_kinematics(packed).velocity;
        }
        // only cells closing along the line between them collide
        let offset: vec2<i32> = direction_offset(neighbor_direction);
        let normal: vec2<f32> = normalize(vec2<f32>(offset));
        let neighbor_velocity: vec2<f32> = unpack_kinematics(kinematics[neighbor_index]).velocity;
        if dot(velocity - neighbor_velocity, normal) <= 0.0 { continue; }
        let neighbor_material: vec4<f32> = materials[neighbor_material_identifier & 0x3fffffffu];
        // cells closing slower than resting speed stop against each other without rebounding,
        // and faster ones rebound by only part of their restitution, which loses energy
        let restitution: f32 = (material.z + neighbor_material.z) * 0.5 * COLLISION_DAMPING;
        // cells pressed against something behind either of them cannot give way, so each bounces
        // off the other like a fixed cell, sending the impact into whatever backs them
        if is_cell_backing(neighbor + offset) || is_cell_backing(cell - offset) {
            let approach_speed: f32 = dot(velocity, normal);
            if approach_speed <= 0.0 { continue; }
            let rebound_speed: f32 = restitution * max(approach_speed - RESTING_SPEED, 0.0);
            velocity_change -= (approach_speed + rebound_speed) * normal;
            continue;
        }
        // otherwise both cells take equal and opposite impulses, keeping their momentum
        let approach_speed: f32 = dot(velocity - neighbor_velocity, normal);
        let rebound_speed: f32 = restitution * max(approach_speed - RESTING_SPEED, 0.0);
        let impulse: f32 = (approach_speed + rebound_speed) * material.x * neighbor_material.x
            / (material.x + neighbor_material.x);
        velocity_change -= impulse / material.x * normal;
    }
    if all(velocity_change == vec2<f32>(0.0)) {
        motions[index].w = packed.x;
    } else {
        motions[index].w = pack_kinematics(
            CellKinematics(velocity + velocity_change, vec2<f32>(0.0), 0u),
        ).x;
    }
}

/// Copies each cell entering a cell this substep, with its temperature and amount, beside it, so
/// no cell is overwritten while read, and collides the dynamic cells that stay
@compute @workgroup_size(64)
fn stage(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_cell: u32,
) {
    let cell: vec2<i32> = workgroup_cell(workgroup.x, local_cell);
    let index: u32 = cell_index(cell);
    let motion: vec4<u32> = motions[index];
    collide(cell, index, motion);
    let picked: u32 = motion.y;
    var payload: vec4<u32> = vec4<u32>(0u);
    var matter_payload: vec2<u32> = vec2<u32>(0u);
    if picked != 0u {
        let claimant_index: u32 = cell_index(cell + direction_offset(picked));
        if (motions[claimant_index].z & STATUS_MASK) == STATUS_SUCCEEDED {
            // the moved cell spends one cell of progress and keeps its velocity and color
            var moved: CellKinematics = unpack_kinematics(kinematics[claimant_index]);
            moved.progress += vec2<f32>(direction_offset(picked));
            let packed: vec2<u32> = pack_kinematics(moved);
            payload = vec4<u32>(
                material_identifiers[claimant_index],
                colors[claimant_index],
                packed.x,
                packed.y,
            );
            matter_payload = matter[claimant_index];
        }
    }
    staged[index * 2u] = payload;
    staged[index * 2u + 1u] = vec4<u32>(matter_payload, 0u, 0u);
}

/// Moves staged cells into place, empties the cells they left, gives the dynamic cells that stay
/// the velocity their collisions with each other left them, and bounces the cells that could not
/// move off whatever else blocked them
@compute @workgroup_size(64)
fn commit(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_cell: u32,
) {
    let cell: vec2<i32> = workgroup_cell(workgroup.x, local_cell);
    let index: u32 = cell_index(cell);
    let tile: u32 = tile_index(vec2<u32>(cell) / 8u);
    let payload: vec4<u32> = staged[index * 2u];
    let motion: vec4<u32> = motions[index];
    if payload.x != 0u {
        material_identifiers[index] = payload.x;
        colors[index] = payload.y;
        kinematics[index] = payload.zw;
        matter[index] = staged[index * 2u + 1u].xy;
        atomicOr(&awake[tile], TILE_AWAKE | TILE_DISTURBED);
        return;
    }
    let direction: u32 = motion.x & MOTION_DIRECTION;
    if direction != 0u && (motion.z & STATUS_MASK) == STATUS_SUCCEEDED {
        // a cell escaping a rigid body leaves the body's cell behind it
        let rigid_body_cell: u32 = rigid_body_cells[index];
        if rigid_body_cell != 0u {
            let rigid_cell: vec2<u32> = rigid_body_cellular_tile_cells[rigid_body_cell - 1u];
            material_identifiers[index] = rigid_cell.x;
            colors[index] = rigid_cell.y;
            kinematics[index] = pack_kinematics(CellKinematics(
                rigid_body_velocity(cell_rigid_body(index) - 1u, cell),
                vec2<f32>(0.0),
                0u,
            ));
            matter[index] = rigid_body_cell_states[rigid_body_cell - 1u].zw;
        } else {
            // the emptied cell keeps its temperature for the gas that fills it
            material_identifiers[index] = 0u;
            colors[index] = 0u;
            kinematics[index] = vec2<u32>(0u);
            matter[index].y = 0u;
        }
        atomicOr(&awake[tile], TILE_AWAKE | TILE_DISTURBED);
        return;
    }
    let material_identifier: u32 = material_identifiers[index];
    if !is_dynamic(material_identifier) { return; }
    var cell_kinematics: CellKinematics = unpack_kinematics(kinematics[index]);
    let collided_velocity: vec2<f32> = unpack_kinematics(vec2<u32>(motion.w, 0u)).velocity;
    let velocity_change: vec2<f32> = collided_velocity - cell_kinematics.velocity;
    cell_kinematics.velocity = collided_velocity;
    if direction == 0u {
        if all(velocity_change == vec2<f32>(0.0)) { return; }
        // a resting cell hit hard enough starts moving again
        if length(velocity_change) >= RESTING_SPEED {
            cell_kinematics.flags &= ~CELL_RESTING;
            atomicOr(&awake[tile], TILE_AWAKE);
        }
        kinematics[index] = pack_kinematics(cell_kinematics);
        return;
    }
    // find the axes that are blocked, treating a blocked diagonal alone as a floor or ceiling, and
    // which of them are blocked by the target cell alone
    let offset: vec2<i32> = direction_offset(direction);
    var is_blocked: vec2<bool> = vec2<bool>(
        offset.x != 0 && is_cell_blocking(cell + vec2<i32>(offset.x, 0)),
        offset.y != 0 && is_cell_blocking(cell + vec2<i32>(0, offset.y)),
    );
    var is_target_blocking: vec2<bool> = vec2<bool>(offset.y == 0, offset.x == 0);
    if !any(is_blocked) && is_cell_blocking(cell + offset) {
        is_blocked = vec2<bool>(offset.y == 0, offset.y != 0);
        is_target_blocking = is_blocked;
    }
    // a cell that only lost its target or waits behind another keeps its motion and retries
    if !any(is_blocked) {
        kinematics[index] = pack_kinematics(cell_kinematics);
        return;
    }
    // a dynamic target that stays already collided with this cell, so only other blockers bounce
    // it; a target that stays and receives no cell keeps its material while others commit
    let target_cell: vec2<i32> = cell + offset;
    var is_target_collided: bool = false;
    if is_cell_active(target_cell) {
        let target_index: u32 = cell_index(target_cell);
        is_target_collided = staged[target_index * 2u].x == 0u
            && (motions[target_index].z & STATUS_MASK) != STATUS_SUCCEEDED
            && is_dynamic(material_identifiers[target_index]);
    }
    let material: vec4<f32> = materials[material_identifier & 0x3fffffffu];
    // rigid body cells are walls moving at their body's velocity, so cells bounce off them
    // relative to that velocity and push the body by the momentum they lose
    let blockers: array<vec2<i32>, 2> = array<vec2<i32>, 2>(
        select(cell + vec2<i32>(offset.x, 0), target_cell, is_target_blocking.x),
        select(cell + vec2<i32>(0, offset.y), target_cell, is_target_blocking.y),
    );
    let initial_velocity: vec2<f32> = cell_kinematics.velocity;
    var stopped_speed: f32 = 0.0;
    var wall: vec2<f32> = vec2<f32>(0.0);
    if is_blocked.x {
        if is_target_collided && is_target_blocking.x {
            stopped_speed += abs(velocity_change.x);
        } else {
            wall = wall_velocity(blockers[0]);
            let approach: f32 = cell_kinematics.velocity.x - wall.x;
            // static cells stay put, so only moving walls can outrun the cell
            if approach * f32(offset.x) > 0.0 || all(wall == vec2<f32>(0.0)) {
                stopped_speed += abs(approach);
                cell_kinematics.velocity.x = wall.x - approach * material.z;
            }
        }
        cell_kinematics.progress.x = 0.0;
    }
    if is_blocked.y {
        if is_target_collided && is_target_blocking.y {
            stopped_speed += abs(velocity_change.y);
        } else {
            wall = wall_velocity(blockers[1]);
            let approach: f32 = cell_kinematics.velocity.y - wall.y;
            // static cells stay put, so only moving walls can outrun the cell
            if approach * f32(offset.y) > 0.0 || all(wall == vec2<f32>(0.0)) {
                stopped_speed += abs(approach);
                cell_kinematics.velocity.y = wall.y - approach * material.z;
            }
        }
        cell_kinematics.progress.y = 0.0;
    }
    // friction removes sliding speed relative to the wall in proportion to the speed stopped by
    // the impact
    let friction_speed: f32 = material.y * stopped_speed;
    if is_blocked.x && !is_blocked.y {
        let sliding: f32 = cell_kinematics.velocity.y - wall.y;
        cell_kinematics.velocity.y -= sign(sliding) * min(abs(sliding), friction_speed);
    }
    if is_blocked.y && !is_blocked.x {
        let sliding: f32 = cell_kinematics.velocity.x - wall.x;
        cell_kinematics.velocity.x -= sign(sliding) * min(abs(sliding), friction_speed);
    }
    let impulse: vec2<f32> = material.x * (initial_velocity - cell_kinematics.velocity);
    if is_blocked.x && !(is_target_collided && is_target_blocking.x)
        && (cell_kinematics.flags & CELL_RESTING) == 0u
    {
        push_wall(blockers[0], vec2<f32>(impulse.x, select(0.0, impulse.y, !is_blocked.y)));
    }
    if is_blocked.y && !(is_target_collided && is_target_blocking.y)
        && (cell_kinematics.flags & CELL_RESTING) == 0u
    {
        push_wall(blockers[1], vec2<f32>(select(0.0, impulse.x, !is_blocked.x), impulse.y));
    }
    kinematics[index] = pack_kinematics(cell_kinematics);
}

/// Accumulates, for every actor, the dynamic cells still inside it after moving: cells that
/// could not get out of its way and hold it back or up
@compute @workgroup_size(64)
fn contact(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_cell: u32,
) {
    let cell: vec2<i32> = workgroup_cell(workgroup.x, local_cell);
    let dynamic: bool = is_dynamic(material_identifiers[cell_index(cell)]);
    let center: vec2<f32> = vec2<f32>(cell) + 0.5;
    for (var actor_index: u32 = 0u; actor_index < uniforms.actor_count; actor_index++) {
        let actor: CellularTileActor = actors[actor_index];
        if any(center < actor.bounds.xy) || any(center > actor.bounds.zw) { continue; }
        let distance: vec3<f32> = actor_distance(actor, center);
        if distance.x >= 0.0 { continue; }
        let contact_index: u32 = actor_index * CONTACT_STRIDE;
        if dynamic {
            let normal: vec2<i32> = vec2<i32>(round(-distance.yz * CONTACT_SCALE));
            atomicAdd(&contacts[contact_index + 2u], normal.x);
            atomicAdd(&contacts[contact_index + 3u], normal.y);
            atomicAdd(&contacts[contact_index + 4u], 1);
            atomicMax(&contacts[contact_index + 5u], i32(-distance.x * CONTACT_SCALE));
        }
        let offsets: array<vec2<i32>, 4> = array<vec2<i32>, 4>(
            vec2<i32>(-1, 0),
            vec2<i32>(1, 0),
            vec2<i32>(0, -1),
            vec2<i32>(0, 1),
        );
        for (var face: u32 = 0u; face < 4u; face++) {
            let neighbor: vec2<i32> = cell + offsets[face];
            if !is_cell_inside(neighbor) { continue; }
            // only faces between the actor and cells outside it push it
            let neighbor_center: vec2<f32> = vec2<f32>(neighbor) + 0.5;
            if all(neighbor_center >= actor.bounds.xy)
                && all(neighbor_center <= actor.bounds.zw)
                && actor_distance(actor, neighbor_center).x < 0.0
            { continue; }
            let gauge: f32 = pressure[cell_index(neighbor)];
            if abs(gauge) < PRESSURE_PUSH_MINIMUM { continue; }
            let impulse: vec2<i32> = vec2<i32>(round(
                -vec2<f32>(offsets[face]) * gauge * PRESSURE_IMPULSE * CONTACT_SCALE,
            ));
            atomicAdd(&contacts[contact_index], impulse.x);
            atomicAdd(&contacts[contact_index + 1u], impulse.y);
        }
    }
}
