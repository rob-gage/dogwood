// Copyright Rob Gage 2026

//! Shared deterministic reaction eligibility and arbitration rules. GPU discovery
//! emits the same compact candidate shape; keeping arbitration here makes the
//! ordering contract explicit and independently testable.

use super::ReactionMaterialTable;
use super::fluids::FluidAuthorityView;
use crate::materials::CompiledMaterialReaction;
use engine_compute::{Accelerator, AcceleratorBuffer};
use std::collections::BTreeSet;
use std::sync::mpsc::{Receiver, sync_channel};

const RIGID_REMOVAL_EVENT_SIZE: u64 = 32;
const RIGID_REMOVAL_EVENTS_OFFSET: u64 = 256;

/// Immutable-snapshot GPU reaction discovery. Application is intentionally a
/// separate stage so no product becomes an input until the next chemistry tick.
pub(crate) struct MaterialReactions {
    candidates: AcceleratorBuffer,
    candidate_indices: AcceleratorBuffer,
    candidate_count: AcceleratorBuffer,
    sort_steps: AcceleratorBuffer,
    sort_indirect: wgpu::Buffer,
    sort_parameters: wgpu::Buffer,
    fluid_reservations: AcceleratorBuffer,
    gas_reservations: AcceleratorBuffer,
    gas_output_reservations: AcceleratorBuffer,
    canonical_reservations: AcceleratorBuffer,
    rigid_reservations: AcceleratorBuffer,
    rigid_removal_events: AcceleratorBuffer,
    rigid_removal_count: AcceleratorBuffer,
    rigid_removal_readback: wgpu::Buffer,
    rigid_removal_readback_len: u64,
    rigid_removal_readback_capacity: u32,
    rigid_removal_readback_result: Option<Receiver<Result<(), wgpu::BufferAsyncError>>>,
    fluid_reservation_owners: AcceleratorBuffer,
    reaction_energy: AcceleratorBuffer,
    parameters: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    prepare_sort_bind_group: wgpu::BindGroup,
    sort_bind_group: wgpu::BindGroup,
    clear_pipeline: wgpu::ComputePipeline,
    discover_pipeline: wgpu::ComputePipeline,
    compact_pipeline: wgpu::ComputePipeline,
    prepare_sort_pipeline: wgpu::ComputePipeline,
    sort_pipeline: wgpu::ComputePipeline,
    reserve_pipeline: wgpu::ComputePipeline,
    apply_pipeline: wgpu::ComputePipeline,
    cell_count: u32,
    clear_count: u32,
    sort_capacity: u32,
    sort_step_count: usize,
    reaction_count: u32,
}

