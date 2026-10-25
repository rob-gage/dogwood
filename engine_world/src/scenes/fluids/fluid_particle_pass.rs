// Copyright Rob Gage 2026

use super::{
    FluidActorContact,
    FluidParticleImport,
    FLUID_ITERATION_COUNT,
    FLUID_BIN_SIZE,
    FLUID_COHESION_NORMALIZATION_2D,
    FLUID_COHESION_SCALE,
    FLUID_KERNEL_RADIUS,
    FLUID_RASTER_SCALE,
    FLUID_SCAN_WORKGROUP_SIZE,
    FLUID_SUBSTEP_COUNT,
    FLUID_TIME_STEP,
    FLUID_ACTOR_CONTACT_SCALE,
    FLUID_VELOCITY_MAXIMUM,
    FLUID_WORKGROUP_SIZE,
    FLUID_RELAXATION,
};
use crate::{
    actors::{
        Actor,
        components::ActorCollisionComponent,
    },
    materials::{
        MaterialIdentifier,
        MaterialTable,
    },
    scenes::{
        cellular_tiles::{
            CellCoordinates,
            CellularTileBuffers,
            CellularTileRegion,
            CELLULAR_TILE_ACTOR_COUNT_MAXIMUM,
            CELLULAR_TILE_SIZE,
        },
        chunks::{
            ChunkCoordinates,
            ChunkFluidParticle,
            CHUNK_SIZE,
        },
        collisions::CollisionRigidBodyPass,
        SceneError,
    },
};
use engine_host::acceleration::{Accelerator, AcceleratorReadback, AcceleratorReadbackError};
use std::{
    borrow::Cow,
    collections::HashMap,
    sync::mpsc::{
        Receiver,
        Sender,
        TryRecvError,
        channel,
    },
};

/// Bytes between the dynamic uniforms for consecutive particle dispatches
const UNIFORM_STRIDE: u64 = 256;

/// Bytes in one dispatch's `FluidUniforms` record
const UNIFORM_BYTE_COUNT: u64 = 152;

/// Bytes in the packed fluid particle state record
const PARTICLE_BYTE_COUNT: u64 = 32;

/// Bytes in one exact-position particle import command
const IMPORT_BYTE_COUNT: u64 = 32;

/// Words in one exported particle record, excluding the leading count
const EXPORT_WORD_COUNT: u64 = 10;

/// Bytes in one compact neighbor record: old position, cohesion, pressure, and sleep flag
const NEIGHBOR_DATA_BYTE_COUNT: u64 = 20;

/// Bytes in one GPU fluid edit command
const EDIT_BYTE_COUNT: u64 = 16;

/// Bytes in one actor contact record, including impulse and fluid sample values
const ACTOR_CONTACT_BYTE_COUNT: u64 = 32;

/// Bytes in one shared cellular tile actor record
const ACTOR_BYTE_COUNT: u64 = 64;

/// Number of readbacks that can be in flight for live particle counts
const READBACK_COUNT: usize = 3;

/// First uniform slot after the edit, occupancy, and boundary preparation slots
const SUBSTEP_UNIFORM_START: usize = 4;

/// Number of per-substep uniform slots for prediction, iterations, and commit
const SUBSTEP_UNIFORM_COUNT: usize = 5 + FLUID_ITERATION_COUNT * 2;

/// Total number of dynamic uniform records
const UNIFORM_COUNT: usize = SUBSTEP_UNIFORM_START
    + FLUID_SUBSTEP_COUNT * SUBSTEP_UNIFORM_COUNT;

/// One fluid edit applied when the solver next simulates
#[derive(Copy, Clone)]
pub(super) struct FluidEdit {
    /// Material identifier spawned at the edited cell
    pub(super) material_identifier: MaterialIdentifier,
    /// One for Insert, zero for Replace
    pub(super) mode: u32,
}

/// GPU storage and compute dispatches for the scene's packed fluid particles
pub struct FluidParticlePass {
    /// Compute pipelines, named by their WGSL entry points
    pipelines: HashMap<&'static str, wgpu::ComputePipeline>,
    /// Uniforms, cell materials, fluid materials, particles, bins, and edit resources
    bind_group: wgpu::BindGroup,
    /// Bind group used only by `prepare` to write indirect dispatch arguments
    dispatch_bind_group: wgpu::BindGroup,
    /// Reduced bind group used by the prepare entry points
    prepare_bind_group: wgpu::BindGroup,
    /// Reduced bind group used to prepare sparse raster-tile clearing
    prepare_raster_bind_group: wgpu::BindGroup,
    /// Layout for preprocessing moving boundary-cell velocities
    boundary_motion_bind_group_layout: wgpu::BindGroupLayout,
    /// Boundary motion resources shared with cellular kinematics and rigid-body rasterization
    boundary_motion_bind_group: Option<wgpu::BindGroup>,
    /// Actor records copied from the cellular tile simulation for projection and immersion sampling
    actor_buffer: wgpu::Buffer,
    /// Dedicated fixed-point actor reactions and immersion samples
    actor_contact_buffer: wgpu::Buffer,
    /// Bind group for actor projection and sleeping-tile wake passes
    actor_projection_bind_group: wgpu::BindGroup,
    /// Bind group for actor immersion and current sampling
    actor_sample_bind_group: wgpu::BindGroup,
    /// Bind group for preserving actor-normal velocity after the particle speed clamp
    actor_commit_bind_group: wgpu::BindGroup,
    /// Spawn commands uploaded when queued fluid edits are applied
    edit_buffer: wgpu::Buffer,
    /// Exact-position chunk particles uploaded before the solver pass
    import_buffer: wgpu::Buffer,
    /// One rejection flag per exact-position import
    import_result_buffer: wgpu::Buffer,
    /// World-cell particle records written by compaction
    export_buffer: wgpu::Buffer,
    /// Double-buffered packed fluid particle state
    particle_state_a: wgpu::Buffer,
    /// Second side of the packed fluid particle state
    particle_state_b: wgpu::Buffer,
    /// Per-physical-tile masks selecting particles erased by cell edits
    erasure_mask_buffer: wgpu::Buffer,
    /// Physical cell raster followed by sparse tile flags, list, and count
    fluid_raster_buffer: wgpu::Buffer,
    /// Per-cell Akinci weights and moving-boundary velocity
    boundary_psi_buffer: wgpu::Buffer,
    /// Sparse boundary tile list and packed solid-cell occupancy
    boundary_tile_data_buffer: wgpu::Buffer,
    /// Dynamic uniform records, one per particle pass in a tick
    uniform_buffer: wgpu::Buffer,
    /// GPU live counts, tile-list count, and Insert occupancy generations
    control_buffer: wgpu::Buffer,
    /// Indirect arguments written by `prepare` and consumed by particle dispatches
    dispatch_buffer: wgpu::Buffer,
    /// GPU mappable copies of the live count and their submitted edit revisions
    count_readbacks: Vec<(wgpu::Buffer, Option<(u64, u64)>)>,
    /// Sender for asynchronous count mapping results
    response_sender: Sender<(usize, Result<(), wgpu::BufferAsyncError>)>,
    /// Asynchronous count mapping results
    response_receiver: Receiver<(usize, Result<(), wgpu::BufferAsyncError>)>,
    /// Mappable actor contact copies paired with their tick and uploaded actor identities
    actor_contact_readbacks: AcceleratorReadback<(u64, Vec<(Actor, ActorCollisionComponent)>)>,
    /// Import status readbacks paired with source records
    import_readbacks: Vec<(wgpu::Buffer, Option<Vec<FluidParticleImport>>)>,
    /// Sender for imported particle status mappings
    import_response_sender: Sender<(usize, Result<(), wgpu::BufferAsyncError>)>,
    /// Imported particle status mapping responses
    import_response_receiver: Receiver<(usize, Result<(), wgpu::BufferAsyncError>)>,
    /// Mappable snapshot of the live pool
    snapshot_readback: wgpu::Buffer,
    /// World origin paired with a pending snapshot
    snapshot_origin: Option<(i64, i64)>,
    /// Sender for snapshot mapping response
    snapshot_response_sender: Sender<Result<(), wgpu::BufferAsyncError>>,
    /// Snapshot mapping response receiver
    snapshot_response_receiver: Receiver<Result<(), wgpu::BufferAsyncError>>,
    /// Particle capacity fixed when the cellular tile buffers were created
    capacity: u32,
    /// Number of allocated dense neighbor bins
    bin_count: u32,
    /// Current side of the double-buffered particle state
    state_side: u32,
}

impl FluidParticlePass {

