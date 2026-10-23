// Copyright Rob Gage 2026

@group(0) @binding(16)
var<storage, read_write> fluid_raster: array<atomic<u32>>;

@group(0) @binding(17)
var<storage, read> fluid_materials: array<vec4<f32>>;

/// First record word of a rigid body's wet-face fluid mass, momentum, and moments
const RIGID_BODY_FLUID_CURRENT_WORD: u32 = 14u;

/// Fixed-point units per fluid mass and momentum value in the shared raster
const FLUID_RASTER_SCALE: f32 = 4096.0;

/// Significant adjacent fluid speed that wakes a resting dynamic-cell tile
const FLUID_CURRENT_WAKE_SPEED: f32 = 0.1;

/// Smallest fluid speed into a solid face, in cells per tick, that produces a contact impulse
const FLUID_CONTACT_APPROACH_MINIMUM: f32 = 0.0001;

/// Record word counting a rigid body's fluid contact faces during the current tick
const RIGID_BODY_FLUID_FACE_WORD: u32 = 13u;

/// Cells past a rigid body's edge searched for the fluid that submerges a row or column
const FLUID_RIGID_BODY_SURFACE_REACH: u32 = 3u;

/// Returns mean fluid immersion, mean dominant-fluid density, and local velocity
fn sample_neighboring_fluid(cell: vec2<i32>) -> vec4<f32> {
    let offsets: array<vec2<i32>, 4> = array<vec2<i32>, 4>(
        vec2<i32>(-1, 0),
        vec2<i32>(1, 0),
        vec2<i32>(0, -1),
        vec2<i32>(0, 1),
    );
    var neighbor_materials: array<u32, 4>;
    var neighbor_masses: array<f32, 4>;
    var occupied_count: u32 = 0u;
    var total_mass: f32 = 0.0;
    var total_momentum: vec2<f32> = vec2<f32>(0.0);
    for (var neighbor_index: u32 = 0u; neighbor_index < 4u; neighbor_index++) {
        let neighbor: vec2<i32> = cell + offsets[neighbor_index];
        if !is_cell_inside(neighbor) { continue; }
        let raster_index: u32 = cell_index(neighbor) * 4u;
        let mass: f32 = max(
            f32(bitcast<i32>(atomicLoad(&fluid_raster[raster_index]))) / FLUID_RASTER_SCALE,
            0.0,
        );
        if mass <= 0.0 { continue; }
        let material_identifier: u32 = atomicLoad(&fluid_raster[raster_index + 3u]);
        let fluid_density: f32 = fluid_materials[(material_identifier & 0x3fffffffu) * 2u].x;
        if fluid_density <= 0.0 { continue; }
        let momentum: vec2<f32> = vec2<f32>(
            f32(bitcast<i32>(atomicLoad(&fluid_raster[raster_index + 1u]))),
            f32(bitcast<i32>(atomicLoad(&fluid_raster[raster_index + 2u]))),
        ) / FLUID_RASTER_SCALE;
        neighbor_materials[occupied_count] = material_identifier;
        neighbor_masses[occupied_count] = mass;
        occupied_count += 1u;
        total_mass += mass;
        total_momentum += momentum;
    }
    if occupied_count == 0u || total_mass <= 0.0 { return vec4<f32>(0.0); }
    var dominant_material: u32 = neighbor_materials[0];
    var dominant_mass: f32 = 0.0;
    for (var candidate: u32 = 0u; candidate < occupied_count; candidate++) {
        var candidate_mass: f32 = 0.0;
        for (var neighbor: u32 = 0u; neighbor < occupied_count; neighbor++) {
            if neighbor_materials[neighbor] == neighbor_materials[candidate] {
                candidate_mass += neighbor_masses[neighbor];
            }
        }
        if candidate_mass > dominant_mass {
            dominant_mass = candidate_mass;
            dominant_material = neighbor_materials[candidate];
        }
    }
    let dominant_density: f32 = fluid_materials[(dominant_material & 0x3fffffffu) * 2u].x;
    let immersion: f32 = clamp(total_mass * 0.25 / dominant_density, 0.0, 1.0);
    return vec4<f32>(immersion, dominant_density, total_momentum / total_mass);
}

/// Returns the first raster word of the per-cell fluid velocity changes from solid contacts
fn fluid_contact_offset() -> u32 {
    let tile_count: u32 = uniforms.region_width * uniforms.region_height;
    return tile_count * 64u * 4u + tile_count * 2u + 1u;
}

