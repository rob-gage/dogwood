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
    /// Simulated scene time in seconds, which moves the fluid texture
    time: f32,
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
var<storage, read> fluid_colors: array<u32>;

@group(0) @binding(4)
var<storage, read_write> water_cells: array<u32>;

@group(0) @binding(5)
var<storage, read_write> shaded_cells: array<vec4<u32>>;

/// Fixed-point heat capacity, energy, conduction, and temperature change of every cell's fluid
@group(0) @binding(6)
var<storage, read> fluid_heat: array<vec4<i32>>;

/// Per-cell material slots and their amounts in `AMOUNT_PER_UNIT`
@group(0) @binding(7)
var<storage, read> fluid_species: array<u32>;

/// Cellular material identifier of every cell, zero where no solid matter is
@group(0) @binding(8)
var<storage, read> material_identifiers: array<u32>;

/// Three records per fluid material: linear RGB absorption per cell of depth and opacity gained
/// per cell of depth; surface brightness, surface sheen, surface band height in cells, and a
/// reserved value; then shimmer strength, texture feature size in cells, texture speed, and crust
/// strength
@group(0) @binding(9)
var<storage, read> fluid_graphics: array<vec4<f32>>;

/// Smoothed occupancy above which a cell counts as filled with fluid
const SURFACE_OCCUPANCY: f32 = 0.5;

/// Fixed-point scale of fluid radiance emission sums
const FLUID_EMISSION_SCALE: f32 = 64.0;

/// Rows classified above the shaded rectangle for the depth search
const DEPTH_SEARCH_ROWS: u32 = 16u;

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

/// Returns the amount of one material in a physical cell, or zero when it is absent
fn fluid_amount(cell_index: u32, material_index: u32) -> f32 {
    let base: u32 = cell_index * 4u;
    let slots: u32 = fluid_species[base];
    if (slots & 0xffffu) == material_index + 1u {
        return f32(fluid_species[base + 1u]) / 65536.0;
    }
    if (slots >> 16u) == material_index + 1u {
        return f32(fluid_species[base + 2u]) / 65536.0;
    }
    return 0.0;
}

/// Reads the fixed-point mass and material identifier at an in-region cell
fn raster_mass_and_material(cell: vec2<i32>) -> vec2<u32> {
    let base: u32 = physical_cell_index(cell) * 4u;
    let mass: u32 = u32(max(fluid_raster[base], 0));
    let material_identifier: u32 = bitcast<u32>(fluid_raster[base + 3u]);
    return vec2<u32>(mass, material_identifier);
}

/// Returns whether an in-region logical cell holds solid matter
fn is_solid(cell: vec2<i32>) -> bool {
    let region_cell_width: i32 = i32(uniforms.region_width * uniforms.cellular_tile_size);
    let region_cell_height: i32 = i32(uniforms.region_height * uniforms.cellular_tile_size);
    if cell.x < 0 || cell.y < 0 || cell.x >= region_cell_width || cell.y >= region_cell_height {
        return false;
    }
    return material_identifiers[physical_cell_index(cell)] != 0u;
}

