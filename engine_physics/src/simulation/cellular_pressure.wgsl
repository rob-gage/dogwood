// Copyright Rob Gage 2026

#define_import_path compute::cellular_pressure

#import utility::cell_coordinates::{
    CELL_COUNT_PER_TILE,
    world_cell_from_logical_tile_major_index,
}
#import utility::material_identifier::{
    CELLULAR_DYNAMIC_MATERIAL_FORM,
    CELLULAR_STATIC_MATERIAL_FORM,
    EMPTY_MATERIAL_IDENTIFIER,
    material_form_from_identifier,
    material_index_from_identifier,
}
#import utility::tile_ring::{
    INVALID_PHYSICAL_CELL_INDEX,
    physical_cell_index_from_world_cell,
    physical_tile_from_logical_tile,
}

struct Parameters {
    buffered_origin: vec2<i32>,
    buffered_tile_size: vec2<u32>,
    ring_offset: vec2<u32>,
    impulse_center: vec2<f32>,
    impulse_radius: f32,
    impulse_strength: f32,
    delta_time: f32,
    tick: u32,
    buffered_cell_count: u32,
    damage_rate: f32,
    gas_count: u32,
    rigid_body_count: u32,
    gravity: vec2<f32>,
    rigid_cell_count: u32,
    padding: u32,
}

struct StaticProperties {
    pressure_ignore_threshold: f32,
    pressure_transmission: f32,
    debris: u32,
    debris_yield_rate: f32,
    friction: f32,
    restitution: f32,
    padding: vec2<f32>,
}

struct MechanicalFluidCell {
    material_identifier: u32,
    mass: f32,
    velocity: vec2<f32>,
}

@group(0) @binding(0) var<storage, read_write> cellular_material_identifiers: array<u32>;
@group(0) @binding(1) var<storage, read_write> cellular_appearances: array<u32>;
@group(0) @binding(2) var<storage, read_write> cellular_integrities: array<f32>;
@group(0) @binding(3) var<storage, read_write> cellular_kinematics: array<vec4<f32>>;
@group(0) @binding(4) var<storage, read> cellular_static_properties: array<StaticProperties>;
@group(0) @binding(5) var<storage, read> cellular_dynamic_properties: array<vec4<f32>>;
@group(0) @binding(6) var<storage, read_write> pending_pressure: array<vec4<f32>>;
@group(0) @binding(7) var<storage, read_write> pressure_a: array<vec4<f32>>;
@group(0) @binding(8) var<storage, read_write> pressure_b: array<vec4<f32>>;
@group(0) @binding(9) var<storage, read_write> retained_pressure: array<vec4<f32>>;
@group(0) @binding(10) var<storage, read> external_body_occupancy: array<u32>;
@group(0) @binding(11) var<storage, read> external_body_velocity: array<vec4<f32>>;
@group(0) @binding(13) var<uniform> parameters: Parameters;
@group(0) @binding(14) var<storage, read_write> active_pressure_tiles: array<atomic<u32>>;
@group(0) @binding(15) var<storage, read_write> active_pressure_tile_indices: array<u32>;
@group(0) @binding(16) var<storage, read_write> mechanical_fluid_cells: array<MechanicalFluidCell>;
@group(0) @binding(17) var<storage, read> fluid_pressure_properties: array<vec4<f32>>;
@group(0) @binding(18) var<storage, read_write> gas_velocity: array<vec2<f32>>;
@group(0) @binding(19) var<storage, read> gas_concentrations: array<f32>;
@group(0) @binding(20) var<storage, read> gas_properties: array<vec4<f32>>;
@group(0) @binding(21) var<storage, read> fluid_coverage: array<f32>;
@group(0) @binding(22) var<storage, read> rigid_owners: array<u32>;
@group(0) @binding(23) var<storage, read> rigid_material_identifiers: array<u32>;
@group(0) @binding(24) var<storage, read> rigid_transforms: array<vec4<f32>>;

struct RigidReaction {
    impulse_x: atomic<i32>,
    impulse_y: atomic<i32>,
    angular_impulse: atomic<i32>,
    overflow: atomic<i32>,
}

struct RigidContactStatistics {
    contact_count: atomic<u32>,
    static_contact_count: atomic<u32>,
    padding_1: atomic<u32>,
    padding_2: atomic<u32>,
    geometric_support: array<atomic<u32>, 4>,
    motion_support: array<atomic<u32>, 4>,
}

struct RigidStaticContact {
    found: u32,
    body: u32,
    material: u32,
    channel: u32,
    cell_index: u32,
    normal: vec2<f32>,
    point: vec2<f32>,
    penetration: f32,
}

@group(0) @binding(27) var<storage, read_write> rigid_reactions: array<RigidReaction>;
@group(0) @binding(28) var<storage, read_write> rigid_contact_statistics: array<RigidContactStatistics>;
@group(0) @binding(29) var<storage, read> rigid_cells: array<vec4<u32>>;
struct RigidPredictedMotion {
    x: atomic<i32>, y: atomic<i32>, angular: atomic<i32>, padding: atomic<i32>,
}
@group(0) @binding(30) var<storage, read_write> rigid_predicted_motion: array<RigidPredictedMotion>;
@group(1) @binding(0) var<storage, read_write> pressure_indirect_dispatch: array<atomic<u32>>;

const IMMOVABLE_CONTACT_MASS: f32 = 1000000.0;
const CONTACT_PRESSURE_TRANSFER: f32 = 0.02;
const LINEAR_FIXED_SCALE: f32 = 256.0;
const ANGULAR_FIXED_SCALE: f32 = 64.0;
const CELL_SIZE: f32 = 0.125;
const CELL_HALF: f32 = 0.0625;
const CELL_RADIUS: f32 = 0.08838835;
var<workgroup> pressure_tile_has_source: atomic<u32>;

@compute @workgroup_size(64)
fn initialize_rigid_contact_state(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let body: u32 = invocation.x;
    if body >= parameters.rigid_body_count { return; }
    atomicStore(&rigid_contact_statistics[body].contact_count, 0u);
    atomicStore(&rigid_contact_statistics[body].static_contact_count, 0u);
    atomicStore(&rigid_contact_statistics[body].padding_1, 0u);
    atomicStore(&rigid_contact_statistics[body].padding_2, 0u);
    let motion: vec4<f32> = rigid_transforms[body * 3u + 1u];
    atomicStore(&rigid_predicted_motion[body].x, i32(round(motion.x * 4096.0)));
    atomicStore(&rigid_predicted_motion[body].y, i32(round(motion.y * 4096.0)));
    atomicStore(&rigid_predicted_motion[body].angular, i32(round(motion.z * 4096.0)));
    for (var channel: u32 = 0u; channel < 4u; channel++) {
        atomicStore(&rigid_contact_statistics[body].geometric_support[channel], 0u);
        atomicStore(&rigid_contact_statistics[body].motion_support[channel], 0u);
    }
}

@compute @workgroup_size(64)
fn gather_rigid_static_contacts(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let contact: RigidStaticContact = find_rigid_static_contact(invocation.x);
    if contact.found == 0u { return; }
    process_rigid_cellular_contact(contact.body, rigid_cells[invocation.x * 2u].w,
        contact.cell_index, -contact.normal, contact.point, contact.penetration, true);
}

@compute @workgroup_size(64)
fn resolve_rigid_static_contacts(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let contact: RigidStaticContact = find_rigid_static_contact(invocation.x);
    if contact.found == 0u { return; }
    process_rigid_cellular_contact(contact.body, rigid_cells[invocation.x * 2u].w,
        contact.cell_index, -contact.normal, contact.point, contact.penetration, false);
}

// Clears the transient coarse mask before current pressure sources are discovered
@compute @workgroup_size(64)
fn clear_active_cellular_pressure_tiles(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let tile_count: u32 = parameters.buffered_tile_size.x * parameters.buffered_tile_size.y;
    if invocation.x < tile_count { atomicStore(&active_pressure_tiles[invocation.x], 0u); }
}

