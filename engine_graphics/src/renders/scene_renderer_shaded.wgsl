// Copyright Rob Gage 2026

struct ShadedUniforms {
    viewport: vec2<f32>,
    pixels_per_tile: f32,
    cellular_tile_size: f32,
    cell_grid_origin: vec2<f32>,
    padding: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> uniforms: ShadedUniforms;

/// The outlines of every body this frame
@group(0) @binding(1)
var<storage, read> outline_points: array<vec2<f32>>;

struct ShadedVertexOutput {
    @builtin(position) position: vec4<f32>,
    /// Camera-relative position in tiles
    @location(0) camera_relative_position: vec2<f32>,
    /// First outline point, how many, whether in whole cells, and the outline's deepest distance
    @location(1) @interpolate(flat) outline: vec4<f32>,
    /// Center of the outline, relative to the camera
    @location(2) @interpolate(flat) center: vec2<f32>,
    @location(3) @interpolate(flat) parameter_0: vec4<f32>,
    @location(4) @interpolate(flat) parameter_1: vec4<f32>,
    @location(5) @interpolate(flat) parameter_2: vec4<f32>,
    @location(6) @interpolate(flat) parameter_3: vec4<f32>,
};

/// What a shader is given about one point of a body
struct ShadeInput {
    /// The point relative to the camera in tiles
    position: vec2<f32>,
    /// Middle of the body's outline relative to the camera
    center: vec2<f32>,
    /// Tiles from the point in to the outline
    distance: f32,
    /// Tiles from the deepest point of the body to the outline
    depth: f32,
    /// Unit direction toward the nearest part of the outline
    outward: vec2<f32>,
    /// The actor's parameters
    parameters: array<vec4<f32>, 4>,
};

/// What a shader returns about one point of a body
struct ShadeOutput {
    /// Linear color and opacity lit by the scene
    albedo: vec4<f32>,
    /// Linear light it shows regardless of the scene's
    emission: vec3<f32>,
    /// Linear light it gives the scene per cell, like a material's radiance; zero unless a shader
    /// sets it
    radiance: vec3<f32>,
    /// Fraction of light it stops per tile of thickness, like a material's radiance alpha; zero
    /// unless a shader sets it
    occlusion: f32,
};

/// Returns what a body's corner hands to its fragments, placed in the view
fn place_corner(
    position: vec2<f32>,
    outline: vec4<f32>,
    center: vec2<f32>,
    parameter_0: vec4<f32>,
    parameter_1: vec4<f32>,
    parameter_2: vec4<f32>,
    parameter_3: vec4<f32>,
) -> ShadedVertexOutput {
    var output: ShadedVertexOutput;
    output.position = vec4<f32>(position * uniforms.pixels_per_tile * 2.0 / uniforms.viewport, 0.0, 1.0);
    output.camera_relative_position = position;
    output.outline = outline;
    output.center = center;
    output.parameter_0 = parameter_0;
    output.parameter_1 = parameter_1;
    output.parameter_2 = parameter_2;
    output.parameter_3 = parameter_3;
    return output;
}

/// Places a body's corner in the view
@vertex
fn vertex(
    @location(0) position: vec2<f32>,
    @location(1) outline: vec4<f32>,
    @location(2) center: vec2<f32>,
    @location(3) parameter_0: vec4<f32>,
    @location(4) parameter_1: vec4<f32>,
    @location(5) parameter_2: vec4<f32>,
    @location(6) parameter_3: vec4<f32>,
) -> ShadedVertexOutput {
    return place_corner(position, outline, center, parameter_0, parameter_1, parameter_2, parameter_3);
}

/// Places a body's corner in the lighting domain, one texel further out so that every texel it
/// touches is drawn
@vertex
fn vertex_optics(
    @location(0) position: vec2<f32>,
    @location(1) outline: vec4<f32>,
    @location(2) center: vec2<f32>,
    @location(3) parameter_0: vec4<f32>,
    @location(4) parameter_1: vec4<f32>,
    @location(5) parameter_2: vec4<f32>,
    @location(6) parameter_3: vec4<f32>,
) -> ShadedVertexOutput {
    var output: ShadedVertexOutput = place_corner(
        position, outline, center, parameter_0, parameter_1, parameter_2, parameter_3,
    );
    let margin: f32 = f32(lighting_view.texel_size) / lighting_view.cellular_tile_size;
    output.position = lighting_domain_clip(position + sign(position - center) * margin);
    return output;
}

/// What a body is at one point, if anything
struct ShadedPoint {
    is_inside: bool,
    shaded: ShadeOutput,
};

/// Shades a point from its distance to the outline, which is what makes a body smooth however
/// coarse the mesh under it
fn shade_point(input: ShadedVertexOutput, point: vec2<f32>) -> ShadedPoint {
    var result: ShadedPoint;
    let first: u32 = u32(input.outline.x);
    let count: u32 = u32(input.outline.y);
    var nearest: f32 = 1e9;
    var toward: vec2<f32> = vec2<f32>(0.0, 1.0);
    for (var index: u32 = 0u; index < count; index++) {
        let a: vec2<f32> = outline_points[first + index];
        let b: vec2<f32> = outline_points[first + (index + 1u) % count];
        let along: vec2<f32> = b - a;
        let fraction: f32 = clamp(dot(point - a, along) / max(dot(along, along), 1e-6), 0.0, 1.0);
        let to_side: vec2<f32> = a + along * fraction - point;
        let squared: f32 = dot(to_side, to_side);
        if squared < nearest {
            nearest = squared;
            toward = to_side;
        }
        if (a.y > point.y) != (b.y > point.y)
            && point.x < a.x + along.x * (point.y - a.y) / along.y
        {
            result.is_inside = !result.is_inside;
        }
    }
    if !result.is_inside { return result; }
    let distance: f32 = sqrt(nearest);
    var shade_input: ShadeInput;
    shade_input.position = point;
    shade_input.center = input.center;
    shade_input.distance = distance;
    shade_input.depth = input.outline.w;
    shade_input.outward = toward / max(distance, 1e-5);
    shade_input.parameters = array<vec4<f32>, 4>(
        input.parameter_0,
        input.parameter_1,
        input.parameter_2,
        input.parameter_3,
    );
    result.shaded = shade(shade_input);
    return result;
}

/// Draws a body as it looks
@fragment
fn fragment(input: ShadedVertexOutput) -> @location(0) vec4<f32> {
    var point: vec2<f32> = input.camera_relative_position;
    if input.outline.z > 0.5 {
        let size: f32 = uniforms.cellular_tile_size;
        point = (floor((point - uniforms.cell_grid_origin) * size) + 0.5) / size + uniforms.cell_grid_origin;
    }
    let shaded_point: ShadedPoint = shade_point(input, point);
    if !shaded_point.is_inside { discard; }
    let shaded: ShadeOutput = shaded_point.shaded;
    let light: vec3<f32> = light_at_tiles(point);
    return vec4<f32>(
        (shaded.albedo.rgb * light + shaded.emission) * shaded.albedo.a,
        shaded.albedo.a,
    );
}

/// Draws a body into the lighting domain like cells of a material: each cell of a lighting texel
/// is shaded at its center and the texel holds their average
@fragment
fn fragment_optics(input: ShadedVertexOutput) -> @location(0) vec4<f32> {
    var emission: vec3<f32> = vec3<f32>(0.0);
    var transmission: f32 = 0.0;
    for (var y: u32 = 0u; y < lighting_view.texel_size; y++) {
        for (var x: u32 = 0u; x < lighting_view.texel_size; x++) {
            let shaded_point: ShadedPoint = shade_point(
                input,
                lighting_texel_cell(input.position.xy, vec2<u32>(x, y)),
            );
            if shaded_point.is_inside {
                let shaded: ShadeOutput = shaded_point.shaded;
                emission += shaded.radiance;
                transmission += lighting_transmission(shaded.occlusion);
            } else {
                transmission += 1.0;
            }
        }
    }
    let count: f32 = f32(lighting_view.texel_size * lighting_view.texel_size);
    return lighting_optics(emission / count, transmission / count);
}
