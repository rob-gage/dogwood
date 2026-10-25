// Copyright Rob Gage 2026

struct FluidUniforms {
    region_width: u32,
    region_height: u32,
    ring_offset_x: u32,
    ring_offset_y: u32,
    cell_width: u32,
    cell_height: u32,
    bin_width: u32,
    bin_height: u32,
    bin_count: u32,
    capacity: u32,
    state_side: u32,
    predicted_side: u32,
    edit_count: u32,
    erasure_count: u32,
    velocity_maximum: f32,
    scan_block_count: u32,
    relaxation: f32,
    rho0: f32,
    gravity_substep: f32,
    rebase_x: f32,
    rebase_y: f32,
    substep_count: f32,
    cohesion_scale: f32,
    cohesion_normalization_2d: f32,
    /// Fixed-point units per mass or momentum unit in the cell raster
    raster_scale: f32,
    fluid_material_count: u32,
    /// Active region origin relative to collision space, in cells
    region_origin: vec2<i32>,
    /// Index of the substep this dispatch belongs to, zero outside the substeps
    substep_index: u32,
    import_count: u32,
    world_origin_low: vec2<u32>,
    world_origin_high: vec2<u32>,
    actor_count: u32,
    actor_contact_scale: f32,
    actor_time: f32,
    /// Alignment padding after the actor fields
    padding: u32,
};

struct FluidParticle {
    state: vec4<f32>,
    material_identifier: u32,
    /// Position at the start of the current tick, in cells
    tick_start_x: f32,
    /// Position at the start of the current tick, in cells
    tick_start_y: f32,
    padding_3: u32,
};

/// Exact-position particle command imported from a chunk
struct FluidParticleImport {
    /// Position in active-region cells and velocity in cells per tick
    state: vec4<f32>,
    /// Fluid material identifier
    material_identifier: u32,
    /// Alignment padding
    padding_0: u32,
    /// Alignment padding
    padding_1: u32,
    /// Alignment padding
    padding_2: u32,
};

@group(0) @binding(0)
var<uniform> uniforms: FluidUniforms;

@group(0) @binding(1)
var<storage, read> material_identifiers: array<u32>;

@group(0) @binding(2)
var<storage, read> fluid_materials: array<vec4<f32>>;

@group(0) @binding(3)
var<storage, read_write> particles_a: array<FluidParticle>;

@group(0) @binding(4)
var<storage, read_write> particles_b: array<FluidParticle>;

@group(0) @binding(5)
var<storage, read_write> predicted_a: array<vec2<f32>>;

@group(0) @binding(6)
var<storage, read_write> predicted_b: array<vec2<f32>>;

@group(0) @binding(7)
var<storage, read_write> lambdas: array<f32>;

@group(0) @binding(8)
var<storage, read_write> boundary_psi: array<vec4<f32>>;

@group(0) @binding(9)
var<storage, read_write> bin_data: array<atomic<u32>>;

@group(0) @binding(10)
var<storage, read_write> control: array<atomic<u32>>;

@group(0) @binding(11)
var<storage, read> edit_commands: array<vec4<u32>>;

@group(0) @binding(12)
var<storage, read> erasure_masks: array<vec2<u32>>;

@group(0) @binding(13)
var<storage, read_write> boundary_tile_data: array<atomic<u32>>;

@group(0) @binding(14)
var<storage, read_write> boundary_node_map: array<vec4<f32>>;

@group(0) @binding(15)
var<storage, read_write> neighbor_data: array<u32>;

@group(0) @binding(16)
var<storage, read_write> fluid_raster: array<atomic<u32>>;

@group(0) @binding(21)
var<storage, read> pressure: array<f32>;

/// Analytic cellular tile actor record shared with dynamic-cell collision
struct FluidActor {
    /// Collider center relative to the fluid region, in cells
    center: vec2<f32>,
    /// Linear velocity in cells per tick
    velocity: vec2<f32>,
    /// Cosine and sine of collider rotation
    rotation: vec2<f32>,
    /// Circle radius, rectangle half-size, or capsule half-segment and radius
    size: vec2<f32>,
    /// Conservative center bounds used to skip distant actors
    bounds: vec4<f32>,
    /// Circle, rectangle, or capsule shape tag
    shape: u32,
    /// Actor mass in cellular mass units
    mass: f32,
    /// Alignment padding shared with the cellular tile record
    swimming: u32,
    padding: u32,
};

@group(0) @binding(23)
var<storage, read> fluid_actors: array<FluidActor>;

@group(0) @binding(24)
var<storage, read_write> fluid_actor_contacts: array<atomic<i32>>;

@group(0) @binding(25)
var<storage, read> fluid_particle_imports: array<FluidParticleImport>;

@group(0) @binding(26)
var<storage, read_write> fluid_particle_import_results: array<atomic<u32>>;

@group(0) @binding(27)
var<storage, read_write> fluid_particle_exports: array<atomic<u32>>;

/// Rigid-body tile record shared with cellular kinematics
struct FluidRigidBodyCellularTile {
    /// GPU record index of the owning body
    rigid_body: u32,
    /// Alignment word
    padding_0: u32,
    /// Tile coordinates in body-local tile space
    tile: vec2<i32>,
    /// Current body-local occupied-cell mask
    cells: vec2<u32>,
    /// Prior body-local occupied-cell mask
    previous_cells: vec2<u32>,
};

/// Linear and angular motion data shared with cellular kinematics
struct FluidRigidBody {
    /// Body position in collision-origin cells
    position: vec2<f32>,
    /// Cosine and sine of body rotation
    rotation: vec2<f32>,
    /// Position at the prior rasterization
    previous_position: vec2<f32>,
    /// Rotation at the prior rasterization
    previous_rotation: vec2<f32>,
    /// Linear velocity in cells per tick
    velocity: vec2<f32>,
    /// Center of mass in collision-origin cells
    center: vec2<f32>,
    /// Angular velocity in radians per tick
    angular_velocity: f32,
    /// Body mass in cellular mass units
    mass: f32,
    /// Alignment padding
    padding: vec2<u32>,
};

/// Result of projecting a particle from a boundary cell
struct FluidProjectionResult {
    /// Projected particle position in active-region cells
    position: vec2<f32>,
    /// Velocity of the contacted boundary in cells per tick
    wall_velocity: vec2<f32>,
};

@group(0) @binding(18)
var<storage, read> rigid_body_cells: array<u32>;

@group(0) @binding(19)
var<storage, read> rigid_body_cellular_tiles: array<FluidRigidBodyCellularTile>;

@group(0) @binding(20)
var<storage, read> rigid_bodies: array<FluidRigidBody>;

@group(0) @binding(22)
var<storage, read> cell_kinematics: array<vec2<u32>>;

/// Control state for the currently sleeping tiles and their per-tick reductions
const SLEEP_TILE_STRIDE: u32 = 7u;
/// Extra radius around a collider used to sample nearby fluid, in cells
const FLUID_ACTOR_SAMPLE_MARGIN: f32 = 2.5;
/// Duration of one fluid tick in seconds
const FLUID_TICK_TIME_STEP: f32 = 1.0 / 60.0;

/// Pressure impulse in mass * cells per tick per atmosphere, matching the other pressure shaders
const PRESSURE_IMPULSE: f32 = 0.1111;

/// Absolute gauge pressure below this value in atmospheres does not produce an impulse
const PRESSURE_PUSH_MINIMUM: f32 = 0.02;

/// Bit marking a tile whose fluid particles are frozen
const SLEEP_TILE_SLEEPING: u32 = 1u;

/// Bit marking a tile with a valid previous solid-occupancy hash
const SLEEP_TILE_HASH_VALID: u32 = 2u;

/// Bit marking a tile that held particles at the previous tick's end
const SLEEP_TILE_HAS_PARTICLE: u32 = 4u;

/// Bit marking a tile that must remain awake for the current tick
const SLEEP_TILE_WAKE_THIS_TICK: u32 = 8u;

/// Bit marking a sleeping tile queued to wake after the current prediction
const SLEEP_TILE_WAKE_PENDING: u32 = 16u;

/// Bit marking a tile whose solid occupancy changed during this tick
const SLEEP_TILE_SOLID_CHANGED: u32 = 32u;

/// Empty marker for a workgroup-local sleep reduction slot
const SLEEP_REDUCTION_EMPTY: u32 = 0xffffffffu;

/// Maximum consecutive calm ticks before a tile sleeps
const SLEEP_TICK_COUNT: u32 = 30u;

/// Net particle displacement over a tick below which its tile may sleep, in cells
const SLEEP_SPEED: f32 = 0.02;

/// Density-constraint threshold for sleeping
const SLEEP_DENSITY_ERROR: f32 = 0.02;

/// Net neighbor particle displacement over a tick that wakes a tile, in cells
const WAKE_SPEED: f32 = 0.1;

/// Fraction of particle velocity kept at the end of each tick, so that leftover motion decays
const VELOCITY_DECAY: f32 = 0.998;

/// Tile flag of a tile holding a particle
const BOUNDARY_TILE_PARTICLE: u32 = 1u;

/// Tile flag of a tile within the boundary margin of a particle tile
const BOUNDARY_TILE_MARGIN: u32 = 2u;

/// Fraction of relative tangential speed removed by one swept boundary contact
const FLUID_BOUNDARY_FRICTION: f32 = 0.05;

@group(1) @binding(0)
var<storage, read_write> dispatch_arguments: array<u32>;

/// Mathematical constant pi used by the two-dimensional kernels
const PI: f32 = 3.141592653589793;

/// Support radius used by the poly6 and spiky kernels
const KERNEL_RADIUS: f32 = 2.5;

/// Squared support radius used to reject out-of-range neighbors before square roots
const KERNEL_RADIUS_SQUARED: f32 = 6.25;

/// Poly6 coefficient for a 2D kernel with support radius 2.5 cells
const POLY6_COEFFICIENT: f32 = 4.0 / (PI * 1525.87890625);

/// Spiky-gradient coefficient for a 2D kernel with support radius 2.5 cells
const SPIKY_COEFFICIENT: f32 = -30.0 / (PI * 97.65625);

/// Poly6 value at the artificial-pressure reference distance
const S_CORR_DELTA_Q_KERNEL: f32 = POLY6_COEFFICIENT * 5.6875 * 5.6875 * 5.6875;

/// Reciprocal poly6 value at the artificial-pressure reference distance
const S_CORR_DELTA_Q_KERNEL_INVERSE: f32 = 1.0 / S_CORR_DELTA_Q_KERNEL;

/// Reciprocal sixth power of the support radius for the cohesion spline
const KERNEL_RADIUS_SIXTH_INVERSE: f32 = 1.0 / 244.140625;

/// Width of the dense neighbor bins
const BIN_SIZE: f32 = __FLUID_BIN_SIZE__;

/// Maximum Jacobi position correction in one iteration, in cells
const MAX_POSITION_CORRECTION: f32 = 0.25;

/// Returns whether both components are finite WGSL floating-point values
fn fluid_vector_is_finite(value: vec2<f32>) -> bool {
    return all(value == value) && all(abs(value) <= vec2<f32>(3.4e38));
}

/// Number of invocations used by particle passes
const PARTICLE_WORKGROUP_SIZE: u32 = 128u;

/// Tile keys shared while reducing sleep values within one workgroup
var<workgroup> sleep_reduction_tiles: array<atomic<u32>, 128>;
/// Maximum speed or constraint shared for each workgroup tile
var<workgroup> sleep_reduction_values: array<atomic<u32>, 128>;

/// Hash multiplier for repeatable sub-cell spawn jitter
const JITTER_HASH: u32 = 747796405u;

/// Returns the physical index of a logical tile inside the toroidal region
fn tile_index(tile: vec2<u32>) -> u32 {
    let physical_x: u32 = (tile.x + uniforms.ring_offset_x) % uniforms.region_width;
    let physical_y: u32 = (tile.y + uniforms.ring_offset_y) % uniforms.region_height;
    return physical_y * uniforms.region_width + physical_x;
}

/// Returns the physical cell-buffer index of a logical cell inside the region
fn cell_index(cell: vec2<i32>) -> u32 {
    let position: vec2<u32> = vec2<u32>(cell);
    return tile_index(position / 8u) * 64u + (position.y % 8u) * 8u + position.x % 8u;
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
        if !cell_inside(neighbor) { continue; }
        let gauge: f32 = pressure[cell_index(neighbor)];
        if abs(gauge) < PRESSURE_PUSH_MINIMUM { continue; }
        push -= vec2<f32>(offsets[face]) * gauge;
    }
    return push;
}

/// Decodes the packed dynamic-cell velocity stored in the kinematics buffer
fn unpack_cell_velocity(packed: vec2<u32>) -> vec2<f32> {
    let velocity_bits: i32 = bitcast<i32>(packed.x);
    return vec2<f32>(
        f32(extractBits(velocity_bits, 0u, 16u)),
        f32(extractBits(velocity_bits, 16u, 16u)),
    ) / 1024.0;
}

/// Returns the rasterized rigid-body owner at one physical cell, plus one for none
fn cell_rigid_body(index: u32) -> u32 {
    let rigid_body_cell: u32 = rigid_body_cells[index];
    if rigid_body_cell == 0u { return 0u; }
    return rigid_body_cellular_tiles[(rigid_body_cell - 1u) / 64u].rigid_body + 1u;
}

/// Returns the body velocity at a logical cell center in cells per tick
fn rigid_body_velocity(rigid_body: u32, cell: vec2<i32>) -> vec2<f32> {
    let world_cell: vec2<f32> = vec2<f32>(uniforms.region_origin + cell) + 0.5;
    let arm: vec2<f32> = world_cell - rigid_bodies[rigid_body].center;
    return rigid_bodies[rigid_body].velocity
        + rigid_bodies[rigid_body].angular_velocity * vec2<f32>(-arm.y, arm.x);
}

