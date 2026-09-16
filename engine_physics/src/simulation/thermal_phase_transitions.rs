use engine_compute::{Accelerator, AcceleratorBuffer};

/// Evaluates declarative phase metadata after scatter; mutation application remains shared.
pub(crate) struct ThermalPhaseTransitions {
    parameters: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    cells: wgpu::ComputePipeline,
    particles: wgpu::ComputePipeline,
    gases: wgpu::ComputePipeline,
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
    ) {
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
            0,
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
        for (pipeline, count) in [
            (&self.cells, self.cell_count),
            (&self.particles, self.particle_count),
            (&self.gases, self.cell_count * self.gas_count),
        ] {
            pass.set_pipeline(pipeline);
            pass.dispatch_workgroups(count.div_ceil(64), 1, 1);
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
        cell_count: u32,
        particle_count: u32,
        gas_count: u32,
        rigid_claims: &AcceleratorBuffer,
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
            cell_count,
            particle_count,
            gas_count,
            tick: 0,
        }
    }
    pub(crate) fn evaluate(
        &mut self,
        accelerator: &Accelerator,
        origin: [i32; 2],
        tiles: [u32; 2],
        ring: [u32; 2],
    ) {
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
            0,
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
        p.dispatch_workgroups((self.cell_count * self.gas_count).div_ceil(64), 1, 1);
        drop(p);
        accelerator.wgpu_queue().submit(Some(e.finish()));
    }
}
impl Drop for ThermalPhaseTransitions {
    fn drop(&mut self) {
        self.parameters.destroy();
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
