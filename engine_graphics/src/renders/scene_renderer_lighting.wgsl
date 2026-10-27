// Copyright Rob Gage 2026

// Lighting shared by the appearance passes, bound as their lighting group. The including shader
// declares `LIGHTING_GROUP` and provides `MATERIAL_RADIANCE_WGSL`.

/// Lighting values shared by every appearance pass in one view
struct LightingView {
    /// Lighting domain size in cells
    domain_size: vec2<f32>,
    /// Camera-relative position of the domain's bottom-left corner in tiles
    domain_origin: vec2<f32>,
    /// Linear light added everywhere
    ambient: vec3<f32>,
    /// Number of cells along each side of a tile
    cellular_tile_size: f32,
    /// First thermal index of each material form
    thermal_offsets: vec4<u32>,
    /// Whether the light texture covers the domain
    has_light: u32,
    /// Number of mip levels of the light texture
    light_mip_count: f32,
    /// Alignment padding
    padding_0: u32,
    /// Alignment padding
    padding_1: u32,
};

@group(LIGHTING_GROUP) @binding(0)
var<uniform> lighting_view: LightingView;

/// Light of every domain cell, with mip levels averaging it over growing areas
@group(LIGHTING_GROUP) @binding(1)
var light_texture: texture_2d<f32>;

@group(LIGHTING_GROUP) @binding(2)
var light_sampler: sampler;

/// Temperature color keys: one `(first vector, key count)` header per thermal index, then keys
/// of eleven vectors each
@group(LIGHTING_GROUP) @binding(3)
var<storage, read> temperature_colors: array<vec4<f32>>;

/// Returns the light at a domain position in cells
fn light_at(domain_position: vec2<f32>) -> vec3<f32> {
    if lighting_view.has_light == 0u {
        return lighting_view.ambient;
    }
    let coordinates: vec2<f32> = domain_position / lighting_view.domain_size;
    return lighting_view.ambient
        + textureSampleLevel(light_texture, light_sampler, coordinates, 0.0).rgb;
}

/// Returns the light at a camera-relative position in tiles
fn light_at_tiles(camera_relative: vec2<f32>) -> vec3<f32> {
    return light_at((camera_relative - lighting_view.domain_origin) * lighting_view.cellular_tile_size);
}

/// Returns the light a background at a physical depth in tiles receives from the gameplay plane
/// in front of it: the gameplay light averaged over an area growing with depth, less the part of
/// that area outside the domain
fn background_light_at_tiles(camera_relative: vec2<f32>, depth: f32) -> vec3<f32> {
    if lighting_view.has_light == 0u {
        return lighting_view.ambient;
    }
    let depth_cells: f32 = max(depth, 0.0) * lighting_view.cellular_tile_size;
    let level: f32 = clamp(log2(max(depth_cells, 1.0)), 0.0, lighting_view.light_mip_count - 1.0);
    let coordinates: vec2<f32> = (camera_relative - lighting_view.domain_origin)
        * lighting_view.cellular_tile_size / lighting_view.domain_size;
    let radius: f32 = 0.5 * min(lighting_view.domain_size.x, lighting_view.domain_size.y);
    let coverage: f32 = 1.0 - depth_cells / sqrt(radius * radius + depth_cells * depth_cells);
    return lighting_view.ambient
        + coverage * textureSampleLevel(light_texture, light_sampler, coordinates, level).rgb;
}

/// Returns the thermal index of a material identifier
fn thermal_index(material_identifier: u32) -> u32 {
    return lighting_view.thermal_offsets[material_identifier >> 30u]
        + (material_identifier & 0x3fffffffu);
}

/// Applies the temperature color keys of a thermal index to a linear color, or to a radiance
/// when `is_radiance` holds
fn temperature_color(thermal_index: u32, temperature: f32, value: vec4<f32>, is_radiance: bool) -> vec4<f32> {
    let header: vec4<u32> = bitcast<vec4<u32>>(temperature_colors[thermal_index]);
    let count: u32 = header.y;
    if count == 0u {
        return value;
    }
    var first: u32 = header.x;
    var second: u32 = header.x;
    var weight: f32 = 0.0;
    for (var key: u32 = 1u; key < count; key++) {
        let candidate: u32 = header.x + key * 11u;
        if temperature_colors[candidate].x <= temperature {
            first = candidate;
            second = candidate;
        } else {
            second = candidate;
            let low: f32 = temperature_colors[first].x;
            let high: f32 = temperature_colors[second].x;
            weight = clamp((temperature - low) / max(high - low, 1e-6), 0.0, 1.0);
            if temperature < low {
                weight = 0.0;
            }
            break;
        }
    }
    let base: u32 = select(1u, 6u, is_radiance);
    var transformed: vec4<f32>;
    for (var row: u32 = 0u; row < 4u; row++) {
        let matrix_row: vec4<f32> = mix(
            temperature_colors[first + base + row],
            temperature_colors[second + base + row],
            weight,
        );
        transformed[row] = dot(matrix_row, value);
    }
    transformed += mix(temperature_colors[first + base + 4u], temperature_colors[second + base + 4u], weight);
    if is_radiance {
        return vec4<f32>(
            clamp(transformed.rgb, vec3<f32>(0.0), vec3<f32>(MATERIAL_RADIANCE_MAXIMUM)),
            clamp(transformed.a, 0.0, 1.0),
        );
    }
    return clamp(transformed, vec4<f32>(0.0), vec4<f32>(1.0));
}

/// Returns a lit premultiplied color: the linear color lit by the light, plus its emission
/// covered by its alpha
fn lit_color(linear_color: vec4<f32>, light: vec3<f32>, emission: vec3<f32>) -> vec4<f32> {
    return vec4<f32>((linear_color.rgb * light + emission) * linear_color.a, linear_color.a);
}
