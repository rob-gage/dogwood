// Copyright Rob Gage 2026

/// One cascade level: probes `spacing` lighting texels apart, each tracing `direction_count` intervals from
/// `interval_start` to `interval_end` lighting texels, merged with the next coarser level
struct CascadeLevel {
    /// Lighting domain size in texels
    domain_size: vec2<u32>,
    /// Number of probes along each axis
    probe_count: vec2<u32>,
    /// Domain position of the first probe's center in lighting texels
    probe_origin: vec2<f32>,
    /// Distance between neighboring probes in lighting texels
    spacing: f32,
    /// Number of evenly spaced directions traced from each probe
    direction_count: u32,
    /// Distance from a probe where its intervals start in lighting texels
    interval_start: f32,
    /// Distance from a probe where its intervals end in lighting texels
    interval_end: f32,
    /// Number of probes along each axis of the coarser level
    upper_probe_count: vec2<u32>,
    /// Domain position of the coarser level's first probe center in lighting texels
    upper_probe_origin: vec2<f32>,
    /// Distance between neighboring probes of the coarser level in lighting texels
    upper_spacing: f32,
    /// Whether a coarser level exists to merge with
    has_upper: u32,
    /// Number of consecutive invocations that split each ray of one interval between them
    lanes: u32,
    /// Cells along each side of a lighting texel
    texel_size: f32,
};

@group(0) @binding(0)
var<uniform> level: CascadeLevel;

/// Linear emission per lighting texel of path in `rgb` and extinction per lighting texel in `a`
@group(0) @binding(1)
var optical: texture_2d<f32>;

/// The shared optical value of every uniform block of 8x8 cells, or a negative extinction
@group(0) @binding(5)
var blocks: texture_2d<f32>;

/// Merged radiance of the coarser level, packed as two `pack2x16float` words per interval. The
/// spare upper half of an interval's second word flags that its radiance changed this frame.
@group(0) @binding(2)
var<storage, read> upper_intervals: array<vec2<u32>>;

/// Merged radiance of this level, packed like `upper_intervals`, kept between frames
@group(0) @binding(3)
var<storage, read_write> intervals: array<vec2<u32>>;

/// Fluence of every domain cell, written by `integrate` as `pack2x16float` red and green, then
/// blue
@group(0) @binding(4)
var<storage, read_write> fluence: array<vec2<u32>>;

/// Cache words built by the cache pass: 2 is nonzero when every interval must be recomputed, and
/// 4 to 7 hold the bounds of the changed optical cells as the domain width minus their minimum
/// x, their exclusive maximum x, the domain height minus their minimum y and their exclusive
/// maximum y, zero when none changed
@group(0) @binding(6)
var<storage, read> cache_state: array<u32>;

/// Bits of an interval's second word holding its change flag
const CHANGED: u32 = 0xffff0000u;

/// Extra cells around the changed optical cells. A changed cell can change the classification
/// of its whole block of 8x8 cells, and with it how a ray crossing the block is grouped, and one
/// more cell covers the rounding of block boundary crossings.
const CHANGE_MARGIN: f32 = 9.0;

const TAU: f32 = 6.283185307179586;

/// The radiance and transmission each invocation of a workgroup traced along its ray segment
var<workgroup> segments: array<vec4<f32>, 64>;

/// Least radiance a ray segment gathers once its whole ray can no longer be clear, even after an
/// earlier segment passing at least `CLEAR_TRANSMISSION` of the light
const LIT_RADIANCE: f32 = 1.00011e-6;

/// Least transmission of a ray that crosses nothing
const CLEAR_TRANSMISSION: f32 = 0.9999;