impl MaterialReactions {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        accelerator: &Accelerator,
        table: &ReactionMaterialTable,
        material_identifiers: &AcceleratorBuffer,
        amounts: &AcceleratorBuffer,
        temperatures: &AcceleratorBuffer,
        gas_temperatures: &AcceleratorBuffer,
        rigid_temperatures: &AcceleratorBuffer,
        retained_pressure: &AcceleratorBuffer,
        fluid_coverage: &AcceleratorBuffer,
        gas_concentrations: &AcceleratorBuffer,
        external_occupancy: &AcceleratorBuffer,
        rigid_claims: &AcceleratorBuffer,
        rigid_cells: &AcceleratorBuffer,
        rigid_amounts: &AcceleratorBuffer,
        reaction_energy: AcceleratorBuffer,
        pending_pressure: &AcceleratorBuffer,
        mutation_requests: &AcceleratorBuffer,
        mutation_request_count: &AcceleratorBuffer,
        fluid_authority: FluidAuthorityView<'_>,
        cell_count: u32,
        gas_count: u32,
        reaction_count: u32,
    ) -> Self {
        let device = accelerator.wgpu_device();
        let candidates = accelerator.allocate::<[u32; 32]>(cell_count as usize);
        let sort_capacity = cell_count.max(1).next_power_of_two();
        let candidate_indices = accelerator.allocate::<u32>(sort_capacity as usize);
        let candidate_count = accelerator.allocate::<u32>(1);
        let sort_step_values: Vec<[u32; 2]> = (1..=sort_capacity.trailing_zeros())
            .flat_map(|level| {
                let k = 1u32 << level;
                (0..level).rev().map(move |j| [k, 1u32 << j])
            })
            .collect();
        let sort_steps = accelerator.allocate::<[u32; 2]>(sort_step_values.len().max(1));
        let sort_indirect = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("chemistry sort indirect dispatch"),
            size: 12,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::INDIRECT,
            mapped_at_creation: false,
        });
        if !sort_step_values.is_empty() {
            accelerator.wgpu_queue().write_buffer(
                sort_steps.wgpu_buffer(),
                0,
                &sort_step_values
                    .iter()
                    .flat_map(|step| step.iter().flat_map(|v| v.to_le_bytes()))
                    .collect::<Vec<_>>(),
            );
        }
        let sort_parameters = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("chemistry sort parameters"),
            size: 8,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let fluid_reservations =
            accelerator.allocate::<u32>(fluid_authority.particle_capacity as usize);
        let gas_reservations =
            accelerator.allocate::<u32>((cell_count * gas_count.max(1)) as usize);
        let gas_output_reservations =
            accelerator.allocate::<u32>((cell_count * gas_count.max(1)) as usize);
        let canonical_reservations = accelerator.allocate::<u32>(cell_count as usize);
        let rigid_reservations = accelerator.allocate::<u32>(cell_count as usize);
        let rigid_removal_events = accelerator.allocate::<[u32; 8]>(cell_count as usize);
        let rigid_removal_count = accelerator.allocate::<u32>(1);
        let rigid_removal_readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rigid chemistry removal readback"),
            size: RIGID_REMOVAL_EVENTS_OFFSET + u64::from(cell_count) * RIGID_REMOVAL_EVENT_SIZE,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        accelerator.wgpu_queue().write_buffer(
            rigid_removal_count.wgpu_buffer(),
            0,
            &0u32.to_le_bytes(),
        );
        let fluid_reservation_owners =
            accelerator.allocate::<u32>(fluid_authority.particle_capacity as usize);
        let parameters = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("material reaction parameters"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        accelerator.wgpu_queue().write_buffer(
            &parameters,
            0,
            &[
                cell_count.to_le_bytes(),
                gas_count.to_le_bytes(),
                reaction_count.to_le_bytes(),
                0u32.to_le_bytes(),
            ]
            .concat(),
        );
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
            label: Some("material reaction discovery layout"),
            entries: &[
                storage(0, true),
                storage(1, true),
                storage(2, true),
                storage(3, false),
                storage(4, true),
                storage(5, true),
                storage(6, true),
                storage(7, false),
                storage(8, true),
                storage(9, true),
                storage(10, false),
                wgpu::BindGroupLayoutEntry {
                    binding: 11,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                storage(12, false),
                storage(13, false),
                storage(14, false),
                storage(15, false),
                storage(16, false),
                storage(17, true),
                storage(18, true),
                wgpu::BindGroupLayoutEntry {
                    binding: 19,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                storage(20, false),
                storage(21, false),
                storage(22, false),
                storage(23, false),
                storage(24, false),
                storage(25, false),
                storage(26, false),
                storage(27, true),
                storage(28, false),
                storage(29, false),
                storage(30, false),
                storage(31, false),
                storage(32, false),
                storage(33, false),
                storage(37, true),
                storage(38, true),
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("material reaction discovery"),
            layout: &layout,
            entries: &[
                Self::binding(0, table.records_buffer()),
                Self::binding(1, table.selector_members_buffer()),
                Self::binding(2, material_identifiers),
                Self::binding(3, amounts),
                Self::binding(4, temperatures),
                Self::binding(5, retained_pressure),
                Self::binding(6, fluid_coverage),
                Self::binding(7, gas_concentrations),
                Self::binding(8, external_occupancy),
                Self::binding(9, rigid_claims),
                Self::binding(10, &candidates),
                wgpu::BindGroupEntry {
                    binding: 11,
                    resource: parameters.as_entire_binding(),
                },
                Self::binding(12, mutation_requests),
                Self::binding(13, mutation_request_count),
                Self::binding(14, &reaction_energy),
                Self::binding(15, pending_pressure),
                Self::binding(16, fluid_authority.particles),
                Self::binding(17, fluid_authority.bucket_heads),
                Self::binding(18, fluid_authority.next_particle),
                wgpu::BindGroupEntry {
                    binding: 19,
                    resource: fluid_authority.parameters.as_entire_binding(),
                },
                Self::binding(20, fluid_authority.free_indices),
                Self::binding(21, fluid_authority.free_count),
                Self::binding(22, &fluid_reservations),
                Self::binding(23, &gas_reservations),
                Self::binding(24, &gas_output_reservations),
                Self::binding(25, &canonical_reservations),
                Self::binding(26, &fluid_reservation_owners),
                Self::binding(27, rigid_cells),
                Self::binding(28, rigid_amounts),
                Self::binding(29, &rigid_reservations),
                Self::binding(30, &rigid_removal_events),
                Self::binding(31, &rigid_removal_count),
                Self::binding(32, &candidate_indices),
                Self::binding(33, &candidate_count),
                Self::binding(37, gas_temperatures),
                Self::binding(38, rigid_temperatures),
            ],
        });
        let prepare_sort_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("chemistry sort preparation layout"),
                entries: &[storage(32, false), storage(33, false), storage(36, false)],
            });
        let prepare_sort_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("chemistry sort preparation"),
            layout: &prepare_sort_layout,
            entries: &[
                Self::binding(32, &candidate_indices),
                Self::binding(33, &candidate_count),
                wgpu::BindGroupEntry {
                    binding: 36,
                    resource: sort_indirect.as_entire_binding(),
                },
            ],
        });
        let sort_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("chemistry candidate sort layout"),
            entries: &[
                storage(0, true),
                storage(10, false),
                storage(32, false),
                storage(33, false),
                wgpu::BindGroupLayoutEntry {
                    binding: 34,
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
        let sort_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("chemistry candidate sort"),
            layout: &sort_layout,
            entries: &[
                Self::binding(0, table.records_buffer()),
                Self::binding(10, &candidates),
                Self::binding(32, &candidate_indices),
                Self::binding(33, &candidate_count),
                wgpu::BindGroupEntry {
                    binding: 34,
                    resource: sort_parameters.as_entire_binding(),
                },
            ],
        });
        let shader = super::create_simulation_shader_module(
            device,
            "material reactions",
            include_str!("material_reactions.wgsl"),
            file!(),
        );
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("material reaction discovery pipeline layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = |entry_point, label| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                module: &shader,
                entry_point: Some(entry_point),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        let prepare_sort_pipeline =
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("chemistry sort dispatch preparation pipeline"),
                layout: Some(
                    &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                        label: Some("chemistry sort preparation pipeline layout"),
                        bind_group_layouts: &[Some(&prepare_sort_layout)],
                        immediate_size: 0,
                    }),
                ),
                module: &shader,
                entry_point: Some("prepare_sort_dispatch"),
                compilation_options: Default::default(),
                cache: None,
            });
        let sort_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("chemistry candidate sort pipeline"),
            layout: Some(
                &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("chemistry candidate sort pipeline layout"),
                    bind_group_layouts: &[Some(&sort_layout)],
                    immediate_size: 0,
                }),
            ),
            module: &shader,
            entry_point: Some("sort_candidates"),
            compilation_options: Default::default(),
            cache: None,
        });
        Self {
            candidates,
            candidate_indices,
            candidate_count,
            sort_steps,
            sort_indirect,
            sort_parameters,
            fluid_reservations,
            gas_reservations,
            gas_output_reservations,
            canonical_reservations,
            rigid_reservations,
            rigid_removal_events,
            rigid_removal_count,
            rigid_removal_readback,
            rigid_removal_readback_len: 0,
            rigid_removal_readback_capacity: 0,
            rigid_removal_readback_result: None,
            fluid_reservation_owners,
            reaction_energy,
            parameters,
            bind_group,
            prepare_sort_bind_group,
            sort_bind_group,
            clear_pipeline: pipeline(
                "clear_transaction_state",
                "material reaction transaction clear pipeline",
            ),
            discover_pipeline: pipeline(
                "discover_canonical",
                "material reaction discovery pipeline",
            ),
            compact_pipeline: pipeline(
                "compact_candidates",
                "chemistry candidate compaction pipeline",
            ),
            prepare_sort_pipeline,
            sort_pipeline,
            reserve_pipeline: pipeline(
                "reserve_fluid_authority",
                "material reaction fluid reservation pipeline",
            ),
            apply_pipeline: pipeline(
                "apply_canonical",
                "material reaction canonical apply pipeline",
            ),
            cell_count,
            clear_count: fluid_authority
                .particle_capacity
                .max(cell_count.saturating_mul(gas_count.max(1)))
                .max(sort_capacity),
            sort_capacity,
            sort_step_count: sort_step_values.len(),
            reaction_count,
        }
    }
    pub(crate) fn encode(&self, accelerator: &Accelerator, encoder: &mut wgpu::CommandEncoder) {
        if self.reaction_count == 0 {
            return;
        }
        let mut pass = accelerator.begin_compute_pass(encoder, "chemistry clear");
        pass.set_pipeline(&self.clear_pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.dispatch_workgroups(self.clear_count.div_ceil(64), 1, 1);
        drop(pass);
        let mut pass = accelerator.begin_compute_pass(encoder, "chemistry discovery");
        pass.set_pipeline(&self.discover_pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.dispatch_workgroups(self.cell_count.div_ceil(64), 1, 1);
        drop(pass);
        let mut pass = accelerator.begin_compute_pass(encoder, "chemistry candidate compaction");
        pass.set_pipeline(&self.compact_pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.dispatch_workgroups(self.cell_count.div_ceil(64), 1, 1);
        drop(pass);
        let mut pass =
            accelerator.begin_compute_pass(encoder, "chemistry sort dispatch preparation");
        pass.set_pipeline(&self.prepare_sort_pipeline);
        pass.set_bind_group(0, &self.prepare_sort_bind_group, &[]);
        pass.dispatch_workgroups(1, 1, 1);
        drop(pass);
        for step in 0..self.sort_step_count {
            encoder.copy_buffer_to_buffer(
                self.sort_steps.wgpu_buffer(),
                step as u64 * 8,
                &self.sort_parameters,
                0,
                8,
            );
            let mut pass = accelerator.begin_compute_pass(encoder, "chemistry candidate sort");
            pass.set_pipeline(&self.sort_pipeline);
            pass.set_bind_group(0, &self.sort_bind_group, &[]);
            pass.dispatch_workgroups_indirect(&self.sort_indirect, 0);
            drop(pass);
        }
        let mut pass = accelerator.begin_compute_pass(encoder, "chemistry arbitration");
        pass.set_pipeline(&self.reserve_pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        // One invocation performs the ordered arbitration. It is intentionally
        // serialized: reservation order is a correctness rule, not a race.
        pass.dispatch_workgroups(1, 1, 1);
        drop(pass);
        let mut pass = accelerator.begin_compute_pass(encoder, "chemistry apply");
        pass.set_pipeline(&self.apply_pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.dispatch_workgroups(self.cell_count.div_ceil(64), 1, 1);
    }
    pub(crate) const fn reaction_energy_buffer(&self) -> &AcceleratorBuffer {
        &self.reaction_energy
    }
    fn binding(binding: u32, buffer: &AcceleratorBuffer) -> wgpu::BindGroupEntry<'_> {
        wgpu::BindGroupEntry {
            binding,
            resource: buffer.wgpu_buffer().as_entire_binding(),
        }
    }
    pub(crate) fn submit_rigid_removal_readback(&mut self, accelerator: &Accelerator) {
        if self.rigid_removal_readback_result.is_some() {
            return;
        }
        let len = RIGID_REMOVAL_EVENTS_OFFSET;
        let mut encoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("rigid chemistry removal readback"),
                });
        encoder.copy_buffer_to_buffer(
            self.rigid_removal_count.wgpu_buffer(),
            0,
            &self.rigid_removal_readback,
            0,
            4,
        );
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
        self.rigid_removal_readback_len = len;
        self.rigid_removal_readback_capacity = 0;
        let (sender, receiver) = sync_channel(1);
        self.rigid_removal_readback
            .slice(0..len)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = sender.send(result);
            });
        self.rigid_removal_readback_result = Some(receiver);
    }
    pub(crate) fn take_rigid_removal_events(
        &mut self,
        accelerator: &Accelerator,
    ) -> Option<Vec<[u32; 6]>> {
        let result = self
            .rigid_removal_readback_result
            .as_ref()?
            .try_recv()
            .ok()?;
        self.rigid_removal_readback_result = None;
        if result.is_err() {
            self.rigid_removal_readback.unmap();
            return Some(Vec::new());
        }
        let bytes = self
            .rigid_removal_readback
            .slice(0..self.rigid_removal_readback_len)
            .get_mapped_range()
            .ok()?;
        let count = u32::from_le_bytes(bytes[..4].try_into().ok()?).min(self.cell_count) as usize;
        if self.rigid_removal_readback_capacity == 0 {
            drop(bytes);
            self.rigid_removal_readback.unmap();
            if count == 0 {
                return Some(Vec::new());
            }
            let len = RIGID_REMOVAL_EVENTS_OFFSET + count as u64 * RIGID_REMOVAL_EVENT_SIZE;
            let mut encoder =
                accelerator
                    .wgpu_device()
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("rigid chemistry event readback"),
                    });
            encoder.copy_buffer_to_buffer(
                self.rigid_removal_events.wgpu_buffer(),
                0,
                &self.rigid_removal_readback,
                RIGID_REMOVAL_EVENTS_OFFSET,
                count as u64 * RIGID_REMOVAL_EVENT_SIZE,
            );
            accelerator.wgpu_queue().submit(Some(encoder.finish()));
            self.rigid_removal_readback_len = len;
            self.rigid_removal_readback_capacity = count as u32;
            let (sender, receiver) = sync_channel(1);
            self.rigid_removal_readback.slice(0..len).map_async(
                wgpu::MapMode::Read,
                move |result| {
                    let _ = sender.send(result);
                },
            );
            self.rigid_removal_readback_result = Some(receiver);
            return None;
        }
        let events = bytes[usize::try_from(RIGID_REMOVAL_EVENTS_OFFSET).unwrap()..]
            .chunks_exact(32)
            .take(count)
            .map(|b| {
                let mut event = [0; 6];
                for (word, value) in event.iter_mut().zip(b.chunks_exact(4)) {
                    *word = u32::from_le_bytes(value.try_into().unwrap());
                }
                event
            })
            .collect();
        drop(bytes);
        self.rigid_removal_readback.unmap();
        Some(events)
    }
}