// Marks source tiles and the one-tile halo reachable by the fixed six-pass stencil
@compute @workgroup_size(64)
fn mark_active_cellular_pressure_tiles(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) lane: u32,
) {
    let logical_tile_index: u32 = workgroup.x;
    let tile_count: u32 = parameters.buffered_tile_size.x * parameters.buffered_tile_size.y;
    if logical_tile_index >= tile_count { return; }
    if lane == 0u { atomicStore(&pressure_tile_has_source, 0u); }
    workgroupBarrier();
    let logical_tile: vec2<u32> = vec2<u32>(
        logical_tile_index % parameters.buffered_tile_size.x,
        logical_tile_index / parameters.buffered_tile_size.x,
    );
    let physical_tile: vec2<u32> = physical_tile_from_logical_tile(
        logical_tile, parameters.buffered_tile_size, parameters.ring_offset,
    );
    let cell_start: u32 =
        (physical_tile.y * parameters.buffered_tile_size.x + physical_tile.x) *
            CELL_COUNT_PER_TILE;
    let index: u32 = cell_start + lane;
    let cell: vec2<i32> = world_cell_from_logical_tile_major_index(
        logical_tile_index * CELL_COUNT_PER_TILE + lane,
        parameters.buffered_origin, parameters.buffered_tile_size,
    );
    var has_source: bool = any(pending_pressure[index] != vec4<f32>(0.0)) ||
        external_body_occupancy[index] == 1u || external_body_occupancy[index] == 2u;
    let material: u32 = cellular_material_identifiers[index];
    let form: u32 = material_form_from_identifier(material);
    for (var channel: u32 = 0u; channel < 4u && !has_source; channel++) {
        let neighbor: u32 = cellular_pressure_physical_cell_index_from_world_cell(
            cell + world_cell_direction_from_pressure_channel(channel));
        if neighbor == INVALID_PHYSICAL_CELL_INDEX { continue; }
        let neighbor_form: u32 = material_form_from_identifier(
            cellular_material_identifiers[neighbor]);
        let rigid_interface: bool = (rigid_owners[index] != 0u) !=
            (rigid_owners[neighbor] != 0u);
        let fluid_interface: bool = (mechanical_fluid_cells[index].mass > 0.0) !=
            (mechanical_fluid_cells[neighbor].mass > 0.0) &&
            (form == CELLULAR_DYNAMIC_MATERIAL_FORM ||
                form == CELLULAR_STATIC_MATERIAL_FORM ||
                neighbor_form == CELLULAR_DYNAMIC_MATERIAL_FORM ||
                neighbor_form == CELLULAR_STATIC_MATERIAL_FORM ||
                gas_cell_is_open(index) || gas_cell_is_open(neighbor));
        let gas_interface: bool = (gas_cell_is_open(index) != gas_cell_is_open(neighbor)) &&
            (form != 0u || neighbor_form != 0u ||
                mechanical_fluid_cells[index].mass > 0.0 ||
                mechanical_fluid_cells[neighbor].mass > 0.0) &&
            (length(gas_velocity[index]) > 0.0001 ||
                length(gas_velocity[neighbor]) > 0.0001);
        var granular_impact: bool = false;
        if form == CELLULAR_DYNAMIC_MATERIAL_FORM &&
                (neighbor_form == CELLULAR_DYNAMIC_MATERIAL_FORM ||
                    neighbor_form == CELLULAR_STATIC_MATERIAL_FORM) {
            granular_impact = dot(
                cellular_kinematics[index].xy - cellular_kinematics[neighbor].xy,
                vec2<f32>(world_cell_direction_from_pressure_channel(channel)),
            ) > 0.0001;
        }
        has_source = rigid_interface || fluid_interface || gas_interface || granular_impact;
    }
    if has_source { atomicStore(&pressure_tile_has_source, 1u); }
    workgroupBarrier();
    if lane != 0u || atomicLoad(&pressure_tile_has_source) == 0u { return; }
    for (var offset_y: i32 = -1; offset_y <= 1; offset_y++) {
        for (var offset_x: i32 = -1; offset_x <= 1; offset_x++) {
            let active_tile: vec2<i32> = vec2<i32>(logical_tile) +
                vec2<i32>(offset_x, offset_y);
            if any(active_tile < vec2<i32>(0)) ||
                    active_tile.x >= i32(parameters.buffered_tile_size.x) ||
                    active_tile.y >= i32(parameters.buffered_tile_size.y) { continue; }
            atomicStore(&active_pressure_tiles[
                u32(active_tile.y) * parameters.buffered_tile_size.x + u32(active_tile.x)
            ], 1u);
        }
    }
}

// Converts the coarse pressure mask into one indirect workgroup per active tile
@compute @workgroup_size(64)
fn compact_active_cellular_pressure_tiles(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if invocation.x == 0u {
        atomicStore(&pressure_indirect_dispatch[1], 1u);
        atomicStore(&pressure_indirect_dispatch[2], 1u);
    }
    let tile_count: u32 = parameters.buffered_tile_size.x * parameters.buffered_tile_size.y;
    let logical_tile_index: u32 = invocation.x;
    if logical_tile_index >= tile_count ||
            atomicLoad(&active_pressure_tiles[logical_tile_index]) == 0u { return; }
    let slot: u32 = atomicAdd(&pressure_indirect_dispatch[0], 1u);
    active_pressure_tile_indices[slot] = logical_tile_index;
}

// Queues editor impulse pressure without bypassing material transmission or mass response
@compute @workgroup_size(64)
fn queue_cellular_radial_impulse(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let logical_index: u32 = invocation.x;
    if logical_index >= parameters.buffered_cell_count { return; }
    let cell: vec2<i32> = cellular_pressure_world_cell_from_logical_index(logical_index);
    let index: u32 = cellular_pressure_physical_cell_index_from_world_cell(cell);
    if index == INVALID_PHYSICAL_CELL_INDEX ||
            cellular_material_identifiers[index] == EMPTY_MATERIAL_IDENTIFIER { return; }
    let delta: vec2<f32> = vec2<f32>(cell) + vec2<f32>(0.5) - parameters.impulse_center;
    let distance: f32 = length(delta);
    if distance > parameters.impulse_radius { return; }
    let direction: vec2<f32> = normalize(select(
        vec2<f32>(1.0, 0.0),
        delta,
        distance > 0.0001,
    ));
    let falloff: f32 = 1.0 - distance / max(parameters.impulse_radius, 0.0001);
    let impulse: vec2<f32> = direction * parameters.impulse_strength * falloff;
    pending_pressure[index] += encode_directional_pressure(impulse);
    let material: u32 = cellular_material_identifiers[index];
    if material_form_from_identifier(material) == CELLULAR_DYNAMIC_MATERIAL_FORM {
        let mass: f32 = cellular_dynamic_properties[material_index_from_identifier(material)].x;
        cellular_kinematics[index].x += impulse.x / mass;
        cellular_kinematics[index].y += impulse.y / mass;
    }
}

// One color writes each ordinary cell at most once, so both sides receive equal momentum.
@compute @workgroup_size(64)
fn resolve_cellular_contacts_horizontal_even(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    process_cellular_face(workgroup.x, local_index, true, 0, false);
}

@compute @workgroup_size(64)
fn resolve_cellular_contacts_horizontal_odd(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    process_cellular_face(workgroup.x, local_index, true, 1, false);
}

@compute @workgroup_size(64)
fn resolve_cellular_contacts_vertical_even(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    process_cellular_face(workgroup.x, local_index, false, 0, false);
}

@compute @workgroup_size(64)
fn resolve_cellular_contacts_vertical_odd(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    process_cellular_face(workgroup.x, local_index, false, 1, false);
}

@compute @workgroup_size(64)
fn gather_rigid_contacts_horizontal_even(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    process_cellular_face(workgroup.x, local_index, true, 0, true);
}

@compute @workgroup_size(64)
fn gather_rigid_contacts(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    let logical_index: u32 = logical_cell_index_from_active_pressure_workgroup(
        workgroup.x, local_index);
    if logical_index >= parameters.buffered_cell_count { return; }
    let cell: vec2<i32> = cellular_pressure_world_cell_from_logical_index(logical_index);
    let index: u32 = cellular_pressure_physical_cell_index_from_world_cell(cell);
    if index == INVALID_PHYSICAL_CELL_INDEX { return; }
    for (var channel: u32 = 0u; channel < 4u; channel++) {
        let direction: vec2<i32> = world_cell_direction_from_pressure_channel(channel);
        let neighbor: u32 = cellular_pressure_physical_cell_index_from_world_cell(
            cell + direction);
        if neighbor == INVALID_PHYSICAL_CELL_INDEX { continue; }
        if rigid_owners[index] != 0u && rigid_owners[neighbor] == 0u {
            process_rigid_cellular_face(index, neighbor, vec2<f32>(direction),
                (vec2<f32>(cell) + vec2<f32>(0.5) + vec2<f32>(direction) * 0.5) / 8.0,
                true);
        }
    }
}

