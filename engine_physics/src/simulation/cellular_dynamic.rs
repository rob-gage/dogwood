// Copyright Rob Gage 2026

use crate::tiles::TileCoordinates;
use engine_compute::{
    Accelerator,
    AcceleratorBuffer,
};

/// The maximum number of grid steps one particle may travel during a fixed tick
const MAXIMUM_MOVEMENT_CELLS: u32 = 4;

/// Simulates dynamic cellular material in the canonical cellular buffers
pub struct CellularDynamic {
    /// Persistent velocity and subcell residual attached to each physical cell slot
    kinematics: AcceleratorBuffer,
    /// Resolved material identifiers before they are committed to canonical storage
    material_identifiers_output: AcceleratorBuffer,
    /// Resolved persistent appearances before they are committed to canonical storage
    appearances_output: AcceleratorBuffer,
    /// One atomic deterministic-priority claim per possible destination
    destination_claims: AcceleratorBuffer,
    /// One proposed destination and integrated kinematic state per source cell
    proposals: AcceleratorBuffer,
    /// Buffered/active ring mapping, gravity, delta time, and tick
    parameters: wgpu::Buffer,
    /// All concrete cellular dynamic input, scratch, and output bindings
    bind_group: wgpu::BindGroup,
    /// Clears destination claims before proposals are submitted
    clear_claims_pipeline: wgpu::ComputePipeline,
    /// Integrates dynamic cells, traces paths, and submits destination claims
    propose_pipeline: wgpu::ComputePipeline,
    /// Resolves claims without mutating the immutable tick-start inputs
    resolve_pipeline: wgpu::ComputePipeline,
    /// Number of cells in the full buffered tile ring
    buffered_cell_count: u32,
    /// Deterministic priority seed advanced once per active fixed tick
    tick: u32,
}

impl CellularDynamic {

    pub const fn kinematics_buffer(&self) -> &AcceleratorBuffer { &self.kinematics }