impl Drop for MaterialReactions {
    fn drop(&mut self) {
        self.candidates.free();
        self.candidate_indices.free();
        self.candidate_count.free();
        self.sort_steps.free();
        self.sort_indirect.destroy();
        self.sort_parameters.destroy();
        self.rigid_reservations.free();
        self.rigid_removal_events.free();
        self.rigid_removal_count.free();
        self.rigid_removal_readback.destroy();
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ReactionEnvironment {
    pub temperature: f32,
    pub pressure: f32,
    pub air: f32,
}

pub(crate) fn environment_matches(
    rule: &CompiledMaterialReaction,
    env: ReactionEnvironment,
) -> bool {
    (rule.minimum_temperature.is_nan() || env.temperature >= rule.minimum_temperature)
        && (rule.maximum_temperature.is_nan() || env.temperature <= rule.maximum_temperature)
        && (rule.minimum_pressure.is_nan() || env.pressure >= rule.minimum_pressure)
        && (rule.maximum_pressure.is_nan() || env.pressure <= rule.maximum_pressure)
        && (rule.minimum_air.is_nan() || env.air >= rule.minimum_air)
        && (rule.maximum_air.is_nan() || env.air <= rule.maximum_air)
}

/// Matches the occupancy convention used by thermal interaction: canonical or
/// rigid/external solid occupancy excludes implicit air; otherwise fluid and
/// explicit gas consume the unit local gas capacity.
pub(crate) fn implicit_air(
    canonical_empty: bool,
    rigid_or_external_blocked: bool,
    fluid_coverage: f32,
    explicit_gas: f32,
) -> f32 {
    if !canonical_empty || rigid_or_external_blocked {
        0.0
    } else {
        (1.0 - fluid_coverage.clamp(0.0, 1.0) - explicit_gas.max(0.0)).clamp(0.0, 1.0)
    }
}

/// A claim key encodes one authoritative inventory location, rather than a
/// derived raster location. The producer is responsible for including rigid
/// state generation in its key.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ReactionCandidate {
    pub anchor: u32,
    pub reaction_index: u32,
    pub priority: i32,
    pub authoring_order: u32,
    pub extent: f32,
    pub authorities: [Option<u64>; 2],
}

/// Sort by explicit priority, stable authoring order, then anchor. Accepted
/// candidates reserve every authority as an all-or-nothing set, so neither GPU
/// invocation order nor overlapping raster claims can double-consume matter.
pub(crate) fn resolve_contention(mut candidates: Vec<ReactionCandidate>) -> Vec<ReactionCandidate> {
    candidates.sort_by_key(candidate_order_key);
    let mut claimed = BTreeSet::new();
    candidates
        .into_iter()
        .filter(|candidate| {
            let keys: Vec<u64> = candidate.authorities.iter().flatten().copied().collect();
            if keys.iter().any(|key| claimed.contains(key)) {
                return false;
            }
            claimed.extend(keys);
            true
        })
        .collect()
}

fn candidate_order_key(candidate: &ReactionCandidate) -> (std::cmp::Reverse<i32>, u32, u32, u32) {
    (
        std::cmp::Reverse(candidate.priority),
        candidate.authoring_order,
        candidate.anchor,
        candidate.reaction_index,
    )
}

/// The extent calculation used by every authority form after discovery.
pub(crate) fn extent(
    maximum: f32,
    available: impl IntoIterator<Item = f32>,
    coefficients: impl IntoIterator<Item = f32>,
) -> f32 {
    let inventory_limit = available
        .into_iter()
        .zip(coefficients)
        .map(|(amount, coefficient)| amount / coefficient)
        .fold(f32::INFINITY, f32::min);
    maximum.min(inventory_limit).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        materials::{CompiledMaterialReaction, MaterialRegistry},
        simulation::ReactionMaterialTable,
    };

