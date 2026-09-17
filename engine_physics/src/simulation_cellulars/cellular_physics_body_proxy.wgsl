// Copyright Rob Gage 2026

#define_import_path compute::cellular_physics_body_proxy

#import utility::simulation_constants::{
    CELLS_PER_TILE,
    CELLS_PER_TILE_FLOAT,
    CELL_COUNT_PER_TILE,
    EMPTY_MATERIAL_IDENTIFIER,
    GAS_MATERIAL_FORM,
    CELLULAR_STATIC_MATERIAL_FORM,
    CELLULAR_DYNAMIC_MATERIAL_FORM,
    FLUID_MATERIAL_FORM,
    MATERIAL_IDENTIFIER_INDEX_MASK,
    INVALID_MATERIAL_DENSE_INDEX,
    FLUID_EDIT_ERASE,
    INVALID_PHYSICAL_CELL_INDEX,
    INVALID_CELLULAR_DYNAMIC_CLAIM_INDEX,
    INVALID_FLUID_PARTICLE_INDEX,
    INVALID_FLUID_BUCKET_INDEX,
    ACTOR_SHAPE_CIRCLE,
    ACTOR_SHAPE_CAPSULE,
    ACTOR_SHAPE_RECTANGLE,
    PI,
    PBF_SUBSTEP_COUNT,
    PBF_CONSTRAINT_ITERATION_COUNT,
    CONSTRAINT_EPSILON,
    ARTIFICIAL_PRESSURE_DELTA_Q_RATIO,
    MAXIMUM_CORRECTION_CELLS,
    HARD_EXTERNAL_BODY_OCCUPANCY,
    SWIMMER_EXTERNAL_BODY_OCCUPANCY,
    RIGID_EXTERNAL_BODY_OCCUPANCY,
    IMMOVABLE_CONTACT_MASS,
    CONTACT_PRESSURE_TRANSFER,
    LINEAR_FIXED_SCALE,
    ANGULAR_FIXED_SCALE,
    CELL_SIZE,
    CELL_HALF,
    CELL_RADIUS,
    INCOMPRESSIBILITY_MIXING,
    RESERVATION_SCALE,
    RESERVATION_SCALE_U32
}

#import utility::actor_collision_shape::{
    actor_shape_from_parameters,
    actor_shape_intersects_axis_aligned_cell,
    actor_shape_world_extent,
}
#import utility::cell_coordinates::world_cell_from_logical_tile_major_index
#import utility::tile_ring::physical_cell_index_from_world_cell

struct Parameters {
    buffered_origin: vec2<i32>,
    buffered_tile_size: vec2<u32>,
    ring_offset: vec2<u32>,
    gravity: vec2<f32>,
    buffered_cell_count: u32,
    actor_count: u32,
    rigid_cell_count: u32,
    rigid_body_count: u32,
    padding: array<vec4<u32>, 3>,
}

struct ActorProxy {
    center: vec2<f32>,
    velocity: vec2<f32>,
    drive: vec2<f32>,
    shape_parameters: vec2<f32>,
    shape_kind: u32,
    occupancy_kind: u32,
    mass: f32,
    padding: u32,
}

struct RigidCell {
    local: vec2<i32>,
    body: u32,
    material_identifier: u32,
    appearance: u32,
    state_slot: u32,
    state_generation: u32,
    padding: u32,
}

@group(0) @binding(0) var<storage, read_write> occupancy: array<u32>;
@group(0) @binding(1) var<storage, read_write> velocity: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read_write> actor_counts: array<atomic<u32>>;
@group(0) @binding(3) var<uniform> parameters: Parameters;
@group(0) @binding(4) var<storage, read_write> rigid_material_identifiers: array<u32
>;
@group(0) @binding(5) var<storage, read_write> rigid_appearances: array<u32>;
@group(0) @binding(6) var<storage, read_write> rigid_claims: array<atomic<u32>>;
@group(0) @binding(7) var<storage, read> rigid_cells: array<RigidCell>;
@group(0) @binding(8) var<storage, read> rigid_transforms: array<vec4<f32>>;
@group(0) @binding(9) var<storage, read_write> rigid_owners: array<u32>;
@group(0) @binding(10) var<storage, read> actor_proxies: array<ActorProxy>;
@group(0) @binding(11) var<storage, read_write> actor_claims: array<atomic<u32>
>;
@group(0) @binding(12) var<storage, read> destroy_requests: array<u32>;
@group(0) @binding(13) var<storage, read_write> destroy_results: array<vec2<u32>
>;

