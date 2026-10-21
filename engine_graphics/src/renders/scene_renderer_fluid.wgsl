// Copyright Rob Gage 2026

/// Uniform values shared by fluid fragments in one viewport
struct FluidUniforms {
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
    /// Alignment padding before the shaded rectangle
    shaded_padding: u32,
    /// Logical cell at the lower left of the shaded cell rectangle
    shaded_origin: vec2<i32>,
    /// Width and height of the shaded cell rectangle
    shaded_size: vec2<u32>,
    /// Uniform alignment padding
    padding_0: u32,
    /// Uniform alignment padding
    padding_1: u32,
};

/// Smoothed occupancy and dominant material around one logical cell
struct FluidSample {
    /// Whether the smoothed cell passes the water occupancy threshold
    is_water: bool,
    /// Dominant fluid material identifier from the sampled neighborhood
    material_identifier: u32,
};

@group(0) @binding(0)
var<uniform> uniforms: FluidUniforms;

@group(0) @binding(1)
var<storage, read> fluid_raster: array<i32>;

@group(0) @binding(2)
var<storage, read> fluid_materials: array<vec4<f32>>;

@group(0) @binding(3)
var<storage, read_write> water_cells: array<u32>;

@group(0) @binding(4)
var<storage, read_write> shaded_cells: array<vec2<u32>>;

/// Rows classified above the shaded rectangle for the depth search
const DEPTH_SEARCH_ROWS: u32 = 8u;

/// Fixed-point scale used by the fluid particle raster
const FLUID_RASTER_SCALE: f32 = 4096.0;

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
    let slot: u32 = physical_y * uniforms.region_width + physical_x;
    return slot * uniforms.cellular_tile_size * uniforms.cellular_tile_size
        + in_tile.y * uniforms.cellular_tile_size
        + in_tile.x;
}

/// Reads the fixed-point mass and material identifier at an in-region cell
fn raster_mass_and_material(cell: vec2<i32>) -> vec2<u32> {
    let base: u32 = physical_cell_index(cell) * 4u;
    let mass: u32 = u32(max(fluid_raster[base], 0));
    let material_identifier: u32 = bitcast<u32>(fluid_raster[base + 3u]);
    return vec2<u32>(mass, material_identifier);
}

/// Smooths a 3x3 neighborhood and applies the half-cell occupancy threshold
fn sample_fluid(cell: vec2<i32>) -> FluidSample {
    let region_cell_width: i32 = i32(uniforms.region_width * uniforms.cellular_tile_size);
    let region_cell_height: i32 = i32(uniforms.region_height * uniforms.cellular_tile_size);
    var weighted_mass: f32 = 0.0;
    var dominant_identifier: u32 = 0u;
    for (var offset_y: i32 = -1; offset_y <= 1; offset_y++) {
        for (var offset_x: i32 = -1; offset_x <= 1; offset_x++) {
            let neighbor: vec2<i32> = cell + vec2<i32>(offset_x, offset_y);
            if neighbor.x < 0 || neighbor.y < 0
                || neighbor.x >= region_cell_width || neighbor.y >= region_cell_height
            { continue; }
            let sample: vec2<u32> = raster_mass_and_material(neighbor);
            let weight: f32 = select(2.0, 1.0, offset_x != 0 && offset_y != 0);
            let center_weighted: f32 = select(weight, 4.0, offset_x == 0 && offset_y == 0);
            weighted_mass += f32(sample.x) * center_weighted;
            if sample.x > 0u { dominant_identifier = max(dominant_identifier, sample.y); }
        }
    }
    if dominant_identifier == 0u {
        return FluidSample(false, 0u);
    }
    let material_index: u32 = dominant_identifier & 0x3fffffffu;
    let density: f32 = fluid_materials[material_index * 2u].x;
    let smoothed_mass: f32 = weighted_mass / (FLUID_RASTER_SCALE * 16.0);
    let occupancy: f32 = smoothed_mass / density;
    return FluidSample(occupancy > 0.5, dominant_identifier);
}

