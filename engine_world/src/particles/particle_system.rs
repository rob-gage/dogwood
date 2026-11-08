// Copyright Rob Gage 2026

use super::{ParticleEffect, ParticleError, ParticlePool};
use crate::actors::{
    components::{
        ActorKinematicsPositionComponent,
        ActorPersistenceComponent,
        ActorSpritesComponent,
    },
    Actor,
    ActorRegistry,
};
use crate::scenes::{Scene, ScenePosition};
use engine_graphics::sprites::SpriteSheet;
use engine_host::acceleration::Accelerator;
use std::sync::Arc;

/// One actor sprite record
type SpriteRecord =
    (Arc<SpriteSheet>, Arc<SpriteSheet>, u32, f32, f32, f32, f32, f32, [f32; 4], f32, i32);

/// Cells per tile
const CELLS_PER_TILE: f64 = 8.0;
/// Offset bits within one cell
const CELL_OFFSET_MASK: u32 = (1 << 29) - 1;
/// Distance in tiles from the anchor beyond which the anchor moves to the first particle
const REANCHOR_DISTANCE: f64 = 128.0;

/// A pooled CPU particle system drawn by one sprite-only actor. The actor is persistent, so it
/// lasts as long as the scene unless `remove` is called. When the pool is full, new particles are
/// dropped.
pub struct ParticleSystem {
    /// Actor drawing the particles
    actor: Actor,
    /// Simulation
    pool: ParticlePool,
    /// Cell-aligned scene position that pool positions are relative to
    anchor: ScenePosition,
    /// Sprites of this update, swapped into the actor so both buffers keep their allocations
    sprites: Vec<SpriteRecord>,
    /// White pixel sheet
    pixel: Arc<SpriteSheet>,
    /// Radiance sheet that glows in the tint only with nonzero intensity
    radiance_pixel: Arc<SpriteSheet>,
}

/// Moves a position to the start of its cell
fn align_to_cell(position: ScenePosition) -> ScenePosition {
    ScenePosition {
        offset_x: position.offset_x & !CELL_OFFSET_MASK,
        offset_y: position.offset_y & !CELL_OFFSET_MASK,
        ..position
    }
}

/// Most cells a streak draws
const STREAK_CELLS_MAXIMUM: u32 = 12;

/// Snaps an axis (tiles from a cell-aligned anchor) so a sprite of `cells` cells lies on the grid
fn snap(tiles: f64, cells: u32) -> f32 {
    let scaled: f64 = tiles * CELLS_PER_TILE;
    let snapped: f64 = if cells % 2 == 1 { scaled.floor() + 0.5 } else { scaled.round() };
    (snapped / CELLS_PER_TILE) as f32
}

impl ParticleSystem {

    /// Spawns the particle actor with room for `capacity` live particles (2048 is a good start)
    pub fn new(
        scene: &mut Scene,
        accelerator: &Accelerator,
        capacity: usize,
    ) -> Result<Self, ParticleError> {
        let actor: Actor = scene.actor_registry_mutable().spawn((
            ActorKinematicsPositionComponent::default(),
            ActorSpritesComponent { sprites: Vec::new() },
            ActorPersistenceComponent,
        ))?;
        Ok(Self {
            actor,
            pool: ParticlePool::new(capacity),
            anchor: ScenePosition::default(),
            sprites: Vec::with_capacity(capacity),
            pixel: Arc::new(SpriteSheet::from_color(accelerator, 255, 255, 255, 255)?),
            radiance_pixel: Arc::new(SpriteSheet::from_color(accelerator, 255, 255, 255, 0)?),
        })
    }

    /// Maximum live particles
    pub fn capacity(&self) -> usize { self.pool.capacity() }

    /// Live particle count
    pub fn len(&self) -> usize { self.pool.len() }

    /// Whether no particles are live
    pub fn is_empty(&self) -> bool { self.pool.is_empty() }

    /// Removes every particle
    pub fn clear(&mut self) { self.pool.clear(); }

    /// Reseeds the deterministic random generator
    pub fn set_seed(&mut self, seed: u64) { self.pool.set_seed(seed); }

    /// Emits a burst with a random count within the effect's range
    pub fn emit(
        &mut self,
        effect: &ParticleEffect,
        position: ScenePosition,
        direction: [f32; 2],
        velocity: [f32; 2],
    ) { self.emit_scaled(effect, position, direction, velocity, 1.0); }

