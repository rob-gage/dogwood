// Copyright Rob Gage 2026

struct CellularUniforms {
    viewport_origin: vec2<f32>,
    viewport_size: vec2<f32>,
    zoom: f32,
    zoom_padding: f32,
    camera_relative_region: vec2<f32>,
    region_width: u32,
    region_height: u32,
    ring_offset_x: u32,
    ring_offset_y: u32,
    cellular_tile_size: u32,
    /// Region-local cell offset to world-aligned surface art, wrapped to its size
    surface_art_offset_x: u32,
    surface_art_offset_y: u32,
    padding: u32,
    /// Region-local cell offset to the surface field, which wraps by world position
    surface_field_offset_x: u32,
    surface_field_offset_y: u32,
};

@group(0) @binding(0)
var<uniform> uniforms: CellularUniforms;

@group(0) @binding(1)
var<storage, read> material_identifiers: array<u32>;

@group(0) @binding(2)
var<storage, read> colors: array<vec2<u32>>;

/// Temperature bits then amount of every cell
@group(0) @binding(3)
var<storage, read> matter: array<vec2<u32>>;

/// Closeness to the opposite state, direction toward open space, and solidity of each cell
@group(0) @binding(4)
var surface_field: texture_2d<f32>;

/// One plus the first surface art layer of each thermal index, or zero with no surface
@group(0) @binding(5)
var<storage, read> surface_layers: array<u32>;

/// World-aligned surface art with color and, in alpha, how deep each texel reaches
@group(0) @binding(6)
var surface_art: texture_2d_array<f32>;

/// Side length of each surface art tile
const SURFACE_ART_SIZE: u32 = 256u;

/// Returns a stable value in zero to one for an art cell
fn surface_hash(cell: vec2<u32>) -> f32 {
    let mixed: u32 = (cell.x * 1597u + cell.y * 28411u) * 2654435761u;
    return f32((mixed ^ (mixed >> 15u)) & 1023u) / 1023.0;
}

/// Returns a cell's color with its material's surface art where the art's reach covers its
/// closeness to open space, choosing up or down facing art per cell so it stays crisp
fn surfaced_color(
    linear_color: vec4<f32>,
    cell: vec2<i32>,
    thermal: u32,
) -> vec4<f32> {
    let layer: u32 = surface_layers[thermal];
    if layer == 0u { return linear_color; }
    let field_size: vec2<i32> = vec2<i32>(textureDimensions(surface_field));
    let field: vec4<f32> = textureLoad(
        surface_field,
        (cell + vec2<i32>(
            i32(uniforms.surface_field_offset_x),
            i32(uniforms.surface_field_offset_y),
        )) % field_size,
        0,
    );
    if field.r <= 0.0 { return linear_color; }
    let art_cell: vec2<u32> = vec2<u32>(
        (u32(cell.x) + uniforms.surface_art_offset_x) % SURFACE_ART_SIZE,
        (u32(cell.y) + uniforms.surface_art_offset_y) % SURFACE_ART_SIZE,
    );
    let facing: f32 = field.b * 2.0 - 1.0;
    let is_up: bool = facing * 0.5 + 0.5 + (surface_hash(art_cell) - 0.5) * 0.45 >= 0.5;
    let texel: vec4<f32> = textureLoad(
        surface_art,
        vec2<u32>(art_cell.x, SURFACE_ART_SIZE - 1u - art_cell.y),
        layer - 1u + select(1u, 0u, is_up),
        0,
    );
    if field.r > 1.0 - texel.a { return vec4<f32>(texel.rgb, linear_color.a); }
    // beyond the rim the color shifts in steps by which way the surface faces, warmer and lighter
    // facing up, cooler and darker facing down, so edges read as turned toward or away from light
    let band: f32 = ceil(field.r * 3.0) / 3.0;
    let tilt: vec3<f32> = select(
        vec3<f32>(0.72, 0.74, 0.82),
        vec3<f32>(1.26, 1.2, 1.08),
        is_up,
    );
    return vec4<f32>(linear_color.rgb * mix(vec3<f32>(1.0), tilt, band), linear_color.a);
}

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

/// Draws one cellular framebuffer pixel
@fragment
fn fragment(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let pixel: vec2<f32> = position.xy - uniforms.viewport_origin;
    let centered_pixel: vec2<f32> = pixel - uniforms.viewport_size * 0.5;
    let world_offset: vec2<f32> = vec2<f32>(
        centered_pixel.x / uniforms.zoom,
        -centered_pixel.y / uniforms.zoom,
    );
    let local: vec2<f32> = uniforms.camera_relative_region + world_offset;
    let cellular_tile_size_float: f32 = f32(uniforms.cellular_tile_size);
    let cell: vec2<i32> = vec2<i32>(floor(local * cellular_tile_size_float));
    let region_cell_width: i32 = i32(uniforms.region_width * uniforms.cellular_tile_size);
    let region_cell_height: i32 = i32(uniforms.region_height * uniforms.cellular_tile_size);
    if cell.x < 0 || cell.y < 0 || cell.x >= region_cell_width || cell.y >= region_cell_height {
        discard;
    }
    let cell_size: i32 = i32(uniforms.cellular_tile_size);
    let tile: vec2<i32> = cell / cell_size;
    let in_tile: vec2<i32> = cell % cell_size;
    let physical_x: u32 = (u32(tile.x) + uniforms.ring_offset_x) % uniforms.region_width;
    let physical_y: u32 = (u32(tile.y) + uniforms.ring_offset_y) % uniforms.region_height;
    let slot: u32 = physical_y * uniforms.region_width + physical_x;
    let cell_index: u32 = slot * uniforms.cellular_tile_size * uniforms.cellular_tile_size
        + u32(in_tile.y) * uniforms.cellular_tile_size
        + u32(in_tile.x);
    let material_identifier: u32 = material_identifiers[cell_index];
    if material_identifier == 0u {
        discard;
    }
    let srgb_color: vec4<f32> = unpack4x8unorm(colors[cell_index].x);
    let thermal: u32 = thermal_index(material_identifier);
    let linear_color: vec4<f32> = surfaced_color(
        vec4<f32>(
            srgb_to_linear(srgb_color.r),
            srgb_to_linear(srgb_color.g),
            srgb_to_linear(srgb_color.b),
            srgb_color.a,
        ),
        cell,
        thermal,
    );
    // each cell keeps the color and radiance it was given when created, shown through its
    // material's temperature colors
    let temperature: f32 = bitcast<f32>(matter[cell_index].x);
    let amount: f32 = f32(matter[cell_index].y) / 65536.0;
    let amount_color_value: vec4<f32> = amount_color(thermal, amount, linear_color, false);
    let amount_radiance: vec4<f32> = amount_color(
        thermal, amount, unpack_radiance(colors[cell_index].y), true);
    let color: vec4<f32> = temperature_color(thermal, temperature, amount_color_value, false);
    let radiance: vec4<f32> = temperature_color(thermal, temperature, amount_radiance, true);
    return lit_color(color, light_at(local * cellular_tile_size_float), radiance.rgb);
}