    /// Creates the private cellular dynamic solver and its fixed-size GPU state
    pub fn new(
        accelerator: &Accelerator,
        cellular_material_identifiers: &AcceleratorBuffer,
        cellular_appearances: &AcceleratorBuffer,
        external_body_occupancy: &AcceleratorBuffer,
        buffered_tile_count: usize,
    ) -> Self {
        // validate the cell count used by buffer indices and reversible claim tickets
        let buffered_cell_count: u32 = u32::try_from(buffered_tile_count * 64)
            .expect("Cellular dynamic buffer exceeds GPU indexing range");
        assert!(buffered_cell_count <= u32::MAX / 2);
        let device: &wgpu::Device = accelerator.wgpu_device();
        // allocate persistent motion state and separate per-tick output/scratch storage
        let kinematics: AcceleratorBuffer =
            accelerator.allocate::<[f32; 4]>(buffered_cell_count as usize);
        let material_identifiers_output: AcceleratorBuffer =
            accelerator.allocate::<u32>(buffered_cell_count as usize);
        let appearances_output: AcceleratorBuffer =
            accelerator.allocate::<u32>(buffered_cell_count as usize);
        let destination_claims: AcceleratorBuffer =
            accelerator.allocate::<u32>(buffered_cell_count as usize);
        let proposals: AcceleratorBuffer =
            accelerator.allocate::<[u32; 8]>(buffered_cell_count as usize);
        // describe the active and buffered ring mapping shared by every pass
        let parameters: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cellular dynamic parameters"),
            size: 96,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group_layout: wgpu::BindGroupLayout = device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("cellular dynamic bind group layout"),
                entries: &[
                    Self::cellular_dynamic_storage_layout_entry(0, true),
                    Self::cellular_dynamic_storage_layout_entry(1, true),
                    Self::cellular_dynamic_storage_layout_entry(2, false),
                    Self::cellular_dynamic_storage_layout_entry(3, false),
                    Self::cellular_dynamic_storage_layout_entry(4, false),
                    Self::cellular_dynamic_storage_layout_entry(5, false),
                    Self::cellular_dynamic_storage_layout_entry(6, false),
                    wgpu::BindGroupLayoutEntry {
                        binding: 7,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    Self::cellular_dynamic_storage_layout_entry(8, true),
                ],
            },
        );
        // bind immutable canonical inputs separately from resolved outputs and transient state
        let bind_group: wgpu::BindGroup = device.create_bind_group(
            &wgpu::BindGroupDescriptor {
                label: Some("cellular dynamic bind group"),
                layout: &bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: cellular_material_identifiers.wgpu_buffer().as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 8,
                        resource: external_body_occupancy.wgpu_buffer().as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: cellular_appearances.wgpu_buffer().as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: kinematics.wgpu_buffer().as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: material_identifiers_output.wgpu_buffer().as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: appearances_output.wgpu_buffer().as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: destination_claims.wgpu_buffer().as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: proposals.wgpu_buffer().as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 7,
                        resource: parameters.as_entire_binding(),
                    },
                ],
            },
        );
        // build the three explicit pass pipelines from one cellular dynamic shader
        let shader: wgpu::ShaderModule = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cellular dynamic shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("cellular_dynamic.wgsl").into()),
        });
        let pipeline_layout: wgpu::PipelineLayout = device.create_pipeline_layout(
            &wgpu::PipelineLayoutDescriptor {
                label: Some("cellular dynamic pipeline layout"),
                bind_group_layouts: &[Some(&bind_group_layout)],
                immediate_size: 0,
            },
        );
        let clear_claims_pipeline: wgpu::ComputePipeline =
            Self::create_cellular_dynamic_compute_pipeline(
                device,
                &pipeline_layout,
                &shader,
                "cellular dynamic clear claims pipeline",
                "clear_cellular_dynamic_destination_claims",
            );
        let propose_pipeline: wgpu::ComputePipeline =
            Self::create_cellular_dynamic_compute_pipeline(
                device,
                &pipeline_layout,
                &shader,
                "cellular dynamic movement proposal pipeline",
                "calculate_cellular_dynamic_movement_proposals",
            );
        let resolve_pipeline: wgpu::ComputePipeline =
            Self::create_cellular_dynamic_compute_pipeline(
                device,
                &pipeline_layout,
                &shader,
                "cellular dynamic movement resolution pipeline",
                "resolve_cellular_dynamic_movement_proposals",
            );
        Self {
            kinematics,
            material_identifiers_output,
            appearances_output,
            destination_claims,
            proposals,
            parameters,
            bind_group,
            clear_claims_pipeline,
            propose_pipeline,
            resolve_pipeline,
            buffered_cell_count,
            tick: 0,
        }
    }

    /// Simulates one fixed tick and commits the resolved state to canonical storage
    pub fn simulate_cellular_dynamic_tick(
        &mut self,
        accelerator: &Accelerator,
        cellular_material_identifiers: &AcceleratorBuffer,
        cellular_appearances: &AcceleratorBuffer,
        buffered_origin: TileCoordinates,
        buffered_width: u16,
        buffered_height: u16,
        ring_offset_x: u16,
        ring_offset_y: u16,
        gravity: [f32; 2],
        delta_time: f32,
    ) {
        // encode signed origins, ring geometry, kinematics inputs, and deterministic tick state
        let parameters: [u32; 24] = [
            buffered_origin.x as u32,
            buffered_origin.y as u32,
            u32::from(buffered_width),
            u32::from(buffered_height),
            buffered_origin.x as u32,
            buffered_origin.y as u32,
            u32::from(buffered_width),
            u32::from(buffered_height),
            u32::from(ring_offset_x),
            u32::from(ring_offset_y),
            gravity[0].to_bits(),
            gravity[1].to_bits(),
            delta_time.to_bits(),
            self.tick,
            self.buffered_cell_count,
            MAXIMUM_MOVEMENT_CELLS,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
        ];
        let mut bytes: Vec<u8> = Vec::with_capacity(64);
        for value in parameters { bytes.extend_from_slice(&value.to_le_bytes()); }
        accelerator.wgpu_queue().write_buffer(&self.parameters, 0, &bytes);
        // order claim clearing, immutable proposals, and winner resolution as separate passes
        let mut encoder: wgpu::CommandEncoder = accelerator.wgpu_device().create_command_encoder(
            &wgpu::CommandEncoderDescriptor { label: Some("cellular dynamic simulation") },
        );
        let workgroup_count: u32 = self.buffered_cell_count.div_ceil(64);
        for (label, pipeline) in [
            ("cellular dynamic clear destination claims", &self.clear_claims_pipeline),
            ("cellular dynamic calculate proposals", &self.propose_pipeline),
            ("cellular dynamic resolve movement proposals", &self.resolve_pipeline),
        ] {
            let mut pass: wgpu::ComputePass<'_> =
                encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some(label),
                    timestamp_writes: None,
                });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups(workgroup_count, 1, 1);
        }
        // commit every resolved field together after no pass can observe partial movement
        let cell_field_size: u64 = u64::from(self.buffered_cell_count) * 4;
        encoder.copy_buffer_to_buffer(
            self.material_identifiers_output.wgpu_buffer(),
            0,
            cellular_material_identifiers.wgpu_buffer(),
            0,
            cell_field_size,
        );
        encoder.copy_buffer_to_buffer(
            self.appearances_output.wgpu_buffer(),
            0,
            cellular_appearances.wgpu_buffer(),
            0,
            cell_field_size,
        );
        // submit one ordered cellular dynamic tick and rotate the next claim priority seed
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
        self.tick = self.tick.wrapping_add(1);
    }

    /// Clears motion state replaced by a CPU cell edit or tile upload
    pub fn clear_cellular_dynamic_kinematics(
        &self,
        accelerator: &Accelerator,
        cell_start: usize,
        cell_count: usize,
    ) {
        accelerator.wgpu_queue().write_buffer(
            self.kinematics.wgpu_buffer(),
            cell_start as u64 * 16,
            &vec![0; cell_count * 16],
        );
    }

    /// Returns one storage-buffer layout entry used by every concrete pass
    fn cellular_dynamic_storage_layout_entry(
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

    /// Creates one entry-point-specific pipeline from the shared cellular dynamic shader
    fn create_cellular_dynamic_compute_pipeline(
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

impl Drop for CellularDynamic {

    /// Releases every GPU allocation owned exclusively by cellular dynamic simulation
    fn drop(&mut self) {
        self.kinematics.free();
        self.material_identifiers_output.free();
        self.appearances_output.free();
        self.destination_claims.free();
        self.proposals.free();
        self.parameters.destroy();
    }

}