/// Returns the first control word after per-cell Insert occupancy stamps
fn sleep_state_offset() -> u32 {
    return 4u + uniforms.cell_width * uniforms.cell_height;
}

/// Returns the first control word in one logical tile's sleep record
fn sleep_tile_offset(tile_index: u32) -> u32 {
    return sleep_state_offset() + tile_index * SLEEP_TILE_STRIDE;
}

/// Returns the control word used to count tiles woken by particle entry
fn wake_count_offset() -> u32 {
    return sleep_state_offset()
        + uniforms.region_width * uniforms.region_height * SLEEP_TILE_STRIDE;
}

/// Returns the logical tile index containing a cell-space particle position
fn particle_tile_index(position: vec2<f32>) -> u32 {
    let tile: vec2<u32> = vec2<u32>(floor(position)) / 8u;
    return tile.y * uniforms.region_width + tile.x;
}

/// Finds or claims one shared sleep-reduction slot for a tile
fn sleep_reduction_slot(tile_index: u32) -> u32 {
    var slot: u32 = (tile_index * 2654435761u) & 127u;
    loop {
        let previous_tile: u32 = atomicLoad(&sleep_reduction_tiles[slot]);
        if previous_tile == tile_index { return slot; }
        if previous_tile == SLEEP_REDUCTION_EMPTY {
            let exchange = atomicCompareExchangeWeak(
                &sleep_reduction_tiles[slot],
                SLEEP_REDUCTION_EMPTY,
                tile_index,
            );
            if exchange.exchanged || exchange.old_value == tile_index { return slot; }
        }
        slot = (slot + 1u) & 127u;
    }
    return 0u;
}

/// Returns whether a tile currently freezes its particles
fn tile_is_sleeping(tile_index: u32) -> bool {
    let flags: u32 = atomicLoad(&control[sleep_tile_offset(tile_index) + 4u]);
    return (flags & SLEEP_TILE_SLEEPING) != 0u;
}

/// Wakes one sleeping tile and records the current-tick wake event once
fn wake_tile(tile_index: u32) {
    let base: u32 = sleep_tile_offset(tile_index);
    var previous_flags: u32 = atomicLoad(&control[base + 4u]);
    loop {
        let next_flags: u32 = (previous_flags & ~SLEEP_TILE_SLEEPING)
            | SLEEP_TILE_WAKE_THIS_TICK;
        let exchange = atomicCompareExchangeWeak(
            &control[base + 4u],
            previous_flags,
            next_flags,
        );
        if exchange.exchanged {
            atomicStore(&control[base + 2u], 0u);
            if (previous_flags & SLEEP_TILE_SLEEPING) != 0u {
                atomicAdd(&control[wake_count_offset()], 1u);
            }
            return;
        }
        previous_flags = exchange.old_value;
    }
}

/// Defers an entry wake until all particles finish reading current sleep states
fn request_tile_wake(tile_index: u32) {
    let base: u32 = sleep_tile_offset(tile_index);
    var previous_flags: u32 = atomicLoad(&control[base + 4u]);
    loop {
        if (previous_flags & SLEEP_TILE_SLEEPING) == 0u
            || (previous_flags & SLEEP_TILE_WAKE_PENDING) != 0u
        { return; }
        let exchange = atomicCompareExchangeWeak(
            &control[base + 4u],
            previous_flags,
            previous_flags | SLEEP_TILE_WAKE_PENDING,
        );
        if exchange.exchanged {
            atomicAdd(&control[wake_count_offset()], 1u);
            return;
        }
        previous_flags = exchange.old_value;
    }
}

/// Returns whether a cell-space position belongs to a sleeping tile
fn position_is_sleeping(position: vec2<f32>) -> bool {
    if any(position < vec2<f32>(0.0))
        || position.x >= f32(uniforms.cell_width)
        || position.y >= f32(uniforms.cell_height)
    { return false; }
    return tile_is_sleeping(particle_tile_index(position));
}

/// Returns the first word after the physical cell raster
fn raster_tile_data_offset() -> u32 {
    return uniforms.cell_width * uniforms.cell_height * 4u;
}

/// Returns the first word in the sparse raster tile flags
fn raster_tile_flags_offset() -> u32 {
    return raster_tile_data_offset();
}

/// Returns the first word in the sparse raster tile list
fn raster_tile_list_offset() -> u32 {
    return raster_tile_data_offset() + uniforms.region_width * uniforms.region_height;
}

/// Returns the count word after the raster tile flags and compact list
fn raster_tile_count_offset() -> u32 {
    return raster_tile_data_offset() + 2u * uniforms.region_width * uniforms.region_height;
}

/// Returns the first word of the per-cell fluid velocity changes from solid contacts
fn fluid_contact_offset() -> u32 {
    return raster_tile_count_offset() + 1u;
}

/// Returns the first word of the sparse solid-contact tile flags
fn fluid_contact_tile_flags_offset() -> u32 {
    return fluid_contact_offset() + uniforms.cell_width * uniforms.cell_height * 2u;
}

/// Returns the first word of the sparse solid-contact tile list
fn fluid_contact_tile_list_offset() -> u32 {
    return fluid_contact_tile_flags_offset() + uniforms.region_width * uniforms.region_height;
}

/// Returns the word counting tiles with solid-contact velocity changes
fn fluid_contact_tile_count_offset() -> u32 {
    return fluid_contact_tile_list_offset() + uniforms.region_width * uniforms.region_height;
}

/// Adds a physical tile to the solid-contact list once
fn add_fluid_contact_tile(physical_tile: u32) {
    let flag_index: u32 = fluid_contact_tile_flags_offset() + physical_tile;
    if atomicLoad(&fluid_raster[flag_index]) != 0u { return; }
    loop {
        let exchange = atomicCompareExchangeWeak(&fluid_raster[flag_index], 0u, 1u);
        if exchange.exchanged {
            let list_index: u32 = atomicAdd(
                &fluid_raster[fluid_contact_tile_count_offset()],
                1u,
            );
            atomicStore(
                &fluid_raster[fluid_contact_tile_list_offset() + list_index],
                physical_tile,
            );
            break;
        }
        if exchange.old_value != 0u { break; }
    }
}

/// Clears all fixed-point values for one physical ring cell
fn clear_raster_cell(index: u32) {
    let base: u32 = index * 4u;
    atomicStore(&fluid_raster[base], 0u);
    atomicStore(&fluid_raster[base + 1u], 0u);
    atomicStore(&fluid_raster[base + 2u], 0u);
    atomicStore(&fluid_raster[base + 3u], 0u);
}

/// Adds a signed fixed-point value while clamping to the i32 range
fn atomic_saturating_add(
    accumulator_index: u32,
    amount: i32,
) {
    var previous_bits: u32 = atomicLoad(&fluid_raster[accumulator_index]);
    loop {
        let previous: i32 = bitcast<i32>(previous_bits);
        var next: i32 = previous;
        if amount > 0 {
            if previous > 2147483647 - amount {
                next = 2147483647;
            } else {
                next = previous + amount;
            }
        } else if amount < 0 {
            if previous < -2147483647 - amount - 1 {
                next = -2147483647 - 1;
            } else {
                next = previous + amount;
            }
        }
        let next_bits: u32 = bitcast<u32>(next);
        let exchange = atomicCompareExchangeWeak(
            &fluid_raster[accumulator_index],
            previous_bits,
            next_bits,
        );
        if exchange.exchanged { return; }
        previous_bits = exchange.old_value;
    }
}

/// Returns whether a logical cell is inside the active region
fn cell_inside(cell: vec2<i32>) -> bool {
    return cell.x >= 0 && cell.y >= 0
        && cell.x < i32(uniforms.cell_width)
        && cell.y < i32(uniforms.cell_height);
}

/// Returns whether a logical cell is solid in a tile-major grid rebuilt for this tick
fn solid_cell(cell: vec2<i32>) -> bool {
    if !cell_inside(cell) { return false; }
    let tile: vec2<u32> = vec2<u32>(cell) / 8u;
    let logical_tile: u32 = tile.y * uniforms.region_width + tile.x;
    if atomicLoad(&boundary_tile_data[logical_tile]) == 0u { return false; }
    let tile_count: u32 = uniforms.region_width * uniforms.region_height;
    let local_cell: u32 = (u32(cell.y) % 8u) * 8u + u32(cell.x) % 8u;
    let solid_index: u32 = 2u * tile_count + logical_tile * 16u + local_cell / 4u;
    let shift: u32 = (local_cell & 3u) * 8u;
    return (atomicLoad(&boundary_tile_data[solid_index]) & (1u << shift)) != 0u;
}

/// Loads a boundary node only when its owning tile was rebuilt this tick
fn load_boundary_node(node: vec2<u32>) -> vec4<f32> {
    if node.x > uniforms.cell_width || node.y > uniforms.cell_height {
        return vec4<f32>(0.0);
    }
    let tile: vec2<u32> = min(node / 8u, vec2<u32>(
        uniforms.region_width - 1u,
        uniforms.region_height - 1u,
    ));
    let logical_tile: u32 = tile.y * uniforms.region_width + tile.x;
    if atomicLoad(&boundary_tile_data[logical_tile]) == 0u {
        return vec4<f32>(0.0);
    }
    let node_width: u32 = uniforms.cell_width + 1u;
    return boundary_node_map[node.y * node_width + node.x];
}

/// Bilinearly samples boundary density and its gradient at a particle position
fn sample_boundary(position: vec2<f32>) -> vec4<f32> {
    let lower: vec2<u32> = vec2<u32>(floor(position));
    let upper: vec2<u32> = lower + vec2<u32>(1u);
    let fraction: vec2<f32> = position - vec2<f32>(lower);
    let lower_left: vec4<f32> = load_boundary_node(lower);
    let lower_right: vec4<f32> = load_boundary_node(vec2<u32>(upper.x, lower.y));
    let upper_left: vec4<f32> = load_boundary_node(vec2<u32>(lower.x, upper.y));
    let upper_right: vec4<f32> = load_boundary_node(upper);
    return lower_left * ((1.0 - fraction.x) * (1.0 - fraction.y))
        + lower_right * (fraction.x * (1.0 - fraction.y))
        + upper_left * ((1.0 - fraction.x) * fraction.y)
        + upper_right * (fraction.x * fraction.y);
}

/// Returns the dense bin index of a point in cells
fn bin_index(position: vec2<f32>) -> u32 {
    let bin: vec2<u32> = vec2<u32>(floor(position / BIN_SIZE));
    return bin.y * uniforms.bin_width + bin.x;
}

/// Returns the particle stored in the selected packed state buffer
fn load_particle(index: u32, side: u32) -> FluidParticle {
    if side == 0u { return particles_a[index]; }
    return particles_b[index];
}

/// Writes a particle to the selected packed state buffer
fn store_particle(index: u32, side: u32, particle: FluidParticle) {
    if side == 0u {
        particles_a[index] = particle;
    } else {
        particles_b[index] = particle;
    }
}

/// Reads a predicted position from one half of the packed scratch record
fn load_predicted(index: u32, side: u32) -> vec2<f32> {
    if side == 0u { return predicted_a[index]; }
    return predicted_b[index];
}

/// Writes a predicted position to one half of the packed scratch record
fn store_predicted(index: u32, side: u32, position: vec2<f32>) {
    if side == 0u {
        predicted_a[index] = position;
    } else {
        predicted_b[index] = position;
    }
}

/// Writes compact old-position, cohesion, pressure, and sleep state beside sorted particles
fn store_neighbor_data(
    index: u32,
    old_position: vec2<f32>,
    cohesion: f32,
    pressure: f32,
    sleeping: bool,
) {
    let base: u32 = index * 5u;
    neighbor_data[base] = bitcast<u32>(old_position.x);
    neighbor_data[base + 1u] = bitcast<u32>(old_position.y);
    neighbor_data[base + 2u] = bitcast<u32>(cohesion);
    neighbor_data[base + 3u] = bitcast<u32>(pressure);
    neighbor_data[base + 4u] = select(0u, 1u, sleeping);
}

/// Reads the compact old position used to reconstruct a neighbor's corrected velocity
fn load_neighbor_old_position(index: u32) -> vec2<f32> {
    let base: u32 = index * 5u;
    return vec2<f32>(
        bitcast<f32>(neighbor_data[base]),
        bitcast<f32>(neighbor_data[base + 1u]),
    );
}

/// Reads one particle's material cohesion strength from its compact sorted record
fn load_neighbor_cohesion(index: u32) -> f32 {
    return bitcast<f32>(neighbor_data[index * 5u + 2u]);
}

/// Reads the cached artificial-pressure strength of one sorted particle
fn load_neighbor_pressure(index: u32) -> f32 {
    return bitcast<f32>(neighbor_data[index * 5u + 3u]);
}

/// Returns whether one sorted particle was sleeping when it was scattered
fn neighbor_is_sleeping(index: u32) -> bool {
    return neighbor_data[index * 5u + 4u] != 0u;
}

/// Distance from a particle's current position within which every kernel neighbor was binned,
/// since a neighbor moves at most two capped corrections between sorts
const NEIGHBOR_SEARCH_RADIUS: f32 = KERNEL_RADIUS + 2.0 * MAX_POSITION_CORRECTION;

/// Returns the first and last bin rows within the neighbor search radius of a position
fn neighbor_rows(position: vec2<f32>) -> vec2<i32> {
    return vec2<i32>(
        i32(floor((position.y - NEIGHBOR_SEARCH_RADIUS) / BIN_SIZE)),
        i32(floor((position.y + NEIGHBOR_SEARCH_RADIUS) / BIN_SIZE)),
    );
}

