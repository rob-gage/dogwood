// Copyright Rob Gage 2026

use super::{
    form_index_counts,
    tag_masks,
    SceneActorSensing,
    SceneActorSensingConfiguration,
    SCENE_ACTOR_SENSING_TAG_COUNT_MAXIMUM,
};
use crate::{
    actors::Actor,
    materials::MaterialTable,
    scenes::{
        cellular_tiles::{
            CellularTileBuffers,
            CellularTileRegion,
            CELLULAR_TILE_ACTOR_COUNT_MAXIMUM,
        },
        fluids::FLUID_RASTER_SCALE,
        pressure::GAS_AMOUNT_PER_ATMOSPHERE,
        SceneError,
    },
};
use engine_host::acceleration::{
    Accelerator,
    AcceleratorReadback,
    AcceleratorReadbackError,
};
use std::collections::HashMap;

/// Bytes of one actor's result record: eight tag counts, the breathable and breathing band
/// counts, and padding
const RESULT_BYTE_COUNT: u64 = 64;

/// Bytes of the sensing uniforms
const UNIFORM_BYTE_COUNT: usize = 80;

/// Sensing results that may be copied for the CPU at once
const READBACK_COUNT: usize = 3;

/// Fluid coverage of a cell, as raster mass over its fluid's rest density, at which the fluid
/// touches an actor; every particle rasterizes its fluid's rest density into one cell, so
/// coverage counts particles and this means at least one particle
const CONTACT_COVERAGE: f32 = 0.5;

/// Fluid coverage of a cell and its eight neighbours at which it can no longer be breathed: at
/// least one particle among them, since resting fluid leaves cells between its particles empty
const DROWNING_COVERAGE: f32 = 0.5;

/// GPU sensing of the cells touching and surrounding every cellular tile actor, read back one
/// simulation step later
pub(crate) struct ActorSensingPass {
    /// Pipeline sensing one actor per workgroup
    pipeline: wgpu::ComputePipeline,
    /// Bind group of the uniforms, actors, cell buffers, fluid raster, tag masks, and results
    bind_group: wgpu::BindGroup,
    /// Region layout, material layout, and limits of the next sensing step
    uniform_buffer: wgpu::Buffer,
    /// Tag masks of cellular static, cellular dynamic, fluid, then gas material indices
    tag_mask_buffer: wgpu::Buffer,
    /// Material indices of cellular static, cellular dynamic, fluid, and gas forms in the masks
    form_index_counts: [u32; 4],
    /// Result record of every actor
    result_buffer: wgpu::Buffer,
    /// Asynchronous copies of the results with the actors they belong to, by record
    readbacks: AcceleratorReadback<Vec<Actor>>,
    /// Configuration the tag masks were written for
    configuration: SceneActorSensingConfiguration,
}

impl ActorSensingPass {

