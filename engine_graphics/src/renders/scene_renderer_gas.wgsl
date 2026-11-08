// Copyright Rob Gage 2026

/// Uniform values shared by gas shading and fragments in one viewport
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
    /// Logical cell at the lower left of the shaded cell rectangle
    shaded_origin: vec2<i32>,
    /// Width and height of the shaded cell rectangle
    shaded_size: vec2<u32>,
    /// Simulated scene time in seconds, which moves the gas texture
    time: f32,
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

/// Temperature bits then amount of every cell
@group(0) @binding(4)
var<storage, read> matter: array<vec2<u32>>;

/// Two packed records per cell of the shaded rectangle, as `pack_cell` writes them, zero where no
/// gas is visible
@group(0) @binding(5)
var<storage, read_write> shaded_cells: array<vec4<u32>>;

/// The shaded records with the pressure checker cancelled, zero where no gas is visible
@group(0) @binding(6)
var<storage, read_write> filtered_cells: array<vec4<u32>>;

/// One record per gas material: texture strength, texture feature size in cells, upward drift in
/// cells per second, and emission flicker strength
@group(0) @binding(7)
var<storage, read> gas_graphics: array<vec4<f32>>;

/// Opacity below which a cell's gas is not drawn and ends the cloud
const VISIBLE_ALPHA_MINIMUM: f32 = 0.02;

/// Screen pixels per texture feature below which the texture fades out instead of sparkling
const TEXTURE_PIXELS_MINIMUM: f32 = 2.0;

/// Screen pixels per texture feature above which the texture shows at full strength
const TEXTURE_PIXELS_FULL: f32 = 6.0;

/// Transmission below which one gas's opacity is clamped, keeping its optical depth finite
const TRANSMISSION_MINIMUM: f32 = 1e-4;

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

/// What one gas cell shows: its lit premultiplied body, its emission, and its texture
struct GasCell {
    /// Premultiplied linear color of the light the gas scatters, and its alpha
    body: vec4<f32>,
    /// Linear emission of the gas
    emission: vec3<f32>,
    /// Texture strength, feature size in cells, drift in cells per second, and flicker strength,
    /// each weighted over the cell's gases by their share of its look
    texture: vec4<f32>,
};

