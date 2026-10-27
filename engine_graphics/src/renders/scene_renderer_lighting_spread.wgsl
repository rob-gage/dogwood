// Copyright Rob Gage 2026

/// One spread pass along every row or every column of the lighting domain
struct Spread {
    /// Lighting domain size in cells
    domain_size: vec2<u32>,
    /// Zero spreads along rows, one along columns
    axis: u32,
    /// Divisor of a cell's extinction while light spreads into it, at least one
    penetration: f32,
    /// Number of cells along each side of a tile
    cellular_tile_size: f32,
    /// Whether this pass adds the backlight to its input
    adds_backlight: u32,
    /// Whether this pass also writes the light texture
    writes_light: u32,
    /// Alignment padding
    padding: u32,
};

@group(0) @binding(0)
var<uniform> spread: Spread;

/// Linear emission per cell of path in `rgb` and extinction per cell in `a`
@group(0) @binding(1)
var optical: texture_2d<f32>;

/// Light of every domain cell before this pass, as `pack2x16float` red and green, then blue
@group(0) @binding(2)
var<storage, read> input: array<vec2<u32>>;

/// Light of every domain cell after this pass, packed like `input`
@group(0) @binding(3)
var<storage, read_write> output: array<vec2<u32>>;

/// Light sampled by the appearance passes
@group(0) @binding(4)
var light_texture: texture_storage_2d<rgba16float, write>;

/// Light arriving from parallax backgrounds behind the domain
@group(0) @binding(5)
var backlight: texture_2d<f32>;

@group(0) @binding(6)
var backlight_sampler: sampler;

override WORKGROUP_SIZE: u32 = 256u;

/// Cells each invocation holds per segment of a line
const SEGMENT_CELLS: u32 = 8u;

var<workgroup> scan_attenuations: array<f32, WORKGROUP_SIZE>;
var<workgroup> scan_lights: array<vec3<f32>, WORKGROUP_SIZE>;
var<workgroup> segment_carry: vec3<f32>;

/// Unpacks one packed light
fn unpack_light(packed: vec2<u32>) -> vec3<f32> {
    return vec3<f32>(unpack2x16float(packed.x), unpack2x16float(packed.y).x);
}

/// Packs one light
fn pack_light(light: vec3<f32>) -> vec2<u32> {
    return vec2<u32>(pack2x16float(light.rg), pack2x16float(vec2<f32>(light.b, 0.0)));
}

/// Returns the domain cell at a position along a line
fn line_cell(line: u32, position: u32) -> vec2<u32> {
    if spread.axis == 0u {
        return vec2<u32>(position, line);
    }
    return vec2<u32>(line, position);
}

/// Returns the fraction of light spreading into a cell from its neighbor; light spreads only
/// into cells that occlude
fn attenuation(extinction: f32) -> f32 {
    if extinction <= 0.0 {
        return 0.0;
    }
    return exp(-extinction / spread.penetration);
}