@compute @workgroup_size(64)
fn gather_rigid_contacts_horizontal_odd(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    process_cellular_face(workgroup.x, local_index, true, 1, true);
}

@compute @workgroup_size(64)
fn gather_rigid_contacts_vertical_even(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    process_cellular_face(workgroup.x, local_index, false, 0, true);
}

@compute @workgroup_size(64)
fn gather_rigid_contacts_vertical_odd(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    process_cellular_face(workgroup.x, local_index, false, 1, true);
}

fn process_cellular_face(
    workgroup: u32, local_index: u32, horizontal: bool, parity: i32, gather: bool,
) {
    let logical_index: u32 = logical_cell_index_from_active_pressure_workgroup(workgroup, local_index);
    if logical_index >= parameters.buffered_cell_count { return; }
    let cell: vec2<i32> = cellular_pressure_world_cell_from_logical_index(logical_index);
    if (select(cell.y, cell.x, horizontal) & 1) != parity { return; }
    let direction: vec2<i32> = select(vec2<i32>(0, 1), vec2<i32>(1, 0), horizontal);
    let first: u32 = cellular_pressure_physical_cell_index_from_world_cell(cell);
    let second: u32 = cellular_pressure_physical_cell_index_from_world_cell(cell + direction);
    if first == INVALID_PHYSICAL_CELL_INDEX || second == INVALID_PHYSICAL_CELL_INDEX {
        return;
    }
    let first_material: u32 = cellular_material_identifiers[first];
    let second_material: u32 = cellular_material_identifiers[second];
    let first_form: u32 = material_form_from_identifier(first_material);
    let second_form: u32 = material_form_from_identifier(second_material);
    let owner_first: u32 = rigid_owners[first];
    let owner_second: u32 = rigid_owners[second];
    if owner_first != 0u && owner_second == 0u {
        process_rigid_cellular_face(first, second, vec2<f32>(direction),
            (vec2<f32>(cell) + vec2<f32>(0.5) + vec2<f32>(direction) * 0.5) / 8.0,
            gather);
    }
    if owner_second != 0u && owner_first == 0u {
        process_rigid_cellular_face(second, first, -vec2<f32>(direction),
            (vec2<f32>(cell) + vec2<f32>(0.5) + vec2<f32>(direction) * 0.5) / 8.0,
            gather);
    }
    if gather { return; }
    if external_body_occupancy[first] != 0u || external_body_occupancy[second] != 0u {
        return;
    }
    if mechanical_fluid_cells[first].mass > 0.0 &&
            second_form == CELLULAR_STATIC_MATERIAL_FORM ||
            mechanical_fluid_cells[first].mass > 0.0 &&
                second_form == CELLULAR_DYNAMIC_MATERIAL_FORM {
        resolve_fluid_cellular_face(first, second, vec2<f32>(direction));
    }
    if mechanical_fluid_cells[second].mass > 0.0 &&
            first_form == CELLULAR_STATIC_MATERIAL_FORM ||
            mechanical_fluid_cells[second].mass > 0.0 &&
                first_form == CELLULAR_DYNAMIC_MATERIAL_FORM {
        resolve_fluid_cellular_face(second, first, -vec2<f32>(direction));
    }
    if gas_cell_is_open(first) {
        if second_form == CELLULAR_STATIC_MATERIAL_FORM ||
                second_form == CELLULAR_DYNAMIC_MATERIAL_FORM {
            resolve_gas_cellular_face(first, second, vec2<f32>(direction));
        } else if mechanical_fluid_cells[second].mass > 0.0 {
            resolve_gas_fluid_face(first, second, vec2<f32>(direction));
        }
    }
    if gas_cell_is_open(second) {
        if first_form == CELLULAR_STATIC_MATERIAL_FORM ||
                first_form == CELLULAR_DYNAMIC_MATERIAL_FORM {
            resolve_gas_cellular_face(second, first, -vec2<f32>(direction));
        } else if mechanical_fluid_cells[first].mass > 0.0 {
            resolve_gas_fluid_face(second, first, -vec2<f32>(direction));
        }
    }
    let first_dynamic: bool = first_form == CELLULAR_DYNAMIC_MATERIAL_FORM;
    let second_dynamic: bool = second_form == CELLULAR_DYNAMIC_MATERIAL_FORM;
    if (!first_dynamic && first_form != CELLULAR_STATIC_MATERIAL_FORM) ||
            (!second_dynamic && second_form != CELLULAR_STATIC_MATERIAL_FORM) ||
            (!first_dynamic && !second_dynamic) { return; }
    let first_inverse_mass: f32 = select(0.0,
        1.0 / max(cellular_contact_mass_at_physical_cell_index(first), 0.000001), first_dynamic);
    let second_inverse_mass: f32 = select(0.0,
        1.0 / max(cellular_contact_mass_at_physical_cell_index(second), 0.000001), second_dynamic);
    let inverse_mass_sum: f32 = first_inverse_mass + second_inverse_mass;
    if inverse_mass_sum <= 0.0 { return; }
    let normal: vec2<f32> = vec2<f32>(direction);
    let relative: vec2<f32> = cellular_kinematics[first].xy - cellular_kinematics[second].xy;
    let approach: f32 = dot(relative, normal);
    if approach <= 0.0001 { return; }
    let restitution: f32 = clamp(cellular_contact_restitution(first, second), 0.0, 1.0);
    let normal_impulse: f32 = (1.0 + restitution) * approach / inverse_mass_sum;
    let tangent: vec2<f32> = vec2<f32>(-normal.y, normal.x);
    let friction: f32 = max(0.0, min(
        cellular_contact_friction_at_physical_cell_index(first),
        cellular_contact_friction_at_physical_cell_index(second),
    ));
    let tangent_impulse: f32 = clamp(
        dot(relative, tangent) / inverse_mass_sum,
        -friction * normal_impulse, friction * normal_impulse,
    );
    let impulse: vec2<f32> = normal * normal_impulse + tangent * tangent_impulse;
    if first_dynamic {
        cellular_kinematics[first].x -= impulse.x * first_inverse_mass;
        cellular_kinematics[first].y -= impulse.y * first_inverse_mass;
    }
    if second_dynamic {
        cellular_kinematics[second].x += impulse.x * second_inverse_mass;
        cellular_kinematics[second].y += impulse.y * second_inverse_mass;
    }
    let stress: f32 = normal_impulse * CONTACT_PRESSURE_TRANSFER;
    if horizontal {
        pending_pressure[first].x += stress;
        pending_pressure[second].y += stress;
    } else {
        pending_pressure[first].z += stress;
        pending_pressure[second].w += stress;
    }
}

fn process_rigid_static_overlap(cell: vec2<i32>, gather: bool) {
    let index: u32 = cellular_pressure_physical_cell_index_from_world_cell(cell);
    if index == INVALID_PHYSICAL_CELL_INDEX || rigid_owners[index] == 0u ||
            material_form_from_identifier(cellular_material_identifiers[index]) !=
                CELLULAR_STATIC_MATERIAL_FORM { return; }
    let left: u32 = cellular_pressure_physical_cell_index_from_world_cell(cell + vec2<i32>(-1, 0));
    let right: u32 = cellular_pressure_physical_cell_index_from_world_cell(cell + vec2<i32>(1, 0));
    let below: u32 = cellular_pressure_physical_cell_index_from_world_cell(cell + vec2<i32>(0, -1));
    let above: u32 = cellular_pressure_physical_cell_index_from_world_cell(cell + vec2<i32>(0, 1));
    let gradient: vec2<f32> = vec2<f32>(
        f32(is_static_cell(left)) - f32(is_static_cell(right)),
        f32(is_static_cell(below)) - f32(is_static_cell(above)),
    );
    let body: u32 = rigid_owners[index] - 1u;
    let motion: vec2<f32> = rigid_transforms[body * 3u + 1u].xy;
    var normal: vec2<f32> = vec2<f32>(0.0);
    if length(gradient) > 0.0 {
        normal = -normalize(gradient);
    } else if length(motion) > 0.0001 {
        normal = normalize(motion);
    } else {
        return;
    }
    process_rigid_cellular_face(index, index, normal,
        (vec2<f32>(cell) + vec2<f32>(0.5)) / 8.0, gather);
}

fn is_static_cell(index: u32) -> bool {
    return index != INVALID_PHYSICAL_CELL_INDEX &&
        material_form_from_identifier(cellular_material_identifiers[index]) ==
            CELLULAR_STATIC_MATERIAL_FORM;
}

