// Copyright Rob Gage 2026

use super::{
    CellularTileBuffers,
    CELLULAR_TILE_AWAKE,
    CELLULAR_TILE_GAS_AWAKE,
    CELLULAR_TILE_SIZE,
    CELLULAR_TILE_THERMAL_AWAKE,
};
use engine_host::acceleration::Accelerator;
use std::num::NonZeroU64;

/// Number of invocations in one compute workgroup
const WORKGROUP_SIZE: u32 = 64;

/// Largest number of workgroups dispatched along one dimension
const WORKGROUP_COUNT_MAXIMUM: u32 = 65_535;

/// Bytes between the parameters of consecutive dispatches in a parameters buffer
const PARAMETERS_STRIDE: u64 = 256;

/// Bytes of one dispatch's parameters
const PARAMETERS_BYTE_COUNT: u64 = 32;

/// Tile index marking a requested tile that is not resident
pub const ABSENT_TILE_INDEX: u32 = u32::MAX;

/// Words of one tile in the material identifier buffer, one per cell
const MATERIAL_WORD_COUNT: u32 = (CELLULAR_TILE_SIZE * CELLULAR_TILE_SIZE) as u32;

/// Words of one tile in the packed color and radiance buffer, two per cell
const COLOR_WORD_COUNT: u32 = MATERIAL_WORD_COUNT * 2;

/// Words of one tile in the kinematics buffer, two per cell
const KINEMATICS_WORD_COUNT: u32 = MATERIAL_WORD_COUNT * 2;

/// Words of one tile in the gas buffer, eight per cell
const GAS_WORD_COUNT: u32 = MATERIAL_WORD_COUNT * 8;

/// Words of one tile in the matter buffer, two per cell
const MATTER_WORD_COUNT: u32 = MATERIAL_WORD_COUNT * 2;

/// First word of the material identifiers within a record
const MATERIAL_RECORD_OFFSET: u32 = 0;

/// First word of the colors and radiances within a record
const COLOR_RECORD_OFFSET: u32 = MATERIAL_RECORD_OFFSET + MATERIAL_WORD_COUNT;

/// First word of the kinematics within a record
const KINEMATICS_RECORD_OFFSET: u32 = COLOR_RECORD_OFFSET + COLOR_WORD_COUNT;

/// First word of the gas within a record
const GAS_RECORD_OFFSET: u32 = KINEMATICS_RECORD_OFFSET + KINEMATICS_WORD_COUNT;

/// First word of the matter within a record
const MATTER_RECORD_OFFSET: u32 = GAS_RECORD_OFFSET + GAS_WORD_COUNT;

/// Words of one packed record: material identifiers, colors, kinematics, gas, then matter
pub const RECORD_WORD_COUNT: u32 = MATTER_RECORD_OFFSET + MATTER_WORD_COUNT;

/// Tiles the transfer buffers hold before they first grow
const INITIAL_TILE_CAPACITY: usize = 256;

/// One destination field of an upload
struct UploadField {
    /// Words of the field per tile
    word_count: u32,
    /// First word of the field within a record
    record_word_offset: u32,
    /// Constant filling the field instead of the record, if any
    constant: Option<u32>,
}

/// Fields written by an upload, in the order of the upload bind groups
const UPLOAD_FIELDS: [UploadField; 9] = [
    UploadField {
        word_count: MATERIAL_WORD_COUNT,
        record_word_offset: MATERIAL_RECORD_OFFSET,
        constant: None,
    },
    UploadField { word_count: COLOR_WORD_COUNT, record_word_offset: COLOR_RECORD_OFFSET, constant: None },
    UploadField {
        word_count: KINEMATICS_WORD_COUNT,
        record_word_offset: KINEMATICS_RECORD_OFFSET,
        constant: None,
    },
    UploadField { word_count: 1, record_word_offset: 0, constant: Some(CELLULAR_TILE_AWAKE) },
    UploadField { word_count: GAS_WORD_COUNT, record_word_offset: GAS_RECORD_OFFSET, constant: None },
    UploadField { word_count: 1, record_word_offset: 0, constant: Some(CELLULAR_TILE_GAS_AWAKE) },
    UploadField { word_count: MATTER_WORD_COUNT, record_word_offset: MATTER_RECORD_OFFSET, constant: None },
    UploadField { word_count: 1, record_word_offset: 0, constant: Some(CELLULAR_TILE_THERMAL_AWAKE) },
    // entering tiles hold no rigid body cells until rigid bodies rasterize into them
    UploadField { word_count: MATERIAL_WORD_COUNT, record_word_offset: 0, constant: Some(0) },
];

