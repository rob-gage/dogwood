// Copyright Rob Gage 2026

use std::sync::mpsc::Receiver;
use std::sync::mpsc::SyncSender;
use std::sync::mpsc::sync_channel;

use engine_compute::Accelerator;
use engine_compute::AcceleratorBuffer;

use crate::materials::MaterialRegistry;
use crate::scenes::MaterialExtractionAmount;
use crate::scenes::MaterialExtractionRequest;
use crate::scenes::MaterialExtractionResult;
use crate::scenes::PendingMaterialExtraction;
use crate::tiles::TileCoordinates;

const MATERIAL_EXTRACTION_READBACK_SLOTS: usize = 4;
// Integer atomics avoid nondeterministic f32 atomics; 1/4096 amount precision
// leaves more than one million normalized units before u32 overflow.
const MATERIAL_EXTRACTION_FIXED_POINT_SCALE: f32 = 4096.0;

struct MaterialExtractionSlot {
    material_filter_mask: AcceleratorBuffer,
    material_amounts: AcceleratorBuffer,
    material_extraction_parameters: wgpu::Buffer,
    readback: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    active_request: Option<MaterialExtractionRequest>,
    readback_result: Option<Receiver<Result<(), wgpu::BufferAsyncError>>>,
}

/// Bounded asynchronous extraction dispatch and aggregate readback resources.
pub(crate) struct MaterialExtraction {
    slots: Vec<MaterialExtractionSlot>,
    extract_pipeline: wgpu::ComputePipeline,
    material_count: u32,
    buffered_cell_count: u32,
    particle_capacity: u32,
    gas_count: u32,
    material_offsets: [u32; 4],
    material_counts: [u32; 4],
}