    /// Emits a burst with the effect's count range multiplied by `scale`
    pub fn emit_scaled(
        &mut self,
        effect: &ParticleEffect,
        position: ScenePosition,
        direction: [f32; 2],
        velocity: [f32; 2],
        scale: f32,
    ) {
        let count: u32 = self.pool.burst_count(effect, scale);
        self.emit_counted(effect, position, direction, velocity, count);
    }

    /// Emits exactly `count` particles
    pub(super) fn emit_counted(
        &mut self,
        effect: &ParticleEffect,
        position: ScenePosition,
        direction: [f32; 2],
        velocity: [f32; 2],
        count: u32,
    ) {
        if count == 0 { return; }
        if self.pool.is_empty() { self.anchor = align_to_cell(position); }
        let (x, y): (f64, f64) = position.relative_position(self.anchor);
        if x.abs().max(y.abs()) > 4.0 * REANCHOR_DISTANCE { return; }
        self.pool.spawn(effect, [x, y], direction, velocity, count);
    }

    /// Simulates, then publishes the sprites to the actor. Call once per game update.
    pub fn update(&mut self, scene: &mut Scene, elapsed: f32) {
        self.pool.step(elapsed);
        if let Some(first) = self.pool.first_position()
            && first[0].abs().max(first[1].abs()) > REANCHOR_DISTANCE
        {
            let moved: ScenePosition =
                align_to_cell(self.anchor.translate(first[0], first[1]));
            let (shift_x, shift_y): (f64, f64) = self.anchor.relative_position(moved);
            self.anchor = moved;
            self.pool.shift([shift_x, shift_y]);
        }
        self.sprites.clear();
        let (pixel, radiance_pixel) = (&self.pixel, &self.radiance_pixel);
        let sprites: &mut Vec<SpriteRecord> = &mut self.sprites;
        // every particle is drawn in whole cells on the scene's cell grid, never rotated, so its
        // pixels match the terrain's and actors' cells; a streak is a trail of single cells
        // stepped back along its velocity, fading toward the tail
        self.pool.for_each(|sample| {
            let mut push = |x: f32, y: f32, cells: f32, color: [f32; 4]| {
                sprites.push((
                    pixel.clone(),
                    radiance_pixel.clone(),
                    0,
                    x,
                    y,
                    cells,
                    cells,
                    0.0,
                    color,
                    sample.glow,
                    sample.depth,
                ));
            };
            let is_streak: bool = sample.rotation != 0.0 || sample.size[0] != sample.size[1];
            if !is_streak {
                let cells: f32 = sample.size[0].round().max(1.0);
                push(
                    snap(sample.position[0], cells as u32),
                    snap(sample.position[1], cells as u32),
                    cells,
                    sample.color,
                );
                return;
            }
            let length: u32 = (sample.size[0].round() as u32).clamp(1, STREAK_CELLS_MAXIMUM);
            let (sine, cosine): (f64, f64) = f64::from(sample.rotation).sin_cos();
            let mut previous: Option<(f32, f32)> = None;
            for index in 0..length {
                // from the head back toward the tail, one cell per step
                let back: f64 = f64::from(index) / CELLS_PER_TILE;
                let x: f32 = snap(sample.position[0] - cosine * back, 1);
                let y: f32 = snap(sample.position[1] - sine * back, 1);
                if previous == Some((x, y)) { continue; }
                previous = Some((x, y));
                let fade: f32 = 1.0 - 0.75 * index as f32 / length as f32;
                let mut color: [f32; 4] = sample.color;
                color[3] *= fade;
                push(x, y, 1.0, color);
            }
        });
        let registry: &mut ActorRegistry = scene.actor_registry_mutable();
        if let Ok(component) =
            registry.component_mutable::<ActorKinematicsPositionComponent>(self.actor)
        { component.position = self.anchor; }
        if let Ok(component) = registry.component_mutable::<ActorSpritesComponent>(self.actor) {
            std::mem::swap(&mut component.sprites, &mut self.sprites);
        }
    }

    /// Despawns the particle actor
    pub fn remove(self, scene: &mut Scene) {
        let _ = scene.actor_registry_mutable().despawn(self.actor);
    }

}