/// Records the dominant material of each smoothed water cell in and just above the shaded
/// rectangle, or zero for cells below the occupancy threshold
@compute @workgroup_size(8, 8)
fn classify(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let rows: u32 = uniforms.shaded_size.y + DEPTH_SEARCH_ROWS;
    if invocation.x >= uniforms.shaded_size.x || invocation.y >= rows { return; }
    let sample: FluidSample = sample_fluid(uniforms.shaded_origin + vec2<i32>(invocation.xy));
    water_cells[invocation.y * uniforms.shaded_size.x + invocation.x] =
        select(0u, sample.material_identifier, sample.is_water);
}

/// Returns the classified material of a cell relative to the shaded origin
fn water_cell(local: vec2<u32>) -> u32 {
    return water_cells[local.y * uniforms.shaded_size.x + local.x];
}

/// Shades each classified water cell once with its depth below the surface, leaving a zero
/// premultiplied color for cells without water
@compute @workgroup_size(8, 8)
fn shade(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if invocation.x >= uniforms.shaded_size.x || invocation.y >= uniforms.shaded_size.y {
        return;
    }
    let index: u32 = invocation.y * uniforms.shaded_size.x + invocation.x;
    let material_identifier: u32 = water_cell(invocation.xy);
    if material_identifier == 0u {
        shaded_cells[index] = vec2<u32>(0u);
        return;
    }
    var depth: u32 = 0u;
    for (var above: u32 = 1u; above <= DEPTH_SEARCH_ROWS; above++) {
        if water_cell(invocation.xy + vec2<u32>(0u, above)) == 0u { break; }
        depth++;
    }
    let surface: bool = water_cell(invocation.xy + vec2<u32>(0u, 1u)) == 0u;
    let material_index: u32 = material_identifier & 0x3fffffffu;
    let packed_color: u32 = bitcast<u32>(fluid_materials[material_index * 2u + 1u].w);
    let srgb_color: vec4<f32> = unpack4x8unorm(packed_color);
    let linear_rgb: vec3<f32> = vec3<f32>(
        srgb_to_linear(srgb_color.r),
        srgb_to_linear(srgb_color.g),
        srgb_to_linear(srgb_color.b),
    );
    let depth_factor: f32 = 1.0 - f32(depth) * 0.035;
    let surface_factor: f32 = select(1.0, 1.08, surface);
    let alpha: f32 = srgb_color.a;
    let color: vec3<f32> = linear_rgb * depth_factor * surface_factor;
    let premultiplied: vec4<f32> = vec4<f32>(color * alpha, alpha);
    shaded_cells[index] =
        vec2<u32>(pack2x16float(premultiplied.rg), pack2x16float(premultiplied.ba));
}

/// Maps one fragment to a logical cell and draws its shaded fluid color
@fragment
fn fragment(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let pixel: vec2<f32> = position.xy - uniforms.viewport_origin;
    let centered_pixel: vec2<f32> = pixel - uniforms.viewport_size * 0.5;
    let world_offset: vec2<f32> = vec2<f32>(
        centered_pixel.x / uniforms.zoom,
        -centered_pixel.y / uniforms.zoom,
    );
    let local: vec2<f32> = uniforms.camera_relative_region + world_offset;
    let cell: vec2<i32> = vec2<i32>(floor(local * f32(uniforms.cellular_tile_size)));
    let shaded: vec2<i32> = cell - uniforms.shaded_origin;
    if any(shaded < vec2<i32>(0)) || any(vec2<u32>(shaded) >= uniforms.shaded_size) { discard; }
    let packed: vec2<u32> =
        shaded_cells[u32(shaded.y) * uniforms.shaded_size.x + u32(shaded.x)];
    // a zero premultiplied color leaves the target unchanged
    if all(packed == vec2<u32>(0u)) { discard; }
    return vec4<f32>(unpack2x16float(packed.x), unpack2x16float(packed.y));
}
