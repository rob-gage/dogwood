// Copyright Rob Gage 2026

/// Region layout, material layout, and scales of one sampling step
struct SamplingUniforms {
    /// Active region width in tiles
    region_width: u32,
    /// Active region height in tiles
    region_height: u32,
    /// Horizontal ring offset of the active region in tiles
    ring_offset_x: u32,
    /// Vertical ring offset of the active region in tiles
    ring_offset_y: u32,
    /// Number of samples
    sample_count: u32,
    /// Cellular static material indices in the tag masks
    static_count: u32,
    /// Cellular dynamic material indices in the tag masks
    dynamic_count: u32,
    /// Fluid material indices in the tag masks
    fluid_count: u32,
    /// Gas material indices in the tag masks
    gas_count: u32,
    /// Fixed-point units per mass unit in the fluid raster
    raster_scale: f32,
    /// Tiles per second of one cell per pressure substep
    gas_speed_scale: f32,
    /// Tiles per second of one cell per tick
    fluid_speed_scale: f32,
};

@group(0) @binding(0)
var<uniform> uniforms: SamplingUniforms;

/// Logical cell of each sample inside the active region
@group(0) @binding(1)
var<storage, read> samples: array<vec2<u32>>;

@group(0) @binding(2)
var<storage, read> material_identifiers: array<u32>;

/// Fluid raster: fixed-point mass, momentum x and y, and selected material per cell
@group(0) @binding(3)
var<storage, read> fluid_raster: array<i32>;

/// Fluid material properties, two records per fluid index with rest density in the first x
@group(0) @binding(4)
var<storage, read> fluid_materials: array<vec4<f32>>;

/// Gas species slots and the phantom air, flows, and stored pressure, two records per cell
@group(0) @binding(5)
var<storage, read> gas: array<vec4<u32>>;

/// Tag masks of cellular static, cellular dynamic, fluid, then gas material indices
@group(0) @binding(6)
var<storage, read> tag_masks: array<u32>;

/// Temperature bits then amount of every cell
@group(0) @binding(7)
var<storage, read> matter: array<vec2<u32>>;

@group(0) @binding(8)
var<storage, read> pressures: array<f32>;

/// Ten words per sample: temperature, pressure, pressure gradient, gas velocity, fluid coverage,
/// fluid velocity, and tag mask
@group(0) @binding(9)
var<storage, read_write> results: array<u32>;

/// Returns the physical cell-buffer index of a logical cell, wrapped into the region
fn cell_index(cell: vec2<i32>) -> u32 {
    let size: vec2<i32> = vec2<i32>(i32(uniforms.region_width), i32(uniforms.region_height)) * 8;
    let position: vec2<u32> = vec2<u32>((cell % size + size) % size);
    let tile: vec2<u32> = position / 8u;
    let physical_x: u32 = (tile.x + uniforms.ring_offset_x) % uniforms.region_width;
    let physical_y: u32 = (tile.y + uniforms.ring_offset_y) % uniforms.region_height;
    return (physical_y * uniforms.region_width + physical_x) * 64u
        + (position.y % 8u) * 8u + position.x % 8u;
}

/// Returns the configured mask of a cellular static, dynamic, fluid, or gas material
fn material_tag_mask(identifier: u32) -> u32 {
    let form: u32 = identifier >> 30u;
    let index: u32 = identifier & 0x3fffffffu;
    var offset: u32 = 0u;
    var count: u32 = uniforms.static_count;
    if form == 1u {
        offset = uniforms.static_count;
        count = uniforms.dynamic_count;
    } else if form == 2u {
        offset = uniforms.static_count + uniforms.dynamic_count;
        count = uniforms.fluid_count;
    } else if form == 3u {
        offset = uniforms.static_count + uniforms.dynamic_count + uniforms.fluid_count;
        count = uniforms.gas_count;
    }
    if index >= count { return 0u; }
    return tag_masks[offset + index];
}

/// Returns the tags of a cell's matter and gases
fn cell_tags(index: u32) -> u32 {
    var mask: u32 = 0u;
    let material: u32 = material_identifiers[index];
    if material != 0u { mask = material_tag_mask(material); }
    let slots: vec4<u32> = gas[index * 2u];
    for (var slot: u32 = 0u; slot < 4u; slot++) {
        if (slots[slot] & 0xffffffu) > 0u {
            mask |= material_tag_mask((3u << 30u) | (slots[slot] >> 24u));
        }
    }
    return mask;
}