impl MaterialExtraction {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        accelerator: &Accelerator,
        materials: &MaterialRegistry,
        cellular_material_identifiers: &AcceleratorBuffer,
        cellular_appearances: &AcceleratorBuffer,
        cellular_integrities: &AcceleratorBuffer,
        cellular_kinematics: &AcceleratorBuffer,
        cellular_amounts: &AcceleratorBuffer,
        cellular_temperatures: &AcceleratorBuffer,
        external_body_occupancy: &AcceleratorBuffer,
        particles: &AcceleratorBuffer,
        fluid_free_indices: &AcceleratorBuffer,
        fluid_free_count: &AcceleratorBuffer,
        gas_concentrations: &AcceleratorBuffer,
        rigid_cells: &AcceleratorBuffer,
        rigid_transforms: &AcceleratorBuffer,
        rigid_cell_amounts: &AcceleratorBuffer,
        buffered_cell_count: u32,
        particle_capacity: u32,
        gas_count: u32,
    ) -> Self {
        let device: &wgpu::Device = accelerator.wgpu_device();
        let material_count: u32 = materials.material_count();
        let material_mask_word_count: u32 = material_count.div_ceil(32).max(1);
        let static_count: u32 = materials
            .iter()
            .filter(|(identifier, _)| {
                identifier.form() == crate::materials::MaterialForm::CellularStatic
            })
            .count() as u32;
        let dynamic_count: u32 = materials
            .iter()
            .filter(|(identifier, _)| {
                identifier.form() == crate::materials::MaterialForm::CellularDynamic
            })
            .count() as u32;
        let fluid_count: u32 = materials
            .iter()
            .filter(|(identifier, _)| identifier.form() == crate::materials::MaterialForm::Fluid)
            .count() as u32;
        let gas_count_registered: u32 = materials
            .iter()
            .filter(|(identifier, _)| identifier.form() == crate::materials::MaterialForm::Gas)
            .count() as u32;
        let material_offsets: [u32; 4] = [
            static_count + dynamic_count + fluid_count,
            0,
            static_count,
            static_count + dynamic_count,
        ];
        let material_counts: [u32; 4] = [
            gas_count_registered,
            static_count,
            dynamic_count,
            fluid_count,
        ];
        let storage: fn(u32, bool) -> wgpu::BindGroupLayoutEntry =
            crate::simulation::storage_bind_group_layout_entry;
        let layout_entries: Vec<wgpu::BindGroupLayoutEntry> = [
            storage(0, false),
            storage(1, false),
            storage(2, false),
            storage(3, false),
            storage(4, false),
            storage(5, false),
            storage(6, true),
            storage(7, false),
            storage(8, false),
            storage(9, false),
            storage(10, false),
            storage(11, true),
            storage(12, false),
            storage(13, true),
            storage(14, true),
            storage(15, false),
            crate::simulation::uniform_bind_group_layout_entry(16),
        ]
        .into_iter()
        .collect();
        let layout: wgpu::BindGroupLayout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("material extraction layout"),
                entries: &layout_entries,
            });
        let shader: wgpu::ShaderModule = crate::simulation::create_simulation_shader_module(
            device,
            "material extraction shader",
            include_str!("material_extraction.wgsl"),
            "engine_physics/src/simulation_materials/material_extraction.wgsl",
        );
        let pipeline_layout: wgpu::PipelineLayout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("material extraction pipeline layout"),
                bind_group_layouts: &[Some(&layout)],
                immediate_size: 0,
            });
        let extract_pipeline: wgpu::ComputePipeline =
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("extract authoritative material"),
                layout: Some(&pipeline_layout),
                module: &shader,
                entry_point: Some("extract_materials"),
                compilation_options: Default::default(),
                cache: None,
            });
        let mut slots: Vec<MaterialExtractionSlot> = Vec::new();
        for _ in 0..MATERIAL_EXTRACTION_READBACK_SLOTS {
            let material_filter_mask: AcceleratorBuffer =
                accelerator.allocate::<u32>(material_mask_word_count as usize);
            let material_amounts: AcceleratorBuffer =
                accelerator.allocate::<u32>(material_count.max(1) as usize);
            let material_extraction_parameters: wgpu::Buffer =
                crate::simulation::create_simulation_uniform_buffer(
                    device,
                    "material extraction parameters",
                    96,
                );
            let readback: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("material extraction readback"),
                size: material_count.max(1) as u64 * 4,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            let bind_group: wgpu::BindGroup =
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("material extraction"),
                    layout: &layout,
                    entries: &[
                        crate::simulation::accelerator_buffer_bind_group_entry(
                            0,
                            cellular_material_identifiers,
                        ),
                        crate::simulation::accelerator_buffer_bind_group_entry(
                            1,
                            cellular_appearances,
                        ),
                        crate::simulation::accelerator_buffer_bind_group_entry(
                            2,
                            cellular_integrities,
                        ),
                        crate::simulation::accelerator_buffer_bind_group_entry(
                            3,
                            cellular_kinematics,
                        ),
                        crate::simulation::accelerator_buffer_bind_group_entry(4, cellular_amounts),
                        crate::simulation::accelerator_buffer_bind_group_entry(
                            5,
                            cellular_temperatures,
                        ),
                        crate::simulation::accelerator_buffer_bind_group_entry(
                            6,
                            external_body_occupancy,
                        ),
                        crate::simulation::accelerator_buffer_bind_group_entry(7, particles),
                        crate::simulation::accelerator_buffer_bind_group_entry(
                            8,
                            fluid_free_indices,
                        ),
                        crate::simulation::accelerator_buffer_bind_group_entry(9, fluid_free_count),
                        crate::simulation::accelerator_buffer_bind_group_entry(
                            10,
                            gas_concentrations,
                        ),
                        crate::simulation::accelerator_buffer_bind_group_entry(
                            11,
                            &material_filter_mask,
                        ),
                        crate::simulation::accelerator_buffer_bind_group_entry(
                            12,
                            &material_amounts,
                        ),
                        crate::simulation::accelerator_buffer_bind_group_entry(13, rigid_cells),
                        crate::simulation::accelerator_buffer_bind_group_entry(
                            14,
                            rigid_transforms,
                        ),
                        crate::simulation::accelerator_buffer_bind_group_entry(
                            15,
                            rigid_cell_amounts,
                        ),
                        wgpu::BindGroupEntry {
                            binding: 16,
                            resource: material_extraction_parameters.as_entire_binding(),
                        },
                    ],
                });
            slots.push(MaterialExtractionSlot {
                material_filter_mask,
                material_amounts,
                material_extraction_parameters,
                readback,
                bind_group,
                active_request: None,
                readback_result: None,
            });
        }
        Self {
            slots,
            extract_pipeline,
            material_count,
            buffered_cell_count,
            particle_capacity,
            gas_count,
            material_offsets,
            material_counts,
        }
    }

    pub(crate) fn submit_next(
        &mut self,
        accelerator: &Accelerator,
        pending: &mut std::collections::VecDeque<PendingMaterialExtraction>,
        buffered_origin: TileCoordinates,
        buffered_tile_size: [u32; 2],
        ring_offset: [u32; 2],
        rigid_cell_count: u32,
    ) -> bool {
        let Some(slot) = self
            .slots
            .iter_mut()
            .find(|slot| slot.active_request.is_none())
        else {
            return false;
        };
        let Some(request) = pending.pop_front() else {
            return false;
        };
        let (center, radius) = request.region.circle();
        let mask_bytes: Vec<u8> = request
            .material_mask
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .collect();
        accelerator.wgpu_queue().write_buffer(
            slot.material_filter_mask.wgpu_buffer(),
            0,
            &mask_bytes,
        );
        let mut parameters: [u32; 24] = [0; 24];
        parameters[0] = (center.tile_coordinates.x as f32 + center.x_offset).to_bits();
        parameters[1] = (center.tile_coordinates.y as f32 + center.y_offset).to_bits();
        parameters[2] = radius.to_bits();
        parameters[3] = MATERIAL_EXTRACTION_FIXED_POINT_SCALE.to_bits();
        parameters[4] = buffered_origin.x as u32;
        parameters[5] = buffered_origin.y as u32;
        parameters[6] = buffered_tile_size[0];
        parameters[7] = buffered_tile_size[1];
        parameters[8] = ring_offset[0];
        parameters[9] = ring_offset[1];
        parameters[10] = self.buffered_cell_count;
        parameters[11] = self.gas_count;
        parameters[12] = self.particle_capacity;
        parameters[13] = rigid_cell_count;
        parameters[14] = self.material_count;
        parameters[16..20].copy_from_slice(&self.material_offsets);
        parameters[20..24].copy_from_slice(&self.material_counts);
        accelerator.wgpu_queue().write_buffer(
            &slot.material_extraction_parameters,
            0,
            &parameters
                .iter()
                .flat_map(|word| word.to_le_bytes())
                .collect::<Vec<_>>(),
        );
        let mut encoder: wgpu::CommandEncoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("material extraction"),
                });
        encoder.clear_buffer(slot.material_amounts.wgpu_buffer(), 0, None);
        let work_count: u32 = self
            .buffered_cell_count
            .max(self.particle_capacity)
            .max(self.gas_count.saturating_mul(self.buffered_cell_count))
            .max(rigid_cell_count);
        let mut extraction_compute_pass: wgpu::ComputePass<'_> =
            accelerator.begin_compute_pass(&mut encoder, "extract authoritative materials");
        extraction_compute_pass.set_pipeline(&self.extract_pipeline);
        extraction_compute_pass.set_bind_group(0, &slot.bind_group, &[]);
        extraction_compute_pass.dispatch_workgroups(work_count.div_ceil(64), 1, 1);
        drop(extraction_compute_pass);
        encoder.copy_buffer_to_buffer(
            slot.material_amounts.wgpu_buffer(),
            0,
            &slot.readback,
            0,
            self.material_count.max(1) as u64 * 4,
        );
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
        let (sender, receiver): (
            SyncSender<Result<(), wgpu::BufferAsyncError>>,
            Receiver<Result<(), wgpu::BufferAsyncError>>,
        ) = sync_channel(1);
        slot.readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                sender.send(result).ok();
            });
        slot.active_request = Some(request.request);
        slot.readback_result = Some(receiver);
        true
    }

    pub(crate) fn has_free_slot(&self) -> bool {
        self.slots.iter().any(|slot| slot.active_request.is_none())
    }

    pub(crate) fn take_completed(
        &mut self,
        accelerator: &Accelerator,
        materials: &MaterialRegistry,
    ) -> Vec<MaterialExtractionResult> {
        let mut results: Vec<MaterialExtractionResult> = Vec::new();
        for slot in &mut self.slots {
            let Some(readback_result) = slot.readback_result.as_ref() else {
                continue;
            };
            let Ok(readback_result) = readback_result.try_recv() else {
                continue;
            };
            let request: MaterialExtractionRequest = slot.active_request.take().unwrap();
            slot.readback_result = None;
            if readback_result.is_err() {
                slot.readback.unmap();
                results.push(MaterialExtractionResult {
                    request,
                    materials: Vec::new(),
                });
                continue;
            }
            let mapped: wgpu::BufferView = slot
                .readback
                .slice(..)
                .get_mapped_range()
                .expect("mapped extraction readback");
            let mut amounts: Vec<MaterialExtractionAmount> = Vec::new();
            for (dense_index, amount_bytes) in mapped.as_chunks::<4>().0.iter().enumerate() {
                let fixed_amount: u32 = u32::from_le_bytes(*amount_bytes);
                if fixed_amount == 0 {
                    continue;
                }
                if let Some(material) = materials.identifier_from_dense_index(dense_index as u32) {
                    amounts.push(MaterialExtractionAmount {
                        material,
                        amount: fixed_amount as f32 / MATERIAL_EXTRACTION_FIXED_POINT_SCALE,
                    });
                }
            }
            drop(mapped);
            slot.readback.unmap();
            results.push(MaterialExtractionResult {
                request,
                materials: amounts,
            });
        }
        results.sort_unstable_by_key(|result| result.request);
        let _ = accelerator;
        results
    }
}

impl Drop for MaterialExtraction {
    fn drop(&mut self) {
        for slot in &mut self.slots {
            slot.material_filter_mask.free();
            slot.material_amounts.free();
            slot.material_extraction_parameters.destroy();
            slot.readback.destroy();
        }
    }
}
