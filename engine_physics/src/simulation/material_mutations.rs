// Copyright Rob Gage 2026

use crate::materials::{Material, MaterialRegistry};
use engine_compute::{Accelerator, AcceleratorBuffer};

/// GPU-resident replacements requested by simulation passes.  The queue is deliberately
/// source-form agnostic so future chemistry may consume fluid and gas as well as cells.
pub struct MaterialMutations {
    requests: AcceleratorBuffer,
    request_count: AcceleratorBuffer,
    gas_fluid_candidates: AcceleratorBuffer,
    _static_defaults: AcceleratorBuffer,
    parameters: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    prepare_indirect_bind_group: wgpu::BindGroup,
    resolve_pipeline: wgpu::ComputePipeline,
    allocate_pipeline: wgpu::ComputePipeline,
    prepare_pipeline: wgpu::ComputePipeline,
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
        amounts: &AcceleratorBuffer,
        temperatures: &AcceleratorBuffer,
        fluid_edits: &AcceleratorBuffer,
        fluid_edit_amounts: &AcceleratorBuffer,
        fluid_edit_temperatures: &AcceleratorBuffer,
        fluid_edits_pending: &AcceleratorBuffer,
        gas_velocity: &AcceleratorBuffer,
        gas_concentrations: &AcceleratorBuffer,
        gas_temperatures: &AcceleratorBuffer,
        particles: &AcceleratorBuffer,
        fluid_free_indices: &AcceleratorBuffer,
        fluid_free_count: &AcceleratorBuffer,
        buffered_cell_count: usize,
        gas_count: u32,
    ) -> Self {
        let device = accelerator.wgpu_device();
        // Nine words retain the old cell replacement prefix and add an exact authority locator.
        let requests =
            accelerator.allocate::<[u32; 9]>(buffered_cell_count * (2 + gas_count as usize));
        let request_count = accelerator.allocate::<u32>(1);
        accelerator
            .wgpu_queue()
            .write_buffer(request_count.wgpu_buffer(), 0, &0u32.to_le_bytes());
        // Two branches (cold/hot), one dense slot per gas cell.  This is transient,
        // overwritten by phase_gases every tick, so it needs no clear pass.
        let gas_fluid_candidates =
            accelerator.allocate::<[u32; 6]>((buffered_cell_count * gas_count as usize * 2).max(1));
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
        let indirect = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("material mutation indirect"),
            size: 12,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::INDIRECT,
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
                storage(10, false),
                storage(11, false),
                storage(12, false),
                storage(13, false),
                storage(14, false),
                storage(15, false),
                storage(16, false),
                storage(17, false),
                storage(18, false),
                storage(19, false),
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
                    binding: 10,
                    resource: fluid_edits_pending.wgpu_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 11,
                    resource: amounts.wgpu_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 12,
                    resource: temperatures.wgpu_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 13,
                    resource: fluid_edit_amounts.wgpu_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 14,
                    resource: fluid_edit_temperatures.wgpu_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 15,
                    resource: gas_temperatures.wgpu_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 16,
                    resource: particles.wgpu_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 17,
                    resource: fluid_free_indices.wgpu_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 18,
                    resource: fluid_free_count.wgpu_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 19,
                    resource: gas_fluid_candidates.wgpu_buffer().as_entire_binding(),
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
        let prepare_indirect_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("material mutation indirect preparation layout"),
                entries: &[storage(0, false)],
            });
        let prepare_indirect_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("material mutation indirect preparation"),
            layout: &prepare_indirect_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: indirect.as_entire_binding(),
            }],
        });
        let prepare_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("material mutation preparation"),
                bind_group_layouts: &[Some(&layout), Some(&prepare_indirect_layout)],
                immediate_size: 0,
            });
        let resolve_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("resolve material mutations"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("resolve_material_mutations_nonallocating"),
            compilation_options: Default::default(),
            cache: None,
        });
        let allocate_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("aggregate gas fluid condensation"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("resolve_gas_fluid_condensation"),
            compilation_options: Default::default(),
            cache: None,
        });
        let prepare_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("prepare material mutation dispatch"),
            layout: Some(&prepare_pipeline_layout),
            module: &shader,
            entry_point: Some("prepare_material_mutation_dispatch"),
            compilation_options: Default::default(),
            cache: None,
        });
        let _ = gas_velocity;
        Self {
            requests,
            request_count,
            gas_fluid_candidates,
            _static_defaults: static_defaults,
            parameters,
            bind_group,
            prepare_indirect_bind_group,
            resolve_pipeline,
            allocate_pipeline,
            prepare_pipeline,
            indirect,
        }
    }
    pub(crate) const fn request_count_buffer(&self) -> &AcceleratorBuffer {
        &self.request_count
    }
    pub(crate) const fn requests_buffer(&self) -> &AcceleratorBuffer {
        &self.requests
    }
    pub(crate) const fn gas_fluid_candidates_buffer(&self) -> &AcceleratorBuffer {
        &self.gas_fluid_candidates
    }
    pub fn resolve(&self, accelerator: &Accelerator, buffered_cell_count: u32, gas_count: u32) {
        let mut encoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("material mutations"),
                });
        self.encode_resolve(accelerator, &mut encoder, buffered_cell_count, gas_count);
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
    }
    pub(crate) fn encode_resolve(
        &self,
        accelerator: &Accelerator,
        mut encoder: &mut wgpu::CommandEncoder,
        buffered_cell_count: u32,
        gas_count: u32,
    ) {
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
        {
            let mut pass =
                accelerator.begin_compute_pass(&mut encoder, "prepare material mutation dispatch");
            pass.set_pipeline(&self.prepare_pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_bind_group(1, &self.prepare_indirect_bind_group, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        {
            let mut pass =
                accelerator.begin_compute_pass(&mut encoder, "resolve material mutations");
            pass.set_pipeline(&self.resolve_pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups_indirect(&self.indirect, 0);
        }
        {
            let mut pass =
                accelerator.begin_compute_pass(&mut encoder, "aggregate gas fluid condensation");
            pass.set_pipeline(&self.allocate_pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups(buffered_cell_count.div_ceil(64), gas_count, 2);
        }
        encoder.clear_buffer(self.request_count.wgpu_buffer(), 0, None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::materials::MaterialIdentifier;
    use crate::simulation::Fluids;
    use engine_graphics::{Color, MaterialAppearance};
    use std::{
        sync::mpsc,
        time::{Duration, Instant},
    };

    fn read_u32(accelerator: &Accelerator, source: &AcceleratorBuffer, count: u64) -> Vec<u32> {
        let buffer = accelerator
            .wgpu_device()
            .create_buffer(&wgpu::BufferDescriptor {
                label: Some("material mutation test readback"),
                size: count * 4,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
        let mut encoder = accelerator
            .wgpu_device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        encoder.copy_buffer_to_buffer(source.wgpu_buffer(), 0, &buffer, 0, count * 4);
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
        let (sender, receiver) = mpsc::sync_channel(1);
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                sender.send(result).unwrap();
            });
        let start = Instant::now();
        loop {
            accelerator.poll().unwrap();
            if let Ok(result) = receiver.try_recv() {
                result.unwrap();
                break;
            }
            assert!(start.elapsed() < Duration::from_secs(10));
            std::thread::yield_now();
        }
        let result = buffer
            .slice(..)
            .get_mapped_range()
            .unwrap()
            .chunks_exact(4)
            .map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()))
            .collect();
        buffer.unmap();
        result
    }

    #[test]
    fn resolver_pipeline_compiles() {
        let _gpu_test = crate::GPU_TEST_LOCK.lock().unwrap();
        let accelerator = Accelerator::new().unwrap();
        let cells = accelerator.allocate::<u32>(64);
        let appearances = accelerator.allocate::<u32>(64);
        let integrities = accelerator.allocate::<f32>(64);
        let kinematics = accelerator.allocate::<[f32; 4]>(64);
        let amounts = accelerator.allocate::<f32>(64);
        let temperatures = accelerator.allocate::<f32>(64);
        let fluid_edits = accelerator.allocate::<u32>(64);
        let fluid_edit_amounts = accelerator.allocate::<f32>(64);
        let fluid_edit_temperatures = accelerator.allocate::<f32>(64);
        let gas_velocity = accelerator.allocate::<[f32; 2]>(64);
        let gas_concentrations = accelerator.allocate::<f32>(64);
        let mutations = MaterialMutations::new(
            &accelerator,
            &MaterialRegistry::new(),
            &cells,
            &appearances,
            &integrities,
            &kinematics,
            &amounts,
            &temperatures,
            &fluid_edits,
            &fluid_edit_amounts,
            &fluid_edit_temperatures,
            &accelerator.allocate::<u32>(1),
            &gas_velocity,
            &gas_concentrations,
            &accelerator.allocate::<f32>(64),
            &accelerator.allocate::<[u32; 10]>(64),
            &accelerator.allocate::<u32>(64),
            &accelerator.allocate::<u32>(1),
            64,
            0,
        );
        mutations.resolve(&accelerator, 64, 0);
        accelerator.poll().unwrap();
    }

    #[test]
    fn indirect_resolver_replaces_and_deletes_cells() {
        let _gpu_test = crate::GPU_TEST_LOCK.lock().unwrap();
        let accelerator = Accelerator::new().unwrap();
        let mut materials = MaterialRegistry::new();
        let static_material = materials.register(Material::CellularStatic {
            name: "static".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(1, 1, 1)),
            mass: 1.0,
            pressure_ignore_threshold: 1.0,
            default_integrity: 2.0,
            minimum_rigid_body_cell_count: 1,
            debris_material: None,
            debris_yield_rate: 0.0,
            pressure_transmission: 0.5,
            friction: 0.5,
            restitution: 0.0,
        });
        let dynamic_material = materials.register(Material::CellularDynamic {
            name: "dynamic".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(2, 2, 2)),
            mass: 1.0,
            pressure_transmission: 0.5,
            friction: 0.5,
            restitution: 0.0,
        });
        let cells = accelerator.allocate::<u32>(64);
        let appearances = accelerator.allocate::<u32>(64);
        let integrities = accelerator.allocate::<f32>(64);
        let kinematics = accelerator.allocate::<[f32; 4]>(64);
        let amounts = accelerator.allocate::<f32>(64);
        let temperatures = accelerator.allocate::<f32>(64);
        let fluid_edits = accelerator.allocate::<u32>(64);
        let fluid_edit_amounts = accelerator.allocate::<f32>(64);
        let fluid_edit_temperatures = accelerator.allocate::<f32>(64);
        let fluid_pending = accelerator.allocate::<u32>(1);
        let gas_velocity = accelerator.allocate::<[f32; 2]>(64);
        let gas_concentrations = accelerator.allocate::<f32>(64);
        let mutations = MaterialMutations::new(
            &accelerator,
            &materials,
            &cells,
            &appearances,
            &integrities,
            &kinematics,
            &amounts,
            &temperatures,
            &fluid_edits,
            &fluid_edit_amounts,
            &fluid_edit_temperatures,
            &fluid_pending,
            &gas_velocity,
            &gas_concentrations,
            &accelerator.allocate::<f32>(64),
            &accelerator.allocate::<[u32; 10]>(64),
            &accelerator.allocate::<u32>(64),
            &accelerator.allocate::<u32>(1),
            64,
            0,
        );
        accelerator.wgpu_queue().write_buffer(
            cells.wgpu_buffer(),
            0,
            &[
                static_material.as_u32().to_le_bytes(),
                static_material.as_u32().to_le_bytes(),
            ]
            .concat(),
        );
        accelerator.wgpu_queue().write_buffer(
            amounts.wgpu_buffer(),
            0,
            &[
                0.37f32.to_bits().to_le_bytes(),
                0.8f32.to_bits().to_le_bytes(),
            ]
            .concat(),
        );
        accelerator.wgpu_queue().write_buffer(
            temperatures.wgpu_buffer(),
            0,
            &[
                777.0f32.to_bits().to_le_bytes(),
                555.0f32.to_bits().to_le_bytes(),
            ]
            .concat(),
        );
        let requests = [
            [
                0u32,
                0,
                0,
                static_material.as_u32(),
                dynamic_material.as_u32(),
                0,
                0,
                0,
                0,
            ],
            [
                1u32,
                0,
                1,
                static_material.as_u32(),
                MaterialIdentifier::NULL.as_u32(),
                0,
                0,
                0,
                0,
            ],
        ];
        accelerator.wgpu_queue().write_buffer(
            mutations.requests_buffer().wgpu_buffer(),
            0,
            &requests
                .iter()
                .flat_map(|request| request.iter().flat_map(|word| word.to_le_bytes()))
                .collect::<Vec<_>>(),
        );
        accelerator.wgpu_queue().write_buffer(
            mutations.request_count_buffer().wgpu_buffer(),
            0,
            &2u32.to_le_bytes(),
        );
        mutations.resolve(&accelerator, 64, 0);
        assert_eq!(
            read_u32(&accelerator, &cells, 2),
            vec![dynamic_material.as_u32(), 0]
        );
        assert_eq!(
            read_u32(&accelerator, &fluid_edits, 2),
            vec![Fluids::erase_edit(), Fluids::erase_edit()]
        );
        assert_eq!(
            read_u32(&accelerator, &amounts, 2),
            vec![0.37f32.to_bits(), 0]
        );
        assert_eq!(
            read_u32(&accelerator, &temperatures, 2),
            vec![777.0f32.to_bits(), 0]
        );
    }

    #[test]
    fn gas_condensation_aggregates_a_tile_into_unit_particles() {
        let _gpu_test = crate::GPU_TEST_LOCK.lock().unwrap();
        let accelerator = Accelerator::new().unwrap();
        let cells = accelerator.allocate::<u32>(64);
        let appearances = accelerator.allocate::<u32>(64);
        let integrities = accelerator.allocate::<f32>(64);
        let kinematics = accelerator.allocate::<[f32; 4]>(64);
        let amounts = accelerator.allocate::<f32>(64);
        let temperatures = accelerator.allocate::<f32>(64);
        let fluid_edits = accelerator.allocate::<u32>(64);
        let fluid_edit_amounts = accelerator.allocate::<f32>(64);
        let fluid_edit_temperatures = accelerator.allocate::<f32>(64);
        let gas_concentrations = accelerator.allocate::<f32>(64);
        let particles = accelerator.allocate::<[u32; 10]>(4);
        let free_indices = accelerator.allocate::<u32>(4);
        let free_count = accelerator.allocate::<u32>(1);
        let mutations = MaterialMutations::new(
            &accelerator,
            &MaterialRegistry::new(),
            &cells,
            &appearances,
            &integrities,
            &kinematics,
            &amounts,
            &temperatures,
            &fluid_edits,
            &fluid_edit_amounts,
            &fluid_edit_temperatures,
            &accelerator.allocate::<u32>(1),
            &accelerator.allocate::<[f32; 2]>(64),
            &gas_concentrations,
            &accelerator.allocate::<f32>(64),
            &particles,
            &free_indices,
            &free_count,
            64,
            1,
        );
        accelerator.wgpu_queue().write_buffer(
            gas_concentrations.wgpu_buffer(),
            0,
            &vec![0.0625f32.to_bits().to_le_bytes(); 64].concat(),
        );
        accelerator.wgpu_queue().write_buffer(
            free_indices.wgpu_buffer(),
            0,
            &[0u32, 1, 2, 3]
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect::<Vec<_>>(),
        );
        accelerator
            .wgpu_queue()
            .write_buffer(free_count.wgpu_buffer(), 0, &4u32.to_le_bytes());
        let fluid = MaterialIdentifier::new(crate::materials::MaterialForm::Fluid, 0).as_u32();
        for tick in 1..=4 {
            let gas = read_u32(&accelerator, &gas_concentrations, 64);
            let candidates: Vec<[u32; 6]> = gas
                .iter()
                .enumerate()
                .map(|(cell, amount)| {
                    [
                        fluid,
                        *amount,
                        300.0f32.to_bits(),
                        0,
                        (cell as f32).to_bits(),
                        0.5f32.to_bits(),
                    ]
                })
                .collect();
            accelerator.wgpu_queue().write_buffer(
                mutations.gas_fluid_candidates_buffer().wgpu_buffer(),
                0,
                &candidates
                    .iter()
                    .flat_map(|candidate| candidate.iter().flat_map(|word| word.to_le_bytes()))
                    .collect::<Vec<_>>(),
            );
            mutations.resolve(&accelerator, 64, 1);
            let particle_words = read_u32(&accelerator, &particles, 40);
            assert_eq!(
                particle_words
                    .chunks_exact(10)
                    .filter(|particle| particle[1] != 0)
                    .count(),
                tick
            );
            assert!(
                particle_words
                    .chunks_exact(10)
                    .filter(|particle| particle[1] != 0)
                    .all(|particle| particle[8] == 1.0f32.to_bits())
            );
            let remaining: f32 = read_u32(&accelerator, &gas_concentrations, 64)
                .into_iter()
                .map(f32::from_bits)
                .sum();
            assert!((remaining - (4 - tick) as f32).abs() < 0.001);
        }
    }
}
