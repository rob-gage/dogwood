// Copyright Rob Gage 2026

/// WGSL `pack_radiance` and `unpack_radiance`, matching the Rust functions
pub const MATERIAL_RADIANCE_WGSL: &str = include_str!("material_radiance.wgsl");

/// Largest linear emission a packed radiance holds in each channel
pub const MATERIAL_RADIANCE_MAXIMUM: f32 = 127.0;

/// Packs linear red, green, and blue emission and an occlusion into one `u32`.
///
/// Emission shares one exponent: bits 0-20 hold three 7-bit mantissas, bits 21-24 the exponent,
/// and bits 25-31 the occlusion, the fraction of light one tile of thickness blocks. Emission is
/// clamped to `0.0..=MATERIAL_RADIANCE_MAXIMUM` and occlusion to `0.0..=1.0`; nonfinite values
/// become zero.
pub fn pack_radiance(radiance: [f32; 4]) -> u32 {
    let clamp = |value: f32, maximum: f32| -> f32 {
        if value.is_finite() { value.clamp(0.0, maximum) } else { 0.0 }
    };
    let red: f32 = clamp(radiance[0], MATERIAL_RADIANCE_MAXIMUM);
    let green: f32 = clamp(radiance[1], MATERIAL_RADIANCE_MAXIMUM);
    let blue: f32 = clamp(radiance[2], MATERIAL_RADIANCE_MAXIMUM);
    let occlusion: u32 = (clamp(radiance[3], 1.0) * 127.0).round() as u32;
    let brightest: f32 = red.max(green).max(blue);
    let mut exponent: i32 = if brightest > 0.0 {
        (brightest.log2().floor() as i32 + 1 + 8).clamp(0, 15)
    } else {
        0
    };
    // rounding the brightest channel up to a whole mantissa can need the next exponent
    if (brightest / 2.0_f32.powi(exponent - 8) * 128.0).round() >= 128.0 && exponent < 15 {
        exponent += 1;
    }
    let scale: f32 = 128.0 / 2.0_f32.powi(exponent - 8);
    let mantissa = |value: f32| -> u32 { ((value * scale).round() as u32).min(127) };
    mantissa(red)
        | mantissa(green) << 7
        | mantissa(blue) << 14
        | (exponent as u32) << 21
        | occlusion << 25
}

/// Unpacks linear red, green, and blue emission and an occlusion packed by `pack_radiance`
pub fn unpack_radiance(packed: u32) -> [f32; 4] {
    let scale: f32 = 2.0_f32.powi(((packed >> 21) & 15) as i32 - 8) / 128.0;
    [
        (packed & 127) as f32 * scale,
        ((packed >> 7) & 127) as f32 * scale,
        ((packed >> 14) & 127) as f32 * scale,
        (packed >> 25) as f32 / 127.0,
    ]
}
