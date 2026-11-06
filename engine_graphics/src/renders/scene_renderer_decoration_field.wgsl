// Copyright Rob Gage 2026

/// One thing pushing the field
struct Mover {
    /// Center and half size of its bounds in camera-relative tiles
    bounds: vec4<f32>,
    /// Velocity in tiles per second
    velocity: vec4<f32>,
};

struct FieldUniforms {
    /// Camera-relative corner of the resident region in tiles
    region_origin: vec2<f32>,
    /// Seconds to advance
    elapsed: f32,
    padding: f32,
    /// Resident region size in tiles
    region_size: vec2<u32>,
    /// Ring offset of the resident region in tiles
    ring_offset: vec2<u32>,
    /// Corner of the texels stepped, counted from the region's corner
    step_origin: vec2<u32>,
    /// Size of the texels stepped
    step_size: vec2<u32>,
    movers: array<Mover, 8>,
};

@group(0) @binding(0)
var<uniform> uniforms: FieldUniforms;

/// Gas species slots and the phantom air, flows, and stored pressure, two records per cell
@group(0) @binding(1)
var<storage, read> gas: array<vec4<u32>>;

/// Displacement in tiles and its velocity, a ring aligned to the world like the cell buffers
@group(0) @binding(2)
var previous: texture_2d<f32>;

@group(0) @binding(3)
var next: texture_storage_2d<rgba16float, write>;

/// Field texels along one tile
const TEXELS_PER_TILE: u32 = 2u;
/// Pull back toward rest per tile of displacement, per second squared
const STIFFNESS: f32 = 45.0;
/// Velocity lost per second, leaving a visible overshoot
const DAMPING: f32 = 4.2;
/// Pull toward neighboring texels per tile of difference, per second squared, spreading waves
const COUPLING: f32 = 26.0;
/// Reach of a mover's touch past its bounds, in tiles
const CONTACT_MARGIN: f32 = 0.3;
/// Push out of a mover's bounds in tiles per second squared
const CONTACT_PUSH: f32 = 80.0;
/// How quickly touched texels take on a mover's velocity, per second
const CONTACT_DRAG: f32 = 14.0;
/// Tiles per second of a gas flow of one cell per pressure substep at three substeps per tick
const GAS_SPEED: f32 = 22.5;
/// Share of a gas's speed felt as acceleration, per second
const GAS_RESPONSE: f32 = 1.4;
/// Largest acceleration from gas in tiles per second squared
const GAS_ACCELERATION_MAXIMUM: f32 = 260.0;
/// Largest displacement in tiles
const DISPLACEMENT_MAXIMUM: f32 = 6.0;

@compute @workgroup_size(16, 16)
fn step_field(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= uniforms.step_size.x || id.y >= uniforms.step_size.y { return; }
    let size: vec2<u32> = uniforms.region_size * TEXELS_PER_TILE;
    let logical: vec2<u32> = uniforms.step_origin + id.xy;
    let texel: vec2<u32> = (logical + uniforms.ring_offset * TEXELS_PER_TILE) % size;
    let here: vec4<f32> = textureLoad(previous, texel, 0);
    let left: vec2<f32> = textureLoad(previous, vec2<u32>((texel.x + size.x - 1u) % size.x, texel.y), 0).xy;
    let right: vec2<f32> = textureLoad(previous, vec2<u32>((texel.x + 1u) % size.x, texel.y), 0).xy;
    let below: vec2<f32> = textureLoad(previous, vec2<u32>(texel.x, (texel.y + size.y - 1u) % size.y), 0).xy;
    let above: vec2<f32> = textureLoad(previous, vec2<u32>(texel.x, (texel.y + 1u) % size.y), 0).xy;
    let displacement: vec2<f32> = here.xy;
    var velocity: vec2<f32> = here.zw;
    var acceleration: vec2<f32> = displacement * -STIFFNESS
        + velocity * -DAMPING
        + (left + right + below + above - displacement * 4.0) * COUPLING;
    // the moving air at this texel: slots and flows of one cell near its middle
    let ring_tile: vec2<u32> = texel / TEXELS_PER_TILE;
    let within: vec2<u32> = texel % TEXELS_PER_TILE * 4u + 2u;
    let cell: u32 = (ring_tile.y * uniforms.region_size.x + ring_tile.x) * 64u + within.y * 8u + within.x;
    let slots: vec4<u32> = gas[cell * 2u];
    let words: vec4<u32> = gas[cell * 2u + 1u];
    let amount: f32 = f32(slots.x & 0xffffffu) + f32(slots.y & 0xffffffu)
        + f32(slots.z & 0xffffffu) + f32(slots.w & 0xffffffu) + f32(words.x);
    let flow: vec2<f32> = vec2<f32>(bitcast<f32>(words.y), bitcast<f32>(words.z))
        / max(amount, 1.0) * GAS_SPEED * GAS_RESPONSE;
    if flow.x == flow.x && flow.y == flow.y {
        let speed: f32 = length(flow);
        if speed > 0.0 {
            acceleration += flow / speed * min(speed, GAS_ACCELERATION_MAXIMUM);
        }
    }
    // whatever touches this texel, felt only at its real bounds
    let place: vec2<f32> = uniforms.region_origin
        + (vec2<f32>(logical) + 0.5) / f32(TEXELS_PER_TILE);
    for (var index: u32 = 0u; index < 8u; index++) {
        let mover: Mover = uniforms.movers[index];
        let half_size: vec2<f32> = mover.bounds.zw;
        if half_size.x <= 0.0 { continue; }
        let offset: vec2<f32> = place - mover.bounds.xy;
        let distance: f32 = length(max(abs(offset) - half_size, vec2<f32>(0.0)));
        let contact: f32 = 1.0 - smoothstep(0.0, CONTACT_MARGIN, distance);
        if contact <= 0.0 { continue; }
        // zero at the mover's center, so that nothing flips as it passes over a plant
        let outward: vec2<f32> = clamp(offset / (half_size + CONTACT_MARGIN), vec2<f32>(-1.0), vec2<f32>(1.0));
        acceleration += (outward * CONTACT_PUSH + (mover.velocity.xy - velocity) * CONTACT_DRAG) * contact;
    }
    velocity += acceleration * uniforms.elapsed;
    var moved: vec2<f32> = displacement + velocity * uniforms.elapsed;
    let reach: f32 = length(moved);
    if reach > DISPLACEMENT_MAXIMUM { moved *= DISPLACEMENT_MAXIMUM / reach; }
    textureStore(next, texel, vec4<f32>(moved, velocity));
}