@compute @workgroup_size(64)
fn resolve_rigid_destruction(
    @builtin(global_invocation_id) invocation: vec3<u32>
) {
    let request = invocation.x;
    if request >= destroy_requests[0] {
        return;
    }
    let index = destroy_requests[request + 1u];
    if index >= parameters.buffered_cell_count {
        destroy_results[request] = vec2<u32>(0xffffffffu);
        return;
    }
    let source = atomicLoad(&rigid_claims[index]);
    if source == 0xffffffffu || source >= arrayLength(&rigid_cells) {
        destroy_results[request] = vec2<u32>(0xffffffffu);
        return;
    }
    let cell = rigid_cells[source];
    destroy_results[request] = vec2<u32>(cell.state_slot, cell.state_generation);
}

@compute @workgroup_size(64)
fn clear_cellular_physics_body_proxy(
    @builtin(global_invocation_id) invocation: vec3<u32>
) {
    if invocation.x >= parameters.buffered_cell_count {
        return;
    }
    occupancy[invocation.x] = 0u;
    velocity[invocation.x] = vec4<f32>(0.0);
    rigid_material_identifiers[invocation.x] = 0u;
    rigid_appearances[invocation.x] = 0u;
    rigid_owners[invocation.x] = 0u;
    atomicStore(&rigid_claims[invocation.x], 0xffffffffu);
    atomicStore(&actor_claims[invocation.x], 0xffffffffu);
}

@compute @workgroup_size(64)
fn clear_actor_counts(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if invocation.x < parameters.actor_count {
        atomicStore(&actor_counts[invocation.x], 0u);
    }
}

fn actor_bounds(actor: ActorProxy) -> vec4<i32> {
    let shape = actor_shape_from_parameters(
      actor.center,
      parameters.gravity,
      actor.shape_parameters,
      actor.shape_kind);
    let extent = actor_shape_world_extent(shape) + vec2<f32>(0.5 / CELLS_PER_TILE_FLOAT);
    return
    vec4<i32>(
      vec2<i32>(floor((actor.center - extent) * CELLS_PER_TILE_FLOAT)),
      vec2<i32>(ceil((actor.center + extent) * CELLS_PER_TILE_FLOAT)));
}

fn actor_claim_candidate(actor_index: u32, actor: ActorProxy, cell: vec2<i32>) {
    let position = (vec2<f32>(cell) + vec2<f32>(0.5)) / CELLS_PER_TILE_FLOAT;
    let shape = actor_shape_from_parameters(
      actor.center,
      parameters.gravity,
      actor.shape_parameters,
      actor.shape_kind);
    if !actor_shape_intersects_axis_aligned_cell(
      position,
      0.5 / CELLS_PER_TILE_FLOAT,
      shape) {
        return;
    }
    let index = physical_cell_index_from_world_cell(
      cell,
      parameters.buffered_origin,
      parameters.buffered_tile_size,
      parameters.ring_offset);
    if index != INVALID_PHYSICAL_CELL_INDEX {
        atomicMin(&actor_claims[index], actor_index);
    }
}

