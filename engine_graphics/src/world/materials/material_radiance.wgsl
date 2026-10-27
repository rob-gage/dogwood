// Copyright Rob Gage 2026

/// Largest linear emission a packed radiance holds in each channel
const MATERIAL_RADIANCE_MAXIMUM: f32 = 127.0;

/// Packs linear red, green, and blue emission and an occlusion per tile into one `u32`, matching
/// the Rust `pack_radiance`
fn pack_radiance(radiance: vec4<f32>) -> u32 {
    // NaN fails every comparison, so `select` turns it into zero
    let finite: vec4<f32> = select(vec4<f32>(0.0), radiance, radiance == radiance);
    let emission: vec3<f32> = clamp(finite.rgb, vec3<f32>(0.0), vec3<f32>(MATERIAL_RADIANCE_MAXIMUM));
    let occlusion: u32 = u32(round(clamp(finite.a, 0.0, 1.0) * 127.0));
    let brightest: f32 = max(emission.r, max(emission.g, emission.b));
    var exponent: i32 = 0;
    if brightest > 0.0 {
        exponent = clamp(i32(floor(log2(brightest))) + 1 + 8, 0, 15);
    }
    if round(brightest / exp2(f32(exponent - 8)) * 128.0) >= 128.0 && exponent < 15 {
        exponent += 1;
    }
    let scale: f32 = 128.0 / exp2(f32(exponent - 8));
    let mantissas: vec3<u32> = min(vec3<u32>(round(emission * scale)), vec3<u32>(127u));
    return mantissas.r
        | (mantissas.g << 7u)
        | (mantissas.b << 14u)
        | (u32(exponent) << 21u)
        | (occlusion << 25u);
}

/// Unpacks linear red, green, and blue emission and an occlusion packed by `pack_radiance`
fn unpack_radiance(packed: u32) -> vec4<f32> {
    let scale: f32 = exp2(f32(i32((packed >> 21u) & 15u) - 8)) / 128.0;
    return vec4<f32>(
        f32(packed & 127u) * scale,
        f32((packed >> 7u) & 127u) * scale,
        f32((packed >> 14u) & 127u) * scale,
        f32(packed >> 25u) / 127.0,
    );
}