fn process_rigid_cellular_face(
    rigid: u32, other: u32, normal: vec2<f32>, point: vec2<f32>, gather: bool,
) {
    let owner: u32 = rigid_owners[rigid];
    if owner == 0u || owner > parameters.rigid_body_count { return; }
    // Static contacts are discovered by the swept/overlap pass. Sending the
    // adjacent grid face as well would solve the same contact twice.
    if material_form_from_identifier(cellular_material_identifiers[other]) ==
            CELLULAR_STATIC_MATERIAL_FORM { return; }
    process_rigid_cellular_contact(owner - 1u, rigid_material_identifiers[rigid], other,
        normal, point, 0.0, gather);
}

// The single rigid/cellular response solver. Static-specific code may discover a swept or
// overlap contact, but it must feed that geometry here rather than implement different physics.
fn process_rigid_cellular_contact(
    body: u32, rigid_material: u32, other: u32, normal: vec2<f32>, point: vec2<f32>,
    penetration: f32, gather: bool,
) {
    let material: u32 = cellular_material_identifiers[other];
    let form: u32 = material_form_from_identifier(material);
    let dynamic: bool = form == CELLULAR_DYNAMIC_MATERIAL_FORM;
    let static_cell: bool = form == CELLULAR_STATIC_MATERIAL_FORM;
    let fluid: bool = !dynamic && !static_cell && mechanical_fluid_cells[other].mass > 0.0;
    let gas: bool = !dynamic && !static_cell && !fluid && gas_cell_is_open(other);
    if !dynamic && !static_cell && !fluid && !gas { return; }
    let mass_record: vec4<f32> = rigid_transforms[body * 3u + 2u];
    let radius: vec2<f32> = point - mass_record.xy;
    let shadow: vec4<f32> = vec4<f32>(
        f32(atomicLoad(&rigid_predicted_motion[body].x)) / 4096.0,
        f32(atomicLoad(&rigid_predicted_motion[body].y)) / 4096.0,
        f32(atomicLoad(&rigid_predicted_motion[body].angular)) / 4096.0, 0.0);
    let rigid_velocity: vec2<f32> = shadow.xy +
        shadow.z * vec2<f32>(-radius.y, radius.x);
    let other_velocity: vec2<f32> = select(
        select(vec2<f32>(0.0), cellular_kinematics[other].xy, dynamic),
        select(mechanical_fluid_cells[other].velocity, gas_velocity[other] / 8.0, gas),
        fluid || gas,
    );
    let relative: vec2<f32> = rigid_velocity - other_velocity;
    let approach: f32 = dot(relative, normal);
    let channel: u32 = dominant_cardinal_channel(normal);
    if gather {
        atomicAdd(&rigid_contact_statistics[body].geometric_support[channel], 1u);
        if approach > 0.02 || dynamic && length(other_velocity) > 0.02 ||
                penetration > 0.0001 {
            atomicAdd(&rigid_contact_statistics[body].motion_support[channel], 1u);
        }
        return;
    }
    let normal_arm: f32 = radius.x * normal.y - radius.y * normal.x;
    let rigid_inverse_effective_mass: f32 = mass_record.z +
        mass_record.w * normal_arm * normal_arm;
    var other_inverse_mass: f32 = select(0.0,
        1.0 / max(cellular_contact_mass_at_physical_cell_index(other), 0.000001), dynamic);
    other_inverse_mass = select(other_inverse_mass,
        1.0 / max(mechanical_fluid_cells[other].mass, 0.000001), fluid);
    other_inverse_mass = select(other_inverse_mass, gas_cell_inverse_mass(other), gas);
    let simultaneous_contacts: f32 = f32(max(
        atomicLoad(&rigid_contact_statistics[body].motion_support[channel]), 1u));
    let inverse_mass_sum: f32 = rigid_inverse_effective_mass + other_inverse_mass;
    let rigid_index: u32 = material_index_from_identifier(rigid_material);
    let rigid_properties: StaticProperties = cellular_static_properties[rigid_index];
    var restitution: f32 = 0.0;
    var friction: f32 = 0.0;
    if dynamic || static_cell {
        restitution = min(rigid_properties.restitution,
            cellular_material_restitution_at_physical_cell_index(other));
        friction = min(rigid_properties.friction,
            cellular_contact_friction_at_physical_cell_index(other));
    } else if fluid {
        let properties: vec4<f32> = fluid_pressure_properties[
            material_index_from_identifier(mechanical_fluid_cells[other].material_identifier)];
        restitution = min(rigid_properties.restitution, properties.z);
        friction = min(rigid_properties.friction, properties.y);
    }
    var normal_impulse: f32 = 0.0;
    if dynamic || static_cell {
        let rigid_toward: f32 = max(0.0, dot(rigid_velocity, normal));
        let grain_toward: f32 = max(0.0, -dot(other_velocity, normal));
        let toward_sum: f32 = rigid_toward + grain_toward;
        if toward_sum > 0.0001 {
            let relative_approach: f32 = max(approach, 0.0);
            let grain_approach: f32 = relative_approach * grain_toward / toward_sum;
            let rigid_approach: f32 = relative_approach * rigid_toward / toward_sum;
            if inverse_mass_sum > 0.0 {
                normal_impulse += (1.0 + clamp(restitution, 0.0, 1.0)) *
                    grain_approach / inverse_mass_sum;
            }
            if rigid_inverse_effective_mass > 0.000001 {
                normal_impulse += rigid_approach /
                    rigid_inverse_effective_mass / simultaneous_contacts;
            }
        }
    } else if inverse_mass_sum > 0.0 {
        normal_impulse = (1.0 + clamp(restitution, 0.0, 1.0)) *
            max(approach, 0.0) /
                (simultaneous_contacts * rigid_inverse_effective_mass + other_inverse_mass);
    }
    if penetration > CELL_SIZE * 0.02 && rigid_inverse_effective_mass > 0.000001 {
        let correction_speed: f32 = min(CELL_SIZE / parameters.delta_time,
            (penetration - CELL_SIZE * 0.02) * 0.2 / parameters.delta_time);
        normal_impulse += correction_speed /
            (simultaneous_contacts * rigid_inverse_effective_mass + other_inverse_mass);
    }
    let geometric_contacts: u32 = atomicLoad(
        &rigid_contact_statistics[body].geometric_support[channel]);
    var support_impulse: f32 = 0.0;
    if geometric_contacts != 0u && mass_record.z > 0.000001 &&
            (static_cell || dynamic) && approach >= -0.0001 {
        support_impulse = max(0.0,
            dot(parameters.gravity * parameters.delta_time / mass_record.z, normal)) /
                f32(geometric_contacts);
        normal_impulse += support_impulse;
    }
    if normal_impulse <= 0.0 { return; }
    let tangent: vec2<f32> = vec2<f32>(-normal.y, normal.x);
    let tangent_arm: f32 = radius.x * tangent.y - radius.y * tangent.x;
    let tangent_inverse_mass: f32 = mass_record.z + mass_record.w * tangent_arm * tangent_arm +
        other_inverse_mass;
    let tangent_impulse: f32 = select(0.0, clamp(
        dot(relative, tangent) / max(tangent_inverse_mass, 0.000001),
        -max(friction, 0.0) * normal_impulse,
        max(friction, 0.0) * normal_impulse,
    ), tangent_inverse_mass > 0.0);
    let impulse: vec2<f32> = normal * normal_impulse + tangent * tangent_impulse;
    if other_inverse_mass > 0.0 {
        let after: vec2<f32> = other_velocity + impulse * other_inverse_mass;
        let transferred_energy: f32 = max(0.0,
            (dot(other_velocity, other_velocity) - dot(after, after)) /
                (2.0 * other_inverse_mass));
        if transferred_energy < 1000000.0 {
            atomicAdd(&rigid_contact_statistics[body].padding_1,
                u32(round(transferred_energy * 256.0)));
        }
    }
    if dynamic {
        cellular_kinematics[other].x += impulse.x * other_inverse_mass;
        cellular_kinematics[other].y += impulse.y * other_inverse_mass;
    } else if fluid {
        mechanical_fluid_cells[other].velocity += impulse * other_inverse_mass;
    } else if gas {
        gas_velocity[other] += impulse * other_inverse_mass * 8.0;
    }
    if static_cell || dynamic {
        pending_pressure[other] += encode_directional_pressure(normal *
            normal_impulse * CONTACT_PRESSURE_TRANSFER);
    }
    let reaction: vec2<f32> = -impulse;
    let torque: f32 = radius.x * reaction.y - radius.y * reaction.x;
    if support_impulse > 0.0 {
        let support_reaction: vec2<f32> = -normal * support_impulse;
        let support_torque: f32 = radius.x * support_reaction.y -
            radius.y * support_reaction.x;
        let support_credit: f32 = max(0.0,
            dot(rigid_velocity, support_reaction) + shadow.z * support_torque) +
            0.5 * f32(geometric_contacts) * (
                mass_record.z * dot(support_reaction, support_reaction) +
                mass_record.w * support_torque * support_torque);
        atomicAdd(&rigid_contact_statistics[body].padding_1,
            u32(round(min(support_credit, 1000000.0) * 256.0)));
    }
    if any(reaction != reaction) || any(abs(reaction) > vec2<f32>(1000000.0)) ||
            abs(torque) > 1000000.0 {
        atomicStore(&rigid_reactions[body].overflow, 1);
        return;
    }
    var overflowed: bool = saturating_rigid_atomic_add(body,
        i32(round(reaction.x * LINEAR_FIXED_SCALE)), 0u);
    overflowed = saturating_rigid_atomic_add(body,
        i32(round(reaction.y * LINEAR_FIXED_SCALE)), 1u) || overflowed;
    overflowed = saturating_rigid_atomic_add(body,
        i32(round(torque * ANGULAR_FIXED_SCALE)), 2u) || overflowed;
    if overflowed { atomicStore(&rigid_reactions[body].overflow, 1); }
    atomicAdd(&rigid_predicted_motion[body].x,
        i32(round(reaction.x * mass_record.z * 4096.0)));
    atomicAdd(&rigid_predicted_motion[body].y,
        i32(round(reaction.y * mass_record.z * 4096.0)));
    atomicAdd(&rigid_predicted_motion[body].angular,
        i32(round(torque * mass_record.w * 4096.0)));
    atomicAdd(&rigid_contact_statistics[body].contact_count, 1u);
    if static_cell { atomicAdd(&rigid_contact_statistics[body].static_contact_count, 1u); }
    if dynamic {
        let moving_grain: bool = approach > 0.02 || length(other_velocity) > 0.02;
        atomicAdd(&rigid_contact_statistics[body].padding_2,
            select(1u, 0x00010001u, moving_grain));
    }
}

