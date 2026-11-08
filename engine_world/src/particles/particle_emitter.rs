// Copyright Rob Gage 2026

use super::{ParticleEffect, ParticleSystem};
use crate::scenes::ScenePosition;

/// Rate-based continuous emission into a `ParticleSystem`
#[derive(Clone, Debug)]
pub struct ParticleEmitter {
    /// Particles per second
    pub rate: f32,
    /// Fractional particles carried between calls
    accumulator: f32,
}

impl ParticleEmitter {

    /// Creates an emitter with a rate in particles per second
    pub fn new(rate: f32) -> Self { Self { rate, accumulator: 0.0 } }

    /// Accumulates `rate * elapsed` and emits the whole particles reached, one per particle
    /// regardless of the effect's count range
    pub fn emit(
        &mut self,
        system: &mut ParticleSystem,
        effect: &ParticleEffect,
        position: ScenePosition,
        direction: [f32; 2],
        velocity: [f32; 2],
        elapsed: f32,
    ) {
        self.accumulator += (self.rate * elapsed).max(0.0);
        let whole: f32 = self.accumulator.floor();
        self.accumulator -= whole;
        for _ in 0..(whole as u32).min(system.capacity() as u32) {
            system.emit_counted(effect, position, direction, velocity, 1);
        }
    }

}
