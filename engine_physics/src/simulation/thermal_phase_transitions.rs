use engine_compute::{Accelerator, AcceleratorBuffer};
use std::sync::mpsc::{Receiver, sync_channel};

/// Evaluates declarative phase metadata after scatter; mutation application remains shared.
pub(crate) struct ThermalPhaseTransitions {
    parameters: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    cells: wgpu::ComputePipeline,
    particles: wgpu::ComputePipeline,
    gases: wgpu::ComputePipeline,
    rigid: wgpu::ComputePipeline,
    rollback: wgpu::ComputePipeline,
    rigid_phase_candidates: AcceleratorBuffer,
    rigid_phase_count: AcceleratorBuffer,
    rigid_phase_readback: wgpu::Buffer,
    rigid_phase_readback_result: Option<Receiver<Result<(), wgpu::BufferAsyncError>>>,
    rollback_slots: AcceleratorBuffer,
    rollback_count: AcceleratorBuffer,
    cell_count: u32,
    particle_count: u32,
    gas_count: u32,
    tick: u32,
}
impl ThermalPhaseTransitions {
    pub(crate) fn encode(
        &mut self,
        accelerator: &Accelerator,
        encoder: &mut wgpu::CommandEncoder,
        origin: [i32; 2],
        tiles: [u32; 2],
        ring: [u32; 2],
        rigid_count: u32,
    ) {
        if self.rigid_phase_readback_result.is_none() {
            self.clear_rigid_candidates(accelerator);
        }
        let v = [
            origin[0] as u32,
            origin[1] as u32,
            tiles[0],
            tiles[1],
            ring[0],
            ring[1],
            self.cell_count,
            self.particle_count,
            self.gas_count,
            self.tick,
            rigid_count,
            0,
        ];
        accelerator.wgpu_queue().write_buffer(
            &self.parameters,
            0,
            &v.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<_>>(),
        );
        self.tick = self.tick.wrapping_add(1);
        let mut pass = accelerator.begin_compute_pass(encoder, "thermal phase transitions");
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_pipeline(&self.cells);
        pass.dispatch_workgroups(self.cell_count.div_ceil(64), 1, 1);
        pass.set_pipeline(&self.particles);
        pass.dispatch_workgroups(self.particle_count.div_ceil(64), 1, 1);
        pass.set_pipeline(&self.gases);
        pass.dispatch_workgroups(self.cell_count / 64, self.gas_count, 2);
        if self.rigid_phase_readback_result.is_none() {
            pass.set_pipeline(&self.rigid);
            pass.dispatch_workgroups(self.cell_count.div_ceil(64), 1, 1);
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        accelerator: &Accelerator,
        cells: &AcceleratorBuffer,
        amounts: &AcceleratorBuffer,
        temperatures: &AcceleratorBuffer,
        particles: &AcceleratorBuffer,
        concentrations: &AcceleratorBuffer,
        gas_temperatures: &AcceleratorBuffer,
        properties: &AcceleratorBuffer,
        thermal: &wgpu::Buffer,
        requests: &AcceleratorBuffer,
        request_count: &AcceleratorBuffer,
        gas_fluid_candidates: &AcceleratorBuffer,
        cell_count: u32,
        particle_count: u32,
        gas_count: u32,
        rigid_claims: &AcceleratorBuffer,
        rigid_cells: &AcceleratorBuffer,
        rigid_amounts: &AcceleratorBuffer,
        rigid_temperatures: &AcceleratorBuffer,
        fluid_free_indices: &AcceleratorBuffer,
        fluid_free_count: &AcceleratorBuffer,
    ) -> Self {
        let d = accelerator.wgpu_device();
        let parameters = d.create_buffer(&wgpu::BufferDescriptor {
            label: Some("thermal phase parameters"),
            size: 48,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let storage = |binding, read_only| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let mut entries: Vec<_> = (0..10)
            .filter(|b| *b != 7)
            .map(|b| storage(b, !matches!(b, 8 | 9)))
            .collect();
        entries.push(wgpu::BindGroupLayoutEntry {
            binding: 7,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        });
        entries.push(storage(11, true));
        entries.push(storage(12, false));
        entries.push(storage(13, true));
        entries.push(storage(14, true));
        entries.push(storage(15, true));
        entries.push(storage(16, false));
        entries.push(storage(17, false));
        entries.push(storage(18, false));
        entries.push(storage(19, false));
        entries.push(storage(20, true));
        entries.push(storage(21, true));
        entries.push(wgpu::BindGroupLayoutEntry {
            binding: 10,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        });
        let layout = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("thermal phase"),
            entries: &entries,
        });
        let bs = [
            cells,
            amounts,
            temperatures,
            particles,
            concentrations,
            gas_temperatures,
            properties,
        ];
        let mut e: Vec<_> = bs
            .iter()
            .enumerate()
            .map(|(i, b)| wgpu::BindGroupEntry {
                binding: i as u32,
                resource: b.wgpu_buffer().as_entire_binding(),
            })
            .collect();
        e.push(wgpu::BindGroupEntry {
            binding: 7,
            resource: thermal.as_entire_binding(),
        });
        e.push(wgpu::BindGroupEntry {
            binding: 8,
            resource: requests.wgpu_buffer().as_entire_binding(),
        });
        e.push(wgpu::BindGroupEntry {
            binding: 9,
            resource: request_count.wgpu_buffer().as_entire_binding(),
        });
        e.push(wgpu::BindGroupEntry {
            binding: 10,
            resource: parameters.as_entire_binding(),
        });
        e.push(wgpu::BindGroupEntry {
            binding: 11,
            resource: rigid_claims.wgpu_buffer().as_entire_binding(),
        });
        e.push(wgpu::BindGroupEntry {
            binding: 12,
            resource: gas_fluid_candidates.wgpu_buffer().as_entire_binding(),
        });
        let rigid_phase_candidates = accelerator.allocate::<[u32; 10]>(cell_count as usize);
        let rigid_phase_count = accelerator.allocate::<u32>(1);
        let rigid_phase_readback = d.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rigid thermal phase readback"),
            size: 256 + cell_count as u64 * 40,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let rollback_slots = accelerator.allocate::<u32>(cell_count as usize);
        let rollback_count = accelerator.allocate::<u32>(1);
        accelerator.wgpu_queue().write_buffer(
            rigid_phase_count.wgpu_buffer(),
            0,
            &0u32.to_le_bytes(),
        );
        for (binding, buffer) in [
            (13, rigid_cells),
            (14, rigid_amounts),
            (15, rigid_temperatures),
            (16, &rigid_phase_candidates),
            (17, &rigid_phase_count),
        ] {
            e.push(wgpu::BindGroupEntry {
                binding,
                resource: buffer.wgpu_buffer().as_entire_binding(),
            });
        }
        e.push(wgpu::BindGroupEntry {
            binding: 18,
            resource: fluid_free_indices.wgpu_buffer().as_entire_binding(),
        });
        e.push(wgpu::BindGroupEntry {
            binding: 19,
            resource: fluid_free_count.wgpu_buffer().as_entire_binding(),
        });
        e.push(wgpu::BindGroupEntry {
            binding: 20,
            resource: rollback_slots.wgpu_buffer().as_entire_binding(),
        });
        e.push(wgpu::BindGroupEntry {
            binding: 21,
            resource: rollback_count.wgpu_buffer().as_entire_binding(),
        });
        let bind_group = d.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("thermal phase"),
            layout: &layout,
            entries: &e,
        });
        let shader = super::create_simulation_shader_module(
            d,
            "thermal phase shader",
            include_str!("thermal_phase_transitions.wgsl"),
            "engine_physics/src/simulation/thermal_phase_transitions.wgsl",
        );
        let pl = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("thermal phase"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipe = |entry| {
            d.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: Some(&pl),
                module: &shader,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        Self {
            parameters,
            bind_group,
            cells: pipe("phase_cells"),
            particles: pipe("phase_particles"),
            gases: pipe("phase_gases"),
            rigid: pipe("phase_rigid"),
            rollback: pipe("rollback_rigid_reservations"),
            rigid_phase_candidates,
            rigid_phase_count,
            rigid_phase_readback,
            rigid_phase_readback_result: None,
            rollback_slots,
            rollback_count,
            cell_count,
            particle_count,
            gas_count,
            tick: 0,
        }
    }
    pub(crate) fn clear_rigid_candidates(&self, accelerator: &Accelerator) {
        accelerator.wgpu_queue().write_buffer(
            self.rigid_phase_count.wgpu_buffer(),
            0,
            &0u32.to_le_bytes(),
        );
    }
    pub(crate) const fn rigid_phase_candidates_buffer(&self) -> &AcceleratorBuffer {
        &self.rigid_phase_candidates
    }
    pub(crate) const fn rigid_phase_count_buffer(&self) -> &AcceleratorBuffer {
        &self.rigid_phase_count
    }
    pub(crate) fn rigid_readback_pending(&self) -> bool {
        self.rigid_phase_readback_result.is_some()
    }
    pub(crate) fn submit_rigid_readback(&mut self, accelerator: &Accelerator) {
        if self.rigid_phase_readback_result.is_some() {
            return;
        }
        let mut encoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("rigid thermal phase readback"),
                });
        encoder.copy_buffer_to_buffer(
            self.rigid_phase_count.wgpu_buffer(),
            0,
            &self.rigid_phase_readback,
            0,
            4,
        );
        encoder.copy_buffer_to_buffer(
            self.rigid_phase_candidates.wgpu_buffer(),
            0,
            &self.rigid_phase_readback,
            256,
            self.cell_count as u64 * 40,
        );
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
        let (sender, receiver) = sync_channel(1);
        self.rigid_phase_readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = sender.send(result);
            });
        self.rigid_phase_readback_result = Some(receiver);
    }
    pub(crate) fn take_rigid_candidates(&mut self) -> Option<Vec<[u32; 10]>> {
        let result = self.rigid_phase_readback_result.as_ref()?.try_recv().ok()?;
        self.rigid_phase_readback_result = None;
        if result.is_err() {
            self.rigid_phase_readback.unmap();
            return Some(Vec::new());
        }
        let bytes = self
            .rigid_phase_readback
            .slice(..)
            .get_mapped_range()
            .ok()?;
        let count = u32::from_le_bytes(bytes[..4].try_into().ok()?).min(self.cell_count) as usize;
        let records = bytes[256..]
            .chunks_exact(40)
            .take(count)
            .map(|b| {
                let mut record = [0u32; 10];
                for (word, value) in record.iter_mut().zip(b.chunks_exact(4)) {
                    *word = u32::from_le_bytes(value.try_into().unwrap());
                }
                record
            })
            .collect();
        drop(bytes);
        self.rigid_phase_readback.unmap();
        Some(records)
    }
    pub(crate) fn rollback_rigid_reservations(&self, accelerator: &Accelerator, slots: &[u32]) {
        if slots.is_empty() {
            return;
        }
        let count = slots.len().min(self.cell_count as usize) as u32;
        accelerator.wgpu_queue().write_buffer(
            self.rollback_slots.wgpu_buffer(),
            0,
            &slots[..count as usize]
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect::<Vec<_>>(),
        );
        accelerator.wgpu_queue().write_buffer(
            self.rollback_count.wgpu_buffer(),
            0,
            &count.to_le_bytes(),
        );
        let mut encoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("rollback rigid thermal reservations"),
                });
        let mut pass =
            accelerator.begin_compute_pass(&mut encoder, "rollback rigid thermal reservations");
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_pipeline(&self.rollback);
        pass.dispatch_workgroups(count.div_ceil(64), 1, 1);
        drop(pass);
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
    }
    pub(crate) fn evaluate(
        &mut self,
        accelerator: &Accelerator,
        origin: [i32; 2],
        tiles: [u32; 2],
        ring: [u32; 2],
        rigid_count: u32,
    ) {
        if self.rigid_phase_readback_result.is_none() {
            self.clear_rigid_candidates(accelerator);
        }
        let v = [
            origin[0] as u32,
            origin[1] as u32,
            tiles[0],
            tiles[1],
            ring[0],
            ring[1],
            self.cell_count,
            self.particle_count,
            self.gas_count,
            self.tick,
            rigid_count,
            0,
        ];
        accelerator.wgpu_queue().write_buffer(
            &self.parameters,
            0,
            &v.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<_>>(),
        );
        self.tick = self.tick.wrapping_add(1);
        let mut e =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("thermal phase transitions"),
                });
        let mut p = accelerator.begin_compute_pass(&mut e, "thermal phase transitions");
        p.set_bind_group(0, &self.bind_group, &[]);
        p.set_pipeline(&self.cells);
        p.dispatch_workgroups(self.cell_count.div_ceil(64), 1, 1);
        p.set_pipeline(&self.particles);
        p.dispatch_workgroups(self.particle_count.div_ceil(64), 1, 1);
        p.set_pipeline(&self.gases);
        p.dispatch_workgroups(self.cell_count / 64, self.gas_count, 2);
        if self.rigid_phase_readback_result.is_none() {
            p.set_pipeline(&self.rigid);
            p.dispatch_workgroups(self.cell_count.div_ceil(64), 1, 1);
        }
        drop(p);
        accelerator.wgpu_queue().submit(Some(e.finish()));
    }
}
impl Drop for ThermalPhaseTransitions {
    fn drop(&mut self) {
        self.parameters.destroy();
        self.rigid_phase_candidates.free();
        self.rigid_phase_count.free();
        self.rigid_phase_readback.destroy();
        self.rollback_slots.free();
        self.rollback_count.free();
    }
}