fn gas_cell_is_open(index: u32) -> bool {
    return cellular_material_identifiers[index] == EMPTY_MATERIAL_IDENTIFIER &&
        external_body_occupancy[index] == 0u && fluid_coverage[index] < 0.85;
}

fn gas_cell_inverse_mass(index: u32) -> f32 {
    var density: f32 = 1.0;
    for (var species: u32 = 0u; species < parameters.gas_count; species++) {
        let concentration: f32 = gas_concentrations[
            species * parameters.buffered_cell_count + index];
        density += concentration * (gas_properties[species * 2u].x - 1.0);
    }
    return 64.0 / max(density, 0.000001);
}

fn resolve_gas_cellular_face(gas: u32, cellular: u32, normal: vec2<f32>) {
    let dynamic: bool = material_form_from_identifier(
        cellular_material_identifiers[cellular]) == CELLULAR_DYNAMIC_MATERIAL_FORM;
    let inverse_gas_mass: f32 = gas_cell_inverse_mass(gas);
    let inverse_cellular_mass: f32 = select(0.0,
        1.0 / max(cellular_contact_mass_at_physical_cell_index(cellular), 0.000001), dynamic);
    let relative: vec2<f32> = gas_velocity[gas] / 8.0 - cellular_kinematics[cellular].xy;
    let approach: f32 = dot(relative, normal);
    if approach <= 0.0001 { return; }
    let normal_impulse: f32 = approach / (inverse_gas_mass + inverse_cellular_mass);
    gas_velocity[gas] -= normal * normal_impulse * inverse_gas_mass * 8.0;
    if dynamic {
        cellular_kinematics[cellular].x += normal.x * normal_impulse * inverse_cellular_mass;
        cellular_kinematics[cellular].y += normal.y * normal_impulse * inverse_cellular_mass;
    }
    pending_pressure[cellular] += encode_directional_pressure(normal *
        normal_impulse * CONTACT_PRESSURE_TRANSFER);
}

fn resolve_gas_fluid_face(gas: u32, fluid: u32, normal: vec2<f32>) {
    let mass: f32 = mechanical_fluid_cells[fluid].mass;
    if mass <= 0.0 || cellular_material_identifiers[fluid] != EMPTY_MATERIAL_IDENTIFIER { return; }
    let inverse_gas_mass: f32 = gas_cell_inverse_mass(gas);
    let inverse_fluid_mass: f32 = 1.0 / mass;
    let relative: vec2<f32> = gas_velocity[gas] / 8.0 - mechanical_fluid_cells[fluid].velocity;
    let approach: f32 = dot(relative, normal);
    if approach <= 0.0001 { return; }
    let normal_impulse: f32 = approach / (inverse_gas_mass + inverse_fluid_mass);
    gas_velocity[gas] -= normal * normal_impulse * inverse_gas_mass * 8.0;
    mechanical_fluid_cells[fluid].velocity += normal * normal_impulse * inverse_fluid_mass;
}

fn resolve_fluid_cellular_face(fluid: u32, cellular: u32, normal: vec2<f32>) {
    if cellular_material_identifiers[fluid] != EMPTY_MATERIAL_IDENTIFIER { return; }
    let fluid_state: MechanicalFluidCell = mechanical_fluid_cells[fluid];
    if fluid_state.mass <= 0.0 { return; }
    let material: u32 = cellular_material_identifiers[cellular];
    let dynamic: bool = material_form_from_identifier(material) == CELLULAR_DYNAMIC_MATERIAL_FORM;
    let inverse_fluid_mass: f32 = 1.0 / fluid_state.mass;
    let inverse_cellular_mass: f32 = select(0.0,
        1.0 / max(cellular_contact_mass_at_physical_cell_index(cellular), 0.000001), dynamic);
    let inverse_mass_sum: f32 = inverse_fluid_mass + inverse_cellular_mass;
    let relative: vec2<f32> = fluid_state.velocity - cellular_kinematics[cellular].xy;
    let approach: f32 = dot(relative, normal);
    if approach <= 0.0001 { return; }
    let properties: vec4<f32> = fluid_pressure_properties[
        material_index_from_identifier(fluid_state.material_identifier)];
    let restitution: f32 = clamp(min(properties.z,
        cellular_material_restitution_at_physical_cell_index(cellular)), 0.0, 1.0);
    let normal_impulse: f32 = (1.0 + restitution) * approach / inverse_mass_sum;
    let tangent: vec2<f32> = vec2<f32>(-normal.y, normal.x);
    let friction: f32 = max(0.0, min(properties.y,
        cellular_contact_friction_at_physical_cell_index(cellular)));
    let tangent_impulse: f32 = clamp(dot(relative, tangent) / inverse_mass_sum,
        -friction * normal_impulse, friction * normal_impulse);
    let impulse: vec2<f32> = normal * normal_impulse + tangent * tangent_impulse;
    mechanical_fluid_cells[fluid].velocity -= impulse * inverse_fluid_mass;
    if dynamic {
        cellular_kinematics[cellular].x += impulse.x * inverse_cellular_mass;
        cellular_kinematics[cellular].y += impulse.y * inverse_cellular_mass;
    }
    pending_pressure[cellular] += encode_directional_pressure(normal *
        normal_impulse * CONTACT_PRESSURE_TRANSFER);
}

@compute @workgroup_size(64)
fn propagate_pending_cellular_pressure(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    let logical_index: u32 = logical_cell_index_from_active_pressure_workgroup(workgroup.x, local_index);
    if logical_index >= parameters.buffered_cell_count { return; }
    let cell: vec2<i32> = cellular_pressure_world_cell_from_logical_index(logical_index);
    let index: u32 = cellular_pressure_physical_cell_index_from_world_cell(
        cell);
    if index == INVALID_PHYSICAL_CELL_INDEX { return; }
    retained_pressure[index] = vec4<f32>(0.0);
    pressure_a[index] = vec4<f32>(0.0);
    let source: vec4<f32> = pending_pressure_source(index);
    for (var channel: u32 = 0u; channel < 4u; channel++) {
        retained_pressure[index][channel] = source[channel] -
            calculate_outgoing_cellular_pressure(cell, channel, source[channel]);
        pressure_b[index][channel] = gather_pending_cellular_pressure(cell, channel);
    }
}

