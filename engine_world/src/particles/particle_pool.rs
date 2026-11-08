// Copyright Rob Gage 2026

use super::ParticleEffect;

/// One simulated particle
#[derive(Clone, Copy, Debug)]
struct Particle {
    /// Position in tiles relative to the pool's anchor
    position: [f64; 2],
    /// Velocity in tiles per second
    velocity: [f32; 2],
    /// Age in seconds
    age: f32,
    /// Lifetime in seconds
    lifetime: f32,
    /// Start size in cells
    size: f32,
    /// Size multiplier at end of life
    size_end: f32,
    /// Start color
    color: [f32; 4],
    /// End color
    color_end: [f32; 4],
    /// Fraction of life fading in
    fade_in: f32,
    /// Birth radiance intensity
    glow: f32,
    /// Gravity in tiles per second squared
    gravity: f32,
    /// Drag per second
    drag: f32,
    /// Stretch factor
    stretch: f32,
    /// Sprite depth
    depth: i32,
}

/// One particle ready to draw: position in tiles from the anchor, size in cells, color,
/// radiance intensity, rotation, and depth
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParticleSample {
    /// Position in tiles relative to the anchor
    pub position: [f64; 2],
    /// Width and height in cells, before rounding
    pub size: [f32; 2],
    /// Rotation in radians
    pub rotation: f32,
    /// Linear RGBA color
    pub color: [f32; 4],
    /// Radiance intensity
    pub glow: f32,
    /// Sprite depth
    pub depth: i32,
}

/// Fixed-capacity particle simulation with no rendering. Positions are in tiles relative to an
/// anchor owned by the caller, with y increasing upward. When full, new particles are dropped.
#[derive(Clone, Debug)]
pub struct ParticlePool {
    /// Live particles
    particles: Vec<Particle>,
    /// Maximum live particles
    capacity: usize,
    /// Random state
    state: u64,
}

impl ParticlePool {

    /// Creates an empty pool
    pub fn new(capacity: usize) -> Self {
        Self { particles: Vec::with_capacity(capacity), capacity, state: 0x9E37_79B9_7F4A_7C15 }
    }

    /// Maximum live particles
    pub fn capacity(&self) -> usize { self.capacity }

    /// Live particle count
    pub fn len(&self) -> usize { self.particles.len() }

    /// Whether no particles are live
    pub fn is_empty(&self) -> bool { self.particles.is_empty() }

    /// Removes every particle
    pub fn clear(&mut self) { self.particles.clear(); }

    /// Reseeds the random generator
    pub fn set_seed(&mut self, seed: u64) { self.state = seed; }

    /// Position of the first live particle relative to the anchor
    pub fn first_position(&self) -> Option<[f64; 2]> {
        self.particles.first().map(|particle| particle.position)
    }

    /// Moves every particle by an offset, used when the anchor changes
    pub fn shift(&mut self, offset: [f64; 2]) {
        for particle in &mut self.particles {
            particle.position[0] += offset[0];
            particle.position[1] += offset[1];
        }
    }

