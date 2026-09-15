// Copyright Rob Gage 2026

/// Persistent per-cell appearance variation packed as four signed normalized bytes.
#[derive(Copy, Clone, Default)]
pub struct CellularAppearance(pub u32);

impl CellularAppearance {
    /// No appearance variation on any channel
    pub const NEUTRAL: Self = Self(0);

    /// Creates a packed appearance from normalized RGBA channels
    pub fn from_channels(channels: [f32; 4]) -> Self {
        let mut packed: u32 = 0;
        for (index, channel) in channels.into_iter().enumerate() {
            let value: i32 = (channel.clamp(-1.0, 1.0) * 127.0).round() as i32;
            packed |= (value as u8 as u32) << (index * 8);
        }
        Self(packed)
    }

    /// Creates a deterministic correlated sample scaled by per-channel amplitudes
    pub fn from_seed(seed: u32, variation: [f32; 4]) -> Self {
        let mut hash: u32 = seed.wrapping_add(0x9e37_79b9);
        hash ^= hash >> 16;
        hash = hash.wrapping_mul(0x85eb_ca6b);
        hash ^= hash >> 13;
        let factor: f32 = ((hash & 0xffff) as f32 / 32767.5) - 1.0;
        Self::from_channels(variation.map(|amplitude| factor * amplitude))
    }

    /// Returns the packed signed-normalized RGBA channels
    pub fn channels(self) -> [f32; 4] {
        std::array::from_fn(|index| {
            let value: i8 = (self.0 >> (index * 8)) as u8 as i8;
            (i32::from(value).max(-127) as f32) / 127.0
        })
    }
}