/// Returns the sorted particle range of the bins in one row that overlap the neighbor search
/// circle around a position, which is contiguous because bins are sorted in row-major order
fn neighbor_row_range(position: vec2<f32>, bin_y: i32, particle_count: u32) -> vec2<u32> {
    if bin_y < 0 || bin_y >= i32(uniforms.bin_height) {
        return vec2<u32>(0u);
    }
    let row_bottom: f32 = f32(bin_y) * BIN_SIZE;
    let row_distance: f32 =
        max(max(row_bottom - position.y, position.y - row_bottom - BIN_SIZE), 0.0);
    let half_width_squared: f32 =
        NEIGHBOR_SEARCH_RADIUS * NEIGHBOR_SEARCH_RADIUS - row_distance * row_distance;
    if half_width_squared < 0.0 { return vec2<u32>(0u); }
    let half_width: f32 = sqrt(half_width_squared);
    let minimum_x: i32 = max(i32(floor((position.x - half_width) / BIN_SIZE)), 0);
    let maximum_x: i32 = min(
        i32(floor((position.x + half_width) / BIN_SIZE)),
        i32(uniforms.bin_width) - 1,
    );
    if minimum_x > maximum_x { return vec2<u32>(0u); }
    let first_bin: u32 = u32(bin_y) * uniforms.bin_width + u32(minimum_x);
    let end_bin: u32 = u32(bin_y) * uniforms.bin_width + u32(maximum_x + 1);
    let first: u32 = atomicLoad(&bin_data[uniforms.bin_count + first_bin]);
    var end: u32 = particle_count;
    if end_bin < uniforms.bin_count {
        end = atomicLoad(&bin_data[uniforms.bin_count + end_bin]);
    }
    return vec2<u32>(first, end);
}

/// Returns the two-dimensional poly6 density kernel from a squared distance
fn poly6_squared(distance_squared: f32) -> f32 {
    if distance_squared >= KERNEL_RADIUS_SQUARED { return 0.0; }
    let difference: f32 = KERNEL_RADIUS_SQUARED - distance_squared;
    return POLY6_COEFFICIENT * difference * difference * difference;
}

/// Returns the two-dimensional spiky kernel gradient for a point difference
fn spiky_gradient(difference: vec2<f32>, distance_squared: f32) -> vec2<f32> {
    if distance_squared <= 0.0000000001 || distance_squared >= KERNEL_RADIUS_SQUARED {
        return vec2<f32>(0.0);
    }
    let distance: f32 = sqrt(distance_squared);
    let remaining: f32 = KERNEL_RADIUS - distance;
    return SPIKY_COEFFICIENT * remaining * remaining * difference / distance;
}

/// Returns the signed, area-normalized two-dimensional Akinci cohesion spline
fn cohesion_spline(distance: f32, remaining: f32, distance_squared: f32) -> f32 {
    if distance <= 0.00001 || distance >= KERNEL_RADIUS { return 0.0; }
    let remaining_cubed: f32 = remaining * remaining * remaining;
    let product: f32 = distance_squared * distance * remaining_cubed
        * KERNEL_RADIUS_SIXTH_INVERSE;
    var shape: f32 = product;
    if distance <= KERNEL_RADIUS * 0.5 { shape = 2.0 * product - 1.0 / 64.0; }
    return uniforms.cohesion_normalization_2d * shape / KERNEL_RADIUS_SQUARED;
}

/// Returns the outward normal of the nearest face of a solid cell that opens onto free space,
/// so a center pushed inside a wall leaves on its own side instead of crossing into the next
/// solid cell, or the nearest face when every side is solid
fn solid_exit_normal(cell: vec2<i32>, relative: vec2<f32>) -> vec2<f32> {
    let normals: array<vec2<f32>, 4> = array<vec2<f32>, 4>(
        vec2<f32>(1.0, 0.0),
        vec2<f32>(-1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, -1.0),
    );
    var best_normal: vec2<f32> = vec2<f32>(0.0, 1.0);
    var best_distance: f32 = 2.0;
    var nearest_normal: vec2<f32> = vec2<f32>(0.0, 1.0);
    var nearest_distance: f32 = 2.0;
    for (var face: u32 = 0u; face < 4u; face++) {
        let normal: vec2<f32> = normals[face];
        let distance: f32 = 0.5 - dot(relative, normal);
        if distance < nearest_distance {
            nearest_distance = distance;
            nearest_normal = normal;
        }
        if distance < best_distance && !solid_cell(cell + vec2<i32>(normal)) {
            best_distance = distance;
            best_normal = normal;
        }
    }
    return select(nearest_normal, best_normal, best_distance < 2.0);
}

/// Moves a particle center away from nearby solid cell squares and clamps it to the wall boundary
fn project_from_solids(position: vec2<f32>) -> FluidProjectionResult {
    var projected: vec2<f32> = clamp(
        position,
        vec2<f32>(0.4501),
        vec2<f32>(f32(uniforms.cell_width) - 0.4501, f32(uniforms.cell_height) - 0.4501),
    );
    var wall_velocity: vec2<f32> = vec2<f32>(0.0);
    for (var projection_iteration: u32 = 0u; projection_iteration < 2u;
         projection_iteration++) {
        var projected_any: bool = false;
        let base: vec2<i32> = vec2<i32>(floor(projected));
        for (var y: i32 = -1; y <= 1; y++) {
            for (var x: i32 = -1; x <= 1; x++) {
                let cell: vec2<i32> = base + vec2<i32>(x, y);
                if !solid_cell(cell) { continue; }
                let minimum: vec2<f32> = vec2<f32>(cell);
                let maximum: vec2<f32> = minimum + 1.0;
                let closest: vec2<f32> = clamp(projected, minimum, maximum);
                let offset: vec2<f32> = projected - closest;
                let distance_squared: f32 = dot(offset, offset);
                if distance_squared >= 0.25010002 { continue; }
                var normal: vec2<f32>;
                if distance_squared > 0.0000000001 {
                    normal = offset / sqrt(distance_squared);
                } else {
                    normal = solid_exit_normal(cell, projected - (minimum + 0.5));
                }
                projected = closest + normal * 0.4501;
                let logical_index: u32 = u32(cell.y) * uniforms.cell_width + u32(cell.x);
                let boundary_cell: vec4<f32> = boundary_psi[logical_index];
                wall_velocity = boundary_cell.yz;
                projected_any = true;
            }
        }
        projected = clamp(
            projected,
            vec2<f32>(0.4501),
            vec2<f32>(f32(uniforms.cell_width) - 0.4501,
                f32(uniforms.cell_height) - 0.4501),
        );
        if !projected_any { break; }
    }
    return FluidProjectionResult(projected, wall_velocity);
}

/// Adds a signed fixed-point value to one actor contact record without integer wrap
fn add_actor_contact(accumulator_index: u32, amount: i32) {
    var previous: i32 = atomicLoad(&fluid_actor_contacts[accumulator_index]);
    loop {
        var next: i32 = previous;
        if amount > 0 {
            if previous > 2147483647 - amount {
                next = 2147483647;
            } else {
                next = previous + amount;
            }
        } else if amount < 0 {
            if previous < -2147483647 - amount - 1 {
                next = -2147483647 - 1;
            } else {
                next = previous + amount;
            }
        }
        let exchange = atomicCompareExchangeWeak(
            &fluid_actor_contacts[accumulator_index],
            previous,
            next,
        );
        if exchange.exchanged { return; }
        previous = exchange.old_value;
    }
}

/// Adds the opposite particle momentum change caused by an actor projection
fn add_actor_reaction(
    actor_index: u32,
    material_identifier: u32,
    displacement: vec2<f32>,
) {
    let density: f32 = fluid_materials[(material_identifier & 0x3fffffffu) * 2u].x;
    let impulse: vec2<f32> = -density * displacement * uniforms.substep_count;
    let fixed_impulse: vec2<i32> = vec2<i32>(round(clamp(
        impulse * uniforms.actor_contact_scale,
        vec2<f32>(-2147483000.0),
        vec2<f32>(2147483000.0),
    )));
    let contact_base: u32 = actor_index * 8u;
    add_actor_contact(contact_base, fixed_impulse.x);
    add_actor_contact(contact_base + 1u, fixed_impulse.y);
}

/// Rotates a normal from actor-local space into the fluid region
fn actor_world_normal(normal_local: vec2<f32>, rotation: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(
        normal_local.x * rotation.x - normal_local.y * rotation.y,
        normal_local.x * rotation.y + normal_local.y * rotation.x,
    );
}

/// Projects a particle center outside one rotated actor shape inflated by half a cell
fn project_from_actor(position: vec2<f32>, actor: FluidActor) -> vec4<f32> {
    let difference: vec2<f32> = position - actor.center;
    let local: vec2<f32> = vec2<f32>(
        difference.x * actor.rotation.x + difference.y * actor.rotation.y,
        -difference.x * actor.rotation.y + difference.y * actor.rotation.x,
    );
    var projected_local: vec2<f32> = local;
    var normal_local: vec2<f32> = vec2<f32>(0.0);
    if actor.shape == 0u {
        let distance: f32 = length(local);
        let radius: f32 = actor.size.x + 0.4501;
        if distance >= radius {
            if distance <= radius + 0.002 {
                normal_local = select(vec2<f32>(1.0, 0.0), local / distance,
                    distance > 0.00001);
                return vec4<f32>(position, actor_world_normal(normal_local, actor.rotation));
            }
            return vec4<f32>(position, vec2<f32>(0.0));
        }
        normal_local = select(vec2<f32>(1.0, 0.0), local / distance,
            distance > 0.00001);
        projected_local = normal_local * radius;
    } else if actor.shape == 1u {
        let half_size: vec2<f32> = actor.size;
        let outside: vec2<f32> = max(abs(local) - half_size, vec2<f32>(0.0));
        let outside_distance: f32 = length(outside);
        if outside_distance > 0.00001 {
            if outside_distance >= 0.4501 {
                if outside_distance <= 0.4521 {
                    normal_local = sign(local) * outside / outside_distance;
                    return vec4<f32>(position,
                        actor_world_normal(normal_local, actor.rotation));
                }
                return vec4<f32>(position, vec2<f32>(0.0));
            }
            normal_local = sign(local) * outside / outside_distance;
            projected_local = local + normal_local * (0.4501 - outside_distance);
        } else {
            let face_distance: vec2<f32> = half_size - abs(local);
            if face_distance.x <= face_distance.y {
                normal_local = vec2<f32>(select(-1.0, 1.0, local.x >= 0.0), 0.0);
                projected_local.x = local.x + normal_local.x * (face_distance.x + 0.5001);
            } else {
                normal_local = vec2<f32>(0.0, select(-1.0, 1.0, local.y >= 0.0));
                projected_local.y = local.y + normal_local.y * (face_distance.y + 0.5001);
            }
        }
    } else {
        let closest: vec2<f32> = vec2<f32>(
            0.0,
            clamp(local.y, -actor.size.x, actor.size.x),
        );
        let offset: vec2<f32> = local - closest;
        let distance: f32 = length(offset);
        let radius: f32 = actor.size.y + 0.5001;
        if distance >= radius {
            if distance <= radius + 0.002 {
                normal_local = select(vec2<f32>(1.0, 0.0), offset / distance,
                    distance > 0.00001);
                return vec4<f32>(position, actor_world_normal(normal_local, actor.rotation));
            }
            return vec4<f32>(position, vec2<f32>(0.0));
        }
        normal_local = select(vec2<f32>(1.0, 0.0), offset / distance,
            distance > 0.00001);
        projected_local = closest + normal_local * radius;
    }
    let projected: vec2<f32> = actor.center + vec2<f32>(
        projected_local.x * actor.rotation.x - projected_local.y * actor.rotation.y,
        projected_local.x * actor.rotation.y + projected_local.y * actor.rotation.x,
    );
    return vec4<f32>(projected, actor_world_normal(normal_local, actor.rotation));
}

/// Projects predicted positions against moving actors and accumulates their reactions
@compute @workgroup_size(PARTICLE_WORKGROUP_SIZE)
fn project_actors(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let particle_count: u32 = atomicLoad(&control[0]);
    if invocation.x >= particle_count { return; }
    let particle: FluidParticle = load_particle(invocation.x, uniforms.state_side);
    if position_is_sleeping(particle.state.xy) { return; }
    let output_side: u32 = 1u - uniforms.predicted_side;
    var position: vec2<f32> = load_predicted(invocation.x, output_side);
    let actor_step_duration: f32 = 1.0 / uniforms.substep_count;
    let actor_start_time: f32 = max(uniforms.actor_time - actor_step_duration, 0.0);
    for (var actor_index: u32 = 0u; actor_index < uniforms.actor_count; actor_index++) {
        var actor: FluidActor = fluid_actors[actor_index];
        let motion: vec2<f32> = actor.velocity * actor_step_duration;
        let start_offset: vec2<f32> = actor.velocity * actor_start_time;
        let margin: vec2<f32> = abs(motion) + vec2<f32>(0.5);
        if any(position < actor.bounds.xy + start_offset - margin)
            || any(position > actor.bounds.zw + start_offset + margin)
        { continue; }
        actor.center += start_offset;
        let sweep_steps: u32 = clamp(u32(ceil(length(motion) / 0.5)), 1u, 8u);
        for (var sweep_step: u32 = 0u; sweep_step < sweep_steps; sweep_step++) {
            actor.center += motion / f32(sweep_steps);
            let projection: vec4<f32> = project_from_actor(position, actor);
            var projected: vec2<f32> = projection.xy;
            let normal: vec2<f32> = projection.zw;
            if dot(normal, normal) > 0.5 {
                let particle_velocity: vec2<f32> =
                    (projected - particle.state.xy) * uniforms.substep_count;
                let into_actor: f32 = dot(particle_velocity - actor.velocity, normal);
                if into_actor < 0.0 {
                    projected += normal * (-into_actor / uniforms.substep_count);
                }
            }
            let displacement: vec2<f32> = projected - position;
            if dot(displacement, displacement) > 0.0000000001 {
                add_actor_reaction(actor_index, particle.material_identifier, displacement);
                position = projected;
                let solid_projection: FluidProjectionResult = project_from_solids(position);
                position = solid_projection.position;
            }
        }
    }
    store_predicted(invocation.x, output_side, position);
}

