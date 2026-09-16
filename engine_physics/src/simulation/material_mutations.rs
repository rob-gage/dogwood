// Copyright Rob Gage 2026

use crate::materials::{Material, MaterialRegistry};
use engine_compute::{Accelerator, AcceleratorBuffer};

/// GPU-resident replacements requested by simulation passes.  The queue is deliberately
/// source-form agnostic so future chemistry may consume fluid and gas as well as cells.
pub struct MaterialMutations {
    requests: AcceleratorBuffer,
    request_count: AcceleratorBuffer,
    _static_defaults: AcceleratorBuffer,
    parameters: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    resolve_pipeline: wgpu::ComputePipeline,
    indirect: wgpu::Buffer,
}

impl MaterialMutations {
    pub fn new(
        accelerator: &Accelerator,
        materials: &MaterialRegistry,
        material_ids: &AcceleratorBuffer,
        appearances: &AcceleratorBuffer,
        integrities: &AcceleratorBuffer,
        kinematics: &AcceleratorBuffer,
        fluid_edits: &AcceleratorBuffer,
        gas_velocity: &AcceleratorBuffer,
        gas_concentrations: &AcceleratorBuffer,
        buffered_cell_count: usize,
        _gas_count: u32,
    ) -> Self {
        let device = accelerator.wgpu_device();
        let requests = accelerator.allocate::<[u32; 4]>(buffered_cell_count);
        let request_count = accelerator.allocate::<u32>(1);
        let defaults: Vec<u32> = materials
            .iter()
            .filter_map(|(_, m)| match m {
                Material::CellularStatic {
                    default_integrity, ..
                } => Some(default_integrity.to_bits()),
                _ => None,
            })
            .collect();
        let static_defaults = accelerator.allocate::<u32>(defaults.len().max(1));
        if !defaults.is_empty() {
            accelerator.wgpu_queue().write_buffer(
                static_defaults.wgpu_buffer(),
                0,
                &defaults
                    .iter()
                    .flat_map(|v| v.to_le_bytes())
                    .collect::<Vec<_>>(),
            );
        }
        let parameters = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("material mutation parameters"),
            size: 16,
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
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("material mutation layout"),
            entries: &[
                storage(0, true),
                storage(1, true),
                storage(2, false),
                storage(3, false),
                storage(4, true),
                storage(5, false),
                storage(6, false),
                storage(7, false),
                storage(8, false),
                wgpu::BindGroupLayoutEntry {
                    binding: 9,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("material mutations"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: requests.wgpu_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: request_count.wgpu_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: material_ids.wgpu_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: appearances.wgpu_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: static_defaults.wgpu_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: integrities.wgpu_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: kinematics.wgpu_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: fluid_edits.wgpu_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: gas_concentrations.wgpu_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 9,
                    resource: parameters.as_entire_binding(),
                },
            ],
        });
        let shader = super::create_simulation_shader_module(
            device,
            "material mutation shader",
            include_str!("material_mutations.wgsl"),
            "engine_physics/src/simulation/material_mutations.wgsl",
        );
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("material mutations"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let resolve_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("resolve material mutations"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("resolve_material_mutations"),
            compilation_options: Default::default(),
            cache: None,
        });
        let indirect = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("material mutation indirect"),
            size: 12,
            usage: wgpu::BufferUsages::INDIRECT | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let _ = gas_velocity;
        Self {
            requests,
            request_count,
            _static_defaults: static_defaults,
            parameters,
            bind_group,
            resolve_pipeline,
            indirect,
        }
    }
    pub(crate) const fn request_count_buffer(&self) -> &AcceleratorBuffer {
        &self.request_count
    }
    pub(crate) const fn requests_buffer(&self) -> &AcceleratorBuffer {
        &self.requests
    }
    pub fn reset(&self, accelerator: &Accelerator) {
        let mut encoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("clear material mutations"),
                });
        encoder.clear_buffer(self.request_count.wgpu_buffer(), 0, None);
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
    }
    pub fn resolve(&self, accelerator: &Accelerator, buffered_cell_count: u32, gas_count: u32) {
        accelerator.wgpu_queue().write_buffer(
            &self.parameters,
            0,
            &[
                buffered_cell_count.to_le_bytes(),
                gas_count.to_le_bytes(),
                0u32.to_le_bytes(),
                0u32.to_le_bytes(),
            ]
            .concat(),
        );
        let mut encoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("material mutations"),
                });
        // A zero count produces zero work; no cell-grid scan occurs when no producer requested a mutation.
        encoder.copy_buffer_to_buffer(self.request_count.wgpu_buffer(), 0, &self.indirect, 0, 4);
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
        let mut encoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("resolve material mutations"),
                });
        {
            let mut pass =
                accelerator.begin_compute_pass(&mut encoder, "resolve material mutations");
            pass.set_pipeline(&self.resolve_pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups_indirect(&self.indirect, 0);
        }
        encoder.clear_buffer(self.request_count.wgpu_buffer(), 0, None);
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolver_pipeline_compiles() {
        let _gpu_test = crate::GPU_TEST_LOCK.lock().unwrap();
        let accelerator = Accelerator::new().unwrap();
        let cells = accelerator.allocate::<u32>(64);
        let appearances = accelerator.allocate::<u32>(64);
        let integrities = accelerator.allocate::<f32>(64);
        let kinematics = accelerator.allocate::<[f32; 4]>(64);
        let fluid_edits = accelerator.allocate::<u32>(64);
        let gas_velocity = accelerator.allocate::<[f32; 2]>(64);
        let gas_concentrations = accelerator.allocate::<f32>(64);
        let mutations = MaterialMutations::new(
            &accelerator,
            &MaterialRegistry::new(),
            &cells,
            &appearances,
            &integrities,
            &kinematics,
            &fluid_edits,
            &gas_velocity,
            &gas_concentrations,
            64,
            0,
        );
        mutations.reset(&accelerator);
        mutations.resolve(&accelerator, 64, 0);
        accelerator.poll().unwrap();
    }
}
