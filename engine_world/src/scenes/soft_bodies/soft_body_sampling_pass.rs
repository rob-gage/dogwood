// Copyright Rob Gage 2026

use super::{
    SceneSoftBody,
    SoftBodies,
    SoftBodyEnvironment,
    SOFT_BODY_SAMPLE_COUNT_MAXIMUM,
};
use crate::{
    materials::MaterialTable,
    scenes::{
        actor_sensing::{form_index_counts, tag_masks},
        cellular_tiles::{
            CellularTileBuffers,
            CellularTileRegion,
            CELLULAR_TILE_SIZE,
        },
        fluids::FLUID_RASTER_SCALE,
        SceneError,
    },
};
use engine_host::acceleration::{
    Accelerator,
    AcceleratorReadback,
    AcceleratorReadbackError,
};

/// Words of one sample's result
const RESULT_WORD_COUNT: usize = 10;

/// Sample results that may be copied for the CPU at once
const READBACK_COUNT: usize = 3;

/// Pressure simulation substeps of one tick, converting gas flows per substep to speeds
const PRESSURE_SUBSTEPS_PER_TICK: f32 = 3.0;

/// GPU sampling of the cells around chosen particles, read back some simulation steps later so that
/// coupling soft bodies to the cell grid never stalls on the GPU and never reads back whole grids
pub(crate) struct SoftBodySamplingPass {
    /// Pipeline sampling up to one workgroup of particles at a time
    pipeline: wgpu::ComputePipeline,
    /// Bind group of the uniforms, samples, cell buffers, fluid raster, tag masks, and results
    bind_group: wgpu::BindGroup,
    /// Region layout, material layout, and scales of the next step
    uniform_buffer: wgpu::Buffer,
    /// Logical cell of each sample
    sample_buffer: wgpu::Buffer,
    /// Tag masks of cellular static, cellular dynamic, fluid, then gas material indices
    tag_mask_buffer: wgpu::Buffer,
    /// Material indices of cellular static, cellular dynamic, fluid, and gas forms in the masks
    form_index_counts: [u32; 4],
    /// Result record of every sample
    result_buffer: wgpu::Buffer,
    /// Asynchronous copies of the results with the particles they belong to
    readbacks: AcceleratorReadback<Vec<(SceneSoftBody, u32, u32)>>,
}

impl SoftBodySamplingPass {