/// Smooths a 3x3 neighborhood and applies the half-cell occupancy threshold
fn sample_fluid(cell: vec2<i32>) -> FluidSample {
    let region_cell_width: i32 = i32(uniforms.region_width * uniforms.cellular_tile_size);
    let region_cell_height: i32 = i32(uniforms.region_height * uniforms.cellular_tile_size);
    var weighted_mass: f32 = 0.0;
    var open_weight: f32 = 0.0;
    var dominant_identifier: u32 = 0u;
    var dominant_mass: f32 = 0.0;
    for (var offset_y: i32 = -1; offset_y <= 1; offset_y++) {
        for (var offset_x: i32 = -1; offset_x <= 1; offset_x++) {
            let neighbor: vec2<i32> = cell + vec2<i32>(offset_x, offset_y);
            if neighbor.x < 0 || neighbor.y < 0
                || neighbor.x >= region_cell_width || neighbor.y >= region_cell_height
            {
                open_weight += select(select(2.0, 1.0, offset_x != 0 && offset_y != 0), 4.0,
                    offset_x == 0 && offset_y == 0);
                continue;
            }
            let weight: f32 = select(2.0, 1.0, offset_x != 0 && offset_y != 0);
            let center_weighted: f32 = select(weight, 4.0, offset_x == 0 && offset_y == 0);
            // solid neighbors hold no fluid and do not thin the fluid against them
            if material_identifiers[physical_cell_index(neighbor)] != 0u { continue; }
            let sample: vec2<u32> = raster_mass_and_material(neighbor);
            open_weight += center_weighted;
            weighted_mass += f32(sample.x) * center_weighted;
            // the material with the most weighted mass leads, so where fluids meet each side keeps
            // its own material instead of the one with the higher identifier
            let sample_mass: f32 = f32(sample.x) * center_weighted;
            if sample.x > 0u && sample_mass > dominant_mass {
                dominant_identifier = sample.y;
                dominant_mass = sample_mass;
            }
        }
    }
    if dominant_identifier == 0u || open_weight <= 0.0 {
        return FluidSample(false, 0u);
    }
    let material_index: u32 = dominant_identifier & 0x3fffffffu;
    let density: f32 = fluid_materials[material_index * 2u].x;
    let smoothed_mass: f32 = weighted_mass / (FLUID_RASTER_SCALE * open_weight);
    let occupancy: f32 = smoothed_mass / density;
    return FluidSample(occupancy > SURFACE_OCCUPANCY, dominant_identifier);
}

