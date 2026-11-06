// Copyright Rob Gage 2026

/// One view of the plants
struct DecorationView {
    viewport: vec2<f32>,
    pixels_per_tile: f32,
    time: f32,
    /// Camera-relative origin of the cell grid in tiles
    grid_origin: vec2<f32>,
    /// Camera-relative corner of the resident region in tiles
    region_origin: vec2<f32>,
    /// Resident region size in tiles
    region_size: vec2<u32>,
    /// Ring offset of the resident region in tiles
    ring_offset: vec2<u32>,
};

@group(0) @binding(0)
var<uniform> view: DecorationView;

@group(0) @binding(1)
var<storage, read> material_identifiers: array<u32>;

/// Temperature bits then amount of every cell
@group(0) @binding(2)
var<storage, read> matter: array<vec2<u32>>;

@group(0) @binding(3)
var atlas: texture_2d<f32>;

/// Displacement in tiles and its velocity, a ring aligned to the world
@group(0) @binding(4)
var field: texture_2d<f32>;

@group(0) @binding(5)
var field_sampler: sampler;

/// Camera-relative corner of a chunk's batch in tiles
struct DecorationBatch {
    origin: vec2<f32>,
};

@group(1) @binding(0)
var<uniform> batch: DecorationBatch;

struct DecorationOutput {
    @builtin(position) position: vec4<f32>,
    /// Camera-relative position in tiles
    @location(0) place: vec2<f32>,
    /// Position in the art in pixels, up from its bottom left
    @location(1) art: vec2<f32>,
    /// Left, top, width, and height of the art in the atlas
    @location(2) @interpolate(flat) frame: vec4<f32>,
    /// Atlas row of the ramp
    @location(3) @interpolate(flat) ramp: u32,
    /// Scorch, glow, and brightness
    @location(4) @interpolate(flat) state: vec3<f32>,
};

/// Returns the cell-buffer index of a cell at a camera-relative position in tiles, if resident
fn resident_cell(place: vec2<f32>) -> i32 {
    let cell: vec2<i32> = vec2<i32>(floor((place - view.region_origin) * 8.0));
    let size: vec2<i32> = vec2<i32>(view.region_size) * 8;
    if any(cell < vec2<i32>(0)) || any(cell >= size) { return -1; }
    let position: vec2<u32> = vec2<u32>(cell);
    let tile: vec2<u32> = (position / 8u + view.ring_offset) % view.region_size;
    return i32((tile.y * view.region_size.x + tile.x) * 64u + (position.y % 8u) * 8u + position.x % 8u);
}

/// Returns an output that draws nothing
fn nothing() -> DecorationOutput {
    var output: DecorationOutput;
    output.position = vec4<f32>(2.0, 2.0, 0.0, 1.0);
    return output;
}

/// Returns the motion field at a camera-relative position in tiles
fn field_at(place: vec2<f32>) -> vec4<f32> {
    let ring: vec2<f32> = place - view.region_origin + vec2<f32>(view.ring_offset);
    return textureSampleLevel(field, field_sampler, ring / vec2<f32>(view.region_size), 0.0);
}

/// Returns the clip position of a camera-relative position in tiles
fn clip(place: vec2<f32>) -> vec4<f32> {
    return vec4<f32>(place * view.pixels_per_tile * 2.0 / view.viewport, 0.0, 1.0);
}

/// Returns how far the air and whatever touches a plant carry a point, in cells from its root, of
/// a plant with some reach, given the field at its root
fn carried(local: vec2<f32>, root: vec2<f32>, at_root: vec4<f32>, reach: f32, flex: f32, gust: f32) -> vec2<f32> {
    let distance: f32 = length(local);
    let along: f32 = min(distance / reach, 1.25);
    var moved: vec2<f32> = (field_at(root + local / 8.0).xy + at_root.xy) * 0.6 * 8.0
        + vec2<f32>(gust * (0.28 + reach * 0.035), 0.0);
    moved *= flex * along * along;
    // stems do not stretch: what moves along one is mostly lost, and bending shortens it a little
    if distance > 0.01 {
        let direction: vec2<f32> = local / distance;
        moved -= direction * dot(moved, direction) * 0.75;
        let limit: f32 = reach * 0.85;
        let amount: f32 = length(moved);
        if amount > limit { moved *= limit / amount; }
        moved -= direction * min(amount, limit) * min(amount, limit) * 0.5 / max(distance, 1.0);
    }
    return moved;
}