/// Spreads light along one line in one direction as a scan of `max(a * x, b)` maps, one segment
/// of the line at a time with each invocation holding its cells, writing the result or raising
/// the written result to it
fn spread_line(line: u32, thread: u32, length: u32, reverse: bool) {
    if thread == 0u {
        segment_carry = vec3<f32>(0.0);
    }
    let segment_length: u32 = WORKGROUP_SIZE * SEGMENT_CELLS;
    for (var segment_start: u32 = 0u; segment_start < length; segment_start += segment_length) {
        var attenuations: array<f32, 8>;
        var seeds: array<vec3<f32>, 8>;
        var indices: array<u32, 8>;
        var attenuation_product: f32 = 1.0;
        var light: vec3<f32> = vec3<f32>(0.0);
        let first: u32 = segment_start + thread * SEGMENT_CELLS;
        for (var cell_index: u32 = 0u; cell_index < SEGMENT_CELLS; cell_index++) {
            let step: u32 = first + cell_index;
            if step >= length {
                attenuations[cell_index] = 1.0;
                seeds[cell_index] = vec3<f32>(0.0);
                continue;
            }
            let cell: vec2<u32> = line_cell(line, select(step, length - 1u - step, reverse));
            let index: u32 = cell.y * spread.domain_size.x + cell.x;
            let extinction: f32 = textureLoad(optical, cell, 0).a;
            var seed: vec3<f32> = unpack_light(input[index]);
            if spread.adds_backlight != 0u {
                // backlight reaches the cells a tile of matter does not hide from behind
                let coordinates: vec2<f32> = (vec2<f32>(cell) + 0.5) / vec2<f32>(spread.domain_size);
                seed += textureSampleLevel(backlight, backlight_sampler, coordinates, 0.0).rgb
                    * exp(-extinction * spread.cellular_tile_size);
            }
            indices[cell_index] = index;
            attenuations[cell_index] = attenuation(extinction);
            seeds[cell_index] = seed;
            attenuation_product *= attenuations[cell_index];
            light = max(attenuations[cell_index] * light, seed);
        }
        scan_attenuations[thread] = attenuation_product;
        scan_lights[thread] = light;
        workgroupBarrier();
        for (var offset: u32 = 1u; offset < WORKGROUP_SIZE; offset *= 2u) {
            var earlier_attenuation: f32 = 1.0;
            var earlier_light: vec3<f32> = vec3<f32>(0.0);
            if thread >= offset {
                earlier_attenuation = scan_attenuations[thread - offset];
                earlier_light = scan_lights[thread - offset];
            }
            workgroupBarrier();
            if thread >= offset {
                light = max(attenuation_product * earlier_light, light);
                attenuation_product *= earlier_attenuation;
                scan_attenuations[thread] = attenuation_product;
                scan_lights[thread] = light;
            }
            workgroupBarrier();
        }
        // the light entering this invocation's cells, from earlier invocations and segments
        var carried: vec3<f32> = segment_carry;
        if thread > 0u {
            carried = max(scan_attenuations[thread - 1u] * segment_carry, scan_lights[thread - 1u]);
        }
        let segment_end: vec3<f32> = max(
            scan_attenuations[WORKGROUP_SIZE - 1u] * segment_carry,
            scan_lights[WORKGROUP_SIZE - 1u],
        );
        workgroupBarrier();
        for (var cell_index: u32 = 0u; cell_index < SEGMENT_CELLS; cell_index++) {
            if first + cell_index >= length {
                break;
            }
            carried = max(attenuations[cell_index] * carried, seeds[cell_index]);
            let index: u32 = indices[cell_index];
            if reverse {
                let spread_light: vec3<f32> = max(unpack_light(output[index]), carried);
                output[index] = pack_light(spread_light);
                if spread.writes_light != 0u {
                    textureStore(light_texture, vec2<u32>(index % spread.domain_size.x,
                        index / spread.domain_size.x), vec4<f32>(spread_light, 1.0));
                }
            } else {
                output[index] = pack_light(carried);
            }
        }
        if thread == 0u {
            segment_carry = segment_end;
        }
        workgroupBarrier();
    }
}