/// Wakes sleeping particle tiles covered or approached by an actor this tick
@compute @workgroup_size(1)
fn wake_actor_neighbors(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let tile_count: u32 = uniforms.region_width * uniforms.region_height;
    if invocation.x >= tile_count || !tile_is_sleeping(invocation.x) { return; }
    let tile: vec2<f32> = vec2<f32>(
        f32(invocation.x % uniforms.region_width) * 8.0,
        f32(invocation.x / uniforms.region_width) * 8.0,
    );
    let tile_maximum: vec2<f32> = tile + vec2<f32>(8.0);
    for (var actor_index: u32 = 0u; actor_index < uniforms.actor_count; actor_index++) {
        let actor: FluidActor = fluid_actors[actor_index];
        let margin: vec2<f32> = vec2<f32>(1.0) + abs(actor.velocity);
        if all(actor.bounds.zw + margin >= tile)
            && all(actor.bounds.xy - margin <= tile_maximum)
        {
            wake_tile(invocation.x);
            return;
        }
    }
}

/// Returns whether a particle cell is marked for deletion by the uploaded per-tile edit mask
fn particle_is_erased(position: vec2<f32>) -> bool {
    let cell: vec2<i32> = vec2<i32>(floor(position));
    if !cell_inside(cell) { return true; }
    let tile: vec2<u32> = vec2<u32>(cell) / 8u;
    let tile_buffer_index: u32 = tile.y * uniforms.region_width + tile.x;
    let local_cell: u32 = (u32(cell.y) % 8u) * 8u + u32(cell.x) % 8u;
    let erased_word: u32 = erasure_masks[tile_buffer_index][local_cell / 32u];
    return (erased_word & (1u << (local_cell % 32u))) != 0u;
}

/// Writes an indirect particle dispatch sized from the current GPU live count
@compute @workgroup_size(1)
fn prepare() {
    let particle_count: u32 = atomicLoad(&control[0]);
    dispatch_arguments[0] = max(1u, (particle_count + PARTICLE_WORKGROUP_SIZE - 1u)
        / PARTICLE_WORKGROUP_SIZE);
    dispatch_arguments[1] = 1u;
    dispatch_arguments[2] = 1u;
}

/// Writes one indirect workgroup for every compactly listed tile
@compute @workgroup_size(1)
fn prepare_boundary() {
    let tile_count: u32 = atomicLoad(&control[2]);
    dispatch_arguments[0] = max(1u, tile_count);
    dispatch_arguments[1] = 1u;
    dispatch_arguments[2] = 1u;
}

/// Resets the global count used to launch a second prediction after a tile wakes
@compute @workgroup_size(1)
fn reset_wake_count() {
    atomicStore(&control[wake_count_offset()], 0u);
}

/// Writes a conditional indirect particle dispatch for tiles woken during prediction
@compute @workgroup_size(1)
fn prepare_wake_dispatch() {
    let wake_count: u32 = atomicLoad(&control[wake_count_offset()]);
    let workgroup_count: u32 = select(
        0u,
        (atomicLoad(&control[0]) + PARTICLE_WORKGROUP_SIZE - 1u)
            / PARTICLE_WORKGROUP_SIZE,
        wake_count != 0u,
    );
    dispatch_arguments[3] = workgroup_count;
    dispatch_arguments[4] = 1u;
    dispatch_arguments[5] = 1u;
}

/// Applies tile wake requests after the first prediction has finished
@compute @workgroup_size(1)
fn apply_pending_wakes(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let tile_count: u32 = uniforms.region_width * uniforms.region_height;
    if invocation.x >= tile_count { return; }
    let base: u32 = sleep_tile_offset(invocation.x);
    let previous_flags: u32 = atomicLoad(&control[base + 4u]);
    if (previous_flags & SLEEP_TILE_WAKE_PENDING) == 0u { return; }
    let next_flags: u32 = (previous_flags
        & ~(SLEEP_TILE_SLEEPING | SLEEP_TILE_WAKE_PENDING))
        | SLEEP_TILE_WAKE_THIS_TICK;
    atomicStore(&control[base + 2u], 0u);
    atomicStore(&control[base + 4u], next_flags);
}

/// Updates per-tile wake state, compares solid occupancy, and clears the new reductions
@compute @workgroup_size(1)
fn prepare_sleep(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let tile_count: u32 = uniforms.region_width * uniforms.region_height;
    if invocation.x >= tile_count { return; }
    let tile_index: u32 = invocation.x;
    let base: u32 = sleep_tile_offset(tile_index);
    let previous_flags: u32 = atomicLoad(&control[base + 4u]);
    let tile_flags: u32 = atomicLoad(&boundary_tile_data[tile_index]);
    let has_particles: bool = tile_flags == BOUNDARY_TILE_PARTICLE;
    let listed: bool = tile_flags != 0u;
    var solid_hash: u32 = 2166136261u;
    if listed {
        let solid_base: u32 = 2u * tile_count + tile_index * 16u;
        for (var word: u32 = 0u; word < 16u; word++) {
            solid_hash = (solid_hash ^ atomicLoad(&boundary_tile_data[solid_base + word]))
                * 16777619u;
        }
    }
    var wake: bool = (previous_flags & SLEEP_TILE_WAKE_THIS_TICK) != 0u;
    let hash_valid: bool = (previous_flags & SLEEP_TILE_HASH_VALID) != 0u;
    let solid_changed: bool = listed && hash_valid
        && atomicLoad(&control[base + 3u]) != solid_hash;
    let rebased: bool = uniforms.rebase_x != 0.0 || uniforms.rebase_y != 0.0;
    if solid_changed || rebased { wake = true; }
    if uniforms.erasure_count != 0u {
        let erased: vec2<u32> = erasure_masks[tile_index];
        if erased.x != 0u || erased.y != 0u { wake = true; }
    }
    for (var offset_y: i32 = -1; offset_y <= 1; offset_y++) {
        for (var offset_x: i32 = -1; offset_x <= 1; offset_x++) {
            if offset_x == 0 && offset_y == 0 { continue; }
            let neighbor: vec2<i32> = vec2<i32>(
                i32(tile_index % uniforms.region_width),
                i32(tile_index / uniforms.region_width),
            ) + vec2<i32>(offset_x, offset_y);
            if any(neighbor < vec2<i32>(0))
                || neighbor.x >= i32(uniforms.region_width)
                || neighbor.y >= i32(uniforms.region_height)
            { continue; }
            let neighbor_index: u32 = u32(neighbor.y) * uniforms.region_width
                + u32(neighbor.x);
            let neighbor_base: u32 = sleep_tile_offset(neighbor_index);
            let neighbor_flags: u32 = atomicLoad(&control[neighbor_base + 6u]);
            let neighbor_speed: f32 = bitcast<f32>(atomicLoad(&control[neighbor_base + 5u]));
            if (neighbor_flags & SLEEP_TILE_HAS_PARTICLE) != 0u
                && (neighbor_flags & SLEEP_TILE_SLEEPING) == 0u
                && neighbor_speed > WAKE_SPEED
            { wake = true; }
        }
    }
    var flags: u32 = previous_flags & (SLEEP_TILE_SLEEPING | SLEEP_TILE_HASH_VALID);
    if listed { flags |= SLEEP_TILE_HASH_VALID; }
    if has_particles && !hash_valid { wake = true; }
    if solid_changed { flags |= SLEEP_TILE_SOLID_CHANGED; }
    if wake {
        flags = (flags & ~SLEEP_TILE_SLEEPING) | SLEEP_TILE_WAKE_THIS_TICK;
        atomicStore(&control[base + 2u], 0u);
    } else if !has_particles {
        flags &= ~SLEEP_TILE_SLEEPING;
        atomicStore(&control[base + 2u], 0u);
    }
    if listed { atomicStore(&control[base + 3u], solid_hash); }
    atomicStore(&control[base], 0u);
    atomicStore(&control[base + 1u], 0u);
    atomicStore(&control[base + 4u], flags);
}

/// Wakes particle tiles next to a tile whose solid occupancy changed
@compute @workgroup_size(1)
fn wake_solid_neighbors(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let tile_count: u32 = uniforms.region_width * uniforms.region_height;
    if invocation.x >= tile_count
        || atomicLoad(&boundary_tile_data[invocation.x]) != BOUNDARY_TILE_PARTICLE
    { return; }
    let tile: vec2<i32> = vec2<i32>(
        i32(invocation.x % uniforms.region_width),
        i32(invocation.x / uniforms.region_width),
    );
    for (var node_y: u32 = 0u; node_y <= 8u; node_y++) {
        for (var node_x: u32 = 0u; node_x <= 8u; node_x++) {
            let node: vec2<u32> = vec2<u32>(tile) * 8u + vec2<u32>(node_x, node_y);
            if abs(load_boundary_node(node).w) > 0.000001 {
                wake_tile(invocation.x);
                return;
            }
        }
    }
    for (var offset_y: i32 = -1; offset_y <= 1; offset_y++) {
        for (var offset_x: i32 = -1; offset_x <= 1; offset_x++) {
            let neighbor: vec2<i32> = tile + vec2<i32>(offset_x, offset_y);
            if any(neighbor < vec2<i32>(0))
                || neighbor.x >= i32(uniforms.region_width)
                || neighbor.y >= i32(uniforms.region_height)
            { continue; }
            let neighbor_index: u32 = u32(neighbor.y) * uniforms.region_width
                + u32(neighbor.x);
            let neighbor_flags: u32 =
                atomicLoad(&control[sleep_tile_offset(neighbor_index) + 4u]);
            if (neighbor_flags & SLEEP_TILE_SOLID_CHANGED) != 0u {
                wake_tile(invocation.x);
                return;
            }
        }
    }
}

/// Advances calm-tick counters and freezes tiles whose particles stayed at rest
@compute @workgroup_size(1)
fn finish_sleep(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let tile_count: u32 = uniforms.region_width * uniforms.region_height;
    if invocation.x >= tile_count { return; }
    let base: u32 = sleep_tile_offset(invocation.x);
    var flags: u32 = atomicLoad(&control[base + 4u]);
    let has_particles: bool = (flags & SLEEP_TILE_HAS_PARTICLE) != 0u;
    let was_woken: bool = (flags & SLEEP_TILE_WAKE_THIS_TICK) != 0u;
    let speed: f32 = bitcast<f32>(atomicLoad(&control[base]));
    let density_error: f32 = bitcast<f32>(atomicLoad(&control[base + 1u]));
    var calm_ticks: u32 = atomicLoad(&control[base + 2u]);
    if !has_particles || was_woken {
        calm_ticks = 0u;
        flags &= ~SLEEP_TILE_SLEEPING;
    } else if speed < SLEEP_SPEED && density_error < SLEEP_DENSITY_ERROR {
        calm_ticks = min(calm_ticks + 1u, SLEEP_TICK_COUNT);
        if calm_ticks >= SLEEP_TICK_COUNT { flags |= SLEEP_TILE_SLEEPING; }
    } else {
        calm_ticks = 0u;
        flags &= ~SLEEP_TILE_SLEEPING;
    }
    flags &= ~SLEEP_TILE_WAKE_THIS_TICK;
    atomicStore(&control[base + 2u], calm_ticks);
    atomicStore(&control[base + 4u], flags);
    atomicStore(&control[base + 5u], atomicLoad(&control[base]));
    atomicStore(&control[base + 6u], flags);
}

/// Writes an indirect dispatch sized from the previous sparse raster-tile list
@compute @workgroup_size(1)
fn prepare_raster() {
    let listed_count: u32 = atomicLoad(&fluid_raster[raster_tile_count_offset()]);
    dispatch_arguments[0] = max(1u, listed_count);
    dispatch_arguments[1] = 1u;
    dispatch_arguments[2] = 1u;
}

/// Writes an indirect dispatch sized from the solid-contact tile list
@compute @workgroup_size(1)
fn prepare_fluid_contacts() {
    let listed_count: u32 = atomicLoad(&fluid_raster[fluid_contact_tile_count_offset()]);
    dispatch_arguments[0] = max(1u, listed_count);
    dispatch_arguments[1] = 1u;
    dispatch_arguments[2] = 1u;
}

/// Clears raster cells from the prior tick's physical tile list only
@compute @workgroup_size(64)
fn clear_fluid_raster_tiles(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    let listed_count: u32 = atomicLoad(&fluid_raster[raster_tile_count_offset()]);
    if workgroup.x >= listed_count { return; }
    let physical_tile: u32 = atomicLoad(
        &fluid_raster[raster_tile_list_offset() + workgroup.x],
    );
    if local_index == 0u {
        atomicStore(&fluid_raster[raster_tile_flags_offset() + physical_tile], 0u);
    }
    clear_raster_cell(physical_tile * 64u + local_index);
}

/// Resets the sparse raster-tile list after its old entries have been cleared
@compute @workgroup_size(1)
fn reset_fluid_raster_tile_count() {
    atomicStore(&fluid_raster[raster_tile_count_offset()], 0u);
}

/// Clears current particle tiles and their motion margin before this tick's splats
@compute @workgroup_size(64)
fn clear_fluid_current_tiles(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    let tile_count: u32 = uniforms.region_width * uniforms.region_height;
    let listed_count: u32 = atomicLoad(&control[2]);
    if workgroup.x >= listed_count { return; }
    let logical_tile: u32 = atomicLoad(&boundary_tile_data[tile_count + workgroup.x]);
    let tile: vec2<u32> = vec2<u32>(
        logical_tile % uniforms.region_width,
        logical_tile / uniforms.region_width,
    );
    let physical_tile: u32 = tile_index(tile);
    clear_raster_cell(physical_tile * 64u + local_index);
}