/// Records the dominant material of each smoothed water cell in and just above the shaded
/// rectangle, or zero for cells below the occupancy threshold
@compute @workgroup_size(8, 8)
fn classify(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let rows: u32 = uniforms.shaded_size.y + DEPTH_SEARCH_ROWS;
    if invocation.x >= uniforms.shaded_size.x || invocation.y >= rows { return; }
    let cell: vec2<i32> = uniforms.shaded_origin + vec2<i32>(invocation.xy);
    let index: u32 = invocation.y * uniforms.shaded_size.x + invocation.x;
    if is_solid(cell) {
        water_cells[index] = 0u;
        return;
    }
    let sample: FluidSample = sample_fluid(cell);
    water_cells[index] = select(0u, sample.material_identifier, sample.is_water);
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
        shaded_cells[index * 2u] = vec4<u32>(0u);
        shaded_cells[index * 2u + 1u] = vec4<u32>(0u);
        return;
    }
    var depth: u32 = 0u;
    for (var above: u32 = 1u; above <= DEPTH_SEARCH_ROWS; above++) {
        if water_cell(invocation.xy + vec2<u32>(0u, above)) == 0u { break; }
        depth++;
    }
    let cell: vec2<i32> = uniforms.shaded_origin + vec2<i32>(invocation.xy);
    // an open surface: neither fluid nor solid matter lies directly above
    let surface: bool = water_cell(invocation.xy + vec2<u32>(0u, 1u)) == 0u
        && !is_solid(cell + vec2<i32>(0, 1));
    let region_cell_width: i32 = i32(uniforms.region_width * uniforms.cellular_tile_size);
    let region_cell_height: i32 = i32(uniforms.region_height * uniforms.cellular_tile_size);
    var color_mass: f32 = 0.0;
    var weighted_color: vec4<f32> = vec4<f32>(0.0);
    var weighted_radiance: vec4<f32> = vec4<f32>(0.0);
    for (var offset_y: i32 = -1; offset_y <= 1; offset_y++) {
        for (var offset_x: i32 = -1; offset_x <= 1; offset_x++) {
            let neighbor: vec2<i32> = cell + vec2<i32>(offset_x, offset_y);
            if neighbor.x < 0 || neighbor.y < 0
                || neighbor.x >= region_cell_width || neighbor.y >= region_cell_height
            { continue; }
            let neighbor_index: u32 = physical_cell_index(neighbor);
            let neighbor_base: u32 = neighbor_index * 4u;
            let mass: f32 = f32(max(fluid_raster[neighbor_base], 0));
            if mass <= 0.0 { continue; }
            color_mass += mass;
            let color_base: u32 = neighbor_index * 8u;
            weighted_color += vec4<f32>(
                f32(fluid_colors[color_base]),
                f32(fluid_colors[color_base + 1u]),
                f32(fluid_colors[color_base + 2u]),
                f32(fluid_colors[color_base + 3u]),
            );
            weighted_radiance += vec4<f32>(
                f32(fluid_colors[color_base + 4u]) / FLUID_EMISSION_SCALE,
                f32(fluid_colors[color_base + 5u]) / FLUID_EMISSION_SCALE,
                f32(fluid_colors[color_base + 6u]) / FLUID_EMISSION_SCALE,
                f32(fluid_colors[color_base + 7u]) / 255.0,
            );
        }
    }
    var radiance: vec4<f32> = vec4<f32>(0.0);
    if color_mass > 0.0 {
        radiance = weighted_radiance / color_mass;
    }
    var srgb_color: vec4<f32>;
    if color_mass > 0.0 {
        srgb_color = weighted_color / color_mass / 255.0;
    } else {
        let material_index: u32 = material_identifier & 0x3fffffffu;
        let packed_color: u32 = bitcast<u32>(fluid_materials[material_index * 2u + 1u].w);
        srgb_color = vec4<f32>(
            f32(packed_color & 0xffu),
            f32((packed_color >> 8u) & 0xffu),
            f32((packed_color >> 16u) & 0xffu),
            f32((packed_color >> 24u) & 0xffu),
        ) / 255.0;
    }
    // the fluid shows its material's amount and temperature colors at its cell values
    let heat: vec4<i32> = fluid_heat[physical_cell_index(cell)];
    let thermal: u32 = thermal_index(material_identifier);
    let amount: f32 = fluid_amount(physical_cell_index(cell), material_identifier & 0x3fffffffu);
    var linear_color: vec4<f32> = vec4<f32>(
        srgb_to_linear(srgb_color.r),
        srgb_to_linear(srgb_color.g),
        srgb_to_linear(srgb_color.b),
        srgb_color.a,
    );
    linear_color = amount_color(thermal, amount, linear_color, false);
    radiance = amount_color(thermal, amount, radiance, true);
    if heat.x > 0 {
        let temperature: f32 = (f32(heat.y) / 64.0) / (f32(heat.x) / 1024.0) + bitcast<f32>(heat.w);
        linear_color = temperature_color(thermal, temperature, linear_color, false);
        radiance = temperature_color(thermal, temperature, radiance, true);
    }
    // light passing down through the fluid is absorbed per channel and the fluid grows more
    // opaque, by the material's own rates
    let material_index: u32 = material_identifier & 0x3fffffffu;
    let depth_record: vec4<f32> = fluid_graphics[material_index * 3u];
    let surface_record: vec4<f32> = fluid_graphics[material_index * 3u + 1u];
    let texture_record: vec4<f32> = fluid_graphics[material_index * 3u + 2u];
    let depth_cells: f32 = f32(depth);
    let absorption: vec3<f32> = exp(-depth_record.rgb * depth_cells);
    let alpha: f32 = 1.0 - (1.0 - clamp(linear_color.a, 0.0, 1.0)) * exp(-depth_record.a * depth_cells);
    let light: vec3<f32> = light_at(vec2<f32>(cell) + 0.5);
    let body: vec3<f32> = linear_color.rgb * absorption * light;
    // emission stays apart from the lit body so the fragment can texture it
    let premultiplied: vec4<f32> = vec4<f32>(body * alpha, alpha);
    // the surface band brightens the fluid and adds a sheen of the light falling on it
    var band: vec4<f32> = vec4<f32>(0.0);
    if surface && surface_record.z > 0.0 {
        band = vec4<f32>(
            body * alpha * (surface_record.x - 1.0) + light * surface_record.y * alpha,
            min(surface_record.z, 1.0),
        );
    }
    // a shaded cell never packs to zero, which marks cells without fluid
    shaded_cells[index * 2u] = vec4<u32>(
        pack2x16float(vec2<f32>(premultiplied.r, premultiplied.g)),
        pack2x16float(vec2<f32>(premultiplied.b, max(premultiplied.a, 1e-3))),
        pack2x16float(radiance.rg),
        pack2x16float(vec2<f32>(radiance.b, 0.0)),
    );
    shaded_cells[index * 2u + 1u] = vec4<u32>(
        pack2x16float(band.rg),
        pack2x16float(band.ba),
        pack2x16float(texture_record.xy),
        pack2x16float(texture_record.zw),
    );
}