    /// Creates fixed-capacity particle, scratch, bin, and readback buffers
    pub fn new(
        accelerator: &Accelerator,
        cellular_tile_buffers: &CellularTileBuffers,
        material_table: &MaterialTable,
        capacity: u32,
        region_width: u32,
        region_height: u32,
    ) -> Self {
        let device: &wgpu::Device = accelerator.device();
        let cell_width: u32 = region_width * CELLULAR_TILE_SIZE as u32;
        let cell_height: u32 = region_height * CELLULAR_TILE_SIZE as u32;
        let bin_width: u32 = (cell_width as f32 / FLUID_BIN_SIZE).ceil() as u32;
        let bin_height: u32 = (cell_height as f32 / FLUID_BIN_SIZE).ceil() as u32;
        let bin_count: u32 = bin_width * bin_height;
        let scan_block_count: u32 = bin_count.div_ceil(FLUID_SCAN_WORKGROUP_SIZE);
        let scan_chunk_count: u32 = scan_block_count.div_ceil(FLUID_SCAN_WORKGROUP_SIZE);
        let cell_count: u64 = u64::from(cell_width) * u64::from(cell_height);
        let tile_count: u64 = u64::from(region_width) * u64::from(region_height);
        let bin_data_word_count: u64 = u64::from(bin_count) * 3
            + u64::from(scan_block_count) * 2
            + u64::from(scan_chunk_count) * 2;
        let storage_entry = |binding: u32, read_only: bool| -> wgpu::BindGroupLayoutEntry {
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
        let uniform_entry = || -> wgpu::BindGroupLayoutEntry {
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(UNIFORM_BYTE_COUNT),
                },
                count: None,
            }
        };
        let bind_group_layout: wgpu::BindGroupLayout = device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("`FluidParticlePass.bind_group_layout`"),
                entries: &[
                    uniform_entry(),
                    storage_entry(1, true),
                    storage_entry(2, true),
                    storage_entry(3, false),
                    storage_entry(4, false),
                    storage_entry(5, false),
                    storage_entry(6, false),
                    storage_entry(7, false),
                    storage_entry(8, false),
                    storage_entry(9, false),
                    storage_entry(10, false),
                    storage_entry(11, true),
                    storage_entry(12, true),
                    storage_entry(13, false),
                    storage_entry(14, false),
                    storage_entry(15, false),
                    storage_entry(16, false),
                    storage_entry(21, true),
                    storage_entry(25, true),
                    storage_entry(26, false),
                    storage_entry(27, false),
                ],
            },
        );
        let boundary_motion_bind_group_layout: wgpu::BindGroupLayout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("`FluidParticlePass.boundary_motion_bind_group_layout`"),
                entries: &[
                    uniform_entry(),
                    storage_entry(1, true),
                    storage_entry(8, false),
                    storage_entry(10, false),
                    storage_entry(13, false),
                    storage_entry(18, true),
                    storage_entry(19, true),
                    storage_entry(20, true),
                    storage_entry(22, true),
                ],
            });
        let actor_projection_bind_group_layout: wgpu::BindGroupLayout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("`FluidParticlePass.actor_projection_bind_group_layout`"),
                entries: &[
                    uniform_entry(),
                    storage_entry(1, true),
                    storage_entry(2, true),
                    storage_entry(3, false),
                    storage_entry(4, false),
                    storage_entry(5, false),
                    storage_entry(6, false),
                    storage_entry(8, false),
                    storage_entry(10, false),
                    storage_entry(13, false),
                    storage_entry(23, true),
                    storage_entry(24, false),
                ],
            });
        let actor_sample_bind_group_layout: wgpu::BindGroupLayout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("`FluidParticlePass.actor_sample_bind_group_layout`"),
                entries: &[
                    uniform_entry(),
                    storage_entry(1, true),
                    storage_entry(2, true),
                    storage_entry(16, false),
                    storage_entry(18, true),
                    storage_entry(23, true),
                    storage_entry(24, false),
                ],
            });
        let actor_commit_bind_group_layout: wgpu::BindGroupLayout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("`FluidParticlePass.actor_commit_bind_group_layout`"),
                entries: &[
                    uniform_entry(),
                    storage_entry(2, true),
                    storage_entry(3, false),
                    storage_entry(4, false),
                    storage_entry(10, false),
                    storage_entry(23, true),
                    storage_entry(24, false),
                ],
            });
        let prepare_bind_group_layout: wgpu::BindGroupLayout = device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("`FluidParticlePass.prepare_bind_group_layout`"),
                entries: &[uniform_entry(), storage_entry(10, false)],
            },
        );
        let prepare_raster_bind_group_layout: wgpu::BindGroupLayout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("`FluidParticlePass.prepare_raster_bind_group_layout`"),
                entries: &[uniform_entry(), storage_entry(16, false)],
            });
        let dispatch_bind_group_layout: wgpu::BindGroupLayout = device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("`FluidParticlePass.dispatch_bind_group_layout`"),
                entries: &[storage_entry(0, false)],
            },
        );
        let pipeline_layout: wgpu::PipelineLayout = device.create_pipeline_layout(
            &wgpu::PipelineLayoutDescriptor {
                label: Some("`FluidParticlePass.pipeline_layout`"),
                bind_group_layouts: &[Some(&bind_group_layout)],
                immediate_size: 0,
            },
        );
        let prepare_pipeline_layout: wgpu::PipelineLayout = device.create_pipeline_layout(
            &wgpu::PipelineLayoutDescriptor {
                label: Some("`FluidParticlePass.prepare_pipeline_layout`"),
                bind_group_layouts: &[
                    Some(&prepare_bind_group_layout),
                    Some(&dispatch_bind_group_layout),
                ],
                immediate_size: 0,
            },
        );
        let prepare_raster_pipeline_layout: wgpu::PipelineLayout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("`FluidParticlePass.prepare_raster_pipeline_layout`"),
                bind_group_layouts: &[
                    Some(&prepare_raster_bind_group_layout),
                    Some(&dispatch_bind_group_layout),
                ],
                immediate_size: 0,
            });
        let boundary_motion_pipeline_layout: wgpu::PipelineLayout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("`FluidParticlePass.boundary_motion_pipeline_layout`"),
                bind_group_layouts: &[Some(&boundary_motion_bind_group_layout)],
                immediate_size: 0,
            });
        let actor_projection_pipeline_layout: wgpu::PipelineLayout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("`FluidParticlePass.actor_projection_pipeline_layout`"),
                bind_group_layouts: &[Some(&actor_projection_bind_group_layout)],
                immediate_size: 0,
            });
        let actor_sample_pipeline_layout: wgpu::PipelineLayout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("`FluidParticlePass.actor_sample_pipeline_layout`"),
                bind_group_layouts: &[Some(&actor_sample_bind_group_layout)],
                immediate_size: 0,
            });
        let actor_commit_pipeline_layout: wgpu::PipelineLayout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("`FluidParticlePass.actor_commit_pipeline_layout`"),
                bind_group_layouts: &[Some(&actor_commit_bind_group_layout)],
                immediate_size: 0,
            });
        let shader_source: String = include_str!("fluid_particle.wgsl")
            .replace("__FLUID_BIN_SIZE__", &FLUID_BIN_SIZE.to_string());
        let shader_module: wgpu::ShaderModule = device.create_shader_module(
            wgpu::ShaderModuleDescriptor {
                label: Some("`FluidParticlePass.shader_module`"),
                source: wgpu::ShaderSource::Wgsl(Cow::Owned(shader_source)),
            },
        );
        let mut pipelines: HashMap<&'static str, wgpu::ComputePipeline> =
            HashMap::new();
        for entry_point in [
            "prepare",
            "prepare_boundary",
            "prepare_raster",
            "prepare_fluid_contacts",
            "compact",
            "finish_compaction",
            "begin_occupancy",
            "spawn",
            "spawn_imported",
            "mark_occupancy",
            "mark_boundary_tiles",
            "dilate_boundary_tiles",
            "clear_boundary_tiles",
            "reset_boundary_tile_count",
            "clear_fluid_raster_tiles",
            "reset_fluid_raster_tile_count",
            "clear_fluid_current_tiles",
            "rasterize_fluid",
            "actor_contacts",
            "boundary",
            "boundary_weights",
            "boundary_motion",
            "boundary_nodes",
            "apply_fluid_contacts",
            "clear_fluid_contacts",
            "reset_fluid_contact_tile_count",
            "predict",
            "project_actors",
            "prepare_sleep",
            "wake_actor_neighbors",
            "wake_solid_neighbors",
            "finish_sleep",
            "reset_wake_count",
            "apply_pending_wakes",
            "prepare_wake_dispatch",
            "clear_bins",
            "count_bins",
            "scan_bins",
            "scan_blocks",
            "scan_block_prefix",
            "add_block_prefix",
            "scatter",
            "lambda",
            "correct",
            "commit",
            "xsph",
            "apply_xsph",
            "commit_actor_velocity",
        ] {
            let layout: &wgpu::PipelineLayout = if entry_point == "boundary_motion" {
                &boundary_motion_pipeline_layout
            } else if entry_point == "project_actors" || entry_point == "wake_actor_neighbors" {
                &actor_projection_pipeline_layout
            } else if entry_point == "actor_contacts" {
                &actor_sample_pipeline_layout
            } else if entry_point == "commit_actor_velocity" {
                &actor_commit_pipeline_layout
            } else if entry_point == "prepare_raster" || entry_point == "prepare_fluid_contacts" {
                &prepare_raster_pipeline_layout
            } else if entry_point == "prepare"
                || entry_point == "prepare_boundary"
                || entry_point == "prepare_wake_dispatch"
            {
                &prepare_pipeline_layout
            } else {
                &pipeline_layout
            };
            pipelines.insert(entry_point, device.create_compute_pipeline(
                &wgpu::ComputePipelineDescriptor {
                    label: Some("`FluidParticlePass.pipelines`"),
                    layout: Some(layout),
                    module: &shader_module,
                    entry_point: Some(entry_point),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    cache: None,
                },
            ));
        }
        let create_buffer = |label: &'static str, size: u64, usage: wgpu::BufferUsages| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: size.max(16),
                usage,
                mapped_at_creation: false,
            })
        };
        let uniform_buffer: wgpu::Buffer = create_buffer(
            "`FluidParticlePass.uniform_buffer`",
            UNIFORM_STRIDE * UNIFORM_COUNT as u64,
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        );
        let particle_state_a: wgpu::Buffer = create_buffer(
            "`FluidParticlePass.particle_state_a`",
            u64::from(capacity) * PARTICLE_BYTE_COUNT,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        );
        let particle_state_b: wgpu::Buffer = create_buffer(
            "`FluidParticlePass.particle_state_b`",
            u64::from(capacity) * PARTICLE_BYTE_COUNT,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        );
        let predicted_a_buffer: wgpu::Buffer = create_buffer(
            "`FluidParticlePass.predicted_a_buffer`",
            u64::from(capacity) * 8,
            wgpu::BufferUsages::STORAGE,
        );
        let predicted_b_buffer: wgpu::Buffer = create_buffer(
            "`FluidParticlePass.predicted_b_buffer`",
            u64::from(capacity) * 8,
            wgpu::BufferUsages::STORAGE,
        );
        let neighbor_data_buffer: wgpu::Buffer = create_buffer(
            "`FluidParticlePass.neighbor_data_buffer`",
            u64::from(capacity) * NEIGHBOR_DATA_BYTE_COUNT,
            wgpu::BufferUsages::STORAGE,
        );
        let lambda_buffer: wgpu::Buffer = create_buffer(
            "`FluidParticlePass.lambda_buffer`",
            u64::from(capacity) * 4,
            wgpu::BufferUsages::STORAGE,
        );
        let boundary_psi_buffer: wgpu::Buffer = create_buffer(
            "`FluidParticlePass.boundary_psi_buffer`",
            cell_count * 16,
            wgpu::BufferUsages::STORAGE,
        );
        let boundary_node_buffer: wgpu::Buffer = create_buffer(
            "`FluidParticlePass.boundary_node_buffer`",
            u64::from(cell_width + 1) * u64::from(cell_height + 1) * 16,
            wgpu::BufferUsages::STORAGE,
        );
        let bin_data_buffer: wgpu::Buffer = create_buffer(
            "`FluidParticlePass.bin_data_buffer`",
            bin_data_word_count * 4,
            wgpu::BufferUsages::STORAGE,
        );
        let control_buffer: wgpu::Buffer = create_buffer(
            "`FluidParticlePass.control_buffer`",
            20 + cell_count * 4 + tile_count * 28,
            wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
        );
        let dispatch_buffer: wgpu::Buffer = create_buffer(
            "`FluidParticlePass.dispatch_buffer`",
            24,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::INDIRECT,
        );
        let edit_buffer: wgpu::Buffer = create_buffer(
            "`FluidParticlePass.edit_buffer`",
            cell_count * EDIT_BYTE_COUNT,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        );
        let import_buffer: wgpu::Buffer = create_buffer(
            "`FluidParticlePass.import_buffer`",
            u64::from(capacity) * IMPORT_BYTE_COUNT,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        );
        let import_result_buffer: wgpu::Buffer = create_buffer(
            "`FluidParticlePass.import_result_buffer`",
            u64::from(capacity) * 4,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        );
        let export_buffer: wgpu::Buffer = create_buffer(
            "`FluidParticlePass.export_buffer`",
            4 + u64::from(capacity) * EXPORT_WORD_COUNT * 4,
            wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
        );
        let erasure_mask_buffer: wgpu::Buffer = create_buffer(
            "`FluidParticlePass.erasure_mask_buffer`",
            tile_count * 8,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        );
        let fluid_raster_buffer: wgpu::Buffer = create_buffer(
            "`FluidParticlePass.fluid_raster_buffer`",
            cell_count * 24 + (tile_count * 4 + 2) * 4,
            wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
        );
        let boundary_tile_data_buffer: wgpu::Buffer = create_buffer(
            "`FluidParticlePass.boundary_tile_data_buffer`",
            (tile_count * 2 + cell_count) * 4,
            wgpu::BufferUsages::STORAGE,
        );
        let actor_buffer: wgpu::Buffer = create_buffer(
            "`FluidParticlePass.actor_buffer`",
            CELLULAR_TILE_ACTOR_COUNT_MAXIMUM as u64 * ACTOR_BYTE_COUNT,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        );
        let actor_contact_buffer: wgpu::Buffer = create_buffer(
            "`FluidParticlePass.actor_contact_buffer`",
            CELLULAR_TILE_ACTOR_COUNT_MAXIMUM as u64 * ACTOR_CONTACT_BYTE_COUNT,
            wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
        );
        let bind_group: wgpu::BindGroup = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("`FluidParticlePass.bind_group`"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &uniform_buffer,
                        offset: 0,
                        size: wgpu::BufferSize::new(UNIFORM_BYTE_COUNT),
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: cellular_tile_buffers.material_identifier_buffer()
                        .as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: material_table.fluid_properties_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry { binding: 3, resource: particle_state_a.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 4, resource: particle_state_b.as_entire_binding() },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: predicted_a_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: predicted_b_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry { binding: 7, resource: lambda_buffer.as_entire_binding() },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: boundary_psi_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry { binding: 9, resource: bin_data_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 10, resource: control_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 11, resource: edit_buffer.as_entire_binding() },
                wgpu::BindGroupEntry {
                    binding: 12,
                    resource: erasure_mask_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 13,
                    resource: boundary_tile_data_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 14,
                    resource: boundary_node_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 15,
                    resource: neighbor_data_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 16,
                    resource: fluid_raster_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 21,
                    resource: cellular_tile_buffers.pressure_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry { binding: 25, resource: import_buffer.as_entire_binding() },
                wgpu::BindGroupEntry {
                    binding: 26,
                    resource: import_result_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry { binding: 27, resource: export_buffer.as_entire_binding() },
            ],
        });
        let actor_projection_bind_group: wgpu::BindGroup = device.create_bind_group(
            &wgpu::BindGroupDescriptor {
                label: Some("`FluidParticlePass.actor_projection_bind_group`"),
                layout: &actor_projection_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &uniform_buffer,
                            offset: 0,
                            size: wgpu::BufferSize::new(UNIFORM_BYTE_COUNT),
                        }),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: cellular_tile_buffers.material_identifier_buffer()
                            .as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: material_table.fluid_properties_buffer().as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: particle_state_a.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: particle_state_b.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: predicted_a_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: predicted_b_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 8,
                        resource: boundary_psi_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 10,
                        resource: control_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 13,
                        resource: boundary_tile_data_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 23,
                        resource: actor_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 24,
                        resource: actor_contact_buffer.as_entire_binding(),
                    },
                ],
            },
        );
        let actor_sample_bind_group: wgpu::BindGroup = device.create_bind_group(
            &wgpu::BindGroupDescriptor {
                label: Some("`FluidParticlePass.actor_sample_bind_group`"),
                layout: &actor_sample_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &uniform_buffer,
                            offset: 0,
                            size: wgpu::BufferSize::new(UNIFORM_BYTE_COUNT),
                        }),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: cellular_tile_buffers.material_identifier_buffer()
                            .as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: material_table.fluid_properties_buffer().as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 16,
                        resource: fluid_raster_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 18,
                        resource: cellular_tile_buffers.rigid_body_cell_buffer()
                            .as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 23,
                        resource: actor_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 24,
                        resource: actor_contact_buffer.as_entire_binding(),
                    },
                ],
            },
        );
        let actor_commit_bind_group: wgpu::BindGroup = device.create_bind_group(
            &wgpu::BindGroupDescriptor {
                label: Some("`FluidParticlePass.actor_commit_bind_group`"),
                layout: &actor_commit_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &uniform_buffer,
                            offset: 0,
                            size: wgpu::BufferSize::new(UNIFORM_BYTE_COUNT),
                        }),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: material_table.fluid_properties_buffer().as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: particle_state_a.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: particle_state_b.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 10,
                        resource: control_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 23,
                        resource: actor_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 24,
                        resource: actor_contact_buffer.as_entire_binding(),
                    },
                ],
            },
        );
        let prepare_bind_group: wgpu::BindGroup = device.create_bind_group(
            &wgpu::BindGroupDescriptor {
                label: Some("`FluidParticlePass.prepare_bind_group`"),
                layout: &prepare_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &uniform_buffer,
                            offset: 0,
                        size: wgpu::BufferSize::new(UNIFORM_BYTE_COUNT),
                        }),
                    },
                    wgpu::BindGroupEntry {
                        binding: 10,
                        resource: control_buffer.as_entire_binding(),
                    },
                ],
            },
        );
        let prepare_raster_bind_group: wgpu::BindGroup = device.create_bind_group(
            &wgpu::BindGroupDescriptor {
                label: Some("`FluidParticlePass.prepare_raster_bind_group`"),
                layout: &prepare_raster_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &uniform_buffer,
                            offset: 0,
                        size: wgpu::BufferSize::new(UNIFORM_BYTE_COUNT),
                        }),
                    },
                    wgpu::BindGroupEntry {
                        binding: 16,
                        resource: fluid_raster_buffer.as_entire_binding(),
                    },
                ],
            },
        );
        let dispatch_bind_group: wgpu::BindGroup = device.create_bind_group(
            &wgpu::BindGroupDescriptor {
                label: Some("`FluidParticlePass.dispatch_bind_group`"),
                layout: &dispatch_bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: dispatch_buffer.as_entire_binding(),
                }],
            },
        );
        let (response_sender, response_receiver): (
            Sender<(usize, Result<(), wgpu::BufferAsyncError>)>,
            Receiver<(usize, Result<(), wgpu::BufferAsyncError>)>,
        ) = channel();
        let count_readbacks: Vec<(wgpu::Buffer, Option<(u64, u64)>)> =
            (0..READBACK_COUNT).map(|_| (
                create_buffer(
                    "`FluidParticlePass.count_readbacks`",
                    16,
                    wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                ),
                None,
            )).collect();
        let actor_contact_readbacks: AcceleratorReadback<
            (u64, Vec<(Actor, ActorCollisionComponent)>),
        > = AcceleratorReadback::new(
            accelerator,
            "`FluidParticlePass.actor_contact_readbacks`",
            actor_contact_buffer.size(),
            READBACK_COUNT,
        );
        let (import_response_sender, import_response_receiver): (
            Sender<(usize, Result<(), wgpu::BufferAsyncError>)>,
            Receiver<(usize, Result<(), wgpu::BufferAsyncError>)>,
        ) = channel();
        let import_readbacks: Vec<(wgpu::Buffer, Option<Vec<FluidParticleImport>>)> =
            (0..READBACK_COUNT).map(|_| (
                create_buffer("`FluidParticlePass.import_readbacks`", u64::from(capacity) * 4,
                    wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST),
                None,
            )).collect();
        let snapshot_readback: wgpu::Buffer = create_buffer(
            "`FluidParticlePass.snapshot_readback`",
            4 + u64::from(capacity) * PARTICLE_BYTE_COUNT,
            wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        );
        let (snapshot_response_sender, snapshot_response_receiver): (
            Sender<Result<(), wgpu::BufferAsyncError>>,
            Receiver<Result<(), wgpu::BufferAsyncError>>,
        ) = channel();
        Self {
            pipelines,
            bind_group,
            dispatch_bind_group,
            prepare_bind_group,
            prepare_raster_bind_group,
            boundary_motion_bind_group_layout,
            boundary_motion_bind_group: None,
            actor_buffer,
            actor_contact_buffer,
            actor_projection_bind_group,
            actor_sample_bind_group,
            actor_commit_bind_group,
            edit_buffer,
            import_buffer,
            import_result_buffer,
            export_buffer,
            particle_state_a,
            particle_state_b,
            erasure_mask_buffer,
            fluid_raster_buffer,
            boundary_psi_buffer,
            boundary_tile_data_buffer,
            uniform_buffer,
            control_buffer,
            dispatch_buffer,
            count_readbacks,
            response_sender,
            response_receiver,
            actor_contact_readbacks,
            import_readbacks,
            import_response_sender,
            import_response_receiver,
            snapshot_readback,
            snapshot_origin: None,
            snapshot_response_sender,
            snapshot_response_receiver,
            capacity,
            bin_count,
            state_side: 0,
        }
    }

    /// Returns the fixed byte size of one particle export readback
    pub fn particle_export_buffer_size(&self) -> u64 { self.export_buffer.size() }

    /// Returns whether an import status slot is available
    pub(super) fn can_submit_particle_imports(&self) -> bool {
        self.import_readbacks.iter().any(|(_, submission)| submission.is_none())
    }

    /// Returns whether imported particle statuses remain pending
    pub(super) fn has_pending_particle_imports(&self) -> bool {
        self.import_readbacks.iter().any(|(_, submission)| submission.is_some())
    }

    /// Copies the live pool asynchronously for a resident save
    pub(super) fn request_particle_snapshot(
        &mut self,
        accelerator: &Accelerator,
        origin: (i64, i64),
    ) -> bool {
        if self.snapshot_origin.is_some() { return false; }
        let state_buffer: &wgpu::Buffer = if self.state_side == 0 {
            &self.particle_state_a
        } else { &self.particle_state_b };
        let mut command_encoder: wgpu::CommandEncoder = accelerator.device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Fluid particle snapshot"),
            });
        command_encoder.copy_buffer_to_buffer(
            &self.control_buffer,
            0,
            &self.snapshot_readback,
            0,
            4,
        );
        if self.capacity > 0 {
            command_encoder.copy_buffer_to_buffer(
                state_buffer,
                0,
                &self.snapshot_readback,
                4,
                state_buffer.size(),
            );
        }
        accelerator.queue().submit(Some(command_encoder.finish()));
        self.snapshot_origin = Some(origin);
        let response_sender: Sender<Result<(), wgpu::BufferAsyncError>> =
            self.snapshot_response_sender.clone();
        self.snapshot_readback.map_async(
            wgpu::MapMode::Read,
            ..,
            move |result: Result<(), wgpu::BufferAsyncError>| {
                let _ = response_sender.send(result);
            },
        );
        true
    }

    /// Returns a completed resident snapshot
    pub(super) fn read_particle_snapshot(
        &mut self,
        accelerator: &Accelerator,
    ) -> Result<Option<Vec<(ChunkCoordinates, ChunkFluidParticle)>>, SceneError> {
        accelerator.device().poll(wgpu::PollType::Poll).map_err(|_| SceneError::FluidParticlePoll)?;
        let result: Result<(), wgpu::BufferAsyncError> =
            match self.snapshot_response_receiver.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return Ok(None),
            Err(TryRecvError::Disconnected) => {
                return Err(SceneError::FluidParticleSnapshotResponseChannelClosed);
            }
        };
        result.map_err(|error| SceneError::FluidParticleResponse { error })?;
        let origin: (i64, i64) = self.snapshot_origin.take()
            .ok_or(SceneError::FluidParticleExportRecordInvalid)?;
        let particles: Result<Vec<(ChunkCoordinates, ChunkFluidParticle)>, SceneError> = {
            let bytes: wgpu::BufferView = self.snapshot_readback.get_mapped_range(..)
                .expect("mapped fluid particle snapshot should be readable");
            decode_live_particle_snapshot(&bytes, origin, self.capacity)
        };
        self.snapshot_readback.unmap();
        particles.map(Some)
    }

    /// Reads the live particle pool synchronously for scene teardown
    pub(super) fn read_live_particles_blocking(
        &self,
        accelerator: &Accelerator,
        origin: (i64, i64),
    ) -> Result<Vec<(ChunkCoordinates, ChunkFluidParticle)>, SceneError> {
        let staging_buffer: wgpu::Buffer = accelerator.device()
            .create_buffer(&wgpu::BufferDescriptor {
                label: Some("Final fluid particle snapshot"),
                size: 4 + u64::from(self.capacity) * PARTICLE_BYTE_COUNT,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        let state_buffer: &wgpu::Buffer = if self.state_side == 0 {
            &self.particle_state_a
        } else { &self.particle_state_b };
        let mut command_encoder: wgpu::CommandEncoder = accelerator.device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Final fluid particle snapshot"),
            });
        command_encoder.copy_buffer_to_buffer(&self.control_buffer, 0, &staging_buffer, 0, 4);
        if self.capacity > 0 {
            command_encoder.copy_buffer_to_buffer(
                state_buffer,
                0,
                &staging_buffer,
                4,
                state_buffer.size(),
            );
        }
        accelerator.queue().submit(Some(command_encoder.finish()));
        let (response_sender, response_receiver): (
            Sender<Result<(), wgpu::BufferAsyncError>>,
            Receiver<Result<(), wgpu::BufferAsyncError>>,
        ) = channel();
        staging_buffer.map_async(
            wgpu::MapMode::Read,
            ..,
            move |result: Result<(), wgpu::BufferAsyncError>| {
                let _ = response_sender.send(result);
            },
        );
        accelerator.device().poll(wgpu::PollType::wait_indefinitely())
            .map_err(|_| SceneError::FluidParticlePoll)?;
        response_receiver.recv()
            .map_err(|_| SceneError::FluidParticleSnapshotResponseChannelClosed)?
            .map_err(|error| SceneError::FluidParticleResponse { error })?;
        let particles: Result<Vec<(ChunkCoordinates, ChunkFluidParticle)>, SceneError> = {
            let bytes: wgpu::BufferView = staging_buffer.get_mapped_range(..)
                .expect("mapped final fluid particle snapshot should be readable");
            decode_live_particle_snapshot(&bytes, origin, self.capacity)
        };
        staging_buffer.unmap();
        particles
    }

    /// Returns whether a live snapshot is pending
    pub(super) fn has_pending_particle_snapshot(&self) -> bool { self.snapshot_origin.is_some() }

    /// Returns the physical ring-cell raster of mass, momentum, and material identifiers
    pub fn fluid_raster_buffer(&self) -> &wgpu::Buffer { &self.fluid_raster_buffer }

    /// Returns every actor sample whose asynchronous mapping completed
    pub(super) fn read_actor_contacts(
        &mut self,
        accelerator: &Accelerator,
    ) -> Result<Vec<(u64, Actor, ActorCollisionComponent, FluidActorContact)>, SceneError> {
        let mut contacts: Vec<(u64, Actor, ActorCollisionComponent, FluidActorContact)> =
            Vec::new();
        let read_result: Result<(), AcceleratorReadbackError> = self.actor_contact_readbacks.read(
            accelerator,
            true,
            |(tick, actors), mapped_bytes| {
                for ((actor, component), contact_bytes) in actors.iter().zip(
                    mapped_bytes.chunks_exact(ACTOR_CONTACT_BYTE_COUNT as usize),
                ) {
                    let value = |field: usize| -> f32 {
                        i32::from_le_bytes([
                            contact_bytes[field * 4],
                            contact_bytes[field * 4 + 1],
                            contact_bytes[field * 4 + 2],
                            contact_bytes[field * 4 + 3],
                        ]) as f32 / FLUID_ACTOR_CONTACT_SCALE
                    };
                    contacts.push((tick, *actor, component.clone(), FluidActorContact {
                        impulse: [value(0), value(1)],
                        immersion: value(2).clamp(0.0, 1.0),
                        fluid_velocity: [value(3), value(4)],
                        fluid_density: value(5).max(0.0),
                        fluid_drag: value(6).max(0.0),
                    }));
                }
            },
        );
        read_result.map_err(|error| match error {
            AcceleratorReadbackError::Poll(_) => SceneError::FluidParticlePoll,
            AcceleratorReadbackError::BufferAsync(error) =>
                SceneError::FluidParticleResponse { error },
        })?;
        Ok(contacts)
    }

    /// Rebases and edits particles, runs the PBF substeps, and submits a live-count readback
    pub fn simulate(
        &mut self,
        accelerator: &Accelerator,
        active_region: CellularTileRegion,
        ring_offset: (u32, u32),
        region_origin: (i64, i64),
        coupling_origin: (i32, i32),
        rebase: [f32; 2],
        gravity: f32,
        cellular_tile_buffers: &CellularTileBuffers,
        collision_rigid_body_pass: &CollisionRigidBodyPass,
        cellular_tile_actor_buffer: Option<&wgpu::Buffer>,
        fluid_properties_buffer: &wgpu::Buffer,
        actors: &[(Actor, ActorCollisionComponent)],
        edits: &[(CellCoordinates, FluidEdit)],
        erasures: &[CellCoordinates],
        imports: &[FluidParticleImport],
        export_target: Option<&wgpu::Buffer>,
        tick: u64,
        spawn_revision: u64,
    ) -> Result<(Option<(u32, u64, u64)>, Vec<FluidParticleImport>, usize), SceneError> {
        let newest_count: Option<(u32, u64, u64)> = self.read_count_readbacks(accelerator)?;
        let device: &wgpu::Device = accelerator.device();
        let width: u32 = active_region.width() as u32;
        let height: u32 = active_region.height() as u32;
        if width == 0 || height == 0 {
            return Err(SceneError::InvalidCellularTileRegion);
        }
        let cell_width: u32 = width * CELLULAR_TILE_SIZE as u32;
        let cell_height: u32 = height * CELLULAR_TILE_SIZE as u32;
        let bin_width: u32 = (cell_width as f32 / FLUID_BIN_SIZE).ceil() as u32;
        let bin_height: u32 = (cell_height as f32 / FLUID_BIN_SIZE).ceil() as u32;
        let bin_count: u32 = bin_width * bin_height;
        let scan_block_count: u32 = bin_count.div_ceil(FLUID_SCAN_WORKGROUP_SIZE);
        let fluid_material_count: u32 = (fluid_properties_buffer.size() / 32)
            .min(u64::from(u32::MAX)) as u32;
        if bin_count != self.bin_count { return Err(SceneError::InvalidCellularTileRegion); }
        let actors: &[(Actor, ActorCollisionComponent)] = if cellular_tile_actor_buffer.is_some() {
            &actors[..actors.len().min(CELLULAR_TILE_ACTOR_COUNT_MAXIMUM)]
        } else {
            &[]
        };
        let tile_count: usize = (width * height) as usize;
        let origin_x: i64 = region_origin.0;
        let origin_y: i64 = region_origin.1;
        let mut erase_words: Vec<[u32; 2]> = vec![[0; 2]; tile_count];
        let mut spawn_commands: Vec<[u32; 4]> = Vec::with_capacity(edits.len());
        let mut has_insert_spawn: bool = false;
        let mut set_erased_cell = |world_cell: CellCoordinates| {
            let local_x: i128 = i128::from(world_cell.x) - i128::from(origin_x);
            let local_y: i128 = i128::from(world_cell.y) - i128::from(origin_y);
            if local_x < 0 || local_y < 0
                || local_x >= i128::from(cell_width)
                || local_y >= i128::from(cell_height)
            { return; }
            let x: u32 = local_x as u32;
            let y: u32 = local_y as u32;
            let tile: usize = ((y / 8) * width + x / 8) as usize;
            let local_cell: u32 = (y % 8) * 8 + x % 8;
            erase_words[tile][(local_cell / 32) as usize] |= 1 << (local_cell % 32);
        };
        for cell in erasures { set_erased_cell(*cell); }
        for (cell, edit) in edits {
            let local_x: i128 = i128::from(cell.x) - i128::from(origin_x);
            let local_y: i128 = i128::from(cell.y) - i128::from(origin_y);
            if local_x < 0 || local_y < 0
                || local_x >= i128::from(cell_width)
                || local_y >= i128::from(cell_height)
            { continue; }
            if edit.mode == 0 { set_erased_cell(*cell); }
            if edit.mode != 0 { has_insert_spawn = true; }
            spawn_commands.push([
                local_x as u32,
                local_y as u32,
                edit.material_identifier.as_u32(),
                edit.mode,
            ]);
        }
        let has_erasures: bool = erase_words.iter()
            .any(|words| words[0] != 0 || words[1] != 0);
        let mut edit_bytes: Vec<u8> = Vec::with_capacity(spawn_commands.len() * 16);
        for command in &spawn_commands {
            for value in command { edit_bytes.extend_from_slice(&value.to_le_bytes()); }
        }
        if !edit_bytes.is_empty() {
            accelerator.queue().write_buffer(&self.edit_buffer, 0, &edit_bytes);
        }
        let rejected_imports: Vec<FluidParticleImport> =
            self.read_particle_import_readbacks(accelerator)?;
        let import_readback_slot: Option<usize> = if imports.is_empty() { None } else {
            self.import_readbacks.iter().position(|(_, submission)| submission.is_none())
        };
        let submitted_import_count: usize = import_readback_slot
            .map_or(0, |_| imports.len().min(self.capacity as usize));
        let imports: &[FluidParticleImport] = &imports[..submitted_import_count];
        let import_count: u32 = imports.len() as u32;
        let mut import_bytes: Vec<u8> =
            Vec::with_capacity(imports.len() * IMPORT_BYTE_COUNT as usize);
        for import in imports {
            for value in [import.position[0].to_bits(), import.position[1].to_bits(),
                import.particle.velocity()[0].to_bits(), import.particle.velocity()[1].to_bits(),
                import.particle.material_identifier().as_u32(), 0, 0, 0]
            { import_bytes.extend_from_slice(&value.to_le_bytes()); }
        }
        if !import_bytes.is_empty() {
            accelerator.queue().write_buffer(&self.import_buffer, 0, &import_bytes);
        }
        if has_erasures {
            let mut erasure_bytes: Vec<u8> = Vec::with_capacity(tile_count * 8);
            for words in &erase_words {
                erasure_bytes.extend_from_slice(&words[0].to_le_bytes());
                erasure_bytes.extend_from_slice(&words[1].to_le_bytes());
            }
            accelerator.queue().write_buffer(&self.erasure_mask_buffer, 0, &erasure_bytes);
        }
        let rho0: f32 = reference_density();
        // convert gravity to a per-substep velocity increment in cells per tick
        let gravity_substep: f32 = gravity * CELLULAR_TILE_SIZE as f32
            * FLUID_TIME_STEP.powi(2) / FLUID_SUBSTEP_COUNT as f32;
        let old_state_side: u32 = self.state_side;
        let needs_compaction: bool = has_erasures || rebase[0] != 0.0 || rebase[1] != 0.0
            || export_target.is_some();
        let after_compaction_side: u32 = if needs_compaction {
            1 - old_state_side
        } else {
            old_state_side
        };
        let mut uniform_bytes: Vec<u8> = vec![0; UNIFORM_STRIDE as usize * UNIFORM_COUNT];
        let mut write_uniform = |slot: usize,
                                 state_side: u32,
                                 predicted_side: u32,
                                 actor_time: f32,
                                 substep_index: u32| {
            let start: usize = slot * UNIFORM_STRIDE as usize;
            let fields: [u32; 16] = [
                width,
                height,
                ring_offset.0,
                ring_offset.1,
                cell_width,
                cell_height,
                bin_width,
                bin_height,
                bin_count,
                self.capacity,
                state_side,
                predicted_side,
                spawn_commands.len() as u32,
                if has_erasures { 1 } else { 0 },
                FLUID_VELOCITY_MAXIMUM.to_bits(),
                scan_block_count,
            ];
            for (field, value) in fields.into_iter().enumerate() {
                uniform_bytes[start + field * 4..start + field * 4 + 4]
                    .copy_from_slice(&value.to_le_bytes());
            }
            for (field, value) in [
                FLUID_RELAXATION,
                rho0,
                gravity_substep,
                rebase[0],
                rebase[1],
                FLUID_SUBSTEP_COUNT as f32,
                FLUID_COHESION_SCALE,
                FLUID_COHESION_NORMALIZATION_2D,
                FLUID_RASTER_SCALE,
            ]
                .into_iter().enumerate()
            {
                let byte_offset: usize = start + 64 + field * 4;
                uniform_bytes[byte_offset..byte_offset + 4]
                    .copy_from_slice(&value.to_le_bytes());
            }
            for (field, value) in [
                fluid_material_count,
                coupling_origin.0 as u32,
                coupling_origin.1 as u32,
                substep_index,
                import_count,
            ].into_iter().enumerate() {
                let byte_offset: usize = start + 100 + field * 4;
                uniform_bytes[byte_offset..byte_offset + 4]
                    .copy_from_slice(&value.to_le_bytes());
            }
            for (field, value) in [
                actors.len() as u32,
                FLUID_ACTOR_CONTACT_SCALE.to_bits(),
                actor_time.to_bits(),
            ].into_iter().enumerate() {
                let byte_offset: usize = start + 136 + field * 4;
                uniform_bytes[byte_offset..byte_offset + 4]
                    .copy_from_slice(&value.to_le_bytes());
            }
        };
        write_uniform(0, old_state_side, 0, 1.0, 0);
        write_uniform(1, after_compaction_side, 0, 1.0, 0);
        write_uniform(2, after_compaction_side, 0, 1.0, 0);
        write_uniform(3, after_compaction_side, 0, 1.0, 0);
        let mut current_state_side: u32 = after_compaction_side;
        let mut next_slot: usize = SUBSTEP_UNIFORM_START;
        for substep in 0..FLUID_SUBSTEP_COUNT {
            let predict_slot: usize = next_slot;
            let actor_time: f32 = (substep + 1) as f32 / FLUID_SUBSTEP_COUNT as f32;
            let substep_index: u32 = substep as u32;
            write_uniform(predict_slot, current_state_side, 0, actor_time, substep_index);
            write_uniform(predict_slot + 1, current_state_side, 1, actor_time, substep_index);
            write_uniform(predict_slot + 2, current_state_side, 0, actor_time, substep_index);
            let mut predicted_side: u32 = 1;
            let mut sorted_state_side: u32 = 1 - current_state_side;
            for iteration in 0..FLUID_ITERATION_COUNT {
                let iteration_slot: usize = predict_slot + 3 + iteration * 2;
                write_uniform(
                    iteration_slot,
                    sorted_state_side,
                    predicted_side,
                    actor_time,
                    substep_index,
                );
                write_uniform(
                    iteration_slot + 1,
                    sorted_state_side,
                    predicted_side,
                    actor_time,
                    substep_index,
                );
                predicted_side = 1 - predicted_side;
                if iteration == 1 && FLUID_ITERATION_COUNT > 2 {
                    let middle_sort_slot: usize = predict_slot + 3
                        + FLUID_ITERATION_COUNT * 2;
                    write_uniform(
                        middle_sort_slot,
                        sorted_state_side,
                        predicted_side,
                        actor_time,
                        substep_index,
                    );
                    sorted_state_side = 1 - sorted_state_side;
                    predicted_side = 1 - predicted_side;
                }
            }
            let commit_slot: usize = predict_slot + SUBSTEP_UNIFORM_COUNT - 1;
            write_uniform(
                commit_slot,
                sorted_state_side,
                predicted_side,
                actor_time,
                substep_index,
            );
            // commit writes the half the last sort did not, whatever the number of sorts
            current_state_side = 1 - sorted_state_side;
            next_slot = commit_slot + 1;
        }
        let world_origin_x: u64 = region_origin.0 as u64;
        let world_origin_y: u64 = region_origin.1 as u64;
        for slot in 0..UNIFORM_COUNT {
            let start: usize = slot * UNIFORM_STRIDE as usize;
            for (index, value) in [
                (120, world_origin_x as u32),
                (124, world_origin_y as u32),
                (128, (world_origin_x >> 32) as u32),
                (132, (world_origin_y >> 32) as u32),
            ] {
                uniform_bytes[start + index..start + index + 4]
                    .copy_from_slice(&value.to_le_bytes());
            }
        }
        accelerator.queue().write_buffer(&self.uniform_buffer, 0, &uniform_bytes);
        if let Some(export_target) = export_target
            && export_target.size() != self.export_buffer.size()
        { return Err(SceneError::FluidParticleExportBufferSizeInvalid); }
        if imports.len() > self.capacity as usize {
            return Err(SceneError::FluidParticleExportRecordInvalid);
        }
        if self.boundary_motion_bind_group.is_none() {
            self.boundary_motion_bind_group = Some(device.create_bind_group(
                &wgpu::BindGroupDescriptor {
                    label: Some("`FluidParticlePass.boundary_motion_bind_group`"),
                    layout: &self.boundary_motion_bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                                buffer: &self.uniform_buffer,
                                offset: 0,
                                size: wgpu::BufferSize::new(UNIFORM_BYTE_COUNT),
                            }),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: cellular_tile_buffers.material_identifier_buffer()
                                .as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 8,
                            resource: self.boundary_psi_buffer.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 10,
                            resource: self.control_buffer.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 13,
                            resource: self.boundary_tile_data_buffer.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 18,
                            resource: cellular_tile_buffers.rigid_body_cell_buffer()
                                .as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 19,
                            resource: collision_rigid_body_pass.cellular_tile_buffer()
                                .as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 20,
                            resource: collision_rigid_body_pass.rigid_body_buffer()
                                .as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 22,
                            resource: cellular_tile_buffers.kinematics_buffer()
                                .as_entire_binding(),
                        },
                    ],
                },
            ));
        }
        let mut command_encoder: wgpu::CommandEncoder = accelerator.device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Scene fluid particles"),
            });
        command_encoder.clear_buffer(&self.export_buffer, 0, Some(4));
        if let Some(cellular_tile_actor_buffer) = cellular_tile_actor_buffer {
            command_encoder.copy_buffer_to_buffer(
                cellular_tile_actor_buffer,
                0,
                &self.actor_buffer,
                0,
                self.actor_buffer.size(),
            );
        }
        command_encoder.clear_buffer(&self.actor_contact_buffer, 0, None);
        if needs_compaction {
            command_encoder.clear_buffer(&self.control_buffer, 4, Some(4));
        }
        {
            let mut compute_pass: wgpu::ComputePass<'_> = command_encoder
                .begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("Scene fluid particles"),
                    timestamp_writes: None,
                });
            let pipelines: &HashMap<&'static str, wgpu::ComputePipeline> =
                &self.pipelines;
            let set_slot = |compute_pass: &mut wgpu::ComputePass<'_>, slot: usize| {
                compute_pass.set_bind_group(
                    0,
                    &self.bind_group,
                    &[(slot as u64 * UNIFORM_STRIDE) as u32],
                );
            };
            let set_prepare_slot = |compute_pass: &mut wgpu::ComputePass<'_>, slot: usize| {
                compute_pass.set_bind_group(
                    0,
                    &self.prepare_bind_group,
                    &[(slot as u64 * UNIFORM_STRIDE) as u32],
                );
            };
            let set_prepare_raster_slot = |compute_pass: &mut wgpu::ComputePass<'_>, slot: usize| {
                compute_pass.set_bind_group(
                    0,
                    &self.prepare_raster_bind_group,
                    &[(slot as u64 * UNIFORM_STRIDE) as u32],
                );
            };
            let set_boundary_motion_slot =
                |compute_pass: &mut wgpu::ComputePass<'_>, slot: usize| {
                    compute_pass.set_bind_group(
                        0,
                        self.boundary_motion_bind_group.as_ref()
                            .expect("fluid boundary motion bind group should exist"),
                        &[(slot as u64 * UNIFORM_STRIDE) as u32],
                    );
                };
            let set_actor_projection_slot =
                |compute_pass: &mut wgpu::ComputePass<'_>, slot: usize| {
                    compute_pass.set_bind_group(
                        0,
                        &self.actor_projection_bind_group,
                        &[(slot as u64 * UNIFORM_STRIDE) as u32],
                    );
                };
            let set_actor_sample_slot = |compute_pass: &mut wgpu::ComputePass<'_>, slot: usize| {
                compute_pass.set_bind_group(
                    0,
                    &self.actor_sample_bind_group,
                    &[(slot as u64 * UNIFORM_STRIDE) as u32],
                );
            };
            let set_actor_commit_slot = |compute_pass: &mut wgpu::ComputePass<'_>, slot: usize| {
                compute_pass.set_bind_group(
                    0,
                    &self.actor_commit_bind_group,
                    &[(slot as u64 * UNIFORM_STRIDE) as u32],
                );
            };
            set_prepare_raster_slot(&mut compute_pass, 3);
            compute_pass.set_pipeline(&pipelines["prepare_raster"]);
            compute_pass.set_bind_group(1, &self.dispatch_bind_group, &[]);
            compute_pass.dispatch_workgroups(1, 1, 1);
            set_slot(&mut compute_pass, 3);
            compute_pass.set_pipeline(&pipelines["clear_fluid_raster_tiles"]);
            compute_pass.dispatch_workgroups_indirect(&self.dispatch_buffer, 0);
            compute_pass.set_pipeline(&pipelines["reset_fluid_raster_tile_count"]);
            compute_pass.dispatch_workgroups(1, 1, 1);
            if needs_compaction {
                set_prepare_slot(&mut compute_pass, 0);
                compute_pass.set_pipeline(&pipelines["prepare"]);
                compute_pass.set_bind_group(1, &self.dispatch_bind_group, &[]);
                compute_pass.dispatch_workgroups(1, 1, 1);
                set_slot(&mut compute_pass, 0);
                compute_pass.set_pipeline(&pipelines["compact"]);
                compute_pass.dispatch_workgroups_indirect(&self.dispatch_buffer, 0);
                compute_pass.set_pipeline(&pipelines["finish_compaction"]);
                compute_pass.dispatch_workgroups(1, 1, 1);
            }
            if has_insert_spawn || import_count > 0 {
                set_slot(&mut compute_pass, 2);
                compute_pass.set_pipeline(&pipelines["begin_occupancy"]);
                compute_pass.dispatch_workgroups(1, 1, 1);
                set_prepare_slot(&mut compute_pass, 2);
                compute_pass.set_pipeline(&pipelines["prepare"]);
                compute_pass.set_bind_group(1, &self.dispatch_bind_group, &[]);
                compute_pass.dispatch_workgroups(1, 1, 1);
                set_slot(&mut compute_pass, 2);
                compute_pass.set_pipeline(&pipelines["mark_occupancy"]);
                compute_pass.dispatch_workgroups_indirect(&self.dispatch_buffer, 0);
            }
            if !spawn_commands.is_empty() {
                set_slot(&mut compute_pass, 1);
                compute_pass.set_pipeline(&pipelines["spawn"]);
                compute_pass.dispatch_workgroups(
                    (spawn_commands.len() as u32).div_ceil(FLUID_WORKGROUP_SIZE),
                    1,
                    1,
                );
                set_prepare_slot(&mut compute_pass, 2);
                compute_pass.set_pipeline(&pipelines["prepare"]);
                compute_pass.set_bind_group(1, &self.dispatch_bind_group, &[]);
                compute_pass.dispatch_workgroups(1, 1, 1);
            }
            if import_count > 0 {
                set_slot(&mut compute_pass, 1);
                compute_pass.set_pipeline(&pipelines["spawn_imported"]);
                compute_pass.dispatch_workgroups(import_count.div_ceil(FLUID_WORKGROUP_SIZE), 1, 1);
            }
            set_prepare_slot(&mut compute_pass, 3);
            compute_pass.set_pipeline(&pipelines["prepare_boundary"]);
            compute_pass.set_bind_group(1, &self.dispatch_bind_group, &[]);
            compute_pass.dispatch_workgroups(1, 1, 1);
            set_slot(&mut compute_pass, 3);
            compute_pass.set_pipeline(&pipelines["clear_boundary_tiles"]);
            compute_pass.dispatch_workgroups_indirect(&self.dispatch_buffer, 0);
            compute_pass.set_pipeline(&pipelines["reset_boundary_tile_count"]);
            compute_pass.dispatch_workgroups(1, 1, 1);
            set_prepare_slot(&mut compute_pass, 3);
            compute_pass.set_pipeline(&pipelines["prepare"]);
            compute_pass.set_bind_group(1, &self.dispatch_bind_group, &[]);
            compute_pass.dispatch_workgroups(1, 1, 1);
            set_slot(&mut compute_pass, 3);
            compute_pass.set_pipeline(&pipelines["mark_boundary_tiles"]);
            compute_pass.dispatch_workgroups_indirect(&self.dispatch_buffer, 0);
            // widen the particle tiles by the kernel and motion margin in one pass over the region
            compute_pass.set_pipeline(&pipelines["dilate_boundary_tiles"]);
            compute_pass.dispatch_workgroups(
                (width * height).div_ceil(FLUID_WORKGROUP_SIZE),
                1,
                1,
            );
            set_prepare_slot(&mut compute_pass, 3);
            compute_pass.set_pipeline(&pipelines["prepare_boundary"]);
            compute_pass.set_bind_group(1, &self.dispatch_bind_group, &[]);
            compute_pass.dispatch_workgroups(1, 1, 1);
            set_slot(&mut compute_pass, 3);
            compute_pass.set_pipeline(&pipelines["boundary"]);
            compute_pass.dispatch_workgroups_indirect(&self.dispatch_buffer, 0);
            compute_pass.set_pipeline(&pipelines["boundary_weights"]);
            compute_pass.dispatch_workgroups_indirect(&self.dispatch_buffer, 0);
            set_prepare_slot(&mut compute_pass, 3);
            compute_pass.set_pipeline(&pipelines["prepare_boundary"]);
            compute_pass.set_bind_group(1, &self.dispatch_bind_group, &[]);
            compute_pass.dispatch_workgroups(1, 1, 1);
            set_boundary_motion_slot(&mut compute_pass, 3);
            compute_pass.set_pipeline(&pipelines["boundary_motion"]);
            compute_pass.dispatch_workgroups_indirect(&self.dispatch_buffer, 0);
            set_slot(&mut compute_pass, 3);
            compute_pass.set_pipeline(&pipelines["boundary_nodes"]);
            compute_pass.dispatch_workgroups_indirect(&self.dispatch_buffer, 0);
            set_slot(&mut compute_pass, 3);
            compute_pass.set_pipeline(&pipelines["prepare_sleep"]);
            compute_pass.dispatch_workgroups(width * height, 1, 1);
            set_actor_projection_slot(&mut compute_pass, 3);
            compute_pass.set_pipeline(&pipelines["wake_actor_neighbors"]);
            compute_pass.dispatch_workgroups(width * height, 1, 1);
            set_slot(&mut compute_pass, 3);
            compute_pass.set_pipeline(&pipelines["wake_solid_neighbors"]);
            compute_pass.dispatch_workgroups(width * height, 1, 1);
            set_prepare_slot(&mut compute_pass, 3);
            compute_pass.set_pipeline(&pipelines["prepare_boundary"]);
            compute_pass.set_bind_group(1, &self.dispatch_bind_group, &[]);
            compute_pass.dispatch_workgroups(1, 1, 1);
            set_slot(&mut compute_pass, 3);
            compute_pass.set_pipeline(&pipelines["clear_fluid_current_tiles"]);
            compute_pass.dispatch_workgroups_indirect(&self.dispatch_buffer, 0);
            // size the particle dispatches by the particle count again after the tile passes
            set_prepare_slot(&mut compute_pass, 3);
            compute_pass.set_pipeline(&pipelines["prepare"]);
            compute_pass.set_bind_group(1, &self.dispatch_bind_group, &[]);
            compute_pass.dispatch_workgroups(1, 1, 1);
            // apply the previous raster's solid contacts before the first prediction reads velocity
            set_slot(&mut compute_pass, SUBSTEP_UNIFORM_START);
            compute_pass.set_pipeline(&pipelines["apply_fluid_contacts"]);
            compute_pass.dispatch_workgroups_indirect(&self.dispatch_buffer, 0);
            set_prepare_raster_slot(&mut compute_pass, SUBSTEP_UNIFORM_START);
            compute_pass.set_pipeline(&pipelines["prepare_fluid_contacts"]);
            compute_pass.set_bind_group(1, &self.dispatch_bind_group, &[]);
            compute_pass.dispatch_workgroups(1, 1, 1);
            set_slot(&mut compute_pass, SUBSTEP_UNIFORM_START);
            compute_pass.set_pipeline(&pipelines["clear_fluid_contacts"]);
            compute_pass.dispatch_workgroups_indirect(&self.dispatch_buffer, 0);
            compute_pass.set_pipeline(&pipelines["reset_fluid_contact_tile_count"]);
            compute_pass.dispatch_workgroups(1, 1, 1);
            // contact clearing sized the shared indirect dispatch for tiles, so it is sized for the
            // live particles again before prediction, sorting, and every solver substep reuse it
            set_prepare_slot(&mut compute_pass, SUBSTEP_UNIFORM_START);
            compute_pass.set_pipeline(&pipelines["prepare"]);
            compute_pass.set_bind_group(1, &self.dispatch_bind_group, &[]);
            compute_pass.dispatch_workgroups(1, 1, 1);
            for substep in 0..FLUID_SUBSTEP_COUNT {
                let predict_slot: usize = SUBSTEP_UNIFORM_START
                    + substep * SUBSTEP_UNIFORM_COUNT;
                let wake_predict_slot: usize = predict_slot + 1;
                set_slot(&mut compute_pass, predict_slot);
                if substep == 0 {
                    compute_pass.set_pipeline(&pipelines["reset_wake_count"]);
                    compute_pass.dispatch_workgroups(1, 1, 1);
                }
                compute_pass.set_pipeline(&pipelines["predict"]);
                compute_pass.dispatch_workgroups_indirect(&self.dispatch_buffer, 0);
                if substep == 0 {
                    set_slot(&mut compute_pass, predict_slot);
                    compute_pass.set_pipeline(&pipelines["apply_pending_wakes"]);
                    compute_pass.dispatch_workgroups(width * height, 1, 1);
                    set_prepare_slot(&mut compute_pass, predict_slot);
                    compute_pass.set_pipeline(&pipelines["prepare_wake_dispatch"]);
                    compute_pass.set_bind_group(1, &self.dispatch_bind_group, &[]);
                    compute_pass.dispatch_workgroups(1, 1, 1);
                    set_slot(&mut compute_pass, wake_predict_slot);
                    compute_pass.set_pipeline(&pipelines["predict"]);
                    compute_pass.dispatch_workgroups_indirect(&self.dispatch_buffer, 12);
                }
                if !actors.is_empty() {
                    set_actor_projection_slot(&mut compute_pass, wake_predict_slot);
                    compute_pass.set_pipeline(&pipelines["project_actors"]);
                    compute_pass.dispatch_workgroups_indirect(&self.dispatch_buffer, 0);
                    set_slot(&mut compute_pass, wake_predict_slot);
                }
                macro_rules! sort_particles {
                    ($slot:expr) => {{
                    let slot: usize = $slot;
                    set_slot(&mut compute_pass, slot);
                    compute_pass.set_pipeline(&pipelines["clear_bins"]);
                    compute_pass.dispatch_workgroups(
                        bin_count.div_ceil(FLUID_SCAN_WORKGROUP_SIZE),
                        1,
                        1,
                    );
                    compute_pass.set_pipeline(&pipelines["count_bins"]);
                    compute_pass.dispatch_workgroups_indirect(&self.dispatch_buffer, 0);
                    compute_pass.set_pipeline(&pipelines["scan_bins"]);
                    compute_pass.dispatch_workgroups(scan_block_count, 1, 1);
                    compute_pass.set_pipeline(&pipelines["scan_blocks"]);
                    compute_pass.dispatch_workgroups(
                        scan_block_count.div_ceil(FLUID_SCAN_WORKGROUP_SIZE),
                        1,
                        1,
                    );
                    compute_pass.set_pipeline(&pipelines["scan_block_prefix"]);
                    compute_pass.dispatch_workgroups(1, 1, 1);
                    compute_pass.set_pipeline(&pipelines["add_block_prefix"]);
                    compute_pass.dispatch_workgroups(
                        bin_count.div_ceil(FLUID_SCAN_WORKGROUP_SIZE),
                        1,
                        1,
                    );
                    compute_pass.set_pipeline(&pipelines["scatter"]);
                    compute_pass.dispatch_workgroups_indirect(&self.dispatch_buffer, 0);
                    }};
                }
                sort_particles!(predict_slot + 2);
                for iteration in 0..FLUID_ITERATION_COUNT {
                    let lambda_slot: usize = predict_slot + 3 + iteration * 2;
                    set_slot(&mut compute_pass, lambda_slot);
                    compute_pass.set_pipeline(&pipelines["lambda"]);
                    compute_pass.dispatch_workgroups_indirect(&self.dispatch_buffer, 0);
                    set_slot(&mut compute_pass, lambda_slot + 1);
                    compute_pass.set_pipeline(&pipelines["correct"]);
                    compute_pass.dispatch_workgroups_indirect(&self.dispatch_buffer, 0);
                    if !actors.is_empty() {
                        set_actor_projection_slot(&mut compute_pass, lambda_slot + 1);
                        compute_pass.set_pipeline(&pipelines["project_actors"]);
                        compute_pass.dispatch_workgroups_indirect(&self.dispatch_buffer, 0);
                        set_slot(&mut compute_pass, lambda_slot + 1);
                    }
                    if iteration == 1 && FLUID_ITERATION_COUNT > 2 {
                        let middle_sort_slot: usize = predict_slot + 3
                            + FLUID_ITERATION_COUNT * 2;
                        sort_particles!(middle_sort_slot);
                    }
                }
                let commit_slot: usize = predict_slot + SUBSTEP_UNIFORM_COUNT - 1;
                set_slot(&mut compute_pass, commit_slot);
                compute_pass.set_pipeline(&pipelines["commit"]);
                compute_pass.dispatch_workgroups_indirect(&self.dispatch_buffer, 0);
                if !actors.is_empty() {
                    set_actor_commit_slot(&mut compute_pass, commit_slot);
                    compute_pass.set_pipeline(&pipelines["commit_actor_velocity"]);
                    compute_pass.dispatch_workgroups_indirect(&self.dispatch_buffer, 0);
                }
            }
            set_slot(&mut compute_pass, 3);
            compute_pass.set_pipeline(&pipelines["xsph"]);
            compute_pass.dispatch_workgroups_indirect(&self.dispatch_buffer, 0);
            compute_pass.set_pipeline(&pipelines["apply_xsph"]);
            compute_pass.dispatch_workgroups_indirect(&self.dispatch_buffer, 0);
            set_slot(&mut compute_pass, 3);
            compute_pass.set_pipeline(&pipelines["finish_sleep"]);
            compute_pass.dispatch_workgroups(width * height, 1, 1);
            set_prepare_slot(&mut compute_pass, 3);
            compute_pass.set_pipeline(&pipelines["prepare"]);
            compute_pass.set_bind_group(1, &self.dispatch_bind_group, &[]);
            compute_pass.dispatch_workgroups(1, 1, 1);
            set_slot(&mut compute_pass, 3);
            compute_pass.set_pipeline(&pipelines["rasterize_fluid"]);
            compute_pass.dispatch_workgroups_indirect(&self.dispatch_buffer, 0);
            if !actors.is_empty() {
                set_actor_sample_slot(&mut compute_pass, 3);
                compute_pass.set_pipeline(&pipelines["actor_contacts"]);
                compute_pass.dispatch_workgroups(
                    (actors.len() as u32).div_ceil(FLUID_WORKGROUP_SIZE),
                    1,
                    1,
                );
            }
        }
        let readback: Option<usize> = self.count_readbacks.iter()
            .position(|(_, submission)| submission.is_none());
        if let Some(readback) = readback {
            command_encoder.copy_buffer_to_buffer(
                &self.control_buffer,
                0,
                &self.count_readbacks[readback].0,
                0,
                4,
            );
        }
        let actor_readback: bool = if actors.is_empty() { false } else {
            self.actor_contact_readbacks.copy(
                &mut command_encoder,
                &self.actor_contact_buffer,
                0,
                actors.len() as u64 * ACTOR_CONTACT_BYTE_COUNT,
                (tick, actors.to_vec()),
            )
        };
        let import_readback: Option<usize> = import_readback_slot;
        // erased or rebased particles can leave contact changes in cells no particle clears
        if needs_compaction {
            let cell_count: u64 = tile_count as u64 * 64;
            command_encoder.clear_buffer(
                &self.fluid_raster_buffer,
                (cell_count * 4 + tile_count as u64 * 2 + 1) * 4,
                Some(cell_count * 8),
            );
        }
        if let Some(export_target) = export_target {
            command_encoder.copy_buffer_to_buffer(
                &self.export_buffer, 0, export_target, 0, self.export_buffer.size(),
            );
        }
        if let Some(import_readback) = import_readback {
            command_encoder.copy_buffer_to_buffer(
                &self.import_result_buffer,
                0,
                &self.import_readbacks[import_readback].0,
                0,
                u64::from(import_count) * 4,
            );
        }
        let submission_index: wgpu::SubmissionIndex =
            accelerator.queue().submit(Some(command_encoder.finish()));
        if let Some(import_readback) = import_readback {
            let (buffer, submission): &mut (wgpu::Buffer, Option<Vec<FluidParticleImport>>) =
                &mut self.import_readbacks[import_readback];
            *submission = Some(imports.to_vec());
            let import_response_sender: Sender<(usize, Result<(), wgpu::BufferAsyncError>)> =
                self.import_response_sender.clone();
            buffer.slice(..u64::from(import_count) * 4).map_async(
                wgpu::MapMode::Read,
                move |result| { let _ = import_response_sender.send((import_readback, result)); },
            );
        }
        if let Some(readback) = readback {
            let (buffer, submission): &mut (wgpu::Buffer, Option<(u64, u64)>) =
                &mut self.count_readbacks[readback];
            *submission = Some((tick, spawn_revision));
            let response_sender: Sender<(usize, Result<(), wgpu::BufferAsyncError>)> =
                self.response_sender.clone();
            buffer.slice(..4).map_async(
                wgpu::MapMode::Read,
                move |result: Result<(), wgpu::BufferAsyncError>| {
                    let _ = response_sender.send((readback, result));
                },
            );
        }
        if actor_readback { self.actor_contact_readbacks.map(submission_index); }
        self.state_side = current_state_side;
        Ok((newest_count, rejected_imports, submitted_import_count))
    }

    /// Returns imported records whose GPU spawns were rejected
    pub(super) fn read_particle_import_readbacks(
        &mut self,
        accelerator: &Accelerator,
    ) -> Result<Vec<FluidParticleImport>, SceneError> {
        accelerator.device().poll(wgpu::PollType::Poll).map_err(|_| SceneError::FluidParticlePoll)?;
        let mut rejected: Vec<FluidParticleImport> = Vec::new();
        while let Ok((index, result)) = self.import_response_receiver.try_recv() {
            let (buffer, submission): &mut (wgpu::Buffer, Option<Vec<FluidParticleImport>>) =
                &mut self.import_readbacks[index];
            let Some(imports) = submission.take() else { continue; };
            result.map_err(|error| SceneError::FluidParticleResponse { error })?;
            {
                let bytes: wgpu::BufferView = buffer.get_mapped_range(..imports.len() as u64 * 4)
                    .expect("mapped particle import status should be readable");
                for (import, status) in imports.into_iter().zip(bytes.chunks_exact(4)) {
                    if u32::from_le_bytes([status[0], status[1], status[2], status[3]]) != 0 {
                        rejected.push(import);
                    }
                }
            }
            buffer.unmap();
        }
        Ok(rejected)
    }

    /// Waits for outstanding particle import results
    pub(super) fn wait_particle_import_readbacks(
        &mut self,
        accelerator: &Accelerator,
    ) -> Result<Vec<FluidParticleImport>, SceneError> {
        let mut rejected: Vec<FluidParticleImport> = Vec::new();
        while self.has_pending_particle_imports() {
            accelerator.device().poll(wgpu::PollType::wait_indefinitely())
                .map_err(|_| SceneError::FluidParticlePoll)?;
            rejected.extend(self.read_particle_import_readbacks(accelerator)?);
        }
        Ok(rejected)
    }

    /// Returns the newest completed count readback and releases every completed slot
    fn read_count_readbacks(
        &mut self,
        accelerator: &Accelerator,
    ) -> Result<Option<(u32, u64, u64)>, SceneError> {
        accelerator.device().poll(wgpu::PollType::Poll)
            .map_err(|_| SceneError::FluidParticlePoll)?;
        let mut newest: Option<(u32, u64, u64)> = None;
        while let Ok((readback, result)) = self.response_receiver.try_recv() {
            let (buffer, submission): &mut (wgpu::Buffer, Option<(u64, u64)>) =
                &mut self.count_readbacks[readback];
            let Some((tick, revision)) = submission.take() else { continue; };
            result.map_err(|error| SceneError::FluidParticleResponse { error })?;
            let particle_count: u32 = {
                let bytes: wgpu::BufferView = buffer.get_mapped_range(..4)
                    .expect("mapped fluid particle count should be readable");
                u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
            };
            buffer.unmap();
            if newest.is_none_or(|(_, newest_tick, _)| tick >= newest_tick) {
                newest = Some((particle_count, tick, revision));
            }
        }
        Ok(newest)
    }

}

