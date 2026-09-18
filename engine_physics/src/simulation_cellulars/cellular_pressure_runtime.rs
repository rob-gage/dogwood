// Copyright Rob Gage 2026

use std::sync::Arc;
use std::sync::Mutex;

use engine_compute::Accelerator;

use super::CellularPressure;
use crate::simulation::RigidGranularReadbackSlot;
use crate::simulation::RigidGranularReadbackStatus;
use crate::simulation::simulation_constants::PRESSURE_DAMAGE_RATE;
use crate::simulation::simulation_constants::RIGID_REACTION_READBACK_SLOT_COUNT;
use crate::tiles::CellCoordinates;
use crate::tiles::TileCoordinates;

impl CellularPressure {
    pub(super) fn write_parameters(
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
        gravity: [f32; 2],
        rigid_body_count: u32,
        rigid_cell_count: u32,
        impulse_min: CellCoordinates,
        impulse_size: [u32; 2],
    ) {
        let cellular_pressure_parameter_values: [u32; 24] = [
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
            self.gas_count,
            rigid_body_count,
            gravity[0].to_bits(),
            gravity[1].to_bits(),
            rigid_cell_count,
            0,
            impulse_min.x as u32,
            impulse_min.y as u32,
            impulse_size[0],
            impulse_size[1],
        ];
        let cellular_pressure_parameter_bytes: Vec<u8> = cellular_pressure_parameter_values
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect();
        accelerator.wgpu_queue().write_buffer(
            &self.cellular_pressure_parameters,
            0,
            &cellular_pressure_parameter_bytes,
        );
    }

    pub(super) fn ensure_rigid_body_capacity(&mut self, accelerator: &Accelerator, count: usize) {
        if count <= self.rigid_body_capacity {
            return;
        }
        self.rigid_contact_statistics.free();
        self.rigid_reactions.free();
        self.rigid_predicted_motion.free();
        self.rigid_body_capacity = count.next_power_of_two();
        self.rigid_contact_statistics = accelerator.allocate::<[u32; 12]>(self.rigid_body_capacity);
        self.rigid_reactions = accelerator.allocate::<[i32; 20]>(self.rigid_body_capacity);
        self.rigid_predicted_motion = accelerator.allocate::<[i32; 4]>(self.rigid_body_capacity);
        for (binding, buffer) in &mut self.bound_buffers {
            if *binding == 27 {
                *buffer = self.rigid_reactions.wgpu_buffer().clone();
            }
            if *binding == 30 {
                *buffer = self.rigid_predicted_motion.wgpu_buffer().clone();
            }
            if *binding == 28 {
                *buffer = self.rigid_contact_statistics.wgpu_buffer().clone();
            }
        }
        self.bind_group = Self::create_bind_group(
            accelerator.wgpu_device(),
            &self.bind_group_layout,
            &self.cellular_pressure_parameters,
            &self.bound_buffers,
        );
        let size: u64 =
            self.rigid_body_capacity as u64 * 128 + 4 + self.rigid_fracture_word_count * 4;
        self.rigid_reaction_readback_slots = (0..RIGID_REACTION_READBACK_SLOT_COUNT)
            .map(|_| RigidGranularReadbackSlot {
                buffer: accelerator
                    .wgpu_device()
                    .create_buffer(&wgpu::BufferDescriptor {
                        label: Some("rigid granular reaction readback"),
                        size,
                        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    }),
                status: Arc::new(Mutex::new(RigidGranularReadbackStatus::Available)),
            })
            .collect();
        self.rigid_reaction_completed.clear();
        self.rigid_reaction_sequence_apply_next = self.rigid_reaction_sequence_next;
        self.rigid_topology_revision = u64::MAX;
    }

    pub(super) fn create_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        cellular_pressure_parameters: &wgpu::Buffer,
        cellular_pressure_buffers: &[(u32, wgpu::Buffer)],
    ) -> wgpu::BindGroup {
        let mut cellular_pressure_bind_group_entries: Vec<wgpu::BindGroupEntry<'_>> =
            cellular_pressure_buffers
                .iter()
                .map(|(binding, buffer)| wgpu::BindGroupEntry {
                    binding: *binding,
                    resource: buffer.as_entire_binding(),
                })
                .collect();
        cellular_pressure_bind_group_entries.push(wgpu::BindGroupEntry {
            binding: 13,
            resource: cellular_pressure_parameters.as_entire_binding(),
        });
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cellular pressure bind group"),
            layout,
            entries: &cellular_pressure_bind_group_entries,
        })
    }

    pub(super) fn storage_layout_entry(
        binding: u32,
        read_only: bool,
    ) -> wgpu::BindGroupLayoutEntry {
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

    pub(super) fn create_pipeline(
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