    #[test]
    fn environment_bounds_are_independent() {
        let mut rule = CompiledMaterialReaction::default();
        rule.minimum_temperature = 10.0;
        rule.maximum_temperature = 20.0;
        rule.minimum_pressure = 2.0;
        rule.maximum_pressure = 4.0;
        rule.minimum_air = 0.25;
        rule.maximum_air = 0.75;
        assert!(environment_matches(
            &rule,
            ReactionEnvironment {
                temperature: 15.0,
                pressure: 3.0,
                air: 0.5
            }
        ));
        assert!(!environment_matches(
            &rule,
            ReactionEnvironment {
                temperature: 9.0,
                pressure: 3.0,
                air: 0.5
            }
        ));
        assert!(!environment_matches(
            &rule,
            ReactionEnvironment {
                temperature: 15.0,
                pressure: 5.0,
                air: 0.5
            }
        ));
        assert!(!environment_matches(
            &rule,
            ReactionEnvironment {
                temperature: 15.0,
                pressure: 3.0,
                air: 0.9
            }
        ));
    }
    #[test]
    fn contention_is_priority_then_stable_and_atomic() {
        let candidates = vec![
            ReactionCandidate {
                anchor: 8,
                reaction_index: 1,
                priority: 1,
                authoring_order: 1,
                extent: 1.0,
                authorities: [Some(7), Some(9)],
            },
            ReactionCandidate {
                anchor: 2,
                reaction_index: 2,
                priority: 2,
                authoring_order: 0,
                extent: 1.0,
                authorities: [Some(7), Some(10)],
            },
            ReactionCandidate {
                anchor: 3,
                reaction_index: 3,
                priority: 1,
                authoring_order: 0,
                extent: 1.0,
                authorities: [Some(11), None],
            },
        ];
        let accepted = resolve_contention(candidates);
        assert_eq!(
            accepted
                .iter()
                .map(|c| c.reaction_index)
                .collect::<Vec<_>>(),
            vec![2, 3]
        );
    }

