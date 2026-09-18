// Copyright Rob Gage 2026

const TAU: f32 = 6.283185307179586;

struct CascadeConfiguration {
    scene_size: vec2<u32>,
    probe_size: vec2<u32>,
    probe_spacing: u32,
    direction_count: u32,
    interval_start: f32,
    interval_end: f32,
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
    let probe_position = vec2<f32>(probe) * f32(trace_configuration.probe_spacing) +
        vec2<f32>(f32(trace_configuration.probe_spacing) * 0.5);
    let angle = (f32(direction_index) + 0.5) * TAU /
        f32(trace_configuration.direction_count);
    let direction = vec2<f32>(cos(angle), sin(angle));
    var travel = trace_configuration.interval_start;
    var radiance = vec3<f32>(0.0);
    var transmission = 1.0;
    for (var step = 0u; step < 64u; step++) {
        if travel >= trace_configuration.interval_end || transmission <= 0.001 { break; }
        let position = probe_position + direction * travel;
        if any(position < vec2<f32>(0.0)) ||
                any(position >= vec2<f32>(trace_configuration.scene_size)) { break; }
        let optical = textureLoad(optical_field, vec2<i32>(position), 0);
        let extinction = max(optical.a, 0.0);
        let attenuation = exp(-extinction);
        let segment_radiance = select(optical.rgb, optical.rgb *
            ((1.0 - attenuation) / max(extinction, 0.000001)), extinction > 0.000001);
        radiance += transmission * segment_radiance;
        transmission *= attenuation;
        travel += 1.0;
    }
    trace_output.values[invocation.x] = vec4<f32>(radiance, transmission);
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
    let position = vec2<f32>(probe) * f32(merge_configuration.probe_spacing) +
        vec2<f32>(f32(merge_configuration.probe_spacing) * 0.5);
    var far = vec4<f32>(0.0);
    for (var child = 0u; child < 4u; child++) {
        far += sample_upper(position, direction_index * 4u + child);
    }
    far *= 0.25;
    lower_intervals.values[invocation.x] = vec4<f32>(near.rgb + near.a * far.rgb, near.a * far.a);
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
    illumination /= f32(integrate_configuration.direction_count);
    textureStore(illumination_output, vec2<i32>(pixel), vec4<f32>(illumination, 1.0));
}

fn sample_upper(position: vec2<f32>, direction: u32) -> vec4<f32> {
    let spacing = merge_configuration.probe_spacing * 2u;
    let size = (merge_configuration.scene_size + vec2<u32>(spacing - 1u)) / spacing + vec2<u32>(2);
    let coordinate = position / f32(spacing) - vec2<f32>(0.5);
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
    let coordinate = position / f32(spacing) - vec2<f32>(0.5);
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
