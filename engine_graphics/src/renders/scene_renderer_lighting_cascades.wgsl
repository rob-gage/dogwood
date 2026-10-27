// Copyright Rob Gage 2026

/// One cascade level: probes `spacing` cells apart, each tracing `direction_count` intervals from
/// `interval_start` to `interval_end` cells, merged with the next coarser level
struct CascadeLevel {
    /// Lighting domain size in cells
    domain_size: vec2<u32>,
    /// Number of probes along each axis
    probe_count: vec2<u32>,
    /// Domain position of the first probe's center in cells
    probe_origin: vec2<f32>,
    /// Distance between neighboring probes in cells
    spacing: f32,
    /// Number of evenly spaced directions traced from each probe
    direction_count: u32,
    /// Distance from a probe where its intervals start in cells
    interval_start: f32,
    /// Distance from a probe where its intervals end in cells
    interval_end: f32,
    /// Number of probes along each axis of the coarser level
    upper_probe_count: vec2<u32>,
    /// Domain position of the coarser level's first probe center in cells
    upper_probe_origin: vec2<f32>,
    /// Distance between neighboring probes of the coarser level in cells
    upper_spacing: f32,
    /// Whether a coarser level exists to merge with
    has_upper: u32,
};

@group(0) @binding(0)
var<uniform> level: CascadeLevel;

/// Linear emission per cell of path in `rgb` and extinction per cell in `a`
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

/// Returns the radiance gathered along a ray and the fraction of light passing through it,
/// crossing uniform blocks of 8x8 cells in one step
fn trace(origin: vec2<f32>, direction: vec2<f32>, start: f32, end: f32) -> vec4<f32> {
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
        let visibility: f32 = exp(-sample.a * length);
        // an optically thick cell emits its radiance from its surface, and a thin one along
        // its path
        var path: f32 = length;
        if sample.a > 1e-4 {
            path = (1.0 - visibility) / min(sample.a, 1.0);
        }
        radiance += transmission * sample.rgb * path;
        transmission *= visibility;
        distance = next;
        if transmission < 1e-3 {
            transmission = 0.0;
            break;
        }
    }
    return vec4<f32>(radiance, transmission);
}

/// Unpacks one merged interval's radiance
fn unpack_interval(packed: vec2<u32>) -> vec3<f32> {
    return vec3<f32>(unpack2x16float(packed.x), unpack2x16float(packed.y).x);
}

/// Returns the radiance arriving at a domain position along a direction through one interval,
/// merged with the coarser level. Each of the four surrounding coarser probes gets its own ray,
/// from where this interval starts to where that probe's interval starts, so occluders between
/// the probes do not leak light across.
fn merged_radiance(position: vec2<f32>, direction: u32) -> vec3<f32> {
    let angle: f32 = (f32(direction) + 0.5) * TAU / f32(level.direction_count);
    let ray: vec2<f32> = vec2<f32>(cos(angle), sin(angle));
    if level.has_upper == 0u {
        return trace(position, ray, level.interval_start, level.interval_end).rgb;
    }
    let start: vec2<f32> = position + ray * level.interval_start;
    let upper_direction_count: u32 = level.direction_count * 2u;
    // where the interval crosses nothing, the four rays would see nothing either, so one ray and
    // interpolated coarser probes suffice
    let plain: vec4<f32> = trace(position, ray, level.interval_start, level.interval_end);
    let is_clear: bool = plain.a >= 0.9999 && all(plain.rgb <= vec3<f32>(1e-6));
    let lattice: vec2<f32> = (position - level.upper_probe_origin) / level.upper_spacing;
    let maximum: vec2<i32> = max(vec2<i32>(level.upper_probe_count) - vec2<i32>(2), vec2<i32>(0));
    let base: vec2<i32> = clamp(vec2<i32>(floor(lattice)), vec2<i32>(0), maximum);
    let weight: vec2<f32> = clamp(lattice - vec2<f32>(base), vec2<f32>(0.0), vec2<f32>(1.0));
    var radiance: vec3<f32> = vec3<f32>(0.0);
    for (var corner: u32 = 0u; corner < 4u; corner++) {
        let offset: vec2<i32> = vec2<i32>(i32(corner & 1u), i32(corner >> 1u));
        let corner_weight: f32 = select(1.0 - weight.x, weight.x, offset.x == 1)
            * select(1.0 - weight.y, weight.y, offset.y == 1);
        if corner_weight <= 0.0 {
            continue;
        }
        let probe: vec2<u32> = vec2<u32>(min(base + offset, vec2<i32>(level.upper_probe_count) - vec2<i32>(1)));
        let upper_position: vec2<f32> = level.upper_probe_origin + vec2<f32>(probe) * level.upper_spacing;
        var near: vec4<f32> = plain;
        if !is_clear {
            let segment: vec2<f32> = upper_position + ray * level.interval_end - start;
            let length: f32 = max(length(segment), 1e-6);
            near = trace(start, segment / length, 0.0, length);
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
            return true;
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
            return true;
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
    @builtin(num_workgroups) workgroups: vec3<u32>,
) {
    let index: u32 = invocation.x + invocation.y * workgroups.x * 64u;
    let probe_total: u32 = level.probe_count.x * level.probe_count.y;
    if index >= probe_total * level.direction_count {
        return;
    }
    let direction: u32 = index % level.direction_count;
    let probe_index: u32 = index / level.direction_count;
    let probe: vec2<u32> = vec2<u32>(probe_index % level.probe_count.x, probe_index / level.probe_count.x);
    let position: vec2<f32> = level.probe_origin + vec2<f32>(probe) * level.spacing;
    if !interval_inputs_changed(position, direction) {
        intervals[index].y = intervals[index].y & ~CHANGED;
        return;
    }
    let radiance: vec3<f32> = merged_radiance(position, direction);
    let packed: vec2<u32> = vec2<u32>(pack2x16float(radiance.rg), pack2x16float(vec2<f32>(radiance.b, 0.0)));
    let previous: vec2<u32> = vec2<u32>(intervals[index].x, intervals[index].y & ~CHANGED);
    intervals[index] = vec2<u32>(packed.x, packed.y | select(0u, 0x10000u, any(packed != previous)));
}

/// Gathers the fluence of every cell from its finest intervals merged with the stored levels,
/// keeping the previous fluence of cells where nothing changed
@compute @workgroup_size(8, 8, 1)
fn integrate(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if any(invocation.xy >= level.domain_size) {
        return;
    }
    let position: vec2<f32> = vec2<f32>(invocation.xy) + 0.5;
    if !cell_inputs_changed(position) {
        return;
    }
    var radiance: vec3<f32> = vec3<f32>(0.0);
    for (var direction: u32 = 0u; direction < level.direction_count; direction++) {
        radiance += merged_radiance(position, direction);
    }
    let cell: u32 = invocation.y * level.domain_size.x + invocation.x;
    let light: vec3<f32> = min(radiance / f32(level.direction_count), vec3<f32>(60000.0));
    fluence[cell] = vec2<u32>(pack2x16float(light.rg), pack2x16float(vec2<f32>(light.b, 0.0)));
}