/// Rasterizes each live particle into its nearest physical ring cell
@compute @workgroup_size(PARTICLE_WORKGROUP_SIZE)
fn rasterize_fluid(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let particle_count: u32 = atomicLoad(&control[0]);
    if invocation.x >= particle_count { return; }
    let particle: FluidParticle = load_particle(invocation.x, uniforms.state_side);
    let cell: vec2<i32> = vec2<i32>(floor(particle.state.xy));
    if !cell_inside(cell) { return; }
    let material_index: u32 = particle.material_identifier & 0x3fffffffu;
    let material: vec4<f32> = fluid_materials[material_index * 2u];
    let mass: f32 = material.x;
    let mass_fixed: i32 = i32(clamp(
        round(mass * uniforms.raster_scale),
        0.0,
        2147483000.0,
    ));
    let momentum_fixed: vec2<i32> = vec2<i32>(round(clamp(
        particle.state.zw * mass * uniforms.raster_scale,
        vec2<f32>(-2147483000.0),
        vec2<f32>(2147483000.0),
    )));
    let physical_cell: u32 = cell_index(cell);
    let base: u32 = physical_cell * 4u;
    atomic_saturating_add(base, mass_fixed);
    atomic_saturating_add(base + 1u, momentum_fixed.x);
    atomic_saturating_add(base + 2u, momentum_fixed.y);
    atomicMax(&fluid_raster[base + 3u], particle.material_identifier);
    let physical_tile: u32 = tile_index(vec2<u32>(cell) / 8u);
    let flag_index: u32 = raster_tile_flags_offset() + physical_tile;
    // reading the flag first leaves only a tile's first particles contending to list it
    if atomicLoad(&fluid_raster[flag_index]) != 0u { return; }
    loop {
        let exchange = atomicCompareExchangeWeak(&fluid_raster[flag_index], 0u, 1u);
        if exchange.exchanged {
            let list_index: u32 = atomicAdd(
                &fluid_raster[raster_tile_count_offset()],
                1u,
            );
            atomicStore(
                &fluid_raster[raster_tile_list_offset() + list_index],
                physical_tile,
            );
            break;
        }
        if exchange.old_value != 0u { break; }
    }
}

/// Adds the velocity change that cellular kinematics' solid contacts left in a particle's cell
@compute @workgroup_size(PARTICLE_WORKGROUP_SIZE)
fn apply_fluid_contacts(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let particle_count: u32 = atomicLoad(&control[0]);
    if invocation.x >= particle_count { return; }
    var particle: FluidParticle = load_particle(invocation.x, uniforms.state_side);
    let cell: vec2<i32> = vec2<i32>(floor(particle.state.xy));
    if !cell_inside(cell) { return; }
    let base: u32 = fluid_contact_offset() + cell_index(cell) * 2u;
    let velocity_change: vec2<f32> = vec2<f32>(
        f32(bitcast<i32>(atomicLoad(&fluid_raster[base]))),
        f32(bitcast<i32>(atomicLoad(&fluid_raster[base + 1u]))),
    ) / uniforms.raster_scale;
    if all(velocity_change == vec2<f32>(0.0)) { return; }
    particle.state = vec4<f32>(particle.state.xy, particle.state.zw + velocity_change);
    store_particle(invocation.x, uniforms.state_side, particle);
}

/// Clears contact velocity changes in tiles that received them
@compute @workgroup_size(64)
fn clear_fluid_contacts(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    let listed_count: u32 = atomicLoad(&fluid_raster[fluid_contact_tile_count_offset()]);
    if workgroup.x >= listed_count { return; }
    let physical_tile: u32 = atomicLoad(
        &fluid_raster[fluid_contact_tile_list_offset() + workgroup.x],
    );
    let base: u32 = fluid_contact_offset() + (physical_tile * 64u + local_index) * 2u;
    atomicStore(&fluid_raster[base], 0u);
    atomicStore(&fluid_raster[base + 1u], 0u);
    if local_index == 0u {
        atomicStore(&fluid_raster[fluid_contact_tile_flags_offset() + physical_tile], 0u);
    }
}

/// Resets the solid-contact tile list after its old entries have been cleared
@compute @workgroup_size(1)
fn reset_fluid_contact_tile_count() {
    atomicStore(&fluid_raster[fluid_contact_tile_count_offset()], 0u);
}

/// Returns whether a cell center lies inside an actor's uninflated rotated shape
fn actor_covers_cell(position: vec2<f32>, actor: FluidActor) -> bool {
    let difference: vec2<f32> = position - actor.center;
    let local: vec2<f32> = vec2<f32>(
        difference.x * actor.rotation.x + difference.y * actor.rotation.y,
        -difference.x * actor.rotation.y + difference.y * actor.rotation.x,
    );
    if actor.shape == 0u {
        return dot(local, local) <= actor.size.x * actor.size.x;
    }
    if actor.shape == 1u { return all(abs(local) <= actor.size); }
    let closest: vec2<f32> = vec2<f32>(
        0.0,
        clamp(local.y, -actor.size.x, actor.size.x),
    );
    let offset: vec2<f32> = local - closest;
    return dot(offset, offset) <= actor.size.y * actor.size.y;
}

/// Returns whether a cell center lies inside an actor shape expanded by the fluid sample margin
fn actor_covers_sample_cell(position: vec2<f32>, actor: FluidActor) -> bool {
    let difference: vec2<f32> = position - actor.center;
    let local: vec2<f32> = vec2<f32>(
        difference.x * actor.rotation.x + difference.y * actor.rotation.y,
        -difference.x * actor.rotation.y + difference.y * actor.rotation.x,
    );
    if actor.shape == 0u {
        let radius: f32 = actor.size.x + FLUID_ACTOR_SAMPLE_MARGIN;
        return dot(local, local) <= radius * radius && !actor_covers_cell(position, actor);
    }
    if actor.shape == 1u {
        return all(abs(local) <= actor.size + FLUID_ACTOR_SAMPLE_MARGIN)
            && !actor_covers_cell(position, actor);
    }
    let closest: vec2<f32> = vec2<f32>(
        0.0,
        clamp(local.y, -actor.size.x, actor.size.x),
    );
    let radius: f32 = actor.size.y + FLUID_ACTOR_SAMPLE_MARGIN;
    let offset: vec2<f32> = local - closest;
    return dot(offset, offset) <= radius * radius && !actor_covers_cell(position, actor);
}

/// Converts a bounded fluid sample to the actor contact fixed-point scale
fn actor_sample_fixed(value: f32) -> i32 {
    return i32(round(clamp(
        value * uniforms.actor_contact_scale,
        -2147483000.0,
        2147483000.0,
    )));
}

/// Samples fluid cells around each actor and stores one fixed-point contact result per actor
@compute @workgroup_size(PARTICLE_WORKGROUP_SIZE)
fn actor_contacts(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if invocation.x >= uniforms.actor_count { return; }
    var actor: FluidActor = fluid_actors[invocation.x];
    actor.center += actor.velocity * uniforms.actor_time;
    let minimum: vec2<i32> = vec2<i32>(floor(actor.bounds.xy));
    let maximum: vec2<i32> = vec2<i32>(ceil(actor.bounds.zw));
    var sampled_count: u32 = 0u;
    var total_mass: f32 = 0.0;
    var total_momentum: vec2<f32> = vec2<f32>(0.0);
    for (var y: i32 = minimum.y; y < maximum.y; y++) {
        for (var x: i32 = minimum.x; x < maximum.x; x++) {
            let cell: vec2<i32> = vec2<i32>(x, y);
            if !actor_covers_sample_cell(vec2<f32>(cell) + 0.5, actor) { continue; }
            if !cell_inside(cell) { continue; }
            let index: u32 = cell_index(cell);
            if material_identifiers[index] != 0u || rigid_body_cells[index] != 0u { continue; }
            sampled_count += 1u;
            let base: u32 = index * 4u;
            let mass: f32 = max(
                f32(bitcast<i32>(atomicLoad(&fluid_raster[base]))) / uniforms.raster_scale,
                0.0,
            );
            if mass <= 0.0 { continue; }
            total_mass += mass;
            total_momentum += vec2<f32>(
                f32(bitcast<i32>(atomicLoad(&fluid_raster[base + 1u]))),
                f32(bitcast<i32>(atomicLoad(&fluid_raster[base + 2u]))),
            ) / uniforms.raster_scale;
        }
    }
    var dominant_material: u32 = 0u;
    var dominant_mass: f32 = 0.0;
    for (var material_index: u32 = 0u;
         material_index < uniforms.fluid_material_count;
         material_index++)
    {
        var material_mass: f32 = 0.0;
        for (var y: i32 = minimum.y; y < maximum.y; y++) {
            for (var x: i32 = minimum.x; x < maximum.x; x++) {
                let cell: vec2<i32> = vec2<i32>(x, y);
                if !cell_inside(cell)
                    || !actor_covers_sample_cell(vec2<f32>(cell) + 0.5, actor)
                { continue; }
                let index: u32 = cell_index(cell);
                if material_identifiers[index] != 0u || rigid_body_cells[index] != 0u { continue; }
                let base: u32 = index * 4u;
                let identifier: u32 = atomicLoad(&fluid_raster[base + 3u]);
                if (identifier & 0x3fffffffu) != material_index { continue; }
                material_mass += max(
                    f32(bitcast<i32>(atomicLoad(&fluid_raster[base])))
                        / uniforms.raster_scale,
                    0.0,
                );
            }
        }
        if material_mass > dominant_mass {
            dominant_material = material_index;
            dominant_mass = material_mass;
        }
    }
    var immersion: f32 = 0.0;
    var fluid_velocity: vec2<f32> = vec2<f32>(0.0);
    var fluid_density: f32 = 0.0;
    var fluid_drag: f32 = 0.0;
    if total_mass > 0.0 && dominant_mass > 0.0 {
        let material: vec4<f32> = fluid_materials[dominant_material * 2u];
        fluid_density = material.x;
        fluid_drag = fluid_materials[dominant_material * 2u + 1u].x;
        immersion = clamp(
            total_mass / f32(max(sampled_count, 1u)) / max(fluid_density, 0.000001),
            0.0,
            1.0,
        );
        fluid_velocity = total_momentum / total_mass;
    }
    let contact_base: u32 = invocation.x * 8u;
    atomicStore(
        &fluid_actor_contacts[contact_base + 2u],
        actor_sample_fixed(immersion),
    );
    atomicStore(
        &fluid_actor_contacts[contact_base + 3u],
        actor_sample_fixed(fluid_velocity.x),
    );
    atomicStore(
        &fluid_actor_contacts[contact_base + 4u],
        actor_sample_fixed(fluid_velocity.y),
    );
    atomicStore(
        &fluid_actor_contacts[contact_base + 5u],
        actor_sample_fixed(fluid_density),
    );
    atomicStore(
        &fluid_actor_contacts[contact_base + 6u],
        actor_sample_fixed(fluid_drag),
    );
    if actor.swimming != 0u && fluid_drag > 0.0 {
        for (var y: i32 = minimum.y; y < maximum.y; y++) {
            for (var x: i32 = minimum.x; x < maximum.x; x++) {
                let cell: vec2<i32> = vec2<i32>(x, y);
                if !cell_inside(cell)
                    || !actor_covers_sample_cell(vec2<f32>(cell) + 0.5, actor)
                { continue; }
                let index: u32 = cell_index(cell);
                if material_identifiers[index] != 0u || rigid_body_cells[index] != 0u { continue; }
                let base: u32 = index * 4u;
                let mass: f32 = max(
                    f32(bitcast<i32>(atomicLoad(&fluid_raster[base])))
                        / uniforms.raster_scale,
                    0.0,
                );
                if mass <= 0.0 { continue; }
                let velocity: vec2<f32> = vec2<f32>(
                    f32(bitcast<i32>(atomicLoad(&fluid_raster[base + 1u]))),
                    f32(bitcast<i32>(atomicLoad(&fluid_raster[base + 2u]))),
                ) / uniforms.raster_scale / mass;
                let coverage: f32 = clamp(mass / max(fluid_density, 0.000001), 0.0, 1.0);
                let coupling: f32 = clamp(
                    1.0 - exp(-fluid_drag * coverage * FLUID_TICK_TIME_STEP),
                    0.0,
                    1.0,
                );
                let change: vec2<i32> = vec2<i32>(round(clamp(
                    (actor.velocity - velocity) * coupling * uniforms.raster_scale,
                    vec2<f32>(-1.0e9),
                    vec2<f32>(1.0e9),
                )));
                let delta_base: u32 = fluid_contact_offset() + index * 2u;
                add_fluid_contact_tile(index / 64u);
                atomicAdd(&fluid_raster[delta_base], bitcast<u32>(change.x));
                atomicAdd(&fluid_raster[delta_base + 1u], bitcast<u32>(change.y));
            }
        }
    }
}

