// Copyright Rob Gage 2026

const TAU: f32 = 6.283185307179586;

struct CascadeConfiguration {
    scene_size: vec2<u32>,
    probe_size: vec2<u32>,
    probe_spacing: u32,
    direction_count: u32,
    interval_start: f32,
    interval_end: f32,
    world_origin: vec2<f32>,
    _padding: vec4<f32>,
}

struct RadianceIntervals { values: array<vec4<f32>>, }

@group(0) @binding(0) var optical_field: texture_2d<f32>;
@group(0) @binding(3) var<uniform> trace_configuration: CascadeConfiguration;
@group(0) @binding(4) var<storage, read_write> trace_output: RadianceIntervals;

@group(0) @binding(5) var<uniform> merge_configuration: CascadeConfiguration;
@group(0) @binding(6) var<storage, read_write> lower_intervals: RadianceIntervals;
@group(0) @binding(7) var<storage, read> upper_intervals: RadianceIntervals;

@group(0) @binding(8) var<storage, read> integrated_intervals: RadianceIntervals;
@group(0) @binding(9) var<uniform> integrate_configuration: CascadeConfiguration;
@group(0) @binding(10) var illumination_output: texture_storage_2d<rgba16float, write>;

@compute @workgroup_size(64)
fn trace(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let interval_count = trace_configuration.probe_size.x * trace_configuration.probe_size.y *
        trace_configuration.direction_count;
    if invocation.x >= interval_count { return; }
    let direction_index = invocation.x % trace_configuration.direction_count;
    let probe_index = invocation.x / trace_configuration.direction_count;
    let probe = vec2<u32>(
        probe_index % trace_configuration.probe_size.x,
        probe_index / trace_configuration.probe_size.x,
    );
    let probe_position = probe_origin(
        trace_configuration.world_origin,
        trace_configuration.probe_spacing,
    ) + vec2<f32>(probe) * f32(trace_configuration.probe_spacing);
    let angle = (f32(direction_index) + 0.5) * TAU /
        f32(trace_configuration.direction_count);
    let direction = vec2<f32>(cos(angle), sin(angle));
    var radiance = vec3<f32>(0.0);
    var transmission = 1.0;
    var last_attenuated_cell = vec2<i32>(0x7fffffff);
    var origin_cell = vec2<i32>(0);
    var escaping_origin_cell = false;
    if all(probe_position >= vec2<f32>(0.0)) &&
            all(probe_position < vec2<f32>(trace_configuration.scene_size)) {
        origin_cell = vec2<i32>(probe_position);
        escaping_origin_cell = textureLoad(optical_field, origin_cell, 0).a > 0.000001;
    }
    var interval_start = trace_configuration.interval_start;
    var interval_end = trace_configuration.interval_end;
    for (var axis = 0u; axis < 2u; axis++) {
        let origin = probe_position[axis];
        let component = direction[axis];
        let extent = f32(trace_configuration.scene_size[axis]);
        if abs(component) < 0.000001 {
            if origin < 0.0 || origin >= extent {
                trace_output.values[invocation.x] = vec4<f32>(0.0, 0.0, 0.0, 1.0);
                return;
            }
        } else {
            let first = (0.0 - origin) / component;
            let last = (extent - origin) / component;
            interval_start = max(interval_start, min(first, last));
            interval_end = min(interval_end, max(first, last));
        }
    }
    if interval_start >= interval_end {
        trace_output.values[invocation.x] = vec4<f32>(0.0, 0.0, 0.0, 1.0);
        return;
    }

    let position = probe_position + direction * interval_start;
    var cell = vec2<i32>(floor(position));
    if direction.x < 0.0 && position.x == floor(position.x) { cell.x -= 1; }
    if direction.y < 0.0 && position.y == floor(position.y) { cell.y -= 1; }
    let step = vec2<i32>(
        select(0, 1, direction.x > 0.0),
        select(0, 1, direction.y > 0.0),
    ) - vec2<i32>(
        select(0, 1, direction.x < 0.0),
        select(0, 1, direction.y < 0.0),
    );
    var next_crossing = vec2<f32>(1e30);
    var crossing_delta = vec2<f32>(1e30);
    if abs(direction.x) >= 0.000001 {
        let boundary = select(f32(cell.x), f32(cell.x + 1), direction.x > 0.0);
        next_crossing.x = (boundary - probe_position.x) / direction.x;
        crossing_delta.x = abs(1.0 / direction.x);
    }
    if abs(direction.y) >= 0.000001 {
        let boundary = select(f32(cell.y), f32(cell.y + 1), direction.y > 0.0);
        next_crossing.y = (boundary - probe_position.y) / direction.y;
        crossing_delta.y = abs(1.0 / direction.y);
    }

    var travel = interval_start;
    while travel < interval_end && transmission > 0.001 {
        if all(cell >= vec2<i32>(0)) &&
                all(cell < vec2<i32>(trace_configuration.scene_size)) {
            let pixel = cell;
            let optical = textureLoad(optical_field, pixel, 0);
            if optical.a > 0.000001 {
                if escaping_origin_cell && all(cell == origin_cell) {
                    // Keep walking until the ray escapes an opaque origin cell.
                } else {
                    escaping_origin_cell = false;
                    if any(cell != last_attenuated_cell) {
                        let tau = max(optical.a, 0.0);
                        let cell_transmission = exp(-tau);
                        radiance += transmission * optical.rgb *
                            ((1.0 - cell_transmission) / max(tau, 0.000001));
                        transmission *= cell_transmission;
                        last_attenuated_cell = cell;
                    }
                }
            } else {
                escaping_origin_cell = false;
            }
        }
        let next_travel = min(min(next_crossing.x, next_crossing.y), interval_end);
        if next_crossing.x <= next_crossing.y {
            cell.x += step.x;
            next_crossing.x += crossing_delta.x;
        }
        if next_crossing.y <= next_crossing.x {
            cell.y += step.y;
            next_crossing.y += crossing_delta.y;
        }
        travel = next_travel;
    }
    trace_output.values[invocation.x] = vec4<f32>(max(radiance, vec3<f32>(0.0)),
        clamp(transmission, 0.0, 1.0));
}

