// Copyright Rob Gage 2026

use crate::{
    materials::{
        Material,
        MaterialIdentifier,
        MaterialRegistry,
    },
    tiles::{
        CellCoordinates,
        TileCoordinates,
    },
};
use engine_compute::{
    Accelerator,
    AcceleratorBuffer,
};

const PRESSURE_DAMAGE_RATE: f32 = 10.0;

/// Applies transient directional cellular pressure and static integrity damage
pub struct CellularPressure {
    static_properties: AcceleratorBuffer,
    dynamic_properties: AcceleratorBuffer,
    pending_impulses: AcceleratorBuffer,
    pressure_a: AcceleratorBuffer,
    pressure_b: AcceleratorBuffer,
    retained_pressure: AcceleratorBuffer,
    /// Transient logical-tile mask covering current pressure sources and their stencil halo
    active_tiles: AcceleratorBuffer,
    /// Dense logical indices of the currently active pressure tiles
    active_tile_indices: AcceleratorBuffer,
    /// GPU-written indirect dispatch record for active pressure tiles
    indirect_dispatch: wgpu::Buffer,
    parameters: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    /// Compaction-only binding for the indirect dispatch record
    indirect_bind_group: wgpu::BindGroup,
    impulse_pipeline: wgpu::ComputePipeline,
    /// Clears the coarse pressure work mask at the start of each fixed tick
    clear_active_tiles_pipeline: wgpu::ComputePipeline,
    /// Finds current pressure sources and activates every tile their fixed stencil can reach
    mark_active_tiles_pipeline: wgpu::ComputePipeline,
    /// Compacts the coarse mask for indirect cell-stage dispatches
    compact_active_tiles_pipeline: wgpu::ComputePipeline,
    copy_contact_velocity_pipeline: wgpu::ComputePipeline,
    resolve_contacts_pipeline: wgpu::ComputePipeline,
    seed_pipeline: wgpu::ComputePipeline,
    propagate_a_pipeline: wgpu::ComputePipeline,
    propagate_b_pipeline: wgpu::ComputePipeline,
    apply_pipeline: wgpu::ComputePipeline,
    buffered_cell_count: u32,
    tick: u32,
}

impl CellularPressure {

    /// Returns the transient retained-pressure field for viewport visualization
    pub(crate) const fn retained_pressure(&self) -> &AcceleratorBuffer {
        &self.retained_pressure
    }

