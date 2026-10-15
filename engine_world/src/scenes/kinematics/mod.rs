// Copyright Rob Gage 2026

mod kinematics;
mod kinematics_cellular_tile_pass;

pub use kinematics::Kinematics;

/// Duration of one fixed tick in seconds, matching the collision step
const KINEMATICS_TIME_STEP: f32 = 1.0 / 60.0;

/// Maximum number of actors pushing and resting on dynamic cells in one tick
const KINEMATICS_ACTOR_COUNT_MAXIMUM: usize = 32;

/// Largest number of passes resolving cells that move together, each doubling the longest chain
const KINEMATICS_RESOLVE_COUNT: usize = 8;

/// Mass given to fixed bodies, which dynamic cells never move
const KINEMATICS_FIXED_MASS: f32 = 1.0e20;