/// Returns what one logical gas cell shows: every gas in the cell adds its optical depth, and the
/// cell takes the colors of its gases weighted by their optical depths, as the lighting optics
/// count them
fn cell_gas(cell: vec2<i32>) -> GasCell {
    var result: GasCell = GasCell(vec4<f32>(0.0), vec3<f32>(0.0), vec4<f32>(0.0));
    let region_cell_width: i32 = i32(uniforms.region_width * uniforms.cellular_tile_size);
    let region_cell_height: i32 = i32(uniforms.region_height * uniforms.cellular_tile_size);
    if cell.x < 0 || cell.y < 0 || cell.x >= region_cell_width || cell.y >= region_cell_height {
        return result;
    }
    let cell_index: u32 = physical_cell_index(cell);
    if material_identifiers[cell_index] != 0u {
        return result;
    }
    let slots: vec4<u32> = gas_cells[cell_index * 2u];
    let temperature: f32 = bitcast<f32>(matter[cell_index].x);
    var optical_depth: f32 = 0.0;
    var weighted_color: vec3<f32> = vec3<f32>(0.0);
    var radiance: vec3<f32> = vec3<f32>(0.0);
    var weighted_texture: vec4<f32> = vec4<f32>(0.0);
    var texture_weight: f32 = 0.0;
    for (var slot_index: u32 = 0u; slot_index < 4u; slot_index = slot_index + 1u) {
        let amount: u32 = slots[slot_index] & 0x00ffffffu;
        if amount == 0u { continue; }
        let gas_index: u32 = slots[slot_index] >> 24u;
        let properties: vec4<f32> = gas_materials[gas_index * 2u];
        let opacity: f32 = gas_materials[gas_index * 2u + 1u].x;
        let srgb_color: vec4<f32> = unpack4x8unorm(bitcast<u32>(properties.w));
        let thermal: u32 = lighting_view.thermal_offsets.w + gas_index;
        let color: vec4<f32> = temperature_color(thermal, temperature, vec4<f32>(
            srgb_to_linear(srgb_color.r),
            srgb_to_linear(srgb_color.g),
            srgb_to_linear(srgb_color.b),
            srgb_color.a,
        ), false);
        let gas_radiance: vec4<f32> = temperature_color(
            thermal,
            temperature,
            unpack_radiance(bitcast<u32>(gas_materials[gas_index * 2u + 1u].y)),
            true,
        );
        let atmospheres: f32 = f32(amount) / uniforms.amount_per_atmosphere;
        // one atmosphere of the gas passes `1 - alpha` of the light behind it
        let depth: f32 = -log(max(1.0 - clamp(color.a, 0.0, 1.0), TRANSMISSION_MINIMUM))
            * atmospheres * opacity;
        optical_depth += depth;
        weighted_color += color.rgb * depth;
        // glowing gas shows its emission per atmosphere, however transparent it is
        let emission: vec3<f32> = gas_radiance.rgb * min(atmospheres, 1.0);
        radiance += emission;
        // each gas's texture counts by how much it shows, through opacity or glow
        let share: f32 = depth + dot(emission, vec3<f32>(1.0)) + 1e-6;
        weighted_texture += gas_graphics[gas_index] * share;
        texture_weight += share;
    }
    let alpha: f32 = clamp(1.0 - exp(-optical_depth), 0.0, 1.0);
    var color: vec3<f32> = vec3<f32>(0.0);
    if optical_depth > 0.0 {
        color = weighted_color / optical_depth;
    }
    let light: vec3<f32> = light_at(vec2<f32>(cell) + 0.5);
    result.body = vec4<f32>(color * light * alpha, alpha);
    result.emission = radiance;
    if texture_weight > 0.0 {
        result.texture = weighted_texture / texture_weight;
    }
    return result;
}

/// Returns whether a gas cell is visible
fn is_visible(gas: GasCell) -> bool {
    let shown: vec3<f32> = gas.body.rgb + gas.emission;
    return max(gas.body.a, max(shown.r, max(shown.g, shown.b))) > VISIBLE_ALPHA_MINIMUM;
}

/// Packs a gas cell into its two cell buffer records
fn pack_cell(gas: GasCell) -> array<vec4<u32>, 2> {
    return array<vec4<u32>, 2>(
        vec4<u32>(
            pack2x16float(gas.body.rg),
            pack2x16float(gas.body.ba),
            pack2x16float(gas.emission.rg),
            pack2x16float(vec2<f32>(gas.emission.b, 0.0)),
        ),
        vec4<u32>(pack2x16float(gas.texture.xy), pack2x16float(gas.texture.zw), 0u, 0u),
    );
}

/// Unpacks a gas cell from its two cell buffer records
fn unpack_cell(first: vec4<u32>, second: vec4<u32>) -> GasCell {
    return GasCell(
        vec4<f32>(unpack2x16float(first.x), unpack2x16float(first.y)),
        vec3<f32>(unpack2x16float(first.z), unpack2x16float(first.w).x),
        vec4<f32>(unpack2x16float(second.x), unpack2x16float(second.y)),
    );
}

/// Returns a gas cell scaled by a weight, for weighted sums
fn scale_cell(gas: GasCell, weight: f32) -> GasCell {
    return GasCell(gas.body * weight, gas.emission * weight, gas.texture * weight);
}

/// Returns the sum of two gas cells
fn add_cells(first: GasCell, second: GasCell) -> GasCell {
    return GasCell(
        first.body + second.body,
        first.emission + second.emission,
        first.texture + second.texture,
    );
}