/// Returns a fluid raster cell's mass and mass-weighted velocity, with zero mass when it is empty
fn fluid_raster_cell(index: u32) -> vec3<f32> {
    let raster_index: u32 = index * 4u;
    let mass: f32 = max(
        f32(bitcast<i32>(atomicLoad(&fluid_raster[raster_index]))) / FLUID_RASTER_SCALE,
        0.0,
    );
    if mass <= 0.0 { return vec3<f32>(0.0); }
    let momentum: vec2<f32> = vec2<f32>(
        f32(bitcast<i32>(atomicLoad(&fluid_raster[raster_index + 1u]))),
        f32(bitcast<i32>(atomicLoad(&fluid_raster[raster_index + 2u]))),
    ) / FLUID_RASTER_SCALE;
    return vec3<f32>(mass, momentum / mass);
}

/// Returns the friction and restitution of the fluid material in a raster cell
fn fluid_raster_contact_material(index: u32) -> vec2<f32> {
    let material_identifier: u32 = atomicLoad(&fluid_raster[index * 4u + 3u]);
    return fluid_materials[(material_identifier & 0x3fffffffu) * 2u + 1u].yz;
}

/// Returns how many of a fluid cell's four faces touch matter, so contacts share its mass
fn fluid_contact_face_count(cell: vec2<i32>) -> f32 {
    let offsets: array<vec2<i32>, 4> = array<vec2<i32>, 4>(
        vec2<i32>(-1, 0),
        vec2<i32>(1, 0),
        vec2<i32>(0, -1),
        vec2<i32>(0, 1),
    );
    var count: f32 = 0.0;
    for (var neighbor_index: u32 = 0u; neighbor_index < 4u; neighbor_index++) {
        let neighbor: vec2<i32> = cell + offsets[neighbor_index];
        if !is_cell_inside(neighbor) { continue; }
        let index: u32 = cell_index(neighbor);
        if material_identifiers[index] != 0u || rigid_body_cells[index] != 0u { count += 1.0; }
    }
    return max(count, 1.0);
}

/// Adds a fluid velocity change in cells per tick to a raster cell for the next fluid step
fn add_fluid_contact_velocity(index: u32, velocity_change: vec2<f32>) {
    let fixed: vec2<i32> = vec2<i32>(round(clamp(
        velocity_change * FLUID_RASTER_SCALE,
        vec2<f32>(-1.0e9),
        vec2<f32>(1.0e9),
    )));
    let base: u32 = fluid_contact_offset() + index * 2u;
    atomicAdd(&fluid_raster[base], bitcast<u32>(fixed.x));
    atomicAdd(&fluid_raster[base + 1u], bitcast<u32>(fixed.y));
}

/// Returns the impulse a fluid cell applies to a solid across one face
///
/// The normal points from the fluid into the solid; inverse masses of the fluid share and the
/// solid set the effective mass, and the tangential part is limited by Coulomb friction.
fn fluid_face_impulse(
    fluid_velocity: vec2<f32>,
    solid_velocity: vec2<f32>,
    normal: vec2<f32>,
    fluid_inverse_mass: f32,
    solid_inverse_mass: f32,
    friction: f32,
    restitution: f32,
) -> vec2<f32> {
    let relative: vec2<f32> = fluid_velocity - solid_velocity;
    let approach: f32 = dot(relative, normal);
    if approach <= FLUID_CONTACT_APPROACH_MINIMUM { return vec2<f32>(0.0); }
    let inverse_mass: f32 = fluid_inverse_mass + solid_inverse_mass;
    let normal_impulse: f32 = (1.0 + restitution) * approach / inverse_mass;
    let tangent: vec2<f32> = vec2<f32>(-normal.y, normal.x);
    let tangent_impulse: f32 = clamp(
        dot(relative, tangent) / inverse_mass,
        -friction * normal_impulse,
        friction * normal_impulse,
    );
    return normal * normal_impulse + tangent * tangent_impulse;
}