@compute @workgroup_size(64)
fn claim_actor_proxy(
    @builtin(workgroup_id) group: vec3<u32>,
    @builtin(local_invocation_index) lane: u32
) {
    if group.x >= parameters.actor_count {
        return;
    }
    let actor = actor_proxies[group.x];
    let bounds = actor_bounds(actor);
    let width = u32(max(0, bounds.z - bounds.x));
    let count = width * u32(max(0, bounds.w - bounds.y));
    for (var candidate = lane; candidate < count; candidate   += 64u) {
        actor_claim_candidate(
      group.x,
      actor,
      vec2<i32>(
        bounds.x + i32(candidate % width),
        bounds.y + i32(candidate / width)));
    }
}

@compute @workgroup_size(64)
fn count_actor_proxy(
    @builtin(workgroup_id) group: vec3<u32>,
    @builtin(local_invocation_index) lane: u32
) {
    if group.x >= parameters.actor_count {
        return;
    }
    let bounds = actor_bounds(actor_proxies[group.x]);
    let width = u32(max(0, bounds.z - bounds.x));
    let count = width * u32(max(0, bounds.w - bounds.y));
    for (var candidate = lane; candidate < count; candidate   += 64u) {
        let cell = vec2<i32>(
        bounds.x + i32(candidate % width),
        bounds.y + i32(candidate / width));
        let index = physical_cell_index_from_world_cell(
        cell,
        parameters.buffered_origin,
        parameters.buffered_tile_size,
        parameters.ring_offset);
        if index != INVALID_PHYSICAL_CELL_INDEX && atomicLoad(
        &actor_claims[index]) == group.x {
            atomicAdd(&actor_counts[group.x], 1u);
        }
    }
}

@compute @workgroup_size(64)
fn resolve_actor_proxy(
    @builtin(workgroup_id) group: vec3<u32>,
    @builtin(local_invocation_index) lane: u32
) {
    if group.x >= parameters.actor_count {
        return;
    }
    let actor = actor_proxies[group.x];
    let bounds = actor_bounds(actor);
    let width = u32(max(0, bounds.z - bounds.x));
    let count = width * u32(max(0, bounds.w - bounds.y));
    let divisor = f32(max(atomicLoad(&actor_counts[group.x]), 1u));
    for (var candidate = lane; candidate < count; candidate   += 64u) {
        let cell = vec2<i32>(
        bounds.x + i32(candidate % width),
        bounds.y + i32(candidate / width));
        let index = physical_cell_index_from_world_cell(
        cell,
        parameters.buffered_origin,
        parameters.buffered_tile_size,
        parameters.ring_offset);
        if index != INVALID_PHYSICAL_CELL_INDEX && atomicLoad(
        &actor_claims[index]) == group.x {
            occupancy[index] = actor.occupancy_kind;
            velocity[index] = vec4<f32>(actor.velocity, actor.drive / divisor);
        }
    }
}

fn rigid_cell_world_bounds(index: u32) -> vec4<i32> {
    let cell: RigidCell = rigid_cells[index];
    let transform: vec4<f32> = rigid_transforms[cell.body * 3u];
    let local_center: vec2<f32> = (vec2<f32>(cell.local) + vec2<f32>(0.5)) / CELLS_PER_TILE_FLOAT;
    let center: vec2<f32> = transform.xy + vec2<f32>(
      transform.z * local_center.x - transform.w * local_center.y,
      transform.w * local_center.x + transform.z * local_center.y);
    let extent: f32 = (abs(transform.z) + abs(transform.w)) * 0.5;
    let minimum: vec2<i32> = vec2<i32>(floor(center * CELLS_PER_TILE_FLOAT - extent));
    let maximum: vec2<i32> = vec2<i32>(ceil(center * CELLS_PER_TILE_FLOAT + extent));
    return vec4<i32>(minimum, maximum);
}