/// Spreads light along a line that fits one segment and divides into whole invocations in both
/// directions, each invocation keeping its cells in registers. Reversed, the invocation holding
/// cells `8j..8j+8` is exactly the invocation `n - 1 - j` of a reversed scan, so the arithmetic
/// and grouping match two separate `spread_line` calls bit for bit.
fn spread_line_fused(line: u32, thread: u32, length: u32) {
    let count: u32 = length / SEGMENT_CELLS;
    let is_active: bool = thread < count;
    var attenuations: array<f32, 8>;
    var seeds: array<vec3<f32>, 8>;
    var indices: array<u32, 8>;
    var forward: array<vec2<u32>, 8>;
    let zero_carry: vec3<f32> = vec3<f32>(0.0);
    var attenuation_product: f32 = 1.0;
    var light: vec3<f32> = vec3<f32>(0.0);
    let first: u32 = thread * SEGMENT_CELLS;
    for (var cell_index: u32 = 0u; cell_index < SEGMENT_CELLS; cell_index++) {
        if !is_active {
            attenuations[cell_index] = 1.0;
            seeds[cell_index] = vec3<f32>(0.0);
            continue;
        }
        let cell: vec2<u32> = line_cell(line, first + cell_index);
        let index: u32 = cell.y * spread.domain_size.x + cell.x;
        let extinction: f32 = textureLoad(optical, cell, 0).a;
        var seed: vec3<f32> = unpack_light(input[index]);
        if spread.adds_backlight != 0u {
            let coordinates: vec2<f32> = (vec2<f32>(cell) + 0.5) / vec2<f32>(spread.domain_size);
            seed += textureSampleLevel(backlight, backlight_sampler, coordinates, 0.0).rgb
                * exp(-extinction * spread.cellular_tile_size);
        }
        indices[cell_index] = index;
        attenuations[cell_index] = attenuation(extinction);
        seeds[cell_index] = seed;
        attenuation_product *= attenuations[cell_index];
        light = max(attenuations[cell_index] * light, seed);
    }
    // forward scan, invocation order is thread order
    scan_attenuations[thread] = attenuation_product;
    scan_lights[thread] = light;
    workgroupBarrier();
    for (var offset: u32 = 1u; offset < WORKGROUP_SIZE; offset *= 2u) {
        var earlier_attenuation: f32 = 1.0;
        var earlier_light: vec3<f32> = vec3<f32>(0.0);
        if thread >= offset {
            earlier_attenuation = scan_attenuations[thread - offset];
            earlier_light = scan_lights[thread - offset];
        }
        workgroupBarrier();
        if thread >= offset {
            light = max(attenuation_product * earlier_light, light);
            attenuation_product *= earlier_attenuation;
            scan_attenuations[thread] = attenuation_product;
            scan_lights[thread] = light;
        }
        workgroupBarrier();
    }
    var carried: vec3<f32> = zero_carry;
    if thread > 0u {
        carried = max(scan_attenuations[thread - 1u] * zero_carry, scan_lights[thread - 1u]);
    }
    workgroupBarrier();
    for (var cell_index: u32 = 0u; cell_index < SEGMENT_CELLS; cell_index++) {
        if !is_active {
            break;
        }
        carried = max(attenuations[cell_index] * carried, seeds[cell_index]);
        forward[cell_index] = pack_light(carried);
    }
    // reverse scan, this invocation acts as reversed invocation `count - 1 - thread`
    let virtual_thread: u32 = select(thread, count - 1u - thread, is_active);
    attenuation_product = 1.0;
    light = vec3<f32>(0.0);
    for (var step: u32 = 0u; step < SEGMENT_CELLS; step++) {
        let cell_index: u32 = SEGMENT_CELLS - 1u - step;
        attenuation_product *= attenuations[cell_index];
        light = max(attenuations[cell_index] * light, seeds[cell_index]);
    }
    scan_attenuations[virtual_thread] = attenuation_product;
    scan_lights[virtual_thread] = light;
    workgroupBarrier();
    for (var offset: u32 = 1u; offset < WORKGROUP_SIZE; offset *= 2u) {
        var earlier_attenuation: f32 = 1.0;
        var earlier_light: vec3<f32> = vec3<f32>(0.0);
        if virtual_thread >= offset {
            earlier_attenuation = scan_attenuations[virtual_thread - offset];
            earlier_light = scan_lights[virtual_thread - offset];
        }
        workgroupBarrier();
        if virtual_thread >= offset {
            light = max(attenuation_product * earlier_light, light);
            attenuation_product *= earlier_attenuation;
            scan_attenuations[virtual_thread] = attenuation_product;
            scan_lights[virtual_thread] = light;
        }
        workgroupBarrier();
    }
    carried = zero_carry;
    if virtual_thread > 0u {
        carried = max(scan_attenuations[virtual_thread - 1u] * zero_carry,
            scan_lights[virtual_thread - 1u]);
    }
    if !is_active {
        return;
    }
    for (var step: u32 = 0u; step < SEGMENT_CELLS; step++) {
        let cell_index: u32 = SEGMENT_CELLS - 1u - step;
        carried = max(attenuations[cell_index] * carried, seeds[cell_index]);
        let index: u32 = indices[cell_index];
        let spread_light: vec3<f32> = max(unpack_light(forward[cell_index]), carried);
        // the last pass's buffer is never read again, only its light texture
        if spread.writes_light != 0u {
            textureStore(light_texture, vec2<u32>(index % spread.domain_size.x,
                index / spread.domain_size.x), vec4<f32>(spread_light, 1.0));
        } else {
            output[index] = pack_light(spread_light);
        }
    }
}

/// Spreads light along one row or column in both directions
@compute @workgroup_size(WORKGROUP_SIZE, 1, 1)
fn spread_lines(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) thread: u32,
) {
    let line: u32 = workgroup.x;
    let length: u32 = select(spread.domain_size.y, spread.domain_size.x, spread.axis == 0u);
    if length % SEGMENT_CELLS == 0u && length <= WORKGROUP_SIZE * SEGMENT_CELLS {
        spread_line_fused(line, thread, length);
        return;
    }
    spread_line(line, thread, length, false);
    storageBarrier();
    workgroupBarrier();
    spread_line(line, thread, length, true);
}