/// Returns the density of the fluid a ray through one rigid body exits into, or zero when the
/// first cells past the body hold no sufficiently dense fluid
fn rigid_cell_submerged(cell: vec2<i32>, direction: vec2<i32>, rigid_body: u32) -> f32 {
    var sample_cell: vec2<i32> = cell;
    var exit_step: u32 = 0u;
    for (var step: u32 = 0u; step < 32u; step++) {
        sample_cell += direction;
        if !is_cell_inside(sample_cell) { return 0.0; }
        let index: u32 = cell_index(sample_cell);
        if exit_step == 0u && cell_rigid_body(index) == rigid_body { continue; }
        // the solver keeps particles about a cell from walls, so look a little past the body
        exit_step += 1u;
        if exit_step > FLUID_RIGID_BODY_SURFACE_REACH || material_identifiers[index] != 0u {
            return 0.0;
        }
        let raster_index: u32 = index * 4u;
        let mass: f32 = max(
            f32(bitcast<i32>(atomicLoad(&fluid_raster[raster_index]))) / FLUID_RASTER_SCALE,
            0.0,
        );
        let material_identifier: u32 = atomicLoad(&fluid_raster[raster_index + 3u]);
        let density: f32 = fluid_materials[(material_identifier & 0x3fffffffu) * 2u].x;
        if density > 0.0 && mass >= density * 0.5 { return density; }
    }
    return 0.0;
}

/// Rigid body contact words one tile's cells sum before adding them to their body: the linear
/// and angular impulse and the seven wet-face moments
const RIGID_FLUID_SUM_COUNT: u32 = 10u;

/// Marks a tile whose cells belong to no rigid body
const RIGID_FLUID_NO_BODY: u32 = 0xffffffffu;

/// Lowest rigid body index among the cells of the tile this workgroup visits
var<workgroup> rigid_fluid_body: atomic<u32>;

/// Selected body copied for a workgroup-uniform read
var<workgroup> rigid_fluid_selected_body: u32;

/// Per-cell contributions of the tile's lowest-index body, reduced to the first lane
var<workgroup> rigid_fluid_sums: array<array<f32, 64>, 10>;

/// Returns the contact word of the body record that one tile sum adds to
fn rigid_fluid_sum_word(sum: u32) -> u32 {
    if sum < 3u { return sum; }
    return RIGID_BODY_FLUID_CURRENT_WORD + sum - 3u;
}

/// Returns the logical cell of one invocation of a dispatch over every physical tile
fn rigid_fluid_cell(invocation: u32) -> vec2<i32> {
    let physical_tile: u32 = invocation / 64u;
    let local: u32 = invocation % 64u;
    let physical_x: u32 = physical_tile % uniforms.region_width;
    let physical_y: u32 = physical_tile / uniforms.region_width;
    let logical_tile: vec2<u32> = vec2<u32>(
        (physical_x + uniforms.region_width - uniforms.ring_offset_x) % uniforms.region_width,
        (physical_y + uniforms.region_height - uniforms.ring_offset_y) % uniforms.region_height,
    );
    return vec2<i32>(logical_tile * 8u + vec2<u32>(local % 8u, local / 8u));
}

/// Returns the rigid body index of one invocation's cell, or `RIGID_FLUID_NO_BODY`
fn rigid_fluid_body_index(invocation: u32) -> u32 {
    let cell_count: u32 = uniforms.region_width * uniforms.region_height * 64u;
    if invocation >= cell_count { return RIGID_FLUID_NO_BODY; }
    let rigid_body_cell: u32 = rigid_body_cells[cell_index(rigid_fluid_cell(invocation))];
    if rigid_body_cell == 0u { return RIGID_FLUID_NO_BODY; }
    return rigid_body_cellular_tiles[(rigid_body_cell - 1u) / 64u].rigid_body;
}

/// Finds the tile's lowest rigid body index, so that its cells sum in workgroup memory and
/// reach the body's record in one atomic add per word instead of one per cell
fn rigid_fluid_select_body(local_index: u32, body_index: u32) -> u32 {
    if local_index == 0u { atomicStore(&rigid_fluid_body, RIGID_FLUID_NO_BODY); }
    workgroupBarrier();
    if body_index != RIGID_FLUID_NO_BODY { atomicMin(&rigid_fluid_body, body_index); }
    workgroupBarrier();
    if local_index == 0u { rigid_fluid_selected_body = atomicLoad(&rigid_fluid_body); }
    return workgroupUniformLoad(&rigid_fluid_selected_body);
}