#[cfg(test)]
fn transitioned_temperature(
    amount: f32,
    source_cp: f32,
    target_cp: f32,
    threshold: f32,
    latent: f32,
    temperature: f32,
    hot: bool,
) -> Option<f32> {
    let sensible = amount
        * source_cp
        * if hot {
            (temperature - threshold).max(0.0)
        } else {
            (threshold - temperature).max(0.0)
        };
    let required = amount * latent.max(0.0);
    if amount <= 0.0 || target_cp <= 0.0 || (latent > 0.0 && sensible < required) {
        return None;
    }
    Some(
        (threshold
            + if hot { 1.0 } else { -1.0 } * (sensible - required).max(0.0) / (amount * target_cp))
            .max(0.0),
    )
}

#[cfg(test)]
mod tests {
    use super::transitioned_temperature;
    #[test]
    fn latent_arithmetic_is_symmetric() {
        assert_eq!(
            transitioned_temperature(1.0, 2.0, 4.0, 10.0, 3.0, 11.0, true),
            None
        );
        assert_eq!(
            transitioned_temperature(1.0, 2.0, 4.0, 10.0, 3.0, 11.5, true),
            Some(10.0)
        );
        assert_eq!(
            transitioned_temperature(1.0, 2.0, 4.0, 10.0, 3.0, 13.5, true),
            Some(11.0)
        );
        assert_eq!(
            transitioned_temperature(1.0, 2.0, 4.0, 10.0, 3.0, 9.0, false),
            None
        );
        assert_eq!(
            transitioned_temperature(1.0, 2.0, 4.0, 10.0, 3.0, 8.5, false),
            Some(10.0)
        );
        assert_eq!(
            transitioned_temperature(1.0, 2.0, 4.0, 10.0, 3.0, 6.5, false),
            Some(9.0)
        );
        assert_eq!(
            transitioned_temperature(1.0, 2.0, 4.0, 10.0, 0.0, 11.0, true),
            Some(10.5)
        );
    }
}
