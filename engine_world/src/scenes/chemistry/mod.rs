// Copyright Rob Gage 2026

mod chemistry;
mod chemistry_cellular_tile_pass;

pub use chemistry::Chemistry;

/// Duration of one fixed tick in seconds, matching the collision step
const CHEMISTRY_TIME_STEP: f32 = 1.0 / 60.0;

#[cfg(test)]
mod contact_traversal_tests;

#[cfg(test)]
mod pair_cache_solver_tests;