/// Adds one cell's contributions to its body: through the tile sums for the tile's selected
/// body, or directly for a second body sharing the tile, then adds the tile sums
fn rigid_fluid_add_sums(
    local_index: u32,
    body_index: u32,
    selected_body: u32,
    sums: array<f32, 10>,
    sum_count: u32,
) {
    for (var sum: u32 = 0u; sum < sum_count; sum++) {
        var value: f32 = 0.0;
        if body_index == selected_body {
            value = sums[sum];
        } else if body_index != RIGID_FLUID_NO_BODY && sums[sum] != 0.0 {
            add_rigid_body_contact(
                body_index * RIGID_BODY_CONTACT_STRIDE + rigid_fluid_sum_word(sum),
                sums[sum],
            );
        }
        rigid_fluid_sums[sum][local_index] = value;
    }
    workgroupBarrier();
    for (var stride: u32 = 32u; stride > 0u; stride = stride / 2u) {
        if local_index < stride {
            for (var sum: u32 = 0u; sum < sum_count; sum++) {
                rigid_fluid_sums[sum][local_index] += rigid_fluid_sums[sum][local_index + stride];
            }
        }
        workgroupBarrier();
    }
    if local_index < sum_count && selected_body != RIGID_FLUID_NO_BODY {
        let total: f32 = rigid_fluid_sums[local_index][0];
        if total != 0.0 {
            add_rigid_body_contact(
                selected_body * RIGID_BODY_CONTACT_STRIDE + rigid_fluid_sum_word(local_index),
                total,
            );
        }
    }
}

/// Applies raster-sampled buoyancy and drag to each rigid body cell, including sleeping tiles,
/// with one workgroup per physical tile
@compute @workgroup_size(64)
fn rigid_fluid(
    @builtin(global_invocation_id) invocation: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    let body_index: u32 = rigid_fluid_body_index(invocation.x);
    let selected_body: u32 = rigid_fluid_select_body(local_index, body_index);
    // a tile without rigid body cells adds nothing
    if selected_body == RIGID_FLUID_NO_BODY { return; }
    var sums: array<f32, 10>;
    if body_index != RIGID_FLUID_NO_BODY {
        let cell: vec2<i32> = rigid_fluid_cell(invocation.x);
        let rigid_body: CellularTileRigidBody = rigid_bodies[body_index];
        let arm: vec2<f32> = rigid_body_arm(body_index, cell);
        let body_velocity: vec2<f32> = rigid_body.velocity
            + rigid_body.angular_velocity * vec2<f32>(-arm.y, arm.x);
        // rows are submerged by fluid beside them, not by water a rising body carries on top
        let displaced_density: f32 = max(
            rigid_cell_submerged(cell, vec2<i32>(-1, 0), body_index + 1u),
            rigid_cell_submerged(cell, vec2<i32>(1, 0), body_index + 1u),
        );
        let impulse_y: f32 = uniforms.gravity * displaced_density;
        sums[1] = impulse_y;
        sums[2] = arm.x * impulse_y;
        // every fluid face of the body shares its mass, as when the body takes all hits at once
        let contact_index: u32 = body_index * RIGID_BODY_CONTACT_STRIDE;
        let face_count: f32 = max(
            bitcast<f32>(atomicLoad(
                &rigid_body_contacts[contact_index + RIGID_BODY_FLUID_FACE_WORD],
            )),
            1.0,
        );
        let solid_inverse_mass: f32 = face_count / max(rigid_body.mass, 0.000001);
        let offsets: array<vec2<i32>, 4> = array<vec2<i32>, 4>(
            vec2<i32>(-1, 0),
            vec2<i32>(1, 0),
            vec2<i32>(0, -1),
            vec2<i32>(0, 1),
        );
        for (var neighbor_index: u32 = 0u; neighbor_index < 4u; neighbor_index++) {
            let neighbor: vec2<i32> = cell + offsets[neighbor_index];
            if !is_cell_inside(neighbor) { continue; }
            let fluid_index: u32 = cell_index(neighbor);
            if material_identifiers[fluid_index] != 0u || rigid_body_cells[fluid_index] != 0u {
                continue;
            }
            let fluid: vec3<f32> = fluid_raster_cell(fluid_index);
            if fluid.x <= 0.0 { continue; }
            let fluid_inverse_mass: f32 = fluid_contact_face_count(neighbor) / fluid.x;
            let contact_material: vec2<f32> = fluid_raster_contact_material(fluid_index);
            let face_impulse: vec2<f32> = fluid_face_impulse(
                fluid.yz,
                body_velocity,
                -vec2<f32>(offsets[neighbor_index]),
                fluid_inverse_mass,
                solid_inverse_mass,
                contact_material.x,
                contact_material.y,
            );
            add_fluid_contact_velocity(fluid_index, -face_impulse * fluid_inverse_mass);
            // the body relaxes toward the wet faces' current on the CPU with its own newest
            // velocity, because an impulse from this tick's velocity would arrive late and
            // overshoot
            let face_mass: f32 = fluid.x / fluid_contact_face_count(neighbor);
            let fluid_arm: vec2<f32> = arm + vec2<f32>(offsets[neighbor_index]);
            sums[3] += face_mass;
            sums[4] += face_mass * fluid.y;
            sums[5] += face_mass * fluid.z;
            sums[6] += face_mass * (fluid_arm.x * fluid.z - fluid_arm.y * fluid.y);
            sums[7] += face_mass * dot(fluid_arm, fluid_arm);
            sums[8] += face_mass * fluid_arm.x;
            sums[9] += face_mass * fluid_arm.y;
        }
    }
    rigid_fluid_add_sums(local_index, body_index, selected_body, sums, RIGID_FLUID_SUM_COUNT);
}