/// Returns the radiance gathered along a ray and the fraction of light passing through it,
/// crossing uniform blocks of 8x8 cells in one step. A ray that `stops_when_lit` ends as soon as
/// it gathers light or loses transmission, because only whether it crossed nothing matters.
fn trace(origin: vec2<f32>, direction: vec2<f32>, start: f32, end: f32, stops_when_lit: bool) -> vec4<f32> {
    let size: vec2<f32> = vec2<f32>(level.domain_size);
    let is_moving: vec2<bool> = abs(direction) > vec2<f32>(1e-9);
    let inverse: vec2<f32> = 1.0 / select(vec2<f32>(1.0), direction, is_moving);
    // clip the interval to the domain, outside which nothing emits or occludes
    let near: vec2<f32> = select(vec2<f32>(-1e30), (vec2<f32>(0.0) - origin) * inverse, is_moving);
    let far: vec2<f32> = select(vec2<f32>(1e30), (size - origin) * inverse, is_moving);
    var distance: f32 = max(start, max(min(near.x, far.x), min(near.y, far.y)));
    let exit: f32 = min(end, min(max(near.x, far.x), max(near.y, far.y)));
    var radiance: vec3<f32> = vec3<f32>(0.0);
    var transmission: f32 = 1.0;
    let is_positive: vec2<bool> = direction >= vec2<f32>(0.0);
    let maximum_cell: vec2<i32> = vec2<i32>(level.domain_size) - vec2<i32>(1);
    for (var step: u32 = 0u; step < 8192u && distance < exit; step++) {
        let position: vec2<f32> = origin + direction * (distance + 1e-4);
        let cell: vec2<i32> = clamp(vec2<i32>(floor(position)), vec2<i32>(0), maximum_cell);
        let block: vec2<i32> = cell / 8;
        let block_value: vec4<f32> = textureLoad(blocks, block, 0);
        var corner: vec2<f32> = vec2<f32>(cell);
        var extent: f32 = 1.0;
        var sample: vec4<f32> = block_value;
        if block_value.a >= 0.0 {
            corner = vec2<f32>(block * 8);
            extent = 8.0;
        } else {
            sample = textureLoad(optical, cell, 0);
        }
        let boundary: vec2<f32> = corner + select(vec2<f32>(0.0), vec2<f32>(extent), is_positive);
        let crossing: vec2<f32> = select(vec2<f32>(1e30), (boundary - origin) * inverse, is_moving);
        let next: f32 = max(min(min(crossing.x, crossing.y), exit), distance + 1e-4);
        let length: f32 = next - distance;
        if sample.a == 0.0 && all(sample.rgb == vec3<f32>(0.0)) {
            distance = next;
            continue;
        }
        let visibility: f32 = exp(-sample.a * length);
        // an optically thick cell emits its radiance from its surface, and a thin one along
        // its path
        var path: f32 = length;
        if sample.a > 1e-4 * level.texel_size {
            // Both coefficients are per lighting texel. The surface-emission cutoff is
            // one extinction per world cell, independent of the lighting resolution.
            path = (1.0 - visibility) / min(sample.a, level.texel_size);
        }
        radiance += transmission * sample.rgb * path;
        transmission *= visibility;
        distance = next;
        if transmission < 1e-3 {
            transmission = 0.0;
            break;
        }
        if stops_when_lit && (transmission < CLEAR_TRANSMISSION || any(radiance >= vec3<f32>(LIT_RADIANCE))) {
            break;
        }
    }
    return vec4<f32>(radiance, transmission);
}

/// Traces a ray split into equal segments across the `level.lanes` invocations sharing one
/// interval, then composes the segments in order. Every invocation of the workgroup must call it.
fn trace_lanes(
    local: u32,
    origin: vec2<f32>,
    direction: vec2<f32>,
    start: f32,
    end: f32,
    stops_when_lit: bool,
) -> vec4<f32> {
    // With one invocation per ray there is no cross-lane data to exchange. The single
    // segment is exactly [start, end], so shared storage and two workgroup barriers add no work
    // to the result. This uniform branch preserves the same trace and arithmetic.
    if level.lanes == 1u {
        return trace(origin, direction, start, end, stops_when_lit);
    }
    let lane: u32 = local % level.lanes;
    let share: f32 = (end - start) / f32(level.lanes);
    let segment_start: f32 = start + share * f32(lane);
    let segment_end: f32 = select(segment_start + share, end, lane == level.lanes - 1u);
    segments[local] = trace(origin, direction, segment_start, segment_end, stops_when_lit);
    workgroupBarrier();
    var radiance: vec3<f32> = vec3<f32>(0.0);
    var transmission: f32 = 1.0;
    for (var segment: u32 = local - lane; segment <= local - lane + level.lanes - 1u; segment++) {
        radiance += transmission * segments[segment].rgb;
        transmission *= segments[segment].a;
        if transmission < 1e-3 {
            transmission = 0.0;
            break;
        }
    }
    workgroupBarrier();
    return vec4<f32>(radiance, transmission);
}