/// Returns whether a cell holds static matter, which holds no gas of its own to read
fn is_solid(cell: vec2<i32>) -> bool {
    let material: u32 = material_identifiers[cell_index(cell)];
    return material != 0u && (material >> 30u) == 0u;
}

/// Returns a cell's published pressure, or a fallback where a solid cell holds the gas back
fn open_pressure(cell: vec2<i32>, fallback: f32) -> f32 {
    if is_solid(cell) { return fallback; }
    return pressures[cell_index(cell)];
}

/// Returns the cell whose air a sample reads: its own, or the nearest open one around it, since a
/// body's own cells are static and hold neither gas nor flow
fn air_cell(cell: vec2<i32>) -> vec2<i32> {
    if !is_solid(cell) { return cell; }
    for (var distance: i32 = 1; distance <= 3; distance++) {
        for (var side: i32 = -1; side <= 1; side += 2) {
            let across: vec2<i32> = cell + vec2<i32>(distance * side, 0);
            if !is_solid(across) { return across; }
            let along: vec2<i32> = cell + vec2<i32>(0, distance * side);
            if !is_solid(along) { return along; }
        }
    }
    return cell;
}

@compute @workgroup_size(64)
fn sample(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if invocation.x >= uniforms.sample_count { return; }
    let cell: vec2<i32> = vec2<i32>(samples[invocation.x]);
    let index: u32 = cell_index(cell);
    let offsets: array<vec2<i32>, 4> = array<vec2<i32>, 4>(
        vec2<i32>(1, 0), vec2<i32>(-1, 0), vec2<i32>(0, 1), vec2<i32>(0, -1),
    );
    var temperature: f32 = bitcast<f32>(matter[index].x);
    var tags: u32 = cell_tags(index);
    for (var side: u32 = 0u; side < 4u; side++) {
        let neighbor: u32 = cell_index(cell + offsets[side]);
        temperature = max(temperature, bitcast<f32>(matter[neighbor].x));
        tags |= cell_tags(neighbor);
    }
    // the air around the sample, so that a body never reads a false gradient off its own cells
    let air: vec2<i32> = air_cell(cell);
    let air_index: u32 = cell_index(air);
    let pressure: f32 = pressures[air_index];
    let gradient: vec2<f32> = vec2<f32>(
        open_pressure(air + vec2<i32>(2, 0), pressure)
            - open_pressure(air - vec2<i32>(2, 0), pressure),
        open_pressure(air + vec2<i32>(0, 2), pressure)
            - open_pressure(air - vec2<i32>(0, 2), pressure),
    ) * 0.25;
    let slots: vec4<u32> = gas[air_index * 2u];
    let words: vec4<u32> = gas[air_index * 2u + 1u];
    let amount: f32 = f32(slots.x & 0xffffffu) + f32(slots.y & 0xffffffu)
        + f32(slots.z & 0xffffffu) + f32(slots.w & 0xffffffu) + f32(words.x);
    let flow: vec2<f32> = vec2<f32>(bitcast<f32>(words.y), bitcast<f32>(words.z));
    let gas_velocity: vec2<f32> = flow / max(amount, 1.0) * uniforms.gas_speed_scale;
    let raster: u32 = air_index * 4u;
    let mass: f32 = max(f32(fluid_raster[raster]) / uniforms.raster_scale, 0.0);
    var coverage: f32 = 0.0;
    var fluid_velocity: vec2<f32> = vec2<f32>(0.0);
    let fluid_index: u32 = u32(fluid_raster[raster + 3]) & 0x3fffffffu;
    if mass > 0.0 && fluid_index < uniforms.fluid_count {
        coverage = mass / max(fluid_materials[fluid_index * 2u].x, 0.000001);
        fluid_velocity = vec2<f32>(
            f32(fluid_raster[raster + 1]),
            f32(fluid_raster[raster + 2]),
        ) / uniforms.raster_scale / mass * uniforms.fluid_speed_scale;
    }
    let base: u32 = invocation.x * 10u;
    results[base] = bitcast<u32>(temperature);
    results[base + 1u] = bitcast<u32>(pressure);
    results[base + 2u] = bitcast<u32>(gradient.x);
    results[base + 3u] = bitcast<u32>(gradient.y);
    results[base + 4u] = bitcast<u32>(gas_velocity.x);
    results[base + 5u] = bitcast<u32>(gas_velocity.y);
    results[base + 6u] = bitcast<u32>(coverage);
    results[base + 7u] = bitcast<u32>(fluid_velocity.x);
    results[base + 8u] = bitcast<u32>(fluid_velocity.y);
    results[base + 9u] = tags;
}