/// Counts each rigid body's fluid contact faces before `rigid_fluid` shares its mass over them,
/// with one workgroup per physical tile
@compute @workgroup_size(64)
fn rigid_fluid_faces(
    @builtin(global_invocation_id) invocation: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    let body_index: u32 = rigid_fluid_body_index(invocation.x);
    let selected_body: u32 = rigid_fluid_select_body(local_index, body_index);
    if selected_body == RIGID_FLUID_NO_BODY { return; }
    var sums: array<f32, 10>;
    if body_index != RIGID_FLUID_NO_BODY {
        let cell: vec2<i32> = rigid_fluid_cell(invocation.x);
        let offsets: array<vec2<i32>, 4> = array<vec2<i32>, 4>(
            vec2<i32>(-1, 0),
            vec2<i32>(1, 0),
            vec2<i32>(0, -1),
            vec2<i32>(0, 1),
        );
        for (var neighbor_index: u32 = 0u; neighbor_index < 4u; neighbor_index++) {
            let neighbor: vec2<i32> = cell + offsets[neighbor_index];
            if !is_cell_inside(neighbor) { continue; }
            let fluid_index: u32 = cell_index(neighbor);
            if material_identifiers[fluid_index] != 0u || rigid_body_cells[fluid_index] != 0u {
                continue;
            }
            if fluid_raster_cell(fluid_index).x > 0.0 { sums[0] += 1.0; }
        }
    }
    // the face count is the body record's face word, which the first sum slot does not name
    if body_index != RIGID_FLUID_NO_BODY && body_index != selected_body && sums[0] != 0.0 {
        add_rigid_body_contact(
            body_index * RIGID_BODY_CONTACT_STRIDE + RIGID_BODY_FLUID_FACE_WORD,
            sums[0],
        );
    }
    var value: f32 = 0.0;
    if body_index == selected_body { value = sums[0]; }
    rigid_fluid_sums[0][local_index] = value;
    workgroupBarrier();
    for (var stride: u32 = 32u; stride > 0u; stride = stride / 2u) {
        if local_index < stride {
            rigid_fluid_sums[0][local_index] += rigid_fluid_sums[0][local_index + stride];
        }
        workgroupBarrier();
    }
    if local_index == 0u && rigid_fluid_sums[0][0] != 0.0 {
        add_rigid_body_contact(
            selected_body * RIGID_BODY_CONTACT_STRIDE + RIGID_BODY_FLUID_FACE_WORD,
            rigid_fluid_sums[0][0],
        );
    }
}

/// Returns whether a tile holds dynamic cells next to fluid or a significant fluid current
fn tile_has_fluid_interaction(tile: vec2<i32>) -> bool {
    for (var local_y: i32 = 0; local_y < 8; local_y++) {
        for (var local_x: i32 = 0; local_x < 8; local_x++) {
            let cell: vec2<i32> = tile * 8 + vec2<i32>(local_x, local_y);
            if !is_dynamic(material_identifiers[cell_index(cell)]) { continue; }
            let fluid: vec4<f32> = sample_neighboring_fluid(cell);
            if fluid.x > 0.0 || length(fluid.zw) >= FLUID_CURRENT_WAKE_SPEED { return true; }
        }
    }
    return false;
}