/// Gathered fields as `(word_count, record_word_offset)`, in the order of the gather bind groups
const GATHER_FIELDS: [(u32, u32); 5] = [
    (MATERIAL_WORD_COUNT, MATERIAL_RECORD_OFFSET),
    (COLOR_WORD_COUNT, COLOR_RECORD_OFFSET),
    (KINEMATICS_WORD_COUNT, KINEMATICS_RECORD_OFFSET),
    (GAS_WORD_COUNT, GAS_RECORD_OFFSET),
    (MATTER_WORD_COUNT, MATTER_RECORD_OFFSET),
];

/// Per-tile record buffers and the GPU passes moving whole tiles between them and the cellular
/// tile buffers, replacing many small copies and writes
pub struct CellularTileStreamingTransfer {
    /// Pipeline writing record fields into tile slots
    scatter_pipeline: wgpu::ComputePipeline,
    /// Pipeline reading tile slots into record fields
    gather_pipeline: wgpu::ComputePipeline,
    /// Layout of the parameters, records, tile indices, and field buffers
    bind_group_layout: wgpu::BindGroupLayout,
    /// Dynamic-offset parameters of each upload field
    upload_parameters: wgpu::Buffer,
    /// Dynamic-offset parameters of each gather field
    gather_parameters: wgpu::Buffer,
    /// Tiles the upload buffers hold
    upload_capacity: usize,
    /// Records written by the CPU, one per entering tile
    upload_records: wgpu::Buffer,
    /// Physical tile index of each uploaded record
    upload_indices: wgpu::Buffer,
    /// Bind group of each upload field
    upload_bind_groups: Vec<wgpu::BindGroup>,
    /// Tiles the gather buffers hold
    gather_capacity: usize,
    /// Records written only by the GPU, one per outgoing tile
    gather_records: wgpu::Buffer,
    /// Physical tile index of each gathered record, or `ABSENT_TILE_INDEX`
    gather_indices: wgpu::Buffer,
    /// Bind group of each gather field
    gather_bind_groups: Vec<wgpu::BindGroup>,
}

impl CellularTileStreamingTransfer {

