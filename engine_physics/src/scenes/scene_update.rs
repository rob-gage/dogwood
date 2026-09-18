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
            // a catch-up update may submit several fixed ticks. Give tiny reaction
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
    /// Runs one fixed-rate physics simulation tick
    fn tick(&mut self, is_simulation_active: bool) -> Result<(), io::Error> {
        if let Some(mut snapshot) = self.cellular_collision.latest.take() {
            let age = self.cellular_collision.snapshot_age(snapshot.sequence);
            self.physics_world.set_collision_snapshot_age(age);
            self.detach_unanchored_static_components(&mut snapshot)?;
            let collision_matches_current_ring = snapshot.origin == self.area_buffered().origin();
            let snapshot_origin = snapshot.origin;
            self.physics_world.update_cellular_snapshot(snapshot);
            if collision_matches_current_ring {
                self.rigid_activation_collision_origin = Some(snapshot_origin);
            }
        }
        let delta_time: f32 = 1.0 / TICK_RATE as f32;
        let actor_proxies = self.actor_registry.cellular_proxy_states();
        if is_simulation_active {
            let up = if self.gravity[0].hypot(self.gravity[1]) > 0.0 {
                Vector::new(-self.gravity[0], -self.gravity[1]).normalize()
            } else {
                Vector::Y
            };
            self.physics_world
                .sync_pawn_proxies(&self.actor_registry.physics_proxy_states(), up);
            self.physics_world.prepare_cellular_terrain(
                &self.rigid_cellular_bodies,
                &actor_proxies,
                self.gravity,
                delta_time,
            );
            // `prepare_cellular_terrain` has now rebuilt the actual Rapier
            // terrain colliders from the matching snapshot.  Only then can a
            // staged body take part in this step.
            if self.rigid_activation_collision_origin == Some(self.area_buffered().origin()) {
                for body in &self.rigid_cellular_bodies {
                    if self.rigid_activation_pending.remove(&body.identifier) {
                        self.physics_world
                            .set_rigid_cellular_body_enabled(body, true);
                        if self.rigid_sleeping_pending.remove(&body.identifier) {
                            self.physics_world.sleep_rigid_cellular_body(body);
                        }
                    }
                }
            }
            self.physics_world.step(self.gravity, delta_time);
        }
        self.actor_registry.simulate_actor_pawns(
            1.0 / TICK_RATE as f32,
            is_simulation_active,
            self.gravity,
            &self.physics_world,
        );
        let up = if self.gravity[0].hypot(self.gravity[1]) > 0.0 {
            Vector::new(-self.gravity[0], -self.gravity[1]).normalize()
        } else {
            Vector::Y
        };
        self.physics_world
            .sync_pawn_proxies(&self.actor_registry.physics_proxy_states(), up);
        let actor_proxies = self.actor_registry.cellular_proxy_states();
        let possessed_position: Option<ScenePosition> = self
            .possessed_actor()
            .and_then(|actor| self.actor_registry.get_position(actor))
            .copied();
        if let Some(position) = possessed_position {
            self.follow_position(position);
        }
        let buffer_size: i32 = i32::from(self.simulation_buffer_size);
        let dimensions: u16 = u16::from(self.simulation_buffer_size) * 2;
        let rigid_body_states: Vec<RigidCellularBodyState> = self
            .rigid_cellular_bodies
            .iter()
            .map(|body| {
                self.physics_world
                    .rigid_cellular_body_state(body)
                    .ok_or_else(|| io::Error::other("Rigid cellular body handle is missing"))
            })
            .collect::<Result<_, _>>()?;
        self.cellular_physics_body_proxy.rasterize(
            self.accelerator.as_ref(),
            TileCoordinates {
                x: self.origin.x - buffer_size,
                y: self.origin.y - buffer_size,
            },
            self.simulation_width + dimensions,
            self.simulation_height + dimensions,
            self.tiles_ring_offset_x,
            self.tiles_ring_offset_y,
            self.gravity,
            &actor_proxies,
            &self.rigid_cellular_bodies,
            &rigid_body_states,
            self.rigid_cellular_topology_revision,
        );
        if is_simulation_active {
            let fluid_active_area: TileArea = self.area_fluid_active();
            let fluid_active_dimensions: [u16; 2] = fluid_active_area.dimensions();
            self.fluids.simulate(
                self.accelerator.as_ref(),
                fluid_active_area.origin(),
                fluid_active_dimensions[0],
                fluid_active_dimensions[1],
                TileCoordinates {
                    x: self.origin.x - buffer_size,
                    y: self.origin.y - buffer_size,
                },
                self.simulation_width + dimensions,
                self.simulation_height + dimensions,
                self.tiles_ring_offset_x,
                self.tiles_ring_offset_y,
                self.gravity,
                1.0 / TICK_RATE as f32,
            );
            self.gases.simulate_pre_coupling(
                self.accelerator.as_ref(),
                TileCoordinates {
                    x: self.origin.x - buffer_size,
                    y: self.origin.y - buffer_size,
                },
                self.simulation_width + dimensions,
                self.simulation_height + dimensions,
                self.tiles_ring_offset_x,
                self.tiles_ring_offset_y,
                self.gravity,
                1.0 / TICK_RATE as f32,
            );
            // chemistry discovery observes the post-advection material snapshot
            // and the prior resolved pressure field. Its outputs are applied in
            // later stages, never recursively during this discovery pass.
            {
                let mut encoder = self.accelerator.wgpu_device().create_command_encoder(
                    &wgpu::CommandEncoderDescriptor {
                        label: Some("material reaction discovery"),
                    },
                );
                self.material_reactions
                    .encode(self.accelerator.as_ref(), &mut encoder);
                // resolve chemistry's authority-addressed cell mutations before
                // pressure observes this tick's post-reaction material snapshot.
                self.material_mutations.encode_requests(
                    self.accelerator.as_ref(),
                    &mut encoder,
                    u32::from(self.simulation_width + dimensions)
                        * u32::from(self.simulation_height + dimensions)
                        * 64,
                    self.gases.gas_count(),
                );
                self.accelerator.wgpu_queue().submit(Some(encoder.finish()));
                self.material_reactions
                    .submit_rigid_removal_readback(self.accelerator.as_ref());
            }
            self.cellular_pressure.simulate(
                self.accelerator.as_ref(),
                TileCoordinates {
                    x: self.origin.x - buffer_size,
                    y: self.origin.y - buffer_size,
                },
                self.simulation_width + dimensions,
                self.simulation_height + dimensions,
                self.tiles_ring_offset_x,
                self.tiles_ring_offset_y,
                1.0 / TICK_RATE as f32,
                self.gravity,
                self.rigid_cellular_bodies.len(),
                self.cellular_physics_body_proxy
                    .rigid_cell_count(&self.rigid_cellular_bodies),
                self.rigid_cellular_topology_revision,
            )?;
            self.material_mutations.resolve_requests(
                self.accelerator.as_ref(),
                u32::from(self.simulation_width + dimensions)
                    * u32::from(self.simulation_height + dimensions)
                    * 64,
                self.gases.gas_count(),
            );
            self.fluids.consume_accelerator_edits(
                self.accelerator.as_ref(),
                fluid_active_area.origin(),
                fluid_active_dimensions[0],
                fluid_active_dimensions[1],
                TileCoordinates {
                    x: self.origin.x - buffer_size,
                    y: self.origin.y - buffer_size,
                },
                self.simulation_width + dimensions,
                self.simulation_height + dimensions,
                self.tiles_ring_offset_x,
                self.tiles_ring_offset_y,
            );
            self.cellular_dynamic.simulate_cellular_dynamic_tick(
                self.accelerator.as_ref(),
                &self.cellular_material_identifiers,
                &self.cellular_appearances,
                &self.cellular_amounts,
                &self.cellular_temperatures,
                TileCoordinates {
                    x: self.origin.x - buffer_size,
                    y: self.origin.y - buffer_size,
                },
                self.simulation_width + dimensions,
                self.simulation_height + dimensions,
                self.tiles_ring_offset_x,
                self.tiles_ring_offset_y,
                self.gravity,
                1.0 / TICK_RATE as f32,
            );
            self.fluids
                .scatter_mechanical_response(self.accelerator.as_ref());
            self.gases.simulate_post_coupling(self.accelerator.as_ref());
            let thermal_origin = [
                self.origin.x - i32::from(self.simulation_buffer_size),
                self.origin.y - i32::from(self.simulation_buffer_size),
            ];
            let thermal_tiles = [
                u32::from(self.simulation_width + u16::from(self.simulation_buffer_size) * 2),
                u32::from(self.simulation_height + u16::from(self.simulation_buffer_size) * 2),
            ];
            let thermal_ring = [
                u32::from(self.tiles_ring_offset_x),
                u32::from(self.tiles_ring_offset_y),
            ];
            let mut thermal_encoder = self.accelerator.wgpu_device().create_command_encoder(
                &wgpu::CommandEncoderDescriptor {
                    label: Some("thermal pipeline"),
                },
            );
            self.thermal_interaction.encode(
                self.accelerator.as_ref(),
                &mut thermal_encoder,
                thermal_origin,
                thermal_tiles,
                thermal_ring,
                !self.rigid_cellular_bodies.is_empty(),
            );
            self.thermal_conduction.encode(
                self.accelerator.as_ref(),
                &mut thermal_encoder,
                thermal_origin,
                thermal_tiles,
                thermal_ring,
                1.0 / TICK_RATE as f32,
            );
            self.thermal_scatter.encode(
                self.accelerator.as_ref(),
                &mut thermal_encoder,
                thermal_origin,
                thermal_tiles,
                thermal_ring,
                !self.rigid_cellular_bodies.is_empty(),
            );
            self.thermal_phase_transitions.encode(
                self.accelerator.as_ref(),
                &mut thermal_encoder,
                thermal_origin,
                [
                    u32::from(self.simulation_width + dimensions),
                    u32::from(self.simulation_height + dimensions),
                ],
                thermal_ring,
                self.cellular_physics_body_proxy
                    .rigid_cell_count(&self.rigid_cellular_bodies) as u32,
            );
            self.material_mutations.encode_requests(
                self.accelerator.as_ref(),
                &mut thermal_encoder,
                u32::from(self.simulation_width + dimensions)
                    * u32::from(self.simulation_height + dimensions)
                    * 64,
                self.gases.gas_count(),
            );
            self.material_mutations.encode_thermal_condensation(
                self.accelerator.as_ref(),
                &mut thermal_encoder,
                u32::from(self.simulation_width + dimensions)
                    * u32::from(self.simulation_height + dimensions)
                    * 64,
                self.gases.gas_count(),
            );
            self.accelerator
                .wgpu_queue()
                .submit(Some(thermal_encoder.finish()));
            self.thermal_phase_transitions.submit_rigid_readback(
                self.accelerator.as_ref(),
                self.cellular_physics_body_proxy
                    .rigid_cell_count(&self.rigid_cellular_bodies) as u32,
            );
            self.fluid_sample_submit()?;
            self.cellular_collision_dirty = true;
        }
        if !self.cellular_collision_dirty {
            return Ok(());
        }
        let buffer_size: i32 = i32::from(self.simulation_buffer_size);
        if self.cellular_collision.extract_collision(
            self.accelerator.as_ref(),
            TileCoordinates {
                x: self.origin.x - buffer_size,
                y: self.origin.y - buffer_size,
            },
            self.simulation_width + u16::from(self.simulation_buffer_size) * 2,
            self.simulation_height + u16::from(self.simulation_buffer_size) * 2,
            self.tiles_ring_offset_x,
            self.tiles_ring_offset_y,
        )? {
            self.cellular_collision_dirty = false;
        }
        Ok(())
    }
}
