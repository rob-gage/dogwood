// Copyright Rob Gage 2026

/// One spread pass along every row or every column of the lighting texel domain
struct Spread {
    /// Lighting domain size in texels
    domain_size: vec2<u32>,
    /// Zero spreads along rows, one along columns
    axis: u32,
    /// Divisor of a texel's extinction while light spreads into it, at least one
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

/// One cell of a line segment: its buffer index, the fraction of light spreading into it, and
/// the light it starts with
struct SpreadCell {
    index: u32,
    attenuation: f32,
    seed: vec3<f32>,
};

/// The light entering an invocation's cells and leaving the whole segment
struct SpreadScan {
    entering: vec3<f32>,
    leaving: vec3<f32>,
};

/// Reads a cell at a step along a line, from its far end when `reverse` holds; steps past the
/// line pass light through unchanged
fn load_cell(line: u32, step: u32, length: u32, reverse: bool) -> SpreadCell {
    if step >= length {
        return SpreadCell(0u, 1.0, vec3<f32>(0.0));
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
    return SpreadCell(index, attenuation(extinction), seed);
}

/// Scans the `max(a * x, b)` maps of every invocation's cells in the order of their `slot`s,
/// given the light entering the segment. Every invocation of the workgroup must call it.
fn scan_maps(slot: u32, attenuation_product: f32, light: vec3<f32>, carry: vec3<f32>) -> SpreadScan {
    var product: f32 = attenuation_product;
    var scanned: vec3<f32> = light;
    scan_attenuations[slot] = product;
    scan_lights[slot] = scanned;
    workgroupBarrier();
    for (var offset: u32 = 1u; offset < WORKGROUP_SIZE; offset *= 2u) {
        var earlier_attenuation: f32 = 1.0;
        var earlier_light: vec3<f32> = vec3<f32>(0.0);
        if slot >= offset {
            earlier_attenuation = scan_attenuations[slot - offset];
            earlier_light = scan_lights[slot - offset];
        }
        workgroupBarrier();
        if slot >= offset {
            scanned = max(product * earlier_light, scanned);
            product *= earlier_attenuation;
            scan_attenuations[slot] = product;
            scan_lights[slot] = scanned;
        }
        workgroupBarrier();
    }
    var entering: vec3<f32> = carry;
    if slot > 0u {
        entering = max(scan_attenuations[slot - 1u] * carry, scan_lights[slot - 1u]);
    }
    let leaving: vec3<f32> =
        max(scan_attenuations[WORKGROUP_SIZE - 1u] * carry, scan_lights[WORKGROUP_SIZE - 1u]);
    workgroupBarrier();
    return SpreadScan(entering, leaving);
}

/// Writes a cell's spread light, to the light texture instead when this pass writes it, since the
/// last pass's buffer is never read again
fn store_light(index: u32, light: vec3<f32>) {
    if spread.writes_light != 0u {
        textureStore(light_texture, vec2<u32>(index % spread.domain_size.x,
            index / spread.domain_size.x), vec4<f32>(light, 1.0));
    } else {
        output[index] = pack_light(light);
    }
}

/// Spreads light along a line of one segment in both directions at once, each invocation keeping
/// its cells between the two scans
fn spread_segment(line: u32, thread: u32, length: u32) {
    var cells: array<SpreadCell, SEGMENT_CELLS>;
    var product: f32 = 1.0;
    var light: vec3<f32> = vec3<f32>(0.0);
    for (var cell_index: u32 = 0u; cell_index < SEGMENT_CELLS; cell_index++) {
        cells[cell_index] = load_cell(line, thread * SEGMENT_CELLS + cell_index, length, false);
        product *= cells[cell_index].attenuation;
        light = max(cells[cell_index].attenuation * light, cells[cell_index].seed);
    }
    var forward: array<vec3<f32>, SEGMENT_CELLS>;
    var carried: vec3<f32> = scan_maps(thread, product, light, vec3<f32>(0.0)).entering;
    for (var cell_index: u32 = 0u; cell_index < SEGMENT_CELLS; cell_index++) {
        carried = max(cells[cell_index].attenuation * carried, cells[cell_index].seed);
        forward[cell_index] = carried;
    }
    // the reverse scan runs the invocations from the far end of the line
    light = vec3<f32>(0.0);
    for (var cell_index: i32 = i32(SEGMENT_CELLS) - 1; cell_index >= 0; cell_index--) {
        light = max(cells[cell_index].attenuation * light, cells[cell_index].seed);
    }
    carried = scan_maps(WORKGROUP_SIZE - 1u - thread, product, light, vec3<f32>(0.0)).entering;
    for (var cell_index: i32 = i32(SEGMENT_CELLS) - 1; cell_index >= 0; cell_index--) {
        carried = max(cells[cell_index].attenuation * carried, cells[cell_index].seed);
        if thread * SEGMENT_CELLS + u32(cell_index) < length {
            store_light(cells[cell_index].index, max(forward[cell_index], carried));
        }
    }
}

/// Spreads light along one line in one direction as a scan of `max(a * x, b)` maps, one segment
/// of the line at a time with each invocation holding its cells, writing the result or raising
/// the written result to it
fn spread_line(line: u32, thread: u32, length: u32, reverse: bool) {
    var carry: vec3<f32> = vec3<f32>(0.0);
    let segment_length: u32 = WORKGROUP_SIZE * SEGMENT_CELLS;
    for (var segment_start: u32 = 0u; segment_start < length; segment_start += segment_length) {
        var cells: array<SpreadCell, SEGMENT_CELLS>;
        var product: f32 = 1.0;
        var light: vec3<f32> = vec3<f32>(0.0);
        let first: u32 = segment_start + thread * SEGMENT_CELLS;
        for (var cell_index: u32 = 0u; cell_index < SEGMENT_CELLS; cell_index++) {
            cells[cell_index] = load_cell(line, first + cell_index, length, reverse);
            product *= cells[cell_index].attenuation;
            light = max(cells[cell_index].attenuation * light, cells[cell_index].seed);
        }
        let scan: SpreadScan = scan_maps(thread, product, light, carry);
        var carried: vec3<f32> = scan.entering;
        for (var cell_index: u32 = 0u; cell_index < SEGMENT_CELLS; cell_index++) {
            if first + cell_index >= length {
                break;
            }
            carried = max(cells[cell_index].attenuation * carried, cells[cell_index].seed);
            let index: u32 = cells[cell_index].index;
            if reverse {
                store_light(index, max(unpack_light(output[index]), carried));
            } else {
                output[index] = pack_light(carried);
            }
        }
        carry = scan.leaving;
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
    if length <= WORKGROUP_SIZE * SEGMENT_CELLS {
        spread_segment(line, thread, length);
        return;
    }
    spread_line(line, thread, length, false);
    storageBarrier();
    workgroupBarrier();
    spread_line(line, thread, length, true);
}