    /// Creates sampling resources over a scene's cell buffers and fluid raster
    pub(crate) fn new(
        accelerator: &Accelerator,
        cellular_tile_buffers: &CellularTileBuffers,
        material_table: &MaterialTable,
        fluid_raster_buffer: &wgpu::Buffer,
    ) -> Self {
        let device: &wgpu::Device = accelerator.device();
        let storage = |binding: u32, read_only: bool| -> wgpu::BindGroupLayoutEntry {
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
        let layout: wgpu::BindGroupLayout = device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("`SoftBodySamplingPass.layout`"),
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
                    storage(1, true),
                    storage(2, true),
                    storage(3, true),
                    storage(4, true),
                    storage(5, true),
                    storage(6, true),
                    storage(7, true),
                    storage(8, true),
                    storage(9, false),
                ],
            },
        );
        let pipeline_layout: wgpu::PipelineLayout = device.create_pipeline_layout(
            &wgpu::PipelineLayoutDescriptor {
                label: Some("`SoftBodySamplingPass.pipeline_layout`"),
                bind_group_layouts: &[Some(&layout)],
                immediate_size: 0,
            },
        );
        let module: wgpu::ShaderModule = device.create_shader_module(
            wgpu::ShaderModuleDescriptor {
                label: Some("`SoftBodySamplingPass.module`"),
                source: wgpu::ShaderSource::Wgsl(include_str!("soft_body_sampling.wgsl").into()),
            },
        );
        let pipeline: wgpu::ComputePipeline = device.create_compute_pipeline(
            &wgpu::ComputePipelineDescriptor {
                label: Some("`SoftBodySamplingPass.pipeline`"),
                layout: Some(&pipeline_layout),
                module: &module,
                entry_point: Some("sample"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                cache: None,
            },
        );
        let uniform_buffer: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("`SoftBodySamplingPass.uniform_buffer`"),
            size: 48,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sample_buffer: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("`SoftBodySamplingPass.sample_buffer`"),
            size: SOFT_BODY_SAMPLE_COUNT_MAXIMUM as u64 * 8,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let form_index_counts: [u32; 4] = form_index_counts(material_table);
        let mask_count: u64 = form_index_counts.iter().map(|count: &u32| u64::from(*count)).sum();
        let tag_mask_buffer: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("`SoftBodySamplingPass.tag_mask_buffer`"),
            size: mask_count.max(1) * 4,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let result_size: u64 = (SOFT_BODY_SAMPLE_COUNT_MAXIMUM * RESULT_WORD_COUNT * 4) as u64;
        let result_buffer: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("`SoftBodySamplingPass.result_buffer`"),
            size: result_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        fn entry(binding: u32, buffer: &wgpu::Buffer) -> wgpu::BindGroupEntry<'_> {
            wgpu::BindGroupEntry { binding, resource: buffer.as_entire_binding() }
        }
        let bind_group: wgpu::BindGroup = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("`SoftBodySamplingPass.bind_group`"),
            layout: &layout,
            entries: &[
                entry(0, &uniform_buffer),
                entry(1, &sample_buffer),
                entry(2, cellular_tile_buffers.material_identifier_buffer()),
                entry(3, fluid_raster_buffer),
                entry(4, material_table.fluid_properties_buffer()),
                entry(5, cellular_tile_buffers.gas_buffer()),
                entry(6, &tag_mask_buffer),
                entry(7, cellular_tile_buffers.matter_buffer()),
                entry(8, cellular_tile_buffers.pressure_buffer()),
                entry(9, &result_buffer),
            ],
        });
        Self {
            pipeline,
            bind_group,
            uniform_buffer,
            sample_buffer,
            tag_mask_buffer,
            form_index_counts,
            result_buffer,
            readbacks: AcceleratorReadback::new(
                accelerator,
                "`SoftBodySamplingPass.readbacks`",
                result_size,
                READBACK_COUNT,
            ),
        }
    }

    /// Rewrites the tag masks when the reported tags changed
    pub(crate) fn configure(
        &mut self,
        accelerator: &Accelerator,
        material_table: &MaterialTable,
        soft_bodies: &mut SoftBodies,
    ) {
        if !soft_bodies.is_environment_tags_changed { return; }
        soft_bodies.is_environment_tags_changed = false;
        let masks: Vec<u32> = tag_masks(material_table, &soft_bodies.environment_tags);
        if masks.is_empty() { return; }
        let bytes: Vec<u8> = masks.iter().flat_map(|mask: &u32| mask.to_le_bytes()).collect();
        accelerator.queue().write_buffer(&self.tag_mask_buffer, 0, &bytes);
    }

    /// Applies every completed sample to its particle without waiting for the GPU
    pub(crate) fn read(
        &mut self,
        accelerator: &Accelerator,
        soft_bodies: &mut SoftBodies,
        gravity: f32,
    ) -> Result<(), SceneError> {
        let result: Result<(), AcceleratorReadbackError> = self.readbacks.read(
            accelerator,
            false,
            |records: Vec<(SceneSoftBody, u32, u32)>, bytes: &[u8]| {
                for (index, (body, particle, spread)) in records.into_iter().enumerate() {
                    let word = |offset: usize| -> u32 {
                        let start: usize = (index * RESULT_WORD_COUNT + offset) * 4;
                        u32::from_le_bytes([
                            bytes[start],
                            bytes[start + 1],
                            bytes[start + 2],
                            bytes[start + 3],
                        ])
                    };
                    let float = |offset: usize| -> f32 {
                        let value: f32 = f32::from_bits(word(offset));
                        if value.is_finite() { value } else { 0.0 }
                    };
                    soft_bodies.apply_sample(body, particle, spread, SoftBodyEnvironment {
                        temperature: float(0),
                        pressure: float(1),
                        pressure_gradient: [float(2), float(3)],
                        gas_velocity: [float(4), float(5)],
                        fluid_coverage: float(6),
                        fluid_velocity: [float(7), float(8)],
                        tags: word(9),
                        is_sampled: true,
                    }, gravity);
                }
            },
        );
        result.map_err(|error: AcceleratorReadbackError| match error {
            AcceleratorReadbackError::Poll(_) => SceneError::SoftBodySamplingPoll,
            AcceleratorReadbackError::BufferAsync(error) =>
                SceneError::SoftBodySamplingResponse { error },
        })
    }

    /// Samples the chosen particles after every queued simulation step and copies their results for
    /// a later `read`, skipping the work while every readback is in flight
    pub(crate) fn sample(
        &mut self,
        accelerator: &Accelerator,
        active_region: CellularTileRegion,
        ring_offset: (u32, u32),
        soft_bodies: &SoftBodies,
    ) {
        if soft_bodies.bodies.is_empty() { return; }
        let (cells, records): (Vec<[u32; 2]>, Vec<(SceneSoftBody, u32, u32)>) =
            soft_bodies.choose_samples(active_region);
        if cells.is_empty() { return; }
        let words: [u32; 12] = [
            active_region.width() as u32,
            active_region.height() as u32,
            ring_offset.0,
            ring_offset.1,
            cells.len() as u32,
            self.form_index_counts[0],
            self.form_index_counts[1],
            self.form_index_counts[2],
            self.form_index_counts[3],
            FLUID_RASTER_SCALE.to_bits(),
            (PRESSURE_SUBSTEPS_PER_TICK * 60.0 / CELLULAR_TILE_SIZE as f32).to_bits(),
            (60.0 / CELLULAR_TILE_SIZE as f32).to_bits(),
        ];
        let uniform_bytes: Vec<u8> = words.iter().flat_map(|word: &u32| word.to_le_bytes()).collect();
        accelerator.queue().write_buffer(&self.uniform_buffer, 0, &uniform_bytes);
        let sample_bytes: Vec<u8> = cells.iter()
            .flat_map(|cell: &[u32; 2]| [cell[0].to_le_bytes(), cell[1].to_le_bytes()].concat())
            .collect();
        accelerator.queue().write_buffer(&self.sample_buffer, 0, &sample_bytes);
        let mut encoder: wgpu::CommandEncoder = accelerator.device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Scene soft body sampling"),
            });
        {
            let mut pass: wgpu::ComputePass<'_> = encoder.begin_compute_pass(
                &wgpu::ComputePassDescriptor {
                    label: Some("Scene soft body sampling"),
                    timestamp_writes: None,
                },
            );
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups((cells.len() as u32).div_ceil(64), 1, 1);
        }
        let byte_count: u64 = (cells.len() * RESULT_WORD_COUNT * 4) as u64;
        let is_copied: bool = self.readbacks.copy(
            &mut encoder,
            &self.result_buffer,
            0,
            byte_count,
            records,
        );
        let submission: wgpu::SubmissionIndex = accelerator.queue().submit(Some(encoder.finish()));
        if is_copied { self.readbacks.map(submission); }
    }

}
