fn find_rigid_static_contact(source: u32) -> RigidStaticContact {
    if source >= parameters.rigid_cell_count {
        return empty_rigid_static_contact();
    }
    let cell: vec4<u32> = rigid_cells[source * 2u];
    let body: u32 = cell.z;
    if body >= parameters.rigid_body_count {
        return empty_rigid_static_contact();
    }
    let pose: vec4<f32> = rigid_transforms[body * 3u];
    let motion: vec4<f32> = rigid_transforms[body * 3u + 1u];
    let local_center: vec2<f32> = (vec2<f32>(bitcast<vec2<i32>>(cell.xy)) + vec2<f32>(0.5)) * CELL_SIZE;
    let axis_x: vec2<f32> = pose.zw;
    let axis_y: vec2<f32> = vec2<f32>(-axis_x.y, axis_x.x);
    let current: vec2<f32> = pose.xy + axis_x * local_center.x + axis_y * local_center.y;
    let angle_step: f32 = clamp(motion.z * parameters.delta_time, -3.14159265, 3.14159265);
    let previous_axis_x: vec2<f32> = vec2<f32>(
      axis_x.x * cos(angle_step) + axis_x.y * sin(angle_step),
      -axis_x.x * sin(angle_step) + axis_x.y * cos(angle_step));
    let previous_axis_y: vec2<f32> = vec2<f32>(-previous_axis_x.y, previous_axis_x.x);
    let previous_unbounded: vec2<f32> = pose.xy - motion.xy * parameters.delta_time + previous_axis_x * local_center.x + previous_axis_y * local_center.y;
    let sweep: vec2<f32> = current - previous_unbounded;
    let sweep_scale: f32 = min(1.0, 2.0 / max(length(sweep), 0.0001));
    let previous: vec2<f32> = current - sweep * sweep_scale;
    let rotational_expansion: f32 = min(CELL_SIZE * 2.0, abs(angle_step) * length(local_center));
    let extent: f32 = CELL_RADIUS + rotational_expansion;
    let minimum: vec2<i32> = vec2<i32>(floor((min(previous, current) - vec2<f32>(extent)) * 8.0));
    let maximum: vec2<i32> = vec2<i32>(floor((max(previous, current) + vec2<f32>(extent)) * 8.0));
    var best: RigidStaticContact = empty_rigid_static_contact();
    var best_time: f32 = 2.0;
    var best_overlap: RigidStaticContact = empty_rigid_static_contact();
    var best_blocking: f32 = -1.0;
    let mass_record: vec4<f32> = rigid_transforms[body * 3u + 2u];
    let radius: vec2<f32> = current - mass_record.xy;
    let inward_motion: vec2<f32> = motion.xy + motion.z * vec2<f32>(-radius.y, radius.x) + parameters.gravity * parameters.delta_time;
    for (var y: i32 = minimum.y; y <= maximum.y; y++) {
        for (var x: i32 = minimum.x; x <= maximum.x; x++) {
            let world_cell: vec2<i32> = vec2<i32>(x, y);
            let index: u32 = cellular_pressure_physical_cell_index_from_world_cell(world_cell);
            if index == INVALID_PHYSICAL_CELL_INDEX {
                continue;
            }
            let material: u32 = cellular_material_identifiers[index];
            if material_form_from_identifier(material) != CELLULAR_STATIC_MATERIAL_FORM {
                continue;
            }
            let static_center: vec2<f32> = (vec2<f32>(world_cell) + vec2<f32>(0.5)) * CELL_SIZE;
            let overlap: vec4<f32> = rigid_static_overlap_contact(current, axis_x, axis_y, static_center);
            if overlap.w >= 0.0 {
                let normal: vec2<f32> = overlap.xy;
                let blocking: f32 = max(0.0, -dot(inward_motion, normal));
                if blocking > best_blocking || (blocking == best_blocking && overlap.w > best_overlap.penetration) {
                    best_blocking = blocking;
                    best_overlap = RigidStaticContact(
              1u,
              body,
              material,
              dominant_cardinal_channel(normal),
              index,
              normal,
              current - axis_x * sign(dot(axis_x, normal)) * CELL_HALF - axis_y * sign(dot(axis_y, normal)) * CELL_HALF,
              overlap.w);
                }
                continue;
            }
            let hit: vec4<f32> = rigid_static_swept_contact(previous, current, static_center, extent);
            let time: f32 = hit.z;
            if hit.w >= 0.0 && time < best_time {
                best_time = time;
                let normal: vec2<f32> = hit.xy;
                let point_center: vec2<f32> = mix(previous, current, time);
                best = RigidStaticContact(
            1u,
            body,
            material,
            dominant_cardinal_channel(normal),
            index,
            normal,
            point_center - axis_x * sign(dot(axis_x, normal)) * CELL_HALF - axis_y * sign(dot(axis_y, normal)) * CELL_HALF,
            hit.w);
            }
        }
    }
    if best_overlap.found != 0u {
        return best_overlap;
    }
    return best;
}