/// Unpacks one merged interval's radiance
fn unpack_interval(packed: vec2<u32>) -> vec3<f32> {
    return vec3<f32>(unpack2x16float(packed.x), unpack2x16float(packed.y).x);
}

/// Returns the radiance arriving at a domain position along a direction through one interval,
/// merged with the coarser level. Each of the four surrounding coarser probes gets its own ray,
/// from where this interval starts to where that probe's interval starts, so occluders between
/// the probes do not leak light across. Every invocation of the workgroup must call it; one whose
/// result is unused passes `is_traced` false to trace nothing while it takes part in the barriers.
// Barrier-free equivalent of the merge below. Keep ray/interpolation math in sync; the
// separate function lets WGSL prove that its per-invocation early return skips no barriers.
fn merged_radiance_single(local: u32, position: vec2<f32>, direction: u32, is_traced: bool) -> vec3<f32> {
    // No result is consumed for an unchanged interval. This helper contains no barriers.
    if !is_traced { return vec3<f32>(0.0); }
    let angle: f32 = (f32(direction) + 0.5) * TAU / f32(level.direction_count);
    let ray: vec2<f32> = vec2<f32>(cos(angle), sin(angle));
    let interval_end: f32 = select(level.interval_start, level.interval_end, is_traced);
    if level.has_upper == 0u {
        return trace(position, ray, level.interval_start, interval_end, false).rgb;
    }
    let start: vec2<f32> = position + ray * level.interval_start;
    let upper_direction_count: u32 = level.direction_count * 2u;
    // where the interval crosses nothing, the four rays would see nothing either, so one ray and
    // interpolated coarser probes suffice
    let plain: vec4<f32> = trace(position, ray, level.interval_start, interval_end, true);
    let is_clear: bool = plain.a >= CLEAR_TRANSMISSION && all(plain.rgb <= vec3<f32>(1e-6));
    let lattice: vec2<f32> = (position - level.upper_probe_origin) / level.upper_spacing;
    let maximum: vec2<i32> = max(vec2<i32>(level.upper_probe_count) - vec2<i32>(2), vec2<i32>(0));
    let base: vec2<i32> = clamp(vec2<i32>(floor(lattice)), vec2<i32>(0), maximum);
    let weight: vec2<f32> = clamp(lattice - vec2<f32>(base), vec2<f32>(0.0), vec2<f32>(1.0));
    var radiance: vec3<f32> = vec3<f32>(0.0);
    for (var corner: u32 = 0u; corner < 4u; corner++) {
        let offset: vec2<i32> = vec2<i32>(i32(corner & 1u), i32(corner >> 1u));
        let corner_weight: f32 = select(1.0 - weight.x, weight.x, offset.x == 1)
            * select(1.0 - weight.y, weight.y, offset.y == 1);
        let probe: vec2<u32> = vec2<u32>(min(base + offset, vec2<i32>(level.upper_probe_count) - vec2<i32>(1)));
        if corner_weight <= 0.0 {
            continue;
        }
        var near: vec4<f32> = plain;
        // Single-lane invocations have no barriers to attend for unused corner rays.
        if !is_clear {
            let upper_position: vec2<f32> = level.upper_probe_origin + vec2<f32>(probe) * level.upper_spacing;
            let segment: vec2<f32> = upper_position + ray * level.interval_end - start;
            let length: f32 = max(length(segment), 1e-6);
            near = trace(start, segment / length, 0.0, length, false);
        }
        var corner_radiance: vec3<f32> = near.rgb;
        if near.a > 0.0 {
            let first: u32 = (probe.y * level.upper_probe_count.x + probe.x) * upper_direction_count
                + direction * 2u;
            corner_radiance += near.a * 0.5
                * (unpack_interval(upper_intervals[first]) + unpack_interval(upper_intervals[first + 1u]));
        }
        radiance += corner_weight * corner_radiance;
    }
    return radiance;
}


