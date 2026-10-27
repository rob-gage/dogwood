// Copyright Rob Gage 2026

mod thermal;
mod thermal_cellular_tile_pass;

pub use thermal::Thermal;

/// Duration of one fixed tick in seconds, matching the collision step
const THERMAL_TIME_STEP: f32 = 1.0 / 60.0;

/// Temperature change in kelvin per tick below which a cell lets its tile sleep
const THERMAL_SLEEP_TEMPERATURE: f32 = 0.01;

/// Fixed-point units per heat capacity unit in fluid heat records
pub const THERMAL_FLUID_CAPACITY_SCALE: f32 = 1024.0;

/// Fixed-point units per heat unit in fluid heat records
pub const THERMAL_FLUID_ENERGY_SCALE: f32 = 64.0;

/// Fixed-point units per heat capacity unit times conduction unit in fluid heat records
pub const THERMAL_FLUID_CONDUCTION_SCALE: f32 = 1024.0;