    /// Next random value in `[0, 1)` (splitmix64)
    fn random(&mut self) -> f32 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z: u64 = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        (z >> 40) as f32 / (1u64 << 24) as f32
    }

    /// Random value between two bounds
    fn between(&mut self, range: [f32; 2]) -> f32 {
        range[0] + (range[1] - range[0]) * self.random()
    }

    /// Picks a burst count from an effect's range scaled by `scale`
    pub fn burst_count(&mut self, effect: &ParticleEffect, scale: f32) -> u32 {
        let low: f32 = effect.count[0] as f32 * scale;
        let high: f32 = effect.count[1].max(effect.count[0]) as f32 * scale;
        let value: f32 = low + (high - low + 1.0) * self.random();
        (value.floor().max(0.0) as u32).min(high.ceil().max(0.0) as u32)
    }

    /// Spawns `count` particles at a position relative to the anchor, dropping any beyond capacity
    pub fn spawn(
        &mut self,
        effect: &ParticleEffect,
        position: [f64; 2],
        direction: [f32; 2],
        velocity: [f32; 2],
        count: u32,
    ) {
        let length: f32 = direction[0].hypot(direction[1]);
        let base: f32 = if length > 1e-6 { direction[1].atan2(direction[0]) } else { 0.0 };
        for _ in 0..count {
            if self.particles.len() >= self.capacity { return; }
            let angle: f32 = base + (self.random() * 2.0 - 1.0) * effect.spread;
            let speed: f32 = self.between(effect.speed);
            let jitter_angle: f32 = self.random() * std::f32::consts::TAU;
            let jitter_radius: f32 = effect.jitter * self.random().sqrt();
            let mix: f32 = self.random();
            let color: [f32; 4] = std::array::from_fn(|index| {
                effect.colors[0][index] + (effect.colors[1][index] - effect.colors[0][index]) * mix
            });
            let lifetime: f32 = self.between(effect.lifetime).max(1e-4);
            let size: f32 = self.between(effect.size);
            self.particles.push(Particle {
                position: [
                    position[0] + f64::from(jitter_angle.cos() * jitter_radius),
                    position[1] + f64::from(jitter_angle.sin() * jitter_radius),
                ],
                velocity: [
                    angle.cos() * speed + velocity[0] * effect.inherit,
                    angle.sin() * speed + velocity[1] * effect.inherit,
                ],
                age: 0.0,
                lifetime,
                size,
                size_end: effect.size_end,
                color,
                color_end: effect.color_end,
                fade_in: effect.fade_in,
                glow: effect.glow,
                gravity: effect.gravity,
                drag: effect.drag,
                stretch: effect.stretch,
                depth: effect.depth,
            });
        }
    }

    /// Advances every particle and removes the expired ones
    pub fn step(&mut self, elapsed: f32) {
        let mut index: usize = 0;
        while index < self.particles.len() {
            let particle: &mut Particle = &mut self.particles[index];
            particle.age += elapsed;
            if particle.age >= particle.lifetime {
                self.particles.swap_remove(index);
                continue;
            }
            let damping: f32 = (-particle.drag * elapsed).exp();
            particle.velocity[0] *= damping;
            particle.velocity[1] = particle.velocity[1] * damping - particle.gravity * elapsed;
            particle.position[0] += f64::from(particle.velocity[0] * elapsed);
            particle.position[1] += f64::from(particle.velocity[1] * elapsed);
            index += 1;
        }
    }

    /// Calls a function with every live particle's drawable state
    pub fn for_each(&self, mut visit: impl FnMut(ParticleSample)) {
        for particle in &self.particles {
            let life: f32 = (particle.age / particle.lifetime).clamp(0.0, 1.0);
            let mut color: [f32; 4] = std::array::from_fn(|index| {
                particle.color[index] + (particle.color_end[index] - particle.color[index]) * life
            });
            if particle.fade_in > 0.0 { color[3] *= (life / particle.fade_in).min(1.0); }
            color[3] = color[3].clamp(0.0, 1.0);
            let size: f32 = particle.size * (1.0 + (particle.size_end - 1.0) * life);
            let (width, height, rotation): (f32, f32, f32) = if particle.stretch > 0.0 {
                let speed: f32 = particle.velocity[0].hypot(particle.velocity[1]);
                (
                    size + speed * particle.stretch,
                    size,
                    particle.velocity[1].atan2(particle.velocity[0]),
                )
            } else { (size, size, 0.0) };
            visit(ParticleSample {
                position: particle.position,
                size: [width, height],
                rotation,
                color,
                glow: particle.glow * color[3],
                depth: particle.depth,
            });
        }
    }

}

#[cfg(test)]
mod tests {

    use super::*;

    fn effect() -> ParticleEffect {
        ParticleEffect { count: [4, 4], lifetime: [1.0, 1.0], ..ParticleEffect::DEFAULT }
    }

    #[test]
    fn particles_expire_after_lifetime() {
        let mut pool = ParticlePool::new(16);
        pool.spawn(&effect(), [0.0, 0.0], [1.0, 0.0], [0.0, 0.0], 4);
        assert_eq!(pool.len(), 4);
        pool.step(0.5);
        assert_eq!(pool.len(), 4);
        pool.step(0.6);
        assert!(pool.is_empty());
    }

    #[test]
    fn capacity_drops_new_particles() {
        let mut pool = ParticlePool::new(3);
        pool.spawn(&effect(), [0.0, 0.0], [1.0, 0.0], [0.0, 0.0], 10);
        assert_eq!(pool.len(), 3);
    }

    #[test]
    fn gravity_pulls_downward() {
        let mut pool = ParticlePool::new(4);
        let falling = ParticleEffect { speed: [0.0, 0.0], gravity: 10.0, ..effect() };
        pool.spawn(&falling, [0.0, 0.0], [1.0, 0.0], [0.0, 0.0], 1);
        pool.step(0.1);
        assert!(pool.first_position().unwrap()[1] < 0.0);
        let rising = ParticleEffect { speed: [0.0, 0.0], gravity: -10.0, ..effect() };
        let mut pool = ParticlePool::new(4);
        pool.spawn(&rising, [0.0, 0.0], [1.0, 0.0], [0.0, 0.0], 1);
        pool.step(0.1);
        assert!(pool.first_position().unwrap()[1] > 0.0);
    }

    #[test]
    fn seed_is_deterministic() {
        let wide = ParticleEffect { speed: [1.0, 5.0], jitter: 1.0, ..effect() };
        let run = |seed: u64| {
            let mut pool = ParticlePool::new(16);
            pool.set_seed(seed);
            pool.spawn(&wide, [0.0, 0.0], [0.0, 1.0], [0.0, 0.0], 4);
            pool.step(0.25);
            let mut out: Vec<ParticleSample> = Vec::new();
            pool.for_each(|sample| out.push(sample));
            out
        };
        assert_eq!(run(7), run(7));
        assert_ne!(run(7), run(8));
    }

    #[test]
    fn burst_count_stays_in_range() {
        let mut pool = ParticlePool::new(4);
        let ranged = ParticleEffect { count: [2, 5], ..effect() };
        for _ in 0..200 {
            let count = pool.burst_count(&ranged, 1.0);
            assert!((2..=5).contains(&count));
        }
        assert_eq!(pool.burst_count(&ranged, 0.0), 0);
    }

}