    /// Creates sensing resources over a scene's cell buffers, fluid raster, and cellular tile
    /// actor records, sensing with the default configuration
    pub(crate) fn new(
        accelerator: &Accelerator,
        cellular_tile_buffers: &CellularTileBuffers,
        material_table: &MaterialTable,
        actor_buffer: &wgpu::Buffer,
        fluid_raster_buffer: &wgpu::Buffer,
    ) -> Self {
        let device: &wgpu::Device = accelerator.device();
        let storage_layout_entry = |binding: u32, read_only: bool| -> wgpu::BindGroupLayoutEntry {
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
        };
        let bind_group_layout: wgpu::BindGroupLayout = engine_host::acceleration::create_bind_group_layout(device,
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("`ActorSensingPass.bind_group_layout`"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    storage_layout_entry(1, true),
                    storage_layout_entry(2, true),
                    storage_layout_entry(3, true),
                    storage_layout_entry(4, true),
                    storage_layout_entry(5, true),
                    storage_layout_entry(6, true),
                    storage_layout_entry(7, true),
                    storage_layout_entry(8, true),
                    storage_layout_entry(9, true),
                    storage_layout_entry(10, false),
                ],
            },
        );
        let pipeline_layout: wgpu::PipelineLayout = engine_host::acceleration::create_pipeline_layout(device,
            &wgpu::PipelineLayoutDescriptor {
                label: Some("`ActorSensingPass.pipeline_layout`"),
                bind_group_layouts: &[Some(&bind_group_layout)],
                immediate_size: 0,
            },
        );
        let shader_module: wgpu::ShaderModule = engine_host::acceleration::create_shader_module(device,
            wgpu::ShaderModuleDescriptor {
                label: Some("`ActorSensingPass.shader_module`"),
                source: wgpu::ShaderSource::Wgsl(include_str!("actor_sensing.wgsl").into()),
            },
        );
        let pipeline: wgpu::ComputePipeline = engine_host::acceleration::create_compute_pipeline(device,
            &wgpu::ComputePipelineDescriptor {
                label: Some("`ActorSensingPass.pipeline`"),
                layout: Some(&pipeline_layout),
                module: &shader_module,
                entry_point: Some("sense"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                cache: None,
            },
        );
        let uniform_buffer: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("`ActorSensingPass.uniform_buffer`"),
            size: UNIFORM_BYTE_COUNT as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let form_index_counts: [u32; 4] = form_index_counts(material_table);
        let mask_count: u64 = form_index_counts.iter().map(|count: &u32| u64::from(*count)).sum();
        let tag_mask_buffer: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("`ActorSensingPass.tag_mask_buffer`"),
            size: mask_count.max(1) * 4,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let result_buffer: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("`ActorSensingPass.result_buffer`"),
            size: CELLULAR_TILE_ACTOR_COUNT_MAXIMUM as u64 * RESULT_BYTE_COUNT,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group: wgpu::BindGroup = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("`ActorSensingPass.bind_group`"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: uniform_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: actor_buffer.as_entire_binding() },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: cellular_tile_buffers.material_identifier_buffer()
                        .as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: cellular_tile_buffers.rigid_body_cell_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: fluid_raster_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: material_table.fluid_properties_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: cellular_tile_buffers.gas_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry { binding: 7, resource: tag_mask_buffer.as_entire_binding() },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: cellular_tile_buffers.matter_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 9,
                    resource: cellular_tile_buffers.pressure_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 10,
                    resource: result_buffer.as_entire_binding(),
                },
            ],
        });
        let readbacks: AcceleratorReadback<Vec<Actor>> = AcceleratorReadback::new(
            accelerator,
            "`ActorSensingPass.readbacks`",
            result_buffer.size(),
            READBACK_COUNT,
        );
        let mut pass: Self = Self {
            pipeline,
            bind_group,
            uniform_buffer,
            tag_mask_buffer,
            form_index_counts,
            result_buffer,
            readbacks,
            configuration: SceneActorSensingConfiguration::default(),
        };
        pass.write_tag_masks(accelerator, material_table);
        pass
    }

    /// Senses with a new configuration from the next step, rewriting the tag masks when its tags
    /// changed
    pub(crate) fn configure(
        &mut self,
        accelerator: &Accelerator,
        material_table: &MaterialTable,
        configuration: &SceneActorSensingConfiguration,
    ) {
        if *configuration == self.configuration { return; }
        let is_retagged: bool = configuration.tags != self.configuration.tags;
        self.configuration = configuration.clone();
        if is_retagged { self.write_tag_masks(accelerator, material_table); }
    }

    /// Replaces each sensed actor's record with its newest completed readback, keeping only the
    /// actors of that readback, without waiting for the GPU
    pub(crate) fn read(
        &mut self,
        accelerator: &Accelerator,
        sensing: &mut HashMap<Actor, SceneActorSensing>,
    ) -> Result<(), SceneError> {
        let tag_count: usize = self.configuration.tags.len();
        let result: Result<(), AcceleratorReadbackError> = self.readbacks.read(
            accelerator,
            false,
            |actors: Vec<Actor>, bytes: &[u8]| {
                sensing.clear();
                for (record, actor) in actors.into_iter().enumerate() {
                    let result_word = |index: usize| -> u32 {
                        let start: usize = record * RESULT_BYTE_COUNT as usize + index * 4;
                        u32::from_le_bytes([
                            bytes[start],
                            bytes[start + 1],
                            bytes[start + 2],
                            bytes[start + 3],
                        ])
                    };
                    let mut tag_cell_counts: [u32; SCENE_ACTOR_SENSING_TAG_COUNT_MAXIMUM] =
                        [0; SCENE_ACTOR_SENSING_TAG_COUNT_MAXIMUM];
                    for (tag, count) in tag_cell_counts.iter_mut().enumerate().take(tag_count) {
                        *count = result_word(tag);
                    }
                    sensing.insert(actor, SceneActorSensing {
                        tag_cell_counts,
                        breathable_cell_count: result_word(SCENE_ACTOR_SENSING_TAG_COUNT_MAXIMUM),
                        breathing_cell_count: result_word(
                            SCENE_ACTOR_SENSING_TAG_COUNT_MAXIMUM + 1,
                        ),
                        maximum_temperature: f32::from_bits(result_word(10)),
                        maximum_pressure: f32::from_bits(result_word(11)),
                    });
                }
            },
        );
        result.map_err(|error: AcceleratorReadbackError| match error {
            AcceleratorReadbackError::Poll(_) => SceneError::ActorSensingPoll,
            AcceleratorReadbackError::BufferAsync(error) => SceneError::ActorSensingResponse {
                error,
            },
        })
    }

    /// Senses the cellular tile actors, given in record order, after every queued simulation
    /// step and copies their results for a later `read`, skipping the copy while every readback
    /// is in flight
    pub(crate) fn sense(
        &mut self,
        accelerator: &Accelerator,
        active_region: CellularTileRegion,
        ring_offset: (u32, u32),
        actors: &[Actor],
    ) {
        let actor_count: usize = actors.len().min(CELLULAR_TILE_ACTOR_COUNT_MAXIMUM);
        if actor_count == 0 { return; }
        let words: [u32; UNIFORM_BYTE_COUNT / 4] = [
            active_region.width() as u32,
            active_region.height() as u32,
            ring_offset.0,
            ring_offset.1,
            actor_count as u32,
            self.configuration.tags.len() as u32,
            self.form_index_counts[0],
            self.form_index_counts[1],
            self.form_index_counts[2],
            self.form_index_counts[3],
            FLUID_RASTER_SCALE.to_bits(),
            self.configuration.contact_margin.to_bits(),
            self.configuration.breathing_margin.to_bits(),
            (self.configuration.breathable_gas_amount * GAS_AMOUNT_PER_ATMOSPHERE).to_bits(),
            CONTACT_COVERAGE.to_bits(),
            DROWNING_COVERAGE.to_bits(),
            (self.configuration.gas_tag_amount * GAS_AMOUNT_PER_ATMOSPHERE).to_bits(),
            0,
            0,
            0,
        ];
        let uniform_bytes: Vec<u8> = words.iter().flat_map(|word: &u32| word.to_le_bytes())
            .collect();
        accelerator.queue().write_buffer(&self.uniform_buffer, 0, &uniform_bytes);
        let byte_count: u64 = actor_count as u64 * RESULT_BYTE_COUNT;
        let mut command_encoder: wgpu::CommandEncoder = accelerator.device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Scene actor sensing"),
            });
        command_encoder.clear_buffer(&self.result_buffer, 0, Some(byte_count));
        {
            let mut compute_pass: wgpu::ComputePass<'_> = command_encoder
                .begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("Scene actor sensing"),
                    timestamp_writes: None,
                });
            compute_pass.set_pipeline(&self.pipeline);
            compute_pass.set_bind_group(0, &self.bind_group, &[]);
            compute_pass.dispatch_workgroups(actor_count as u32, 1, 1);
        }
        let is_copied: bool = self.readbacks.copy(
            &mut command_encoder,
            &self.result_buffer,
            0,
            byte_count,
            actors[..actor_count].to_vec(),
        );
        let submission_index: wgpu::SubmissionIndex =
            accelerator.queue().submit(Some(command_encoder.finish()));
        if is_copied { self.readbacks.map(submission_index); }
    }

    /// Writes the tag masks of the configured tags
    fn write_tag_masks(&mut self, accelerator: &Accelerator, material_table: &MaterialTable) {
        let masks: Vec<u32> = tag_masks(material_table, &self.configuration.tags);
        if masks.is_empty() { return; }
        let bytes: Vec<u8> = masks.iter().flat_map(|mask: &u32| mask.to_le_bytes()).collect();
        accelerator.queue().write_buffer(&self.tag_mask_buffer, 0, &bytes);
    }

}