// Conservatively intersects one rotated rigid cell with a canonical world-cell AABB.
fn rigid_source_contains_world_cell(
    source: u32,
    world_cell: vec2<i32>
) -> bool {
    let cell: RigidCell = rigid_cells[source];
    let transform: vec4<f32> = rigid_transforms[cell.body * 3u];
    let axis_x: vec2<f32> = transform.zw;
    let axis_y: vec2<f32> = vec2<f32>(-axis_x.y, axis_x.x);
    let local_center: vec2<f32> = (vec2<f32>(cell.local) + vec2<f32>(0.5)) / CELLS_PER_TILE_FLOAT;
    let rigid_center: vec2<f32> = transform.xy + axis_x * local_center.x + axis_y * local_center.y;
    let world_center: vec2<f32> = (vec2<f32>(world_cell) + vec2<f32>(0.5)) / CELLS_PER_TILE_FLOAT;
    let difference: vec2<f32> = rigid_center - world_center;
    let axes: array<vec2<f32>, 4> = array<vec2<f32>, 4>(
      vec2<f32>(1.0, 0.0),
      vec2<f32>(0.0, 1.0),
      axis_x,
      axis_y,);
    let half: f32 = 0.5 / CELLS_PER_TILE_FLOAT;
    for (var index: u32 = 0u; index < 4u; index++) {
        let axis: vec2<f32> = axes[index];
        let rigid_radius: f32 = half * (abs(dot(axis_x, axis)) + abs(dot(axis_y, axis)));
        let world_radius: f32 = half * (abs(axis.x) + abs(axis.y));
        if abs(dot(difference, axis)) > rigid_radius + world_radius {
            return false;
        }
    }
    return true;
}

@compute @workgroup_size(64)
fn claim_rigid_cell_proxy(
    @builtin(global_invocation_id) invocation: vec3<u32>
) {
    let source: u32 = invocation.x;
    if source >= parameters.rigid_cell_count {
        return;
    }
    let bounds: vec4<i32> = rigid_cell_world_bounds(source);
    for (var y: i32 = bounds.y; y < bounds.w; y++) {
        for (var x: i32 = bounds.x; x < bounds.z; x++) {
            let index: u32 = physical_cell_index_from_world_cell(
          vec2<i32>(x, y),
          parameters.buffered_origin,
          parameters.buffered_tile_size,
          parameters.ring_offset);
            if index != INVALID_PHYSICAL_CELL_INDEX && occupancy[index] == 0u && rigid_source_contains_world_cell(source, vec2<i32>(x, y)) {
                atomicMin(&rigid_claims[index], source);
            }
        }
    }
}

@compute @workgroup_size(64)
fn resolve_rigid_cell_proxy(
    @builtin(global_invocation_id) invocation: vec3<u32>
) {
    let source: u32 = invocation.x;
    if source >= parameters.rigid_cell_count {
        return;
    }
    let cell: RigidCell = rigid_cells[source];
    let motion: vec4<f32> = rigid_transforms[cell.body * 3u + 1u];
    let center_of_mass: vec2<f32> = rigid_transforms[cell.body * 3u + 2u].xy;
    let bounds: vec4<i32> = rigid_cell_world_bounds(source);
    for (var y: i32 = bounds.y; y < bounds.w; y++) {
        for (var x: i32 = bounds.x; x < bounds.z; x++) {
            let index: u32 = physical_cell_index_from_world_cell(
          vec2<i32>(x, y),
          parameters.buffered_origin,
          parameters.buffered_tile_size,
          parameters.ring_offset);
            if index == INVALID_PHYSICAL_CELL_INDEX || atomicLoad(
          &rigid_claims[index]) != source {
                continue;
            }
            let point: vec2<f32> = (vec2<f32>(f32(x), f32(y)) + vec2<f32>(0.5)) / CELLS_PER_TILE_FLOAT;
            let radius: vec2<f32> = point - center_of_mass;
            occupancy[index] = 3u;
            rigid_owners[index] = cell.body + 1u;
            let point_velocity: vec2<f32> = motion.xy + motion.z * vec2<f32>(-radius.y, radius.x);
            velocity[index] = vec4<f32>(point_velocity.x, point_velocity.y, 0.0, 0.0);
            rigid_material_identifiers[index] = cell.material_identifier;
            rigid_appearances[index] = cell.appearance;
        }
    }
}