/// Decodes the copied live particle array into owning chunk records
fn decode_live_particle_snapshot(
    bytes: &[u8],
    origin: (i64, i64),
    capacity: u32,
) -> Result<Vec<(ChunkCoordinates, ChunkFluidParticle)>, SceneError> {
    if bytes.len() < 4 { return Err(SceneError::FluidParticleExportRecordInvalid); }
    let count: usize = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
    if count > capacity as usize || 4 + count * PARTICLE_BYTE_COUNT as usize > bytes.len() {
        return Err(SceneError::FluidParticleExportRecordInvalid);
    }
    let read = |offset: usize| u32::from_le_bytes([
        bytes[offset], bytes[offset + 1], bytes[offset + 2], bytes[offset + 3],
    ]);
    let mut particles: Vec<(ChunkCoordinates, ChunkFluidParticle)> = Vec::with_capacity(count);
    for index in 0..count {
        let start: usize = 4 + index * PARTICLE_BYTE_COUNT as usize;
        let position: [f32; 2] = [f32::from_bits(read(start)), f32::from_bits(read(start + 4))];
        let velocity: [f32; 2] =
            [f32::from_bits(read(start + 8)), f32::from_bits(read(start + 12))];
        let x: i64 = origin.0.checked_add(position[0].floor() as i64)
            .ok_or(SceneError::FluidParticleExportRecordInvalid)?;
        let y: i64 = origin.1.checked_add(position[1].floor() as i64)
            .ok_or(SceneError::FluidParticleExportRecordInvalid)?;
        let coordinates: ChunkCoordinates = ChunkCoordinates {
            x: x.div_euclid(CHUNK_SIZE).try_into()
                .map_err(|_| SceneError::FluidParticleExportRecordInvalid)?,
            y: y.div_euclid(CHUNK_SIZE).try_into()
                .map_err(|_| SceneError::FluidParticleExportRecordInvalid)?,
        };
        let particle: ChunkFluidParticle = ChunkFluidParticle::new(
            [x.rem_euclid(CHUNK_SIZE) as f32 + position[0] - position[0].floor(),
                y.rem_euclid(CHUNK_SIZE) as f32 + position[1] - position[1].floor()],
            velocity,
            MaterialIdentifier::from_u32(read(start + 16)),
        ).map_err(|_| SceneError::FluidParticleExportRecordInvalid)?;
        particles.push((coordinates, particle));
    }
    Ok(particles)
}

/// Returns the poly6 kernel sum over the unit square lattice used as rest density
fn reference_density() -> f32 {
    let mut density: f32 = 0.0;
    for y in -2..=2 {
        for x in -2..=2 {
            let distance: f32 = (x as f32).hypot(y as f32);
            if distance >= FLUID_KERNEL_RADIUS { continue; }
            let difference: f32 = FLUID_KERNEL_RADIUS * FLUID_KERNEL_RADIUS - distance * distance;
            density += 4.0 / (std::f32::consts::PI * FLUID_KERNEL_RADIUS.powi(8))
                * difference.powi(3);
        }
    }
    density
}