/// Rebases live particles, exports particles outside the region, and packs survivors
@compute @workgroup_size(PARTICLE_WORKGROUP_SIZE)
fn compact(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let source_count: u32 = atomicLoad(&control[0]);
    if invocation.x >= source_count { return; }
    let particle: FluidParticle = load_particle(invocation.x, uniforms.state_side);
    var position: vec2<f32> = particle.state.xy
        + vec2<f32>(uniforms.rebase_x, uniforms.rebase_y);
    if uniforms.erasure_count != 0u && particle_is_erased(position) { return; }
    if any(position < vec2<f32>(0.0))
        || position.x >= f32(uniforms.cell_width)
        || position.y >= f32(uniforms.cell_height)
    {
        let export_index: u32 = atomicAdd(&fluid_particle_exports[0], 1u);
        let export_base: u32 = 1u + export_index * 10u;
        let cell: vec2<i32> = vec2<i32>(floor(position));
        let local_x: u32 = bitcast<u32>(cell.x);
        let local_y: u32 = bitcast<u32>(cell.y);
        let world_x_low: u32 = uniforms.world_origin_low.x + local_x;
        let world_y_low: u32 = uniforms.world_origin_low.y + local_y;
        let carry_x: u32 = select(0u, 1u, world_x_low < uniforms.world_origin_low.x);
        let carry_y: u32 = select(0u, 1u, world_y_low < uniforms.world_origin_low.y);
        let sign_x: u32 = select(0u, 0xffffffffu, cell.x < 0);
        let sign_y: u32 = select(0u, 0xffffffffu, cell.y < 0);
        atomicStore(&fluid_particle_exports[export_base], world_x_low);
        atomicStore(&fluid_particle_exports[export_base + 1u],
            uniforms.world_origin_high.x + sign_x + carry_x);
        atomicStore(&fluid_particle_exports[export_base + 2u], world_y_low);
        atomicStore(&fluid_particle_exports[export_base + 3u],
            uniforms.world_origin_high.y + sign_y + carry_y);
        atomicStore(&fluid_particle_exports[export_base + 4u],
            bitcast<u32>(position.x - f32(cell.x)));
        atomicStore(&fluid_particle_exports[export_base + 5u],
            bitcast<u32>(position.y - f32(cell.y)));
        atomicStore(&fluid_particle_exports[export_base + 6u], bitcast<u32>(particle.state.z));
        atomicStore(&fluid_particle_exports[export_base + 7u], bitcast<u32>(particle.state.w));
        atomicStore(&fluid_particle_exports[export_base + 8u], particle.material_identifier);
        atomicStore(&fluid_particle_exports[export_base + 9u], 0u);
        return;
    }
    let output_index: u32 = atomicAdd(&control[1], 1u);
    var survivor: FluidParticle = particle;
    survivor.state = vec4<f32>(position, particle.state.zw);
    let output_side: u32 = 1u - uniforms.state_side;
    store_particle(output_index, output_side, survivor);
}

/// Replaces the live count with the number of survivors after compaction
@compute @workgroup_size(1)
fn finish_compaction() {
    atomicStore(&control[0], atomicLoad(&control[1]));
}

/// Advances the Insert-occupancy stamp so old cells require no full-region clear
@compute @workgroup_size(1)
fn begin_occupancy() {
    let previous_generation: u32 = atomicAdd(&control[3], 1u);
    if previous_generation == 0xffffffffu {
        atomicStore(&control[3], 1u);
    }
}

/// Adds one edited fluid cell when the cell grid, occupancy, and fixed capacity allow it
@compute @workgroup_size(PARTICLE_WORKGROUP_SIZE)
fn spawn(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if invocation.x >= uniforms.edit_count { return; }
    let command: vec4<u32> = edit_commands[invocation.x];
    let material_index: u32 = command.z & 0x3fffffffu;
    if command.z >> 30u != 2u || material_index >= uniforms.fluid_material_count { return; }
    let cell: vec2<i32> = vec2<i32>(command.xy);
    if !cell_inside(cell) { return; }
    let edit_tile_index: u32 = u32(cell.y / 8) * uniforms.region_width
        + u32(cell.x / 8);
    wake_tile(edit_tile_index);
    let cell_buffer_index: u32 = cell_index(cell);
    if material_identifiers[cell_buffer_index] != 0u { return; }
    let logical_index: u32 = u32(cell.y) * uniforms.cell_width + u32(cell.x);
    let occupancy_index: u32 = 4u + logical_index;
    let generation: u32 = atomicLoad(&control[3]);
    if command.w != 0u {
        var old_generation: u32 = atomicLoad(&control[occupancy_index]);
        loop {
            if old_generation == generation { return; }
            let exchange = atomicCompareExchangeWeak(
                &control[occupancy_index],
                old_generation,
                generation,
            );
            if exchange.exchanged { break; }
            old_generation = exchange.old_value;
        }
    } else {
        atomicStore(&control[occupancy_index], generation);
    }
    var count: u32 = atomicLoad(&control[0]);
    loop {
        if count >= uniforms.capacity { return; }
        let exchange = atomicCompareExchangeWeak(&control[0], count, count + 1u);
        if exchange.exchanged { break; }
        count = exchange.old_value;
    }
    let jitter_hash: u32 = (u32(cell.x) * JITTER_HASH) ^ (u32(cell.y) * 2891336453u);
    let jitter_x: f32 = (f32(jitter_hash & 0xffffu) / 65535.0 - 0.5) * 0.02;
    let jitter_y: f32 = (f32(jitter_hash >> 16u) / 65535.0 - 0.5) * 0.02;
    let position: vec2<f32> = vec2<f32>(cell) + 0.5 + vec2<f32>(jitter_x, jitter_y);
    let particle: FluidParticle = FluidParticle(
        vec4<f32>(position, vec2<f32>(0.0)),
        command.z,
        position.x,
        position.y,
        0u,
    );
    store_particle(count, uniforms.state_side, particle);
}

/// Imports a persisted particle if its cell holds no matter or earlier fluid and pool capacity
/// remains
@compute @workgroup_size(PARTICLE_WORKGROUP_SIZE)
fn spawn_imported(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if invocation.x >= uniforms.import_count { return; }
    atomicStore(&fluid_particle_import_results[invocation.x], 1u);
    let imported_particle: FluidParticleImport = fluid_particle_imports[invocation.x];
    let material_index: u32 = imported_particle.material_identifier & 0x3fffffffu;
    if imported_particle.material_identifier >> 30u != 2u
        || material_index >= uniforms.fluid_material_count
    { return; }
    let cell: vec2<i32> = vec2<i32>(floor(imported_particle.state.xy));
    if !cell_inside(cell) { return; }
    let cell_buffer_index: u32 = cell_index(cell);
    if material_identifiers[cell_buffer_index] != 0u { return; }
    // Compressed saved particles may share a fluid cell; preserve each exact-position record.
    var count: u32 = atomicLoad(&control[0]);
    loop {
        if count >= uniforms.capacity { return; }
        let exchange = atomicCompareExchangeWeak(&control[0], count, count + 1u);
        if exchange.exchanged { break; }
        count = exchange.old_value;
    }
    let particle: FluidParticle = FluidParticle(imported_particle.state,
        imported_particle.material_identifier, imported_particle.state.x,
        imported_particle.state.y, 0u);
    store_particle(count, uniforms.state_side, particle);
    atomicStore(&fluid_particle_import_results[invocation.x], 0u);
}

/// Marks the occupied logical cell of every live particle for fluid Insert edits
@compute @workgroup_size(PARTICLE_WORKGROUP_SIZE)
fn mark_occupancy(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let particle_count: u32 = atomicLoad(&control[0]);
    if invocation.x >= particle_count { return; }
    let particle: FluidParticle = load_particle(invocation.x, uniforms.state_side);
    let cell: vec2<i32> = vec2<i32>(floor(particle.state.xy));
    if cell_inside(cell) {
        let logical_index: u32 = u32(cell.y) * uniforms.cell_width + u32(cell.x);
        atomicStore(&control[4u + logical_index], atomicLoad(&control[3]));
    }
}

/// Appends a tile to the compact list once, the first time any flag claims it
fn list_boundary_tile(index: u32, flag: u32) {
    loop {
        let exchange = atomicCompareExchangeWeak(&boundary_tile_data[index], 0u, flag);
        if exchange.exchanged {
            let tile_count: u32 = uniforms.region_width * uniforms.region_height;
            let list_index: u32 = atomicAdd(&control[2], 1u);
            atomicStore(&boundary_tile_data[tile_count + list_index], index);
            return;
        }
        if exchange.old_value != 0u { return; }
    }
}

/// Lists the tile of every live particle, reading its flag first so that only the first
/// particles of a tile contend for it
@compute @workgroup_size(PARTICLE_WORKGROUP_SIZE)
fn mark_boundary_tiles(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let particle_count: u32 = atomicLoad(&control[0]);
    if invocation.x >= particle_count { return; }
    let particle: FluidParticle = load_particle(invocation.x, uniforms.state_side);
    let tile: vec2<i32> = vec2<i32>(floor(particle.state.xy / 8.0));
    if any(tile < vec2<i32>(0))
        || tile.x >= i32(uniforms.region_width)
        || tile.y >= i32(uniforms.region_height)
    { return; }
    let index: u32 = u32(tile.y) * uniforms.region_width + u32(tile.x);
    if atomicLoad(&boundary_tile_data[index]) != 0u { return; }
    list_boundary_tile(index, BOUNDARY_TILE_PARTICLE);
}

/// Lists every unlisted tile within two tiles of a particle tile, covering the kernel support and
/// the motion of one tick
@compute @workgroup_size(64)
fn dilate_boundary_tiles(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if invocation.x >= uniforms.region_width * uniforms.region_height { return; }
    if atomicLoad(&boundary_tile_data[invocation.x]) != 0u { return; }
    let tile: vec2<i32> = vec2<i32>(
        i32(invocation.x % uniforms.region_width),
        i32(invocation.x / uniforms.region_width),
    );
    for (var y: i32 = -2; y <= 2; y++) {
        for (var x: i32 = -2; x <= 2; x++) {
            let neighbor: vec2<i32> = tile + vec2<i32>(x, y);
            if any(neighbor < vec2<i32>(0))
                || neighbor.x >= i32(uniforms.region_width)
                || neighbor.y >= i32(uniforms.region_height)
            { continue; }
            let neighbor_index: u32 = u32(neighbor.y) * uniforms.region_width + u32(neighbor.x);
            if atomicLoad(&boundary_tile_data[neighbor_index]) == BOUNDARY_TILE_PARTICLE {
                list_boundary_tile(invocation.x, BOUNDARY_TILE_MARGIN);
                return;
            }
        }
    }
}

/// Clears tile flags left by the previous tick by walking only its compact list
@compute @workgroup_size(1)
fn clear_boundary_tiles(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let tile_count: u32 = uniforms.region_width * uniforms.region_height;
    let previous_count: u32 = atomicLoad(&control[2]);
    if invocation.x >= previous_count { return; }
    let tile_index: u32 = atomicLoad(&boundary_tile_data[tile_count + invocation.x]);
    atomicStore(&boundary_tile_data[tile_index], 0u);
}

/// Starts this tick's tile append list after its previous flags have been cleared
@compute @workgroup_size(1)
fn reset_boundary_tile_count() {
    atomicStore(&control[2], 0u);
}

/// Packs solid occupancy for the sixty-four cells belonging to one listed tile
@compute @workgroup_size(64)
fn boundary(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    let tile_count: u32 = uniforms.region_width * uniforms.region_height;
    let listed_count: u32 = atomicLoad(&control[2]);
    if workgroup.x >= listed_count { return; }
    let tile_index: u32 = atomicLoad(&boundary_tile_data[tile_count + workgroup.x]);
    let tile: vec2<u32> = vec2<u32>(
        tile_index % uniforms.region_width,
        tile_index / uniforms.region_width,
    );
    if local_index >= 16u { return; }
    var solid_word: u32 = 0u;
    for (var packed_cell: u32 = 0u; packed_cell < 4u; packed_cell++) {
        let local_cell: u32 = local_index * 4u + packed_cell;
        let cell: vec2<i32> = vec2<i32>(tile * 8u + vec2<u32>(local_cell % 8u, local_cell / 8u));
        let cell_buffer_index: u32 = cell_index(cell);
        if material_identifiers[cell_buffer_index] != 0u {
            solid_word |= 1u << (packed_cell * 8u);
        }
    }
    let solid_base: u32 = 2u * tile_count + tile_index * 16u + local_index;
    atomicStore(&boundary_tile_data[solid_base], solid_word);
}

/// Precomputes Akinci weights for the cells belonging to one listed tile
@compute @workgroup_size(64)
fn boundary_weights(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    let tile_count: u32 = uniforms.region_width * uniforms.region_height;
    let listed_count: u32 = atomicLoad(&control[2]);
    if workgroup.x >= listed_count { return; }
    let tile_index: u32 = atomicLoad(&boundary_tile_data[tile_count + workgroup.x]);
    let tile: vec2<u32> = vec2<u32>(
        tile_index % uniforms.region_width,
        tile_index / uniforms.region_width,
    );
    let local_cell: vec2<u32> = vec2<u32>(local_index % 8u, local_index / 8u);
    let cell: vec2<i32> = vec2<i32>(tile * 8u + local_cell);
    let logical_index: u32 = u32(cell.y) * uniforms.cell_width + u32(cell.x);
    if !solid_cell(cell) {
        boundary_psi[logical_index] = vec4<f32>(0.0);
        return;
    }
    let center: vec2<f32> = vec2<f32>(cell) + 0.5;
    var boundary_density: f32 = 0.0;
    for (var y: i32 = -2; y <= 2; y++) {
        for (var x: i32 = -2; x <= 2; x++) {
            let neighbor: vec2<i32> = cell + vec2<i32>(x, y);
            if !solid_cell(neighbor) { continue; }
            let difference: vec2<f32> = center - (vec2<f32>(neighbor) + 0.5);
            let distance_squared: f32 = dot(difference, difference);
            if distance_squared >= KERNEL_RADIUS_SQUARED { continue; }
            boundary_density += poly6_squared(distance_squared);
        }
    }
    boundary_psi[logical_index] = vec4<f32>(
        uniforms.rho0 / max(boundary_density, 0.000001),
        0.0,
        0.0,
        0.0,
    );
}