fn merged_radiance(local: u32, position: vec2<f32>, direction: u32, is_traced: bool) -> vec3<f32> {
    // The lane count is uniform across this dispatch. The barrier-free implementation can
    // return immediately for unchanged intervals instead of executing discarded ray setup.
    if level.lanes == 1u { return merged_radiance_single(local, position, direction, is_traced); }
    let angle: f32 = (f32(direction) + 0.5) * TAU / f32(level.direction_count);
    let ray: vec2<f32> = vec2<f32>(cos(angle), sin(angle));
    let interval_end: f32 = select(level.interval_start, level.interval_end, is_traced);
    if level.has_upper == 0u {
        return trace_lanes(local, position, ray, level.interval_start, interval_end, false).rgb;
    }
    let start: vec2<f32> = position + ray * level.interval_start;
    let upper_direction_count: u32 = level.direction_count * 2u;
    // where the interval crosses nothing, the four rays would see nothing either, so one ray and
    // interpolated coarser probes suffice
    let plain: vec4<f32> = trace_lanes(local, position, ray, level.interval_start, interval_end, true);
    let is_clear: bool = plain.a >= CLEAR_TRANSMISSION && all(plain.rgb <= vec3<f32>(1e-6));
    let lattice: vec2<f32> = (position - level.upper_probe_origin) / level.upper_spacing;
    let maximum: vec2<i32> = max(vec2<i32>(level.upper_probe_count) - vec2<i32>(2), vec2<i32>(0));
    let base: vec2<i32> = clamp(vec2<i32>(floor(lattice)), vec2<i32>(0), maximum);
    let weight: vec2<f32> = clamp(lattice - vec2<f32>(base), vec2<f32>(0.0), vec2<f32>(1.0));
    var radiance: vec3<f32> = vec3<f32>(0.0);
    for (var corner: u32 = 0u; corner < 4u; corner++) {
        let offset: vec2<i32> = vec2<i32>(i32(corner & 1u), i32(corner >> 1u));
        let corner_weight: f32 = select(1.0 - weight.x, weight.x, offset.x == 1)
            * select(1.0 - weight.y, weight.y, offset.y == 1);
        let probe: vec2<u32> = vec2<u32>(min(base + offset, vec2<i32>(level.upper_probe_count) - vec2<i32>(1)));
        let upper_position: vec2<f32> = level.upper_probe_origin + vec2<f32>(probe) * level.upper_spacing;
        // corners that do not contribute trace nothing, but still take part in the lane barriers
        let segment: vec2<f32> = upper_position + ray * level.interval_end - start;
        let length: f32 = max(length(segment), 1e-6);
        let is_corner_traced: bool = is_traced && !is_clear && corner_weight > 0.0;
        var near: vec4<f32> = trace_lanes(
            local, start, segment / length, 0.0, select(0.0, length, is_corner_traced), false,
        );
        if is_clear {
            near = plain;
        }
        if corner_weight <= 0.0 {
            continue;
        }
        var corner_radiance: vec3<f32> = near.rgb;
        if near.a > 0.0 {
            let first: u32 = (probe.y * level.upper_probe_count.x + probe.x) * upper_direction_count
                + direction * 2u;
            corner_radiance += near.a * 0.5
                * (unpack_interval(upper_intervals[first]) + unpack_interval(upper_intervals[first + 1u]));
        }
        radiance += corner_weight * corner_radiance;
    }
    return radiance;
}