/// What one shaded fluid cell shows
struct FluidCell {
    /// Premultiplied lit color and alpha
    body: vec4<f32>,
    /// Linear emission
    emission: vec3<f32>,
    /// Shimmer strength, feature size in cells, speed, and crust strength
    texture: vec4<f32>,
};

/// Returns the shaded-rectangle index of a logical cell, or none outside the rectangle
fn shaded_index(cell: vec2<i32>) -> i32 {
    let local: vec2<i32> = cell - uniforms.shaded_origin;
    if any(local < vec2<i32>(0)) || any(vec2<u32>(local) >= uniforms.shaded_size) {
        return -1;
    }
    return local.y * i32(uniforms.shaded_size.x) + local.x;
}

/// Returns whether a logical cell holds shaded fluid
fn is_shaded(index: i32) -> bool {
    return index >= 0 && any(shaded_cells[index * 2] != vec4<u32>(0u));
}

/// Unpacks the shaded fluid of a shaded-rectangle index
fn fluid_cell(index: i32) -> FluidCell {
    let first: vec4<u32> = shaded_cells[index * 2];
    let second: vec4<u32> = shaded_cells[index * 2 + 1];
    return FluidCell(
        vec4<f32>(unpack2x16float(first.x), unpack2x16float(first.y)),
        vec3<f32>(unpack2x16float(first.z), unpack2x16float(first.w).x),
        vec4<f32>(unpack2x16float(second.z), unpack2x16float(second.w)),
    );
}

/// Returns the surface band of a shaded-rectangle index: color added within the band, then its
/// height in cells, zero where the cell has no band
fn fluid_band(index: i32) -> vec4<f32> {
    let second: vec4<u32> = shaded_cells[index * 2 + 1];
    return vec4<f32>(unpack2x16float(second.x), unpack2x16float(second.y));
}

/// Returns a hash in [0, 1) of an integer lattice point
fn lattice_hash(point: vec2<i32>) -> f32 {
    var value: u32 = bitcast<u32>(point.x) * 0x8da6b343u ^ bitcast<u32>(point.y) * 0xd8163841u;
    value = (value ^ (value >> 15u)) * 0x2c1b3c6du;
    value = (value ^ (value >> 12u)) * 0x297a2d39u;
    value = value ^ (value >> 15u);
    return f32(value >> 8u) / 16777216.0;
}

/// Returns smooth value noise from -1 to 1, averaging zero
fn value_noise(position: vec2<f32>) -> f32 {
    let lower: vec2<f32> = floor(position);
    let fraction: vec2<f32> = position - lower;
    let smooth_fraction: vec2<f32> = fraction * fraction * (3.0 - 2.0 * fraction);
    let corner: vec2<i32> = vec2<i32>(lower);
    let bottom: f32 = mix(
        lattice_hash(corner), lattice_hash(corner + vec2<i32>(1, 0)), smooth_fraction.x);
    let top: f32 = mix(
        lattice_hash(corner + vec2<i32>(0, 1)), lattice_hash(corner + vec2<i32>(1, 1)),
        smooth_fraction.x);
    return mix(bottom, top, smooth_fraction.y) * 2.0 - 1.0;
}

/// Returns two octaves of value noise from about -1 to 1, averaging zero, that slowly churns in
/// place, each octave drifting its own way
fn fluid_noise(position: vec2<f32>, time: f32) -> f32 {
    var noise: f32 = value_noise(position + vec2<f32>(0.31, 0.17) * time) * 0.65;
    noise += value_noise(position * 2.07 + vec2<f32>(-0.23, 0.29) * time + vec2<f32>(11.3, 7.7))
        * 0.35;
    return noise;
}