fn rigid_static_overlap_contact(
    center: vec2<f32>,
    axis_x: vec2<f32>,
    axis_y: vec2<f32>,
    static_center: vec2<f32>,
) -> vec4<f32> {
    let difference: vec2<f32> = center - static_center;
    var best_axis: vec2<f32> = vec2<f32>(0.0);
    var best_penetration: f32 = 3.402823e+38;
    let axes: array<vec2<f32>, 4> = array<vec2<f32>, 4>(
      vec2<f32>(1.0, 0.0),
      vec2<f32>(0.0, 1.0),
      axis_x,
      axis_y);
    for (var index: u32 = 0u; index < 4u; index++) {
        let axis: vec2<f32> = axes[index];
        let rigid_radius: f32 = CELL_HALF * (abs(dot(axis_x, axis)) + abs(dot(axis_y, axis)));
        let static_radius: f32 = CELL_HALF * (abs(axis.x) + abs(axis.y));
        let penetration: f32 = rigid_radius + static_radius - abs(dot(difference, axis));
        if penetration < 0.0 {
            return vec4<f32>(0.0, 0.0, 0.0, -1.0);
        }
        if penetration < best_penetration {
            best_penetration = penetration;
            best_axis = select(-axis, axis, dot(difference, axis) >= 0.0);
        }
    }
    return vec4<f32>(best_axis, 0.0, best_penetration);
}

fn rigid_static_swept_contact(
    start: vec2<f32>,
    finish: vec2<f32>,
    static_center: vec2<f32>,
    extent: f32,
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
    return
    RigidStaticContact(
      0u,
      0u,
      0u,
      0u,
      0u,
      vec2<f32>(0.0),
      vec2<f32>(0.0),
      -1.0);
}

fn accumulate_rigid_sweep_reaction(
    body: u32,
    impulse: vec2<f32>,
    radius: vec2<f32>
) {
    let torque: f32 = radius.x * impulse.y - radius.y * impulse.x;
    if any(impulse != impulse) || any(abs(impulse) > vec2<f32>(1000000.0)) || abs(torque) > 1000000.0 {
        atomicStore(&rigid_reactions[body].overflow, 1);
        return;
    }
    var overflowed: bool = saturating_rigid_atomic_add(
      body,
      i32(round(impulse.x * LINEAR_FIXED_SCALE)),
      0u);
    overflowed = saturating_rigid_atomic_add(
      body,
      i32(round(impulse.y * LINEAR_FIXED_SCALE)),
      1u) || overflowed;
    overflowed = saturating_rigid_atomic_add(
      body,
      i32(round(torque * ANGULAR_FIXED_SCALE)),
      2u) || overflowed;
    if overflowed {
        atomicStore(&rigid_reactions[body].overflow, 1);
    }
}

fn dominant_cardinal_channel(normal: vec2<f32>) -> u32 {
    if abs(normal.x) >= abs(normal.y) {
        return select(1u, 0u, normal.x >= 0.0);
    }
    return select(3u, 2u, normal.y >= 0.0);
}

fn saturating_rigid_atomic_add(
    body: u32,
    value: i32,
    accumulator: u32
) -> bool {
    let maximum: i32 = bitcast<i32>(0x7fffffffu);
    let minimum: i32 = bitcast<i32>(0x80000000u);
    var old: i32 = 0;
    if accumulator == 0u {
        old = atomicLoad(&rigid_reactions[body].impulse_x);
    } else if accumulator == 1u {
        old = atomicLoad(&rigid_reactions[body].impulse_y);
    } else {
        old = atomicLoad(&rigid_reactions[body].angular_impulse);
    }
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
                if result.exchanged {
                    return overflowed;
                }
                old = result.old_value;
            } else if accumulator == 1u {
                let result = atomicCompareExchangeWeak(&rigid_reactions[body].impulse_y, old, next);
                if result.exchanged {
                    return overflowed;
                }
                old = result.old_value;
            } else {
                let result = atomicCompareExchangeWeak(
          &rigid_reactions[body].angular_impulse,
          old,
          next);
                if result.exchanged {
                    return overflowed;
                }
                old = result.old_value;
            }
        }
    return false;
}

