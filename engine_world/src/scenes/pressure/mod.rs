// Copyright Rob Gage 2026

mod pressure;
mod pressure_cellular_tile_pass;

pub use pressure::Pressure;

/// Gas amount units in one atmosphere, matching the shaders
pub const GAS_AMOUNT_PER_ATMOSPHERE: f32 = 4096.0;

/// Largest gas amount one species slot holds
pub const GAS_AMOUNT_MAXIMUM: u32 = 0xff_ffff;

/// Largest pressure in atmospheres one cell holds, in its gas or as stored pressure
pub const PRESSURE_MAXIMUM: f32 = 4095.0;

/// Duration of one fixed tick in seconds, matching the collision step
const PRESSURE_TIME_STEP: f32 = 1.0 / 60.0;

/// Substeps of each tick, even so the cells alternate between their two buffers and end in the
/// cellular tile buffers
const PRESSURE_SUBSTEP_COUNT: usize = 6;

/// Fraction of scene gravity that lifts and sinks gases, keeping buoyant flows well below the
/// speed of pressure waves so gas columns stay close to the ambient pressure
const PRESSURE_BUOYANCY_SCALE: f32 = 0.5;

/// Speed of pressure waves through gas in tiles per second
const PRESSURE_GAS_SPEED: f32 = 18.0;

/// Speed of pressure waves through solid cells in tiles per second, below the stability limit of
/// 0.7 cells per substep
const PRESSURE_SOLID_SPEED: f32 = 30.0;

/// Impedance of a solid cell of unit mass relative to gas, so that heavier solids reflect more of
/// the pressure reaching them
const PRESSURE_SOLID_IMPEDANCE: f32 = 5.0;

/// Fraction of every face flow lost per second
const PRESSURE_FLOW_DAMPING: f32 = 0.96;

/// Flow added per second for each unit of difference in the net outflow per second of the cells
/// across a face, damping pressure waves without damping swirls
const PRESSURE_BULK_VISCOSITY: f32 = 24.0;

/// Flow in amount units per second below which a face flow stops, so resting cells sleep
const PRESSURE_FLOW_MINIMUM: f32 = 360.0;

/// Fraction of phantom air exchanged with each neighboring cell per second
const PRESSURE_AMBIENT_DIFFUSIVITY: f32 = 4.8;

/// Rate per second at which a face flow approaches the velocity of an actor dragging its gas
const PRESSURE_ACTOR_DRAG: f32 = 53.5;

/// Rate per second at which stored pressure relaxes toward the ambient pressure
const PRESSURE_SOLID_RELAXATION: f32 = 2.0;

/// Rate per second at which a solid cell pushes the gas trapped in it out through its open faces
const PRESSURE_TRAPPED_EXPULSION: f32 = 8.0;

/// Fraction of a solid cell's integrity lost per second for each atmosphere its load exceeds its
/// strength, per unit of its material's absorption
const PRESSURE_DAMAGE_RATE: f32 = 40.0;

/// Largest number of tiles whose broken cells one tick reports to collisions, beyond which
/// collisions read every tile
pub const PRESSURE_BROKEN_CELLULAR_TILE_COUNT_MAXIMUM: u64 = 1024;

/// Largest number of rigid body cells one tick breaks, beyond which the rest stay with their
/// bodies as debris until another tick reports them
pub const PRESSURE_BROKEN_RIGID_BODY_CELL_COUNT_MAXIMUM: u64 = 4096;
