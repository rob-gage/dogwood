// Copyright Rob Gage 2026

/// Values shared by overlay vertices and fragments
struct OverlayUniforms {
    /// Origin of the viewport in target pixels
    viewport_origin: vec2<f32>,
    /// Size of the viewport in target pixels
    viewport_size: vec2<f32>,
    /// Target pixels per tile
    zoom: f32,
    /// Size of one cellular cell in tiles
    cell_size: f32,
    /// Visible scene width and height in tiles
    visible_size: vec2<f32>,
    /// Camera-relative origin of the cellular grid in tiles
    cell_grid_origin: vec2<f32>,
    /// Uniform alignment padding
    padding: vec2<f32>,
};

/// One area supplied by scene or host graphics
struct OverlayInstance {
    /// Camera-relative center in tiles
    @location(0) center: vec2<f32>,
    /// Zero for a circle and one for a rectangle
    @location(1) shape: u32,
    /// Circle radius or rectangle half-size in tiles
    @location(2) size: vec2<f32>,
    /// Packed RGBA8 color
    @location(3) color: u32,
};

/// Overlay position interpolated across the bounding quad
struct OverlayVertex {
    @builtin(position) position: vec4<f32>,
    @location(0) world_position: vec2<f32>,
    @location(1) center: vec2<f32>,
    @location(2) @interpolate(flat) shape: u32,
    @location(3) size: vec2<f32>,
    @location(4) @interpolate(flat) color: u32,
};

@group(0) @binding(0)
var<uniform> uniforms: OverlayUniforms;

/// Creates one bounding quad for each overlay
@vertex
fn vertex(
    @builtin(vertex_index) vertex_index: u32,
    instance: OverlayInstance,
) -> OverlayVertex {
    let corners: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(1.0, -1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, 1.0),
    );
    let corner: vec2<f32> = corners[vertex_index];
    let extent: vec2<f32> = select(
        vec2<f32>(instance.size.x),
        instance.size,
        instance.shape == 1u,
    ) + vec2<f32>(uniforms.cell_size * 0.5);
    let world_position: vec2<f32> = instance.center + corner * extent;
    var output: OverlayVertex;
    output.position = vec4<f32>(
        world_position.x * 2.0 / uniforms.visible_size.x,
        world_position.y * 2.0 / uniforms.visible_size.y,
        0.0,
        1.0,
    );
    output.world_position = world_position;
    output.center = instance.center;
    output.shape = instance.shape;
    output.size = instance.size;
    output.color = instance.color;
    return output;
}

/// Tests a cellular cell center against an overlay shape
fn contains(center: vec2<f32>, shape: u32, size: vec2<f32>, cell_center: vec2<f32>) -> bool {
    let offset: vec2<f32> = abs(cell_center - center);
    if shape == 0u {
        return dot(offset, offset) <= size.x * size.x;
    }
    return all(offset <= size);
}

/// Converts an sRGB channel to linear light, matching cellular colors
fn srgb_to_linear(value: f32) -> f32 {
    if value <= 0.04045 {
        return value / 12.92;
    }
    return pow((value + 0.055) / 1.055, 2.4);
}

/// Draws tinted cells and a darker band along the covered cell boundary
@fragment
fn fragment(input: OverlayVertex) -> @location(0) vec4<f32> {
    let cell_position: vec2<f32> =
        (input.world_position - uniforms.cell_grid_origin) / uniforms.cell_size;
    let cell: vec2<f32> = floor(cell_position);
    let cell_center: vec2<f32> = uniforms.cell_grid_origin
        + (cell + vec2<f32>(0.5)) * uniforms.cell_size;
    if !contains(cell_center, input.shape, input.size, input.center) {
        discard;
    }
    let has_outside_neighbor: bool = !contains(
        cell_center + vec2<f32>(uniforms.cell_size, 0.0),
        input.shape,
        input.size,
        input.center,
    ) || !contains(
        cell_center - vec2<f32>(uniforms.cell_size, 0.0),
        input.shape,
        input.size,
        input.center,
    ) || !contains(
        cell_center + vec2<f32>(0.0, uniforms.cell_size),
        input.shape,
        input.size,
        input.center,
    ) || !contains(
        cell_center - vec2<f32>(0.0, uniforms.cell_size),
        input.shape,
        input.size,
        input.center,
    );
    let within_cell: vec2<f32> = fract(cell_position);
    let edge_distance: f32 = min(
        min(within_cell.x, 1.0 - within_cell.x),
        min(within_cell.y, 1.0 - within_cell.y),
    );
    let is_border: bool = has_outside_neighbor && edge_distance < 0.125;
    let packed_color: vec4<f32> = unpack4x8unorm(input.color);
    var color: vec4<f32> = vec4<f32>(
        srgb_to_linear(packed_color.r),
        srgb_to_linear(packed_color.g),
        srgb_to_linear(packed_color.b),
        packed_color.a,
    );
    if is_border {
        color = vec4<f32>(color.rgb * 0.45, min(color.a * 1.6, 1.0));
    }
    return vec4<f32>(color.rgb * color.a, color.a);
}
