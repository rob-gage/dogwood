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
    /// Cells along each side of a lighting texel
    texel_size: f32,
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
    /// Normalized radiance texture region left, top, right, and bottom
    @location(4) @interpolate(flat) region: vec4<f32>,
    /// Radius in tiles of a light
    @location(5) @interpolate(flat) radius: f32,
};

@group(0) @binding(0)
var<uniform> sprite_optics: SpriteOptics;

@group(0) @binding(1)
var radiance_sampler: sampler;

@group(1) @binding(0)
var radiance_texture: texture_2d<f32>;

/// Places one instance corner in the domain, pushing its edges out by a margin in tiles
fn place(vertex_index: u32, instance: SpriteInstance, margin: f32) -> SpriteVarying {
    // a triangle strip of four corners
    let corners: array<vec2<f32>, 4> = array<vec2<f32>, 4>(
        vec2<f32>(-0.5, -0.5), vec2<f32>(0.5, -0.5), vec2<f32>(-0.5, 0.5), vec2<f32>(0.5, 0.5),
    );
    let sizes: vec2<f32> = max(instance.center_size.zw, vec2<f32>(1e-6));
    let corner: vec2<f32> = corners[vertex_index] * (sizes + margin * 2.0) / sizes;
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
    output.region = instance.radiance_region;
    output.radius = instance.center_size.z * 0.5;
    return output;
}

/// Places a background's corner in the domain
@vertex
fn vertex(@builtin(vertex_index) vertex_index: u32, instance: SpriteInstance) -> SpriteVarying {
    return place(vertex_index, instance, 0.0);
}

/// Places the corner of something averaged over each texel it touches, one texel further out so
/// that a thing smaller than a texel still reaches the texels around it wherever it moves
@vertex
fn vertex_covering(@builtin(vertex_index) vertex_index: u32, instance: SpriteInstance) -> SpriteVarying {
    return place(vertex_index, instance, sprite_optics.texel_size / sprite_optics.cellular_tile_size);
}

/// Adds a gameplay sprite's emission and transmission averaged over one lighting texel, so that it
/// keeps its brightness and place however it falls on the texels
@fragment
fn actor_fragment(input: SpriteVarying) -> @location(0) vec4<f32> {
    let side: u32 = select(4u, 2u, sprite_optics.texel_size <= 1.0);
    let across: vec2<f32> = dpdx(input.coordinates);
    let along: vec2<f32> = dpdy(input.coordinates);
    let texture_size: vec2<f32> = vec2<f32>(textureDimensions(radiance_texture, 0));
    let footprint: f32 = max(length(across * texture_size), length(along * texture_size)) / f32(side);
    let level: f32 = log2(max(footprint, 1.0));
    let low: vec2<f32> = min(input.region.xy, input.region.zw);
    let high: vec2<f32> = max(input.region.xy, input.region.zw);
    var emission: vec3<f32> = vec3<f32>(0.0);
    var transmission: f32 = 0.0;
    for (var y: u32 = 0u; y < side; y++) {
        for (var x: u32 = 0u; x < side; x++) {
            let offset: vec2<f32> = (vec2<f32>(f32(x), f32(y)) + 0.5) / f32(side) - 0.5;
            let coordinates: vec2<f32> = input.coordinates + across * offset.x + along * offset.y;
            if any(coordinates < low) || any(coordinates > high) {
                transmission += 1.0;
                continue;
            }
            let radiance: vec4<f32> = textureSampleLevel(radiance_texture, radiance_sampler, coordinates, level);
            emission += radiance.rgb * input.color.rgb;
            // occlusion is per tile of thickness, and each texel spans `texel_size` cells
            transmission += pow(
                1.0 - clamp(radiance.a * input.color.a, 0.0, 1.0),
                sprite_optics.texel_size / sprite_optics.cellular_tile_size,
            );
        }
    }
    let count: f32 = f32(side * side);
    return vec4<f32>(emission / count * sprite_optics.texel_size, 1.0 - transmission / count);
}

/// Adds a light's emission over each lighting texel of a disc, by how much of the texel the disc
/// covers, and scaled to emit the same total as the disc however small it is against the texels
@fragment
fn light_fragment(input: SpriteVarying) -> @location(0) vec4<f32> {
    let texels_per_tile: f32 = sprite_optics.cellular_tile_size / sprite_optics.texel_size;
    let radius: f32 = input.radius * texels_per_tile;
    let distance: f32 = length(input.coordinates - vec2<f32>(0.5)) * 2.0 * radius;
    let coverage: f32 = clamp(radius + 0.5 - distance, 0.0, 1.0);
    if coverage <= 0.0 {
        discard;
    }
    // the coverage's total over the plane is `pi * (r^2 + 1/12)`, or `pi * (r + 1/2)^3 / 3` when the
    // disc fits inside one texel, against the disc's area of `pi * r^2`
    let squared: f32 = radius * radius;
    var scale: f32 = squared / (squared + 1.0 / 12.0);
    if radius < 0.5 {
        scale = 3.0 * squared / pow(radius + 0.5, 3.0);
    }
    return vec4<f32>(input.color.rgb * sprite_optics.texel_size * coverage * scale, 0.0);
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