/// Writes each listed solid cell's moving-boundary velocity beside its Akinci weight
@compute @workgroup_size(64)
fn boundary_motion(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    let tile_count: u32 = uniforms.region_width * uniforms.region_height;
    let listed_count: u32 = atomicLoad(&control[2]);
    if workgroup.x >= listed_count { return; }
    let tile_index: u32 = atomicLoad(&boundary_tile_data[tile_count + workgroup.x]);
    let tile: vec2<u32> = vec2<u32>(
        tile_index % uniforms.region_width,
        tile_index / uniforms.region_width,
    );
    let cell: vec2<i32> = vec2<i32>(
        tile * 8u + vec2<u32>(local_index % 8u, local_index / 8u),
    );
    let logical_index: u32 = u32(cell.y) * uniforms.cell_width + u32(cell.x);
    let physical_index: u32 = cell_index(cell);
    var velocity: vec2<f32> = vec2<f32>(0.0);
    var rigid_body: u32 = 0u;
    if solid_cell(cell) {
        let material_identifier: u32 = material_identifiers[physical_index];
        if material_identifier >> 30u == 1u {
            velocity = unpack_cell_velocity(cell_kinematics[physical_index]);
        } else {
            rigid_body = cell_rigid_body(physical_index);
            if rigid_body != 0u {
                velocity = rigid_body_velocity(rigid_body - 1u, cell);
            }
        }
    }
    boundary_psi[logical_index] = vec4<f32>(
        boundary_psi[logical_index].x,
        velocity,
        f32(rigid_body),
    );
}

/// Computes one cell-corner node's weighted boundary density and gradient
fn compute_boundary_node(node: vec2<u32>) -> vec4<f32> {
    let position: vec2<f32> = vec2<f32>(node);
    var density: f32 = 0.0;
    var gradient: vec2<f32> = vec2<f32>(0.0);
    var moving_gradient: f32 = 0.0;
    for (var y: i32 = -2; y <= 1; y++) {
        for (var x: i32 = -2; x <= 1; x++) {
            let cell: vec2<i32> = vec2<i32>(node) + vec2<i32>(x, y);
            if !solid_cell(cell) { continue; }
            let difference: vec2<f32> = position - (vec2<f32>(cell) + 0.5);
            let distance_squared: f32 = dot(difference, difference);
            if distance_squared >= KERNEL_RADIUS_SQUARED { continue; }
            let boundary_cell: vec4<f32> =
                boundary_psi[u32(cell.y) * uniforms.cell_width + u32(cell.x)];
            let weight: f32 = boundary_cell.x;
            let boundary_gradient: vec2<f32> = spiky_gradient(difference, distance_squared);
            density += weight * poly6_squared(distance_squared);
            gradient += weight * boundary_gradient;
            moving_gradient += weight * dot(boundary_cell.yz, boundary_gradient);
        }
    }
    return vec4<f32>(density, gradient, moving_gradient);
}

/// Precomputes the corner nodes belonging to each listed tile
@compute @workgroup_size(64)
fn boundary_nodes(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    let tile_count: u32 = uniforms.region_width * uniforms.region_height;
    let listed_count: u32 = atomicLoad(&control[2]);
    if workgroup.x >= listed_count { return; }
    let tile_index: u32 = atomicLoad(&boundary_tile_data[tile_count + workgroup.x]);
    let tile: vec2<u32> = vec2<u32>(
        tile_index % uniforms.region_width,
        tile_index / uniforms.region_width,
    );
    let local_node: vec2<u32> = vec2<u32>(local_index % 8u, local_index / 8u);
    let node: vec2<u32> = tile * 8u + local_node;
    let node_width: u32 = uniforms.cell_width + 1u;
    boundary_node_map[node.y * node_width + node.x] = compute_boundary_node(node);
    if tile.x + 1u == uniforms.region_width && local_node.x == 7u {
        let edge_node: vec2<u32> = vec2<u32>(uniforms.cell_width, node.y);
        boundary_node_map[edge_node.y * node_width + edge_node.x] =
            compute_boundary_node(edge_node);
    }
    if tile.y + 1u == uniforms.region_height && local_node.y == 7u {
        let edge_node: vec2<u32> = vec2<u32>(node.x, uniforms.cell_height);
        boundary_node_map[edge_node.y * node_width + edge_node.x] =
            compute_boundary_node(edge_node);
    }
    if tile.x + 1u == uniforms.region_width
        && tile.y + 1u == uniforms.region_height
        && local_node.x == 7u
        && local_node.y == 7u
    {
        let corner: vec2<u32> = vec2<u32>(uniforms.cell_width, uniforms.cell_height);
        boundary_node_map[corner.y * node_width + corner.x] = compute_boundary_node(corner);
    }
}

/// Predicts particle positions under gravity and sweeps them against solid cell squares
@compute @workgroup_size(PARTICLE_WORKGROUP_SIZE)
fn predict(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let particle_count: u32 = atomicLoad(&control[0]);
    if invocation.x >= particle_count { return; }
    var particle: FluidParticle = load_particle(invocation.x, uniforms.state_side);
    // the first substep records where the tick started, which the sleep test measures from
    if uniforms.substep_index == 0u {
        particle.tick_start_x = particle.state.x;
        particle.tick_start_y = particle.state.y;
    }
    var pressure_velocity_change: vec2<f32> = vec2<f32>(0.0);
    if uniforms.substep_index == 0u {
        let cell: vec2<i32> = vec2<i32>(floor(particle.state.xy));
        if cell_inside(cell) {
            let raster_index: u32 = cell_index(cell) * 4u;
            let mass: f32 = f32(bitcast<i32>(atomicLoad(&fluid_raster[raster_index])))
                / uniforms.raster_scale;
            if mass > 0.0 {
                pressure_velocity_change = pressure_push(cell) * PRESSURE_IMPULSE / mass;
                particle.state = vec4<f32>(
                    particle.state.xy,
                    particle.state.zw + pressure_velocity_change,
                );
                if any(pressure_velocity_change != vec2<f32>(0.0)) {
                    request_tile_wake(particle_tile_index(particle.state.xy));
                }
            }
        }
    }
    if position_is_sleeping(particle.state.xy)
        && all(pressure_velocity_change == vec2<f32>(0.0))
    {
        store_predicted(invocation.x, 0u, particle.state.xy);
        var frozen: FluidParticle = particle;
        store_particle(invocation.x, uniforms.state_side, frozen);
        return;
    }
    let gravity: vec2<f32> = vec2<f32>(0.0, -uniforms.gravity_substep);
    var predicted_velocity: vec2<f32> = particle.state.zw + gravity;
    let displacement: vec2<f32> = predicted_velocity / uniforms.substep_count;
    let sweep_steps: u32 = clamp(u32(ceil(length(displacement) / 0.5)), 1u, 8u);
    var position: vec2<f32> = particle.state.xy;
    for (var step: u32 = 0u; step < sweep_steps; step++) {
        let candidate: vec2<f32> = position
            + predicted_velocity / (uniforms.substep_count * f32(sweep_steps));
        let projection: FluidProjectionResult = project_from_solids(candidate);
        let projection_displacement: vec2<f32> = projection.position - candidate;
        if dot(projection_displacement, projection_displacement) > 0.0000000001 {
            let wall_normal: vec2<f32> = normalize(projection_displacement);
            var relative_velocity: vec2<f32> = predicted_velocity - projection.wall_velocity;
            let into_wall: f32 = dot(relative_velocity, wall_normal);
            if into_wall < 0.0 { relative_velocity -= into_wall * wall_normal; }
            let normal_velocity: vec2<f32> = dot(relative_velocity, wall_normal) * wall_normal;
            let tangent_velocity: vec2<f32> = relative_velocity - normal_velocity;
            predicted_velocity = projection.wall_velocity + normal_velocity
                + tangent_velocity * (1.0 - FLUID_BOUNDARY_FRICTION);
        }
        position = projection.position;
    }
    store_predicted(invocation.x, 0u, position);
    var predicted_particle: FluidParticle = particle;
    store_particle(invocation.x, uniforms.state_side, predicted_particle);
    if uniforms.predicted_side == 0u && position_is_sleeping(position) {
        request_tile_wake(particle_tile_index(position));
    }
}

/// Clears dense bin counts and scatter cursors before the counting-sort passes
@compute @workgroup_size(256)
fn clear_bins(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if invocation.x >= uniforms.bin_count { return; }
    atomicStore(&bin_data[invocation.x], 0u);
    atomicStore(&bin_data[2u * uniforms.bin_count + invocation.x], 0u);
}

/// Counts live particles in the bin of their predicted position
@compute @workgroup_size(PARTICLE_WORKGROUP_SIZE)
fn count_bins(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let particle_count: u32 = atomicLoad(&control[0]);
    if invocation.x >= particle_count { return; }
    let position: vec2<f32> = load_predicted(invocation.x, uniforms.predicted_side);
    atomicAdd(&bin_data[bin_index(position)], 1u);
}

/// Inclusive prefix sums of the bin counts scanned by one workgroup
var<workgroup> scan_values: array<u32, 256>;

/// Performs one 256-bin inclusive scan and writes each bin's exclusive local offset
@compute @workgroup_size(256)
fn scan_bins(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    let bin: u32 = workgroup.x * 256u + local_index;
    var value: u32 = 0u;
    if bin < uniforms.bin_count { value = atomicLoad(&bin_data[bin]); }
    scan_values[local_index] = value;
    workgroupBarrier();
    for (var offset: u32 = 1u; offset < 256u; offset *= 2u) {
        var previous: u32 = 0u;
        if local_index >= offset { previous = scan_values[local_index - offset]; }
        workgroupBarrier();
        scan_values[local_index] += previous;
        workgroupBarrier();
    }
    if bin < uniforms.bin_count {
        atomicStore(&bin_data[uniforms.bin_count + bin], scan_values[local_index] - value);
    }
    if local_index == 255u {
        atomicStore(
            &bin_data[3u * uniforms.bin_count + workgroup.x],
            scan_values[local_index],
        );
    }
}

/// Scans 256 block totals per workgroup and writes local block offsets and chunk totals
@compute @workgroup_size(256)
fn scan_blocks(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    let block: u32 = workgroup.x * 256u + local_index;
    let block_total_base: u32 = 3u * uniforms.bin_count;
    var value: u32 = 0u;
    if block < uniforms.scan_block_count {
        value = atomicLoad(&bin_data[block_total_base + block]);
    }
    scan_values[local_index] = value;
    workgroupBarrier();
    for (var offset: u32 = 1u; offset < 256u; offset *= 2u) {
        var previous: u32 = 0u;
        if local_index >= offset { previous = scan_values[local_index - offset]; }
        workgroupBarrier();
        scan_values[local_index] += previous;
        workgroupBarrier();
    }
    if block < uniforms.scan_block_count {
        atomicStore(
            &bin_data[block_total_base + uniforms.scan_block_count + block],
            scan_values[local_index] - value,
        );
    }
    if local_index == 255u || block + 1u == uniforms.scan_block_count {
        let chunk_total_base: u32 = block_total_base + 2u * uniforms.scan_block_count;
        atomicStore(&bin_data[chunk_total_base + workgroup.x], scan_values[local_index]);
    }
}

/// Scans chunk totals, with no more than 256 serial values for up to 65536 blocks
@compute @workgroup_size(256)
fn scan_block_prefix(@builtin(local_invocation_index) local_index: u32) {
    let chunk_count: u32 = (uniforms.scan_block_count + 255u) / 256u;
    let block_total_base: u32 = 3u * uniforms.bin_count;
    let chunk_total_base: u32 = block_total_base + 2u * uniforms.scan_block_count;
    let chunk_prefix_base: u32 = chunk_total_base + chunk_count;
    if chunk_count <= 256u {
        var value: u32 = 0u;
        if local_index < chunk_count {
            value = atomicLoad(&bin_data[chunk_total_base + local_index]);
        }
        scan_values[local_index] = value;
        workgroupBarrier();
        for (var offset: u32 = 1u; offset < 256u; offset *= 2u) {
            var previous: u32 = 0u;
            if local_index >= offset { previous = scan_values[local_index - offset]; }
            workgroupBarrier();
            scan_values[local_index] += previous;
            workgroupBarrier();
        }
        if local_index < chunk_count {
            atomicStore(
                &bin_data[chunk_prefix_base + local_index],
                scan_values[local_index] - value,
            );
        }
    } else if local_index == 0u {
        var prefix: u32 = 0u;
        for (var chunk: u32 = 0u; chunk < chunk_count; chunk++) {
            atomicStore(&bin_data[chunk_prefix_base + chunk], prefix);
            prefix += atomicLoad(&bin_data[chunk_total_base + chunk]);
        }
    }
}

/// Adds each tile's exclusive prefix to its bins, producing dense global bin offsets
@compute @workgroup_size(256)
fn add_block_prefix(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if invocation.x >= uniforms.bin_count { return; }
    let block: u32 = invocation.x / 256u;
    let chunk: u32 = block / 256u;
    let block_prefix: u32 = atomicLoad(
        &bin_data[3u * uniforms.bin_count + uniforms.scan_block_count + block],
    );
    let chunk_count: u32 = (uniforms.scan_block_count + 255u) / 256u;
    let chunk_prefix: u32 = atomicLoad(
        &bin_data[3u * uniforms.bin_count + 2u * uniforms.scan_block_count + chunk_count + chunk],
    );
    let prefix: u32 = block_prefix + chunk_prefix;
    atomicStore(
        &bin_data[uniforms.bin_count + invocation.x],
        atomicLoad(&bin_data[uniforms.bin_count + invocation.x]) + prefix,
    );
}