// Alternating entry points preserve explicit pressure ping-pong ordering on the CPU
@compute @workgroup_size(64)
fn propagate_cellular_pressure_a(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    propagate_cellular_pressure(
        logical_cell_index_from_active_pressure_workgroup(workgroup.x, local_index), true,
    );
}

// Runs the second ping-pong direction of one pressure propagation step
@compute @workgroup_size(64)
fn propagate_cellular_pressure_b(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    let logical_index: u32 = logical_cell_index_from_active_pressure_workgroup(
        workgroup.x, local_index);
    if logical_index < parameters.buffered_cell_count {
        let index: u32 = cellular_pressure_physical_cell_index_from_world_cell(
            cellular_pressure_world_cell_from_logical_index(logical_index));
        if index != INVALID_PHYSICAL_CELL_INDEX { pending_pressure[index] = vec4<f32>(0.0); }
    }
    propagate_cellular_pressure(
        logical_index, false,
    );
}

@compute @workgroup_size(64)
fn finalize_cellular_pressure(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    let logical_index: u32 = logical_cell_index_from_active_pressure_workgroup(
        workgroup.x, local_index);
    if logical_index >= parameters.buffered_cell_count { return; }
    let cell: vec2<i32> = cellular_pressure_world_cell_from_logical_index(logical_index);
    let index: u32 = cellular_pressure_physical_cell_index_from_world_cell(cell);
    if index == INVALID_PHYSICAL_CELL_INDEX { return; }
    let current: vec4<f32> = pressure_b[index];
    var load: vec4<f32> = retained_pressure[index];
    for (var channel: u32 = 0u; channel < 4u; channel++) {
        load[channel] += current[channel] -
            calculate_outgoing_cellular_pressure(cell, channel, current[channel]);
        load[channel] += gather_incoming_cellular_pressure(cell, channel, false);
    }
    let material: u32 = cellular_material_identifiers[index];
    if material_form_from_identifier(material) == CELLULAR_STATIC_MATERIAL_FORM {
        apply_cellular_static_pressure_damage(cell, index, material, load);
    }
    pressure_a[index] = vec4<f32>(0.0);
    pressure_b[index] = vec4<f32>(0.0);
    retained_pressure[index] = vec4<f32>(0.0);
    pending_pressure[index] = vec4<f32>(0.0);
}

// Applies all retained pressure after the fixed propagation budget has been consumed
@compute @workgroup_size(64)
fn apply_retained_cellular_pressure(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    let logical_index: u32 = logical_cell_index_from_active_pressure_workgroup(workgroup.x, local_index);
    if logical_index >= parameters.buffered_cell_count { return; }
    let cell: vec2<i32> = cellular_pressure_world_cell_from_logical_index(logical_index);
    let index: u32 = cellular_pressure_physical_cell_index_from_world_cell(cell);
    if index == INVALID_PHYSICAL_CELL_INDEX { return; }
    // Six propagation passes finish in A; the remaining in-flight pressure is retained here
    let load: vec4<f32> = retained_pressure[index] + pressure_a[index];
    let material: u32 = cellular_material_identifiers[index];
    let form: u32 = material_form_from_identifier(material);
    if form == CELLULAR_STATIC_MATERIAL_FORM {
        apply_cellular_static_pressure_damage(cell, index, material, load);
    }
    pressure_a[index] = vec4<f32>(0.0);
    pressure_b[index] = vec4<f32>(0.0);
    retained_pressure[index] = vec4<f32>(0.0);
}

// Damages or fractures one static cell using scalar compression from all four channels
fn apply_cellular_static_pressure_damage(
    cell: vec2<i32>,
    index: u32,
    material: u32,
    load: vec4<f32>,
) {
    let properties: StaticProperties =
        cellular_static_properties[material_index_from_identifier(material)];
    let compression: f32 = load.x + load.y + load.z + load.w;
    let overload: f32 = compression - properties.pressure_ignore_threshold;
    if overload <= 0.0 { return; }
    cellular_integrities[index] -= overload * parameters.delta_time * parameters.damage_rate;
    if cellular_integrities[index] > 0.0 { return; }
    if properties.debris != EMPTY_MATERIAL_IDENTIFIER &&
            cellular_fracture_yield_random_from_world_cell(cell, parameters.tick) <
                properties.debris_yield_rate {
        cellular_material_identifiers[index] = properties.debris;
        cellular_kinematics[index] = vec4<f32>(0.0);
    } else {
        cellular_material_identifiers[index] = EMPTY_MATERIAL_IDENTIFIER;
        cellular_appearances[index] = EMPTY_MATERIAL_IDENTIFIER;
        cellular_kinematics[index] = vec4<f32>(0.0);
    }
    cellular_integrities[index] = 0.0;
}

// Retains pressure that the source material or blocked stencil routes cannot transmit
fn propagate_cellular_pressure(logical_index: u32, read_pressure_a: bool) {
    if logical_index >= parameters.buffered_cell_count { return; }
    let cell: vec2<i32> = cellular_pressure_world_cell_from_logical_index(logical_index);
    let index: u32 = cellular_pressure_physical_cell_index_from_world_cell(cell);
    if index == INVALID_PHYSICAL_CELL_INDEX { return; }
    let current: vec4<f32> = select(pressure_b[index], pressure_a[index], read_pressure_a);
    var local_retained: vec4<f32> = vec4<f32>(0.0);
    for (var channel: u32 = 0u; channel < 4u; channel++) {
        local_retained[channel] = current[channel] -
            calculate_outgoing_cellular_pressure(cell, channel, current[channel]);
    }
    retained_pressure[index] += local_retained;
    var gathered: vec4<f32> = vec4<f32>(0.0);
    for (var channel: u32 = 0u; channel < 4u; channel++) {
        gathered[channel] = gather_incoming_cellular_pressure(
            cell, channel, read_pressure_a,
        );
    }
    if read_pressure_a {
        pressure_b[index] = gathered;
    } else {
        pressure_a[index] = gathered;
    }
}

// Calculates the fraction that actually leaves one source through its fixed stencil
fn calculate_outgoing_cellular_pressure(cell: vec2<i32>, channel: u32, value: f32) -> f32 {
    let source_transmission: f32 = cellular_pressure_transmission_at_physical_cell_index(
        cellular_pressure_physical_cell_index_from_world_cell(cell),
    );
    if source_transmission <= 0.0 { return 0.0; }
    var transmitted: f32 = 0.0;
    for (var side: i32 = -1; side <= 1; side++) {
        let destination_cell: vec2<i32> = cell + world_cell_direction_from_pressure_channel(channel) +
            world_cell_side_offset_from_pressure_channel(channel, side);
        let destination: u32 = cellular_pressure_physical_cell_index_from_world_cell(destination_cell);
        if destination != INVALID_PHYSICAL_CELL_INDEX &&
                cellular_pressure_transmission_at_physical_cell_index(destination) > 0.0 {
            let weight: f32 = select(0.2, 0.6, side == 0);
            transmitted += value * min(source_transmission,
                cellular_pressure_transmission_at_physical_cell_index(destination)) * weight;
        }
    }
    return transmitted;
}

// Gathers only valid source-routed pressure into one destination cell
fn gather_incoming_cellular_pressure(
    cell: vec2<i32>,
    channel: u32,
    read_pressure_a: bool,
) -> f32 {
    if cellular_pressure_transmission_at_physical_cell_index(
            cellular_pressure_physical_cell_index_from_world_cell(cell)) <= 0.0 { return 0.0; }
    var gathered: f32 = 0.0;
    for (var side: i32 = -1; side <= 1; side++) {
        let source_cell: vec2<i32> = cell - world_cell_direction_from_pressure_channel(channel) -
            world_cell_side_offset_from_pressure_channel(channel, side);
        let source: u32 = cellular_pressure_physical_cell_index_from_world_cell(source_cell);
        if source == INVALID_PHYSICAL_CELL_INDEX { continue; }
        let source_transmission: f32 =
            cellular_pressure_transmission_at_physical_cell_index(source);
        if source_transmission <= 0.0 { continue; }
        let source_pressure: f32 = select(
            pressure_b[source][channel],
            pressure_a[source][channel],
            read_pressure_a,
        );
        let weight: f32 = select(0.2, 0.6, side == 0);
        gathered += source_pressure * min(source_transmission,
            cellular_pressure_transmission_at_physical_cell_index(
                cellular_pressure_physical_cell_index_from_world_cell(cell))) * weight;
    }
    return gathered;
}

