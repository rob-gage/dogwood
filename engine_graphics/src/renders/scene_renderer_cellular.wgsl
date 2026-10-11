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
    material_count: u32,
    padding_1: u32,
    padding_2: u32,
};

@group(0) @binding(0)
var<uniform> uniforms: CellularUniforms;

@group(0) @binding(1)
var<storage, read> material_identifiers: array<u32>;

@group(0) @binding(2)
var<storage, read> colors: array<u32>;

struct CellularMaterial {
    atlas: vec4<u32>,
    repetition_size: vec2<f32>,
    padding: vec2<u32>,
};

@group(0) @binding(3)
var<storage, read> materials: array<CellularMaterial>;

@group(0) @binding(4)
var material_atlas: texture_2d<f32>;

@group(0) @binding(5)
var<storage, read> material_phases: array<vec2<f32>>;

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
    let srgb_color: vec4<f32> = unpack4x8unorm(colors[cell_index]);
    let linear_rgb: vec3<f32> = vec3<f32>(
        srgb_to_linear(srgb_color.r),
        srgb_to_linear(srgb_color.g),
        srgb_to_linear(srgb_color.b),
    );
    // tint the repeated static material albedo, or draw the tint alone without an albedo frame
    var albedo: vec4<f32> = vec4<f32>(1.0);
    let material_index: u32 = material_identifier & 0x3fffffffu;
    if material_identifier >> 30u == 0u && material_index < uniforms.material_count {
        let material: CellularMaterial = materials[material_index];
        if material.atlas.z != 0u {
            let repetition: vec2<f32> =
                fract(material_phases[material_index] + local / material.repetition_size);
            let texture_coordinate: vec2<f32> = vec2<f32>(repetition.x, 1.0 - repetition.y);
            let texel: vec2<u32> = min(
                vec2<u32>(texture_coordinate * vec2<f32>(material.atlas.zw)),
                material.atlas.zw - vec2<u32>(1u),
            );
            albedo = textureLoad(material_atlas, material.atlas.xy + texel, 0);
        }
    }
    let alpha: f32 = albedo.a * srgb_color.a;
    return vec4<f32>(albedo.rgb * linear_rgb * alpha, alpha);
}
