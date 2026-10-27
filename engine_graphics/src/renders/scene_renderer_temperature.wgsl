// Copyright Rob Gage 2026

/// Uniform values shared by temperature fragments in one viewport
struct TemperatureUniforms {
    /// Origin of the viewport in target pixels
    viewport_origin: vec2<f32>,
    /// Size of the viewport in target pixels
    viewport_size: vec2<f32>,
    /// Target pixels per tile
    zoom: f32,
    /// Alignment padding before the region fields
    zoom_padding: f32,
    /// Camera-relative region origin in tile coordinates
    camera_relative_region: vec2<f32>,
    /// Region width in tiles
    region_width: u32,
    /// Region height in tiles
    region_height: u32,
    /// Physical ring column for the logical leftmost tile
    ring_offset_x: u32,
    /// Physical ring row for the logical bottom tile
    ring_offset_y: u32,
    /// Number of cells along each side of a tile
    cellular_tile_size: u32,
    /// Ambient temperature in kelvin
    ambient_temperature: f32,
    /// Uniform alignment padding
    padding_0: u32,
    /// Uniform alignment padding
    padding_1: u32,
};

/// Highest temperature shown by the temperature view mode
const TEMPERATURE_VIEW_MAXIMUM: f32 = 6000.0;

/// First temperature color stop as a fraction of the logarithmic hot range
const TEMPERATURE_VIEW_COLOR_STOP_FIRST: f32 = 1.0 / 3.0;

/// Second temperature color stop as a fraction of the logarithmic hot range
const TEMPERATURE_VIEW_COLOR_STOP_SECOND: f32 = 2.0 / 3.0;

/// Color segments across the logarithmic hot range
const TEMPERATURE_VIEW_COLOR_SEGMENT_COUNT: f32 = 3.0;

@group(0) @binding(0)
var<uniform> uniforms: TemperatureUniforms;

@group(0) @binding(1)
var<storage, read> matter_cells: array<u32>;

/// Creates the fullscreen triangle
@vertex
fn vertex(@builtin(vertex_index) vertex_index: u32) -> @builtin(position) vec4<f32> {
    let positions: array<vec2<f32>, 3> = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    let position: vec2<f32> = positions[vertex_index];
    return vec4<f32>(position, 0.0, 1.0);
}

/// Returns the physical ring cell index for an in-region logical cell
fn physical_cell_index(cell: vec2<i32>) -> u32 {
    let cell_size: i32 = i32(uniforms.cellular_tile_size);
    let tile: vec2<u32> = vec2<u32>(cell / cell_size);
    let in_tile: vec2<u32> = vec2<u32>(cell % cell_size);
    let physical_x: u32 = (tile.x + uniforms.ring_offset_x) % uniforms.region_width;
    let physical_y: u32 = (tile.y + uniforms.ring_offset_y) % uniforms.region_height;
    let tile_slot: u32 = physical_y * uniforms.region_width + physical_x;
    return tile_slot * uniforms.cellular_tile_size * uniforms.cellular_tile_size
        + in_tile.y * uniforms.cellular_tile_size
        + in_tile.x;
}

/// Returns the temperature color at a temperature in kelvin
fn temperature_color(temperature: f32) -> vec4<f32> {
    let ambient: f32 = max(uniforms.ambient_temperature, 1.0);
    if temperature < ambient {
        let amount: f32 = clamp((ambient - temperature) / max(ambient, 1.0), 0.0, 1.0);
        let t: f32 = clamp((ambient - temperature) / max(ambient * 0.5, 1.0), 0.0, 1.0);
        let rgb: vec3<f32> = mix(
            vec3<f32>(0.55, 0.75, 1.0), vec3<f32>(0.05, 0.15, 0.8), amount);
        return vec4<f32>(rgb, 0.85 * sqrt(t));
    }
    let t: f32 = clamp((temperature - ambient) / max(ambient * 0.5, 1.0), 0.0, 1.0);
    let scale: f32 = clamp(
        log(temperature / ambient) / log(TEMPERATURE_VIEW_MAXIMUM / ambient), 0.0, 1.0);
    var rgb: vec3<f32>;
    if scale < TEMPERATURE_VIEW_COLOR_STOP_FIRST {
        rgb = mix(
            vec3<f32>(1.0, 0.0, 0.0),
            vec3<f32>(1.0, 0.35, 0.0),
            scale * TEMPERATURE_VIEW_COLOR_SEGMENT_COUNT,
        );
    } else if scale < TEMPERATURE_VIEW_COLOR_STOP_SECOND {
        rgb = mix(vec3<f32>(1.0, 0.35, 0.0), vec3<f32>(1.0, 1.0, 0.0),
            (scale - TEMPERATURE_VIEW_COLOR_STOP_FIRST)
                * TEMPERATURE_VIEW_COLOR_SEGMENT_COUNT);
    } else {
        rgb = mix(vec3<f32>(1.0, 1.0, 0.0), vec3<f32>(1.0, 1.0, 1.0),
            (scale - TEMPERATURE_VIEW_COLOR_STOP_SECOND)
                * TEMPERATURE_VIEW_COLOR_SEGMENT_COUNT);
    }
    return vec4<f32>(rgb, 0.85 * sqrt(t));
}

/// Draws one temperature cell over the scene
@fragment
fn fragment(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let pixel: vec2<f32> = position.xy - uniforms.viewport_origin;
    let centered_pixel: vec2<f32> = pixel - uniforms.viewport_size * 0.5;
    let world_offset: vec2<f32> = vec2<f32>(
        centered_pixel.x / uniforms.zoom,
        -centered_pixel.y / uniforms.zoom,
    );
    let local: vec2<f32> = uniforms.camera_relative_region + world_offset;
    let cell_position: vec2<f32> = local * f32(uniforms.cellular_tile_size);
    let cell: vec2<i32> = vec2<i32>(floor(cell_position));
    let region_cell_width: i32 = i32(uniforms.region_width * uniforms.cellular_tile_size);
    let region_cell_height: i32 = i32(uniforms.region_height * uniforms.cellular_tile_size);
    if cell.x < 0 || cell.y < 0 || cell.x >= region_cell_width || cell.y >= region_cell_height {
        discard;
    }
    let temperature: f32 = bitcast<f32>(matter_cells[physical_cell_index(cell) * 2u]);
    let color: vec4<f32> = temperature_color(temperature);
    return vec4<f32>(color.rgb * color.a, color.a);
}