fn pending_pressure_source(index: u32) -> vec4<f32> {
    var source: vec4<f32> = pending_pressure[index];
    if external_body_occupancy[index] == 1u || external_body_occupancy[index] == 2u {
        source += encode_directional_pressure(external_body_velocity[index].zw);
    }
    return source;
}

fn gather_pending_cellular_pressure(cell: vec2<i32>, channel: u32) -> f32 {
    let destination: u32 = cellular_pressure_physical_cell_index_from_world_cell(cell);
    let destination_transmission: f32 =
        cellular_pressure_transmission_at_physical_cell_index(destination);
    if destination_transmission <= 0.0 { return 0.0; }
    var gathered: f32 = 0.0;
    for (var side: i32 = -1; side <= 1; side++) {
        let source_cell: vec2<i32> = cell - world_cell_direction_from_pressure_channel(channel) -
            world_cell_side_offset_from_pressure_channel(channel, side);
        let source: u32 = cellular_pressure_physical_cell_index_from_world_cell(source_cell);
        if source == INVALID_PHYSICAL_CELL_INDEX { continue; }
        let source_transmission: f32 =
            cellular_pressure_transmission_at_physical_cell_index(source);
        if source_transmission <= 0.0 { continue; }
        let weight: f32 = select(0.2, 0.6, side == 0);
        gathered += pending_pressure_source(source)[channel] *
            min(source_transmission, destination_transmission) * weight;
    }
    return gathered;
}

// Returns source-material transmission, with transient body cells effectively perfect
fn cellular_pressure_transmission_at_physical_cell_index(index: u32) -> f32 {
    if index == INVALID_PHYSICAL_CELL_INDEX { return 0.0; }
    if external_body_occupancy[index] != 0u { return 1.0; }
    if mechanical_fluid_cells[index].mass > 0.0 {
        return fluid_pressure_properties[material_index_from_identifier(
            mechanical_fluid_cells[index].material_identifier)].x;
    }
    let material: u32 = cellular_material_identifiers[index];
    let form: u32 = material_form_from_identifier(material);
    if form == CELLULAR_STATIC_MATERIAL_FORM {
        return cellular_static_properties[
            material_index_from_identifier(material)
        ].pressure_transmission;
    }
    if form == CELLULAR_DYNAMIC_MATERIAL_FORM {
        return cellular_dynamic_properties[material_index_from_identifier(material)].y;
    }
    return 0.0;
}

// Returns the friction coefficient for one cellular contact medium
fn cellular_contact_friction_at_physical_cell_index(index: u32) -> f32 {
    if index == INVALID_PHYSICAL_CELL_INDEX || external_body_occupancy[index] != 0u { return 0.0; }
    let material: u32 = cellular_material_identifiers[index];
    let form: u32 = material_form_from_identifier(material);
    if form == CELLULAR_STATIC_MATERIAL_FORM {
        return cellular_static_properties[material_index_from_identifier(material)].friction;
    }
    if form == CELLULAR_DYNAMIC_MATERIAL_FORM {
        return cellular_dynamic_properties[material_index_from_identifier(material)].z;
    }
    return 0.0;
}

// Returns the restitution coefficient for one cellular contact medium
fn cellular_material_restitution_at_physical_cell_index(index: u32) -> f32 {
    if index == INVALID_PHYSICAL_CELL_INDEX || external_body_occupancy[index] != 0u { return 0.0; }
    let material: u32 = cellular_material_identifiers[index];
    let form: u32 = material_form_from_identifier(material);
    if form == CELLULAR_STATIC_MATERIAL_FORM {
        return cellular_static_properties[material_index_from_identifier(material)].restitution;
    }
    if form == CELLULAR_DYNAMIC_MATERIAL_FORM {
        return cellular_dynamic_properties[material_index_from_identifier(material)].w;
    }
    return 0.0;
}

// An immovable body uses the cellular material's restitution rather than suppressing bounce
fn cellular_contact_restitution(first_index: u32, second_index: u32) -> f32 {
    if external_body_occupancy[first_index] != 0u {
        return cellular_material_restitution_at_physical_cell_index(second_index);
    }
    if external_body_occupancy[second_index] != 0u {
        return cellular_material_restitution_at_physical_cell_index(first_index);
    }
    return min(
        cellular_material_restitution_at_physical_cell_index(first_index),
        cellular_material_restitution_at_physical_cell_index(second_index),
    );
}

// Returns dynamic mass or the immovable mass used for static and proxy cells
fn cellular_contact_mass_at_physical_cell_index(index: u32) -> f32 {
    if index != INVALID_PHYSICAL_CELL_INDEX &&
            material_form_from_identifier(cellular_material_identifiers[index]) ==
                CELLULAR_DYNAMIC_MATERIAL_FORM &&
            external_body_occupancy[index] == 0u {
        return cellular_dynamic_properties[
            material_index_from_identifier(cellular_material_identifiers[index])
        ].x;
    }
    return IMMOVABLE_CONTACT_MASS;
}

// Channel order is +X, -X, +Y, -Y so opposing compression cannot cancel
fn encode_directional_pressure(value: vec2<f32>) -> vec4<f32> {
    return vec4<f32>(
        max(value.x, 0.0),
        max(-value.x, 0.0),
        max(value.y, 0.0),
        max(-value.y, 0.0),
    );
}

fn find_rigid_static_contact(source: u32) -> RigidStaticContact {
    if source >= parameters.rigid_cell_count { return empty_rigid_static_contact(); }
    let cell: vec4<u32> = rigid_cells[source * 2u];
    let body: u32 = cell.z;
    if body >= parameters.rigid_body_count { return empty_rigid_static_contact(); }
    let pose: vec4<f32> = rigid_transforms[body * 3u];
    let motion: vec4<f32> = rigid_transforms[body * 3u + 1u];
    let local_center: vec2<f32> =
        (vec2<f32>(bitcast<vec2<i32>>(cell.xy)) + vec2<f32>(0.5)) * CELL_SIZE;
    let axis_x: vec2<f32> = pose.zw;
    let axis_y: vec2<f32> = vec2<f32>(-axis_x.y, axis_x.x);
    let current: vec2<f32> = pose.xy + axis_x * local_center.x + axis_y * local_center.y;
    let angle_step: f32 = clamp(motion.z * parameters.delta_time, -3.14159265, 3.14159265);
    let previous_axis_x: vec2<f32> = vec2<f32>(
        axis_x.x * cos(angle_step) + axis_x.y * sin(angle_step),
        -axis_x.x * sin(angle_step) + axis_x.y * cos(angle_step));
    let previous_axis_y: vec2<f32> = vec2<f32>(-previous_axis_x.y, previous_axis_x.x);
    let previous_unbounded: vec2<f32> = pose.xy - motion.xy * parameters.delta_time +
        previous_axis_x * local_center.x + previous_axis_y * local_center.y;
    let sweep: vec2<f32> = current - previous_unbounded;
    let sweep_scale: f32 = min(1.0, 2.0 / max(length(sweep), 0.0001));
    let previous: vec2<f32> = current - sweep * sweep_scale;
    let rotational_expansion: f32 = min(CELL_SIZE * 2.0,
        abs(angle_step) * length(local_center));
    let extent: f32 = CELL_RADIUS + rotational_expansion;
    let minimum: vec2<i32> = vec2<i32>(floor(
        (min(previous, current) - vec2<f32>(extent)) * 8.0));
    let maximum: vec2<i32> = vec2<i32>(floor(
        (max(previous, current) + vec2<f32>(extent)) * 8.0));
    var best: RigidStaticContact = empty_rigid_static_contact();
    var best_time: f32 = 2.0;
    for (var y: i32 = minimum.y; y <= maximum.y; y++) {
        for (var x: i32 = minimum.x; x <= maximum.x; x++) {
            let world_cell: vec2<i32> = vec2<i32>(x, y);
            let index: u32 = cellular_pressure_physical_cell_index_from_world_cell(world_cell);
            if index == INVALID_PHYSICAL_CELL_INDEX { continue; }
            let material: u32 = cellular_material_identifiers[index];
            if material_form_from_identifier(material) != CELLULAR_STATIC_MATERIAL_FORM {
                continue;
            }
            let static_center: vec2<f32> =
                (vec2<f32>(world_cell) + vec2<f32>(0.5)) * CELL_SIZE;
            let overlap: vec4<f32> = rigid_static_overlap_contact(
                current, axis_x, axis_y, static_center);
            var hit: vec4<f32> = overlap;
            var time: f32 = 0.0;
            if overlap.w < 0.0 {
                hit = rigid_static_swept_contact(previous, current, static_center, extent);
                time = hit.z;
            }
            if hit.w >= 0.0 && time < best_time {
                best_time = time;
                let normal: vec2<f32> = hit.xy;
                let point_center: vec2<f32> = mix(previous, current, time);
                best = RigidStaticContact(1u, body, material,
                    dominant_cardinal_channel(normal), index, normal,
                    point_center - normal * CELL_RADIUS, hit.w);
            }
        }
    }
    return best;
}