/// Returns the shaded-rectangle index of a logical cell, or none outside the rectangle
fn shaded_index(cell: vec2<i32>) -> i32 {
    let local: vec2<i32> = cell - uniforms.shaded_origin;
    if any(local < vec2<i32>(0)) || any(vec2<u32>(local) >= uniforms.shaded_size) {
        return -1;
    }
    return local.y * i32(uniforms.shaded_size.x) + local.x;
}

/// Shades every cell of the shaded rectangle once, leaving zero where no gas is visible
@compute @workgroup_size(8, 8)
fn shade(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if any(invocation.xy >= uniforms.shaded_size) { return; }
    let gas: GasCell = cell_gas(uniforms.shaded_origin + vec2<i32>(invocation.xy));
    let index: u32 = invocation.y * uniforms.shaded_size.x + invocation.x;
    var packed: array<vec4<u32>, 2> = array<vec4<u32>, 2>(vec4<u32>(0u), vec4<u32>(0u));
    if is_visible(gas) { packed = pack_cell(gas); }
    shaded_cells[index * 2u] = packed[0];
    shaded_cells[index * 2u + 1u] = packed[1];
}

/// Returns whether a shaded record holds visible gas
fn is_shaded(first: vec4<u32>) -> bool {
    return any(first != vec4<u32>(0u));
}

/// The pressure solver can leave its 2x2 pressure cells alternating, which reads as a checkered
/// pattern; averaging each visible cell with the visible cells one pressure cell away on each side
/// cancels it
@compute @workgroup_size(8, 8)
fn cancel_checker(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if any(invocation.xy >= uniforms.shaded_size) { return; }
    let index: u32 = invocation.y * uniforms.shaded_size.x + invocation.x;
    let first: vec4<u32> = shaded_cells[index * 2u];
    if !is_shaded(first) {
        filtered_cells[index * 2u] = vec4<u32>(0u);
        filtered_cells[index * 2u + 1u] = vec4<u32>(0u);
        return;
    }
    let cell: vec2<i32> = uniforms.shaded_origin + vec2<i32>(invocation.xy);
    let gas: GasCell = unpack_cell(first, shaded_cells[index * 2u + 1u]);
    var across: GasCell = GasCell(vec4<f32>(0.0), vec3<f32>(0.0), vec4<f32>(0.0));
    var across_weight: f32 = 0.0;
    for (var side: u32 = 0u; side < 4u; side++) {
        let step: vec2<i32> = vec2<i32>(
            select(0, select(-2, 2, side == 1u), side < 2u),
            select(0, select(-2, 2, side == 3u), side >= 2u),
        );
        let side_index: i32 = shaded_index(cell + step);
        if side_index < 0 { continue; }
        let side_first: vec4<u32> = shaded_cells[side_index * 2];
        if !is_shaded(side_first) { continue; }
        across = add_cells(across, unpack_cell(side_first, shaded_cells[side_index * 2 + 1]));
        across_weight += 1.0;
    }
    var filtered: GasCell = gas;
    if across_weight > 0.0 {
        filtered = add_cells(scale_cell(gas, 0.5), scale_cell(across, 0.5 / across_weight));
    }
    // a filtered visible cell never packs to zero, which marks cells without gas
    filtered.body.a = max(filtered.body.a, 1e-3);
    let packed: array<vec4<u32>, 2> = pack_cell(filtered);
    filtered_cells[index * 2u] = packed[0];
    filtered_cells[index * 2u + 1u] = packed[1];
}