/// Places one corner of one piece of a plant. The piece moves as one rigid thing, following where
/// the field carries its pivot and its far end, so that its pixels stay square and one cell across
/// at any position and rotation
@vertex
fn vertex_decoration(
    @location(0) words: vec4<u32>,
    @location(1) more: vec4<u32>,
    @builtin(vertex_index) vertex_index: u32,
) -> DecorationOutput {
    let cell: vec2<f32> = vec2<f32>(f32(words.x & 0xffffu), f32(words.x >> 16u));
    let root: vec2<f32> = batch.origin + (cell + 0.5) / 8.0;
    let reach: f32 = max(f32((more.x >> 8u) & 0xffu), 1.0);
    // whatever cannot reach the view costs nothing more
    let half_view: vec2<f32> = view.viewport * 0.5 / view.pixels_per_tile + reach * 0.15;
    if any(abs(root) > half_view) { return nothing(); }
    let support: vec2<f32> = vec2<f32>(
        f32(i32(more.z << 24u) >> 24u),
        f32(i32(more.z << 16u) >> 24u),
    );
    let support_cell: i32 = resident_cell(root + support / 8.0);
    var scorch: f32 = 0.0;
    if support_cell >= 0 {
        if material_identifiers[u32(support_cell)] != more.w { return nothing(); }
        scorch = smoothstep(420.0, 650.0, bitcast<f32>(matter[u32(support_cell)].x));
    }
    let size: vec2<f32> = vec2<f32>(f32(words.z & 0xffu), f32((words.z >> 8u) & 0xffu));
    let pivot: vec2<f32> = vec2<f32>(f32((words.z >> 16u) & 0xffu), f32(words.z >> 24u));
    let offset: vec2<f32> = vec2<f32>(
        f32(i32(words.w << 24u) >> 24u),
        f32(i32(words.w << 16u) >> 24u),
    ) * 0.5;
    let drawn: f32 = f32(words.w >> 24u);
    let extent: vec2<f32> = vec2<f32>(size.x, select(size.y, drawn, drawn > 0.0));
    let flex: f32 = f32((more.x >> 16u) & 0xffu) / 255.0;
    let phase: f32 = f32(more.x >> 24u) / 256.0 * 6.2831853;
    let flutter: f32 = f32((more.z >> 16u) & 0xffu) / 255.0;
    let is_mirrored: bool = (more.z >> 24u) != 0u;
    let at_root: vec4<f32> = field_at(root);
    // the air moves in slow gusts across the world, the same at neighboring plants
    let gust: f32 = sin(view.time * 0.7 + cell.x * 0.012 + batch.origin.x * 0.1)
        + 0.5 * sin(view.time * 1.6 + cell.x * 0.031 + batch.origin.x * 0.25 + phase * 0.5);
    let corner: vec2<f32> = vec2<f32>(
        select(0.0, extent.x, (vertex_index & 1u) == 1u),
        select(0.0, extent.y, vertex_index < 2u),
    );
    var from_pivot: vec2<f32> = corner - pivot;
    if is_mirrored { from_pivot.x = -from_pivot.x; }
    let rest_turn: f32 = f32((words.w >> 16u) & 0xffu) / 256.0 * 6.2831853;
    let up: vec2<f32> = vec2<f32>(-sin(rest_turn), cos(rest_turn)) * max(extent.y - pivot.y, 1.0);
    let moved_pivot: vec2<f32> = offset + carried(offset, root, at_root, reach, flex, gust);
    let far_end: vec2<f32> = offset + up;
    let moved_far: vec2<f32> = far_end + carried(far_end, root, at_root, reach, flex, gust);
    let way: vec2<f32> = moved_far - moved_pivot;
    // a leaf also trails its plant's motion around its own stalk
    let turn: f32 = atan2(way.y, way.x) - 1.5707963
        + flutter * (sin(view.time * 2.1 + phase) * 0.09 + gust * 0.05
            - clamp(at_root.z * 0.05, -0.7, 0.7));
    let sine: f32 = sin(turn);
    let cosine: f32 = cos(turn);
    let local: vec2<f32> = moved_pivot + vec2<f32>(
        from_pivot.x * cosine - from_pivot.y * sine,
        from_pivot.x * sine + from_pivot.y * cosine,
    );
    var output: DecorationOutput;
    output.place = root + local / 8.0;
    output.position = clip(output.place);
    output.art = corner;
    output.frame = vec4<f32>(f32(words.y & 0xffffu), f32(words.y >> 16u), size);
    output.ramp = more.y & 0xffffu;
    output.state = vec3<f32>(
        scorch,
        f32((more.y >> 16u) & 0xffu) / 255.0 * 4.0,
        f32(more.y >> 24u) / 255.0,
    );
    return output;
}

/// Colors one pixel of art through its ramp, lit from the cell beneath it. Nothing snaps to the
/// cell grid, so art moves as smoothly as the screen allows
@fragment
fn fragment(input: DecorationOutput) -> @location(0) vec4<f32> {
    // rows repeat along a piece drawn longer than its frame
    var texel: vec2<f32> = floor(input.art);
    texel.y -= floor(texel.y / input.frame.w) * input.frame.w;
    if texel.x < 0.0 || texel.y < 0.0 || texel.x >= input.frame.z || texel.y >= input.frame.w {
        discard;
    }
    let art: vec4<f32> = textureLoad(
        atlas,
        vec2<i32>(i32(input.frame.x + texel.x), i32(input.frame.y + input.frame.w - 1.0 - texel.y)),
        0,
    );
    if art.a < 0.5 { discard; }
    // shades are stored as evenly spaced sRGB values, read here as linear light
    let shade: i32 = select(select(select(0, 1, art.r > 0.04), 2, art.r > 0.2), 3, art.r > 0.7);
    let row: i32 = i32(input.ramp) + select(0, 1, art.g > 0.5);
    let scorch: f32 = input.state.x;
    let color: vec3<f32> = mix(
        textureLoad(atlas, vec2<i32>(shade, row), 0).rgb * input.state.z,
        vec3<f32>(0.018, 0.012, 0.009),
        scorch,
    );
    var emitted: vec3<f32> = vec3<f32>(0.0);
    if art.a < 0.999 { emitted = color * input.state.y * (1.0 - scorch); }
    let center: vec2<f32> = (floor((input.place - view.grid_origin) * 8.0) + 0.5) / 8.0 + view.grid_origin;
    return vec4<f32>(color * light_at_tiles(center) + emitted, 1.0);
}