fn rigid_static_overlap_contact(
    center: vec2<f32>, axis_x: vec2<f32>, axis_y: vec2<f32>,
    static_center: vec2<f32>,
) -> vec4<f32> {
    let difference: vec2<f32> = center - static_center;
    var best_axis: vec2<f32> = vec2<f32>(0.0);
    var best_penetration: f32 = 3.402823e+38;
    let axes: array<vec2<f32>, 4> = array<vec2<f32>, 4>(
        vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0), axis_x, axis_y);
    for (var index: u32 = 0u; index < 4u; index++) {
        let axis: vec2<f32> = axes[index];
        let rigid_radius: f32 = CELL_HALF *
            (abs(dot(axis_x, axis)) + abs(dot(axis_y, axis)));
        let static_radius: f32 = CELL_HALF * (abs(axis.x) + abs(axis.y));
        let penetration: f32 = rigid_radius + static_radius - abs(dot(difference, axis));
        if penetration < 0.0 { return vec4<f32>(0.0, 0.0, 0.0, -1.0); }
        if penetration < best_penetration {
            best_penetration = penetration;
            best_axis = select(-axis, axis, dot(difference, axis) >= 0.0);
        }
    }
    return vec4<f32>(best_axis, 0.0, best_penetration);
}

fn rigid_static_swept_contact(
    start: vec2<f32>, finish: vec2<f32>, static_center: vec2<f32>, extent: f32,
) -> vec4<f32> {
    let movement: vec2<f32> = finish - start;
    let minimum: vec2<f32> = static_center - vec2<f32>(CELL_HALF + extent);
    let maximum: vec2<f32> = static_center + vec2<f32>(CELL_HALF + extent);
    var enter: f32 = 0.0;
    var leave: f32 = 1.0;
    var normal: vec2<f32> = vec2<f32>(0.0);
    for (var axis: u32 = 0u; axis < 2u; axis++) {
        if abs(movement[axis]) <= 0.0001 {
            if start[axis] < minimum[axis] || start[axis] > maximum[axis] {
                return vec4<f32>(0.0, 0.0, 2.0, -1.0);
            }
            continue;
        }
        let first: f32 = (minimum[axis] - start[axis]) / movement[axis];
        let second: f32 = (maximum[axis] - start[axis]) / movement[axis];
        let axis_enter: f32 = min(first, second);
        let axis_leave: f32 = max(first, second);
        if axis_enter > enter {
            enter = axis_enter;
            normal = vec2<f32>(0.0);
            normal[axis] = select(1.0, -1.0, movement[axis] > 0.0);
        }
        leave = min(leave, axis_leave);
    }
    if enter > leave || enter < 0.0 || enter > 1.0 {
        return vec4<f32>(0.0, 0.0, 2.0, -1.0);
    }
    return vec4<f32>(normal, enter, 0.0);
}

fn empty_rigid_static_contact() -> RigidStaticContact {
    return RigidStaticContact(0u, 0u, 0u, 0u, 0u,
        vec2<f32>(0.0), vec2<f32>(0.0), -1.0);
}

fn accumulate_rigid_sweep_reaction(body: u32, impulse: vec2<f32>, radius: vec2<f32>) {
    let torque: f32 = radius.x * impulse.y - radius.y * impulse.x;
    if any(impulse != impulse) || any(abs(impulse) > vec2<f32>(1000000.0)) ||
            abs(torque) > 1000000.0 {
        atomicStore(&rigid_reactions[body].overflow, 1);
        return;
    }
    var overflowed: bool = saturating_rigid_atomic_add(body,
        i32(round(impulse.x * LINEAR_FIXED_SCALE)), 0u);
    overflowed = saturating_rigid_atomic_add(body,
        i32(round(impulse.y * LINEAR_FIXED_SCALE)), 1u) || overflowed;
    overflowed = saturating_rigid_atomic_add(body,
        i32(round(torque * ANGULAR_FIXED_SCALE)), 2u) || overflowed;
    if overflowed { atomicStore(&rigid_reactions[body].overflow, 1); }
}

fn dominant_cardinal_channel(normal: vec2<f32>) -> u32 {
    if abs(normal.x) >= abs(normal.y) { return select(1u, 0u, normal.x >= 0.0); }
    return select(3u, 2u, normal.y >= 0.0);
}

fn saturating_rigid_atomic_add(body: u32, value: i32, accumulator: u32) -> bool {
    let maximum: i32 = bitcast<i32>(0x7fffffffu);
    let minimum: i32 = bitcast<i32>(0x80000000u);
    var old: i32 = 0;
    if accumulator == 0u { old = atomicLoad(&rigid_reactions[body].impulse_x); }
    else if accumulator == 1u { old = atomicLoad(&rigid_reactions[body].impulse_y); }
    else { old = atomicLoad(&rigid_reactions[body].angular_impulse); }
    loop {
        var next: i32 = 0;
        var overflowed: bool = false;
        if value > 0 && old > maximum - value {
            next = maximum;
            overflowed = true;
        } else if value < 0 && old < minimum - value {
            next = minimum;
            overflowed = true;
        } else {
            next = old + value;
        }
        if accumulator == 0u {
            let result = atomicCompareExchangeWeak(&rigid_reactions[body].impulse_x, old, next);
            if result.exchanged { return overflowed; }
            old = result.old_value;
        } else if accumulator == 1u {
            let result = atomicCompareExchangeWeak(&rigid_reactions[body].impulse_y, old, next);
            if result.exchanged { return overflowed; }
            old = result.old_value;
        } else {
            let result = atomicCompareExchangeWeak(
                &rigid_reactions[body].angular_impulse, old, next);
            if result.exchanged { return overflowed; }
            old = result.old_value;
        }
    }
    return false;
}

// Returns the world-cell direction represented by one pressure channel
fn world_cell_direction_from_pressure_channel(channel: u32) -> vec2<i32> {
    if channel == 0u { return vec2<i32>(1, 0); }
    if channel == 1u { return vec2<i32>(-1, 0); }
    if channel == 2u { return vec2<i32>(0, 1); }
    return vec2<i32>(0, -1);
}

// Returns the lateral stencil offset for one directional pressure channel
fn world_cell_side_offset_from_pressure_channel(channel: u32, side: i32) -> vec2<i32> {
    if channel < 2u { return vec2<i32>(0, side); }
    return vec2<i32>(side, 0);
}

// Converts logical tile-major dispatch order into a signed world cell
fn cellular_pressure_world_cell_from_logical_index(logical_index: u32) -> vec2<i32> {
    return world_cell_from_logical_tile_major_index(
        logical_index, parameters.buffered_origin, parameters.buffered_tile_size,
    );
}

// Maps one signed world cell through the two-dimensional physical tile ring
fn cellular_pressure_physical_cell_index_from_world_cell(world_cell: vec2<i32>) -> u32 {
    return physical_cell_index_from_world_cell(
        world_cell, parameters.buffered_origin, parameters.buffered_tile_size,
        parameters.ring_offset,
    );
}

// Maps one compacted active tile workgroup to its tile-major logical cell
fn logical_cell_index_from_active_pressure_workgroup(workgroup: u32, local_index: u32) -> u32 {
    return active_pressure_tile_indices[workgroup] * CELL_COUNT_PER_TILE + local_index;
}

// Produces a deterministic per-cell random value for fracture yield decisions
fn cellular_fracture_yield_random_from_world_cell(cell: vec2<i32>, tick: u32) -> f32 {
    var value: u32 = u32(cell.x) * 0x9e3779b9u ^
        u32(cell.y) * 0x85ebca6bu ^ tick;
    value ^= value >> 16u;
    return f32(value) / 4294967295.0;
}