/// Returns whether anything one interval depends on changed: the forced flag, the changed optical
/// cells near the interval or the coarser intervals it merges with, or any of those intervals
fn interval_inputs_changed(position: vec2<f32>, direction: u32) -> bool {
    if cache_state[2] != 0u {
        return true;
    }
    let angle: f32 = (f32(direction) + 0.5) * TAU / f32(level.direction_count);
    let ray: vec2<f32> = vec2<f32>(cos(angle), sin(angle));
    let start: vec2<f32> = position + ray * level.interval_start;
    let end: vec2<f32> = position + ray * level.interval_end;
    var low: vec2<f32> = min(start, end);
    var high: vec2<f32> = max(start, end);
    var base: vec2<i32> = vec2<i32>(0);
    if level.has_upper != 0u {
        let lattice: vec2<f32> = (position - level.upper_probe_origin) / level.upper_spacing;
        let maximum: vec2<i32> = max(vec2<i32>(level.upper_probe_count) - vec2<i32>(2), vec2<i32>(0));
        base = clamp(vec2<i32>(floor(lattice)), vec2<i32>(0), maximum);
        let upper_low: vec2<f32> = level.upper_probe_origin
            + vec2<f32>(base) * level.upper_spacing + ray * level.interval_end;
        let upper_high: vec2<f32> = level.upper_probe_origin
            + vec2<f32>(min(base + vec2<i32>(1), vec2<i32>(level.upper_probe_count) - vec2<i32>(1)))
                * level.upper_spacing
            + ray * level.interval_end;
        low = min(low, upper_low);
        high = max(high, upper_high);
    }
    if cache_state[5] != 0u && cache_state[7] != 0u {
        let dirty_low: vec2<f32> = vec2<f32>(level.domain_size - vec2<u32>(cache_state[4], cache_state[6]));
        let dirty_high: vec2<f32> = vec2<f32>(vec2<u32>(cache_state[5], cache_state[7]));
        if all(high + vec2<f32>(CHANGE_MARGIN) >= dirty_low)
            && all(low - vec2<f32>(CHANGE_MARGIN) <= dirty_high) {
            if !LOCAL_DIRTY || local_optical_changes(low-vec2<f32>(CHANGE_MARGIN),high+vec2<f32>(CHANGE_MARGIN)) {return true;}
        }
    }
    if level.has_upper != 0u {
        for (var corner: u32 = 0u; corner < 4u; corner++) {
            let offset: vec2<i32> = vec2<i32>(i32(corner & 1u), i32(corner >> 1u));
            let probe: vec2<u32> = vec2<u32>(min(base + offset, vec2<i32>(level.upper_probe_count) - vec2<i32>(1)));
            let first: u32 = (probe.y * level.upper_probe_count.x + probe.x) * level.direction_count * 2u
                + direction * 2u;
            if ((upper_intervals[first].y | upper_intervals[first + 1u].y) & CHANGED) != 0u {
                return true;
            }
        }
    }
    return false;
}

