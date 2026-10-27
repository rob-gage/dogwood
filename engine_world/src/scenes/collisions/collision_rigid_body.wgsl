// Copyright Rob Gage 2026

struct CollisionRigidBodyUniforms {
    region_width: u32,
    region_height: u32,
    ring_offset_x: u32,
    ring_offset_y: u32,
    region_origin: vec2<i32>,
    item_offset: u32,
    item_count: u32,
    mode: u32,
    padding_0: u32,
    padding_1: u32,
    padding_2: u32,
};

struct CollisionRigidBodyCellularTile {
    rigid_body: u32,
    padding_0: u32,
    tile: vec2<i32>,
    cells: vec2<u32>,
    previous_cells: vec2<u32>,
};

struct CollisionRigidBody {
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

@group(0) @binding(0)
var<uniform> uniforms: CollisionRigidBodyUniforms;

@group(0) @binding(1)
var<storage, read_write> material_identifiers: array<u32>;

@group(0) @binding(2)
var<storage, read_write> colors: array<vec2<u32>>;

@group(0) @binding(3)
var<storage, read_write> kinematics: array<vec2<u32>>;

@group(0) @binding(4)
var<storage, read_write> awake: array<atomic<u32>>;

@group(0) @binding(5)
var<storage, read_write> rigid_body_cells: array<atomic<u32>>;

@group(0) @binding(6)
var<storage, read> rigid_body_cellular_tile_cells: array<vec4<u32>>;

@group(0) @binding(7)
var<storage, read_write> rigid_body_cellular_tiles: array<CollisionRigidBodyCellularTile>;

@group(0) @binding(8)
var<storage, read> rigid_bodies: array<CollisionRigidBody>;

@group(0) @binding(9)
var<storage, read> items: array<u32>;

@group(0) @binding(10)
var<storage, read> cell_writes: array<vec4<u32>>;

@group(0) @binding(11)
var<storage, read_write> cell_reads: array<vec4<u32>>;

@group(0) @binding(12)
var<storage, read_write> changed_cells: array<atomic<u32>>;

@group(0) @binding(13)
var<storage, read_write> rigid_body_cell_states: array<vec4<u32>>;

@group(0) @binding(14)
var<storage, read_write> gas: array<u32>;

@group(0) @binding(15)
var<storage, read_write> matter: array<vec2<u32>>;

@group(0) @binding(16)
var<storage, read_write> thermal_awake: array<atomic<u32>>;

/// Release mode emptying cells a rigid body leaves, or turning them into their cells' debris
const MODE_CLEAR: u32 = 0u;

/// Release mode leaving a rigid body's cells in the grid as static cells
const MODE_RETURN: u32 = 1u;

/// Release mode reporting cells changed under a rigid body without moving it
const MODE_CHANGE: u32 = 2u;

/// Awake buffer value of a tile whose dynamic cells, and those of its neighbors, must be simulated
const TILE_AWAKE: u32 = 3u;

/// Thermal awake buffer value of a tile whose temperatures must be simulated
const TILE_THERMAL_AWAKE: u32 = 1u;

/// Fixed-point units per cell per tick of packed velocities
const VELOCITY_SCALE: f32 = 1024.0;

/// Largest speed of a packed velocity in cells per tick
const SPEED_MAXIMUM: f32 = 8.0;

/// Returns the buffer index of a tile position inside the active region
fn tile_index(tile: vec2<u32>) -> u32 {
    let physical_x: u32 = (tile.x + uniforms.ring_offset_x) % uniforms.region_width;
    let physical_y: u32 = (tile.y + uniforms.ring_offset_y) % uniforms.region_height;
    return physical_y * uniforms.region_width + physical_x;
}

/// Returns the buffer index of a cell position inside the active region
fn cell_index(cell: vec2<i32>) -> u32 {
    let position: vec2<u32> = vec2<u32>(cell);
    return tile_index(position / 8u) * 64u + (position.y % 8u) * 8u + position.x % 8u;
}

/// Returns whether a material identifier is a dynamic cellular material
fn is_dynamic(material_identifier: u32) -> bool { return material_identifier >> 30u == 1u; }

/// Returns whether a material identifier is a static cellular material other than empty
fn is_static(material_identifier: u32) -> bool {
    return material_identifier != 0u && material_identifier >> 30u == 0u;
}

/// Rotates a vector counterclockwise by a cosine and sine
fn rotate(rotation: vec2<f32>, vector: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(
        rotation.x * vector.x - rotation.y * vector.y,
        rotation.y * vector.x + rotation.x * vector.y,
    );
}

/// Packs a velocity in cells per tick as a resting cell's kinematics
fn pack_velocity(velocity: vec2<f32>) -> vec2<u32> {
    let packed: vec2<i32> = vec2<i32>(round(
        clamp(velocity, vec2<f32>(-SPEED_MAXIMUM), vec2<f32>(SPEED_MAXIMUM)) * VELOCITY_SCALE,
    ));
    return vec2<u32>((bitcast<u32>(packed.x) & 0xffffu) | (bitcast<u32>(packed.y) << 16u), 0u);
}

/// Returns the velocity in cells per tick of a rigid body at a cell position
fn point_velocity(rigid_body: CollisionRigidBody, cell: vec2<i32>) -> vec2<f32> {
    let arm: vec2<f32> = vec2<f32>(uniforms.region_origin + cell) + 0.5 - rigid_body.center;
    return rigid_body.velocity + rigid_body.angular_velocity * vec2<f32>(-arm.y, arm.x);
}

/// Returns the inclusive cell bounds inside the active region of some of one rigid body tile's
/// cells placed at a position and rotation
fn cellular_tile_bounds(
    cellular_tile: CollisionRigidBodyCellularTile,
    cells: vec2<u32>,
    position: vec2<f32>,
    rotation: vec2<f32>,
) -> vec4<i32> {
    // bound only the occupied rows and columns so thin bodies test few cells
    var occupied_minimum: vec2<i32> = vec2<i32>(8);
    var occupied_maximum: vec2<i32> = vec2<i32>(-1);
    for (var cell: u32 = 0u; cell < 64u; cell++) {
        if (cells[cell / 32u] & (1u << (cell % 32u))) == 0u { continue; }
        let local: vec2<i32> = vec2<i32>(i32(cell % 8u), i32(cell / 8u));
        occupied_minimum = min(occupied_minimum, local);
        occupied_maximum = max(occupied_maximum, local);
    }
    let local_minimum: vec2<f32> = vec2<f32>(cellular_tile.tile * 8 + occupied_minimum);
    let local_maximum: vec2<f32> = vec2<f32>(cellular_tile.tile * 8 + occupied_maximum + 1);
    var minimum: vec2<f32> = vec2<f32>(1.0e30);
    var maximum: vec2<f32> = vec2<f32>(-1.0e30);
    for (var corner: u32 = 0u; corner < 4u; corner++) {
        let local: vec2<f32> = select(local_minimum, local_maximum, vec2<bool>(
            (corner & 1u) != 0u,
            (corner & 2u) != 0u,
        ));
        let point: vec2<f32> = position + rotate(rotation, local)
            - vec2<f32>(uniforms.region_origin);
        minimum = min(minimum, point);
        maximum = max(maximum, point);
    }
    let region_maximum: vec2<i32> =
        vec2<i32>(i32(uniforms.region_width), i32(uniforms.region_height)) * 8 - 1;
    return vec4<i32>(
        clamp(vec2<i32>(floor(minimum)), vec2<i32>(0), region_maximum),
        clamp(vec2<i32>(floor(maximum)), vec2<i32>(-1), region_maximum),
    );
}

/// Removes every cell of one rigid body tile from where it was last rasterized: clearing the cells
/// it leaves, leaving them as static cells, or reporting and detaching cells edited under it
@compute @workgroup_size(64)
fn release(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    if workgroup.x >= uniforms.item_count { return; }
    let cellular_tile_index: u32 = items[uniforms.item_offset + workgroup.x];
    let cellular_tile: CollisionRigidBodyCellularTile =
        rigid_body_cellular_tiles[cellular_tile_index];
    let rigid_body: CollisionRigidBody = rigid_bodies[cellular_tile.rigid_body];
    // the cells last rasterized are found where the body was last rasterized
    let bounds: vec4<i32> = cellular_tile_bounds(
        cellular_tile,
        cellular_tile.previous_cells,
        rigid_body.previous_position,
        rigid_body.previous_rotation,
    );
    let size: vec2<i32> = max(bounds.zw - bounds.xy + 1, vec2<i32>(0));
    let first: u32 = cellular_tile_index * 64u + 1u;
    for (var candidate: u32 = local_index; candidate < u32(size.x * size.y); candidate += 64u) {
        let cell: vec2<i32> = bounds.xy + vec2<i32>(
            i32(candidate % u32(size.x)),
            i32(candidate / u32(size.x)),
        );
        let index: u32 = cell_index(cell);
        let rigid_body_cell: u32 = atomicLoad(&rigid_body_cells[index]);
        if rigid_body_cell < first || rigid_body_cell >= first + 64u { continue; }
        let rigid_cell: vec4<u32> = rigid_body_cellular_tile_cells[rigid_body_cell - 1u];
        let material_identifier: u32 = material_identifiers[index];
        // dynamic cells covered by the body stay where they are
        if uniforms.mode == MODE_CLEAR {
            // cells whose material became debris or nothing take it moving with the body, and
            // the rest are emptied for wherever their body rasterizes next
            if is_static(material_identifier) {
                rigid_body_cell_states[rigid_body_cell - 1u] = vec4<u32>(
                    gas[index * 8u + 7u],
                    kinematics[index].y & 0xffffu,
                    matter[index],
                );
                // a cell left empty keeps its temperature for the gas that fills it
                if !is_dynamic(rigid_cell.x) { matter[index].y = 0u; }
                material_identifiers[index] = select(0u, rigid_cell.x, !is_static(rigid_cell.x));
                colors[index] = select(vec2<u32>(0u), rigid_cell.yz, is_dynamic(rigid_cell.x));
                gas[index * 8u + 7u] = 0u;
                kinematics[index] = select(
                    vec2<u32>(0u),
                    pack_velocity(point_velocity(rigid_body, cell)),
                    is_dynamic(rigid_cell.x),
                );
            }
        } else if uniforms.mode == MODE_RETURN {
            // the body's cells stay as static cells at rest
            if is_static(material_identifier) {
                rigid_body_cell_states[rigid_body_cell - 1u] = vec4<u32>(
                    gas[index * 8u + 7u],
                    kinematics[index].y & 0xffffu,
                    matter[index],
                );
                kinematics[index] = vec2<u32>(0u, kinematics[index].y & 0xffffu);
            }
        } else {
            if material_identifier == rigid_cell.x || is_dynamic(material_identifier) {
                continue;
            }
            // a full report leaves the cell attached so a later report finds it again
            let change: u32 = atomicAdd(&changed_cells[0], 1u);
            if change + 1u >= arrayLength(&changed_cells) { continue; }
            atomicStore(&changed_cells[change + 1u], rigid_body_cell - 1u);
        }
        atomicStore(&rigid_body_cells[index], 0u);
        atomicOr(&awake[tile_index(vec2<u32>(cell) / 8u)], TILE_AWAKE);
    }
}

/// Rasterizes one rigid body tile at its position: every cell whose center lies in one of the
/// tile's occupied cells takes that cell's material, color, velocity, and state, except cells of
/// other static matter, and dynamic cells there stay covered until they escape
@compute @workgroup_size(64)
fn rasterize(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    if workgroup.x >= uniforms.item_count { return; }
    let cellular_tile_index: u32 = items[uniforms.item_offset + workgroup.x];
    let cellular_tile: CollisionRigidBodyCellularTile =
        rigid_body_cellular_tiles[cellular_tile_index];
    let rigid_body: CollisionRigidBody = rigid_bodies[cellular_tile.rigid_body];
    let bounds: vec4<i32> = cellular_tile_bounds(
        cellular_tile,
        cellular_tile.cells,
        rigid_body.position,
        rigid_body.rotation,
    );
    let size: vec2<i32> = max(bounds.zw - bounds.xy + 1, vec2<i32>(0));
    for (var candidate: u32 = local_index; candidate < u32(size.x * size.y); candidate += 64u) {
        let cell: vec2<i32> = bounds.xy + vec2<i32>(
            i32(candidate % u32(size.x)),
            i32(candidate / u32(size.x)),
        );
        // the body cell holding this cell's center owns it, so cells never overlap or gap
        let center: vec2<f32> = vec2<f32>(uniforms.region_origin + cell) + 0.5;
        let rotation: vec2<f32> = vec2<f32>(rigid_body.rotation.x, -rigid_body.rotation.y);
        let local: vec2<i32> = vec2<i32>(floor(rotate(rotation, center - rigid_body.position)));
        let tile_cell: vec2<i32> = local - cellular_tile.tile * 8;
        if any(tile_cell < vec2<i32>(0)) || any(tile_cell >= vec2<i32>(8)) { continue; }
        let bit: u32 = u32(tile_cell.y * 8 + tile_cell.x);
        if (cellular_tile.cells[bit / 32u] & (1u << (bit % 32u))) == 0u { continue; }
        let index: u32 = cell_index(cell);
        let material_identifier: u32 = material_identifiers[index];
        // other static matter keeps its cell, and the first body to claim a cell owns it
        if is_static(material_identifier) { continue; }
        let rigid_body_cell: u32 = cellular_tile_index * 64u + bit + 1u;
        var is_claimed: bool = false;
        loop {
            let exchange = atomicCompareExchangeWeak(&rigid_body_cells[index], 0u, rigid_body_cell);
            if exchange.exchanged { is_claimed = true; break; }
            if exchange.old_value != 0u { break; }
        }
        if !is_claimed { continue; }
        if material_identifier == 0u {
            let rigid_cell: vec4<u32> = rigid_body_cellular_tile_cells[rigid_body_cell - 1u];
            material_identifiers[index] = rigid_cell.x;
            colors[index] = rigid_cell.yz;
            kinematics[index] = pack_velocity(point_velocity(rigid_body, cell));
            let state: vec4<u32> = rigid_body_cell_states[rigid_body_cell - 1u];
            gas[index * 8u + 7u] = state.x;
            kinematics[index].y = state.y;
            matter[index] = state.zw;
            atomicOr(&thermal_awake[tile_index(vec2<u32>(cell) / 8u)], TILE_THERMAL_AWAKE);
        }
        atomicOr(&awake[tile_index(vec2<u32>(cell) / 8u)], TILE_AWAKE);
    }
    // the next release finds the cells rasterized here once every invocation read the tile
    workgroupBarrier();
    if local_index == 0u {
        rigid_body_cellular_tiles[cellular_tile_index].previous_cells = cellular_tile.cells;
    }
}

/// Replaces one listed cell's material when it still holds the expected material and no rigid
/// body, keeping its color, temperature, and amount unless emptied and leaving it at rest
@compute @workgroup_size(64)
fn write_cells(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if invocation.x >= uniforms.item_count { return; }
    let cell_write: vec4<u32> = cell_writes[uniforms.item_offset + invocation.x];
    let cell: vec2<i32> = vec2<i32>(cell_write.xy);
    let index: u32 = cell_index(cell);
    if material_identifiers[index] != cell_write.w { return; }
    if atomicLoad(&rigid_body_cells[index]) != 0u { return; }
    material_identifiers[index] = cell_write.z;
    // an emptied cell keeps its temperature for the gas that fills it
    if cell_write.z == 0u {
        colors[index] = vec2<u32>(0u);
        matter[index].y = 0u;
    }
    gas[index * 8u + 7u] = 0u;
    kinematics[index] = vec2<u32>(0u);
    atomicOr(&awake[tile_index(vec2<u32>(cell) / 8u)], TILE_AWAKE);
}

/// Copies one listed cell's material, color, pressure, integrity, temperature, and amount for the
/// CPU
@compute @workgroup_size(64)
fn read_cells(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if invocation.x >= uniforms.item_count { return; }
    let cell_write: vec4<u32> = cell_writes[uniforms.item_offset + invocation.x];
    let index: u32 = cell_index(vec2<i32>(cell_write.xy));
    cell_reads[invocation.x * 2u] = vec4<u32>(
        material_identifiers[index],
        colors[index].x,
        gas[index * 8u + 7u],
        kinematics[index].y & 0xffffu,
    );
    cell_reads[invocation.x * 2u + 1u] = vec4<u32>(matter[index], colors[index].y, 0u);
}

/// Copies stored pressure, integrity, temperature, and amount from old rigid body cell indices to
/// newly allocated ones
@compute @workgroup_size(64)
fn copy_states(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if invocation.x >= uniforms.item_count { return; }
    let item_index: u32 = uniforms.item_offset + invocation.x * 2u;
    let old_cell: u32 = items[item_index];
    let new_cell: u32 = items[item_index + 1u];
    rigid_body_cell_states[new_cell] = rigid_body_cell_states[old_cell];
}