    /// Creates the pipelines and initial buffers for a set of cellular tile buffers
    pub fn new(accelerator: &Accelerator, cellular_tile_buffers: &CellularTileBuffers) -> Self {
        let device: &wgpu::Device = accelerator.device();
        let entry = |binding: u32, ty: wgpu::BufferBindingType, has_dynamic_offset: bool| {
            wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty,
                    has_dynamic_offset,
                    min_binding_size: None,
                },
                count: None,
            }
        };
        let bind_group_layout: wgpu::BindGroupLayout = device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("`CellularTileStreamingTransfer.bind_group_layout`"),
                entries: &[
                    entry(0, wgpu::BufferBindingType::Uniform, true),
                    entry(1, wgpu::BufferBindingType::Storage { read_only: false }, false),
                    entry(2, wgpu::BufferBindingType::Storage { read_only: true }, false),
                    entry(3, wgpu::BufferBindingType::Storage { read_only: false }, false),
                ],
            },
        );
        let pipeline_layout: wgpu::PipelineLayout = device.create_pipeline_layout(
            &wgpu::PipelineLayoutDescriptor {
                label: Some("`CellularTileStreamingTransfer.pipeline_layout`"),
                bind_group_layouts: &[Some(&bind_group_layout)],
                immediate_size: 0,
            },
        );
        let shader_module: wgpu::ShaderModule = device.create_shader_module(
            wgpu::ShaderModuleDescriptor {
                label: Some("`CellularTileStreamingTransfer.shader_module`"),
                source: wgpu::ShaderSource::Wgsl(
                    include_str!("cellular_tile_streaming_transfer.wgsl").into(),
                ),
            },
        );
        let pipeline = |label: &str, entry_point: &str| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                module: &shader_module,
                entry_point: Some(entry_point),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                cache: None,
            })
        };
        let scatter_pipeline = pipeline("`CellularTileStreamingTransfer.scatter_pipeline`", "scatter");
        let gather_pipeline = pipeline("`CellularTileStreamingTransfer.gather_pipeline`", "gather");
        let parameters_buffer = |label: &str, count: usize| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: count as u64 * PARAMETERS_STRIDE,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let upload_parameters = parameters_buffer(
            "`CellularTileStreamingTransfer.upload_parameters`",
            UPLOAD_FIELDS.len(),
        );
        let gather_parameters = parameters_buffer(
            "`CellularTileStreamingTransfer.gather_parameters`",
            GATHER_FIELDS.len(),
        );
        let (upload_records, upload_indices) = Self::create_buffers(
            device,
            INITIAL_TILE_CAPACITY,
            "upload",
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        );
        let (gather_records, gather_indices) = Self::create_buffers(
            device,
            INITIAL_TILE_CAPACITY,
            "gather",
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        );
        let mut transfer: Self = Self {
            scatter_pipeline,
            gather_pipeline,
            bind_group_layout,
            upload_parameters,
            gather_parameters,
            upload_capacity: INITIAL_TILE_CAPACITY,
            upload_records,
            upload_indices,
            upload_bind_groups: Vec::new(),
            gather_capacity: INITIAL_TILE_CAPACITY,
            gather_records,
            gather_indices,
            gather_bind_groups: Vec::new(),
        };
        transfer.upload_bind_groups = transfer.create_upload_bind_groups(device, cellular_tile_buffers);
        transfer.gather_bind_groups = transfer.create_gather_bind_groups(device, cellular_tile_buffers);
        transfer
    }

    /// Writes records to the given physical tile slots and submits the scatter immediately
    ///
    /// `records` holds `RECORD_WORD_COUNT` little-endian words per tile in `tile_indices` order.
    pub fn upload(
        &mut self,
        accelerator: &Accelerator,
        cellular_tile_buffers: &CellularTileBuffers,
        records: &[u8],
        tile_indices: &[u32],
    ) {
        let tile_count: usize = tile_indices.len();
        if tile_count == 0 { return; }
        debug_assert_eq!(records.len(), tile_count * RECORD_WORD_COUNT as usize * 4);
        let device: &wgpu::Device = accelerator.device();
        if tile_count > self.upload_capacity {
            self.upload_capacity = tile_count.max(self.upload_capacity + self.upload_capacity / 2);
            (self.upload_records, self.upload_indices) = Self::create_buffers(
                device,
                self.upload_capacity,
                "upload",
                wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            );
            self.upload_bind_groups = self.create_upload_bind_groups(device, cellular_tile_buffers);
        }
        let queue: &wgpu::Queue = accelerator.queue();
        queue.write_buffer(&self.upload_records, 0, records);
        queue.write_buffer(&self.upload_indices, 0, &Self::words_to_bytes(tile_indices));
        let parameters: Vec<[u32; 8]> = UPLOAD_FIELDS.iter().map(|field| Self::parameters(
            tile_count as u32,
            field.word_count,
            field.record_word_offset,
            field.constant,
        )).collect();
        queue.write_buffer(&self.upload_parameters, 0, &Self::parameters_to_bytes(&parameters));
        let mut command_encoder: wgpu::CommandEncoder = device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor {
                label: Some("Scene cellular tile streaming upload"),
            },
        );
        {
            let mut compute_pass: wgpu::ComputePass<'_> = command_encoder.begin_compute_pass(
                &wgpu::ComputePassDescriptor {
                    label: Some("Scene cellular tile streaming upload"),
                    timestamp_writes: None,
                },
            );
            compute_pass.set_pipeline(&self.scatter_pipeline);
            for (field_index, field) in UPLOAD_FIELDS.iter().enumerate() {
                compute_pass.set_bind_group(
                    0,
                    &self.upload_bind_groups[field_index],
                    &[(field_index as u64 * PARAMETERS_STRIDE) as u32],
                );
                Self::dispatch(&mut compute_pass, tile_count as u32 * field.word_count);
            }
        }
        queue.submit(Some(command_encoder.finish()));
    }

    /// Encodes the gather of tiles into packed records followed by one copy of the records into
    /// `destination`; absent tiles leave zeroed records
    ///
    /// The packed record buffer is only ever written by the GPU, so reuse by a later request is
    /// ordered by the queue.
    pub fn encode_gather(
        &mut self,
        accelerator: &Accelerator,
        cellular_tile_buffers: &CellularTileBuffers,
        command_encoder: &mut wgpu::CommandEncoder,
        tile_indices: &[u32],
        destination: &wgpu::Buffer,
    ) {
        let tile_count: usize = tile_indices.len();
        if tile_count == 0 { return; }
        let device: &wgpu::Device = accelerator.device();
        if tile_count > self.gather_capacity {
            self.gather_capacity = tile_count.max(self.gather_capacity + self.gather_capacity / 2);
            (self.gather_records, self.gather_indices) = Self::create_buffers(
                device,
                self.gather_capacity,
                "gather",
                wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            );
            self.gather_bind_groups = self.create_gather_bind_groups(device, cellular_tile_buffers);
        }
        let queue: &wgpu::Queue = accelerator.queue();
        queue.write_buffer(&self.gather_indices, 0, &Self::words_to_bytes(tile_indices));
        let parameters: Vec<[u32; 8]> = GATHER_FIELDS.iter().map(|(word_count, record_word_offset)|
            Self::parameters(tile_count as u32, *word_count, *record_word_offset, None)
        ).collect();
        queue.write_buffer(&self.gather_parameters, 0, &Self::parameters_to_bytes(&parameters));
        {
            let mut compute_pass: wgpu::ComputePass<'_> = command_encoder.begin_compute_pass(
                &wgpu::ComputePassDescriptor {
                    label: Some("Scene cellular tile streaming gather"),
                    timestamp_writes: None,
                },
            );
            compute_pass.set_pipeline(&self.gather_pipeline);
            for (field_index, (word_count, _)) in GATHER_FIELDS.iter().enumerate() {
                compute_pass.set_bind_group(
                    0,
                    &self.gather_bind_groups[field_index],
                    &[(field_index as u64 * PARAMETERS_STRIDE) as u32],
                );
                Self::dispatch(&mut compute_pass, tile_count as u32 * word_count);
            }
        }
        command_encoder.copy_buffer_to_buffer(
            &self.gather_records,
            0,
            destination,
            0,
            tile_count as u64 * u64::from(RECORD_WORD_COUNT) * 4,
        );
    }

    /// Creates the record and index buffers for a number of tiles
    fn create_buffers(
        device: &wgpu::Device,
        tile_capacity: usize,
        label: &str,
        record_usage: wgpu::BufferUsages,
    ) -> (wgpu::Buffer, wgpu::Buffer) {
        let records: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&format!("`CellularTileStreamingTransfer.{label}_records`")),
            size: tile_capacity as u64 * u64::from(RECORD_WORD_COUNT) * 4,
            usage: record_usage,
            mapped_at_creation: false,
        });
        let indices: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&format!("`CellularTileStreamingTransfer.{label}_indices`")),
            size: tile_capacity as u64 * 4,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        (records, indices)
    }

    /// Creates one bind group per upload field
    fn create_upload_bind_groups(
        &self,
        device: &wgpu::Device,
        cellular_tile_buffers: &CellularTileBuffers,
    ) -> Vec<wgpu::BindGroup> {
        let destinations: [&wgpu::Buffer; 9] = [
            cellular_tile_buffers.material_identifier_buffer(),
            cellular_tile_buffers.color_buffer(),
            cellular_tile_buffers.kinematics_buffer(),
            cellular_tile_buffers.awake_buffer(),
            cellular_tile_buffers.gas_buffer(),
            cellular_tile_buffers.gas_awake_buffer(),
            cellular_tile_buffers.matter_buffer(),
            cellular_tile_buffers.thermal_awake_buffer(),
            cellular_tile_buffers.rigid_body_cell_buffer(),
        ];
        destinations.iter().map(|destination| self.create_bind_group(
            device,
            &self.upload_parameters,
            &self.upload_records,
            &self.upload_indices,
            destination,
        )).collect()
    }

    /// Creates one bind group per gather field
    fn create_gather_bind_groups(
        &self,
        device: &wgpu::Device,
        cellular_tile_buffers: &CellularTileBuffers,
    ) -> Vec<wgpu::BindGroup> {
        let sources: [&wgpu::Buffer; 5] = [
            cellular_tile_buffers.material_identifier_buffer(),
            cellular_tile_buffers.color_buffer(),
            cellular_tile_buffers.kinematics_buffer(),
            cellular_tile_buffers.gas_buffer(),
            cellular_tile_buffers.matter_buffer(),
        ];
        sources.iter().map(|source| self.create_bind_group(
            device,
            &self.gather_parameters,
            &self.gather_records,
            &self.gather_indices,
            source,
        )).collect()
    }

    /// Creates a bind group of parameters, records, tile indices, and a field buffer
    fn create_bind_group(
        &self,
        device: &wgpu::Device,
        parameters: &wgpu::Buffer,
        records: &wgpu::Buffer,
        tile_indices: &wgpu::Buffer,
        field: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("`CellularTileStreamingTransfer.bind_group`"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: parameters,
                        offset: 0,
                        size: NonZeroU64::new(PARAMETERS_BYTE_COUNT),
                    }),
                },
                wgpu::BindGroupEntry { binding: 1, resource: records.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: tile_indices.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: field.as_entire_binding() },
            ],
        })
    }

    /// Returns the shader parameters of one dispatch
    fn parameters(
        tile_count: u32,
        field_word_count: u32,
        field_word_offset: u32,
        constant: Option<u32>,
    ) -> [u32; 8] {
        let workgroup_count: u32 = (tile_count * field_word_count).div_ceil(WORKGROUP_SIZE);
        let row_workgroup_count: u32 = workgroup_count.clamp(1, WORKGROUP_COUNT_MAXIMUM);
        [
            tile_count,
            RECORD_WORD_COUNT,
            field_word_offset,
            field_word_count,
            u32::from(constant.is_some()),
            constant.unwrap_or(0),
            row_workgroup_count * WORKGROUP_SIZE,
            0,
        ]
    }

    /// Dispatches one invocation per word, wrapping rows of workgroups into the y dimension
    fn dispatch(compute_pass: &mut wgpu::ComputePass<'_>, word_count: u32) {
        let workgroup_count: u32 = word_count.div_ceil(WORKGROUP_SIZE);
        let row_workgroup_count: u32 = workgroup_count.clamp(1, WORKGROUP_COUNT_MAXIMUM);
        compute_pass.dispatch_workgroups(
            row_workgroup_count,
            workgroup_count.div_ceil(row_workgroup_count),
            1,
        );
    }

    /// Returns little-endian bytes of words
    fn words_to_bytes(words: &[u32]) -> Vec<u8> {
        words.iter().flat_map(|word| word.to_le_bytes()).collect()
    }

    /// Returns parameters laid out at `PARAMETERS_STRIDE` intervals
    fn parameters_to_bytes(parameters: &[[u32; 8]]) -> Vec<u8> {
        let mut bytes: Vec<u8> = vec![0; parameters.len() * PARAMETERS_STRIDE as usize];
        for (index, words) in parameters.iter().enumerate() {
            let start: usize = index * PARAMETERS_STRIDE as usize;
            bytes[start..start + PARAMETERS_BYTE_COUNT as usize]
                .copy_from_slice(&Self::words_to_bytes(words));
        }
        bytes
    }

}