    #[test]
    fn contention_tie_uses_authoring_order() {
        let candidates = vec![
            ReactionCandidate {
                anchor: 4,
                reaction_index: 9,
                priority: 3,
                authoring_order: 8,
                extent: 1.0,
                authorities: [Some(12), None],
            },
            ReactionCandidate {
                anchor: 2,
                reaction_index: 3,
                priority: 3,
                authoring_order: 2,
                extent: 1.0,
                authorities: [Some(12), None],
            },
        ];
        let accepted = resolve_contention(candidates);
        assert_eq!(accepted.len(), 1);
        assert_eq!(accepted[0].reaction_index, 3);
    }

    #[test]
    fn contention_final_tie_uses_reaction_index() {
        let candidates = vec![
            ReactionCandidate {
                anchor: 2,
                reaction_index: 9,
                priority: 3,
                authoring_order: 2,
                extent: 1.0,
                authorities: [Some(12), None],
            },
            ReactionCandidate {
                anchor: 2,
                reaction_index: 3,
                priority: 3,
                authoring_order: 2,
                extent: 1.0,
                authorities: [Some(12), None],
            },
        ];
        let accepted = resolve_contention(candidates);
        assert_eq!(accepted.len(), 1);
        assert_eq!(accepted[0].reaction_index, 3);
    }
    #[test]
    fn extent_is_stoichiometric() {
        assert_eq!(extent(0.5, [1.0, 0.4], [1.0, 2.0]), 0.2);
        // Two slots resolving to one authority share its inventory budget.
        assert_eq!(extent(1.0, [0.7], [1.0 + 1.0]), 0.35);
    }
    #[test]
    fn implicit_air_respects_local_occupancy() {
        assert_eq!(implicit_air(true, false, 0.0, 0.0), 1.0);
        assert_eq!(implicit_air(true, false, 0.0, 0.25), 0.75);
        assert_eq!(implicit_air(true, false, 0.0, 1.0), 0.0);
        assert_eq!(implicit_air(false, false, 0.0, 0.0), 0.0);
        assert_eq!(implicit_air(true, true, 0.0, 0.0), 0.0);
    }

