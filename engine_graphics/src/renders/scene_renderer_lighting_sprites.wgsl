// Copyright Rob Gage 2026

// Rasterizes sprite radiance into the lighting domain: gameplay sprites and lights add emission
// and multiply transmission per cell, and parallax backgrounds composite far to near into the
// backlight, blurred by their physical depth.

/// Domain covered by a rasterization
struct SpriteOptics {
    /// Camera-relative position of the domain's bottom-left corner in tiles
    domain_origin: vec2<f32>,
    /// Domain size in tiles
    domain_size: vec2<f32>,
    /// Number of cells along each side of a tile
    cellular_tile_size: f32,
    /// Alignment padding
    padding_0: f32,
    /// Alignment padding
    padding_1: f32,
    /// Alignment padding
    padding_2: f32,
};

/// One sprite or light
struct SpriteInstance {
    /// Camera-relative center and size in tiles
    @location(0) center_size: vec4<f32>,
    /// Rotation in radians
    @location(1) rotation: f32,
    /// Physical depth in tiles of a background, or zero
    @location(2) depth: f32,
    /// Linear emission multiplier and occlusion multiplier
    @location(3) color: vec4<f32>,
    /// Normalized radiance texture region left, top, right, and bottom
    @location(4) radiance_region: vec4<f32>,
    /// Multiplier of the radiance emission
    @location(5) radiance_intensity: f32,
};

struct SpriteVarying {
    @builtin(position) position: vec4<f32>,
    /// Texture coordinates, or disc coordinates for lights
    @location(0) coordinates: vec2<f32>,
    @location(1) @interpolate(flat) depth: f32,
    @location(2) color: vec4<f32>,
    /// Texture size in texels of the sampled region per tile, for the blur level
    @location(3) @interpolate(flat) texels_per_tile: f32,
};

@group(0) @binding(0)
var<uniform> sprite_optics: SpriteOptics;

@group(0) @binding(1)
var radiance_sampler: sampler;

@group(1) @binding(0)
var radiance_texture: texture_2d<f32>;

/// Places one instance corner in the domain
@vertex
fn vertex(@builtin(vertex_index) vertex_index: u32, instance: SpriteInstance) -> SpriteVarying {
    // a triangle strip of four corners
    let corners: array<vec2<f32>, 4> = array<vec2<f32>, 4>(
        vec2<f32>(-0.5, -0.5), vec2<f32>(0.5, -0.5), vec2<f32>(-0.5, 0.5), vec2<f32>(0.5, 0.5),
    );
    let corner: vec2<f32> = corners[vertex_index];
    let scaled: vec2<f32> = corner * instance.center_size.zw;
    let cosine: f32 = cos(instance.rotation);
    let sine: f32 = sin(instance.rotation);
    let rotated: vec2<f32> = vec2<f32>(
        scaled.x * cosine - scaled.y * sine,
        scaled.x * sine + scaled.y * cosine,
    );
    let position: vec2<f32> = instance.center_size.xy + rotated;
    let normalized: vec2<f32> = (position - sprite_optics.domain_origin) / sprite_optics.domain_size;
    var output: SpriteVarying;
    // texel rows count upward like the domain cells, so the first row is the bottom one
    output.position = vec4<f32>(normalized.x * 2.0 - 1.0, 1.0 - normalized.y * 2.0, 0.0, 1.0);
    output.coordinates = vec2<f32>(
        mix(instance.radiance_region.x, instance.radiance_region.z, corner.x + 0.5),
        mix(instance.radiance_region.w, instance.radiance_region.y, corner.y + 0.5),
    );
    output.depth = instance.depth;
    output.color = vec4<f32>(instance.color.rgb * instance.radiance_intensity, instance.color.a);
    let texture_size: vec2<f32> = vec2<f32>(textureDimensions(radiance_texture, 0));
    let region_size: vec2<f32> = abs(instance.radiance_region.zw - instance.radiance_region.xy) * texture_size;
    output.texels_per_tile = max(region_size.x / max(instance.center_size.z, 1e-6), 1e-6);
    return output;
}

/// Adds a gameplay sprite's emission per cell and its transmission per cell
@fragment
fn actor_fragment(input: SpriteVarying) -> @location(0) vec4<f32> {
    let radiance: vec4<f32> = textureSampleLevel(radiance_texture, radiance_sampler, input.coordinates, 0.0);
    let occlusion: f32 = clamp(radiance.a * input.color.a, 0.0, 1.0);
    // occlusion is per tile of thickness, and each texel of this target is one cell
    let transmission: f32 = pow(1.0 - occlusion, 1.0 / sprite_optics.cellular_tile_size);
    return vec4<f32>(radiance.rgb * input.color.rgb, 1.0 - transmission);
}

/// Adds a light's emission per cell over a disc
@fragment
fn light_fragment(input: SpriteVarying) -> @location(0) vec4<f32> {
    let distance: f32 = length(input.coordinates - vec2<f32>(0.5));
    if distance > 0.5 {
        discard;
    }
    return vec4<f32>(input.color.rgb, 0.0);
}

/// Composites a background's emission over the farther backgrounds, blurred by its depth
@fragment
fn background_fragment(input: SpriteVarying) -> @location(0) vec4<f32> {
    // a plane emitter lights a plane at depth `d` over an area about `d` across
    let blur_texels: f32 = max(input.depth, 0.0) * input.texels_per_tile;
    let level: f32 = log2(max(blur_texels, 1.0));
    let radiance: vec4<f32> = textureSampleLevel(radiance_texture, radiance_sampler, input.coordinates, level);
    let occlusion: f32 = clamp(radiance.a * input.color.a, 0.0, 1.0);
    return vec4<f32>(radiance.rgb * input.color.rgb, occlusion);
}
