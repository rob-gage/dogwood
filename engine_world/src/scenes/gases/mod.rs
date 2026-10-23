// Copyright Rob Gage 2026

mod gas_cellular_tile_pass;
mod gases;

pub use gases::Gases;

/// Gas amount units in one atmosphere, matching the shaders
pub const GAS_AMOUNT_PER_ATMOSPHERE: f32 = 65536.0;

/// Largest gas amount one species slot holds
pub const GAS_AMOUNT_MAXIMUM: u32 = 0xff_ffff;

/// Duration of one fixed tick in seconds, matching the collision step
const GAS_TIME_STEP: f32 = 1.0 / 60.0;

/// Substeps of each tick, even so the gas alternates between its two buffers and ends in the
/// cellular tile buffers
const GAS_SUBSTEP_COUNT: usize = 4;

/// Fraction of scene gravity that lifts and sinks gases, keeping buoyant flows well below the
/// speed of pressure waves so gas columns stay close to the ambient pressure
const GAS_BUOYANCY_SCALE: f32 = 0.5;