    #[test]
    fn apply_shader_is_commit_only() {
        let shader = include_str!("material_reactions.wgsl");
        let apply = shader
            .split_once("fn apply_canonical")
            .and_then(|(_, rest)| rest.split_once("fn has_environment"))
            .map(|(body, _)| body)
            .expect("apply shader entry point must exist");
        for forbidden in [
            "reserve_fluid_slot",
            "reserve_fluid_plan",
            "reserve_gas(",
            "reserve_gas_output",
            "reserve_mutation_requests",
            "source_for(",
            "find_partner(",
            "atomicAdd",
        ] {
            assert!(!apply.contains(forbidden), "apply contains {forbidden}");
        }
    }

    #[test]
    fn canonical_discovery_and_apply_pipelines_compile() {
        let accelerator = Accelerator::new().unwrap();
        let registry = MaterialRegistry::new();
        let table = ReactionMaterialTable::new(&accelerator, &registry);
        let ids = accelerator.allocate::<u32>(64);
        let amounts = accelerator.allocate::<f32>(64);
        let temperatures = accelerator.allocate::<f32>(64);
        let gas_temperatures = accelerator.allocate::<f32>(64);
        let rigid_temperatures = accelerator.allocate::<f32>(64);
        let pressure = accelerator.allocate::<[f32; 4]>(64);
        let coverage = accelerator.allocate::<f32>(64);
        let gas = accelerator.allocate::<f32>(1);
        let occupancy = accelerator.allocate::<u32>(64);
        let claims = accelerator.allocate::<u32>(64);
        let rigid_cells = accelerator.allocate::<[u32; 8]>(64);
        let rigid_amounts = accelerator.allocate::<f32>(64);
        let requests = accelerator.allocate::<[u32; 9]>(128);
        let request_count = accelerator.allocate::<u32>(1);
        accelerator
            .wgpu_queue()
            .write_buffer(request_count.wgpu_buffer(), 0, &0u32.to_le_bytes());
        let reaction_energy = accelerator.allocate::<f32>(64);
        let pending_pressure = accelerator.allocate::<[f32; 4]>(64);
        let fluid_particles = accelerator.allocate::<[u32; 10]>(8);
        let fluid_bucket_heads = accelerator.allocate::<u32>(8);
        let fluid_next_particle = accelerator.allocate::<u32>(8);
        let fluid_parameters = accelerator
            .wgpu_device()
            .create_buffer(&wgpu::BufferDescriptor {
                label: Some("test fluid spatial parameters"),
                size: 128,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        let _reactions = MaterialReactions::new(
            &accelerator,
            &table,
            &ids,
            &amounts,
            &temperatures,
            &gas_temperatures,
            &rigid_temperatures,
            &pressure,
            &coverage,
            &gas,
            &occupancy,
            &claims,
            &rigid_cells,
            &rigid_amounts,
            reaction_energy,
            &pending_pressure,
            &requests,
            &request_count,
            FluidAuthorityView {
                particles: &fluid_particles,
                particle_capacity: 8,
                free_indices: &fluid_next_particle,
                free_count: &fluid_bucket_heads,
                bucket_heads: &fluid_bucket_heads,
                next_particle: &fluid_next_particle,
                parameters: &fluid_parameters,
            },
            64,
            0,
            0,
        );
    }
}
