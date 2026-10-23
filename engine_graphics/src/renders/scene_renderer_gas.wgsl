// Copyright Rob Gage 2026

/// Uniform values shared by gas fragments in one viewport
struct GasUniforms {
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
    /// Gas amount units in one atmosphere
    amount_per_atmosphere: f32,
    /// Uniform alignment padding
    padding_0: u32,
    /// Uniform alignment padding
    padding_1: u32,
};

@group(0) @binding(0)
var<uniform> uniforms: GasUniforms;

@group(0) @binding(1)
var<storage, read> material_identifiers: array<u32>;

@group(0) @binding(2)
var<storage, read> gas_cells: array<vec4<u32>>;

@group(0) @binding(3)
var<storage, read> gas_materials: array<vec4<f32>>;

/// Opacity below which a cell's gas is not drawn and ends the cloud
const VISIBLE_ALPHA_MINIMUM: f32 = 0.02;

/// Converts one sRGB channel to linear light
fn srgb_to_linear(value: f32) -> f32 {
    if value <= 0.04045 {
        return value / 12.92;
    }
    return pow((value + 0.055) / 1.055, 2.4);
}

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

/// Returns premultiplied linear color for one logical gas cell
fn cell_gas_color(cell: vec2<i32>) -> vec4<f32> {
    let region_cell_width: i32 = i32(uniforms.region_width * uniforms.cellular_tile_size);
    let region_cell_height: i32 = i32(uniforms.region_height * uniforms.cellular_tile_size);
    if cell.x < 0 || cell.y < 0 || cell.x >= region_cell_width || cell.y >= region_cell_height {
        return vec4<f32>(0.0);
    }
    let cell_index: u32 = physical_cell_index(cell);
    if material_identifiers[cell_index] != 0u {
        return vec4<f32>(0.0);
    }
    let slots: vec4<u32> = gas_cells[cell_index * 2u];
    var accumulated_rgb: vec3<f32> = vec3<f32>(0.0);
    var accumulated_alpha: f32 = 0.0;
    for (var slot_index: u32 = 0u; slot_index < 4u; slot_index = slot_index + 1u) {
        let packed_slot: u32 = slots[slot_index];
        let amount: u32 = packed_slot & 0x00ffffffu;
        if amount > 0u {
            let gas_index: u32 = packed_slot >> 24u;
            let properties: vec4<f32> = gas_materials[gas_index];
            let srgb_color: vec4<f32> = unpack4x8unorm(bitcast<u32>(properties.w));
            let linear_rgb: vec3<f32> = vec3<f32>(
                srgb_to_linear(srgb_color.r),
                srgb_to_linear(srgb_color.g),
                srgb_to_linear(srgb_color.b),
            );
            let density: f32 = f32(amount) / uniforms.amount_per_atmosphere;
            let alpha: f32 = clamp(1.0 - pow(1.0 - srgb_color.a, density), 0.0, 1.0);
            let remaining: f32 = 1.0 - accumulated_alpha;
            accumulated_rgb = accumulated_rgb + linear_rgb * alpha * remaining;
            accumulated_alpha = accumulated_alpha + alpha * remaining;
        }
    }
    return vec4<f32>(accumulated_rgb, accumulated_alpha);
}

/// Draws one gas framebuffer pixel: only inside cells holding visible gas, so cloud edges follow
/// the cell grid like fluid, blending smoothly between the visible cells around the pixel
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
    if cell_gas_color(vec2<i32>(floor(cell_position))).a <= VISIBLE_ALPHA_MINIMUM {
        discard;
    }
    let continuous_cell: vec2<f32> = cell_position - vec2<f32>(0.5);
    let lower_cell: vec2<i32> = vec2<i32>(floor(continuous_cell));
    let cell_fraction: vec2<f32> = fract(continuous_cell);
    // weigh only the visible cells, so edge cells keep their own color up to the edge
    var color: vec4<f32> = vec4<f32>(0.0);
    var weight_sum: f32 = 0.0;
    for (var corner: u32 = 0u; corner < 4u; corner++) {
        let offset: vec2<u32> = vec2<u32>(corner % 2u, corner / 2u);
        let corner_color: vec4<f32> = cell_gas_color(lower_cell + vec2<i32>(offset));
        if corner_color.a <= VISIBLE_ALPHA_MINIMUM { continue; }
        let weights: vec2<f32> = select(
            1.0 - cell_fraction,
            cell_fraction,
            offset == vec2<u32>(1u),
        );
        let weight: f32 = weights.x * weights.y;
        color += corner_color * weight;
        weight_sum += weight;
    }
    return color / max(weight_sum, 0.000001);
}