@compute @workgroup_size(64)
fn merge(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let interval_count = merge_configuration.probe_size.x * merge_configuration.probe_size.y *
        merge_configuration.direction_count;
    if invocation.x >= interval_count { return; }
    let near = lower_intervals.values[invocation.x];
    let direction_index = invocation.x % merge_configuration.direction_count;
    let probe_index = invocation.x / merge_configuration.direction_count;
    let probe = vec2<u32>(
        probe_index % merge_configuration.probe_size.x,
        probe_index / merge_configuration.probe_size.x,
    );
    let position = probe_origin(
        merge_configuration.world_origin,
        merge_configuration.probe_spacing,
    ) + vec2<f32>(probe) * f32(merge_configuration.probe_spacing);
    var far = vec4<f32>(0.0);
    for (var child = 0u; child < 4u; child++) {
        far += sample_upper(position, direction_index * 4u + child);
    }
    far *= 0.25;
    lower_intervals.values[invocation.x] = vec4<f32>(
        max(near.rgb + near.a * far.rgb, vec3<f32>(0.0)),
        clamp(near.a * far.a, 0.0, 1.0),
    );
}

@compute @workgroup_size(64)
fn integrate(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let pixel_count = integrate_configuration.scene_size.x * integrate_configuration.scene_size.y;
    if invocation.x >= pixel_count { return; }
    let pixel = vec2<u32>(
        invocation.x % integrate_configuration.scene_size.x,
        invocation.x / integrate_configuration.scene_size.x,
    );
    var illumination = vec3<f32>(0.0);
    for (var direction = 0u; direction < integrate_configuration.direction_count; direction++) {
        illumination += sample_integrated(vec2<f32>(pixel) + vec2<f32>(0.5), direction).rgb;
    }
    illumination = max(illumination / f32(integrate_configuration.direction_count),
        vec3<f32>(0.0));
    textureStore(illumination_output, vec2<i32>(pixel), vec4<f32>(illumination, 1.0));
}

fn sample_upper(position: vec2<f32>, direction: u32) -> vec4<f32> {
    let spacing = merge_configuration.probe_spacing * 2u;
    let size = (merge_configuration.scene_size + vec2<u32>(spacing - 1u)) / spacing +
        vec2<u32>(2);
    let origin = probe_origin(merge_configuration.world_origin, spacing);
    let coordinate = (position - origin) / f32(spacing);
    let base = clamp(vec2<i32>(floor(coordinate)), vec2<i32>(0), vec2<i32>(size) - vec2<i32>(1));
    let fraction = fract(coordinate);
    let p00 = sample_upper_probe(base, size, direction);
    let p10 = sample_upper_probe(clamp(base + vec2<i32>(1, 0), vec2<i32>(0), vec2<i32>(size) - vec2<i32>(1)), size, direction);
    let p01 = sample_upper_probe(clamp(base + vec2<i32>(0, 1), vec2<i32>(0), vec2<i32>(size) - vec2<i32>(1)), size, direction);
    let p11 = sample_upper_probe(clamp(base + vec2<i32>(1), vec2<i32>(0), vec2<i32>(size) - vec2<i32>(1)), size, direction);
    return mix(mix(p00, p10, fraction.x), mix(p01, p11, fraction.x), fraction.y);
}

fn sample_upper_probe(probe: vec2<i32>, size: vec2<u32>, direction: u32) -> vec4<f32> {
    let direction_count = merge_configuration.direction_count * 4u;
    let index = (u32(probe.y) * size.x + u32(probe.x)) * direction_count + direction;
    return upper_intervals.values[index];
}

fn sample_integrated(position: vec2<f32>, direction: u32) -> vec4<f32> {
    let spacing = integrate_configuration.probe_spacing;
    let origin = probe_origin(integrate_configuration.world_origin, spacing);
    let coordinate = (position - origin) / f32(spacing);
    let base = clamp(vec2<i32>(floor(coordinate)), vec2<i32>(0),
        vec2<i32>(integrate_configuration.probe_size) - vec2<i32>(1));
    let fraction = fract(coordinate);
    let maximum = vec2<i32>(integrate_configuration.probe_size) - vec2<i32>(1);
    let p00 = sample_integrated_probe(base, direction);
    let p10 = sample_integrated_probe(clamp(base + vec2<i32>(1, 0), vec2<i32>(0), maximum), direction);
    let p01 = sample_integrated_probe(clamp(base + vec2<i32>(0, 1), vec2<i32>(0), maximum), direction);
    let p11 = sample_integrated_probe(clamp(base + vec2<i32>(1), vec2<i32>(0), maximum), direction);
    return mix(mix(p00, p10, fraction.x), mix(p01, p11, fraction.x), fraction.y);
}

fn sample_integrated_probe(probe: vec2<i32>, direction: u32) -> vec4<f32> {
    let index = (u32(probe.y) * integrate_configuration.probe_size.x + u32(probe.x)) *
        integrate_configuration.direction_count + direction;
    return integrated_intervals.values[index];
}

fn probe_origin(world_origin: vec2<f32>, spacing: u32) -> vec2<f32> {
    let s = f32(spacing);
    return floor(world_origin / s) * s - world_origin - vec2<f32>(s);
}