/// Returns whether anything one cell's fluence depends on changed. The finest level integrates
/// every direction, so the first interval of each coarser probe summarizes its changes.
fn cell_inputs_changed(position: vec2<f32>) -> bool {
    if cache_state[2] != 0u {
        return true;
    }
    var low: vec2<f32> = position;
    var high: vec2<f32> = position;
    var base: vec2<i32> = vec2<i32>(0);
    if level.has_upper != 0u {
        let lattice: vec2<f32> = (position - level.upper_probe_origin) / level.upper_spacing;
        let maximum: vec2<i32> = max(vec2<i32>(level.upper_probe_count) - vec2<i32>(2), vec2<i32>(0));
        base = clamp(vec2<i32>(floor(lattice)), vec2<i32>(0), maximum);
        low = min(low, level.upper_probe_origin + vec2<f32>(base) * level.upper_spacing);
        high = max(high, level.upper_probe_origin
            + vec2<f32>(min(base + vec2<i32>(1), vec2<i32>(level.upper_probe_count) - vec2<i32>(1)))
                * level.upper_spacing);
    }
    let margin: vec2<f32> = vec2<f32>(level.interval_end + CHANGE_MARGIN);
    if cache_state[5] != 0u && cache_state[7] != 0u {
        let dirty_low: vec2<f32> = vec2<f32>(level.domain_size - vec2<u32>(cache_state[4], cache_state[6]));
        let dirty_high: vec2<f32> = vec2<f32>(vec2<u32>(cache_state[5], cache_state[7]));
        if all(high + margin >= dirty_low) && all(low - margin <= dirty_high) {
            if !LOCAL_DIRTY || local_optical_changes(low-margin,high+margin) {return true;}
        }
    }
    if level.has_upper != 0u {
        for (var corner: u32 = 0u; corner < 4u; corner++) {
            let offset: vec2<i32> = vec2<i32>(i32(corner & 1u), i32(corner >> 1u));
            let probe: vec2<u32> = vec2<u32>(min(base + offset, vec2<i32>(level.upper_probe_count) - vec2<i32>(1)));
            let first: u32 = (probe.y * level.upper_probe_count.x + probe.x) * level.direction_count * 2u;
            if (upper_intervals[first].y & CHANGED) != 0u {
                return true;
            }
        }
    }
    return false;
}

/// Reduces the change flags of each probe's intervals to its first interval's spare bits, run
/// for the level the finest level merges with
@compute @workgroup_size(64, 1, 1)
fn summarize(
    @builtin(global_invocation_id) invocation: vec3<u32>,
    @builtin(num_workgroups) workgroups: vec3<u32>,
) {
    let probe_index: u32 = invocation.x + invocation.y * workgroups.x * 64u;
    if probe_index >= level.probe_count.x * level.probe_count.y {
        return;
    }
    let first: u32 = probe_index * level.direction_count;
    var changed: u32 = 0u;
    for (var direction: u32 = 0u; direction < level.direction_count; direction++) {
        changed |= intervals[first + direction].y & CHANGED;
    }
    intervals[first].y = (intervals[first].y & ~CHANGED) | changed;
}

/// Traces one probe interval of a level and merges it with the coarser level, unless nothing it
/// depends on changed, and flags whether its radiance changed
@compute @workgroup_size(64, 1, 1)
fn trace_merge(
    @builtin(global_invocation_id) invocation: vec3<u32>,
    @builtin(local_invocation_index) local: u32,
    @builtin(num_workgroups) workgroups: vec3<u32>,
) {
    let interval_total: u32 = level.probe_count.x * level.probe_count.y * level.direction_count;
    let index: u32 = (invocation.x + invocation.y * workgroups.x * 64u) / level.lanes;
    // invocations past the last interval trace a clamped one to keep the lane barriers uniform
    let traced: u32 = min(index, interval_total - 1u);
    let direction: u32 = traced % level.direction_count;
    let probe_index: u32 = traced / level.direction_count;
    let probe: vec2<u32> = vec2<u32>(probe_index % level.probe_count.x, probe_index / level.probe_count.x);
    let position: vec2<f32> = level.probe_origin + vec2<f32>(probe) * level.spacing;
    let is_changed: bool = index < interval_total && interval_inputs_changed(position, direction);
    let radiance: vec3<f32> = merged_radiance(local, position, direction, is_changed);
    if index >= interval_total || local % level.lanes != 0u {
        return;
    }
    if !is_changed {
        intervals[index].y = intervals[index].y & ~CHANGED;
        return;
    }
    let packed: vec2<u32> = vec2<u32>(pack2x16float(radiance.rg), pack2x16float(vec2<f32>(radiance.b, 0.0)));
    let previous: vec2<u32> = vec2<u32>(intervals[index].x, intervals[index].y & ~CHANGED);
    intervals[index] = vec2<u32>(packed.x, packed.y | select(0u, 0x10000u, any(packed != previous)));
}

