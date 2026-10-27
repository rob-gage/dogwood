// Copyright Rob Gage 2026

/// Inputs for exact optical and backlight cache comparison
struct Params {
    width: u32,
    height: u32,
    count: u32,
    force: u32,
};

@group(0) @binding(0)
var<uniform> params: Params;

@group(0) @binding(1)
var optical: texture_2d<f32>;

@group(0) @binding(2)
var backlight: texture_2d<f32>;

/// Packed optical pixels followed by packed backlight pixels
@group(0) @binding(3)
var<storage, read_write> previous: array<vec2<u32>>;

/// Cache state words. Words 0 to 1 and 4 to 8 are cleared before every frame; words 2, 3 and 9
/// persist.
///  - 0: either input texture differs from its previous contents
///  - 1: some optical cell emits
///  - 2: every cascade interval must be recomputed
///  - 3: some optical cell emitted last frame
///  - 4 to 7: bounds of the changed optical cells as the domain width minus their minimum x,
///    their exclusive maximum x, the domain height minus their minimum y and their exclusive
///    maximum y; zero when none changed
///  - 8: some backlight texel emits
///  - 9: some light source existed last frame
@group(0) @binding(4)
var<storage, read_write> dirty: array<atomic<u32>>;

@group(0) @binding(5)
var<storage, read> source_counts: array<vec4<u32>>;

/// Three indirect dispatch counts per optical block
@group(0) @binding(6)
var<storage, read_write> args: array<u32>;

var<workgroup> changed: atomic<u32>;
var<workgroup> emissive: atomic<u32>;
var<workgroup> backlit: atomic<u32>;
var<workgroup> bounds: array<atomic<u32>, 4>;

/// Compares both textures exactly as packed `Rgba16Float` values and updates the cache
@compute @workgroup_size(8, 8, 1)
fn compare(@builtin(global_invocation_id) global_id: vec3<u32>, @builtin(local_invocation_index) local: u32) {
    if local == 0u {
        atomicStore(&changed, 0u);
        atomicStore(&emissive, 0u);
        atomicStore(&backlit, 0u);
        for (var i: u32 = 0u; i < 4u; i++) {
            atomicStore(&bounds[i], 0u);
        }
    }
    workgroupBarrier();
    let cell: vec2<u32> = global_id.xy;
    if cell.x < params.width && cell.y < params.height {

    let index: u32 = cell.y * params.width + cell.x;
    let optical_value: vec4<f32> = textureLoad(optical, vec2<i32>(cell), 0);
    if any(optical_value.rgb != vec3<f32>(0.0)) {atomicStore(&emissive, 1u);}
    let packed_optical: vec2<u32> = vec2<u32>(
        pack2x16float(optical_value.rg),
        pack2x16float(optical_value.ba),
    );
    if any(previous[index] != packed_optical) {
        atomicStore(&changed, 1u);
        atomicMax(&bounds[0], params.width - cell.x);
        atomicMax(&bounds[1], cell.x + 1u);
        atomicMax(&bounds[2], params.height - cell.y);
        atomicMax(&bounds[3], cell.y + 1u);
        previous[index] = packed_optical;
    }

    let backlight_size: vec2<u32> = textureDimensions(backlight, 0);
    if cell.x < backlight_size.x && cell.y < backlight_size.y {
        let backlight_index: u32 = params.width * params.height
            + cell.y * backlight_size.x + cell.x;
        let backlight_value: vec4<f32> = textureLoad(backlight, vec2<i32>(cell), 0);
        if any(backlight_value.rgb != vec3<f32>(0.0)) {atomicStore(&backlit, 1u);}
        let packed_backlight: vec2<u32> = vec2<u32>(
            pack2x16float(backlight_value.rg),
            pack2x16float(backlight_value.ba),
        );
        if any(previous[backlight_index] != packed_backlight) {
            atomicStore(&changed, 1u);
            previous[backlight_index] = packed_backlight;
        }
    }
    }
    workgroupBarrier();
    if local == 0u {
        if atomicLoad(&changed)!=0u {atomicStore(&dirty[0],1u);}
        if atomicLoad(&emissive)!=0u {atomicStore(&dirty[1],1u);}
        if atomicLoad(&backlit) != 0u {atomicStore(&dirty[8], 1u);}
        if atomicLoad(&bounds[1]) != 0u {
            for (var i: u32 = 0u; i < 4u; i++) {
                atomicMax(&dirty[4u + i], atomicLoad(&bounds[i]));
            }
        }
    }
}

/// Builds indirect lighting dispatch counts, running each source count's kind only when it is
/// needed: kind 0 spreads, 1 traces cascades, 2 clears fluence, 3 clears light and 4 builds light
/// mips. Without any light source, light is zero and is cleared only after a source existed or
/// on a fresh domain.
@compute @workgroup_size(1, 1, 1)
fn arguments() {
    let is_dirty: bool = (params.force & 1u) != 0u || atomicLoad(&dirty[0]) != 0u;
    let emits: bool = atomicLoad(&dirty[1]) != 0u;
    let sources: bool = emits || atomicLoad(&dirty[8]) != 0u;
    let clears_light: bool = !sources
        && ((params.force & 2u) != 0u || atomicLoad(&dirty[9]) != 0u);
    // bypassed levels hold stale intervals after a span without optical emission
    let full: bool = (params.force & 1u) != 0u || (emits && atomicLoad(&dirty[3]) == 0u);
    atomicStore(&dirty[9], select(0u, 1u, sources));
    atomicStore(&dirty[2], select(0u, 1u, full));
    atomicStore(&dirty[3], select(0u, 1u, emits));
    for (var index: u32 = 0u; index < params.count; index++) {
        let kind: u32 = source_counts[index].w;
        let runs: bool = kind == 0u || kind == 4u || (kind == 1u && emits) || (kind == 2u && !emits);
        let enabled: bool = (is_dirty && sources && runs)
            || (clears_light && (kind == 3u || kind == 4u));
        let counts: vec3<u32> = select(vec3<u32>(0u), source_counts[index].xyz, enabled);
        args[index * 3u] = counts.x;
        args[index * 3u + 1u] = counts.y;
        args[index * 3u + 2u] = counts.z;
    }
}