/// Screen pixels per texture feature below which the texture fades out instead of sparkling
const TEXTURE_PIXELS_MINIMUM: f32 = 2.0;

/// Screen pixels per texture feature above which the texture shows at full strength
const TEXTURE_PIXELS_FULL: f32 = 6.0;

/// Average of `crust_seams` over noise, which it subtracts so the crust keeps the mean glow
const CRUST_SEAM_MEAN: f32 = 0.27;

/// Returns the glow multiplier of a crust over a noise value: thin bright seams where the noise
/// crosses zero and darker crust elsewhere, averaging one
fn crust_glow(noise: f32, crust: f32) -> f32 {
    let seam: f32 = pow(1.0 - min(abs(noise) * 2.0, 1.0), 3.0);
    return max(1.0 + crust * (seam - CRUST_SEAM_MEAN) / CRUST_SEAM_MEAN * 0.5, 0.0);
}

/// Draws one fluid framebuffer pixel: only inside shaded fluid cells, so edges follow the cell
/// grid, with the fluid interpolated between the shaded cells around the pixel inside, normalized
/// over those cells so edge cells keep their own look up to the edge, then textured by noise whose
/// average leaves the cells' look unchanged
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
    let own_index: i32 = shaded_index(vec2<i32>(floor(cell_position)));
    if !is_shaded(own_index) { discard; }
    let continuous_cell: vec2<f32> = cell_position - vec2<f32>(0.5);
    let lower_cell: vec2<i32> = vec2<i32>(floor(continuous_cell));
    let fraction: vec2<f32> = fract(continuous_cell);
    var fluid: FluidCell = FluidCell(vec4<f32>(0.0), vec3<f32>(0.0), vec4<f32>(0.0));
    var weight_sum: f32 = 0.0;
    for (var corner: u32 = 0u; corner < 4u; corner++) {
        let offset: vec2<u32> = vec2<u32>(corner % 2u, corner / 2u);
        let index: i32 = shaded_index(lower_cell + vec2<i32>(offset));
        if !is_shaded(index) { continue; }
        let weights: vec2<f32> = select(1.0 - fraction, fraction, offset == vec2<u32>(1u));
        let weight: f32 = weights.x * weights.y;
        let corner_fluid: FluidCell = fluid_cell(index);
        fluid.body += corner_fluid.body * weight;
        fluid.emission += corner_fluid.emission * weight;
        fluid.texture += corner_fluid.texture * weight;
        weight_sum += weight;
    }
    let normalizer: f32 = 1.0 / max(weight_sum, 1e-6);
    var body: vec4<f32> = fluid.body * normalizer;
    var emission: vec3<f32> = fluid.emission * normalizer;
    let texture: vec4<f32> = fluid.texture * normalizer;
    let feature_cells: f32 = max(texture.y, 0.25);
    // texture finer than a few pixels fades out instead of sparkling when zoomed out
    let feature_pixels: f32 = feature_cells * uniforms.zoom / f32(uniforms.cellular_tile_size);
    let visibility: f32 = smoothstep(TEXTURE_PIXELS_MINIMUM, TEXTURE_PIXELS_FULL, feature_pixels);
    let shimmer: f32 = texture.x * visibility;
    let crust: f32 = texture.w * visibility;
    if shimmer > 0.0 || crust > 0.0 {
        let noise: f32 = fluid_noise(cell_position / feature_cells, uniforms.time * texture.z);
        body = vec4<f32>(body.rgb * max(1.0 + shimmer * noise, 0.0), body.a);
        emission *= crust_glow(noise, crust);
    }
    var color: vec4<f32> = vec4<f32>(body.rgb + emission, body.a);
    // a surface cell adds its band along its top, antialiased over one pixel at its lower edge
    let band: vec4<f32> = fluid_band(own_index);
    if band.a > 0.0 {
        let pixel_cells: f32 = f32(uniforms.cellular_tile_size) / uniforms.zoom;
        let height: f32 = fract(cell_position.y);
        let coverage: f32 = clamp((height - (1.0 - band.a)) / pixel_cells + 0.5, 0.0, 1.0);
        color += vec4<f32>(band.rgb * coverage, 0.0);
    }
    return color;
}