/// Single-invocation rays need neither clamped dummy work nor barrier participation.
@compute @workgroup_size(64, 1, 1)
fn trace_merge_single(
    @builtin(global_invocation_id) invocation: vec3<u32>,
    @builtin(local_invocation_index) local: u32,
    @builtin(num_workgroups) workgroups: vec3<u32>,
) {
    let interval_total: u32 = level.probe_count.x * level.probe_count.y * level.direction_count;
    let index: u32 = invocation.x + invocation.y * workgroups.x * 64u;
    if index >= interval_total { return; }
    let direction: u32 = index % level.direction_count;
    let probe_index: u32 = index / level.direction_count;
    let probe: vec2<u32> = vec2<u32>(probe_index % level.probe_count.x, probe_index / level.probe_count.x);
    let position: vec2<f32> = level.probe_origin + vec2<f32>(probe) * level.spacing;
    if !interval_inputs_changed(position, direction) {
        intervals[index].y &= ~CHANGED;
        return;
    }
    let radiance: vec3<f32> = merged_radiance_single(local, position, direction, true);
    let packed: vec2<u32> = vec2<u32>(pack2x16float(radiance.rg), pack2x16float(vec2<f32>(radiance.b, 0.0)));
    let previous: vec2<u32> = vec2<u32>(intervals[index].x, intervals[index].y & ~CHANGED);
    intervals[index] = vec2<u32>(packed.x, packed.y | select(0u, 0x10000u, any(packed != previous)));
}

/// Gathers the fluence of every cell from its finest intervals merged with the stored levels,
/// keeping the previous fluence of cells where nothing changed
@compute @workgroup_size(8, 8, 1)
fn integrate(
    @builtin(global_invocation_id) invocation: vec3<u32>,
    @builtin(local_invocation_index) local: u32,
) {
    // The finest cascade always has one lane: this entry point has no barriers.
    if any(invocation.xy >= level.domain_size) { return; }
    let traced: vec2<u32> = invocation.xy;
    let position: vec2<f32> = vec2<f32>(traced) + 0.5;
    let is_changed: bool = all(invocation.xy < level.domain_size) && cell_inputs_changed(position);
    if !is_changed { return; }
    var radiance: vec3<f32> = vec3<f32>(0.0);
    for (var direction: u32 = 0u; direction < level.direction_count; direction++) {
        radiance += merged_radiance_single(local, position, direction, true);
    }
    if !is_changed {
        return;
    }
    let cell: u32 = invocation.y * level.domain_size.x + invocation.x;
    let light: vec3<f32> = min(radiance / f32(level.direction_count), vec3<f32>(60000.0));
    fluence[cell] = vec2<u32>(pack2x16float(light.rg), pack2x16float(vec2<f32>(light.b, 0.0)));
}

override LOCAL_DIRTY:bool=false;
@group(0) @binding(7) var<storage,read> local_changes:array<u32>;

/// Conservative rectangle query over changed 8x8 optical blocks, with a one-cell border.
fn local_optical_changes(low:vec2<f32>,high:vec2<f32>)->bool {
    let size=vec2<i32>(level.domain_size);
    let first=clamp(vec2<i32>(floor(low))-vec2<i32>(1),vec2<i32>(0),size);
    let last=clamp(vec2<i32>(ceil(high))+vec2<i32>(1),vec2<i32>(0),size);
    if any(first>=last) {return false;}
    let lo=vec2<u32>(first)/8u;
    let hi=(vec2<u32>(last)+vec2<u32>(7u))/8u;
    let pitch=(level.domain_size.x+7u)/8u+1u;
    let total=local_changes[hi.y*pitch+hi.x]+local_changes[lo.y*pitch+lo.x]
        -local_changes[lo.y*pitch+hi.x]-local_changes[hi.y*pitch+lo.x];
    return total!=0u;
}