/// Scatters particle state, predicted positions, and compact neighbor data into bin order
@compute @workgroup_size(PARTICLE_WORKGROUP_SIZE)
fn scatter(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let particle_count: u32 = atomicLoad(&control[0]);
    if invocation.x >= particle_count { return; }
    let position: vec2<f32> = load_predicted(invocation.x, uniforms.predicted_side);
    let bin: u32 = bin_index(position);
    let local_offset: u32 = atomicAdd(&bin_data[2u * uniforms.bin_count + bin], 1u);
    let first: u32 = atomicLoad(&bin_data[uniforms.bin_count + bin]);
    let sorted_index: u32 = first + local_offset;
    let particle: FluidParticle = load_particle(invocation.x, uniforms.state_side);
    let sorted_state_side: u32 = 1u - uniforms.state_side;
    store_particle(sorted_index, sorted_state_side, particle);
    store_predicted(sorted_index, 1u - uniforms.predicted_side, position);
    let material: vec4<f32> =
        fluid_materials[(particle.material_identifier & 0x3fffffffu) * 2u];
    store_neighbor_data(
        sorted_index,
        particle.state.xy,
        material.z,
        material.w,
        position_is_sleeping(position),
    );
}


/// Computes unilateral density lambdas from fluid neighbors and weighted solid boundaries
@compute @workgroup_size(PARTICLE_WORKGROUP_SIZE)
fn lambda(
    @builtin(global_invocation_id) invocation: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    atomicStore(&sleep_reduction_tiles[local_index], SLEEP_REDUCTION_EMPTY);
    atomicStore(&sleep_reduction_values[local_index], 0u);
    workgroupBarrier();
    let particle_count: u32 = atomicLoad(&control[0]);
    if invocation.x < particle_count {
        let position: vec2<f32> = load_predicted(invocation.x, uniforms.predicted_side);
        if neighbor_is_sleeping(invocation.x) {
            lambdas[invocation.x] = 0.0;
        } else {
            var density: f32 = poly6_squared(0.0);
            var gradient_sum: vec2<f32> = vec2<f32>(0.0);
            var gradient_norm_sum: f32 = 0.0;
            let rows: vec2<i32> = neighbor_rows(position);
            for (var bin_y: i32 = rows.x; bin_y <= rows.y; bin_y++) {
                let row: vec2<u32> = neighbor_row_range(position, bin_y, particle_count);
                for (var neighbor_index: u32 = row.x; neighbor_index < row.y;
                     neighbor_index++) {
                    if neighbor_index == invocation.x { continue; }
                    let neighbor_position: vec2<f32> =
                        load_predicted(neighbor_index, uniforms.predicted_side);
                    let difference: vec2<f32> = position - neighbor_position;
                    let distance_squared: f32 = dot(difference, difference);
                    if distance_squared >= KERNEL_RADIUS_SQUARED { continue; }
                    density += poly6_squared(distance_squared);
                    let gradient: vec2<f32> = spiky_gradient(difference, distance_squared)
                        / uniforms.rho0;
                    gradient_sum += gradient;
                    if !neighbor_is_sleeping(neighbor_index) {
                        gradient_norm_sum += dot(gradient, gradient);
                    }
                }
            }
            let boundary: vec4<f32> = sample_boundary(position);
            density += boundary.x;
            gradient_sum += boundary.yz / uniforms.rho0;
            let constraint: f32 = density / uniforms.rho0 - 1.0;
            let tile_index: u32 = particle_tile_index(position);
            let reduction_slot: u32 = sleep_reduction_slot(tile_index);
            atomicMax(
                &sleep_reduction_values[reduction_slot],
                bitcast<u32>(max(constraint, 0.0)),
            );
            let denominator: f32 = dot(gradient_sum, gradient_sum)
                + gradient_norm_sum + uniforms.relaxation;
            lambdas[invocation.x] = -constraint / denominator;
        }
    }
    workgroupBarrier();
    let tile_index: u32 = atomicLoad(&sleep_reduction_tiles[local_index]);
    if tile_index != SLEEP_REDUCTION_EMPTY {
        let tile_base: u32 = sleep_tile_offset(tile_index);
        atomicMax(
            &control[tile_base + 1u],
            atomicLoad(&sleep_reduction_values[local_index]),
        );
    }
}

/// Applies one Jacobi position correction and projects the result out of solid cell squares
@compute @workgroup_size(PARTICLE_WORKGROUP_SIZE)
fn correct(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let particle_count: u32 = atomicLoad(&control[0]);
    if invocation.x >= particle_count { return; }
    let position: vec2<f32> = load_predicted(invocation.x, uniforms.predicted_side);
    if neighbor_is_sleeping(invocation.x) {
        store_predicted(invocation.x, 1u - uniforms.predicted_side, position);
        return;
    }
    let lambda_i: f32 = lambdas[invocation.x];
    let cohesion_i: f32 = load_neighbor_cohesion(invocation.x);
    var correction: vec2<f32> = vec2<f32>(0.0);
    let rows: vec2<i32> = neighbor_rows(position);
    for (var bin_y: i32 = rows.x; bin_y <= rows.y; bin_y++) {
        let row: vec2<u32> = neighbor_row_range(position, bin_y, particle_count);
        for (var neighbor_index: u32 = row.x; neighbor_index < row.y;
             neighbor_index++) {
            if neighbor_index == invocation.x { continue; }
            let neighbor_position: vec2<f32> =
                load_predicted(neighbor_index, uniforms.predicted_side);
            let difference: vec2<f32> = position - neighbor_position;
            let distance_squared: f32 = dot(difference, difference);
            if distance_squared >= KERNEL_RADIUS_SQUARED
                || distance_squared <= 0.0000000001
            { continue; }
            let distance: f32 = sqrt(distance_squared);
            let remaining: f32 = KERNEL_RADIUS - distance;
            let kernel_difference: f32 = KERNEL_RADIUS_SQUARED - distance_squared;
            let kernel_difference_cubed: f32 =
                kernel_difference * kernel_difference * kernel_difference;
            let kernel_ratio: f32 = POLY6_COEFFICIENT * kernel_difference_cubed
                * S_CORR_DELTA_Q_KERNEL_INVERSE;
            let kernel_ratio_squared: f32 = kernel_ratio * kernel_ratio;
            let kernel_ratio_fourth: f32 = kernel_ratio_squared * kernel_ratio_squared;
            let pressure_i: f32 = load_neighbor_pressure(invocation.x);
            let correction_kernel: f32 = -pressure_i * kernel_ratio_fourth;
            let pressure: f32 = lambda_i + lambdas[neighbor_index] + correction_kernel;
            let direction: vec2<f32> = difference / distance;
            let spiky_scale: f32 = SPIKY_COEFFICIENT * remaining * remaining;
            correction += pressure * spiky_scale * direction / uniforms.rho0;
            let cohesion_j: f32 = load_neighbor_cohesion(neighbor_index);
            let cohesion: f32 = (cohesion_i + cohesion_j) * 0.5;
            correction -= direction * cohesion * uniforms.cohesion_scale
                * cohesion_spline(distance, remaining, distance_squared);
        }
    }
    let boundary: vec4<f32> = sample_boundary(position);
    correction += lambda_i * boundary.yz / uniforms.rho0;
    if !fluid_vector_is_finite(correction) {
        correction = vec2<f32>(0.0);
    }
    let correction_length: f32 = length(correction);
    if correction_length > MAX_POSITION_CORRECTION {
        let scale: f32 = MAX_POSITION_CORRECTION / correction_length;
        correction *= scale;
    }
    let candidate: vec2<f32> = position + correction;
    let projection: FluidProjectionResult = project_from_solids(candidate);
    store_predicted(invocation.x, 1u - uniforms.predicted_side, projection.position);
}

/// Commits corrected positions and reconstructs substep velocities
@compute @workgroup_size(PARTICLE_WORKGROUP_SIZE)
fn commit(
    @builtin(global_invocation_id) invocation: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    atomicStore(&sleep_reduction_tiles[local_index], SLEEP_REDUCTION_EMPTY);
    workgroupBarrier();
    let particle_count: u32 = atomicLoad(&control[0]);
    if invocation.x < particle_count {
        let particle: FluidParticle = load_particle(invocation.x, uniforms.state_side);
        let output_side: u32 = 1u - uniforms.state_side;
        let old_position: vec2<f32> = particle.state.xy;
        var position: vec2<f32> = load_predicted(invocation.x, uniforms.predicted_side);
        let old_tile_index: u32 = particle_tile_index(old_position);
        var tile_index: u32 = old_tile_index;
        if tile_is_sleeping(old_tile_index) {
            var frozen: FluidParticle = particle;
            frozen.state = vec4<f32>(old_position, vec2<f32>(0.0));
            store_particle(invocation.x, output_side, frozen);
        } else {
            var velocity: vec2<f32> = (position - old_position)
                * uniforms.substep_count;
            let velocity_length: f32 = length(velocity);
            if velocity_length > uniforms.velocity_maximum {
                velocity *= uniforms.velocity_maximum / velocity_length;
            }
            if !fluid_vector_is_finite(position) { position = old_position; }
            if !fluid_vector_is_finite(velocity) { velocity = vec2<f32>(0.0); }
            var committed: FluidParticle = particle;
            committed.state = vec4<f32>(position, velocity);
            store_particle(invocation.x, output_side, committed);
            tile_index = particle_tile_index(position);
        }
        _ = sleep_reduction_slot(tile_index);
    }
    workgroupBarrier();
    let tile_index: u32 = atomicLoad(&sleep_reduction_tiles[local_index]);
    if tile_index != SLEEP_REDUCTION_EMPTY {
        atomicOr(&control[sleep_tile_offset(tile_index) + 4u], SLEEP_TILE_HAS_PARTICLE);
    }
}

/// Calculates once-per-tick XSPH from final committed particle states
@compute @workgroup_size(PARTICLE_WORKGROUP_SIZE)
fn xsph(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let particle_count: u32 = atomicLoad(&control[0]);
    if invocation.x >= particle_count { return; }
    let particle: FluidParticle = load_particle(invocation.x, uniforms.state_side);
    var velocity_change: vec2<f32> = vec2<f32>(0.0);
    var weight_sum: f32 = 0.0;
    let position: vec2<f32> = particle.state.xy;
    let rows: vec2<i32> = neighbor_rows(position);
    for (var bin_y: i32 = rows.x; bin_y <= rows.y; bin_y++) {
        let row: vec2<u32> = neighbor_row_range(position, bin_y, particle_count);
        for (var neighbor_index: u32 = row.x; neighbor_index < row.y; neighbor_index++) {
            if neighbor_index == invocation.x { continue; }
            let neighbor: FluidParticle = load_particle(neighbor_index, uniforms.state_side);
            let difference: vec2<f32> = position - neighbor.state.xy;
            let distance_squared: f32 = dot(difference, difference);
            if distance_squared >= KERNEL_RADIUS_SQUARED { continue; }
            let weight: f32 = poly6_squared(distance_squared);
            velocity_change += (neighbor.state.zw - particle.state.zw) * weight;
            weight_sum += weight;
        }
    }
    let material: vec4<f32> =
        fluid_materials[(particle.material_identifier & 0x3fffffffu) * 2u];
    predicted_a[invocation.x] = particle.state.zw
        + material.y * velocity_change / max(weight_sum, 0.000001);
}

/// Applies the calculated XSPH velocities after every neighbor read is complete
@compute @workgroup_size(PARTICLE_WORKGROUP_SIZE)
fn apply_xsph(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let particle_count: u32 = atomicLoad(&control[0]);
    if invocation.x >= particle_count { return; }
    let output_side: u32 = uniforms.state_side;
    var particle: FluidParticle = load_particle(invocation.x, uniforms.state_side);
    var velocity: vec2<f32> = predicted_a[invocation.x];
    if !fluid_vector_is_finite(velocity) { velocity = particle.state.zw; }
    let velocity_length: f32 = length(velocity);
    if velocity_length > uniforms.velocity_maximum {
        velocity *= uniforms.velocity_maximum / velocity_length;
    }
    // a small loss every tick drains leftover motion until its tile is calm enough to sleep
    velocity *= VELOCITY_DECAY;
    particle.state = vec4<f32>(particle.state.xy, velocity);
    store_particle(invocation.x, output_side, particle);
    // sleep and wake measure net displacement over the tick, which ignores the gravity kick
    // each substep's prediction carries and its correction removes
    let tick_start: vec2<f32> = vec2<f32>(particle.tick_start_x, particle.tick_start_y);
    let tile_index: u32 = particle_tile_index(particle.state.xy);
    atomicMax(
        &control[sleep_tile_offset(tile_index)],
        bitcast<u32>(length(particle.state.xy - tick_start)),
    );
}

/// Restores actor-normal velocity after the ordinary speed clamp and records its reaction
@compute @workgroup_size(PARTICLE_WORKGROUP_SIZE)
fn commit_actor_velocity(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let particle_count: u32 = atomicLoad(&control[0]);
    if invocation.x >= particle_count { return; }
    let output_side: u32 = 1u - uniforms.state_side;
    var particle: FluidParticle = load_particle(invocation.x, output_side);
    var velocity: vec2<f32> = particle.state.zw;
    for (var actor_index: u32 = 0u; actor_index < uniforms.actor_count; actor_index++) {
        var actor: FluidActor = fluid_actors[actor_index];
        let offset: vec2<f32> = actor.velocity * uniforms.actor_time;
        let margin: vec2<f32> = vec2<f32>(0.502);
        if any(particle.state.xy < actor.bounds.xy + offset - margin)
            || any(particle.state.xy > actor.bounds.zw + offset + margin)
        { continue; }
        actor.center += offset;
        let projection: vec4<f32> = project_from_actor(particle.state.xy, actor);
        let normal: vec2<f32> = projection.zw;
        if dot(normal, normal) <= 0.5 { continue; }
        let into_actor: f32 = dot(velocity - actor.velocity, normal);
        if into_actor >= 0.0 { continue; }
        let velocity_change: vec2<f32> = -into_actor * normal;
        velocity += velocity_change;
        add_actor_reaction(
            actor_index,
            particle.material_identifier,
            velocity_change / uniforms.substep_count,
        );
    }
    particle.state.z = velocity.x;
    particle.state.w = velocity.y;
    store_particle(invocation.x, output_side, particle);
}
