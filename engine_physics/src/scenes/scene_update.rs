// Copyright Rob Gage 2026

use super::*;

impl Scene {
    /// Handles Scene streaming and returns the number of completed fixed-rate ticks
    pub fn update(
        &mut self,
        elapsed: Duration,
        is_simulation_active: bool,
    ) -> Result<u32, io::Error> {
        if let Some(origin_target) = self.area_request.take() {
            self.origin_target = origin_target;
        } else {
            let possessed_position: Option<ScenePosition> = self
                .possessed_actor()
                .and_then(|actor| self.actor_registry.get_position(actor))
                .copied();
            if let Some(position) = possessed_position {
                self.follow_position(position);
            }
        }
        self.rigid_streaming_apply_completed()?;
        self.chunks_refresh()?;
        self.tile_downloads_submit()?;
        self.tile_uploads_submit()?;
        self.fluid_downloads_submit()?;
        self.fluid_uploads_submit()?;
        self.gas_downloads_submit()?;
        self.restore_ready_rigids()?;
        if !self.pending_runtime_edits.is_empty() {
            let mut edits = SceneEditBatch::new();
            std::mem::swap(&mut edits, &mut self.pending_runtime_edits);
            self.apply_edits_immediate(&mut edits)?;
            self.pending_runtime_edits.append(edits);
        }
        self.accelerator
            .poll()
            .map_err(|error| io::Error::other(error.to_string()))?;
        self.apply_completed_rigid_thermal_transitions();
        self.apply_completed_rigid_cellular_reactions()?;
        self.rigid_dormancy_apply_completed()?;
        self.cellular_collision.collect_collision()?;
        self.tile_downloads_apply_completed()?;
        self.fluid_downloads_apply_completed()?;
        self.fluid_uploads_apply_completed()?;
        self.gas_downloads_apply_completed()?;
        self.fluid_sample_apply_completed()?;
        self.tile_downloads_clean()?;
        self.tile_uploads_clean()?;
        self.tick_time += elapsed;
        let tick_time: Duration = Duration::from_secs(1) / TICK_RATE;
        self.tick_time = self
            .tick_time
            .min(tick_time.saturating_mul(MAX_CATCH_UP_TICKS));
        let mut ticks: u32 = 0;
        while self.tick_time >= tick_time && ticks < MAX_CATCH_UP_TICKS {
            // A catch-up update may submit several fixed ticks. Give tiny reaction
            // readbacks a nonblocking chance to complete between them so each
            // reaction is applied as its own fixed-tick batch.
            self.accelerator
                .poll()
                .map_err(|error| io::Error::other(error.to_string()))?;
            self.apply_completed_rigid_cellular_reactions()?;
            self.apply_completed_static_detachment()?;
            self.tick(is_simulation_active)?;
            self.tick_time -= tick_time;
            ticks = ticks.saturating_add(1);
        }
        if ticks == MAX_CATCH_UP_TICKS && self.tick_time >= tick_time {
            self.tick_time = tick_time.saturating_sub(Duration::from_nanos(1));
        }
        Ok(ticks)
    }

    /// Returns progress from the previous fixed tick to the current fixed tick
    pub(super) fn tick_interpolation(&self) -> f32 {
        (self.tick_time.as_secs_f32() * TICK_RATE as f32).clamp(0.0, 1.0)
    }
}