    pub fn new(
        accelerator: &Accelerator,
        materials: &MaterialRegistry,
        material_ids: &AcceleratorBuffer,
        appearances: &AcceleratorBuffer,
        integrities: &AcceleratorBuffer,
        kinematics: &AcceleratorBuffer,
        external_body_occupancy: &AcceleratorBuffer,
        external_body_velocity: &AcceleratorBuffer,
        external_body_count: &AcceleratorBuffer,
        buffered_cell_count: usize,
    ) -> Self {
        let device: &wgpu::Device = accelerator.wgpu_device();
        let static_values: Vec<[u32; 8]> = materials.iter().filter_map(
            |(_, material)| match material {
                Material::CellularStatic {
                    pressure_ignore_threshold,
                    pressure_transmission,
                    debris_material,
                    debris_yield_rate,
                    friction,
                    restitution,
                    ..
                } => Some([
                    pressure_ignore_threshold.to_bits(),
                    pressure_transmission.to_bits(),
                    debris_material.unwrap_or(MaterialIdentifier::NULL).as_u32(),
                    debris_yield_rate.to_bits(),
                    friction.to_bits(),
                    restitution.to_bits(),
                    0,
                    0,
                ]),
                _ => None,
            },
        ).collect();
        let dynamic_values: Vec<[f32; 4]> = materials.iter().filter_map(
            |(_, material)| match material {
                Material::CellularDynamic {
                    mass,
                    pressure_transmission,
                    friction,
                    restitution,
                    ..
                } => Some([
                    *mass,
                    *pressure_transmission,
                    *friction,
                    *restitution,
                ]),
                _ => None,
            },
        ).collect();
        let static_properties: AcceleratorBuffer =
            accelerator.allocate::<[u32; 8]>(static_values.len().max(1));
        let dynamic_properties: AcceleratorBuffer =
            accelerator.allocate::<[f32; 4]>(dynamic_values.len().max(1));
        if !static_values.is_empty() {
            accelerator.wgpu_queue().write_buffer(
                static_properties.wgpu_buffer(),
                0,
                &static_values.iter().flat_map(|values| {
                    values.iter().flat_map(|value| value.to_le_bytes())
                }).collect::<Vec<_>>(),
            );
        }
        if !dynamic_values.is_empty() {
            accelerator.wgpu_queue().write_buffer(
                dynamic_properties.wgpu_buffer(),
                0,
                &dynamic_values.iter().flat_map(|values| {
                    values.iter().flat_map(|value| value.to_le_bytes())
                }).collect::<Vec<_>>(),
            );
        }
        let buffered_cell_count: u32 = buffered_cell_count.try_into()
            .expect("Cellular pressure buffer exceeds GPU indexing range");
        let buffered_tile_count: u32 = buffered_cell_count / 64;
        let pending_impulses: AcceleratorBuffer =
            accelerator.allocate::<[f32; 4]>(buffered_cell_count as usize);
        let pressure_a: AcceleratorBuffer =
            accelerator.allocate::<[f32; 4]>(buffered_cell_count as usize);
        let pressure_b: AcceleratorBuffer =
            accelerator.allocate::<[f32; 4]>(buffered_cell_count as usize);
        let retained_pressure: AcceleratorBuffer =
            accelerator.allocate::<[f32; 4]>(buffered_cell_count as usize);
        let active_tiles: AcceleratorBuffer =
            accelerator.allocate::<u32>(buffered_tile_count as usize);
        let active_tile_indices: AcceleratorBuffer =
            accelerator.allocate::<u32>(buffered_tile_count as usize);
        let indirect_dispatch: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cellular pressure indirect dispatch"),
            size: 12,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::INDIRECT |
                wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let parameters: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cellular pressure parameters"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout: wgpu::BindGroupLayout = device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("cellular pressure bind group layout"),
                entries: &[
                    Self::storage_layout_entry(0, false),
                    Self::storage_layout_entry(1, false),
                    Self::storage_layout_entry(2, false),
                    Self::storage_layout_entry(3, false),
                    Self::storage_layout_entry(4, true),
                    Self::storage_layout_entry(5, true),
                    Self::storage_layout_entry(6, false),
                    Self::storage_layout_entry(7, false),
                    Self::storage_layout_entry(8, false),
                    Self::storage_layout_entry(9, false),
                    Self::storage_layout_entry(10, true),
                    Self::storage_layout_entry(11, true),
                    Self::storage_layout_entry(12, true),
                    wgpu::BindGroupLayoutEntry {
                        binding: 13,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    Self::storage_layout_entry(14, false),
                    Self::storage_layout_entry(15, false),
                ],
            },
        );
        let indirect_bind_group_layout: wgpu::BindGroupLayout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("cellular pressure indirect bind group layout"),
                entries: &[Self::storage_layout_entry(0, false)],
            });
        let bind_group: wgpu::BindGroup = device.create_bind_group(
            &wgpu::BindGroupDescriptor {
                label: Some("cellular pressure bind group"),
                layout: &layout,
                entries: &[
                    Self::binding(0, material_ids),
                    Self::binding(1, appearances),
                    Self::binding(2, integrities),
                    Self::binding(3, kinematics),
                    Self::binding(4, &static_properties),
                    Self::binding(5, &dynamic_properties),
                    Self::binding(6, &pending_impulses),
                    Self::binding(7, &pressure_a),
                    Self::binding(8, &pressure_b),
                    Self::binding(9, &retained_pressure),
                    Self::binding(10, external_body_occupancy),
                    Self::binding(11, external_body_velocity),
                    Self::binding(12, external_body_count),
                    wgpu::BindGroupEntry {
                        binding: 13,
                        resource: parameters.as_entire_binding(),
                    },
                    Self::binding(14, &active_tiles),
                    Self::binding(15, &active_tile_indices),
                ],
            },
        );
        let indirect_bind_group: wgpu::BindGroup = device.create_bind_group(
            &wgpu::BindGroupDescriptor {
                label: Some("cellular pressure indirect bind group"),
                layout: &indirect_bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: indirect_dispatch.as_entire_binding(),
                }],
            },
        );
        let shader: wgpu::ShaderModule = super::create_simulation_shader_module(
            device,
            "cellular pressure shader",
            include_str!("cellular_pressure.wgsl"),
            "engine_physics/src/simulation/cellular_pressure.wgsl",
        );
        let pipeline_layout: wgpu::PipelineLayout = device.create_pipeline_layout(
            &wgpu::PipelineLayoutDescriptor {
                label: Some("cellular pressure pipeline layout"),
                bind_group_layouts: &[Some(&layout)],
                immediate_size: 0,
            },
        );
        let compact_pipeline_layout: wgpu::PipelineLayout = device.create_pipeline_layout(
            &wgpu::PipelineLayoutDescriptor {
                label: Some("cellular pressure compaction pipeline layout"),
                bind_group_layouts: &[Some(&layout), Some(&indirect_bind_group_layout)],
                immediate_size: 0,
            },
        );
        Self {
            static_properties,
            dynamic_properties,
            pending_impulses,
            pressure_a,
            pressure_b,
            retained_pressure,
            active_tiles,
            active_tile_indices,
            indirect_dispatch,
            parameters,
            bind_group,
            indirect_bind_group,
            impulse_pipeline: Self::create_pipeline(
                device, &pipeline_layout, &shader,
                "cellular impulse pipeline", "queue_cellular_radial_impulse",
            ),
            clear_active_tiles_pipeline: Self::create_pipeline(
                device, &pipeline_layout, &shader,
                "cellular pressure active tile clear pipeline",
                "clear_active_cellular_pressure_tiles",
            ),
            mark_active_tiles_pipeline: Self::create_pipeline(
                device, &pipeline_layout, &shader,
                "cellular pressure active tile marking pipeline",
                "mark_active_cellular_pressure_tiles",
            ),
            compact_active_tiles_pipeline: Self::create_pipeline(
                device, &compact_pipeline_layout, &shader,
                "cellular pressure active tile compaction pipeline",
                "compact_active_cellular_pressure_tiles",
            ),
            copy_contact_velocity_pipeline: Self::create_pipeline(
                device, &pipeline_layout, &shader,
                "cellular contact velocity copy pipeline", "copy_cellular_contact_velocity_snapshot",
            ),
            resolve_contacts_pipeline: Self::create_pipeline(
                device, &pipeline_layout, &shader,
                "cellular contact resolution pipeline", "resolve_cellular_contacts",
            ),
            seed_pipeline: Self::create_pipeline(
                device, &pipeline_layout, &shader,
                "cellular pressure seed pipeline", "seed_cellular_pressure",
            ),
            propagate_a_pipeline: Self::create_pipeline(
                device, &pipeline_layout, &shader,
                "cellular pressure A propagation", "propagate_cellular_pressure_a",
            ),
            propagate_b_pipeline: Self::create_pipeline(
                device, &pipeline_layout, &shader,
                "cellular pressure B propagation", "propagate_cellular_pressure_b",
            ),
            apply_pipeline: Self::create_pipeline(
                device, &pipeline_layout, &shader,
                "cellular retained pressure pipeline", "apply_retained_cellular_pressure",
            ),
            buffered_cell_count,
            tick: 0,
        }
    }

    pub fn apply_radial_impulse(
        &self,
        accelerator: &Accelerator,
        origin: TileCoordinates,
        width: u16,
        height: u16,
        ring_x: u16,
        ring_y: u16,
        center: CellCoordinates,
        radius: f32,
        strength: f32,
    ) {
        self.write_parameters(
            accelerator, origin, width, height, ring_x, ring_y,
            center, radius, strength, 0.0,
        );
        self.dispatch(accelerator, &self.impulse_pipeline, "queue cellular radial impulse");
    }

    pub fn simulate(
        &mut self,
        accelerator: &Accelerator,
        origin: TileCoordinates,
        width: u16,
        height: u16,
        ring_x: u16,
        ring_y: u16,
        delta_time: f32,
    ) {
        self.write_parameters(
            accelerator, origin, width, height, ring_x, ring_y,
            CellCoordinates { x: 0, y: 0 }, 0.0, 0.0, delta_time,
        );
        let mut encoder: wgpu::CommandEncoder = accelerator.wgpu_device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("cellular pressure simulation"),
            });
        let tile_count: u32 = self.buffered_cell_count / 64;
        for (pipeline, label) in [
            (&self.clear_active_tiles_pipeline, "clear active cellular pressure tiles"),
            (&self.mark_active_tiles_pipeline, "mark active cellular pressure tiles"),
        ] {
            let mut pass: wgpu::ComputePass<'_> =
                accelerator.begin_compute_pass(&mut encoder, label);
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups(tile_count.div_ceil(64), 1, 1);
        }
        encoder.clear_buffer(&self.indirect_dispatch, 0, None);
        {
            let mut pass: wgpu::ComputePass<'_> = accelerator.begin_compute_pass(
                &mut encoder,
                "compact active cellular pressure tiles",
            );
            pass.set_pipeline(&self.compact_active_tiles_pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_bind_group(1, &self.indirect_bind_group, &[]);
            pass.dispatch_workgroups(tile_count.div_ceil(64), 1, 1);
        }
        for (pipeline, label) in [
            (&self.copy_contact_velocity_pipeline, "copy cellular contact velocities"),
            (&self.resolve_contacts_pipeline, "resolve cellular contacts"),
            (&self.seed_pipeline, "seed cellular pressure"),
            (&self.propagate_a_pipeline, "propagate cellular pressure A 1"),
            (&self.propagate_b_pipeline, "propagate cellular pressure B 1"),
            (&self.propagate_a_pipeline, "propagate cellular pressure A 2"),
            (&self.propagate_b_pipeline, "propagate cellular pressure B 2"),
            (&self.propagate_a_pipeline, "propagate cellular pressure A 3"),
            (&self.propagate_b_pipeline, "propagate cellular pressure B 3"),
            (&self.apply_pipeline, "apply retained cellular pressure"),
        ] {
            let mut pass: wgpu::ComputePass<'_> =
                accelerator.begin_compute_pass(&mut encoder, label);
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups_indirect(&self.indirect_dispatch, 0);
        }
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
        self.tick = self.tick.wrapping_add(1);
    }

    pub fn clear_transient_state(
        &self,
        accelerator: &Accelerator,
        cell_start: usize,
        cell_count: usize,
    ) {
        let zeroes: Vec<u8> = vec![0; cell_count * 16];
        let offset: u64 = cell_start as u64 * 16;
        for buffer in [
            &self.pending_impulses,
            &self.pressure_a,
            &self.pressure_b,
            &self.retained_pressure,
        ] {
            accelerator.wgpu_queue().write_buffer(buffer.wgpu_buffer(), offset, &zeroes);
        }
    }

    fn dispatch(
        &self,
        accelerator: &Accelerator,
        pipeline: &wgpu::ComputePipeline,
        label: &str,
    ) {
        let mut encoder: wgpu::CommandEncoder = accelerator.wgpu_device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some(label) });
        let mut pass: wgpu::ComputePass<'_> =
            accelerator.begin_compute_pass(&mut encoder, label);
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.dispatch_workgroups(self.buffered_cell_count.div_ceil(64), 1, 1);
        drop(pass);
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
    }

    fn write_parameters(
        &self,
        accelerator: &Accelerator,
        origin: TileCoordinates,
        width: u16,
        height: u16,
        ring_x: u16,
        ring_y: u16,
        center: CellCoordinates,
        radius: f32,
        strength: f32,
        delta_time: f32,
    ) {
        let values: [u32; 16] = [
            origin.x as u32,
            origin.y as u32,
            u32::from(width),
            u32::from(height),
            u32::from(ring_x),
            u32::from(ring_y),
            (center.x as f32 + 0.5).to_bits(),
            (center.y as f32 + 0.5).to_bits(),
            radius.to_bits(),
            strength.to_bits(),
            delta_time.to_bits(),
            self.tick,
            self.buffered_cell_count,
            PRESSURE_DAMAGE_RATE.to_bits(),
            0,
            0,
        ];
        let bytes: Vec<u8> = values.into_iter().flat_map(u32::to_le_bytes).collect();
        accelerator.wgpu_queue().write_buffer(&self.parameters, 0, &bytes);
    }

    fn binding(binding: u32, buffer: &AcceleratorBuffer) -> wgpu::BindGroupEntry<'_> {
        wgpu::BindGroupEntry {
            binding,
            resource: buffer.wgpu_buffer().as_entire_binding(),
        }
    }

    fn storage_layout_entry(binding: u32, read_only: bool) -> wgpu::BindGroupLayoutEntry {
        wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }
    }

    fn create_pipeline(
        device: &wgpu::Device,
        layout: &wgpu::PipelineLayout,
        shader: &wgpu::ShaderModule,
        label: &str,
        entry_point: &str,
    ) -> wgpu::ComputePipeline {
        device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some(label),
            layout: Some(layout),
            module: shader,
            entry_point: Some(entry_point),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        })
    }

}

impl Drop for CellularPressure {

    fn drop(&mut self) {
        self.static_properties.free();
        self.dynamic_properties.free();
        self.pending_impulses.free();
        self.pressure_a.free();
        self.pressure_b.free();
        self.retained_pressure.free();
        self.active_tiles.free();
        self.active_tile_indices.free();
        self.indirect_dispatch.destroy();
        self.parameters.destroy();
    }

}