/// Returns the uniform cubic B-spline weights of the four cells around a fraction
fn spline_weights(fraction: f32) -> vec4<f32> {
    let inverse: f32 = 1.0 - fraction;
    let squared: f32 = fraction * fraction;
    let cubed: f32 = squared * fraction;
    return vec4<f32>(
        inverse * inverse * inverse,
        3.0 * cubed - 6.0 * squared + 4.0,
        -3.0 * cubed + 3.0 * squared + 3.0 * fraction + 1.0,
        cubed,
    ) / 6.0;
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

/// Returns three octaves of value noise from about -1 to 1, averaging zero, that drifts upward and
/// churns as it goes
fn gas_noise(position: vec2<f32>, drift: f32, time: f32) -> f32 {
    let rising: vec2<f32> = position - vec2<f32>(0.0, drift * time);
    // a slow sideways churn keeps the texture changing even where it does not drift
    let churn: vec2<f32> = vec2<f32>(0.37, 0.21) * time;
    var noise: f32 = value_noise(rising + churn) * 0.57;
    noise += value_noise(rising * 2.03 - churn * 1.6 + vec2<f32>(17.3, 5.1)) * 0.29;
    noise += value_noise(rising * 4.11 + churn * 2.3 + vec2<f32>(3.7, 41.9)) * 0.14;
    return noise;
}

/// Draws one gas framebuffer pixel: only inside cells holding visible gas, so cloud edges follow
/// the cell grid like fluid, with a continuous cubic B-spline of the visible cells around the
/// pixel inside, normalized over those cells so edge cells keep their density up to the edge, then
/// textured by noise whose average leaves the cells' look unchanged
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
    if own_index < 0 || !is_shaded(filtered_cells[own_index * 2]) {
        discard;
    }
    let continuous_cell: vec2<f32> = cell_position - vec2<f32>(0.5);
    let lower_cell: vec2<i32> = vec2<i32>(floor(continuous_cell));
    let fraction: vec2<f32> = fract(continuous_cell);
    let weights_x: vec4<f32> = spline_weights(fraction.x);
    let weights_y: vec4<f32> = spline_weights(fraction.y);
    var gas: GasCell = GasCell(vec4<f32>(0.0), vec3<f32>(0.0), vec4<f32>(0.0));
    var weight_sum: f32 = 0.0;
    for (var row: i32 = 0; row < 4; row++) {
        for (var column: i32 = 0; column < 4; column++) {
            let index: i32 = shaded_index(lower_cell + vec2<i32>(column - 1, row - 1));
            if index < 0 { continue; }
            let first: vec4<u32> = filtered_cells[index * 2];
            if !is_shaded(first) { continue; }
            let weight: f32 = weights_x[column] * weights_y[row];
            gas = add_cells(gas, scale_cell(unpack_cell(first, filtered_cells[index * 2 + 1]), weight));
            weight_sum += weight;
        }
    }
    gas = scale_cell(gas, 1.0 / max(weight_sum, 1e-6));
    var body: vec4<f32> = gas.body;
    var emission: vec3<f32> = gas.emission;
    let feature_cells: f32 = max(gas.texture.y, 0.25);
    // texture finer than a few pixels fades out instead of sparkling when zoomed out
    let feature_pixels: f32 = feature_cells * uniforms.zoom / f32(uniforms.cellular_tile_size);
    let visibility: f32 = smoothstep(TEXTURE_PIXELS_MINIMUM, TEXTURE_PIXELS_FULL, feature_pixels);
    let strength: f32 = gas.texture.x * visibility;
    let flicker: f32 = gas.texture.w * visibility;
    if strength > 0.0 || flicker > 0.0 {
        let noise: f32 =
            gas_noise(cell_position / feature_cells, gas.texture.z / feature_cells, uniforms.time);
        // the noise scales the gas's optical depth, keeping its color and averaging its density
        let density: f32 = max(1.0 + strength * noise, 0.0);
        let alpha: f32 = 1.0 - pow(1.0 - min(body.a, 0.9999), density);
        body = vec4<f32>(body.rgb * (alpha / max(body.a, 1e-6)), alpha);
        emission *= max(1.0 + flicker * noise, 0.0);
    }
    return vec4<f32>(body.rgb + emission, body.a);
}
